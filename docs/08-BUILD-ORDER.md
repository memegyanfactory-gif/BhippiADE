# Bhippi — Build Order
**Doc:** `08-BUILD-ORDER.md` · **Derives from:** spec §26 · **Status:** authoritative

The order is not a suggestion. Each sprint exists because the next one cannot be built or
verified without it. Skipping ahead produces work that has to be redone.

**Ticket ranges are from the spec.** Sub-tickets below are this document's decomposition —
add more within the range as needed, never outside it.

---

## S0 · Foundations · `BHP-001…009`

*Nothing here is glamorous and everything downstream depends on it.*

| Ticket | Work | Done when |
|---|---|---|
| BHP-001 | Cargo workspace, 14 crates, MSRV 1.79, shared lint config (`unwrap` denied) | `cargo build` green, empty crates compile |
| BHP-002 | `bhippi-types`: ids, `Tier::budget()`, `Stage`, `TaskClass`, `Event`, `BhippiError` | snapshot test matches spec §10.1 table |
| BHP-003 | `bhippi-db`: schema migrations `0001`–`0003`, repositories, `sqlx` compile-time queries | migrate on a fresh dir, `foreign_key_check` clean |
| BHP-004 | Config loader (`~/.bhippi/config.toml`) + keychain wrapper | config round-trips; no secret in the file |
| BHP-005 | Event bus + 20/s coalescer | INV-021 unit test |
| BHP-006 | `tracing` + rolling JSON logs + secret scrubbing layer | scrub test on a seeded key |
| BHP-007 | **Session replay dumper** (`~/.bhippi/replay/<id>/`) | a fake session produces a readable dump |
| BHP-008 | Tauri shell, 4 empty screens, chrome, token CSS, single-instance guard | app opens on all three platforms |
| BHP-009 | CI: fmt → clippy → architecture test → build matrix | green on macOS/Windows/Linux |

**Exit:** app opens, DB migrates, logs write, replay dumps, CI green on three platforms.

---

## S1 · Providers · `BHP-010…024`

| Ticket | Work |
|---|---|
| BHP-010 | `Provider` trait + `Capabilities` + streaming `Delta` |
| BHP-011 | CLI detection (claude, codex, opencode, grok, kimi, ollama) + version + 5-token ping |
| BHP-012 | Config-dir detection (model names only) — INV-002 |
| BHP-013 | Env-var presence detection |
| BHP-014 | Loopback port probe, 400 ms/port, ≤ 1.5 s total, non-blocking — INV-062 |
| BHP-015 | Capability probe + tok/s benchmark + cost class + 24 h cache |
| BHP-016 | CLI adapter (argv spawn, scrubbed env, timeout) — INV-003 |
| BHP-017 | OpenAI-compatible HTTP adapter (LM Studio / llama.cpp / vLLM / Jan / TGW) |
| BHP-018 | Ollama native adapter |
| BHP-019 | Cloud API adapters behind the same trait |
| BHP-020 | `ProviderRouter`: TaskClass routing, health, `routing` policy — INV-001 |
| BHP-021 | Editor ≠ Writer pinning — INV-008 |
| BHP-022 | Fallback chain (retry → next → max 3) + event per hop |
| BHP-023 | Budget guard + `complete_json` schema validation with one repair |
| BHP-024 | Settings › Providers tab with live health |

**Exit:** a prompt runs end to end on Ollama **and** on one CLI provider, with a visible
fallback when one is killed. Spec §8.5 acceptance criteria all pass.

---

## S2 · Harvest · `BHP-030…048`

| Ticket | Work |
|---|---|
| BHP-030 | `reqwest` client (rustls, gzip, brotli), timeouts, size cap, honest UA |
| BHP-031 | robots fetch + 12 h cache + enforcement, **no override path** — INV-004 |
| BHP-032 | `governor` per-host limiter, `Crawl-delay`, `Retry-After`, cooldown — INV-006 |
| BHP-033 | Global fetch semaphore + `moka` cache + blob store (content-addressed) |
| BHP-034 | Extraction: charset, `dom_smoothie`, metadata, markdown normalisation |
| BHP-035 | Tables, code blocks, image candidates, outbound link inventory |
| BHP-036 | PDF extraction with page-number provenance |
| BHP-037 | Headless fallback via `chromiumoxide`, 15 % session cap |
| BHP-038 | Paywall detection and stop — INV-005 |
| BHP-039 | Canonical URL normalisation + redirect chains |
| BHP-040 | blake3 content hash + 64-bit simhash + Hamming dedupe |
| BHP-041 | `SearchBackend` trait + SearXNG (default), Brave, Tavily, DDG |
| BHP-042 | Feed discovery (`<link rel=alternate>`) |
| BHP-043 | **Primary-source jump** — INV-050 |
| BHP-044 | Topical classifier (see ADR for its home) — INV-007 |
| BHP-045 | Source trust registry seed (~120 domains, Content-Ops) |
| BHP-046 | Fixture suite: 50 frozen pages, 20 feeds, 10 PDFs |
| BHP-047 | Extraction F1 harness vs. hand-labelled ground truth |
| BHP-048 | Settings › Research crawl controls |

**Exit:** F1 ≥ 0.92 on fixtures; robots honoured under test; wire-story collapse works.

---

## S3 · Research engine I · `BHP-060…078`

| Ticket | Work |
|---|---|
| BHP-060 | Charter schema + planner prompt v1 + domain gate |
| BHP-061 | Mind map in-memory model + persistence + resume |
| BHP-062 | Discover → harvest → extract wiring for one expansion |
| BHP-063 | Dot extraction with provenance offsets + typed values — INV-010 |
| BHP-064 | Frontier scorer (`research.toml` constants) |
| BHP-065 | Anti-drift guard — INV-009 |
| BHP-066 | Loop guard + sibling entity cap |
| BHP-067 | Child derivation (concept/entity/question/metric/contradiction) |
| BHP-068 | Dedupe against mind map ∪ memory |
| BHP-069 | Quote cap enforcement at extraction — INV-011 |
| BHP-070 | Contradiction detection (cosine > 0.82, conflicting typed values) |
| BHP-071 | `advance_stage` checkpointing per expansion — INV-020 |
| BHP-072 | Budget enforcement: expansions, depth, tokens, wall |
| BHP-073 | Tier floors + `thin_evidence` flag — INV-041 |
| BHP-074 | `MindmapDelta` events + coalescing |
| BHP-075 | Pause / resume / cancel / focus-node commands |
| BHP-076 | Prompt-injection filter + `suspicious_source` incidents — INV-038 |
| BHP-077 | Golden-topic harness (20 topics) |
| BHP-078 | Kill-and-resume test with zero duplicate fetches |

**Exit:** X2 and X6 produce complete mind maps on golden topics within budget.

---

## S4 · Research engine II + mind map UI · `BHP-079…092`

| Ticket | Work |
|---|---|
| BHP-079 | X12 / X24 budgets and their floors |
| BHP-080 | Counter-evidence passes |
| BHP-081 | Timeline reconstruction (X12+) |
| BHP-082 | Entity deep-dives |
| BHP-083 | Synthesis blueprint + `unknowns` requirement — INV-012 |
| BHP-084 | Fact-check gate: provenance, corroboration, recency, contradiction |
| BHP-085 | Arithmetic recomputation in Rust + hallucination sweep |
| BHP-086 | `fact_score` and the < 70 forced-review path |
| BHP-087 | Barnes-Hut layout in Rust + position streaming — INV-051 |
| BHP-088 | Canvas renderer: nodes, typed edges, dashed contradictions |
| BHP-089 | Inspector panel: dots, sources, tiers, confidence, offsets |
| BHP-090 | Focus well, prune subtree, keyboard map, `role="tree"` mirror |
| BHP-091 | Export PNG / SVG / `mindmap.json` |
| BHP-092 | 500-node perf pass (≥ 55 fps) — INV-066 |

**Exit:** the live map is watchable and smooth at 500 nodes; the fact gate blocks a seeded
bad claim.

---

## S5 · Memory · `BHP-100…118`

Embeddings (`fastembed`) · `sqlite-vec` · Tantivy · hybrid retrieval (0.6/0.4) · gist writer
with mandatory dead ends · entity graph + links · decay tick · learning loops (source
reputation, query effectiveness, interest graph, style memory) · re-verify rule (INV-014) ·
Settings › Mind (constellation, session ribbon, coverage heat, inspector, wipe).

**Exit:** ≥ 30 % fewer redundant fetches on the paired second run; gist and retrieval token
caps hold; delete-entity is atomic.

---

## S6 · Vision · `BHP-130…152`

Sourcing ladder with licence resolution · vision understanding JSON · reject rules · EXIF
strip · phash dedupe · saliency + `safe_crop_region` reconciliation · crop set · Lanczos3 ·
AVIF/WebP/JPEG · srcset · placement rules · attribution rendering · **engine-generated
diagrams** as the always-available fallback.

**Exit:** every image in a generated post has a licence row and a geometry-verified crop; a
post whose images all fail licence resolution still builds.

---

## S7 · Writer + SEO · `BHP-160…184`, `BHP-190…202`

Section-by-section composition with running context · 12 headlines · 5-strategy hook engine +
scoring · weld pass · editor pass on a different provider · **Rust style linter** (hard
failures list) · structure enforcement · keywords + placement contract · metadata + JSON-LD ·
AI disclosure · internal linker (out and back-insertion, recorded).

**Exit:** zero hard lint failures across 30 generated posts; hook preference ≥ 70 % blind.

---

## S8 · Publish + theme · `BHP-203…216`

Static generator (`minijinja`) · React SSG renderer over the same `post.json` · `DeployTarget`
adapters (local, git/Pages, Netlify, Cloudflare, WordPress) · atomic build-verify-swap ·
rollback · pre-publish verification (blocking) · `themes/bhippi-default` (all 10 page types
in `04-PAGES.md` Part B) · **methodology page** · sitemap/RSS/JSON feed/robots/301 map.

**Exit:** Lighthouse SEO ≥ 95 / Perf ≥ 90 on 10 posts; rollback restores the previous site.

---

## S9 · Ticker + automation · `BHP-220…272`

Feed poller with ETag/backoff/circuit breakers · canonicalise + classify + categorise ·
clustering · burst + velocity · priority score · ticker strip UI with pause and reduced-motion
· detail popover with three actions · trigger contract (all nine conditions + debounce) ·
scheduler + persisted job queue + dead letters · every guardrail in spec §16.3 · review queue
UI · kill switch.

**Exit:** one wire story ⇒ one session; 24 h soak clean; kill switch ≤ 3 s.

---

## S10 · Skills · `BHP-280…308`

Manifest + registry · Rhai runtime · WASM/WASI p2 runtime with fuel and preopens · host API ·
observation → propose → evaluate → trial → enable → monitor · autonomy gates with user
approval and diff view · Skills UI · red-team suite.

**Exit:** red team green; a real engine-authored skill beats its baseline in trial.

---

## S11 · Hardening + beta · `BHP-320…386`

72 h soak · every §24 performance budget · accessibility pass (axe ≥ 95) · error-copy pass
(every error has a fix) · `bhippi doctor` complete · CLI parity audit · installers + signing ·
docs and the methodology page reviewed by a human.

**Exit:** all budgets met; zero P0/P1 open.

---

## Engine track · Game-engine workbench · `ENG-000…199` (ADR-0028)

A parallel capability track shipping inside the ADE shell (workbench mode 3). The authoritative
ticket list and acceptance evidence are in `13-ENGINE-AI-CONTROL-AND-UNREAL-UX-PLAN.md`.
ADR-0028 replaced the old Bevy child-process spike; do not resurrect retired attach work.

| Phase | Tickets | Exit |
|---|---|---|
| P0–P3 · Truth and authored formats | ENG-100…139 | one Rust transaction/journal truth; typed scene/content/HUD formats; deterministic round trips |
| P4 · Editor UX | ENG-140…152 | keyboard-accessible Outliner, Details, Content Browser, viewport tools, command/log surfaces |
| P5 · Rendering truth | ENG-160…168 | webview renders resolved meshes/materials/lights; missing assets loud; GPU budget separate |
| P6 · Playable runtime | ENG-170…180 | disposable deterministic play clone, input/physics/scripts/HUD/travel; Stop byte-identical |
| P7 · Autonomous verification | ENG-185…192 | bounded plan→act→playtest→capture→repair; capability/lease safety; offline golden transcript |
| P8 · Hardening | ENG-195…199 | headless + GPU budgets, recovery, axe matrix, docs and deterministic/host E2E lanes |

## Cross-sprint standing work

| Item | Cadence |
|---|---|
| Golden topics (20 × 4 tiers) | nightly from S3 |
| Fixture regeneration | when a source layout changes |
| Prompt version bumps | every prompt edit, hash pinned |
| Red-team corpus growth | every sprint from S3 |
| `PROGRESS.md` update | every session |
| ADRs for §15 open questions | before the sprint that needs them |
| Engine GPU reference run | every renderer/Three.js change and before release |

---

## The three "do not skip" items

1. **Session replay (BHP-007) in S0.** Without it, every quality regression from S3 onward is
   guesswork.
2. **Checkpointing (BHP-071) in S3.** Retrofitting resumability into a loop that already
   works is a rewrite.
3. **The gates before the automation.** Ticker and Timer (S9) must never land before the fact
   gate (S4), the licence gate (S6), and the verification gate (S8) — automation without gates
   publishes something indefensible on day one.
