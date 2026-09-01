# ADR-0008: The application shell may use providers and db directly
Date: 2026-08-26 · Status: accepted · Supersedes: nothing · Amends: 01-ARCHITECTURE §3.1 (`bhippi-app` row)

## Context

The chat surface (ADR-0006) streams model deltas straight through to the UI. Routing those
calls through `bhippi-core`'s facade would mean re-declaring the whole provider command
surface as pass-through functions in core — indirection with no seam change, since L4 is
already the composition root. Separately, conversation persistence (`chat_turns`, migration
`0001_core.sql`) needs `bhippi-db` repositories in the same process that owns the Tauri
commands. The architecture guard (`tests/architecture.rs`) correctly failed these two edges;
this ADR is its prescribed remedy ("adding an edge requires an ADR").

## Decision

`bhippi-app`'s allowed dependencies become:

- `bhippi-core`, `bhippi-types` (unchanged)
- **`bhippi-providers`** — the shell resolves a chosen backend and streams `Delta`s itself
- **`bhippi-db`** — the shell owns repositories for IPC commands (chat history first user)

No other L1/L2 edge is opened. Domain-to-domain rules are untouched; this loosens only the
top of the graph, where composition happens anyway.

## Consequences

- Easier: chat and settings commands stay thin; no mirror API in core.
- Harder: the shell could grow business logic where core should own it. Guard: R3-style
  review — anything that scores, gates, or persists beyond simple reads belongs in an L2/L3
  crate, and the architecture test still fails on every *other* new edge.
- Docs changed: `01-ARCHITECTURE §3.1` row updated in the same change; test table updated.

## Alternatives rejected

- Pass-through façade in `bhippi-core`: pure but adds ~20 forwarding functions today and
  again per future IPC command, with no isolation benefit at the composition root.
- Moving chat into `bhippi-core`: puts UI-shaped event types into an L3 crate.
