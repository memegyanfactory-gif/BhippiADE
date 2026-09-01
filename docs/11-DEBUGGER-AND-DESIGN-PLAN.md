# 11 · `/debug` rebuilt, and the Bhippi Design System

Status: **delivered** · Written 2026-08-27
Authority: below `00-SPEC`, `06-INVARIANTS` and the ADRs.

---

## Part A — `/debug` becomes a real debugger

### What is wrong with it today (all verified in `crates/bhippi-app/src/debugger.rs`)

| # | Defect | Consequence |
|---|---|---|
| A1 | `check_git_conflict_markers` uses a **non-recursive** `read_dir` on the workspace root | On any project with a `src/` directory it scans nothing. It has never found a conflict marker in a real repo. |
| A2 | Stack detection is an **exclusive `if / else if`** | This very repo is Rust **and** TypeScript; `tsc` never runs because `Cargo.toml` matched first. |
| A3 | `tsc` is only looked for at the **workspace root** | This repo's `tsconfig.json` is in `ui/`. Never found. |
| A4 | A flat **15-second** timeout covers `cargo check --workspace --all-targets` | On a cold target dir that always times out, so the tool reports a timeout instead of diagnostics. |
| A5 | Runs `cargo check` only, never `clippy` | Every lint the project actually gates on is invisible. |
| A6 | **No analysis of its own** — it is a thin wrapper over three compilers | A compiler finds what does not compile. It does not find a committed secret, a disabled test suite, or a swallowed error. |

### What replaces it

`debugger.rs` becomes a module directory:

```
debugger/mod.rs         orchestration, report types, severity roll-up
debugger/walk.rs        one recursive walker with real ignore rules + budgets
debugger/rules.rs       the hardcoded rule set — pure, per-line, unit-tested
debugger/toolchain.rs   cargo / clippy / tsc / python runners
```

**The walker** — recursive, skipping `node_modules · target · .git · dist · build · out ·
.next · vendor · .venv · __pycache__ · coverage`, capping at 4 000 files and 1 MB per file,
and skipping anything binary. Every budget is stated in the report so a truncated scan is
never silently reported as a clean one.

**The rules** — deterministic, zero LLM calls, each one chosen because it finds a bug a
compiler cannot. Grouped by what they mean, not by which language they apply to:

*Correctness*
- Unresolved git conflict markers (recursive, which is the fix for A1)
- A **broken relative import** — the file the import names does not exist on disk
- `.only(` left in a test file — silently disables the entire rest of the suite
- A React list render with **no `key` prop**
- Duplicate keys in one object literal
- An **empty `catch` block** — an error deliberately swallowed
- `==` / `!=` against `null`, `0`, or `""` in TS/JS

*Security*
- Hardcoded credentials: AWS keys, GitHub/Slack/Stripe tokens, private-key headers, and
  long high-entropy string literals assigned to a secret-shaped name
- `eval(` and `new Function(` on non-literal input
- `innerHTML =` and `dangerouslySetInnerHTML`
- A `.env` file that is **not** covered by `.gitignore`
- Shell execution built by string interpolation

*Reliability*
- `unwrap()` / `expect()` outside `#[cfg(test)]` (this repo's own hard rule)
- `todo!()` / `unimplemented!()` / `panic!()` shipped in non-test code
- A `.then(` chain with no `.catch(`
- `async` functions whose rejection is never handled

*Hygiene*
- `console.log` / `debugger` / `dbg!` left behind
- `TODO` / `FIXME` / `HACK` / `XXX`
- Files past 800 lines
- Case-only filename collisions (breaks a case-sensitive checkout)

Every rule carries an id (`BHP-D001…`), a severity, a one-line explanation of **why it is a
bug**, and a concrete fix. Rules are pure functions over `(path, line, text)` so each one is
pinned by its own test with a positive *and* a negative case — a rule with no negative case
is how a linter earns its false-positive reputation.

**The toolchain runners** — every detected stack runs, not the first one (A2). `tsconfig.json`
is discovered anywhere in the tree, not only at the root (A3). Timeouts are per-tool and
generous where they must be — 180 s for `cargo`, 90 s for `tsc` (A4) — and `clippy` runs
alongside `check` when the toolchain has it (A5).

---

## Part B — The Bhippi Design System, and the toggle that turns it on

### B1 · The system itself

A real, written design system in `docs/DESIGN-SYSTEM.md`, structured the way a working one
is rather than as a colour list:

- **Foundations** — the neutral ramp, the single accent, the type scale, the 4px spacing
  grid, radii, elevation, and the motion contract from ADR-0016.
- **Colour** — how a palette is built (one accent, one warm/cool neutral ramp, semantic
  colours that are never decorative), with the contrast floors each pairing must clear.
- **Components** — buttons (5 variants × 4 states), inputs, cards, menus, tables, chips,
  dialogs, empty states, toasts: each with anatomy, sizes, and the rule for when to use it.
- **Layout** — the shell, density, responsive breakpoints, and how panels split.
- **Motion** — which of the named animations applies to which kind of change.
- **Composition rules** — the ten judgements that decide whether a screen looks designed or
  assembled, which is what actually separates the reference templates from a component dump.

**React Bits** (`reactbits.dev`, 165+ animated components: text animations, backgrounds,
animations, components; copy-paste, prop-customisable, no runtime dependency) is referenced
as the sanctioned source for decorative motion, with a rule for when reaching for it is
right and when it is noise.

### B2 · The toggle

A **Bhippi Design** switch inside the effort drop-up, beside the speed rail.

Mechanically it works exactly the way `Effort` already does — a flag on the turn that appends
a directive to the system prompt — because that is the seam the engine already has:

- `chat::Effort` gains a sibling `DesignMode` flag on `TurnPlan`
- `send_chat_message` / `regenerate_last_answer` take `design: Option<bool>`
- When on, the design system's condensed directive is appended after the effort directive,
  so any UI the agent writes follows the system instead of inventing a look per file
- The choice is remembered per provider, like effort already is

---

## Definition of done

`cargo fmt --check` · `cargo clippy --all-targets -D warnings` · `cargo test` ·
`tsc --noEmit` · `vite build` all clean; `/debug` run against this repository finds real
findings in both the Rust and the TypeScript halves; the toggle visibly changes the prompt.
