import assert from "node:assert/strict";
import test from "node:test";
import {
  DEFAULT_RUNTIME_WORKER_BUDGETS,
  RUNTIME_PROTOCOL_FORMAT,
  RuntimeWorkerSession,
} from "../src/engine/runtimeWorkerSession.ts";
import { validateScriptProgram } from "../src/engine/scriptVm.ts";

const document = {
  entities: [
    {
      id: "player",
      name: "Player",
      tags: ["player"],
      components: {
        Transform: { pos: [0, 1, 0], rot: [0, 0, 0], scale: [1, 1, 1] },
        RigidBody: { kind: "dynamic" },
      },
    },
  ],
};
const input = { format: "bhippi-input@1", actions: [], axes: [] };
const envelope = (nonce, sequence, payload) => ({
  format: RUNTIME_PROTOCOL_FORMAT,
  sessionNonce: nonce,
  sequence,
  payload,
});
const start = (overrides = {}) => ({
  kind: "start",
  document,
  gravity: [0, 0, 0],
  input,
  programs: [],
  capabilities: [],
  seed: 7,
  pauseOnError: false,
  budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS },
  ...overrides,
});
const baseProgram = (overrides = {}) => ({
  file: "hostile-fixture",
  code: [{ op: "push_unit", a: 0, b: 0, line: 1 }, { op: "return", a: 0, b: 0, line: 1 }],
  numbers: [],
  strings: [],
  functions: [{ name: "on_start", entry: 0, params: 0, locals: 0, line: 1 }],
  hosts: [],
  hooks: [{ hook: "on_start", function: 0 }],
  step_budget: 100,
  call_depth: 8,
  ...overrides,
});

function runOneFrame(label, program, capabilities = [], budgets = {}) {
  const session = new RuntimeWorkerSession(label);
  const started = session.handle(
    envelope(
      label,
      0,
      start({
        programs: [{ entity: "player", program }],
        capabilities,
        budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS, ...budgets },
      }),
    ),
  );
  assert.equal(started.payload.kind, "started", JSON.stringify(started.payload));
  return session.handle(
    envelope(label, 1, { kind: "tick", deltaSeconds: 1 / 60, timeScale: 1, force: false }),
  );
}

test("invalid opcodes/constants reject and non-finite transforms normalise to bounded values", () => {
  for (const [label, program] of [
    ["opcode", baseProgram({ code: [{ op: "open_socket", a: 0, b: 0, line: 1 }] })],
    ["constant", baseProgram({ numbers: [Number.NaN] })],
  ]) {
    const response = new RuntimeWorkerSession(label).handle(
      envelope(label, 0, start({ programs: [{ entity: "player", program }] })),
    );
    assert.equal(response.payload.kind, "fault");
    assert.equal(response.payload.code, "invalid_start");
  }

  const hostileDocument = structuredClone(document);
  hostileDocument.entities[0].components.Transform.pos = [Number.NaN, Number.POSITIVE_INFINITY, 0];
  const session = new RuntimeWorkerSession("transform");
  assert.equal(
    session.handle(envelope("transform", 0, start({ document: hostileDocument }))).payload.kind,
    "started",
  );
  const frame = session.handle(
    envelope("transform", 1, { kind: "tick", deltaSeconds: 1 / 60, timeScale: 1, force: false }),
  );
  assert.equal(frame.payload.kind, "frame");
  assert.ok(frame.payload.frame.transforms.flat(2).every((value) => typeof value !== "number" || Number.isFinite(value)));
});

test("spawn, event and log floods terminate with typed budget faults", () => {
  const spawnCall = [
    { op: "push_str", a: 0, b: 0, line: 1 },
    { op: "push_num", a: 0, b: 0, line: 1 },
    { op: "push_num", a: 0, b: 0, line: 1 },
    { op: "push_num", a: 0, b: 0, line: 1 },
    { op: "call_host", a: 0, b: 4, line: 1 },
    { op: "pop", a: 0, b: 0, line: 1 },
  ];
  const spawnFlood = baseProgram({
    code: [...spawnCall, ...spawnCall, { op: "push_unit", a: 0, b: 0, line: 1 }, { op: "return", a: 0, b: 0, line: 1 }],
    numbers: [0],
    strings: ["player"],
    hosts: ["spawn"],
  });
  // `spawn` is its own grant now (docs/15 §3.1); declaring only the write grant would fault on
  // an undeclared capability rather than on the budget this case is meant to exercise.
  const spawned = runOneFrame("spawn-flood", spawnFlood, ["entity_lifecycle"], { spawnedEntities: 1 });
  assert.equal(spawned.payload.kind, "fault");
  assert.equal(spawned.payload.code, "budget_exhausted");

  const logCall = (stringIndex) => [
    { op: "push_str", a: stringIndex, b: 0, line: 2 },
    { op: "call_host", a: 0, b: 1, line: 2 },
    { op: "pop", a: 0, b: 0, line: 2 },
  ];
  const eventFlood = baseProgram({
    code: [...logCall(0), ...logCall(1), { op: "push_unit", a: 0, b: 0, line: 2 }, { op: "return", a: 0, b: 0, line: 2 }],
    strings: ["one", "two"],
    hosts: ["log"],
  });
  const events = runOneFrame("event-flood", eventFlood, [], { emittedEvents: 1 });
  assert.equal(events.payload.kind, "fault");
  assert.equal(events.payload.code, "budget_exhausted");

  const logFlood = baseProgram({
    code: [...logCall(0), { op: "push_unit", a: 0, b: 0, line: 2 }, { op: "return", a: 0, b: 0, line: 2 }],
    strings: ["123456789"],
    hosts: ["log"],
  });
  const logs = runOneFrame("log-flood", logFlood, [], { logBytes: 8 });
  assert.equal(logs.payload.kind, "fault");
  assert.equal(logs.payload.code, "budget_exhausted");
});

test("runaway loop and recursion become bounded located script faults", () => {
  const cases = [
    baseProgram({
      code: [{ op: "jump", a: 0, b: 0, line: 7 }],
      step_budget: 20,
    }),
    baseProgram({
      code: [{ op: "call_user", a: 0, b: 0, line: 9 }, { op: "return", a: 0, b: 0, line: 9 }],
      call_depth: 2,
    }),
  ];
  for (const [index, program] of cases.entries()) {
    const response = runOneFrame(`bounded-${index}`, program);
    assert.equal(response.payload.kind, "frame");
    const fault = response.payload.frame.events.find((event) => event.kind === "script_fault");
    assert.ok(fault);
    assert.ok(Number.isSafeInteger(fault.instruction));
  }
});

test("oversized, stale, replayed and undeclared-host messages fail closed", () => {
  const payloadSession = new RuntimeWorkerSession("payload");
  assert.equal(
    payloadSession.handle(
      envelope("payload", 0, start({ budgets: { ...DEFAULT_RUNTIME_WORKER_BUDGETS, messageBytes: 128 } })),
    ).payload.kind,
    "started",
  );
  const oversized = payloadSession.handle(
    envelope("payload", 1, { kind: "set_variable", path: "game.note", value: "x".repeat(256) }),
  );
  assert.equal(oversized.payload.kind, "fault");
  assert.equal(oversized.payload.code, "payload_too_large");

  assert.equal(
    new RuntimeWorkerSession("fresh").handle(envelope("stale", 0, start())).payload.code,
    "invalid_nonce",
  );
  assert.equal(
    new RuntimeWorkerSession("ordered").handle(envelope("ordered", 1, start())).payload.code,
    "out_of_order",
  );

  const hostProgram = baseProgram({
    code: [{ op: "push_str", a: 0, b: 0, line: 1 }, { op: "call_host", a: 0, b: 1, line: 1 }, { op: "return", a: 0, b: 0, line: 1 }],
    strings: ["level_02"],
    hosts: ["load_level"],
  });
  const denied = new RuntimeWorkerSession("host").handle(
    envelope("host", 0, start({ programs: [{ entity: "player", program: hostProgram }] })),
  );
  assert.equal(denied.payload.kind, "fault");
  assert.equal(denied.payload.code, "undeclared_capability");
});

test("generated bytecode shapes are valid bounded starts or typed rejection", () => {
  const operations = ["push_num", "push_str", "push_bool", "jump", "call_host", "return", "open_socket"];
  let state = 0x5eed1234;
  const next = () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state;
  };
  for (let caseIndex = 0; caseIndex < 2_048; caseIndex += 1) {
    const codeLength = next() % 8;
    const candidate = baseProgram({
      code: Array.from({ length: codeLength }, () => ({
        op: operations[next() % operations.length],
        a: (next() % 12) - 2,
        b: (next() % 6) - 1,
        line: next() % 5,
      })),
      numbers: next() % 3 === 0 ? [Number.NaN] : [next() % 100],
      strings: [String(next())],
      functions: codeLength === 0 ? [] : [{ name: "generated", entry: next() % codeLength, params: 0, locals: 0, line: 1 }],
      hooks: [],
      hosts: next() % 2 === 0 ? ["log"] : ["open_socket"],
      step_budget: (next() % 100) + 1,
      call_depth: (next() % 8) + 1,
    });
    const validation = validateScriptProgram(candidate);
    if (validation !== null) {
      assert.equal(typeof validation, "string");
      continue;
    }
    const nonce = `generated-${caseIndex}`;
    const response = new RuntimeWorkerSession(nonce).handle(
      envelope(nonce, 0, start({ programs: [{ entity: "player", program: candidate }] })),
    );
    assert.ok(response.payload.kind === "started" || response.payload.kind === "fault");
    if (response.payload.kind === "fault") {
      assert.ok(["invalid_start", "undeclared_capability", "budget_exhausted"].includes(response.payload.code));
    }
  }
});

test("generated broker arguments stay bounded across every capability family", () => {
  const hosts = [
    ["find", "entity_read", 1],
    ["set_var", "entity_write_runtime", 2],
    ["axis", "input_read", 1],
    ["hud_show", "hud_action", 2],
    ["load_level", "level_travel", 1],
    ["play_sound", "audio_event", 1],
    ["time", "deterministic_timer", 0],
  ];
  const values = [null, true, false, -7, 3.5, "player", "odd:value"];
  let state = 0xbadc0de;
  const next = () => {
    state = (Math.imul(state, 1103515245) + 12345) >>> 0;
    return state;
  };
  for (let caseIndex = 0; caseIndex < 256; caseIndex += 1) {
    const [host, capability, arity] = hosts[caseIndex % hosts.length];
    const numbers = [];
    const strings = [];
    const code = [];
    for (let index = 0; index < arity; index += 1) {
      const value = values[next() % values.length];
      if (value === null) code.push({ op: "push_unit", a: 0, b: 0, line: 1 });
      else if (typeof value === "boolean") code.push({ op: "push_bool", a: value ? 1 : 0, b: 0, line: 1 });
      else if (typeof value === "number") {
        numbers.push(value);
        code.push({ op: "push_num", a: numbers.length - 1, b: 0, line: 1 });
      } else {
        strings.push(value);
        code.push({ op: "push_str", a: strings.length - 1, b: 0, line: 1 });
      }
    }
    code.push(
      { op: "call_host", a: 0, b: arity, line: 1 },
      { op: "pop", a: 0, b: 0, line: 1 },
      { op: "push_unit", a: 0, b: 0, line: 1 },
      { op: "return", a: 0, b: 0, line: 1 },
    );
    const program = baseProgram({ code, numbers, strings, hosts: [host] });
    const response = runOneFrame(`broker-${caseIndex}`, program, [capability]);
    assert.equal(response.payload.kind, "frame", `${host} case ${caseIndex}`);
  }
});
