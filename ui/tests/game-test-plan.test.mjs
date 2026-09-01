import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { PlayRuntime, RuntimeInput } from "../src/engine/playRuntime.ts";
import {
  GAME_TEST_PLAN_FORMAT,
  parseRuntimeHudDocument,
  runGameTestScenarioWithRuntime,
} from "../src/engine/gameTestPlan.ts";
import { executeGameTestBatch } from "../src/engine/gameTestBatchRunner.ts";
import { sha256SessionIdentity } from "../src/engine/gameTestIdentity.ts";
import {
  DEFAULT_RUNTIME_WORKER_BUDGETS,
  RUNTIME_PROTOCOL_FORMAT,
  RuntimeWorkerSession,
} from "../src/engine/runtimeWorkerSession.ts";

const input = {
  format: "bhippi-input@1",
  actions: [{ name: "jump", keys: ["Space"] }],
  axes: [{ name: "move_x", positive: ["KeyD"], negative: ["KeyA"] }],
};

const controllerWorld = () => ({
  entities: [{
    id: "player",
    name: "Player",
    tags: ["player"],
    components: {
      Transform: {
        pos: [0, 0, 0],
        rot: [0, Math.SQRT1_2, 0, Math.SQRT1_2],
        scale: [1, 2, 1],
      },
      RigidBody: { kind: "kinematic" },
      CharacterController: { height: 1.8, radius: 0.35, move_speed: 4 },
    },
  }],
});

test("logical press/release preserves action edge semantics independently of key bindings", () => {
  const state = new RuntimeInput(input);
  state.setAction("jump", true);
  assert.equal(state.action("jump"), true);
  assert.equal(state.actionPressed("jump"), true);
  state.endFrame();
  assert.equal(state.actionPressed("jump"), false);
  state.setAction("jump", false);
  assert.equal(state.action("jump"), false);
});

test("timed logical axes snap deterministically to fixed frames and retain state", () => {
  const document = controllerWorld();
  const runtime = new PlayRuntime(document, [0, 0, 0], input, { seed: 9 });
  const scenario = {
    name: "logical movement",
    initial_level: "assets/scenes/level_01.bscn.json",
    seed: 9,
    input: [
      { at_ms: 0, kind: "axis", axis: "move_x", value: 1 },
      { at_ms: 100, kind: "axis", axis: "move_x", value: 0 },
    ],
    checkpoints: [
      {
        name: "moving",
        at_ms: 83,
        assertions: [{
          kind: "transform",
          entity: "Player",
          translation: [1 / 3, 0.3, 0],
          rotation_degrees: [0, 90, 0],
          tolerance: 1e-9,
        }],
      },
      {
        name: "stopped",
        at_ms: 200,
        assertions: [{
          kind: "transform",
          entity: "player",
          translation: [1 / 3, 0.3, 0],
          scale: [1, 2, 1],
          tolerance: 1e-9,
        }],
      },
    ],
  };
  const first = runGameTestScenarioWithRuntime(
    document,
    runtime,
    scenario,
    null,
    [scenario.initial_level],
    1 / 60,
  );
  const secondDocument = controllerWorld();
  const second = runGameTestScenarioWithRuntime(
    secondDocument,
    new PlayRuntime(secondDocument, [0, 0, 0], input, { seed: 9 }),
    scenario,
    null,
    [scenario.initial_level],
    1 / 60,
  );
  assert.equal(first.frames, 12);
  assert.deepEqual(first.assertions.map((item) => item.passed), [true, true]);
  assert.deepEqual(first.assertions[0].expected.rotation_degrees, [0, 90, 0]);
  assert.equal(first.assertions[0].expected.scale, null);
  assert.deepEqual(first.samples.map((sample) => sample.checkpointHash), second.samples.map((sample) => sample.checkpointHash));
  assert.equal(first.authoredUnchanged, true);
});

test("variable, event, transform, HUD and level assertions use observed runtime facts", () => {
  const program = JSON.parse(readFileSync(new URL("./fixtures/pickup.program.json", import.meta.url), "utf8"));
  const document = {
    entities: [
      {
        id: "pickup",
        name: "Coin",
        tags: ["pickup"],
        components: {
          Transform: { pos: [0, 1, 0], rot: [0, 0, 0], scale: [1, 1, 1] },
          RigidBody: { kind: "static" },
          Collider: { shape: { cuboid: [1, 1, 1] }, sensor: true },
          ScriptRef: { script: "assets/scripts/pickup.rhai" },
        },
      },
      {
        id: "player",
        name: "Player",
        tags: ["player"],
        components: {
          Transform: { pos: [0, 1, 0], rot: [0, 0, 0], scale: [1, 2, 1] },
          RigidBody: { kind: "dynamic" },
          CharacterController: { height: 1.8, radius: 0.35, move_speed: 4 },
        },
      },
    ],
  };
  const hud = parseRuntimeHudDocument(JSON.stringify({
    widgets: [{
      id: "score_label",
      name: "Score",
      visible: true,
      props: { text: "Score 0" },
      style: {},
      bind: {},
    }],
  }));
  const scenario = {
    name: "all facts",
    initial_level: "assets/scenes/level_01.bscn.json",
    seed: 4,
    input: [],
    checkpoints: [{
      name: "after start",
      at_ms: 0,
      assertions: [
        { kind: "variable", path: "game.score", comparison: "equal", expected: 10 },
        { kind: "event", name: "sound", min_count: 1 },
        { kind: "transform", entity: "Player", scale: [1, 2, 1], tolerance: 0 },
        { kind: "hud", widget: "score_label", property: "text", comparison: "equal", expected: "Score 10" },
        { kind: "level_travel", level: "assets/scenes/level_01.bscn.json" },
      ],
    }],
  };
  const report = runGameTestScenarioWithRuntime(
    document,
    new PlayRuntime(document, [0, 0, 0], input, { scripts: new Map([["pickup", program]]) }),
    scenario,
    hud,
    [scenario.initial_level],
    1 / 60,
  );
  assert.deepEqual(report.assertions.map((item) => item.passed), [true, true, true, true, true]);
  assert.equal(report.samples.length, 1);
  assert.equal(report.faults.length, 0);
});

test("missing and ambiguous observations fail explicitly instead of fabricating a pass", () => {
  const document = controllerWorld();
  document.entities.push({ ...document.entities[0], id: "other", name: "Player" });
  const scenario = {
    name: "bad evidence",
    initial_level: "assets/scenes/level_01.bscn.json",
    seed: 1,
    input: [],
    checkpoints: [{
      name: "missing",
      at_ms: 0,
      assertions: [
        { kind: "variable", path: "missing.value", comparison: "not_equal", expected: 0 },
        { kind: "transform", entity: "Player", translation: [0, 0, 0], tolerance: 0 },
        { kind: "hud", widget: "Absent", property: "text", comparison: "not_equal", expected: "" },
        { kind: "event", name: "finish", min_count: 1 },
        { kind: "level_travel", level: "assets/scenes/level_02.bscn.json" },
      ],
    }],
  };
  const report = runGameTestScenarioWithRuntime(
    document,
    new PlayRuntime(document, [0, 0, 0], input),
    scenario,
    null,
    [scenario.initial_level],
    1 / 60,
  );
  assert.deepEqual(report.assertions.map((item) => item.passed), [false, false, false, false, false]);
  assert.match(report.assertions[0].address, /^runtime:\/\/scenario\//);
  assert.deepEqual(report.assertions[0].observed, { status: "missing_variable", path: "missing.value" });
  assert.equal(report.assertions[1].observed.status, "ambiguous_entity");
});

test("worker scenario requests are one-shot and keep their own sandbox evidence", () => {
  const session = new RuntimeWorkerSession("scenario-worker");
  const envelope = (sequence, payload) => ({
    format: RUNTIME_PROTOCOL_FORMAT,
    sessionNonce: "scenario-worker",
    sequence,
    payload,
  });
  assert.equal(session.handle(envelope(0, {
    kind: "start",
    document: controllerWorld(),
    gravity: [0, 0, 0],
    input,
    hud: null,
    levels: ["assets/scenes/level_01.bscn.json"],
    programs: [],
    capabilities: ["input_read"],
    seed: 12,
    pauseOnError: false,
    budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS },
  })).payload.kind, "started");
  const response = session.handle(envelope(1, {
    kind: "game_test_scenario",
    scenario: {
      name: "worker smoke",
      initial_level: "assets/scenes/level_01.bscn.json",
      seed: 12,
      input: [],
      checkpoints: [{
        name: "loaded",
        at_ms: 0,
        assertions: [{ kind: "level_travel", level: "assets/scenes/level_01.bscn.json" }],
      }],
    },
    fixedDeltaSeconds: 1 / 60,
  }));
  assert.equal(response.payload.kind, "game_test_report");
  assert.equal(response.payload.report.assertions[0].passed, true);
  assert.equal(response.payload.report.sandbox.capabilities[0], "input_read");
  assert.equal(session.handle(envelope(2, { kind: "reset" })).payload.kind, "fault");
});

test("a failed assertion keeps clean worker termination separate from scenario completion", () => {
  const session = new RuntimeWorkerSession("failed-assertion");
  const envelope = (sequence, payload) => ({
    format: RUNTIME_PROTOCOL_FORMAT,
    sessionNonce: "failed-assertion",
    sequence,
    payload,
  });
  assert.equal(session.handle(envelope(0, {
    kind: "start",
    document: controllerWorld(),
    gravity: [0, 0, 0],
    input,
    hud: null,
    levels: ["assets/scenes/level_01.bscn.json", "assets/scenes/level_02.bscn.json"],
    programs: [],
    capabilities: [],
    seed: 2,
    pauseOnError: false,
    budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS },
  })).payload.kind, "started");
  const response = session.handle(envelope(1, {
    kind: "game_test_scenario",
    scenario: {
      name: "clean failure",
      initial_level: "assets/scenes/level_01.bscn.json",
      seed: 2,
      input: [],
      checkpoints: [{
        name: "wrong level",
        at_ms: 0,
        assertions: [{ kind: "level_travel", level: "assets/scenes/level_02.bscn.json" }],
      }],
    },
    fixedDeltaSeconds: 1 / 60,
  }));
  assert.equal(response.payload.kind, "game_test_report");
  assert.equal(response.payload.report.assertions[0].passed, false);
  assert.equal(response.payload.report.completed, true, "the timeline itself completed");
  assert.equal(response.payload.report.faults.length, 0);
  assert.equal(response.payload.report.sandbox.terminationReason, "completed");
});

test("batch orchestration loads and isolates every scenario and derives completion", async () => {
  const plan = {
    format: GAME_TEST_PLAN_FORMAT,
    scenarios: ["first", "second"].map((name, index) => ({
      name,
      initial_level: `assets/scenes/${name}.bscn.json`,
      seed: index + 1,
      input: [],
      checkpoints: [{
        name: "loaded",
        at_ms: 0,
        assertions: [{ kind: "level_travel", level: `assets/scenes/${name}.bscn.json` }],
      }],
    })),
  };
  const loaded = [];
  const seeds = [];
  let worker = 0;
  const batch = await executeGameTestBatch(plan, {
    authoredTreeHash: "a".repeat(64),
    fixedDeltaSeconds: 1 / 60,
    watchdogMillis: 2_000,
    loadWorld: async (level) => {
      loaded.push(level);
      return { level };
    },
    startWorker: async (_world, seed) => {
      seeds.push(seed);
      worker += 1;
      const identity = worker;
      return {
        runGameTestScenario: async (scenario) => ({
          authoredUnchanged: true,
          authoredHashBefore: "fnv1a32:00000001",
          authoredHashAfter: "fnv1a32:00000001",
          completed: true,
          frames: 0,
          samples: [{ checkpointHash: `fnv1a32:0000000${identity}` }],
          assertions: [{
            checkpoint: "loaded",
            assertion_index: 0,
            passed: true,
            address: `runtime://worker/${identity}`,
            observed: { current_level: scenario.initial_level },
            expected: scenario.checkpoints[0].assertions[0],
          }],
          stats: null,
          faults: [],
          sandbox: sandboxEvidence(),
          workerSessionHash: `sha256:${String(identity).repeat(64)}`,
        }),
      };
    },
  });
  assert.deepEqual(loaded, plan.scenarios.map((scenario) => scenario.initial_level));
  assert.deepEqual(seeds, [1, 2]);
  assert.equal(new Set(batch.scenarios.map((scenario) => scenario.worker_session_hash)).size, 2);
  assert.deepEqual(batch.scenarios.map((scenario) => scenario.completed), [true, true]);
  assert.notStrictEqual(batch.scenarios[0].runtime, batch.scenarios[1].runtime);
});

test("worker session identities expose only a stable SHA-256 hash", async () => {
  const first = await sha256SessionIdentity("private-nonce");
  const second = await sha256SessionIdentity("private-nonce");
  assert.equal(first, second);
  assert.match(first, /^sha256:[0-9a-f]{64}$/);
  assert.doesNotMatch(first, /private-nonce/);
});

function sandboxEvidence() {
  return {
    protocol: RUNTIME_PROTOCOL_FORMAT,
    execution: "application_module_worker",
    capabilities: [],
    budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS },
    terminationReason: "completed",
    trace: {
      entries: [],
      truncated: false,
      redactions: 0,
      usage: {
        instructions: 0,
        messages: 2,
        spawnedEntities: 0,
        emittedEvents: 0,
        logBytes: 0,
        timers: 0,
        heapEstimateBytes: 100,
        wallClockMillis: 1,
      },
    },
  };
}
