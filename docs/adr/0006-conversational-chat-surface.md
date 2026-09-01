# ADR-0006: Conversational chat surface as a first-class screen
Date: 2026-08-26 · Status: accepted · Supersedes: 04-PAGES.md Part A screen list (extends it)

## Context

The spec's desktop app defines four screens (Research · Automation · Library · Settings) and a
`chat_turns` table (§7, migration `0001_core.sql`) with no owning screen. The owner has directed
that the app open on a conversational surface — while the engine works it must stream its answer,
show what it is reading as tool-activity cards, and ask permission before consequential actions,
matching the interaction quality of Claude/ChatGPT. The data model already anticipated this;
only the screen and the event protocol were missing.

## Decision

1. Add **A1b · Chat** as the default screen of the desktop app, ahead of Research in the title
   bar. The other four screens are unchanged.
2. Define an engine-level turn stream alongside provider-level deltas:
   - Provider level (`bhippi-providers::Delta`): `Text`, `Thinking`, `Usage`, `Done` — what a model emits.
   - Engine level (chat events to the UI): `chat_delta`, `chat_tool`, `chat_permission`,
     `chat_turn_done`, plus forwarded bus events. Tool activity and permission requests are
     **engine** facts, never provider facts.
3. Permission requests block the turn until the user answers (**allow_once / always / deny**) via
   `permission_respond`. Nothing consequential executes without an answer; deny raises a typed
   error carrying the request id. This is a gate, not a warning (INV-031 spirit).
4. When no real provider is available the Chat screen runs the clearly labelled **Demo provider**
   (scripted, offline, deterministic). It exercises the full event protocol so the interface is
   verifiable before S1/S2 land, and is always visibly badged "demo". Silent fallback is forbidden
   (§11), so the badge is carried in `AppStatus`, not chosen by the UI.

## Consequences

- Easier: the Claude-style affordances have one durable event contract now; harvest/research only
  gain richer tool cards later without UI rework.
- Harder: `chat_turns` persistence must be wired into `bhippi-db` when conversations become
  durable (follow-up ticket in the S1 range); for now history lives in-process.
- Docs changed: `04-PAGES.md` gains §A1b; `PROGRESS.md` tracks the new work as BHP-008 scope.

## Alternatives rejected

- Chat inside Research's topic input: hides the conversation model, cannot show per-turn tool
  activity cleanly.
- Waiting for S1 routing to build any chat UI: leaves the largest UX surface unverified until
  every provider ticket lands; violates the owner's direction.
