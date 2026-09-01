# 10 · Polish plan — provider reliability, real streaming, agent motion, UI/theme pass

Status: **Phases 1–4 delivered** · Written 2026-08-27 · Decisions recorded in ADR-0016
Authority: below `00-SPEC`, `06-INVARIANTS` and the ADRs. Architecture decisions taken here
are recorded in **ADR-0016** (CLI streaming + typed faults + agent phase vocabulary).

---

## 0 · The four complaints, and what actually causes each

| # | Reported | Root cause found in the tree |
|---|---|---|
| C1 | "context / weekly limit reached gives no specific error" | `cli.rs::hint_for` matches only 401/402/429 keywords. **Context-window overflow is not matched at all**, and Claude's 5-hour vs weekly limits both collapse into one generic "rate-limited". Worse: a CLI that exits **0** while reporting `is_error: true` in its JSON is read as a successful empty answer, so the user sees "the CLI answered with nothing". |
| C2 | "Claude takes too much time" | **`cli.rs::complete` calls `command.output()`** — it blocks until the vendor process fully exits, then fakes a stream by chunking the finished text. Nothing renders until the whole turn is done. The `--output-format json` in the catalogue makes Claude buffer too. Compounded by a flat 180 s wall-clock kill that fires on long-but-healthy runs. |
| C3 | "auto-update the provider CLI" | `silent_update_sweep` exists but is fire-and-forget once per 24 h, reinstalls unconditionally (slow), never runs when a turn actually fails on a stale CLI, and shows the user nothing. |
| C4 | "animations / chat bar / UI / themes are not good" | Motion is ad-hoc per component; there is no shared motion vocabulary. The engine emits one free-text `label` string, so the UI cannot render a phase-specific state. |

---

## Phase 1 — Provider reliability and speed (Rust)

### 1.1 `bhippi-providers/src/transcript.rs` — incremental reader
Replace the whole-buffer `read()` with a stateful `Reader` that consumes **one line at a
time** and yields events, so a live process can be rendered as it speaks.

- `Reader::push_line(&str) -> Vec<TranscriptEvent>`
- `TranscriptEvent::{Text, Thought, Tool, Usage, Failure, Phase}`
- Detect vendor-reported failure on a **successful exit**: Claude `is_error: true`,
  `subtype != "success"`; Codex `{"type":"error"}`; OpenCode error parts.
- Deduplicate Claude's three overlapping text sources (partial `stream_event` deltas,
  whole `assistant` content blocks, final `result` string) — first source wins per turn.
- `read()` is kept as a thin wrapper so every existing fixture test still pins the shapes.

### 1.2 `bhippi-providers/src/fault.rs` — new, typed failure classification
Pure `classify(text) -> FaultKind` + `advise(spec, fault) -> Advice { title, detail, fix, action }`.

`FaultKind`: `ContextExceeded · RateLimited5h · RateLimitedWeekly · QuotaExhausted ·
Unauthenticated · NotInstalled · Outdated · Timeout · Network · Crashed · EmptyAnswer · Unknown`

Each carries the concrete next action, and where a machine can fix it (`/compact` for
context, update for `Outdated`, provider switch for a limit) it says so as a **button**, not prose.

### 1.3 `bhippi-providers/src/cli.rs` — real streaming
- `spawn()` with piped stdout/stderr; a reader task feeds an `mpsc` channel; `complete()`
  returns a stream over the receiver. First token reaches the UI in ~1 s instead of at the end.
- **Idle** timeout (no output for N s) replaces the wall-clock kill, plus a generous hard cap.
- Child is killed on cancel/drop — no orphaned vendor processes.
- `Capabilities::streaming = true`; real per-provider `context_window`.
- stderr is captured concurrently and only used to explain a failure.

### 1.4 `bhippi-providers/src/catalog.rs` — flags that actually stream, and start faster
- Claude: `--output-format stream-json --verbose --include-partial-messages` (token-level
  streaming) and `--strict-mcp-config` (skips project MCP server boot — measurable startup win).
- OpenCode: `--thinking` so reasoning is visible.
- Add `context_window` per spec entry so the pre-send budget guard has a number to check.

### 1.5 `bhippi-app/src/chat.rs` — typed fault to the UI + a pre-send guard
- `TurnFault { kind, title, detail, fix, provider, retry_after_s, resets_at, action }`
  added to `ChatTurnDone` **and** `ChatTurnView` (the string `error` stays for compatibility).
- Before sending, estimate prompt tokens against the provider's context window; if it would
  overflow, fail fast with `ContextExceeded` and an offered `/compact` — never spend the call.

### 1.6 Auto-update, made useful
- Check the installed version against the registry **before** reinstalling (npm view), so the
  24 h sweep is a no-op when current.
- Repair-on-failure: a `NotInstalled`/`Outdated`/`Crashed` fault offers "Update now",
  and one automatic attempt runs when auto-update is on.
- `update_provider` IPC command + progress on the existing `ProviderInstallProgress` event.

---

## Phase 2 — Agent phase vocabulary (Rust + TS)

The engine currently emits one free-text label. Replace with a typed `AgentPhase` so the UI
can animate the actual state. Derived from the vendor's own stream events.

`connecting · queued · thinking · reasoning · planning · searching · reading · writing ·
editing · refactoring · running · testing · building · debugging · installing · fetching ·
browsing · analyzing · summarizing · reviewing · waiting_permission · compacting ·
retrying · finalizing · streaming · done · failed · stopped`

---

## Phase 3 — Chat surface rebuild (TS + CSS)

- `components/AgentPhase.tsx` — one component, one animation per phase, shared timing.
- Composer bar: real icons for attach / provider / model / effort / skills / commands / mic
  / stop, with tooltips and keyboard hints.
- `components/FaultCard.tsx` — renders `TurnFault` with its action button.
- Turn rows, streaming caret, tool tree, queue cards: consistent enter/exit motion.

## Phase 4 — Motion system and themes

- `styles/motion.css` — duration/easing/stagger tokens and a named, reusable keyframe library.
- Screen and modal transitions become slides with direction, not opacity fades.
- Theme pass: elevation and shadow tokens per theme, contrast fixes, and the accent applied
  consistently across gauge, chart and chrome boundaries.

---

## Definition of done

`cargo fmt --check` · `cargo clippy -D warnings` · `cargo test` · `tsc --noEmit` ·
`vite build` all clean, and the first token of a Claude answer visibly renders in ~1 s.
