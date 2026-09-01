# ADR-0009: Usage metering, and the accent moves from green to amber

Date: 2026-08-26 · Status: accepted · Supersedes: nothing · Amends: 04-PAGES §Design contract

## Context

Two owner-directed changes arrived together, and they touch the same surfaces.

**Metering.** The status bar reported one number — `tokens_today`, an in-memory `AtomicU64`
that reset on every launch and was the same figure whichever backend answered. With a
provider list that spans subscription CLIs, local servers and metered cloud APIs, one shared
counter cannot answer the only question worth asking: *how much of my budget for the backend
that is about to answer is left?* Nothing was persisted, nothing was attributed, and cost was
not represented at all.

**Palette.** The design contract in `04-PAGES` fixed the accent at `#4ADE9B`, a cool green,
over a cool neutral ramp. The owner asked for amber/orange. Since the accent is named in a
document that outranks code, changing it in `tokens.css` alone would be a silent deviation.

## Decision

### 1. The ledger is a JSON file of daily rollups, not a table

`bhippi-core::UsageStore` owns `~/.bhippi/usage.json`, beside `config.toml`. One row per
`(day, provider)` holding input tokens, output tokens, cost in micro-dollars, and turn count;
90 days retained; writes are temp-file-then-rename and serialised by an internal lock.

Deliberately **not** SQLite. The desktop shell does not open a `Database` yet — that lands
with `chat_turns` persistence — and a rollup ledger is a few kilobytes after a year. When
chat history moves into `bhippi-db`, this can move with it; until then a second store beside
the config is smaller than the wiring a `Database` handle in the Tauri runtime would cost.
INV-042 is untouched: there is no SQL here to be outside `bhippi-db`.

### 2. Cost is an estimate, and says so

`bhippi-providers::pricing` holds list prices per vendor in USD per million tokens. Providers
absent from that table are **not free** — they are *not metered per token*: subscription CLIs,
local servers and the demo bill nothing per call, so `metered: false` and the UI renders a
dash, never a fabricated `$0.00` that reads like a measurement. Every surface that prints a
dollar figure labels it estimated.

### 3. One window, one meaning

`UsageSummary.limit_tokens` and `.fraction` are always measured over the same window as
`.total_tokens`: the daily cap from `[budget]` scaled by the window's day count. A monthly
view is therefore never a daily ring in disguise. The status-bar gauge always requests the
day window, so the ring and the composer's provider can never disagree.

Ceilings live in `budget.provider_token_caps` (provider id → daily tokens). An absent id
falls back to `budget.daily_token_cap`; a stored `0` means *uncapped* and renders as an empty
track, not an instantly-full ring. The IPC command rejects an incoming `0` so a slipped
keystroke cannot silently disable the gauge.

### 4. The accent becomes amber over a warm ramp

`--accent: #f0a02c` (8.9:1 on the background) with `--accent-warm: #ff8b3d` for the orange
end, over a warm neutral ramp (`--bg: #100f0d` … `--text: #eae7e1`). The gauge ramp is a
**separate** scale — `--gauge-0` green → `--gauge-1` amber → `--gauge-2` orange →
`--gauge-3` red — because it means budget state, not brand. Per INV-034 no gauge ships
colour alone: the ring, every bar, and every row also print their percentage.

Vendor logo marks keep their real brand colours (Anthropic's terracotta, Groq's red). Only
Bhippi's own `demo` mark moved to the accent.

## Consequences

- Easier: the gauge, its drop-up and Settings › Usage all read one `get_usage_summary` call;
  a new surface needs no new query.
- Easier: usage survives a restart, and a per-provider ceiling is a one-field edit.
- Harder: prices drift. The table is a maintenance item and must stay labelled as an
  estimate; it must never be presented as a bill or drive a hard gate.
- Harder: a second persistence location exists until chat history lands in `bhippi-db`. The
  merge is tracked as a follow-up, not a permanent split.
- Docs changed: `04-PAGES §Design contract` accent line, in the same change.

## Alternatives rejected

- **A row per turn in SQLite.** Correct long-term shape, but it forces a `Database` handle
  into the Tauri runtime a sprint early and buys nothing the gauge needs — no surface asks a
  per-turn question yet.
- **Reading each vendor's own quota API.** Truthful, but it means live credentialed calls to
  six vendors, per-vendor auth, and a network dependency in the status bar. Rejected for v1;
  reconsider once cloud adapters exist.
- **Keeping one shared token counter and colouring it amber.** Cheap, but it cannot answer
  the question the gauge exists to answer.
- **A monthly window for the ring.** Reads calmer, but hides the day you actually burn a cap.
