# ADR-0016 — Live CLI streaming, typed faults, and an agent phase vocabulary

- **Status:** Accepted
- **Date:** 2026-08-27
- **Supersedes:** nothing
- **Relates to:** ADR-0006 (chat surface), ADR-0008 (provider edges), ADR-0009 (usage metering
  and accent), ADR-0014 (workbench & activity dock)

## Context

Three complaints, all of which turned out to have one cause each in the tree.

**1 · "Claude takes too much time."** `CliProvider::complete` called
`tokio::process::Command::output()`, which resolves only when the child process has exited.
The finished text was then split into word-sized pieces and handed back as a `Stream`, so
the type signature said "streaming" while the behaviour was "wait, then dump". A turn that
Claude Code answered in two seconds still showed nothing until the process torn down, and a
ninety-second agent turn was ninety seconds of blank screen. The catalogue compounded it by
asking for `--output-format json`, under which the CLI itself buffers the whole turn.

Measured against the live CLI: `ttft_ms: 1266`, `duration_ms: 2284`. Slightly over a second
of that wait was real; the rest was ours.

**2 · "Context and weekly limits give no specific error."** `hint_for` matched three
keyword families — 401/403, 402, 429 — and everything else fell through to "reinstall it
from Settings", which fixes none of them. Context-window overflow was not matched at all.
Claude's five-hour and seven-day windows both collapsed into one "rate-limited, wait a few
minutes", which is actively wrong advice for a window that resets on a billing boundary.
Worse, a CLI that reported its own failure **in-band while exiting 0** — which is what
Claude Code does for a spent limit — was read as a successful empty answer and surfaced as
"the CLI answered with nothing", the one message that explains none of the four things that
had actually happened.

**3 · The activity dock had nothing real to show.** Tool activity was only ever produced by
the offline demo script. Every vendor CLI announces each file it reads and each command it
runs in its own event stream, and all of it was discarded, so on the providers people
actually use the dock sat empty and animated a fixed `async execute() { working... }`
ticker regardless of what was happening.

## Decision

### 1 · The CLI adapter streams for real

`CliProvider::complete` spawns the child, reads **stdout a line at a time**, and pushes each
line through a new incremental `transcript::Reader` into a `Delta` as it arrives. stderr is
drained concurrently (a full stderr pipe deadlocks a child mid-write, which is
indistinguishable from a hang) and used only to explain a failure.

Consequences taken deliberately:

- **The timeout becomes a silence budget.** 90 s with *no output at all*, plus a 20-minute
  hard ceiling. The old flat 180 s wall clock killed healthy long turns; a working agent is
  never silent, so silence is the honest signal.
- **`kill_on_drop`, and a kill when the receiver goes away.** A stopped turn used to leave a
  vendor process running against the user's own quota.
- **Claude Code is asked for `stream-json --verbose --include-partial-messages`**, which is
  token-level, plus `--strict-mcp-config` with no `--mcp-config` beside it, which loads no
  MCP servers. A chat turn needs none, and booting a project's servers is dead time on
  every turn before the model is even asked anything.
- **`Capabilities::streaming` is now true** for CLI backends, and each catalogue row carries
  a real `context_window`.

Because Claude prints every sentence up to three times (partial deltas, then the finished
`assistant` block, then again in `result`), the reader adopts **the first source that speaks
and ignores the others for the rest of the turn**. Partials always precede their block and
`result` always comes last, so first-wins yields exactly one copy — with a `finish()`
fallback that releases the held `result` if nothing else ever spoke, so the non-streaming
output format still works.

### 2 · Failures are named, not stringified

A new pure module, `bhippi-providers::fault`, classifies a vendor's own words into a
`FaultKind` and pairs it with a `Remedy`. Thirteen kinds, chosen by **what the user must do
next** rather than by what went wrong internally:

| Kind | Why it is its own case |
|---|---|
| `ContextExceeded` | The prompt is the problem. Retrying is *certain* to fail; only compaction helps. |
| `RateLimitedSession` | Clears in minutes. Waiting works. |
| `RateLimitedWeekly` | Clears on a billing boundary. Waiting does **not** work; another provider does. |
| `QuotaExhausted` | Money, not time. |
| `Unauthenticated` / `NotInstalled` / `Outdated` | Three different one-line fixes. |
| `Timeout` / `Network` / `Crashed` / `EmptyAnswer` / `Cancelled` / `Unknown` | Distinct enough to say honestly. |

`TurnFault` crosses IPC on both `ChatTurnDone` and `ChatTurnView` — the latter so a fault
card survives a reload instead of leaving an unexplained empty turn. The plain `error`
string is kept beside it for anything that only wants text.

`transcript::read` now also reports an **in-band failure on a clean exit**, which is what
made "answered with nothing" appear in place of a real explanation.

**A pre-send guard** refuses a prompt that cannot fit the provider's context window before
paying for the round trip. The estimate is four bytes per token and does not need to be
better: it is a guard against prompts that are multiples of the window, not an accounting of
ones that are near it, and erring low simply lets the vendor rule on it.

### 3 · Live plan-limit reporting

Claude Code volunteers a `rate_limit_event` on every turn carrying **both** rolling windows
and both reset times, long before either is spent. That is parsed into `Delta::Limit` and
emitted as `ChatLimits`, which drives a banner that appears at 80 %. This turns the limit
story from "your week is gone" *after* a failure into "your week is nearly gone" before one.

### 4 · An agent phase vocabulary

The engine emitted one free-text label, which the UI could only print. `AgentPhase` is a
closed set of 28 states (`connecting`, `thinking`, `reasoning`, `planning`, `searching`,
`reading`, `writing`, `editing`, `running`, `testing`, … `done`, `stopped`, `failed`) carried
on `ChatThinking` alongside the label. A closed set means each state gets its own motion and
copy, and means adding one is a compile error everywhere it must be handled rather than a
string that silently renders as nothing.

Vendor tool events become `Delta::Step`, mapped through a shared verb vocabulary
(`ToolKind`) so Claude's `MultiEdit`, Codex's `file_change`, and OpenCode's tool parts all
land on the same six verbs.

### 5 · Auto-update checks before it installs

The daily sweep reinstalled every enabled CLI unconditionally; `npm install -g` on an
already-current package still costs ~30 s of resolution. It now asks the registry for the
latest version first and installs only on a genuine mismatch. An unreadable answer means
**leave it alone** — treating unknown as stale is how one flaky network turns into a
reinstall on every launch. A fault card whose remedy is `update` runs the same recipe on the
user's click.

### 6 · A motion system, not per-component animation

`ui/src/styles/motion.css` holds durations named for the *kind* of change they describe,
easings named for what the motion is doing, and a reusable keyframe library. Transforms and
opacity only — animating layout forces reflow every frame. Every state carries a static tell
so `prefers-reduced-motion` can switch motion off entirely rather than merely speeding it up.

Elevation tokens (`--lift-1/2/3`) are introduced as a **scoped exception** to ADR-0009's
"hairlines, never shadows": that rule holds for anything sitting *in* the layout, where a
shadow is a fake light source the flat surfaces around it contradict. It does not hold for
surfaces that float *over* it, where a border alone reads as another panel rather than as
one that has lifted.

## Consequences

**Good.** First token in ~1 s instead of at turn end. Four previously indistinguishable
failures now render as four different cards with four different buttons. Limits are visible
before they bite. The activity dock shows real work. Auto-update stops costing minutes a day
to achieve nothing.

**Costs and risks.**

- `--strict-mcp-config` and `--include-partial-messages` are recent Claude Code flags. An
  older build rejects them with "unexpected argument", which classifies as `Outdated` and
  offers a one-click update — degraded, but self-explaining and self-fixing.
- The fault classifier matches vendor prose, which vendors reword. Every phrasing is pinned
  by a test; a reworded message degrades to `Unknown`, which still names a next step.
- `Delta` gained two variants, so every consumer must handle them. That is the point: the
  match is exhaustive and a new backend cannot quietly drop them.
- `--thinking` on OpenCode makes turns more verbose on stdout. That is what fills the
  reasoning drawer, which was otherwise always empty for that backend.

## Alternatives rejected

- **Keep `output()` and fake the stream faster.** Chunking finished text cannot produce a
  first token before the process exits. The problem is structural, not a tuning constant.
- **A tokeniser per vendor for the context guard.** Exact, out of date the week a vendor
  changes it, and it would not change the decision in any case this catches.
- **One `RateLimited` fault with a detail string.** The whole complaint is that one message
  cannot serve two failures whose correct advice is opposite.
- **Parse vendor reset times into timestamps.** The vendor's own wording ("resets at 4pm")
  is more useful than our paraphrase, and a wrong paraphrase of a reset time is worse than
  none. It is carried verbatim.
