# Bhippi — Invariant Register
**Doc:** `06-INVARIANTS.md` · **Status:** authoritative · **Change control:** ADR + spec bump

Every `[HARD REQ]` in the spec, plus the structural rules this documentation set adds, as one
numbered list. **Each invariant names the place in code that enforces it and the test that
proves it.** "The prompt says so" is never an enforcement point.

**How to use this file**
1. Before writing code, read the invariants for your module (`02-MODULE-CONTRACTS.md` names
   them per crate).
2. In your PR description, list the invariant IDs your change touches.
3. If you cannot enforce one in code, do not merge — raise a `DECISION-CHANGE`.
4. Reviewers check the *enforcement point*, not the intent.

Legend — **Class:** `L` legal/ethical · `S` safety/security · `Q` quality · `A` architecture ·
`P` performance.

---

## Provider & inference

| ID | Class | Invariant | Enforced at | Proven by |
|---|---|---|---|---|
| INV-001 | S | `local-only` / `offline_mode` never opens a network socket for inference; failure is loud and actionable, never a silent cloud fallback | `providers::Router::resolve` | no-network integration test |
| INV-002 | S | Credential *values* are never read from vendor config dirs, copied into Bhippi storage, or logged. Presence only | `providers::detect` | unit + log-scrub test |
| INV-003 | S | Provider CLIs spawn with explicit argv, scrubbed env, and a timeout — never a shell string | `providers::cli::spawn` | unit; injection fixture |
| INV-008 | Q | `Editor` resolves to a different provider instance than `Writer` whenever ≥ 2 healthy providers exist | `Router::pinned` + session `writer_provider` | unit + e2e |
| INV-035 | A | Prompts are files under `prompts/` with a `version:` header, hash-pinned into `prompt_versions` and `posts.prompt_hashes` at use. No prompt string literals in code | `prompts::load` + CI grep | CI lint |
| INV-053 | S | Every provider call is preceded by a budget-guard check that can reject it before it is issued | `core::BudgetGuard::check` | unit + soak |

## Crawling & sources

| ID | Class | Invariant | Enforced at | Proven by |
|---|---|---|---|---|
| INV-004 | L | `robots.txt` fetched, cached 12 h, obeyed. **No bypass exists in the type system** | `harvest::fetch` | robots-disallow test asserts zero requests; CI grep for override flags |
| INV-005 | L | Paywalled ⇒ record, keep only free abstract/metadata, stop. No archive mirrors, cookies, or reader-mode bypass | `harvest::paywall` | fixture test |
| INV-006 | L | Honest UA; 0.5 rps/host; 1 connection/host; `Crawl-delay`, `429`/`503 Retry-After` honoured exactly | `harvest::governor` | backoff test |
| INV-050 | Q | Primary-source jump: when an article references a paper/benchmark/filing/changelog/model card, the primary is fetched and its dots preferred | `harvest::discover::primary_jump` | fixture + golden topics |
| INV-045 | Q | A tier-4 source may suggest a lead but may never be the sole support for a published claim | fact gate | seeded fixture |
| INV-044 | Q | Numbers, dates, prices, benchmarks need ≥ 2 independent sources or 1 tier-1 primary, else they are downgraded to attributed claims | fact gate | unit + golden |

## Research quality

| ID | Class | Invariant | Enforced at | Proven by |
|---|---|---|---|---|
| INV-007 | Q | Domain lock: technology and AI only. `in_scope == false` or score < threshold ⇒ abort with a user-facing message. Never "try anyway" | `research::plan` domain gate | classifier fixture set |
| INV-009 | Q | Anti-drift: every child node holds cosine ≥ 0.45 to the seed, or is an explicitly justified counterpoint/prerequisite | `research::drift_guard` | golden topics: ≤ 2 % off-topic at hop ≥ 3 |
| INV-010 | Q | A dot without `source_id` and character offsets is dropped, never repaired | `research::extract_dots` | unit |
| INV-011 | L | Quotes ≤ 15 words, at most one per source — enforced in code at extraction **and** lint | extractor + style linter | unit both ends |
| INV-012 | Q | `unknowns` is non-empty for X12/X24 blueprints | `research::synthesise` | unit |
| INV-013 | Q | Unresolved contradictions appear in the article; never silently resolved | fact gate + writer structure | golden |
| INV-040 | A | The tier budget table exists in exactly one place: `bhippi-types::Tier::budget()` | type system | snapshot test vs. spec §10.1 |
| INV-041 | Q | Tier floors are gates: below floor ⇒ `thin_evidence` and forced review, never a silent publish | `core` floors check | e2e |
| INV-051 | A | The mind map is a persisted first-class artifact; layout physics runs in Rust and streams positions. **No layout physics in JavaScript** | `research::layout_step` | CI grep + perf test at 500 nodes |

## Memory

| ID | Class | Invariant | Enforced at | Proven by |
|---|---|---|---|---|
| INV-014 | Q | Memory is a prior to verify, never ground truth to repeat. No memory-sourced claim reaches a post without a live source fetched in the current run | fact gate provenance check | e2e |
| INV-054 | P | Gists ≤ 1200 tokens; the injected prior-knowledge block ≤ 6 % of planner context | `memory::write_gist` / `retrieve` | unit |
| INV-055 | A | `forget` removes graph rows, referencing gists, FTS docs and vectors atomically | `MemoryRepo::forget` (one tx) | integration |

## Images

| ID | Class | Invariant | Enforced at | Proven by |
|---|---|---|---|---|
| INV-015 | L | `license = 'unknown'` ⇒ rejected; the publisher refuses to build a post containing a rejected image. Attribution stored verbatim | `vision::licence` + publish verify | e2e |
| INV-016 | Q | No crop cuts a `safe_crop_region`; never upscale beyond source; diagrams and screenshots letterboxed, never cropped; portraits keep headroom | `vision::crop_set` | geometric assertion test |
| INV-047 | L | No images of identifiable private individuals; public figures only from press kits or open-licence archives | vision reject rules | fixture |

## Writing

| ID | Class | Invariant | Enforced at | Proven by |
|---|---|---|---|---|
| INV-017 | Q | Zero style-linter **hard** failures may reach publish (banned openers are a build failure, not a warning) | `writer::lint` + publish verify | 30-post suite |
| INV-018 | Q | Every paragraph maps to ≥ 1 dot; orphan paragraphs fail the build | `writer::lint` | unit |
| INV-056 | Q | The chosen hook's claim maps to a dot with confidence ≥ 0.8 | `writer::score_hook` | unit |

## SEO & publishing

| ID | Class | Invariant | Enforced at | Proven by |
|---|---|---|---|---|
| INV-019 | L | AI disclosure on every post: machine-readable, visible, carries review status, **not removable in the UI** | `seo::metadata` + publish verify + theme | e2e + UI test |
| INV-022 | S | Publish is atomic: build → verify → swap → record. Power loss leaves the old site or the new one, never a half-written one. Rollback is one command | `publish::deploy` | power-loss simulation |
| INV-023 | Q | The build **fails** (never warns) on: broken internal link · missing image variant · unresolved licence · style hard failure · `fact_score < 70` without approval · duplicate slug · missing meta description · missing disclosure · Lighthouse SEO < 95 / Perf < 90 | `publish::verify` | CI |
| INV-024 | A | Both renderers consume identical `post.json`; **no content logic in any template layer** | renderer trait boundary | review + parity test |
| INV-057 | Q | Internal-link insertions into older posts are recorded in `link_edits` and revertible | `seo::internal_links` | unit |
| INV-048 | L | Corrections: `retracted` renders a visible notice with the original struck through; the URL is never silently rewritten | theme + `PostRepo::retract` | e2e |
| INV-046 | L | Negative, uncorroborated claims about named people or companies are blocked at the fact gate; tier-4 can never support them | fact gate | seeded fixture |

## Ticker & automation

| ID | Class | Invariant | Enforced at | Proven by |
|---|---|---|---|---|
| INV-025 | S | Auto-trigger requires **all nine** conditions plus a 5-minute cluster-stability debounce | `ticker::should_trigger` | wire-story + correction fixtures |
| INV-029 | A | Exactly one research session runs at a time | `core` single permit | soak |
| INV-030 | S | The job queue is SQLite-persisted and idempotent; 3 failures ⇒ dead-letter card in the UI | `JobRepo` | crash test |
| INV-031 | L | Thin evidence ⇒ forced review regardless of settings | `core` gate | e2e |
| INV-052 | S | Kill switch stops all work within 3 s and leaves no orphan rows or temp dirs | `core::kill_switch` | timed test |
| INV-058 | S | Crash-loop guard: 3 consecutive failed sessions ⇒ automation disables itself and reports why | `core::automation` | fault-injection test |

## Skills

| ID | Class | Invariant | Enforced at | Proven by |
|---|---|---|---|---|
| INV-026 | S | `script` skills and any skill requesting `net` or `fs_write` require explicit user approval before leaving trial. No silent mode; the user sees a diff | `skills::autonomy_gate` | red-team CI |
| INV-027 | S | Sandbox limits: Rhai — no fs/net, whitelisted stdlib, 200 ms CPU, 8 MB, op counter. WASM — declared preopens only, fuel-metered, 2 s, 64 MB | runtime configs | red-team CI |
| INV-028 | S | Skills never see API keys, the keychain, the DB handle, the filesystem outside the session scratch dir, or raw provider clients | host API surface | red-team CI |
| INV-059 | Q | A broken skill never blocks a session; the baseline path always remains | `skills::invoke` fallback | fault-injection test |

## Platform, security, architecture

| ID | Class | Invariant | Enforced at | Proven by |
|---|---|---|---|---|
| INV-020 | A | Every stage transition is one transaction that writes the artifact and advances `stage_cursor`. Restart resumes there and never re-fetches a session's existing URLs | `SessionRepo::advance_stage` | kill-and-resume test |
| INV-021 | P | `mindmap.delta` and `dot.added` coalesce to ≤ 20 emissions/second; the UI never gets a firehose | `core::bus` coalescer | unit + UI perf |
| INV-032 | A | `ui/src/lib/ipc.ts` is generated by `specta`; hand-written IPC types are forbidden | build step | CI fails on dirty generated file |
| INV-033 | A | CLI/GUI parity — everything the GUI can do, the CLI can do | `bhippi-app` command surface | CLI e2e drives the full flow |
| INV-034 | A | Accessibility floor: keyboard everywhere, visible focus, AA contrast, pausable/reduced-motion ticker, mind map mirrored as `role="tree"`, no colour-only meaning | components + axe run | a11y CI (score ≥ 95) |
| INV-036 | A | No `unwrap()` / `expect()` outside `#[cfg(test)]` | clippy `-D warnings` | CI |
| INV-037 | S | Secrets live only in the OS keychain — never in `config.toml`, logs, the DB, or a crash report | `secrets` module + scrub layer | pre-commit hook + CI scan |
| INV-038 | S | Fetched content is untrusted: wrapped in a data block, schema-constrained extraction, imperative-pattern filter, `suspicious_source` incident surfaced in the UI | extractor + `incidents` | prompt-injection corpus |
| INV-039 | S | Telemetry is off by default **and off in fact** — no telemetry network call exists in v1 | absence of code | CI grep + review |
| INV-042 | A | No SQL string exists outside `bhippi-db` | review + CI grep | CI |
| INV-043 | P | Nothing CPU-bound runs on the async pool (encode, embed, hash, layout, PDF, Tantivy commit → `spawn_blocking`) | review | soak latency test |
| INV-060 | A | Crate dependency edges match the table in `01-ARCHITECTURE §3.1` | `tests/architecture.rs` | CI |
| INV-070 | A | Scene writes only via `bhippi-engine::Transaction` (human, UI, or AI); a write bypassing `apply_transaction` in edit mode is impossible by construction (ADR-0020) | `bhippi-engine::document::apply` | unit + AI-protocol golden tests |
| INV-071 | A | Every applied transaction is journaled to `engine_transactions` with actor + label; "what did the agent change?" and undo/redo render from the journal (ADR-0020) | `bhippi-db` engine repos | integration |
| INV-072 | — | **RETIRED by ADR-0028.** The viewport is no longer a child process. Replacement: the viewport pane is wrapped in an error boundary; a renderer failure shows the pane's error state with a Reload action and never blanks the shell. | `ui/src/engine/*` | review |
| INV-073 | A | **Narrowed by ADR-0028.** Scene state, transactions, undo, validation, the component and widget registries, asset indexing, HUD rect resolution, material/mesh resolution and play composition live in `bhippi-engine`. Rendering, raycast picking against rendered meshes, and camera navigation are the webview's — they are properties of the picture, not of the document. (Extends INV-051 to the engine, ADR-0020.) | crate layout + review | `tests/architecture.rs` grep + review |
| INV-074 | S | A **Release** build containing an asset with `license = "unknown"` **fails**; Debug builds warn-list such assets in the Build tab; gates block, never warn (extends the no-unlicensed-image rule to all assets, ADR-0020) | `bhippi-engine-build::preflight` | gate test (blocks) |
| INV-075 | A | Every engine panel (hierarchy, inspector, assets, console, build) implements loading/empty/error/populated + keyboard reachability + AA contrast (invoked INV-034 for `ui/src/engine/`, ADR-0020) | `ui/src/engine/*` components | axe run + review |
| INV-076 | P | Engine events (transform batches, play stats, build progress, thumbnails) coalesce through the existing ≤20/s bus (invoked INV-021); the 3D viewport never redraws over IPC (ADR-0020) | `core::bus` coalescer | unit + perf |
| INV-081 | A | Play state is a disposable clone of the composed world. Simulation, HUD actions and level travel never mutate an authored scene document or scene file; Stop discards runtime state and returns to the byte-identical authored document (ADR-0028) | `ui/src/engine/playRuntime.ts` + `EngineViewport` | `ui/tests/play-runtime.test.mjs` |
| INV-082 | A | A gameplay script is compiled in Rust (`bhippi-engine::script`) before it can run: the webview VM executes bytecode and **never** parses script source, calls `eval`, or constructs a `Function`. Every hook runs under a step budget and a call-depth cap, so a runaway script is a located fault in the Output Log rather than a frozen pane (ADR-0030) | `bhippi-engine::script` + `ui/src/engine/scriptVm.ts` | `script_fixture.rs` (the committed program is what the compiler still emits) + `ui/tests/script-vm.test.mjs` + `tests/architecture.rs` grep |
| INV-083 | A | Authored HUD/material/shader/prefab documents declare a supported major format, validate completely before entering a session, and are never silently rewritten or best-effort parsed (ADR-0031) | Rust document parsers/writers | format round-trip + future-marker rejection suites |
| INV-084 | A | Viewport observations and AI playtests are bounded and one-shot: camera, PNG bytes/dimensions, timeout, steps/keys/frames and fixed delta are Rust-validated; late/duplicate responses fail; play remains a disposable clone | `bhippi-app::engine::observation` + `runScriptedPlaytest` | observation queue/PNG tests + viewport capture + play-runtime suites |

## Performance budgets (spec §24)

| ID | Metric | Budget | Proven by |
|---|---|---|---|
| INV-061 | Cold start to interactive | ≤ 1.2 s | perf CI |
| INV-062 | Provider detection, all strategies | ≤ 1.5 s, non-blocking | perf CI |
| INV-063 | Idle CPU (ticker on, 25 feeds) | < 2 % | soak |
| INV-064 | Idle RSS | < 220 MB | soak |
| INV-065 | Peak RSS during X24 | < 900 MB | soak |
| INV-066 | Mind map render, 500 nodes | ≥ 55 fps | perf CI |
| INV-067 | DB after 1000 sessions | < 3 GB incl. blobs | synthetic corpus |
| INV-068 | Published article page | ≤ 120 KB, LCP ≤ 1.8 s on 4G | Lighthouse CI |
| INV-069 | Tier wall clock | X2 ≤ 3 min, X24 ≤ 90 min on the reference machine | golden run |
| INV-077 | Viewport frame rate, 1k-entity scene, editor mode (webview renderer, ADR-0028) | ≥ 55 fps | manual QA; the engine-side half is measured by `bhippi-engine/tests/perf_budget.rs` |
| INV-078 | ~~Engine mode cold attach (viewport spawn → first frame)~~ | **RETIRED by ADR-0028** — there is no process to spawn | — |
| INV-079 | Transaction apply → serialised hierarchy/inspector event projection | ≤ 50 ms at 1k entities | `perf_budget::one_edit_on_a_large_scene_stays_inside_the_transaction_budget` |
| INV-080 | Mind-map incremental regen (500 entities) on blocking pool | ≤ 200 ms | unit bench |

---

## The eight editorial gates (spec §21)

A post that cannot satisfy **all eight** is held, not published. This is code in the gate, not
a checklist someone remembers.

1. Copyright — paraphrase by default; quote caps in code; never reproduce an article's
   structure or narrative flow; wire text never republished.
2. Images — licence resolved or the image does not ship.
3. Paywalls and robots — obeyed, with no bypass path in the codebase.
4. Attribution — every source linked, dated, tiered; a scoop is credited prominently.
5. AI disclosure — non-removable, machine-readable and visible.
6. Corrections — retraction workflow with a visible notice, same URL.
7. Defamation surface — negative uncorroborated claims about named parties blocked.
8. Person imagery — no identifiable private individuals.
