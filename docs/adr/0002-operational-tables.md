# ADR-0002: Add the operational tables the spec's behaviour requires
Date: 2026-08-26 · Status: accepted
Amends: `00-SPEC-v1.0.md` §7 (schema)

## Context

Spec §7 defines the domain schema — sessions, nodes, dots, sources, images, memory, ticker,
posts, skills, providers. Several behaviours the spec makes HARD REQs have no table to live
in:

- §16.2 requires a crash-resumable job queue "in SQLite" — no table defined.
- §16.3 requires a dead-letter surface the user can inspect and requeue — no table.
- §5 requires prompts to be hash-pinned so a published post is reproducible — nowhere to pin.
- §14.4 requires internal-link insertions into older posts to be revertible — no record.
- §14.3 requires a 301 map for slug changes — no table.
- §11.4 defines four learning loops (source reputation, query effectiveness, interest graph,
  style memory) with nowhere to accumulate.
- §20 requires `suspicious_source` incidents to be *visible in the UI* — no store.
- §22 requires per-session metrics to be stored so quality regressions are diffable.
- §14.5/§16.5 require rollback to a previous deploy — no deploy history.

Without these, each subsystem would invent its own JSON blob on a session row or a file on
disk, and the "everything is inspectable" principle would quietly stop being true.

## Decision

Add migration `0002_operations.sql` with: `jobs`, `dead_letters`, `prompt_versions`,
`link_edits`, `redirects`, `deploys`, `incidents`, `domain_stats`, `query_stats`,
`interest_weights`, `style_prefs`, `skill_runs`, `session_metrics`.

Add migration `0003` with the column additions listed in `03-DATA-MODEL.md` §2.3 —
`sessions.charter`, `sessions.blueprint`, `sessions.writer_provider`, `sessions.flags`,
`posts.disclosure`, `posts.correction`, `sources.learned_trust_at_fetch`.

Full definitions are canonical in `03-DATA-MODEL.md` §2.2–2.3.

## Consequences

- Easier: resumability, rollback, reproducibility, and the learning loops each have a real
  home and a real test. `bhippi doctor` can check them.
- Harder: 13 more tables to migrate and maintain; `session_metrics` and `skill_runs` grow per
  run and need the retention rules in `03-DATA-MODEL.md` §4.
- `posts.disclosure` is `NOT NULL`, which enforces INV-019 at the schema level rather than in
  a template.
- Documents updated in this change: `03-DATA-MODEL.md` §2.2, §2.3, §3, §5.

## Alternatives considered

- **JSON blobs on existing rows** — rejected: unqueryable, so the UI could not show dead
  letters, incidents, or deploy history without loading everything.
- **Files on disk beside the DB** — rejected: no transaction with the rows they describe,
  which breaks the checkpoint guarantee (INV-020).
- **Defer until the sprint that needs each one** — rejected for the job queue and
  `prompt_versions` specifically: both are load-bearing from S0/S3 and retrofitting them
  means rewriting the loop that already works.
