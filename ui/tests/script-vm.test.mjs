/**
 * The script VM (ENG-176, ADR-0030).
 *
 * Programs here are the exact shape `bhippi-engine::script::compile` emits — hand-written
 * bytecode rather than parsed source, because the whole point of ADR-0030 is that this file
 * never parses anything. Keeping the fixtures literal is also what would catch the compiler
 * and the VM drifting apart.
 */

import assert from "node:assert/strict";
import test from "node:test";

import { ScriptVm, makeRandom, renderValue, truthy } from "../src/engine/scriptVm.ts";

const program = (overrides) => ({
  file: "assets/scripts/test.rhai",
  code: [],
  numbers: [],
  strings: [],
  functions: [],
  hosts: [],
  hooks: [],
  step_budget: 10_000,
  call_depth: 8,
  ...overrides,
});

const instr = (op, a = 0, b = 0, line = 1) => ({ op, a, b, line });

test("a hook runs its instructions and reaches the host by name", () => {
  const seen = [];
  const vm = new ScriptVm(
    program({
      numbers: [42],
      strings: ["game.score"],
      hosts: ["set_var"],
      functions: [{ name: "on_start", entry: 0, params: 0, locals: 0, line: 1 }],
      hooks: [{ hook: "on_start", function: 0 }],
      code: [
        instr("push_str", 0),
        instr("push_num", 0),
        instr("call_host", 0, 2),
        instr("pop"),
        instr("push_unit"),
        instr("return"),
      ],
    }),
    { set_var: (args) => void seen.push(args) ?? null },
  );

  assert.equal(vm.run("on_start"), null);
  assert.deepEqual(seen, [["game.score", 42]]);
});

test("arithmetic, comparison and locals evaluate as the compiler intends", () => {
  let captured = null;
  const vm = new ScriptVm(
    program({
      numbers: [3, 4],
      hosts: ["log"],
      functions: [{ name: "on_start", entry: 0, params: 0, locals: 1, line: 1 }],
      hooks: [{ hook: "on_start", function: 0 }],
      code: [
        instr("push_num", 0), // 3
        instr("push_num", 1), // 4
        instr("mul"), // 12
        instr("store", 0),
        instr("load", 0),
        instr("push_num", 1), // 4
        instr("sub"), // 8
        instr("call_host", 0, 1),
        instr("pop"),
        instr("push_unit"),
        instr("return"),
      ],
    }),
    { log: (args) => void (captured = args[0]) ?? null },
  );

  assert.equal(vm.run("on_start"), null);
  assert.equal(captured, 8);
});

test("a user function call returns through its frame", () => {
  let captured = null;
  const vm = new ScriptVm(
    program({
      numbers: [5, 2],
      hosts: ["log"],
      functions: [
        { name: "on_start", entry: 0, params: 0, locals: 0, line: 1 },
        { name: "double", entry: 6, params: 1, locals: 1, line: 4 },
      ],
      hooks: [{ hook: "on_start", function: 0 }],
      code: [
        instr("push_num", 0), // 5
        instr("call_user", 1, 1),
        instr("call_host", 0, 1),
        instr("pop"),
        instr("push_unit"),
        instr("return"),
        // double(v) { return v * 2; }
        instr("load", 0, 0, 4),
        instr("push_num", 1, 0, 4),
        instr("mul", 0, 0, 4),
        instr("return", 0, 0, 4),
      ],
    }),
    { log: (args) => void (captured = args[0]) ?? null },
  );

  assert.equal(vm.run("on_start"), null);
  assert.equal(captured, 10);
});

test("a runaway loop is a located fault, not a frozen pane", () => {
  const vm = new ScriptVm(
    program({
      step_budget: 500,
      functions: [{ name: "on_update", entry: 0, params: 1, locals: 1, line: 7 }],
      hooks: [{ hook: "on_update", function: 0 }],
      // while true { }
      code: [instr("push_bool", 1, 0, 7), instr("jump_if_false", 3, 0, 7), instr("jump", 0, 0, 7), instr("push_unit", 0, 0, 9), instr("return", 0, 0, 9)],
    }),
    {},
  );

  const fault = vm.run("on_update", [0.016]);
  assert.ok(fault, "the budget must produce a fault");
  assert.equal(fault.line, 7);
  assert.match(fault.message, /budget/);
  assert.match(fault.hint, /while/);
});

test("recursion past the call depth is caught with the call site's line", () => {
  const vm = new ScriptVm(
    program({
      call_depth: 4,
      functions: [{ name: "on_start", entry: 0, params: 0, locals: 0, line: 1 }],
      hooks: [{ hook: "on_start", function: 0 }],
      code: [instr("call_user", 0, 0, 12), instr("return", 0, 0, 12)],
    }),
    {},
  );

  const fault = vm.run("on_start");
  assert.ok(fault);
  assert.equal(fault.line, 12);
  assert.match(fault.message, /nested/);
});

test("dividing by zero names the line rather than yielding Infinity", () => {
  const vm = new ScriptVm(
    program({
      numbers: [1, 0],
      functions: [{ name: "on_start", entry: 0, params: 0, locals: 0, line: 1 }],
      hooks: [{ hook: "on_start", function: 0 }],
      code: [instr("push_num", 0, 0, 3), instr("push_num", 1, 0, 3), instr("div", 0, 0, 3), instr("return", 0, 0, 3)],
    }),
    {},
  );

  const fault = vm.run("on_start");
  assert.ok(fault);
  assert.equal(fault.line, 3);
  assert.equal(fault.instruction, 2);
  assert.match(fault.message, /divides by zero/);
});

test("an unbound host is reported before it is called, and fails loudly if it is", () => {
  const vm = new ScriptVm(
    program({
      hosts: ["teleport_everything"],
      functions: [{ name: "on_start", entry: 0, params: 0, locals: 0, line: 1 }],
      hooks: [{ hook: "on_start", function: 0 }],
      code: [instr("call_host", 0, 0, 2), instr("return", 0, 0, 2)],
    }),
    {},
  );

  assert.deepEqual(vm.unboundHosts(), ["teleport_everything"]);
  const fault = vm.run("on_start");
  assert.ok(fault);
  assert.match(fault.message, /teleport_everything/);
});

test("short-circuit jumps leave the deciding value on the stack", () => {
  let called = 0;
  const vm = new ScriptVm(
    program({
      hosts: ["side_effect"],
      functions: [{ name: "on_start", entry: 0, params: 0, locals: 0, line: 1 }],
      hooks: [{ hook: "on_start", function: 0 }],
      // false && side_effect()
      code: [
        instr("push_bool", 0),
        instr("jump_if_false_peek", 4),
        instr("pop"),
        instr("call_host", 0, 0),
        instr("return"),
      ],
    }),
    { side_effect: () => void (called += 1) ?? true },
  );

  assert.equal(vm.run("on_start"), null);
  assert.equal(called, 0, "the right-hand side must not run");
});

test("a hook the program does not define is a no-op, not an error", () => {
  const vm = new ScriptVm(program({ hooks: [] }), {});
  assert.equal(vm.hasHook("on_update"), false);
  assert.equal(vm.run("on_update", [0.016]), null);
});

test("random() is seeded, so a play session replays identically", () => {
  const first = makeRandom(7);
  const second = makeRandom(7);
  const sequence = [first(), first(), first()];
  assert.deepEqual(sequence, [second(), second(), second()]);
  assert.ok(sequence.every((value) => value >= 0 && value < 1));
  assert.notDeepEqual(sequence, [makeRandom(8)(), makeRandom(8)(), makeRandom(8)()]);
});

test("truthiness and rendering follow the documented value model", () => {
  assert.equal(truthy(0), false);
  assert.equal(truthy(""), false);
  assert.equal(truthy(null), false);
  assert.equal(truthy("no"), true);
  assert.equal(renderValue(3), "3");
  assert.equal(renderValue(1 / 3), "0.333");
  assert.equal(renderValue(null), "");
});

test("string concatenation works, because the subset has no interpolation", () => {
  let captured = null;
  const vm = new ScriptVm(
    program({
      numbers: [7],
      strings: ["score: "],
      hosts: ["log"],
      functions: [{ name: "on_start", entry: 0, params: 0, locals: 0, line: 1 }],
      hooks: [{ hook: "on_start", function: 0 }],
      code: [
        instr("push_str", 0),
        instr("push_num", 0),
        instr("add"),
        instr("call_host", 0, 1),
        instr("pop"),
        instr("push_unit"),
        instr("return"),
      ],
    }),
    { log: (args) => void (captured = args[0]) ?? null },
  );

  assert.equal(vm.run("on_start"), null);
  assert.equal(captured, "score: 7");
});
