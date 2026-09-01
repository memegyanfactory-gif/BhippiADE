# ADR-0030 — Gameplay scripts: a Rust compiler and a webview VM

- **Status:** Accepted
- **Date:** 2026-09-01
- **Relates to:** ADR-0028 (the webview is the renderer and the play runtime), ADR-0020,
  INV-073, `docs/13-ENGINE-AI-CONTROL-AND-UNREAL-UX-PLAN.md` ENG-176

## Context

`ENG-176` asks for a script runtime — Rhai scripts with `on_start`, `on_update(dt)`,
`on_collision(other)` and `on_trigger(other)`, a narrow host API, and errors that surface as
typed faults with file and line. The `ScriptRef` component and `AssetKind::Script` (`.rhai`,
`.rs`) have existed since the schema was written; nothing has ever executed them.

Two decisions collide:

- **ADR-0028** put the play runtime **in the webview**, beside the scene it simulates. The
  simulation ticks at frame rate; nothing on that path may cross an IPC boundary.
- **INV-073** (as narrowed by ADR-0028) keeps parsing, validation and every document-shaped
  computation in `bhippi-engine`. A script is a document: it is authored, hand-edited,
  AI-generated, and must be rejected with a span when it is wrong.

Rhai is a Rust crate. Embedding it means running the interpreter in Rust — which is where the
world is *not*. Calling into Rust for every `on_update` of every scripted entity, sixty times
a second, is the JSON-RPC-on-the-hot-path cost ADR-0028 rejected for rendering, and it would
be worse here because a script reads and writes runtime state on every call.

The three real options:

1. **Rhai in Rust, ticked over IPC.** Correct language, wrong place. One IPC round trip per
   scripted entity per frame.
2. **A JavaScript-shaped script language executed by the webview directly.** Cheapest to
   build and the worst outcome: `eval` in the pane, no sandbox, no spans, no validation, and
   business logic in TypeScript by construction.
3. **Compile in Rust, execute in the webview.**

## Decision

**Scripts are compiled in `bhippi-engine` and executed by a small stack VM in the webview.**

- `crates/bhippi-engine/src/script.rs` owns a lexer, a parser and a compiler for a
  **documented subset of Rhai syntax**. Files keep the `.rhai` extension because what they
  contain *is* Rhai source — the subset is a restriction, never a dialect. Anything outside
  the subset is a compile error naming the construct, the line, the column and what to write
  instead.
- The compiler emits `ScriptProgram`: a flat instruction array, a string table, a function
  table and a per-instruction line map. It is `serde`/`specta`-typed, so it crosses to the
  webview once, when play starts.
- `ui/src/engine/scriptVm.ts` is a stack machine that executes that program. **It does no
  parsing.** It has no `eval`, no `Function`, no access to the DOM, and no host call it was
  not handed. A runtime fault carries the instruction's line straight back out.
- The host surface is a fixed, enumerated list (`HostFn`), defined once in Rust. The compiler
  rejects an unknown call at compile time with the arity and the nearest name; the VM can
  therefore assume every `CallHost` is valid.

### Why a subset, not a new language

Because the extension already says `.rhai`, the schema already says `.rhai`, and the AI
already knows Rhai. A subset of a real language keeps every one of those true, keeps editor
syntax highlighting working, and leaves the door open to executing the same file with the
real Rhai crate later (a headless AI playtest in Rust, say) without a rewrite.

### The subset

`fn` declarations · `let` · assignment · `if` / `else if` / `else` · `while` · `return` ·
`break` / `continue` · the arithmetic, comparison and boolean operators (`&&` and `||`
short-circuit) · number, string and boolean literals · calls to host functions and to
functions declared in the same file · `//` and `/* */` comments.

Deliberately excluded, each rejected by name: closures, arrays and maps, objects and methods,
`import`, `for`, `switch`, string interpolation, and every Rhai standard-library function that
is not on the host list. The exclusions are what make the VM small enough to trust.

### Determinism and safety

- Values are `number | string | bool | unit`. No references, no allocation the VM does not
  own.
- Every hook runs under a **step budget** (`SCRIPT_STEP_BUDGET`). Exceeding it is a typed
  fault — `script.budget` — not a hung pane. This is what makes `while true {}` in an
  AI-written script a red line in the Output Log instead of a frozen editor.
- Call depth is capped; recursion past it is a fault with the call site's line.
- Scripts read and write the runtime world only through host functions, so INV-081 holds by
  construction: there is no path from a script to an authored scene file.

## Consequences

- New module `bhippi-engine::script` (compiler, `ScriptProgram`, `ScriptFault`), new
  `AssetKind::Script` validation in the content gate, new `engine_compile_scripts` command.
- New `INV-082`: a gameplay script is compiled in Rust before it runs; the webview never
  parses script source and never calls `eval`/`Function`. Enforced by the architecture test's
  grep over `ui/src/engine/`.
- `ScriptRef.hooks` stops being free-form JSON in practice: the compiler reports which of the
  four hooks a file actually defines, and the Details panel shows them.
- A script that fails to compile **blocks Play for that entity, not the whole game**: the
  entity runs unscripted, the fault lands in the Output Log with file and line, and the
  transport bar shows the count. A game that will not start because one prop's script has a
  typo is worse than a game that starts with a loud, located error.

## Alternatives rejected

- **Rhai in Rust over IPC per frame** — see above; the hot path is the whole objection.
- **`eval` in the webview** — no spans, no sandbox, no validation, and it puts the language
  semantics in TypeScript where INV-073 says they must not be.
- **WASM-compiled Rhai** — a real option, and a large dependency, for a language whose 5% we
  need. Revisit if the subset stops being enough; the `.rhai` extension is what keeps that
  door open.

## Reversal

If the subset becomes the limit, the compiler is replaced by the Rhai crate compiled to WASM
and loaded by the same `scriptVm.ts` seam. `ScriptProgram` is the interface either way, and
the host function list — the part that actually touches the game — does not change.
