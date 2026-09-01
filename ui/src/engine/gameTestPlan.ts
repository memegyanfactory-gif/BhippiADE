import {
  type PlayRuntime,
  type RuntimeDocument,
  type RuntimeEvent,
  type RuntimeFrame,
  type RuntimeStats,
  type Vec3,
  stableTextHash,
} from "./playRuntime.ts";

export const GAME_TEST_PLAN_FORMAT = "bhippi-game-test-plan@1" as const;
export const GAME_TEST_BATCH_FORMAT = "bhippi-game-test-batch@1" as const;

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export type TestComparison = "equal" | "not_equal" | "greater_or_equal" | "less_or_equal";

export type GameTestInput =
  | { kind: "press"; action: string }
  | { kind: "release"; action: string }
  | { kind: "axis"; axis: string; value: number };

export type GameTestInputStep = GameTestInput & { at_ms: number };

export type GameTestAssertion =
  | { kind: "variable"; path: string; comparison: TestComparison; expected: JsonValue }
  | { kind: "event"; name: string; min_count: number }
  | {
      kind: "transform";
      entity: string;
      translation?: Vec3 | null;
      rotation_degrees?: Vec3 | null;
      scale?: Vec3 | null;
      tolerance: number;
    }
  | { kind: "hud"; widget: string; property: string; comparison: TestComparison; expected: JsonValue }
  | { kind: "level_travel"; level: string };

export type GameTestCheckpoint = {
  name: string;
  at_ms: number;
  assertions: GameTestAssertion[];
};

export type GameTestScenario = {
  name: string;
  initial_level: string;
  seed: number;
  input: GameTestInputStep[];
  checkpoints: GameTestCheckpoint[];
};

export type GameTestPlan = {
  format: typeof GAME_TEST_PLAN_FORMAT;
  scenarios: GameTestScenario[];
};

/** Only authored HUD state required to evaluate a checkpoint crosses into the worker. */
export type RuntimeHudDocument = {
  widgets: Array<{
    id: string;
    name: string;
    visible: boolean;
    props: Record<string, JsonValue>;
    style: Record<string, JsonValue>;
    bind: Record<string, string>;
  }>;
};

export type GameTestAssertionEvidence = {
  checkpoint: string;
  assertion_index: number;
  passed: boolean;
  address: string;
  observed: JsonValue;
  expected: GameTestAssertion;
};

export type GameTestCheckpointSample = {
  checkpoint: string;
  atMs: number;
  transforms: Record<string, Vec3>;
  rotations: Record<string, Vec3>;
  variables: Readonly<Record<string, string | number | boolean>>;
  hud: Readonly<Record<string, string>>;
  hudVisible: Readonly<Record<string, boolean>>;
  events: RuntimeEvent[];
  currentLevel: string;
  checkpointHash: string;
};

export type GameTestScenarioReport = {
  authoredUnchanged: boolean;
  authoredHashBefore: string;
  authoredHashAfter: string;
  completed: boolean;
  frames: number;
  samples: GameTestCheckpointSample[];
  assertions: GameTestAssertionEvidence[];
  stats: RuntimeStats | null;
  faults: RuntimeEvent[];
};

type RuntimeEntityObservation = {
  id: string;
  name: string;
  scale: Vec3;
};

/**
 * Execute one already Rust-validated scenario inside its disposable worker runtime.
 * Millisecond boundaries snap forward to the first fixed simulation frame at or after them.
 * Inputs at a boundary are applied before that frame and checkpoints observe after it.
 */
export function runGameTestScenarioWithRuntime(
  document: RuntimeDocument,
  runtime: PlayRuntime,
  scenario: GameTestScenario,
  hud: RuntimeHudDocument | null,
  levels: string[],
  fixedDeltaSeconds: number,
  onFrame: (frame: RuntimeFrame) => void = () => undefined,
): GameTestScenarioReport {
  if (!Number.isFinite(fixedDeltaSeconds) || fixedDeltaSeconds <= 0) {
    throw new Error("game test fixed delta must be finite and positive");
  }
  const authored = JSON.stringify(document);
  const frameMillis = fixedDeltaSeconds * 1_000;
  const inputs = scenario.input.map((step, order) => ({
    step,
    order,
    frame: frameFor(step.at_ms, frameMillis),
  }));
  const checkpoints = scenario.checkpoints.map((checkpoint, order) => ({
    checkpoint,
    order,
    frame: frameFor(checkpoint.at_ms, frameMillis),
  }));
  const samples: GameTestCheckpointSample[] = [];
  const assertions: GameTestAssertionEvidence[] = [];
  const faults: RuntimeEvent[] = [];
  const events: RuntimeEvent[] = [];
  const entities = runtimeEntityObservations(document);
  let inputIndex = 0;
  let checkpointIndex = 0;
  let last: RuntimeFrame | null = null;
  let frameCount = 0;
  let completed = true;
  let currentLevel = scenario.initial_level;
  let failure = "";

  const applyInputs = (frame: number): void => {
    while (inputIndex < inputs.length && inputs[inputIndex].frame === frame) {
      const input = inputs[inputIndex].step;
      if (input.kind === "press") runtime.input.setAction(input.action, true);
      else if (input.kind === "release") runtime.input.setAction(input.action, false);
      else runtime.input.setAxis(input.axis, input.value);
      inputIndex += 1;
    }
  };

  const updateLevel = (frame: RuntimeFrame): void => {
    for (const event of frame.events) {
      if (event.kind !== "level") continue;
      const resolved = resolveLevel(event.name, levels);
      if (resolved !== null) currentLevel = resolved;
    }
  };

  const recordCheckpoint = (checkpoint: GameTestCheckpoint, frame: RuntimeFrame): void => {
    const base = {
      checkpoint: checkpoint.name,
      atMs: checkpoint.at_ms,
      transforms: Object.fromEntries(frame.transforms),
      rotations: Object.fromEntries(frame.rotations),
      variables: frame.variables,
      hud: frame.hud,
      hudVisible: frame.hudVisible,
      events: [...events],
      currentLevel,
    };
    samples.push({ ...base, checkpointHash: stableTextHash(JSON.stringify(base)) });
    checkpoint.assertions.forEach((assertion, assertionIndex) => {
      assertions.push(
        evaluateAssertion(
          scenario.name,
          checkpoint,
          assertionIndex,
          assertion,
          frame,
          events,
          entities,
          hud,
          currentLevel,
        ),
      );
    });
  };

  const maximumFrame = checkpoints.at(-1)?.frame ?? 0;
  try {
    for (let frameIndex = 0; frameIndex <= maximumFrame; frameIndex += 1) {
      applyInputs(frameIndex);
      last = runtime.update(frameIndex === 0 ? 0 : fixedDeltaSeconds);
      onFrame(last);
      updateEntityObservations(entities, last);
      updateLevel(last);
      events.push(...last.events);
      faults.push(...last.events.filter(isFault));
      if (frameIndex > 0) frameCount += 1;
      while (checkpointIndex < checkpoints.length && checkpoints[checkpointIndex].frame === frameIndex) {
        recordCheckpoint(checkpoints[checkpointIndex].checkpoint, last);
        checkpointIndex += 1;
      }
    }
  } catch (error) {
    completed = false;
    failure = error instanceof Error ? error.message : String(error);
    faults.push({
      kind: "fault",
      message: failure,
      hint: "Fix the runtime fault, then repeat this exact game-test scenario.",
    });
  } finally {
    runtime.input.clear();
  }

  while (checkpointIndex < checkpoints.length) {
    const checkpoint = checkpoints[checkpointIndex].checkpoint;
    checkpoint.assertions.forEach((assertion, assertionIndex) => {
      assertions.push({
        checkpoint: checkpoint.name,
        assertion_index: assertionIndex,
        passed: false,
        address: assertionAddress(scenario.name, checkpoint.name, assertionIndex, assertion),
        observed: { status: "not_reached", reason: failure || "scenario ended before checkpoint" },
        expected: canonicalExpected(assertion),
      });
    });
    checkpointIndex += 1;
  }

  const authoredAfter = JSON.stringify(document);
  return {
    authoredUnchanged: authoredAfter === authored,
    authoredHashBefore: stableTextHash(authored),
    authoredHashAfter: stableTextHash(authoredAfter),
    completed,
    frames: frameCount,
    samples,
    assertions,
    stats: last?.stats ?? null,
    faults,
  };
}

export function parseRuntimeHudDocument(text: string | null): RuntimeHudDocument | null {
  if (text === null) return null;
  try {
    const parsed = JSON.parse(text) as { widgets?: unknown };
    if (!Array.isArray(parsed.widgets)) return null;
    const widgets: RuntimeHudDocument["widgets"] = [];
    for (const raw of parsed.widgets) {
      if (typeof raw !== "object" || raw === null) return null;
      const widget = raw as Record<string, unknown>;
      if (typeof widget.id !== "string" || typeof widget.name !== "string") return null;
      widgets.push({
        id: widget.id,
        name: widget.name,
        visible: widget.visible !== false,
        props: jsonRecord(widget.props),
        style: jsonRecord(widget.style),
        bind: stringRecord(widget.bind),
      });
    }
    return { widgets };
  } catch {
    return null;
  }
}

function evaluateAssertion(
  scenario: string,
  checkpoint: GameTestCheckpoint,
  assertionIndex: number,
  assertion: GameTestAssertion,
  frame: RuntimeFrame,
  events: RuntimeEvent[],
  entities: RuntimeEntityObservation[],
  hud: RuntimeHudDocument | null,
  currentLevel: string,
): GameTestAssertionEvidence {
  const base = {
    checkpoint: checkpoint.name,
    assertion_index: assertionIndex,
    address: assertionAddress(scenario, checkpoint.name, assertionIndex, assertion),
    expected: canonicalExpected(assertion),
  };
  if (assertion.kind === "variable") {
    const found = Object.prototype.hasOwnProperty.call(frame.variables, assertion.path);
    const observed: JsonValue = found
      ? (frame.variables[assertion.path] as string | number | boolean)
      : { status: "missing_variable", path: assertion.path };
    return { ...base, passed: found && compare(observed, assertion.expected, assertion.comparison), observed };
  }
  if (assertion.kind === "event") {
    const matching = events.filter((event) => eventMatches(event, assertion.name));
    return {
      ...base,
      passed: matching.length >= assertion.min_count,
      observed: { name: assertion.name, count: matching.length },
    };
  }
  if (assertion.kind === "transform") {
    const matches = entities.filter((entity) => entity.id === assertion.entity || entity.name === assertion.entity);
    if (matches.length !== 1) {
      return {
        ...base,
        passed: false,
        observed: { status: matches.length === 0 ? "missing_entity" : "ambiguous_entity", entity: assertion.entity },
      };
    }
    const entity = matches[0];
    const translation = frame.transforms.get(entity.id);
    const rotation = frame.rotations.get(entity.id);
    const observed: JsonValue = {
      entity: entity.id,
      translation: translation ?? null,
      rotation_degrees: rotation?.map(toDegrees) as Vec3 | undefined ?? null,
      scale: entity.scale,
    };
    const passed =
      translation !== undefined &&
      rotation !== undefined &&
      (assertion.translation == null || vectorClose(translation, assertion.translation, assertion.tolerance)) &&
      (assertion.rotation_degrees == null ||
        vectorClose(rotation.map(toDegrees) as Vec3, assertion.rotation_degrees, assertion.tolerance)) &&
      (assertion.scale == null || vectorClose(entity.scale, assertion.scale, assertion.tolerance));
    return { ...base, passed, observed };
  }
  if (assertion.kind === "hud") {
    const widgets = hud?.widgets.filter(
      (widget) => widget.id === assertion.widget || widget.name === assertion.widget,
    ) ?? [];
    if (widgets.length !== 1) {
      return {
        ...base,
        passed: false,
        observed: { status: widgets.length === 0 ? "missing_widget" : "ambiguous_widget", widget: assertion.widget },
      };
    }
    const observed = hudProperty(widgets[0], assertion.property, frame);
    return {
      ...base,
      passed: observed.found && compare(observed.value, assertion.expected, assertion.comparison),
      observed: observed.value,
    };
  }
  return {
    ...base,
    passed: currentLevel === assertion.level,
    observed: { current_level: currentLevel },
  };
}

function hudProperty(
  widget: RuntimeHudDocument["widgets"][number],
  property: string,
  frame: RuntimeFrame,
): { found: boolean; value: JsonValue } {
  if (property === "visible") {
    return { found: true, value: frame.hudVisible[widget.id] ?? frame.hudVisible[widget.name] ?? widget.visible };
  }
  if (property === "text" || property === "value") {
    const override = frame.hud[widget.id] ?? frame.hud[widget.name];
    if (override !== undefined) return { found: true, value: override };
  }
  const binding = widget.bind[property];
  if (binding !== undefined && Object.prototype.hasOwnProperty.call(frame.variables, binding)) {
    return { found: true, value: frame.variables[binding] as string | number | boolean };
  }
  if (Object.prototype.hasOwnProperty.call(widget.props, property)) {
    return { found: true, value: widget.props[property] };
  }
  if (property.startsWith("style.") && Object.prototype.hasOwnProperty.call(widget.style, property.slice(6))) {
    return { found: true, value: widget.style[property.slice(6)] };
  }
  return { found: false, value: { status: "missing_hud_property", property } };
}

function compare(observed: JsonValue, expected: JsonValue, comparison: TestComparison): boolean {
  if (comparison === "equal") return deepEqual(observed, expected);
  if (comparison === "not_equal") return !deepEqual(observed, expected);
  if (typeof observed !== "number" || typeof expected !== "number") return false;
  if (!Number.isFinite(observed) || !Number.isFinite(expected)) return false;
  return comparison === "greater_or_equal" ? observed >= expected : observed <= expected;
}

function deepEqual(left: JsonValue, right: JsonValue): boolean {
  if (Object.is(left, right)) return true;
  if (Array.isArray(left) || Array.isArray(right)) {
    return Array.isArray(left) && Array.isArray(right) &&
      left.length === right.length && left.every((value, index) => deepEqual(value, right[index]));
  }
  if (typeof left !== "object" || left === null || typeof right !== "object" || right === null) return false;
  const leftKeys = Object.keys(left).sort();
  const rightKeys = Object.keys(right).sort();
  return leftKeys.length === rightKeys.length && leftKeys.every(
    (key, index) => key === rightKeys[index] && deepEqual(left[key], right[key]),
  );
}

function eventMatches(event: RuntimeEvent, name: string): boolean {
  if (event.kind === name) return true;
  if (event.kind === "trigger" && event.action === name) return true;
  if (event.kind === "level" && event.name === name) return true;
  return false;
}

function assertionAddress(
  scenario: string,
  checkpoint: string,
  index: number,
  assertion: GameTestAssertion,
): string {
  const subject = assertion.kind === "variable" ? assertion.path
    : assertion.kind === "event" ? assertion.name
      : assertion.kind === "transform" ? assertion.entity
        : assertion.kind === "hud" ? `${assertion.widget}/${assertion.property}`
          : assertion.level;
  return `runtime://scenario/${encodeURIComponent(scenario)}/checkpoint/${encodeURIComponent(checkpoint)}/assertion/${index}/${assertion.kind}/${encodeURIComponent(subject)}`;
}

/** Match serde's canonical enum value, including explicit null Option fields. */
function canonicalExpected(assertion: GameTestAssertion): GameTestAssertion {
  if (assertion.kind !== "transform") return assertion;
  return {
    kind: "transform",
    entity: assertion.entity,
    translation: assertion.translation ?? null,
    rotation_degrees: assertion.rotation_degrees ?? null,
    scale: assertion.scale ?? null,
    tolerance: assertion.tolerance,
  };
}

function runtimeEntityObservations(document: RuntimeDocument): RuntimeEntityObservation[] {
  return document.entities.map((entity) => ({
    id: entity.id,
    name: entity.name,
    scale: vector(entity.components.Transform?.scale, [1, 1, 1]),
  }));
}

function updateEntityObservations(entities: RuntimeEntityObservation[], frame: RuntimeFrame): void {
  for (const spawned of frame.spawned) {
    if (entities.some((entity) => entity.id === spawned.id)) continue;
    entities.push({
      id: spawned.id,
      name: spawned.name,
      scale: vector(spawned.components.Transform?.scale, [1, 1, 1]),
    });
  }
}

function frameFor(atMillis: number, frameMillis: number): number {
  return Math.max(0, Math.ceil(atMillis / frameMillis - 1e-9));
}

function resolveLevel(requested: string, levels: string[]): string | null {
  return levels.find(
    (path) => path === requested || path.endsWith(`/${requested}`) || path.endsWith(`/${requested}.bscn.json`),
  ) ?? null;
}

function vector(value: unknown, fallback: Vec3): Vec3 {
  return Array.isArray(value) && value.length === 3 && value.every((part) => Number.isFinite(part))
    ? [Number(value[0]), Number(value[1]), Number(value[2])]
    : [...fallback];
}

function vectorClose(left: Vec3, right: Vec3, tolerance: number): boolean {
  return left.every((value, index) => Math.abs(value - right[index]) <= tolerance);
}

function toDegrees(value: number): number {
  return value * (180 / Math.PI);
}

function isFault(event: RuntimeEvent): boolean {
  return event.kind === "fault" || event.kind === "script_fault";
}

function jsonRecord(value: unknown): Record<string, JsonValue> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return {};
  return value as Record<string, JsonValue>;
}

function stringRecord(value: unknown): Record<string, string> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return {};
  return Object.fromEntries(
    Object.entries(value).filter((entry): entry is [string, string] => typeof entry[1] === "string"),
  );
}
