# ADR-0003: Batch dot-added events without losing node provenance
Date: 2026-08-26 · Status: accepted
Amends: `01-ARCHITECTURE.md` §6

## Context

The event sketch gave `DotAdded` one node and one dot, while the binding bus rule requires
dot events to coalesce to at most 20 emissions per second without silently dropping any
dots. A single-dot payload cannot satisfy both requirements during a burst.

## Decision

`DotAdded` carries `Vec<NodeDotDelta>` plus `merged`. Each delta retains its node id. Dot
and mind-map batches share one paced output lane, so their combined rate is at most 20 per
second. Queue overflow and subscriber lag yield a typed `ResyncRequired` event.

## Consequences

The UI can apply a complete batch in one render and can explicitly refetch after lag. Event
payloads are slightly larger, but no provenance is lost and the rate invariant is testable.

## Alternatives considered

- Dropping intermediate dots was rejected because the bus contract forbids silent loss.
- Emitting several single-dot events every 50 ms was rejected because it violates INV-021.
- A second dot-batch event name was rejected because it would duplicate one semantic fact.
