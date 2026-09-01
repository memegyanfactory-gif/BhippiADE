# ADR-0032 — Fixed game-debug pipeline and independent quality evidence

- **Status:** Accepted
- **Date:** 2026-09-01
- **Relates to:** ADR-0028, ADR-0030, ADR-0031, INV-073, INV-074, INV-081,
  INV-082; ENG-200…219

## Context

Bhippi already has strict authored game formats, content/build gates, a compiled gameplay
language, deterministic playtest requests and bounded observations. They are separate entry
points. A model can still check only the parts that support its conclusion, call a
compiler-clean project a good game, or grade its own output from prose and screenshots.

The existing `/debug` command is intentionally a general repository scanner. Expanding it
with game semantics would make both contracts ambiguous: a non-game repository should remain
debuggable, while a Bhippi game needs manifest, scene, asset, script, play, sandbox and quality
evidence in a fixed order.

## Decision

### One versioned stage graph

`/gamedebug` invokes an engine-owned `bhippi-game-debug@1` pipeline with exactly these ordered
stage ids: `01_discover`, `02_validate`, `03_compile`, `04_sandbox`, `05_exercise`,
`06_inspect`, `07_observe`, `08_score`, `09_report`.

The caller selects `quick`, `full` or `release`, but cannot reorder the graph. A mode may mark
a non-selected stage `skipped`. A selected stage whose implementation or evidence is absent is
`unsupported`, never passed. A report passes only when every stage mandatory for that mode
passes and the authored-tree hash is unchanged.

`quick` is the local static lane: manifest discovery, authored format/content/asset gates,
gameplay script compilation and structural inspection. `full` adds sandboxed deterministic
exercise, runtime inspection/observation and rubric scoring. `release` adds release content
gates, committed quality floors and current hostile-sandbox evidence.

### Reports are evidence, not model output

Every run creates an immutable ULID-addressed JSON report and matching Markdown rendering in
`.bhippi/reports/game-debug/`. `latest.json` is only a pointer. Reports contain schema/mode,
ordered stages, stable finding codes, evidence/reproduction/repair fields, environment and
authored hashes, quality and sandbox evidence, artefact addresses and any approved repair
transaction id. Runtime reports are ignored by source control and bounded by a retention
policy when ENG-207 closes.

The AI may retrieve the compatible report and propose a repair. It may not write report status,
score itself, suppress a finding or use a stale authored hash as proof. `/gamedebug --fix` goes
through the existing capability/approval path and `EngineActionBatch`; it is never a debugger-
specific write path. Each attempt produces a new diagnostic run.

### Quality protocol

`bhippi-game-test-plan@1` is a hand-editable, versioned scenario document: initial level,
fixed seed, timed input steps, checkpoints and assertions. The engine supplies a mandatory
smoke scenario when none is authored. Unknown major versions block.

Rubric v1 separately measures bootability, goal clarity, control correctness, progression and
finishability, failure/recovery, runtime stability, visual legibility, HUD feedback, content
coherence and performance. Every dimension cites deterministic or captured evidence and has a
confidence. Missing evidence yields `not_measured`, not a guessed score. Deterministic scores
and optional multimodal critique are stored separately.

The committed corpus has diverse valid games and deliberately broken mutations. CI checks
stable finding codes, required per-case/per-dimension floors, aggregate floor and baseline
deltas. Averages cannot hide a newly broken canonical case. Live-provider comparisons are a
separate evaluation lane and never make deterministic CI flaky.

## Rollout

The first slice implements the stage graph, `quick` discovery/validation/script compilation,
authored hashing, stable findings, local command parsing and immutable JSON/Markdown reports.
Until the runtime, rubric and sandbox phases land, `full` and `release` return `incomplete` with
their selected runtime stages marked `unsupported`. This is intentional compatibility
behaviour, not a temporary green stub.

## Consequences

- General `/debug` and game-aware `/gamedebug` remain separate offline commands.
- New checks extend existing engine parsers/gates instead of copying their rules into prompts.
- Report schema changes require versioning plus old/new golden fixtures.
- A quality claim is reviewable and reproducible, but the corpus, replay and visual lanes add
  test time and retained artefacts.
- The runtime sandbox boundary and backend choice remain ADR-0033 work; this ADR does not call a
  web worker a security boundary.

## Alternatives rejected

- **Let the model choose tests.** It is useful for extra scenarios, not mandatory coverage.
- **Add game checks to `/debug`.** Repository health and game health have different inputs and
  pass criteria.
- **One subjective “fun” score.** It is neither reproducible nor actionable and hides missing
  evidence.
- **Automatically edit during diagnosis.** It violates the one-write-path architecture and
  destroys the before-state needed to reproduce a finding.
