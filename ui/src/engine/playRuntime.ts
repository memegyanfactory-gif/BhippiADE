/**
 * Deterministic in-pane play runtime (ADR-0028, ENG-171…180).
 *
 * It owns a cloned runtime world. Authored scene objects are read once and never mutated;
 * Stop therefore discards this instance and the editor renders the byte-identical source
 * (INV-081). Rendering stays in EngineViewport; simulation is isolated and unit-testable
 * here, and gameplay scripts run in `scriptVm.ts` against programs Rust compiled (ADR-0030).
 *
 * The solver is a **kinematic, oriented-box** solver, not a full rigid-body engine: bodies
 * have position and linear velocity, no angular momentum and no restitution. Colliders are
 * cuboids, spheres and capsules with real contact normals, so ramps are walkable and slope
 * limits mean something. `mesh` colliders resolve against the mesh's box, and heightfields
 * against their sampled grid — both say so rather than pretending to be exact.
 */

import {
  makeRandom,
  renderValue,
  ScriptVm,
  type ScriptFault,
  type ScriptHostTable,
  type ScriptProgram,
  type ScriptValue,
} from "./scriptVm.ts";

export type Vec3 = [number, number, number];

export type RuntimeEntity = {
  id: string;
  name: string;
  tags: string[];
  components: Record<string, any>;
};

export type RuntimeDocument = {
  entities: RuntimeEntity[];
};

export type InputDocument = {
  format: string;
  actions?: { name: string; keys: string[] }[];
  axes?: { name: string; positive: string[]; negative: string[] }[];
};

export type RuntimeStats = {
  fps: number;
  frameMs: number;
  entities: number;
  simulatedBodies: number;
  contacts: number;
  scripts: number;
  scriptFaults: number;
  elapsed: number;
  paused: boolean;
};

export type RuntimeEvent =
  | { kind: "collision"; entity: string; other: string }
  | { kind: "trigger"; entity: string; other: string; action?: string }
  | { kind: "log"; entity: string; message: string }
  | { kind: "sound"; asset: string }
  | { kind: "level"; name: string }
  | { kind: "spawn"; entity: string }
  | { kind: "destroy"; entity: string }
  | { kind: "fault"; message: string; hint: string }
  | ({ kind: "script_fault"; entity: string; hook: string } & ScriptFault);

export type RuntimeFrame = {
  transforms: Map<string, Vec3>;
  rotations: Map<string, Vec3>;
  /** Entities `spawn()` created this frame, for the viewport to add to the scene. */
  spawned: RuntimeEntity[];
  /** Entities `destroy()` removed this frame. */
  removed: string[];
  variables: Readonly<Record<string, string | number | boolean>>;
  /** `hud_set` overrides, keyed by widget id; they win over bindings for that frame. */
  hud: Readonly<Record<string, string>>;
  /** `hud_show` overrides, keyed by widget id. */
  hudVisible: Readonly<Record<string, boolean>>;
  stats: RuntimeStats;
  events: RuntimeEvent[];
};

/** A collider, resolved from `Collider.shape` or inferred from the transform's scale. */
export type Shape =
  | { kind: "cuboid"; half: Vec3 }
  | { kind: "sphere"; radius: number }
  | { kind: "capsule"; radius: number; half: number }
  | { kind: "heightfield"; half: Vec3; rows: number; cols: number; heights: number[] };

type Body = {
  id: string;
  position: Vec3;
  rotation: Vec3;
  authoredPosition: Vec3;
  authoredRotation: Vec3;
  shape: Shape;
  velocity: Vec3;
  kind: "static" | "dynamic" | "kinematic";
  sensor: boolean;
  grounded: boolean;
  groundNormalY: number;
  controller: null | {
    height: number;
    radius: number;
    stepHeight: number;
    moveSpeed: number;
    jumpSpeed: number;
    /** cos of the steepest walkable slope; a contact flatter than this carries the body. */
    slopeCos: number;
  };
};

type Contact = { normal: Vec3; depth: number };

const EPSILON = 1e-6;

const copyVec = (value: unknown, fallback: Vec3): Vec3 =>
  Array.isArray(value) && value.length === 3 && value.every((part) => Number.isFinite(part))
    ? [Number(value[0]), Number(value[1]), Number(value[2])]
    : [...fallback];

const numeric = (value: unknown, fallback: number): number =>
  typeof value === "number" && Number.isFinite(value) ? value : fallback;

const length = (v: Vec3): number => Math.sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);

const normalise = (v: Vec3): Vec3 => {
  const len = length(v);
  return len < EPSILON ? [0, 1, 0] : [v[0] / len, v[1] / len, v[2] / len];
};

const dot = (a: Vec3, b: Vec3): number => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

/** The three local axes of a body rotated by euler XYZ, as world-space unit vectors. */
const axesOf = (rotation: Vec3): [Vec3, Vec3, Vec3] => {
  const [rx, ry, rz] = rotation;
  const [cx, sx] = [Math.cos(rx), Math.sin(rx)];
  const [cy, sy] = [Math.cos(ry), Math.sin(ry)];
  const [cz, sz] = [Math.cos(rz), Math.sin(rz)];
  // R = Rx * Ry * Rz, matching Three.js's default euler order; the columns are the axes.
  return [
    [cy * cz, cx * sz + sx * sy * cz, sx * sz - cx * sy * cz],
    [-cy * sz, cx * cz - sx * sy * sz, sx * cz + cx * sy * sz],
    [sy, -sx * cy, cx * cy],
  ];
};

const isIdentityRotation = (rotation: Vec3): boolean =>
  Math.abs(rotation[0]) < EPSILON && Math.abs(rotation[1]) < EPSILON && Math.abs(rotation[2]) < EPSILON;

/**
 * Read `Collider.shape` — the schema calls it free-form JSON, so every documented spelling
 * is accepted and anything unreadable falls back to the transform's scale rather than
 * silently dropping the collider.
 */
export const shapeOf = (collider: unknown, scale: Vec3): Shape => {
  const half: Vec3 = [
    Math.max(Math.abs(scale[0]) / 2, EPSILON),
    Math.max(Math.abs(scale[1]) / 2, EPSILON),
    Math.max(Math.abs(scale[2]) / 2, EPSILON),
  ];
  const raw = (collider as { shape?: unknown } | undefined)?.shape;
  if (!raw) return { kind: "cuboid", half };

  const readTriple = (value: unknown): Vec3 | null =>
    Array.isArray(value) && value.length >= 3 && value.slice(0, 3).every((part) => Number.isFinite(part))
      ? [Number(value[0]) / 2, Number(value[1]) / 2, Number(value[2]) / 2]
      : null;

  if (typeof raw === "string") {
    if (raw === "sphere") return { kind: "sphere", radius: Math.max(...half) };
    if (raw === "capsule") return { kind: "capsule", radius: Math.max(half[0], half[2]), half: half[1] };
    return { kind: "cuboid", half };
  }
  const record = raw as Record<string, unknown>;
  const kind = typeof record.kind === "string" ? record.kind : null;

  if (record.cuboid !== undefined || kind === "cuboid") {
    return { kind: "cuboid", half: readTriple(record.cuboid ?? record.half_extents ?? record.size) ?? half };
  }
  if (record.sphere !== undefined || kind === "sphere") {
    const radius = Array.isArray(record.sphere)
      ? numeric(record.sphere[0], Math.max(...half))
      : numeric(record.sphere ?? record.radius, Math.max(...half));
    return { kind: "sphere", radius: Math.max(radius, EPSILON) };
  }
  if (record.capsule !== undefined || kind === "capsule") {
    const capsule = record.capsule;
    const radius = Array.isArray(capsule)
      ? numeric(capsule[0], Math.max(half[0], half[2]))
      : numeric(record.radius, Math.max(half[0], half[2]));
    const total = Array.isArray(capsule) ? numeric(capsule[1], half[1] * 2) : numeric(record.height, half[1] * 2);
    return { kind: "capsule", radius: Math.max(radius, EPSILON), half: Math.max(total / 2 - radius, 0) };
  }
  if (record.heightfield !== undefined || kind === "heightfield") {
    const field = (record.heightfield ?? record) as Record<string, unknown>;
    const heights = Array.isArray(field.heights) ? field.heights.map((part) => numeric(part, 0)) : [];
    const rows = Math.max(1, Math.floor(numeric(field.rows, Math.round(Math.sqrt(heights.length)) || 1)));
    const cols = Math.max(1, Math.floor(numeric(field.cols, heights.length / rows || 1)));
    if (heights.length >= rows * cols) return { kind: "heightfield", half, rows, cols, heights };
    // A heightfield with no samples is a flat box; drawing it as terrain would be a lie.
    return { kind: "cuboid", half };
  }
  // `mesh` and anything unrecognised resolve against the mesh's box (documented above).
  return { kind: "cuboid", half };
};

/** Half-extents that bound any shape, for the cheap broad phase. */
export const boundsOf = (shape: Shape): Vec3 => {
  switch (shape.kind) {
    case "cuboid":
    case "heightfield":
      return shape.half;
    case "sphere":
      return [shape.radius, shape.radius, shape.radius];
    case "capsule":
      return [shape.radius, shape.half + shape.radius, shape.radius];
  }
};

export const isRecognizedCollider = (collider: unknown): boolean => {
  const raw = (collider as { shape?: unknown } | undefined)?.shape;
  if (raw == null) return true;
  if (typeof raw === "string") return ["cuboid", "sphere", "capsule", "mesh", "heightfield"].includes(raw);
  if (typeof raw !== "object" || Array.isArray(raw)) return false;
  const record = raw as Record<string, unknown>;
  const kind = typeof record.kind === "string" ? record.kind : null;
  return ["cuboid", "sphere", "capsule", "mesh", "heightfield"].some(
    (name) => kind === name || record[name] !== undefined,
  );
};

const broadOverlap = (a: Body, b: Body): boolean => {
  const ha = boundsOf(a.shape);
  const hb = boundsOf(b.shape);
  // Rotated boxes need slack, or a ramp's corner falls outside its own axis-aligned bound.
  const slack = isIdentityRotation(a.rotation) && isIdentityRotation(b.rotation) ? 0 : 0.75;
  return (
    Math.abs(a.position[0] - b.position[0]) <= ha[0] + hb[0] + slack &&
    Math.abs(a.position[1] - b.position[1]) <= ha[1] + hb[1] + slack &&
    Math.abs(a.position[2] - b.position[2]) <= ha[2] + hb[2] + slack
  );
};

/** The closest point on an oriented box to `point`, and whether the point is inside it. */
const closestOnBox = (
  point: Vec3,
  center: Vec3,
  half: Vec3,
  axes: [Vec3, Vec3, Vec3],
): { closest: Vec3; inside: boolean; local: Vec3; clamped: Vec3 } => {
  const d: Vec3 = [point[0] - center[0], point[1] - center[1], point[2] - center[2]];
  const local: Vec3 = [dot(d, axes[0]), dot(d, axes[1]), dot(d, axes[2])];
  const clamped: Vec3 = [
    Math.max(-half[0], Math.min(half[0], local[0])),
    Math.max(-half[1], Math.min(half[1], local[1])),
    Math.max(-half[2], Math.min(half[2], local[2])),
  ];
  const closest: Vec3 = [
    center[0] + clamped[0] * axes[0][0] + clamped[1] * axes[1][0] + clamped[2] * axes[2][0],
    center[1] + clamped[0] * axes[0][1] + clamped[1] * axes[1][1] + clamped[2] * axes[2][1],
    center[2] + clamped[0] * axes[0][2] + clamped[1] * axes[1][2] + clamped[2] * axes[2][2],
  ];
  const inside =
    Math.abs(local[0] - clamped[0]) < EPSILON &&
    Math.abs(local[1] - clamped[1]) < EPSILON &&
    Math.abs(local[2] - clamped[2]) < EPSILON;
  return { closest, inside, local, clamped };
};

/** Sphere-vs-oriented-box, returning the push-out direction and depth. */
const sphereBoxContact = (
  point: Vec3,
  radius: number,
  box: Body,
  half: Vec3,
  axes: [Vec3, Vec3, Vec3],
): Contact | null => {
  const { closest, inside, local } = closestOnBox(point, box.position, half, axes);
  if (inside) {
    // Deepest-face escape: pick the axis the point is nearest to leaving through.
    let bestAxis = 0;
    let bestDepth = Number.POSITIVE_INFINITY;
    for (let axis = 0; axis < 3; axis += 1) {
      const depth = half[axis] - Math.abs(local[axis]);
      if (depth < bestDepth) {
        bestDepth = depth;
        bestAxis = axis;
      }
    }
    const sign = local[bestAxis] >= 0 ? 1 : -1;
    const a = axes[bestAxis];
    return { normal: [a[0] * sign, a[1] * sign, a[2] * sign], depth: bestDepth + radius };
  }
  const away: Vec3 = [point[0] - closest[0], point[1] - closest[1], point[2] - closest[2]];
  const distance = length(away);
  if (distance >= radius) return null;
  return { normal: normalise(away), depth: radius - distance };
};

/**
 * The point on a capsule's spine closest to a box. Two refinement passes: project the box
 * centre onto the spine, find the box point nearest that, then re-project. This converges
 * for the shapes a level is made of and costs no iteration budget worth measuring.
 */
const spineClosestToBox = (
  bottom: Vec3,
  top: Vec3,
  box: Body,
  half: Vec3,
  axes: [Vec3, Vec3, Vec3],
): Vec3 => {
  const segment: Vec3 = [top[0] - bottom[0], top[1] - bottom[1], top[2] - bottom[2]];
  const lengthSquared = dot(segment, segment);
  const project = (target: Vec3): Vec3 => {
    if (lengthSquared < EPSILON) return [...bottom];
    const t = Math.max(
      0,
      Math.min(
        1,
        ((target[0] - bottom[0]) * segment[0] +
          (target[1] - bottom[1]) * segment[1] +
          (target[2] - bottom[2]) * segment[2]) /
          lengthSquared,
      ),
    );
    return [bottom[0] + segment[0] * t, bottom[1] + segment[1] * t, bottom[2] + segment[2] * t];
  };
  let point = project(box.position);
  for (let pass = 0; pass < 2; pass += 1) {
    const { closest } = closestOnBox(point, box.position, half, axes);
    point = project(closest);
  }
  return point;
};

/** Sample a heightfield's surface under a world-space point. */
const heightfieldAt = (body: Body, shape: Extract<Shape, { kind: "heightfield" }>, x: number, z: number): number => {
  const u = (x - body.position[0]) / (shape.half[0] * 2) + 0.5;
  const v = (z - body.position[2]) / (shape.half[2] * 2) + 0.5;
  if (u < 0 || u > 1 || v < 0 || v > 1) return Number.NEGATIVE_INFINITY;
  const col = Math.max(0, Math.min(shape.cols - 1, Math.round(u * (shape.cols - 1))));
  const row = Math.max(0, Math.min(shape.rows - 1, Math.round(v * (shape.rows - 1))));
  return body.position[1] + (shape.heights[row * shape.cols + col] ?? 0);
};

/** Keyboard/gamepad state is addressed by DOM `code`, matching assets/input.json. */
export class RuntimeInput {
  private readonly pressed = new Set<string>();
  private readonly previous = new Set<string>();
  private readonly document: InputDocument;

  constructor(document: InputDocument) {
    this.document = document;
  }

  set(code: string, pressed: boolean): void {
    if (pressed) this.pressed.add(code);
    else this.pressed.delete(code);
  }

  action(name: string): boolean {
    const binding = this.document.actions?.find((entry) => entry.name === name);
    return binding?.keys.some((key) => this.pressed.has(key)) ?? false;
  }

  actionPressed(name: string): boolean {
    const binding = this.document.actions?.find((entry) => entry.name === name);
    return binding?.keys.some((key) => this.pressed.has(key) && !this.previous.has(key)) ?? false;
  }

  axis(name: string): number {
    const binding = this.document.axes?.find((entry) => entry.name === name);
    if (!binding) return 0;
    const positive = binding.positive.some((key) => this.pressed.has(key)) ? 1 : 0;
    const negative = binding.negative.some((key) => this.pressed.has(key)) ? 1 : 0;
    return positive - negative;
  }

  endFrame(): void {
    this.previous.clear();
    for (const code of this.pressed) this.previous.add(code);
  }

  clear(): void {
    this.pressed.clear();
    this.previous.clear();
  }
}

export type PlayRuntimeOptions = {
  /** Compiled programs keyed by entity id; `bhippi-engine::script` produced them. */
  scripts?: Map<string, ScriptProgram>;
  /** Seed for `random()`, so a play session replays identically (ENG-187). */
  seed?: number;
  /** Pause the moment a script faults, so the frame that broke is the one on screen. */
  pauseOnError?: boolean;
};

export class PlayRuntime {
  readonly input: RuntimeInput;
  private readonly bodies = new Map<string, Body>();
  private readonly entities = new Map<string, RuntimeEntity>();
  private readonly variables: Record<string, string | number | boolean>;
  private readonly gravity: Vec3;
  private readonly authoredJson: string;
  private readonly vms = new Map<string, ScriptVm>();
  private readonly hudText: Record<string, string> = {};
  private readonly hudVisible: Record<string, boolean> = {};
  private readonly pauseOnError: boolean;
  private readonly seed: number;
  private random: () => number;
  private elapsed = 0;
  private paused = false;
  private started = false;
  private frameSamples: number[] = [];
  private scriptFaults = 0;
  private contacts = 0;
  private spawnCounter = 0;
  /** Filled by host calls during a hook, drained into the frame. */
  private pending: RuntimeEvent[] = [];
  private spawnedThisFrame: RuntimeEntity[] = [];
  private removedThisFrame: string[] = [];
  private currentEntity = "";

  constructor(
    document: RuntimeDocument,
    gravity: Vec3,
    input: InputDocument,
    options: PlayRuntimeOptions = {},
  ) {
    this.gravity = gravity;
    this.authoredJson = JSON.stringify(document);
    this.input = new RuntimeInput(input);
    this.pauseOnError = options.pauseOnError ?? false;
    this.seed = options.seed ?? 0x5eed;
    this.random = makeRandom(this.seed);
    this.variables = {
      "player.health": 100,
      "game.score": 0,
      "game.timer": 0,
      "game.level": "",
      "player.ammo": 0,
    };
    for (const entity of document.entities) {
      this.entities.set(entity.id, entity);
      this.addBody(entity);
    }
    for (const [id, program] of options.scripts ?? []) {
      if (!this.entities.has(id)) continue;
      this.vms.set(id, new ScriptVm(program, this.hostTable()));
    }
  }

  private addBody(entity: RuntimeEntity): void {
    const transform = entity.components.Transform ?? {};
    const rigid = entity.components.RigidBody ?? {};
    const collider = entity.components.Collider;
    const controller = entity.components.CharacterController;
    if (!rigid.kind && !collider && !controller) return;
    const position = copyVec(transform.pos, [0, 0, 0]);
    const rotation = copyVec(transform.rot, [0, 0, 0]);
    const scale = copyVec(transform.scale, [1, 1, 1]);
    const shape: Shape = controller
      ? {
          kind: "capsule",
          radius: Math.max(numeric(controller.radius, 0.35), EPSILON),
          half: Math.max(numeric(controller.height, 1.8) / 2 - numeric(controller.radius, 0.35), 0),
        }
      : shapeOf(collider, scale);
    this.bodies.set(entity.id, {
      id: entity.id,
      position,
      rotation,
      authoredPosition: [...position],
      authoredRotation: [...rotation],
      shape,
      velocity: [0, 0, 0],
      kind: rigid.kind === "static" || rigid.kind === "kinematic" ? rigid.kind : "dynamic",
      sensor: Boolean(collider?.sensor),
      grounded: false,
      groundNormalY: 0,
      controller: controller
        ? {
            height: numeric(controller.height, 1.8),
            radius: numeric(controller.radius, 0.35),
            stepHeight: numeric(controller.step_height, 0.3),
            moveSpeed: numeric(controller.move_speed, 5),
            jumpSpeed: numeric(controller.jump_speed, 5.5),
            // The schema's default walkable limit is 45°, which is what a player expects a
            // ramp to be. A missing value must not mean "climbs walls".
            slopeCos: Math.cos(Math.max(0.01, Math.min(numeric(controller.max_slope, Math.PI / 4), Math.PI / 2))),
          }
        : null,
    });
  }

  setPaused(paused: boolean): void {
    this.paused = paused;
  }

  isPaused(): boolean {
    return this.paused;
  }

  setVariable(path: string, value: string | number | boolean): void {
    this.variables[path] = value;
  }

  /** Which entities carry a compiled script — the transport bar's script count. */
  scriptedEntities(): string[] {
    return [...this.vms.keys()];
  }

  /** Host functions the compiled programs referenced but this runtime does not implement. */
  unboundHosts(): string[] {
    const missing = new Set<string>();
    for (const vm of this.vms.values()) for (const name of vm.unboundHosts()) missing.add(name);
    return [...missing];
  }

  reset(): void {
    this.elapsed = 0;
    this.paused = false;
    this.started = false;
    this.frameSamples = [];
    this.scriptFaults = 0;
    this.contacts = 0;
    this.spawnCounter = 0;
    this.random = makeRandom(this.seed);
    this.input.clear();
    this.spawnedThisFrame = [];
    this.removedThisFrame = [];
    this.pending = [];
    for (const body of this.bodies.values()) {
      body.position = [...body.authoredPosition];
      body.rotation = [...body.authoredRotation];
      body.velocity = [0, 0, 0];
      body.grounded = false;
      body.groundNormalY = 0;
    }
    for (const key of Object.keys(this.hudText)) delete this.hudText[key];
    for (const key of Object.keys(this.hudVisible)) delete this.hudVisible[key];
  }

  /** Advance one deterministic frame. `force` implements Step while paused. */
  update(deltaSeconds: number, timeScale = 1, force = false): RuntimeFrame {
    const rawDelta = Math.max(0, Math.min(deltaSeconds, 0.05));
    const delta = rawDelta * Math.max(0.01, Math.min(timeScale, 4));
    const events: RuntimeEvent[] = [];
    this.spawnedThisFrame = [];
    this.removedThisFrame = [];

    if (!this.paused || force) {
      if (!this.started) {
        this.started = true;
        this.runHook("on_start", []);
      }
      this.elapsed += delta;
      this.variables["game.timer"] = this.elapsed;
      this.contacts = 0;
      this.simulate(delta, events);
      this.runHook("on_update", [delta]);
      // Contact hooks run after the solver so a script sees resolved positions, not the
      // interpenetrating ones that produced the event. Both parties are notified, each with
      // the other's id: a door script and the player script are equally entitled to react,
      // and the frame carries one event either way.
      for (const event of events) {
        if (event.kind === "collision") {
          this.runHookOn(event.entity, "on_collision", [event.other]);
          this.runHookOn(event.other, "on_collision", [event.entity]);
        }
        if (event.kind === "trigger") {
          this.runHookOn(event.entity, "on_trigger", [event.other]);
          this.runHookOn(event.other, "on_trigger", [event.entity]);
        }
      }
    }
    events.push(...this.pending);
    this.pending = [];
    this.input.endFrame();

    if (rawDelta > 0) {
      this.frameSamples.push(rawDelta);
      if (this.frameSamples.length > 60) this.frameSamples.shift();
    }
    const average = this.frameSamples.length
      ? this.frameSamples.reduce((sum, value) => sum + value, 0) / this.frameSamples.length
      : 0;

    return {
      transforms: new Map(Array.from(this.bodies, ([id, body]) => [id, [...body.position] as Vec3])),
      rotations: new Map(Array.from(this.bodies, ([id, body]) => [id, [...body.rotation] as Vec3])),
      spawned: this.spawnedThisFrame,
      removed: this.removedThisFrame,
      variables: { ...this.variables },
      hud: { ...this.hudText },
      hudVisible: { ...this.hudVisible },
      stats: {
        fps: average > 0 ? 1 / average : 0,
        frameMs: average * 1000,
        entities: this.entities.size,
        simulatedBodies: Array.from(this.bodies.values()).filter((body) => body.kind !== "static").length,
        contacts: this.contacts,
        scripts: this.vms.size,
        scriptFaults: this.scriptFaults,
        elapsed: this.elapsed,
        paused: this.paused,
      },
      events,
    };
  }

  authoredStateUnchanged(document: RuntimeDocument): boolean {
    return JSON.stringify(document) === this.authoredJson;
  }

  // -- scripting ----------------------------------------------------------------------

  private runHook(hook: "on_start" | "on_update", args: ScriptValue[]): void {
    for (const id of [...this.vms.keys()]) this.runHookOn(id, hook, args);
  }

  private runHookOn(
    id: string,
    hook: "on_start" | "on_update" | "on_collision" | "on_trigger",
    args: ScriptValue[],
  ): void {
    const vm = this.vms.get(id);
    if (!vm || !vm.hasHook(hook)) return;
    this.currentEntity = id;
    const fault = vm.run(hook, args);
    this.currentEntity = "";
    if (!fault) return;
    this.scriptFaults += 1;
    this.pending.push({ kind: "script_fault", entity: id, hook, ...fault });
    // A script that faults is disabled for the rest of the session: re-running it every
    // frame would bury the Output Log under the same line sixty times a second.
    this.vms.delete(id);
    if (this.pauseOnError) this.paused = true;
  }

  private resolveEntity(value: ScriptValue): string {
    const id = typeof value === "string" ? value : "";
    return id === "" ? this.currentEntity : id;
  }

  private bodyOf(value: ScriptValue): Body | undefined {
    return this.bodies.get(this.resolveEntity(value));
  }

  private hostTable(): ScriptHostTable {
    const num = (value: ScriptValue): number => (typeof value === "number" ? value : Number(value) || 0);
    const text = (value: ScriptValue): string => (typeof value === "string" ? value : renderValue(value));
    const positionOf = (value: ScriptValue, axis: number): number =>
      this.bodyOf(value)?.position[axis] ?? this.entityPosition(this.resolveEntity(value))[axis];

    return {
      self_id: () => this.currentEntity,
      log: ([message]) => {
        this.pending.push({ kind: "log", entity: this.currentEntity, message: text(message) });
        return null;
      },
      time: () => this.elapsed,

      get_var: ([path]) => this.variables[text(path)] ?? null,
      set_var: ([path, value]) => {
        this.variables[text(path)] = value === null ? "" : value;
        return null;
      },

      pos_x: ([id]) => positionOf(id, 0),
      pos_y: ([id]) => positionOf(id, 1),
      pos_z: ([id]) => positionOf(id, 2),
      set_pos: ([id, x, y, z]) => {
        const body = this.bodyOf(id);
        if (body) body.position = [num(x), num(y), num(z)];
        return null;
      },
      translate: ([id, x, y, z]) => {
        const body = this.bodyOf(id);
        if (body) {
          body.position[0] += num(x);
          body.position[1] += num(y);
          body.position[2] += num(z);
        }
        return null;
      },
      rot_y: ([id]) => this.bodyOf(id)?.rotation[1] ?? 0,
      set_rot: ([id, x, y, z]) => {
        const body = this.bodyOf(id);
        if (body) body.rotation = [num(x), num(y), num(z)];
        return null;
      },
      vel_x: ([id]) => this.bodyOf(id)?.velocity[0] ?? 0,
      vel_y: ([id]) => this.bodyOf(id)?.velocity[1] ?? 0,
      vel_z: ([id]) => this.bodyOf(id)?.velocity[2] ?? 0,
      set_vel: ([id, x, y, z]) => {
        const body = this.bodyOf(id);
        if (body) body.velocity = [num(x), num(y), num(z)];
        return null;
      },
      grounded: ([id]) => this.bodyOf(id)?.grounded ?? false,

      find: ([name]) => {
        const wanted = text(name);
        for (const entity of this.entities.values()) if (entity.name === wanted) return entity.id;
        return "";
      },
      find_tag: ([tag]) => {
        const wanted = text(tag);
        for (const entity of this.entities.values()) if (entity.tags.includes(wanted)) return entity.id;
        return "";
      },
      name_of: ([id]) => this.entities.get(this.resolveEntity(id))?.name ?? "",
      has_tag: ([id, tag]) => this.entities.get(this.resolveEntity(id))?.tags.includes(text(tag)) ?? false,
      distance: ([a, b]) => {
        const left = this.entityPosition(this.resolveEntity(a));
        const right = this.entityPosition(this.resolveEntity(b));
        return length([left[0] - right[0], left[1] - right[1], left[2] - right[2]]);
      },
      exists: ([id]) => this.entities.has(this.resolveEntity(id)),

      spawn: ([source, x, y, z]) => this.spawn(text(source), [num(x), num(y), num(z)]),
      destroy: ([id]) => {
        const target = this.resolveEntity(id);
        if (this.entities.delete(target)) {
          this.bodies.delete(target);
          this.vms.delete(target);
          this.removedThisFrame.push(target);
          this.pending.push({ kind: "destroy", entity: target });
        }
        return null;
      },
      play_sound: ([asset]) => {
        this.pending.push({ kind: "sound", asset: text(asset) });
        return null;
      },
      load_level: ([name]) => {
        this.pending.push({ kind: "level", name: text(name) });
        return null;
      },

      hud_set: ([widget, value]) => {
        this.hudText[text(widget)] = renderValue(value);
        return null;
      },
      hud_show: ([widget, visible]) => {
        this.hudVisible[text(widget)] = visible === true || visible === "true" || visible === 1;
        return null;
      },

      is_action: ([name]) => this.input.action(text(name)),
      action_pressed: ([name]) => this.input.actionPressed(text(name)),
      axis: ([name]) => this.input.axis(text(name)),

      abs: ([v]) => Math.abs(num(v)),
      min: ([a, b]) => Math.min(num(a), num(b)),
      max: ([a, b]) => Math.max(num(a), num(b)),
      clamp: ([v, lo, hi]) => Math.max(num(lo), Math.min(num(hi), num(v))),
      floor: ([v]) => Math.floor(num(v)),
      ceil: ([v]) => Math.ceil(num(v)),
      round: ([v]) => Math.round(num(v)),
      sqrt: ([v]) => Math.sqrt(Math.max(0, num(v))),
      sin: ([v]) => Math.sin(num(v)),
      cos: ([v]) => Math.cos(num(v)),
      random: () => this.random(),
      to_string: ([v]) => renderValue(v),
    };
  }

  private entityPosition(id: string): Vec3 {
    const body = this.bodies.get(id);
    if (body) return body.position;
    const entity = this.entities.get(id);
    return copyVec(entity?.components.Transform?.pos, [0, 0, 0]);
  }

  /**
   * Spawn a runtime-only entity. Ids are namespaced `spawn:` so nothing can mistake one for
   * an authored entity and try to persist it — INV-081 again, from the other direction.
   */
  private spawn(source: string, position: Vec3): string {
    this.spawnCounter += 1;
    const id = `spawn:${this.spawnCounter}`;
    const entity: RuntimeEntity = {
      id,
      name: source.replace(/^(builtin:|asset:|prefab:)/, "") || "Spawned",
      tags: ["runtime"],
      components: {
        Transform: { pos: [...position], rot: [0, 0, 0], scale: [1, 1, 1] },
        MeshRenderer: { mesh: source.startsWith("builtin:") || source.startsWith("asset:") ? source : "builtin:cube" },
        RigidBody: { kind: "dynamic" },
        Collider: { shape: { cuboid: [1, 1, 1] }, sensor: false },
      },
    };
    this.entities.set(id, entity);
    this.addBody(entity);
    this.spawnedThisFrame.push(entity);
    this.pending.push({ kind: "spawn", entity: id });
    return id;
  }

  // -- solver -------------------------------------------------------------------------

  private simulate(delta: number, events: RuntimeEvent[]): void {
    const all = Array.from(this.bodies.values());
    const solids = all.filter((body) => body.kind === "static" && !body.sensor);
    const sensors = all.filter((body) => body.sensor);

    for (const body of all) {
      if (body.kind === "static") continue;

      if (body.controller) {
        body.velocity[0] = this.input.axis("move_x") * body.controller.moveSpeed;
        body.velocity[2] = this.input.axis("move_z") * body.controller.moveSpeed;
        if (body.grounded && this.input.actionPressed("jump")) {
          body.velocity[1] = body.controller.jumpSpeed;
        }
      }

      body.velocity[0] += this.gravity[0] * delta;
      body.velocity[1] += this.gravity[1] * delta;
      body.velocity[2] += this.gravity[2] * delta;

      const before: Vec3 = [...body.position];
      body.position[0] += body.velocity[0] * delta;
      body.position[1] += body.velocity[1] * delta;
      body.position[2] += body.velocity[2] * delta;

      body.grounded = false;
      body.groundNormalY = 0;
      this.resolve(body, before, solids, events);

      for (const sensor of sensors) {
        if (sensor.id === body.id || !broadOverlap(body, sensor)) continue;
        if (this.contact(body, sensor)) {
          events.push({ kind: "trigger", entity: body.id, other: sensor.id });
        }
      }
    }
  }

  /**
   * Push `body` out of every solid it overlaps, up to a few passes so a body wedged in a
   * corner settles instead of oscillating between two walls.
   */
  private resolve(body: Body, before: Vec3, solids: Body[], events: RuntimeEvent[]): void {
    const reported = new Set<string>();
    for (let pass = 0; pass < 4; pass += 1) {
      let moved = false;
      for (const solid of solids) {
        if (!broadOverlap(body, solid)) continue;
        const contact = this.contact(body, solid);
        if (!contact) continue;
        moved = true;
        if (!reported.has(solid.id)) {
          reported.add(solid.id);
          this.contacts += 1;
          events.push({ kind: "collision", entity: body.id, other: solid.id });
        }

        const normal = contact.normal;
        const controller = body.controller;
        const walkable = controller ? normal[1] >= controller.slopeCos : normal[1] > 0.5;

        if (controller && !walkable && normal[1] > 0.05) {
          // A slope too steep to stand on: slide along it rather than being held up by it,
          // which is what makes a slope limit visible instead of decorative.
          const into = dot(body.velocity, normal);
          body.velocity[0] -= normal[0] * into;
          body.velocity[1] -= normal[1] * into;
          body.velocity[2] -= normal[2] * into;
        }

        body.position[0] += normal[0] * contact.depth;
        body.position[1] += normal[1] * contact.depth;
        body.position[2] += normal[2] * contact.depth;

        const into = dot(body.velocity, normal);
        if (into < 0) {
          body.velocity[0] -= normal[0] * into;
          body.velocity[1] -= normal[1] * into;
          body.velocity[2] -= normal[2] * into;
        }

        if (normal[1] > 0.05) {
          body.groundNormalY = Math.max(body.groundNormalY, normal[1]);
          if (walkable) body.grounded = true;
        }
      }
      if (!moved) break;
    }

    // Step-up: a ledge under `step_height` that blocked horizontal motion is climbed rather
    // than walked into, which is the difference between a staircase and a wall.
    const controller = body.controller;
    if (controller && !body.grounded && controller.stepHeight > 0) {
      const blocked =
        Math.abs(body.position[0] - before[0]) < Math.abs(body.velocity[0]) * 1e-3 &&
        Math.abs(body.velocity[0]) > EPSILON;
      if (blocked) {
        const lifted: Body = { ...body, position: [body.position[0], body.position[1] + controller.stepHeight, body.position[2]] };
        const clear = solids.every((solid) => !broadOverlap(lifted, solid) || !this.contact(lifted, solid));
        if (clear) {
          body.position[1] += controller.stepHeight;
          body.grounded = true;
        }
      }
    }
  }

  /** Narrow phase. Everything reduces to sphere/capsule against an oriented box. */
  private contact(body: Body, solid: Body): Contact | null {
    const axes = axesOf(solid.rotation);
    if (solid.shape.kind === "heightfield") {
      const surface = heightfieldAt(solid, solid.shape, body.position[0], body.position[2]);
      if (!Number.isFinite(surface)) return null;
      const bottom = body.position[1] - boundsOf(body.shape)[1];
      return bottom < surface ? { normal: [0, 1, 0], depth: surface - bottom } : null;
    }
    const half = solid.shape.kind === "cuboid" ? solid.shape.half : boundsOf(solid.shape);

    switch (body.shape.kind) {
      case "sphere":
        return sphereBoxContact(body.position, body.shape.radius, solid, half, axes);
      case "capsule": {
        const bottom: Vec3 = [body.position[0], body.position[1] - body.shape.half, body.position[2]];
        const top: Vec3 = [body.position[0], body.position[1] + body.shape.half, body.position[2]];
        const point = spineClosestToBox(bottom, top, solid, half, axes);
        return sphereBoxContact(point, body.shape.radius, solid, half, axes);
      }
      case "cuboid":
      case "heightfield": {
        // A moving box is resolved as the sphere that bounds it: the solver is kinematic and
        // has no way to rotate a box out of a contact, so a tighter test would only produce
        // jitter it cannot fix.
        const radius = Math.max(...body.shape.half);
        return sphereBoxContact(body.position, radius, solid, half, axes);
      }
    }
  }
}

export type ScriptedPlaytestStep = {
  keys: string[];
  frames: number;
  note?: string;
};

export type ScriptedPlaytestReport = {
  authoredUnchanged: boolean;
  authoredHashBefore: string;
  authoredHashAfter: string;
  completed: boolean;
  frames: number;
  samples: Array<{
    step: number;
    note?: string;
    keys: string[];
    transforms: Record<string, Vec3>;
    variables: Readonly<Record<string, string | number | boolean>>;
    events: RuntimeEvent[];
  }>;
  stats: RuntimeStats | null;
  faults: RuntimeEvent[];
};

/**
 * Run the same deterministic runtime as Play against a bounded, Rust-validated input plan.
 * The helper has no renderer and no persistence: discarding `runtime` is Stop, and the
 * authored JSON equality in the report proves the test did not leak into edit state.
 */
export function runScriptedPlaytest(
  document: RuntimeDocument,
  gravity: Vec3,
  input: InputDocument,
  scripts: Map<string, ScriptProgram>,
  steps: ScriptedPlaytestStep[],
  fixedDeltaSeconds: number,
): ScriptedPlaytestReport {
  const authored = JSON.stringify(document);
  const runtime = new PlayRuntime(document, gravity, input, { scripts, pauseOnError: false });
  const samples: ScriptedPlaytestReport["samples"] = [];
  const faults: RuntimeEvent[] = [];
  let frameCount = 0;
  let last: RuntimeFrame | null = null;
  let completed = true;

  outer: for (let index = 0; index < steps.length; index += 1) {
    const step = steps[index];
    runtime.input.clear();
    for (const key of step.keys) runtime.input.set(key, true);
    const events: RuntimeEvent[] = [];
    let abort = false;
    for (let frame = 0; frame < step.frames; frame += 1) {
      try {
        last = runtime.update(fixedDeltaSeconds);
      } catch (error) {
        const fault: RuntimeEvent = {
          kind: "fault",
          message: error instanceof Error ? error.message : String(error),
          hint: "Fix the runtime fault, then repeat this exact scripted playtest.",
        };
        events.push(fault);
        faults.push(fault);
        completed = false;
        abort = true;
        break;
      }
      frameCount += 1;
      events.push(...last.events);
      faults.push(
        ...last.events.filter((event) => event.kind === "fault" || event.kind === "script_fault"),
      );
    }
    runtime.input.clear();
    if (last) {
      samples.push({
        step: index,
        note: step.note,
        keys: [...step.keys],
        transforms: Object.fromEntries(last.transforms),
        variables: last.variables,
        events,
      });
    }
    if (abort) break outer;
  }

  const authoredAfter = JSON.stringify(document);
  return {
    authoredUnchanged: authoredAfter === authored,
    authoredHashBefore: stableTextHash(authored),
    authoredHashAfter: stableTextHash(authoredAfter),
    completed,
    frames: frameCount,
    samples,
    stats: last?.stats ?? null,
    faults,
  };
}

/** Small deterministic content hash for an observation report; persistence still uses bytes. */
function stableTextHash(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return `fnv1a32:${(hash >>> 0).toString(16).padStart(8, "0")}`;
}
