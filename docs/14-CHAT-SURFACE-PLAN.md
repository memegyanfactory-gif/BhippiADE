# 14 — Chat surface: a transcript that shows its work

**Owner goal (stated intent):** the chat should work and look like the Codex desktop
transcript — the agent's activity grouped into collapsible rows ("Ran commands ⌄", "Edited
files, ran commands ⌄"), each one expandable to the *actual* command and its output or the
*actual* diff; a turn that ends with a real change summary ("Edited 20 files +1,198 −213")
carrying **Undo** and **Review**; a "Worked for 13m 42s" line; and detail hidden by default
so the prose stays readable.

**Status:** authored 2026-09-01 · **Phase A complete** · **Phase B complete** (113 partial) · **Phase C not started** · ticket range `CHT-100…CHT-199`

**Authority:** below `docs/00-SPEC-v1.0.md`, `docs/06-INVARIANTS.md`, `docs/01-ARCHITECTURE.md`
and the ADRs. ADR-0006 owns the conversational surface; ADR-0014 owns the Activity Dock.
Anything here that changes those needs an ADR **first**.

---

## 1. Audit — what exists today (verified 2026-09-01, read out of the tree)

| Piece | Where | What it actually does |
|---|---|---|
| Turn model | `chat.rs` `ChatTurnView` | id, role, content, thinking, `thinking_elapsed_ms`, provider, `tools`, permission, fault |
| Step model | `ToolActivity` | **`{ id, action, title, detail, state }`** — five fields, all strings |
| Step emission | `chat.rs::tool_card` / `finish_tool` | opens a card with a title and a detail line; closes it by flipping `state` |
| Transcript | `ui/src/screens/Chat.tsx` (2 619 lines) | `TurnRow` → `TurnWorkTree` → one flat row per tool |
| Work tree | `Chat.tsx::TurnWorkTree` | one always-open group whose header is **always** `Exploring N files` |
| Thinking | `Chat.tsx::ThinkingAccordion` | the only collapsible thing in a turn; shows "Thought for Ns" / "Worked for Nm" |
| Diffs | `components/DiffView.tsx`, `screens/ReviewChangesModal.tsx` | a real unified/split diff renderer and a modal that recomputes the workspace diff on demand |
| Diff data | `bhippi-app::review` | `ReviewSummary { files, total_additions, total_deletions, turn_title }`, `FileDiff { path, additions, deletions, status, hunks }` |

## 2. Findings — six reasons the transcript cannot look like the target

| # | Finding | Evidence | Consequence |
|---|---|---|---|
| **C1** | **A step is a label, not a record.** `ToolActivity` carries no command, no output, no exit code, no timing. | `ToolActivity` has exactly `id/action/title/detail/state` | "Ran commands ⌄" can never expand into anything, because there is nothing behind it. This is the finding the other five depend on. |
| **C2** | **The work tree groups nothing and mislabels everything.** | `TurnWorkTree` header is the literal `Exploring ${fileCount} file(s)` | A turn that edited twelve files and ran four commands says "Exploring 12 files". |
| **C3** | **A turn carries no change summary.** `ReviewChangesModal` recomputes one from the workspace when asked. | `ChatTurnView` has no diff field; `collect_review_changes` runs on demand | No "Edited 20 files +1,198 −213" card, and no per-turn **Undo**, because nothing records what the turn touched. |
| **C4** | **Nothing below the top level discloses.** | `ThinkingAccordion` is the only `useState` disclosure in a turn | Detail is either absent or unbounded; there is no "Show 17 more files". |
| **C5** | **Duration is measured for thinking only.** | `thinking_elapsed_ms`; no turn start/end | "Worked for 13m 42s" for the whole turn cannot be rendered. |
| **C6** | **Notices have no lane.** | faults render as a card; nothing else does | A usage-limit or rate-limit notice has nowhere to go in the transcript. |

**The through-line:** every visual difference from the target is downstream of C1. The
screenshots are not a skin over the current data — they are a *richer record* rendered
plainly. Ship C1 first or the rest is decoration.

---

## 3. Target shape

```
  ┌─ turn ────────────────────────────────────────────────────────────────┐
  │  prose (markdown, unchanged)                                          │
  │                                                                       │
  │  ⌄ Ran commands                          ← one row per consecutive run│
  │      $ cargo test --workspace                 of the same kind        │
  │      ┌───────────────────────────────┐   ← expands to the real thing  │
  │      │ 502 passed; 0 failed          │      (scrolls, capped, copyable)│
  │      └───────────────────────────────┘                                │
  │                                                                       │
  │  ⌄ Edited files, ran commands                                         │
  │      mod.rs                        +32 −7    ← expands to a real diff │
  │                                                                       │
  │  prose                                                                │
  │                                                                       │
  │  Worked for 13m 42s                                                   │
  │  ─────────────────────────────────────────────────────────────────    │
  │  ⚠ You've hit your usage limit.                        ← notice lane  │
  │                                                                       │
  │  ┌─ Edited 20 files          +1,198 −213      [Undo ↺] [Review] ─┐    │
  │  │   crates/bhippi-app/src/engine/mod.rs            +73 −13       │    │
  │  │   crates/bhippi-app/src/engine/session.rs       +108 −33       │    │
  │  │   crates/bhippi-app/src/lib.rs                    +3 −1        │    │
  │  │   ⌄ Show 17 more files                                         │    │
  │  └────────────────────────────────────────────────────────────────┘   │
  └───────────────────────────────────────────────────────────────────────┘
```

Three rules the design has to keep:

1. **Collapsed by default, except the last group of a running turn.** The reason to watch a
   turn live is to see what it is doing *now*; the reason to read a finished one is the prose.
2. **A disclosure never lies about its size.** "Show 17 more files" says seventeen because
   there are seventeen. A row that expands to nothing must not be a row.
3. **Output is bounded in the record, not only in the view.** A command that prints 40 MB is
   truncated where it is captured, with the transcript saying so — not held in memory and
   hidden with CSS.

---

## 4. Phases

### Phase A — the record  ·  `CHT-100…CHT-106`

*Closes C1 and C5. Everything else is blocked on this.*

- [x] **CHT-100** — `ToolActivity` grows a typed result: `command: Option<String>`,
      `output: Option<String>`, `exit_code: Option<i32>`, `started_at`, `finished_at`,
      `truncated: bool`. Existing fields keep their meaning; every new one is optional so a
      step that has nothing to show renders exactly as it does today.
- [x] **CHT-101** — Output capture with a hard cap (`TOOL_OUTPUT_CAP`, 64 KiB) applied **at
      capture**, keeping the head and the tail with a counted elision in the middle — the two
      ends are where the command name and the error live.
- [x] **CHT-102** — `finish_tool` records the result rather than only flipping `state`; the
      close event carries it so the pane does not have to refetch.
- [x] **CHT-103** — Per-turn timing: `started_at` / `finished_at` on `ChatTurnView`, and a
      `worked_ms` derived once in Rust (INV-051: the webview does not compute durations).
- [x] **CHT-104** — Per-turn change summary: `ChatTurnView.changes: Option<TurnChanges>` with
      `files: Vec<TurnFileChange { path, additions, deletions, status }>`, `total_additions`,
      `total_deletions`. Built from the same `bhippi-app::review` types the modal already uses.
- [x] **CHT-105** — A turn's file writes are recorded as they happen, so the summary is what
      *this turn* did rather than what the workspace currently differs by.
- [x] **CHT-106** — Notices: `ChatTurnView.notices: Vec<TurnNotice { level, message, hint }>`
      for usage limits, rate limits and provider warnings, so C6 has a home.

**Acceptance:** a turn that runs two commands and edits three files serialises with both
commands, both outputs, both exit codes, three file changes with real line counts, and a
`worked_ms` — provable from a Rust test, with no UI involved.

### Phase B — the transcript  ·  `CHT-110…CHT-118`

*Closes C2, C3, C4, C6.*

- [x] **CHT-110** — Group consecutive steps by kind into one row, with the Codex verb
      phrasing: *Ran commands*, *Edited files*, *Edited files, ran commands*, *Explored*,
      *Searched the web*. The label is derived from what the group actually contains.
- [x] **CHT-111** — `<ActivityGroup>`: collapsed by default; open when it is the last group of
      a running turn; keyboard-operable; the chevron and the count are the affordance.
- [x] **CHT-112** — `<CommandBlock>`: `$ command` in monospace, output in a bounded scroll
      region, exit code shown when non-zero, copy button, and an explicit "output truncated"
      line when it was.
- [~] **CHT-113** (claude, 2026-09-01 — file rows with real `+N −M`; the expandable hunk view is Phase C) — `<InlineDiff>`: per-file `+N −M` header that expands into the existing
      `DiffHunkView`. Reuses `DiffView.tsx` rather than growing a second diff renderer.
- [x] **CHT-114** — `<TurnChangesCard>`: "Edited N files", the totals, the first three files,
      "Show N more files", and the two actions.
- [x] **CHT-115** — **Undo** on that card: revert the turn's file changes as one operation,
      with a confirm, and a disabled state with a reason when the files have since moved on.
- [x] **CHT-116** — **Review** opens the existing `ReviewChangesModal` filtered to this turn.
- [x] **CHT-117** — "Worked for 13m 42s" rendered from `worked_ms`, above the changes card.
- [x] **CHT-118** — The notice lane, rendered from `notices`.

**Acceptance:** replaying a recorded turn produces the shape in §3 — groups collapsed,
commands and diffs behind them, a change summary with working Undo and Review.

**2026-09-01 — Phases A and B ship.**

**The record first, because the screenshots are not a skin.** Every visual difference from
the target was downstream of C1: a step was five strings, so an activity row could not expand
into anything. `ToolActivity` now carries the command, the output, the exit code, the elapsed
time and the files it changed; `finish_tool_with` records them on the same lock that flips the
state, so a finished turn re-renders from the conversation alone with no refetch.

**Numbers that are true.** `line_change` runs a longest-common-subsequence over the two line
lists rather than counting the file, so touching one line in a 500-line file reports **+1 −1**
and not +500 −500 — the failure that makes a summary card worthless. `TurnChanges::from_tools`
folds a file edited three times into **one** file with the lines summed, for the same reason.

**Bounded at capture, not at render.** `TOOL_OUTPUT_CAP` (64 KiB) is applied where the output
is captured, keeping the head *and* the tail with a counted elision, because the command and
the first error are at the top and the summary and the exit are at the bottom — trimming only
the tail throws away the half people scroll to. Truncation is stated in the UI rather than
hidden. The undo snapshot has its own budget (`TURN_UNDO_BUDGET`, 8 MiB) and evicts **whole
turns**, never part of one: an Undo that restores some files and not others is worse than none.

**Grouping says what happened.** The header used to read `Exploring N files` on every turn,
including one that edited twelve files and ran four commands. Rows are now grouped by what
the steps actually were, *consecutively* — read/edit/read/edit is four rows, because folding
it into two would misreport the order the reader is following — and `groupTools` is a plain
module (`turnGrouping.ts`) with tests, not logic buried in JSX.

**Undo is offered only when it can work.** The snapshot is in-memory and session-scoped, so
the card asks `chat_turn_undoable` and disables itself with a reason rather than failing when
pressed. A file that did not exist before the turn is removed, not blanked.

**Not done:** CHT-113's expandable per-file hunk view (the rows carry real counts; expanding
one into `DiffHunkView` is Phase C), and all of Phase C — composer, sticky header, scroll
affordance, and the accessibility pass.

### Phase C — the composer and the chrome  ·  `CHT-120…CHT-126`

- [ ] **CHT-120** — Composer: placeholder "Ask for follow-up changes", the `+` attach affordance,
      the access chip, and the model/effort chip on the right, matching the target's density.
- [ ] **CHT-121** — Access chip reflects the real permission mode and opens the picker.
- [ ] **CHT-122** — Sticky turn header with the conversation title and the transcript actions.
- [ ] **CHT-123** — Auto-scroll with a "jump to latest" affordance that appears only when the
      user has scrolled away from the bottom.
- [ ] **CHT-124** — Copy: whole turn, one command, one output, one diff.
- [ ] **CHT-125** — INV-034/INV-075 pass: loading / empty / error / populated on every new
      component, focus order, and AA contrast on the new surfaces.
- [ ] **CHT-126** — Density and motion consistent with `docs/DESIGN-SYSTEM.md`; no new colours
      outside the token set.

**Acceptance:** the transcript reads at a glance, every disclosure is keyboard-reachable, and
nothing new is hard-coded outside the design tokens.

---

## 5. Rules this plan must not break

| Rule | Why it bites here |
|---|---|
| INV-051 — the webview computes nothing | Durations, line counts, group labels and truncation are computed in Rust. The pane renders. |
| ADR-0006 — the chat surface is conversational | Groups and cards are disclosure, not a second UI. The prose stays the spine of a turn. |
| Bounded memory | `TOOL_OUTPUT_CAP` at capture, not at render. A turn is kept in memory for the life of the conversation. |
| Gates block, never warn | An **Undo** that cannot safely apply is disabled with a reason, never offered and then silently refused. |

---

## 6. Progress log

| Date | Agent | Tickets | What shipped | Why it was worth it | Next |
|---|---|---|---|---|---|
| 2026-09-01 | claude | CHT-100…106, 110–112, 114–118 (113 partial) | **The record, then the transcript.** `ToolActivity` grew command/output/exit code/elapsed/changes; `finish_tool_with` records them; `ChatTurnView` gained `worked_ms`, `changes` and `notices`. New `TurnActivity.tsx` + `turnGrouping.ts` render collapsible activity groups, command blocks with bounded scrolling output, per-file `+N −M` rows, a "Worked for 13m 42s" line, a notice lane, and an "Edited N files +X −Y" card with working **Undo** and **Review**. New `undo_chat_turn` / `chat_turn_undoable` commands over a budgeted in-memory snapshot. | The transcript could not show its work because there was no work recorded — only labels. Nine new tests pin the parts that would break silently: LCS line counts (one line changed is +1 −1, not +500), same-file folding, UTF-8-safe output capping, and consecutive-not-global grouping. | Phase C: composer density, sticky header, scroll affordance, copy affordances, and the INV-034/INV-075 pass. Then CHT-113's expandable hunk view over the existing `DiffHunkView`. |
