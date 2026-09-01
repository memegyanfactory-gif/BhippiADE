import {
  PlayRuntime,
  runScriptedPlaytestWithRuntime,
  type InputDocument,
  type RuntimeCapability,
  type RuntimeDocument,
  type RuntimeFrame,
  type ScriptedPlaytestReport,
  type ScriptedPlaytestStep,
  type Vec3,
} from "./playRuntime.ts";
import type { ScriptProgram } from "./scriptVm.ts";

export const RUNTIME_PROTOCOL_FORMAT = "bhippi-runtime-protocol@1";

export type RuntimeWorkerBudgets = {
  instructionsPerTick: number;
  instructionsTotal: number;
  callDepth: number;
  messageBytes: number;
  messagesPerTick: number;
  spawnedEntities: number;
  emittedEvents: number;
  logBytes: number;
};

export const DEFAULT_RUNTIME_WORKER_BUDGETS: RuntimeWorkerBudgets = {
  instructionsPerTick: 200_000,
  instructionsTotal: 20_000_000,
  callDepth: 64,
  messageBytes: 1_048_576,
  messagesPerTick: 4_096,
  spawnedEntities: 4_096,
  emittedEvents: 16_384,
  logBytes: 1_048_576,
};

export type RuntimeWorkerStart = {
  kind: "start";
  document: RuntimeDocument;
  gravity: Vec3;
  input: InputDocument;
  programs: Array<{ entity: string; program: ScriptProgram }>;
  capabilities: RuntimeCapability[];
  seed: number;
  pauseOnError: boolean;
  budgets: RuntimeWorkerBudgets;
};

export type RuntimeWorkerRequest =
  | RuntimeWorkerStart
  | { kind: "input"; code: string; pressed: boolean }
  | { kind: "pause"; paused: boolean }
  | { kind: "set_variable"; path: string; value: string | number | boolean }
  | { kind: "reset" }
  | { kind: "tick"; deltaSeconds: number; timeScale: number; force: boolean }
  | { kind: "scripted_playtest"; steps: ScriptedPlaytestStep[]; fixedDeltaSeconds: number }
  | { kind: "stop" };

export type RuntimeWorkerEnvelope<T> = {
  format: typeof RUNTIME_PROTOCOL_FORMAT;
  sessionNonce: string;
  sequence: number;
  payload: T;
};

export type SerializableRuntimeFrame = Omit<RuntimeFrame, "transforms" | "rotations"> & {
  transforms: Array<[string, Vec3]>;
  rotations: Array<[string, Vec3]>;
};

export type RuntimeWorkerFaultCode =
  | "invalid_format"
  | "invalid_nonce"
  | "out_of_order"
  | "payload_too_large"
  | "invalid_start"
  | "undeclared_capability"
  | "budget_exhausted"
  | "runtime_fault";

export type RuntimeWorkerResponse =
  | { kind: "started" }
  | { kind: "frame"; frame: SerializableRuntimeFrame }
  | {
      kind: "playtest_report";
      report: ScriptedPlaytestReport & { sandbox: RuntimeWorkerSandboxEvidence };
    }
  | { kind: "ack" }
  | { kind: "stopped"; reason: "requested" | "fault" }
  | { kind: "fault"; code: RuntimeWorkerFaultCode; message: string };

export type RuntimeWorkerSandboxEvidence = {
  protocol: typeof RUNTIME_PROTOCOL_FORMAT;
  execution: "application_module_worker";
  capabilities: RuntimeCapability[];
  budgets: RuntimeWorkerBudgets;
  terminationReason: "completed" | "runtime_fault";
};

const CAPABILITIES = new Set<RuntimeCapability>([
  "entity_read",
  "entity_write_runtime",
  "input_read",
  "hud_action",
  "level_travel",
  "audio_event",
  "deterministic_timer",
]);
const utf8 = new TextEncoder();

const HOST_CAPABILITY: Readonly<Record<string, RuntimeCapability>> = {
  self_id: "entity_read", get_var: "entity_read", pos_x: "entity_read", pos_y: "entity_read",
  pos_z: "entity_read", rot_y: "entity_read", vel_x: "entity_read", vel_y: "entity_read",
  vel_z: "entity_read", grounded: "entity_read", find: "entity_read", find_tag: "entity_read",
  name_of: "entity_read", has_tag: "entity_read", distance: "entity_read", exists: "entity_read",
  set_var: "entity_write_runtime", set_pos: "entity_write_runtime", translate: "entity_write_runtime",
  set_rot: "entity_write_runtime", set_vel: "entity_write_runtime", spawn: "entity_write_runtime",
  destroy: "entity_write_runtime", is_action: "input_read", action_pressed: "input_read",
  axis: "input_read", hud_set: "hud_action", hud_show: "hud_action", load_level: "level_travel",
  play_sound: "audio_event", time: "deterministic_timer", random: "deterministic_timer",
};

export class RuntimeWorkerSession {
  private readonly nonce: string;
  private nextSequence = 0;
  private runtime: PlayRuntime | null = null;
  private authoredDocument: RuntimeDocument | null = null;
  private capabilities: RuntimeCapability[] = [];
  private budgets: RuntimeWorkerBudgets = DEFAULT_RUNTIME_WORKER_BUDGETS;
  private messagesThisTick = 0;
  private spawned = 0;
  private events = 0;
  private logBytes = 0;
  private terminated = false;

  constructor(nonce: string) {
    if (nonce.trim() === "") throw new Error("runtime session nonce cannot be empty");
    this.nonce = nonce;
  }

  handle(envelope: RuntimeWorkerEnvelope<RuntimeWorkerRequest>): RuntimeWorkerEnvelope<RuntimeWorkerResponse> {
    const sequence = envelope.sequence;
    const respond = (payload: RuntimeWorkerResponse): RuntimeWorkerEnvelope<RuntimeWorkerResponse> => ({
      format: RUNTIME_PROTOCOL_FORMAT,
      sessionNonce: this.nonce,
      sequence,
      payload,
    });
    if (this.terminated) return respond({ kind: "fault", code: "runtime_fault", message: "session terminated" });
    if (envelope.format !== RUNTIME_PROTOCOL_FORMAT) return this.fail(respond, "invalid_format", "unknown runtime protocol");
    if (envelope.sessionNonce !== this.nonce) return this.fail(respond, "invalid_nonce", "session nonce mismatch");
    if (sequence !== this.nextSequence) return this.fail(respond, "out_of_order", `expected sequence ${this.nextSequence}`);
    this.nextSequence += 1;
    this.messagesThisTick += 1;
    if (utf8.encode(JSON.stringify(envelope)).byteLength > this.budgets.messageBytes) {
      return this.fail(respond, "payload_too_large", "runtime message exceeded its byte budget");
    }
    if (this.messagesThisTick > this.budgets.messagesPerTick) {
      return this.fail(respond, "budget_exhausted", "message rate budget exhausted");
    }

    try {
      const request = envelope.payload;
      if (request.kind === "start") return respond(this.start(request));
      if (!this.runtime) return this.fail(respond, "invalid_start", "runtime was not started");
      if (request.kind === "input") this.runtime.input.set(request.code, request.pressed);
      if (request.kind === "pause") this.runtime.setPaused(request.paused);
      if (request.kind === "set_variable") this.runtime.setVariable(request.path, request.value);
      if (request.kind === "reset") this.runtime.reset();
      if (request.kind === "stop") {
        this.terminated = true;
        this.runtime = null;
        return respond({ kind: "stopped", reason: "requested" });
      }
      if (request.kind === "scripted_playtest") {
        const authored = this.authoredDocument;
        if (!authored) return this.fail(respond, "invalid_start", "authored runtime snapshot is unavailable");
        const report = runScriptedPlaytestWithRuntime(
          authored,
          this.runtime,
          request.steps,
          request.fixedDeltaSeconds,
          (frame) => this.consume(frame),
        );
        const sandbox: RuntimeWorkerSandboxEvidence = {
          protocol: RUNTIME_PROTOCOL_FORMAT,
          execution: "application_module_worker",
          capabilities: [...this.capabilities],
          budgets: { ...this.budgets },
          terminationReason: report.completed ? "completed" : "runtime_fault",
        };
        this.terminated = true;
        this.runtime = null;
        this.authoredDocument = null;
        return respond({ kind: "playtest_report", report: { ...report, sandbox } });
      }
      if (request.kind !== "tick") return respond({ kind: "ack" });
      const frame = this.runtime.update(request.deltaSeconds, request.timeScale, request.force);
      this.consume(frame);
      this.messagesThisTick = 0;
      return respond({ kind: "frame", frame: serialiseRuntimeFrame(frame) });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      return this.fail(
        respond,
        message.includes("budget exhausted") ? "budget_exhausted" : "runtime_fault",
        message,
      );
    }
  }

  private start(request: RuntimeWorkerStart): RuntimeWorkerResponse {
    if (this.runtime) {
      this.terminated = true;
      this.runtime = null;
      return { kind: "fault", code: "invalid_start", message: "runtime already started" };
    }
    const capabilities = new Set(request.capabilities);
    if (capabilities.size !== request.capabilities.length || [...capabilities].some((item) => !CAPABILITIES.has(item))) {
      this.terminated = true;
      return { kind: "fault", code: "invalid_start", message: "capability list is invalid" };
    }
    if (!validBudgets(request.budgets)) {
      this.terminated = true;
      return { kind: "fault", code: "invalid_start", message: "runtime budgets are invalid" };
    }
    for (const { entity, program } of request.programs) {
      if (program.file.includes("/") || program.file.includes("\\") || program.file.includes(":")) {
        this.terminated = true;
        return { kind: "fault", code: "invalid_start", message: `worker program ${entity} leaked an authored path` };
      }
      const missing = program.hosts.find((host) => {
        const required = HOST_CAPABILITY[host];
        return required !== undefined && !capabilities.has(required);
      });
      if (missing) {
        this.terminated = true;
        return { kind: "fault", code: "undeclared_capability", message: `host ${missing} was not declared` };
      }
      if (
        program.step_budget > request.budgets.instructionsPerTick ||
        program.call_depth > request.budgets.callDepth
      ) {
        this.terminated = true;
        return {
          kind: "fault",
          code: "budget_exhausted",
          message: `worker program ${entity} exceeds the instruction or call-depth ceiling`,
        };
      }
    }
    this.budgets = request.budgets;
    this.messagesThisTick = 0;
    this.authoredDocument = request.document;
    this.capabilities = [...request.capabilities];
    this.runtime = new PlayRuntime(request.document, request.gravity, request.input, {
      scripts: new Map(request.programs.map((item) => [item.entity, item.program])),
      seed: request.seed,
      pauseOnError: request.pauseOnError,
      capabilities,
    });
    return { kind: "started" };
  }

  private consume(frame: RuntimeFrame): void {
    this.spawned += frame.spawned.length;
    this.events += frame.events.length;
    this.logBytes += frame.events.reduce(
      (total, event) => total + (event.kind === "log" ? utf8.encode(event.message).byteLength : 0),
      0,
    );
    if (
      frame.stats.scriptInstructionsThisFrame > this.budgets.instructionsPerTick ||
      frame.stats.scriptInstructions > this.budgets.instructionsTotal ||
      this.spawned > this.budgets.spawnedEntities ||
      this.events > this.budgets.emittedEvents ||
      this.logBytes > this.budgets.logBytes
    ) {
      throw new Error("runtime resource budget exhausted");
    }
  }

  private fail(
    respond: (payload: RuntimeWorkerResponse) => RuntimeWorkerEnvelope<RuntimeWorkerResponse>,
    code: RuntimeWorkerFaultCode,
    message: string,
  ): RuntimeWorkerEnvelope<RuntimeWorkerResponse> {
    this.terminated = true;
    this.runtime = null;
    return respond({ kind: "fault", code, message });
  }
}

export function serialiseRuntimeFrame(frame: RuntimeFrame): SerializableRuntimeFrame {
  return { ...frame, transforms: [...frame.transforms], rotations: [...frame.rotations] };
}

export function deserialiseRuntimeFrame(frame: SerializableRuntimeFrame): RuntimeFrame {
  return { ...frame, transforms: new Map(frame.transforms), rotations: new Map(frame.rotations) };
}

function validBudgets(value: RuntimeWorkerBudgets): boolean {
  const maximum = DEFAULT_RUNTIME_WORKER_BUDGETS;
  return (Object.keys(maximum) as Array<keyof RuntimeWorkerBudgets>).every(
    (key) => Number.isSafeInteger(value[key]) && value[key] > 0 && value[key] <= maximum[key],
  );
}
