# ADR-0001: Add a `bhippi-types` foundation crate
Date: 2026-08-26 · Status: accepted
Amends: `00-SPEC-v1.0.md` §4 (workspace crate table)

## Context

The spec lists 13 crates and gives each a clear responsibility, but several types are needed
by nearly all of them: `SessionId` and the other ULID newtypes, `Tier` and its budget table,
`Stage`, `TaskClass`, the `Event` enum, and `BhippiError`.

With no shared crate below them, there are only bad options: duplicate the types per crate
(and drift), define them in `bhippi-core` (forcing every capability crate to depend upward on
the orchestrator, breaking the layering), or define them in `bhippi-db` (making every crate
depend on sqlx). Each of those violates a rule the architecture depends on.

The specific risk this addresses: the tier budget table (spec §10.1) is referenced by the
research engine, the orchestrator's budget guard, the writer's length targets, and the UI's
tier-chip hover text. If it exists in more than one place, the depth ladder stops being a
contract — which is a HARD REQ.

## Decision

Add `bhippi-types` as layer L0. It contains types, enums, ID newtypes, event payloads, error
types, and pure functions only. Its dependency set is limited to `serde`, `thiserror`,
`ulid`, `chrono`, and `specta` for type generation. It may not depend on `tokio`, `sqlx`,
`reqwest`, or any workspace crate.

`Tier::budget() -> TierBudget` lives here and is the **only** encoding of the spec §10.1
table anywhere in the codebase, verified by a snapshot test against the spec values.

## Consequences

- Easier: every crate shares one vocabulary; capability crates keep no upward dependency;
  the tier table has exactly one home (INV-040).
- Harder: one more crate to keep disciplined. `bhippi-types` will attract unrelated
  additions; reviewers must reject anything with IO in it.
- Documents updated in this change: `01-ARCHITECTURE.md` §3 and §3.1 (crate graph and
  dependency table), `02-MODULE-CONTRACTS.md` (M0 section), `06-INVARIANTS.md` (INV-040),
  `08-BUILD-ORDER.md` (BHP-002).

## Alternatives considered

- **Types in `bhippi-core`** — rejected: every L2 crate would depend on L3, inverting the
  layering and making capability crates untestable without the orchestrator.
- **Duplicate small type sets per crate** — rejected: guarantees drift in exactly the values
  that must not drift (tier budgets, stage names, error codes).
- **Types in `bhippi-db`** — rejected: pulls sqlx into the writer, the vision pipeline, and
  the provider layer for no reason.
