import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  DEFAULT_RUNTIME_WORKER_BUDGETS,
  RUNTIME_PROTOCOL_FORMAT,
  RuntimeWorkerSession,
  deserialiseRuntimeFrame,
  redactRuntimeTraceText,
} from "../src/engine/runtimeWorkerSession.ts";

const document = { entities: [{ id: "player", name: "Player", tags: ["player"], components: { Transform: { pos: [0, 1, 0], rot: [0, 0, 0], scale: [1, 1, 1] }, RigidBody: { kind: "dynamic" } } }] };
const input = { format: "bhippi-input@1", actions: [], axes: [] };
const envelope = (sequence, payload, nonce = "run-1") => ({ format: RUNTIME_PROTOCOL_FORMAT, sessionNonce: nonce, sequence, payload });
const start = (overrides = {}) => ({ kind: "start", document, gravity: [0, 0, 0], input, programs: [], capabilities: [], seed: 7, pauseOnError: false, budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS }, ...overrides });

test("a fresh worker session is ordered, serializable and disposable", () => {
  const session = new RuntimeWorkerSession("run-1");
  assert.equal(session.handle(envelope(0, start())).payload.kind, "started");
  const response = session.handle(envelope(1, { kind: "tick", deltaSeconds: 1 / 60, timeScale: 1, force: false }));
  assert.equal(response.payload.kind, "frame");
  assert.deepEqual(deserialiseRuntimeFrame(response.payload.frame).transforms.get("player"), [0, 1, 0]);
  assert.equal(session.handle(envelope(2, { kind: "stop" })).payload.kind, "stopped");
  assert.equal(session.handle(envelope(3, { kind: "reset" })).payload.kind, "fault");
});

test("a scripted playtest runs once in the worker and returns sandbox evidence", () => {
  const session = new RuntimeWorkerSession("playtest-1");
  assert.equal(session.handle(envelope(0, start(), "playtest-1")).payload.kind, "started");
  const response = session.handle(
    envelope(
      1,
      {
        kind: "scripted_playtest",
        steps: [{ keys: [], frames: 2, note: "smoke" }],
        fixedDeltaSeconds: 1 / 60,
      },
      "playtest-1",
    ),
  );
  assert.equal(response.payload.kind, "playtest_report");
  assert.equal(response.payload.report.frames, 2);
  assert.equal(response.payload.report.authoredUnchanged, true);
  assert.equal(response.payload.report.sandbox.protocol, RUNTIME_PROTOCOL_FORMAT);
  assert.equal(response.payload.report.sandbox.execution, "application_module_worker");
  assert.equal(response.payload.report.sandbox.terminationReason, "completed");
  assert.equal(response.payload.report.sandbox.trace.entries.length, 7);
  assert.equal(response.payload.report.sandbox.trace.usage.timers, 0);
  assert.equal(session.handle(envelope(2, { kind: "reset" }, "playtest-1")).payload.kind, "fault");
});

test("sandbox trace redacts credentials, owner paths and large binary-like payloads", () => {
  const binary = "A".repeat(100);
  const redacted = redactRuntimeTraceText(
    `token=super-secret Bearer abc.def C:\\Users\\owner\\game.txt /home/owner/game ${binary}`,
  );
  assert.ok(redacted.redactions >= 5);
  assert.doesNotMatch(redacted.text, /super-secret|abc\.def|Users\\owner|home\/owner|A{80}/);
  assert.match(redacted.text, /\[redacted/);
});

test("outbound checkpoints and fault spans use the same redacted trace boundary", () => {
  const program = {
    file: "redaction-script",
    code: [
      { op: "push_str", a: 0, b: 0, line: 2 },
      { op: "call_host", a: 0, b: 1, line: 2 },
      { op: "pop", a: 0, b: 0, line: 2 },
      { op: "push_num", a: 0, b: 0, line: 3 },
      { op: "push_num", a: 1, b: 0, line: 3 },
      { op: "div", a: 0, b: 0, line: 3 },
      { op: "return", a: 0, b: 0, line: 3 },
    ],
    numbers: [1, 0],
    strings: ["token=live-secret C:\\Users\\owner\\game.txt"],
    functions: [{ name: "on_start", entry: 0, params: 0, locals: 0, line: 1 }],
    hosts: ["log"],
    hooks: [{ hook: "on_start", function: 0 }],
    step_budget: 20,
    call_depth: 2,
  };
  const session = new RuntimeWorkerSession("redacted");
  assert.equal(
    session.handle(
      envelope(0, start({ programs: [{ entity: "player", program }] }), "redacted"),
    ).payload.kind,
    "started",
  );
  const response = session.handle(
    envelope(
      1,
      { kind: "scripted_playtest", steps: [{ keys: [], frames: 1 }], fixedDeltaSeconds: 1 / 60 },
      "redacted",
    ),
  );
  assert.equal(response.payload.kind, "playtest_report");
  const encoded = JSON.stringify(response.payload.report);
  assert.doesNotMatch(encoded, /live-secret|Users\\\\owner/);
  const trace = response.payload.report.sandbox.trace;
  assert.ok(trace.redactions >= 2);
  assert.ok(trace.entries.some((entry) => entry.kind === "log" && /redacted/.test(entry.message)));
  assert.ok(
    trace.entries.some(
      (entry) =>
        entry.kind === "script_fault" && entry.line === 3 && entry.instruction === 5,
    ),
  );
});

test("nonce, sequence, authored paths and undeclared hosts fail closed", () => {
  assert.equal(new RuntimeWorkerSession("n").handle(envelope(0, start(), "wrong")).payload.kind, "fault");
  assert.equal(new RuntimeWorkerSession("n").handle({ ...envelope(1, start(), "n") }).payload.kind, "fault");
  const program = { file: "scripts/level.rhai", code: [], numbers: [], strings: [], functions: [], hosts: ["load_level"], hooks: [], step_budget: 10, call_depth: 2 };
  const pathLeak = new RuntimeWorkerSession("run-1").handle(envelope(0, start({ programs: [{ entity: "player", program }] })));
  assert.equal(pathLeak.payload.kind, "fault");
  assert.equal(pathLeak.payload.code, "invalid_start");
  const clean = { ...program, file: "player-script" };
  const denied = new RuntimeWorkerSession("run-1").handle(envelope(0, start({ programs: [{ entity: "player", program: clean }] })));
  assert.equal(denied.payload.kind, "fault");
  assert.equal(denied.payload.code, "undeclared_capability");
});

test("resource exhaustion is typed and the application-owned worker has no ambient authority", () => {
  const session = new RuntimeWorkerSession("run-1");
  assert.equal(
    session.handle(envelope(0, start({ budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS, messagesPerTick: 1 } }))).payload.kind,
    "started",
  );
  assert.equal(session.handle(envelope(1, { kind: "input", code: "KeyW", pressed: true })).payload.kind, "ack");
  const exhausted = session.handle(envelope(2, { kind: "input", code: "KeyW", pressed: false }));
  assert.equal(exhausted.payload.kind, "fault");
  assert.equal(exhausted.payload.code, "budget_exhausted");

  const heapDenied = new RuntimeWorkerSession("heap").handle(
    envelope(
      0,
      start({ budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS, heapEstimateBytes: 1 } }),
      "heap",
    ),
  );
  assert.equal(heapDenied.payload.kind, "fault");
  assert.equal(heapDenied.payload.code, "budget_exhausted");

  const worker = readFileSync(new URL("../src/engine/playRuntime.worker.ts", import.meta.url), "utf8");
  for (const forbidden of ["fetch(", "XMLHttpRequest", "WebSocket", "importScripts", "eval(", "new Function", "import("]) {
    assert.equal(worker.includes(forbidden), false, `worker must not contain ${forbidden}`);
  }
  assert.match(worker, /from "\.\/runtimeWorkerSession\.ts"/);
  assert.deepEqual(
    [...worker.matchAll(/from\s+"([^"]+)"/g)].map((match) => match[1]),
    ["./runtimeWorkerSession.ts"],
  );
  assert.doesNotMatch(worker, /@tauri-apps|__TAURI|\binvoke\s*\(/);

  const client = readFileSync(new URL("../src/engine/runtimeWorkerClient.ts", import.meta.url), "utf8");
  assert.match(client, /new Worker\(new URL\("\.\/playRuntime\.worker\.ts", import\.meta\.url\)/);
  assert.doesNotMatch(client, /Blob|createObjectURL|data:/);
  const viewport = readFileSync(new URL("../src/engine/EngineViewport.tsx", import.meta.url), "utf8");
  assert.match(viewport, /RuntimeWorkerClient\.start/);
  assert.doesNotMatch(viewport, /new PlayRuntime\(/);
  const engineView = readFileSync(new URL("../src/engine/EngineView.tsx", import.meta.url), "utf8");
  assert.match(engineView, /runtime\.runScriptedPlaytest/);
  assert.doesNotMatch(engineView, /const report = runScriptedPlaytest\(/);
});

test("instruction and call-depth ceilings are enforced by the worker broker", () => {
  const program = {
    file: "bounded-script",
    code: [
      { op: "push_unit", a: 0, b: 0, line: 1 },
      { op: "return", a: 0, b: 0, line: 1 },
    ],
    numbers: [],
    strings: [],
    functions: [{ name: "on_update", entry: 0, params: 1, locals: 1, line: 1 }],
    hosts: [],
    hooks: [{ hook: "on_update", function: 0 }],
    step_budget: 10,
    call_depth: 2,
  };
  const tooSmall = new RuntimeWorkerSession("small").handle(
    envelope(
      0,
      start({
        programs: [{ entity: "player", program }],
        budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS, instructionsPerTick: 1 },
      }),
      "small",
    ),
  );
  assert.equal(tooSmall.payload.kind, "fault");
  assert.equal(tooSmall.payload.code, "budget_exhausted");

  const session = new RuntimeWorkerSession("total");
  assert.equal(
    session.handle(
      envelope(
        0,
        start({
          programs: [{ entity: "player", program }],
          budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS, instructionsTotal: 3 },
        }),
        "total",
      ),
    ).payload.kind,
    "started",
  );
  assert.equal(
    session.handle(envelope(1, { kind: "tick", deltaSeconds: 1 / 60, timeScale: 1, force: false }, "total"))
      .payload.kind,
    "frame",
  );
  const exhausted = session.handle(
    envelope(2, { kind: "tick", deltaSeconds: 1 / 60, timeScale: 1, force: false }, "total"),
  );
  assert.equal(exhausted.payload.kind, "fault");
  assert.equal(exhausted.payload.code, "budget_exhausted");
});

test("malformed bytecode terminates only its session and a fresh nonce restarts cleanly", () => {
  const authoredBefore = JSON.stringify(document);
  const malformedProgram = {
    file: "malformed-script",
    code: [{ op: "jump", a: 99, b: 0, line: 1 }],
    numbers: [],
    strings: [],
    functions: [{ name: "on_start", entry: 0, params: 0, locals: 0, line: 1 }],
    hosts: [],
    hooks: [{ hook: "on_start", function: 0 }],
    step_budget: 10,
    call_depth: 2,
  };
  const poisoned = new RuntimeWorkerSession("poisoned");
  const rejected = poisoned.handle(
    envelope(0, start({ programs: [{ entity: "player", program: malformedProgram }] }), "poisoned"),
  );
  assert.equal(rejected.payload.kind, "fault");
  assert.equal(rejected.payload.code, "invalid_start");
  assert.match(rejected.payload.message, /invalid jump target/);
  assert.equal(poisoned.handle(envelope(1, { kind: "reset" }, "poisoned")).payload.kind, "fault");
  assert.equal(JSON.stringify(document), authoredBefore);

  const restarted = new RuntimeWorkerSession("fresh");
  assert.equal(restarted.handle(envelope(0, start(), "fresh")).payload.kind, "started");
  assert.equal(
    restarted.handle(
      envelope(1, { kind: "tick", deltaSeconds: 1 / 60, timeScale: 1, force: false }, "fresh"),
    ).payload.kind,
    "frame",
  );
  assert.equal(restarted.handle(envelope(2, { kind: "stop" }, "fresh")).payload.kind, "stopped");
  assert.equal(JSON.stringify(document), authoredBefore);
});
