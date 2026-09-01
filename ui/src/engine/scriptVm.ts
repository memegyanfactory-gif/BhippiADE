/**
 * The gameplay script VM (ENG-176, ADR-0030).
 *
 * It executes a `ScriptProgram` that `bhippi-engine::script` already lexed, parsed,
 * validated and compiled. **Nothing here parses script source**, and there is no `eval` and
 * no `Function` — the only things this file can do are the ones the compiler emitted
 * (INV-082). Host calls bind by name against a table the caller supplies, so adding a host
 * function in Rust cannot silently repoint an existing call.
 *
 * Faults are returned, never thrown: a script that divides by a string should put a located
 * red line in the Output Log, not take the play session down with it.
 */

export type ScriptValue = number | string | boolean | null;

export type ScriptInstr = {
  op: string;
  a: number;
  b: number;
  line: number;
};

export type ScriptFunctionInfo = {
  name: string;
  entry: number;
  params: number;
  locals: number;
  line: number;
};

export type ScriptHookName = "on_start" | "on_update" | "on_collision" | "on_trigger";

export type ScriptProgram = {
  file: string;
  code: ScriptInstr[];
  numbers: number[];
  strings: string[];
  functions: ScriptFunctionInfo[];
  hosts: string[];
  hooks: { hook: ScriptHookName; function: number }[];
  step_budget: number;
  call_depth: number;
};

/** A located runtime failure. Compile faults use the same shape, from Rust. */
export type ScriptFault = {
  file: string;
  line: number;
  message: string;
  hint?: string;
};

/** One host implementation: it receives already-evaluated arguments. */
export type ScriptHostFn = (args: ScriptValue[]) => ScriptValue;

/** The host surface, keyed by the names the compiler recorded in `program.hosts`. */
export type ScriptHostTable = Record<string, ScriptHostFn>;

// Numeric opcodes: the wire form is `snake_case` strings, which are pleasant to read in a
// dump and far too slow to switch on 200 000 times a frame. Interned once, at construction.
const OPS = [
  "push_num",
  "push_str",
  "push_bool",
  "push_unit",
  "load",
  "store",
  "pop",
  "add",
  "sub",
  "mul",
  "div",
  "rem",
  "neg",
  "not",
  "eq",
  "ne",
  "lt",
  "le",
  "gt",
  "ge",
  "jump",
  "jump_if_false",
  "jump_if_false_peek",
  "jump_if_true_peek",
  "call_host",
  "call_user",
  "return",
] as const;

const OP_INDEX = new Map<string, number>(OPS.map((name, index) => [name, index]));
const HOOK_NAMES = new Set<ScriptHookName>([
  "on_start",
  "on_update",
  "on_collision",
  "on_trigger",
]);

/** Validate the untrusted wire shape before it can allocate a VM or execute an instruction. */
export function validateScriptProgram(value: unknown): string | null {
  if (!value || typeof value !== "object") return "program is not an object";
  const program = value as ScriptProgram;
  if (typeof program.file !== "string" || program.file.trim() === "") return "file id is invalid";
  if (!Array.isArray(program.code)) return "code is not an array";
  if (!Array.isArray(program.numbers) || program.numbers.some((item) => !Number.isFinite(item))) {
    return "number pool contains a non-finite value";
  }
  if (!Array.isArray(program.strings) || program.strings.some((item) => typeof item !== "string")) {
    return "string pool is invalid";
  }
  if (!Array.isArray(program.hosts) || program.hosts.some((item) => typeof item !== "string" || item === "")) {
    return "host table is invalid";
  }
  if (!Array.isArray(program.functions)) return "function table is not an array";
  if (!Array.isArray(program.hooks)) return "hook table is not an array";
  if (!positiveInteger(program.step_budget) || !positiveInteger(program.call_depth)) {
    return "program budgets are invalid";
  }

  for (const [index, item] of program.functions.entries()) {
    if (!item || typeof item !== "object" || typeof item.name !== "string") {
      return `function ${index} is invalid`;
    }
    if (
      !nonNegativeInteger(item.entry) ||
      item.entry >= program.code.length ||
      !nonNegativeInteger(item.params) ||
      !nonNegativeInteger(item.locals) ||
      item.params > item.locals ||
      !nonNegativeInteger(item.line)
    ) {
      return `function ${index} has invalid bounds`;
    }
  }

  const hooks = new Set<ScriptHookName>();
  for (const [index, item] of program.hooks.entries()) {
    if (
      !item ||
      typeof item !== "object" ||
      !HOOK_NAMES.has(item.hook) ||
      !nonNegativeInteger(item.function) ||
      item.function >= program.functions.length ||
      hooks.has(item.hook)
    ) {
      return `hook ${index} is invalid`;
    }
    hooks.add(item.hook);
  }

  for (const [index, item] of program.code.entries()) {
    if (
      !item ||
      typeof item !== "object" ||
      !OP_INDEX.has(item.op) ||
      !Number.isSafeInteger(item.a) ||
      !Number.isSafeInteger(item.b) ||
      !nonNegativeInteger(item.line)
    ) {
      return `instruction ${index} is invalid`;
    }
    if (item.op === "push_num" && !poolIndex(item.a, program.numbers.length)) {
      return `instruction ${index} has an invalid number index`;
    }
    if (item.op === "push_str" && !poolIndex(item.a, program.strings.length)) {
      return `instruction ${index} has an invalid string index`;
    }
    if (item.op === "push_bool" && item.a !== 0 && item.a !== 1) {
      return `instruction ${index} has an invalid boolean`;
    }
    if (["load", "store"].includes(item.op) && !nonNegativeInteger(item.a)) {
      return `instruction ${index} has an invalid local index`;
    }
    if (item.op.startsWith("jump") && !poolIndex(item.a, program.code.length)) {
      return `instruction ${index} has an invalid jump target`;
    }
    if (
      item.op === "call_host" &&
      (!poolIndex(item.a, program.hosts.length) || !nonNegativeInteger(item.b))
    ) {
      return `instruction ${index} has an invalid host call`;
    }
    if (item.op === "call_user" && !poolIndex(item.a, program.functions.length)) {
      return `instruction ${index} has an invalid function call`;
    }
  }
  return null;
}

const nonNegativeInteger = (value: number): boolean => Number.isSafeInteger(value) && value >= 0;
const positiveInteger = (value: number): boolean => Number.isSafeInteger(value) && value > 0;
const poolIndex = (value: number, length: number): boolean => nonNegativeInteger(value) && value < length;

const PUSH_NUM = 0;
const PUSH_STR = 1;
const PUSH_BOOL = 2;
const PUSH_UNIT = 3;
const LOAD = 4;
const STORE = 5;
const POP = 6;
const ADD = 7;
const SUB = 8;
const MUL = 9;
const DIV = 10;
const REM = 11;
const NEG = 12;
const NOT = 13;
const EQ = 14;
const NE = 15;
const LT = 16;
const LE = 17;
const GT = 18;
const GE = 19;
const JUMP = 20;
const JUMP_IF_FALSE = 21;
const JUMP_IF_FALSE_PEEK = 22;
const JUMP_IF_TRUE_PEEK = 23;
const CALL_HOST = 24;
const CALL_USER = 25;
const RETURN = 26;

/** Rhai's truthiness, narrowed to the subset's four value kinds. */
export const truthy = (value: ScriptValue): boolean => {
  if (typeof value === "boolean") return value;
  if (typeof value === "number") return value !== 0 && !Number.isNaN(value);
  if (typeof value === "string") return value.length > 0;
  return false;
};

/** How a value renders in `to_string`, a log line or a HUD binding. */
export const renderValue = (value: ScriptValue): string => {
  if (value === null) return "";
  if (typeof value === "number") return Number.isInteger(value) ? String(value) : value.toFixed(3);
  return String(value);
};

class Halt extends Error {
  readonly fault: ScriptFault;

  constructor(fault: ScriptFault) {
    super(fault.message);
    this.fault = fault;
  }
}

export class ScriptVm {
  readonly program: ScriptProgram;
  private readonly ops: Int32Array;
  private readonly bound: (ScriptHostFn | null)[];
  private readonly hookEntry = new Map<ScriptHookName, number>();
  private readonly missing: string[];
  private executedInstructions = 0;

  constructor(program: ScriptProgram, hosts: ScriptHostTable) {
    this.program = program;
    this.ops = new Int32Array(program.code.length);
    for (let index = 0; index < program.code.length; index += 1) {
      this.ops[index] = OP_INDEX.get(program.code[index].op) ?? -1;
    }
    this.missing = program.hosts.filter((name) => typeof hosts[name] !== "function");
    this.bound = program.hosts.map((name) => hosts[name] ?? null);
    for (const entry of program.hooks) this.hookEntry.set(entry.hook, entry.function);
  }

  /**
   * Host names the compiler emitted that this VM was given no implementation for. A
   * non-empty list is a wiring bug in the pane, not in the user's script — surfaced rather
   * than discovered as a null call at frame 1.
   */
  unboundHosts(): string[] {
    return [...this.missing];
  }

  hasHook(hook: ScriptHookName): boolean {
    return this.hookEntry.has(hook);
  }

  hooks(): ScriptHookName[] {
    return this.program.hooks.map((entry) => entry.hook);
  }

  instructionCount(): number {
    return this.executedInstructions;
  }

  /** Run one hook. Returns its fault, or null when it completed. */
  run(hook: ScriptHookName, args: ScriptValue[] = []): ScriptFault | null {
    const target = this.hookEntry.get(hook);
    if (target === undefined) return null;
    try {
      this.execute(target, args);
      return null;
    } catch (error) {
      if (error instanceof Halt) return error.fault;
      const message = error instanceof Error ? error.message : String(error);
      return {
        file: this.program.file,
        line: 0,
        message: `The script runtime failed: ${message}`,
        hint: "This is an engine bug rather than a script one — please report it with the script attached.",
      };
    }
  }

  private fault(line: number, message: string, hint: string): Halt {
    return new Halt({ file: this.program.file, line, message, hint });
  }

  private execute(functionIndex: number, args: ScriptValue[]): ScriptValue {
    const program = this.program;
    const code = program.code;
    const stack: ScriptValue[] = [];
    // One frame: the code index to return to, the base of the caller's locals, and the
    // stack depth to unwind to. Frames are an array rather than JS recursion so a runaway
    // script hits the depth cap instead of the host's stack.
    const frames: { back: number; locals: ScriptValue[] }[] = [];

    const first = program.functions[functionIndex];
    if (!first) {
      throw this.fault(0, "The hook points at a function that was not compiled.", "Recompile the script.");
    }
    let locals: ScriptValue[] = new Array(first.locals).fill(null);
    for (let index = 0; index < first.params; index += 1) {
      locals[index] = args[index] ?? null;
    }
    let pc = first.entry;
    let steps = 0;

    const pop = (line: number): ScriptValue => {
      const value = stack.pop();
      if (value === undefined) {
        throw this.fault(line, "The script's value stack ran dry.", "This is a compiler bug; please report it.");
      }
      return value;
    };

    const asNumber = (value: ScriptValue, line: number, what: string): number => {
      if (typeof value === "number") return value;
      if (typeof value === "boolean") return value ? 1 : 0;
      throw this.fault(
        line,
        `${what} needs a number, and got ${value === null ? "nothing" : `the text "${value}"`}.`,
        "Convert it first, or check the host function you read it from.",
      );
    };

    for (;;) {
      steps += 1;
      this.executedInstructions += 1;
      if (steps > program.step_budget) {
        throw this.fault(
          code[Math.min(pc, code.length - 1)]?.line ?? 0,
          `This script ran past its ${program.step_budget}-step budget without finishing.`,
          "Look for a `while` loop whose condition never becomes false.",
        );
      }
      const instr = code[pc];
      if (!instr) {
        throw this.fault(0, "The script ran off the end of its code.", "This is a compiler bug; please report it.");
      }
      const op = this.ops[pc];
      pc += 1;

      switch (op) {
        case PUSH_NUM:
          stack.push(program.numbers[instr.a] ?? 0);
          break;
        case PUSH_STR:
          stack.push(program.strings[instr.a] ?? "");
          break;
        case PUSH_BOOL:
          stack.push(instr.a === 1);
          break;
        case PUSH_UNIT:
          stack.push(null);
          break;
        case LOAD:
          stack.push(locals[instr.a] ?? null);
          break;
        case STORE:
          locals[instr.a] = pop(instr.line);
          break;
        case POP:
          pop(instr.line);
          break;
        case ADD: {
          const right = pop(instr.line);
          const left = pop(instr.line);
          // `+` concatenates when either side is text, exactly as Rhai does — it is the only
          // way to build a message in a language with no interpolation.
          if (typeof left === "string" || typeof right === "string") {
            stack.push(renderValue(left) + renderValue(right));
          } else {
            stack.push(asNumber(left, instr.line, "`+`") + asNumber(right, instr.line, "`+`"));
          }
          break;
        }
        case SUB: {
          const right = asNumber(pop(instr.line), instr.line, "`-`");
          stack.push(asNumber(pop(instr.line), instr.line, "`-`") - right);
          break;
        }
        case MUL: {
          const right = asNumber(pop(instr.line), instr.line, "`*`");
          stack.push(asNumber(pop(instr.line), instr.line, "`*`") * right);
          break;
        }
        case DIV: {
          const right = asNumber(pop(instr.line), instr.line, "`/`");
          const left = asNumber(pop(instr.line), instr.line, "`/`");
          if (right === 0) {
            throw this.fault(
              instr.line,
              "This divides by zero.",
              "Guard the divisor, e.g. `if d != 0.0 { ... }`.",
            );
          }
          stack.push(left / right);
          break;
        }
        case REM: {
          const right = asNumber(pop(instr.line), instr.line, "`%`");
          const left = asNumber(pop(instr.line), instr.line, "`%`");
          if (right === 0) {
            throw this.fault(instr.line, "This takes a remainder by zero.", "Guard the divisor.");
          }
          stack.push(left % right);
          break;
        }
        case NEG:
          stack.push(-asNumber(pop(instr.line), instr.line, "`-`"));
          break;
        case NOT:
          stack.push(!truthy(pop(instr.line)));
          break;
        case EQ: {
          const right = pop(instr.line);
          stack.push(pop(instr.line) === right);
          break;
        }
        case NE: {
          const right = pop(instr.line);
          stack.push(pop(instr.line) !== right);
          break;
        }
        case LT:
        case LE:
        case GT:
        case GE: {
          const right = pop(instr.line);
          const left = pop(instr.line);
          // Text compares lexicographically, so `name < "m"` means what it looks like.
          if (typeof left === "string" && typeof right === "string") {
            stack.push(
              op === LT ? left < right : op === LE ? left <= right : op === GT ? left > right : left >= right,
            );
            break;
          }
          const l = asNumber(left, instr.line, "a comparison");
          const r = asNumber(right, instr.line, "a comparison");
          stack.push(op === LT ? l < r : op === LE ? l <= r : op === GT ? l > r : l >= r);
          break;
        }
        case JUMP:
          pc = instr.a;
          break;
        case JUMP_IF_FALSE:
          if (!truthy(pop(instr.line))) pc = instr.a;
          break;
        case JUMP_IF_FALSE_PEEK:
          if (!truthy(stack[stack.length - 1] ?? null)) pc = instr.a;
          break;
        case JUMP_IF_TRUE_PEEK:
          if (truthy(stack[stack.length - 1] ?? null)) pc = instr.a;
          break;
        case CALL_HOST: {
          const host = this.bound[instr.a];
          const callArgs: ScriptValue[] = new Array(instr.b);
          for (let index = instr.b - 1; index >= 0; index -= 1) callArgs[index] = pop(instr.line);
          if (!host) {
            throw this.fault(
              instr.line,
              `\`${program.hosts[instr.a] ?? "?"}\` is not available in this runtime.`,
              "The engine did not bind this host function — please report it.",
            );
          }
          const result = host(callArgs);
          stack.push(result === undefined ? null : result);
          break;
        }
        case CALL_USER: {
          if (frames.length + 1 > program.call_depth) {
            throw this.fault(
              instr.line,
              `Calls nested more than ${program.call_depth} deep.`,
              "Check for a function that calls itself without a stopping condition.",
            );
          }
          const callee = program.functions[instr.a];
          if (!callee) {
            throw this.fault(instr.line, "This calls a function that was not compiled.", "Recompile the script.");
          }
          const next: ScriptValue[] = new Array(callee.locals).fill(null);
          for (let index = callee.params - 1; index >= 0; index -= 1) next[index] = pop(instr.line);
          frames.push({ back: pc, locals });
          locals = next;
          pc = callee.entry;
          break;
        }
        case RETURN: {
          const value = pop(instr.line);
          const frame = frames.pop();
          if (!frame) return value;
          locals = frame.locals;
          pc = frame.back;
          stack.push(value);
          break;
        }
        default:
          throw this.fault(
            instr.line,
            `Unknown instruction \`${instr.op}\`.`,
            "The compiled program is newer than this runtime; rebuild the app.",
          );
      }
    }
  }
}

/**
 * A deterministic PRNG for `random()`. Play sessions must replay identically for the AI
 * playtest (ENG-187) to mean anything, so `Math.random` is not an option.
 */
export const makeRandom = (seed: number): (() => number) => {
  let state = seed >>> 0 || 0x9e3779b9;
  return () => {
    state ^= state << 13;
    state >>>= 0;
    state ^= state >>> 17;
    state ^= state << 5;
    state >>>= 0;
    return state / 0x1_0000_0000;
  };
};
