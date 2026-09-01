# Bhippi — Module Contracts
**Doc:** `02-MODULE-CONTRACTS.md` · **Derives from:** spec §8–§17 · **Status:** authoritative

One section per crate. Each gives the crate's single responsibility, its public surface, the
invariants it enforces, what it must never do, and how it is tested. **Build against these
signatures, not against your memory of the spec.** Signatures may be refined during
implementation; a change to a *contract* (inputs, outputs, guarantees) needs an ADR.

Every async fn below returns `Result<T, BhippiError>` unless stated.

---

## M0 · `bhippi-types` (L0)

**Owns:** shared vocabulary. No IO, no tokio, no sqlx, no reqwest.

```rust
pub struct SessionId(Ulid); // + NodeId, DotId, SourceId, ImageId, PostId, SkillId, ProviderId
pub enum Tier { X2, X6, X12, X24 }
pub enum Stage { Planning, Expanding, Synthesising, FactCheck, Writing, Imaging, Seo, Review, Publishing, Done, Failed, Cancelled, Rejected }
pub enum Origin { Manual, Timer, Ticker, Skill }
pub enum TaskClass { Planner, Expander, Extractor, Classifier, Vision, Writer, Editor, SkillAuthor }
pub enum NodeKind { Concept, Entity, Claim, Question, Counterpoint, Timeline, Metric, SourceCluster }
pub enum Relation { Causes, Enables, CompetesWith, PartOf, Contradicts, Precedes, FundedBy, BuiltOn, BenchmarksAgainst }
pub struct TierBudget { pub max_hop: u8, pub expansions: u8, pub branch: u8,
                        pub sources: RangeInclusive<u16>, pub min_tier2: u16, pub min_primary: u16,
                        pub target_dots: u16, pub counter_passes: u8, pub timeline: bool,
                        pub entity_deep_dives: u8, pub wall: Duration, pub tokens: u64,
                        pub words: RangeInclusive<u16> }
impl Tier { pub const fn budget(self) -> TierBudget }   // the table in spec §10.1, in code, once
pub enum Event { /* §6 of 01-ARCHITECTURE */ }
pub enum BhippiError { /* §7 of 01-ARCHITECTURE */ }
```

**Invariants:** the tier table exists in exactly one place — here. Anyone hardcoding "12
expansions" anywhere else is deleted in review.
**Tests:** pure unit; `budget()` snapshot test against the spec table.

---

## M1 · `bhippi-providers` (L1) — spec §8 · BHP-010…024

**Owns:** finding LLMs, describing them, calling them, failing over.

```rust
pub async fn detect(cfg: &ProvidersConfig, tx: EventSender) -> Vec<ProviderInfo>;
pub struct Registry { /* live set + health */ }
impl Registry {
    pub fn router(&self, routing: Routing) -> Arc<dyn ProviderRouter>;
    pub async fn rescan(&self) -> Vec<ProviderInfo>;
    pub async fn probe_caps(&self, id: &ProviderId) -> Capabilities;
    pub fn set_enabled(&self, id: &ProviderId, on: bool);
    pub fn add_manual(&self, spec: ManualProviderSpec) -> Result<ProviderId>;
}
pub trait Provider  { /* spec §8.3 */ }
pub trait ProviderRouter { /* 01-ARCHITECTURE §9 */ }
```

**Detection (all four run concurrently, results merged, ≤ 1.5 s, non-blocking):**
CLI on `PATH` · vendor config dirs (model names only) · env var presence · loopback port
probe (11434 / 1234 / 8080 / 8000 / 1337 / 5000 + user URLs), 400 ms per port.

**Invariants**
- `INV-001` local-only / offline mode never opens a network socket for inference; it fails
  loudly with an actionable hint instead of falling back to cloud.
- `INV-002` credential *values* are never read from vendor config dirs, never copied into
  Bhippi storage, never logged. Presence only.
- `INV-008` `Editor` resolves to a different provider instance than `Writer` whenever ≥ 2
  healthy providers exist.
- `INV-003` provider CLIs are spawned with explicit argv, scrubbed env, and a timeout —
  never through a shell string.
- Fallback chain: same provider once with backoff → next candidate → max 3 providers, every
  hop logged and surfaced as an event.

**Never:** persist rows itself (returns values; `core` writes them), reach into `db`.
**Tests:** contract-conformance suite every backend must pass; a fake provider that fails
mid-stream proves clean failover; a no-network integration test proves `INV-001`.

---

## M2 · `bhippi-harvest` (L2) — spec §9 · BHP-030…048

**Owns:** getting bytes off the web, politely, and turning them into clean text.

```rust
pub struct Harvester { /* client, robots cache, governors, blob store */ }
impl Harvester {
    pub async fn fetch(&self, url: &Url, ctx: FetchCtx) -> Result<Fetched>;
    pub async fn extract(&self, raw: Fetched) -> Result<Extracted>;
    pub async fn harvest(&self, urls: Vec<Url>, ctx: FetchCtx) -> Vec<Result<Extracted>>; // rate-limited fan-out
    pub fn dedupe(&self, cand: &Extracted, seen: &SeenIndex) -> DupeVerdict;
}
pub trait SearchBackend { async fn search(&self, q: &str, n: usize) -> Result<Vec<Hit>>; }
pub struct Extracted { pub canonical_url: Url, pub title: Option<String>, pub author: Option<String>,
                       pub published_at: Option<Timestamp>, pub text_md: String,
                       pub blocks: Vec<Block>, pub images: Vec<ImageCandidate>,
                       pub links: Vec<OutLink>, pub content_hash: String, pub simhash: u64,
                       pub lang: String, pub paywalled: bool, pub thin: bool }
```

**Pipeline:** charset normalise → boilerplate strip (`dom_smoothie`) → JSON-LD/OG/byline
metadata → markdown-normalised main text (headings, tables, code preserved) → image
candidates → outbound link inventory → blob write + `content_hash` + `simhash`.

**Invariants**
- `INV-004` `robots.txt` fetched, cached 12 h, obeyed. **No override exists in the type
  system** — there is no bypass flag to set. A PR adding one is rejected.
- `INV-005` paywall detected ⇒ record `paywalled`, keep only free abstract/metadata, stop.
  No archive mirrors, no cookie tricks, no reader-mode bypass.
- `INV-006` honest UA `BhippiBot/1.0 (+https://bhippi.example/bot)`; 0.5 rps/host; 1 conn per
  host; `Crawl-delay`, `429`/`503 Retry-After` respected exactly.
- 8 s connect / 20 s total; 3 retries on 5xx/timeout with jitter; 0 retries on 4xx; 4 MB cap
  with streaming abort.
- Headless (`chromiumoxide`) is a *fallback only*: text < 400 chars **and** ≥ 8 script tags,
  15 s budget, ≤ 15 % of a session's fetches.
- Dedupe order is canonical URL → `content_hash` → `simhash` (Hamming ≤ 3); the highest
  `trust_tier` copy wins and the rest become corroborations.

**Discovery priority:** registered feeds → search backend → link following → **primary-source
jump** (paper/benchmark/filing/changelog/model card/official blog referenced by an article
must be fetched and preferred).

**Never:** decide research meaning, score relevance, or call the Writer.
**Tests:** 50 frozen pages, extraction F1 ≥ 0.92; robots-disallow test asserts zero requests;
429 backoff test; wire-story collapse test.

---

## M3 · `bhippi-research` (L2) — spec §10 · BHP-060…092

**Owns:** the charter, the frontier, dots, the mind map, the blueprint, the fact gate.
**This is the product.** Everything else is plumbing.

```rust
pub async fn plan(seed: &Seed, memory: &PriorKnowledge, r: &dyn ProviderRouter) -> Result<Charter>;
pub async fn expand_once(state: &mut MindMap, node: NodeId, deps: ExpandDeps) -> Result<Expansion>;
pub fn score_frontier(map: &MindMap, charter: &Charter, cfg: &ScoringCfg) -> Vec<(NodeId, f32)>;
pub fn drift_guard(child: &Node, seed_emb: &[f32], cfg: &ScoringCfg) -> DriftVerdict;
pub async fn extract_dots(node: &Node, src: &Extracted, r: &dyn ProviderRouter) -> Result<Vec<Dot>>;
pub async fn synthesise(map: &MindMap, mem: &PriorKnowledge, r: &dyn ProviderRouter) -> Result<Blueprint>;
pub async fn fact_check(bp: &Blueprint, map: &MindMap, r: &dyn ProviderRouter) -> Result<FactReport>;
pub fn layout_step(map: &MindMap, prev: &Positions) -> Positions;   // Barnes-Hut, blocking pool
```

**Frontier priority** (constants live in `research.toml`, never hardcoded):

```
priority = 0.35*relevance + 0.25*novelty + 0.20*gap_fill
         + 0.10*authority_potential + 0.10*recency_pressure - 0.15*cost_estimate
```

**Invariants**
- `INV-007` domain gate: `in_scope == false` or `score < reject_threshold` ⇒ abort with a
  user-facing message. Never "try anyway".
- `INV-009` anti-drift: every child holds cosine ≥ 0.45 to the seed embedding, or is
  explicitly justified as a required counterpoint/prerequisite. Otherwise pruned with
  reason `drift`.
- `INV-010` a dot without `source_id` + character offsets is **dropped, never repaired**.
- `INV-011` quotes ≤ 15 words, at most one per source — enforced in code at extraction
  *and* at lint, not by prompt.
- `INV-012` `unknowns` is non-empty for X12/X24 blueprints.
- `INV-013` unresolved contradictions appear in the article; they are never silently
  resolved.
- Loop guard: a normalised label already explored is rejected; ≤ 3 siblings may share a
  parent's entity.
- Counter-evidence passes run at the count the tier demands; a sunny-only post is a defect.
- Arithmetic in the fact gate is recomputed **in Rust**, never trusted from the model.

**Never:** write to the DB, fetch directly (it asks `harvest`), or produce prose.
**Tests:** golden 20 topics × 4 tiers; drift ≤ 2 % at hop ≥ 3 by human raters; seeded false
claim must be caught by the gate; kill-and-resume produces no duplicate fetches.

---

## M4 · `bhippi-memory` (L2) — spec §11 · BHP-100…118

**Owns:** getting better over time.

```rust
pub async fn retrieve(seed: &Seed, k: usize) -> Result<PriorKnowledge>;   // 0.6 vector + 0.4 BM25
pub async fn write_gist(session: &SessionSummary, r: &dyn ProviderRouter) -> Result<Gist>;
pub fn decay_tick(now: Timestamp) -> DecayReport;                        // 0.5^(idle/half_life)
pub async fn upsert_entities(dots: &[Dot]) -> Result<Vec<EntityId>>;
pub fn learned_trust(domain: &str) -> f32;                               // bounded +/- 1 tier
pub fn record_query_outcome(q: &str, yielded_tier: u8);
pub fn interest_graph() -> InterestGraph;                                // feeds Timer picker
pub async fn forget(target: ForgetTarget) -> Result<()>;                 // atomic across DB+FTS+vec
```

**Invariants**
- `INV-014` memory is a **prior to verify, never ground truth to repeat**. Anything from
  memory that reaches a published post must be re-verified against a live source fetched in
  the current run.
- Gists ≤ 1200 tokens; the injected `PRIOR KNOWLEDGE` block ≤ 6 % of the planner's context.
- Dead ends are **mandatory** in every gist — negative knowledge is the highest-value part.
- `forget` removes graph rows, referencing gists, FTS docs and vectors in one transaction.
- Below `decay_score` 0.15 and unpinned ⇒ archived after 180 days, not deleted.

**Tests:** paired-topic set shows ≥ 30 % fewer redundant fetches on the second run; delete-
entity atomicity test; token-cap test on gists and retrieval block.

---

## M5 · `bhippi-vision` (L2) — spec §12 · BHP-130…152

**Owns:** every image that ships, and its licence.

```rust
pub async fn source_candidates(intent: &ImageIntent, deps: VisionDeps) -> Vec<Candidate>;
pub async fn understand(c: &Candidate, r: &dyn ProviderRouter) -> Result<Understanding>;
pub fn saliency(img: &DynamicImage) -> SaliencyMap;                  // blocking pool
pub fn crop_set(img: &DynamicImage, u: &Understanding) -> Result<Variants>;
pub async fn render_diagram(spec: &ChartSpec) -> Result<Svg>;        // engine's own figures
```

**Sourcing order:** press kits → open-licence repositories → open-access paper figures under
quotation (captioned with paper/authors/year) → **engine-generated diagrams** (often the
best answer).

**Invariants**
- `INV-015` `license = 'unknown'` ⇒ `status = 'rejected'`, and the publisher refuses to
  build a post containing a rejected image. Attribution string stored verbatim.
- `INV-016` no crop cuts through `safe_crop_region`; never upscale beyond source; diagrams
  and screenshots are **letterboxed, never cropped**; portraits keep headroom.
- No images of identifiable private individuals; public figures only from press kits or
  open-licence archives. No hotlinking. No watermarked stock previews.
- Reject when `relevance < 0.55`, `sharpness < 0.35`, upscaling artefacts, unreadable text
  after downscale, or any `concerns` entry naming a private individual.
- EXIF stripped; `phash` dedupe (Hamming ≤ 6) against existing site images.
- Variants: `hero_16x9` 1600×900, `card_4x3` 800×600, `og_1200x630` (title-safe), `inline_3x2`
  1200×800, `thumb_1x1` 400×400; AVIF q60 + WebP q78 + JPEG q82; srcset [400, 800, 1200, 1600].

**Tests:** geometric assertion that no published crop intersects a safe region; a post whose
images all fail licence resolution still builds using generated diagrams; hero LCP ≤ 1.8 s.

---

## M6 · `bhippi-writer` (L2) — spec §13 · BHP-160…184

**Owns:** prose, and refusing bad prose.

```rust
pub async fn headlines(bp: &Blueprint, r: &dyn ProviderRouter) -> Result<Vec<Headline>>;  // 12
pub async fn hooks(bp: &Blueprint, r: &dyn ProviderRouter) -> Result<Vec<Hook>>;          // 5 strategies
pub fn score_hook(h: &Hook, dots: &[Dot]) -> HookScore;                                   // Rust part
pub async fn draft_section(s: &SectionPlan, ctx: &RunningCtx, r: &dyn ProviderRouter) -> Result<Section>;
pub async fn weld(sections: &mut [Section], r: &dyn ProviderRouter) -> Result<()>;
pub async fn editor_pass(draft: &Draft, r: &dyn ProviderRouter) -> Result<EditorReport>;  // different provider
pub fn lint(draft: &Draft) -> LintReport;                                                 // deterministic
```

**Composition:** section by section — each section receives only its own dots plus a
200-token running context. Never one giant prompt.

**Invariants**
- `INV-017` zero style-linter **hard** failures may reach publish. Hard: banned phrases ·
  avg sentence > 24 words · paragraph > 5 sentences · passive > 20 % · quote ≥ 15 words ·
  > 1 quote per source · > 2 consecutive sentences starting with the same word · em-dash
  density > 1/120 words · heading not in sentence case · claim without a resolvable dot id ·
  promotional adjective stack.
- `INV-018` every paragraph maps to ≥ 1 dot; orphan paragraphs fail the build.
- Hook truthfulness: the hook's claim maps to a dot with confidence ≥ 0.8.
- Banned openers are a **build failure, not a warning** (spec §13.2 list).
- Structure per spec §13.3, including mandatory "What's disputed" when contradictions exist
  and "What we still don't know" at X12+.

**Never:** fetch, score sources, or touch SEO metadata.
**Tests:** 30 generated posts with zero hard failures; blind hook preference ≥ 70 % vs.
baseline lead paragraph.

---

## M7 · `bhippi-seo` + `bhippi-publish` (L2) — spec §14 · BHP-190…216

```rust
// seo
pub async fn keywords(bp: &Blueprint, corpus: &CorpusIndex, r: &dyn ProviderRouter) -> Result<KeywordSet>;
pub fn metadata(post: &Post, kw: &KeywordSet) -> Metadata;      // title, dek, canonical, OG, JSON-LD
pub fn internal_links(new: &Post, corpus: &CorpusIndex) -> LinkPlan;  // 2-4 out, 1-2 in, revertible
pub fn slug(title: &str) -> String;                              // kebab, <= 60, stopwords out, date-free

// publish
pub fn build(post_set: &[PostJson], theme: &Theme, r: &dyn SiteRenderer) -> Result<SiteBundle>;
pub fn verify(b: &SiteBundle) -> Result<VerifyReport>;           // blocking gate
pub trait DeployTarget { async fn preflight(&self)->Result<()>;
                         async fn publish(&self,&SiteBundle)->Result<DeployRef>;
                         async fn rollback(&self,&DeployRef)->Result<()>; }
```

**Invariants**
- `INV-019` **AI disclosure** is machine-readable and visible on every post, with review
  status, and is **not removable in the UI**.
- `INV-022` publish is atomic: build to temp → verify → swap → record `deploy_ref`. Power
  loss leaves the old site or the new one, never a half-written one. Rollback is one command.
- `INV-023` the build **fails** (not warns) on: broken internal link · missing image variant
  · unresolved image licence · style hard failure · `fact_score < 70` without human approval
  · duplicate slug · missing meta description · missing disclosure · Lighthouse SEO < 95 or
  Performance < 90.
- `INV-024` no content logic in either renderer's template layer; both consume identical
  `post.json` (Appendix A). The content model is the contract; the renderer is swappable.
- Keyword density outside 0.6–1.6 % fails lint. Slugs are date-free so evergreen posts update
  in place; slug changes write a 301 map entry.
- Internal link insertions into older posts are recorded so they can be reverted.

**Theme budgets:** ≤ 40 KB CSS, ≤ 25 KB JS per article route, zero third-party scripts, zero
cookies, no analytics by default. Content column 68ch, dark-primary, one accent.

---

## M8 · `bhippi-ticker` (L2) — spec §15 · BHP-220…246

```rust
pub struct Ticker { /* feeds, ETag cache, circuit breakers */ }
impl Ticker {
    pub async fn poll_tick(&self) -> Vec<TickerEvent>;
    pub fn cluster(&self, items: &[Item]) -> Vec<Cluster>;   // simhash + entity overlap + 6h window
    pub fn priority(&self, c: &Cluster, interest: &InterestGraph) -> f32;   // 0-100
    pub fn should_trigger(&self, e: &TickerEvent, guards: &Guards) -> TriggerVerdict;
}
```

**Priority:** `28*trust_max + 24*burst + 18*velocity + 14*interest + 10*novelty +
6*primary_available - 20*recently_covered`.

**Invariants**
- `INV-025` auto-trigger requires **all** of: priority ≥ `auto_trigger_score` · burst ≥
  `burst_sources` **or** single tier-1 primary · `domain_score ≥ reject_threshold` · no
  existing post covers the cluster · daily cap not reached · outside quiet hours · budget
  green · no running session for that cluster · **cluster stable for 5 minutes**.
- Zero API keys must still produce a working ticker from RSS alone.
- Per-feed circuit breaker after 5 consecutive errors, surfaced in Settings, never hidden.
- Polling staggered; ETag / `If-Modified-Since` honoured.

**Tests:** one wire story across 6 outlets ⇒ exactly one cluster and one candidate session; a
correction inside the debounce window produces no post; 25 feeds ⇒ CPU < 2 %, +RSS < 40 MB.

---

## M9 · `bhippi-skills` (L2) — spec §17 · BHP-280…308

```rust
pub fn registry() -> SkillRegistry;
pub async fn propose(observation: &RepeatedProcedure, r: &dyn ProviderRouter) -> Result<SkillDraft>;
pub async fn evaluate(skill: &Skill, set: &EvalSet) -> Result<EvalScore>;
pub async fn invoke(skill: &Skill, input: Value, host: &HostApi) -> Result<Value>;
pub trait SkillRuntime { /* Rhai, WASM */ }
```

**Lifecycle:** OBSERVE (≥ 5 repeats, stable shape) → PROPOSE (manifest + body + ≥ 10 real
eval cases) → EVALUATE (sandbox) → TRIAL (score ≥ `min_score`, shadow-compared 20 runs) →
ENABLE (win-rate ≥ 60 % + autonomy gate) → MONITOR (3 failures or −15 % ⇒ auto-quarantine).

**Invariants**
- `INV-026` `script` skills, and any skill requesting `net` or `fs_write`, **require explicit
  user approval** before leaving trial. No silent mode. The user sees a diff of what the
  engine wrote.
- `INV-027` sandbox limits: Rhai — no fs, no net, whitelisted stdlib, 200 ms CPU, 8 MB, op
  counter. WASM — capability-based preopens only, fuel-metered, 2 s wall, 64 MB.
- `INV-028` skills never see API keys, the keychain, the DB handle, the user's filesystem
  outside the session scratch dir, or raw provider clients. Only the narrow audited host API.
- Every invocation logged with hashed inputs, duration, status, version.
- A broken skill never blocks a session; the baseline path always remains.

**Tests:** red-team CI proves no unapproved net/fs reach; deliberately broken skill is
quarantined within 3 runs; invocation overhead < 15 ms.

---

## M10 · `bhippi-core` (L3) — spec §16 · BHP-250…272

**Owns:** the session FSM, the job queue, budgets, automation policy, the kill switch, and
the event bus. The only crate allowed to write session state.

```rust
pub struct Engine { /* repos, registry, bus, scheduler, tokens */ }
impl Engine {
    pub async fn start_session(&self, seed: Seed, tier: Tier, origin: Origin) -> Result<SessionId>;
    pub async fn resume(&self, id: SessionId) -> Result<()>;
    pub async fn pause(&self, id: SessionId) / resume_run / cancel(&self, id) -> Result<()>;
    pub async fn focus_node(&self, id: SessionId, node: NodeId) -> Result<()>;  // user boosts a branch
    pub fn budget_guard(&self) -> &BudgetGuard;                                 // checked BEFORE every call
    pub async fn enqueue(&self, job: Job) -> Result<JobId>;                     // bounded 32, SQLite-backed
    pub async fn kill_switch(&self) -> Result<()>;                              // <= 3 s, consistent DB
    pub fn subscribe(&self) -> BroadcastReceiver<Event>;
}
```

**Guardrails (all HARD REQ, spec §16.3):** daily post cap · quiet hours · budget guard ·
duplicate guard (slug + embedding ≥ 0.93 ⇒ refresh, not a new post) · review gate ·
thin-evidence forced review · kill switch · crash-loop guard (3 failures ⇒ automation
disables itself and reports why).

**Timer picker order:** uncovered ticker cluster (24 h) → coverage-heat gap → scheduled
refresh of a post > 30 days old whose entities moved (update in place, never a near
duplicate) → user queue. Never a topic covered in the last 14 days unless `refresh`.

**Invariants**
- `INV-029` exactly one research session runs at a time.
- `INV-030` the job queue is SQLite-persisted and idempotent; 3 failures ⇒ dead-letter row
  surfaced in the UI as an inspectable, requeueable card.
- `INV-031` thin evidence (below tier floors) ⇒ forced review **regardless of settings**;
  never auto-publish an under-evidenced post.

**Tests:** 72 h soak (no crash, no duplicates, caps exact, +RSS < 150 MB); kill switch stops
all work in ≤ 3 s with no orphan rows or temp dirs; power-loss-during-publish test.

---

## M11 · `bhippi-db` (L1) — spec §7

**Owns:** schema, migrations, repositories, indexes, blob store. See `03-DATA-MODEL.md`.
**Invariants:** no SQL outside this crate · single writer · forward-only migrations ·
compile-time checked queries · every multi-table write in one transaction.

---

## M12 · `bhippi-app` (L4) + `ui/` (L5) — spec §18, §19, §25

**Owns:** the Tauri command surface, the CLI, tray, single-instance guard, and the four
screens. See `04-PAGES.md` for every screen and `05-PIPELINES.md` for what each control
triggers.

**Invariants**
- `INV-032` `ui/src/lib/ipc.ts` is **generated** from Rust via `specta`. Hand-written IPC
  types are forbidden and CI fails on a dirty generated file.
- `INV-033` CLI/GUI parity: everything the GUI can do, the CLI can do (spec Appendix D).
- `INV-034` accessibility floor: keyboard reachable everywhere, visible focus rings, AA
  contrast, ticker pausable and reduced-motion aware, mind map mirrored as a
  `role="tree"` list, no colour-only meaning.

---

## M13 · `bhippi-engine` (L2) — ADR-0020 · ENG-000-series

**Owns:** the game-project domain. `Bhippi.game.toml` manifest parse/validate; new-game
scaffold template; the authoritative in-memory **scene document** (`.bscn.json`); the
**transaction system** (apply/inverse/undo/redo, interactive coalescing) — the *only* write
path for scene data (human, UI, or AI); the **asset index** (ULID ⇄ path ⇄ blake3 hash,
meta sidecars, license field); the **schema registry** (reflection-derived component
catalog with field types/ranges/docs); the **Engine Mind Map** generator + ≤1.5k-token
digest; the `engine.query`/`engine_action` op handlers; build preflight inputs; mind-map
and transaction facts for `bhippi-types::Event::Engine`.

**Guarantees**
- Pure editor-domain library: **no windowing, no rendering, no wgpu/winit and no Bevy.**
  ADR-0028 retired the child-process viewport; `cargo test` is fully headless.
- `INV-070` every scene mutation is a `Transaction` validated against the schema; a
  caught-by-construction escape is impossible. `INV-071` every applied transaction produces
  a journalable record (scene, actor, label, ops).
- `INV-073` all engine math and state live here and are testable with zero database and
  zero GPU (`bhippi-engine` tests run headless in CI).
- Deterministic sorted-key JSON serialisation for `.bscn.json` and `.bprefab.json`
  (diff-friendly, byte-identical re-save test).

---

## M14 · `bhippi-engine-build` (L2) — ADR-0020 · ENG-060…064

**Owns:** build orchestration for Windows / macOS / Linux / Android / iOS / Web. Shared
pipeline (preflight → asset compile → codegen `game/` shell → compile → package+sign →
ledger), per-target specifics, the **toolchain doctor**, cancellation (one build at a time,
kill-switch via the existing cancellation-token pattern), and the artifact ledger rows
(`engine_builds`).

**Guarantees**
- `INV-074` preflight **fails** a Release build that contains any asset with
  `license = "unknown"`; Debug builds warn-list. Gates block, never warn.
- All toolchain invocations are explicit-argv child processes with scrubbed env and
  timeouts (the provider-spawn hygiene of INV-003); signing secrets come only from the
  keychain (`C11`/`INV-037`).
- Every build emits coalesced `BuildProgress` / `BuildFinished` events and records a ledger
  row (hash, size, duration).

---

## M15 · Webview viewport + play runtime — ADR-0028 · ENG-160…180

**Owns:** Three.js rendering, editor camera navigation and raycast picking in
`ui/src/engine/EngineViewport.tsx`; the isolated in-pane simulation in `playRuntime.ts`;
named input consumption from validated `bhippi-input@1`; runtime HUD binding/actions;
camera possession/eject; pause/step/restart/time scale; and play diagnostics. Scene
composition, schemas, asset/material resolution and input-document validation remain Rust.

**Guarantees**
- `INV-070` edit-mode writes still cross the Rust transaction seam only.
- `INV-081` play owns a clone; Stop discards it and never writes an authored scene.
- `INV-077` targets this webview renderer. Rust measures the engine-side 1k-entity budget;
  browser fps is a separate reference-GPU gate and may not be inferred from headless time.
- `bhippi-engine-viewport/protocol.rs` is retained as an unused design only. The Bevy child
  process and its old INV-072/INV-078 contracts are retired by ADR-0028.

---

## M16 · `bhippi-engine` × `chat.rs` — the AI action channel (ADR-0020 · ENG-040…044)

**Owns:** the `<engine_action>{json}</engine_action>` inline-tag parser (relaxed-JSON
tolerated, same seam as `<computer_action>` and `<write_file>`), routing reads to queries
and writes to `bhippi-engine` transactions with `actor: Agent`, the permission-gate mapping
(queries/screenshots always allowed; deletes/batches/build prompt), the ≤5 txn/s agent rate
limit, `EngineActionApplied` facts, and the ActivityDock step rendering contract.

**Prompt** in `prompts/chat-engine.md` (versioned, INV-035). Doctrine: coordinate
conventions, query-before-edit, small labelled transactions, screenshot-after-visual-change.

**Observation and autonomous verification (ENG-185…191).** `chat.rs` owns a maximum-six-round
model loop. `engine/observation.rs` is the only cross-boundary seam: a typed one-shot request
id asks the active pane for an exact viewport PNG or a fixed-step scripted playtest. Rust
validates camera names, PNG/IHDR dimensions, byte/dimension caps, step/key/frame caps and
timeout; late or duplicate responses are rejected. The webview may render/copy/simulate but
never mutates authored data. Screenshot paths attach to the next multimodal request. Typed
failures re-enter the loop; a repeated patch, structural fault or round cap exits with the
unresolved remedy. Dynamic facts are capped at `ENGINE_CONTEXT_TOKEN_BUDGET`; deeper scene
facts are retrieved with `engine_query`, never injected as a whole-scene dump.

## M17 · Gameplay scripts — ADR-0030 · ENG-176

**`bhippi-engine::script`** — the compiler. `compile(file, source) -> Result<ScriptProgram,
ScriptFault>` lexes, parses and validates a documented subset of Rhai and emits bytecode:
a flat `Vec<Instr>` (`op`, `a`, `b`, `line`), constant tables for numbers and strings, a
function table, the hooks the file defines, and the host functions it calls **by name**.
`HOST_FNS` is the complete host surface; `host_reference()` renders it as text. Faults carry
file, line, column and a hint. Nothing here touches the filesystem or a GPU.

**`ui/src/engine/scriptVm.ts`** — the VM. Executes a `ScriptProgram` against a
`ScriptHostTable` keyed by the names the program recorded, under the program's own step
budget and call-depth cap. It has no parser, no `eval` and no `Function` (INV-082). Faults are
returned, never thrown.

**`bhippi-app::engine`** — `engine_play_world` compiles every `ScriptRef` in the composed
world and returns `scripts: Vec<EngineCompiledScript>` plus `script_faults`. A file that will
not compile does not stop Play: that entity runs unscripted and the fault reaches the Output
Log with its line.

**Contract:** the host-name list is the ABI, not the table order. Adding a host function in
Rust and not implementing it in the VM is reported by `PlayRuntime::unboundHosts()` before a
script trips over it.

## M18 · Agent capabilities and scene leases — ENG-190 / ENG-192

**`bhippi-engine::capability`** — `Capability` (7) × `Decision` (allow/ask/deny),
`CapabilityPolicy` serialised as `[agent]` in `Bhippi.game.toml` (absent keys take their
default, unknown keys are **refused** by `validate()`), `capability_for(kind)` mapping every
action kind, and `evaluate(policy, kinds) -> CapabilityVerdict`.

**`bhippi-app::engine`** — `capability_verdict()` reads the project's policy;
`apply_batch_in_workspace` refuses an agent batch whose verdict names a denied capability, and
never gates `EngineActor::User`. `engine_agent_capabilities` / `engine_set_agent_capability`
are the panel's commands; `scaffold::format_manifest` is the single writer of the file.

**`bhippi-app::engine::session`** — `SceneLease { owner, taken_at, revision }` per open scene.
`BatchRequest` gains `owner` and `base_revision`. A batch is refused when a different live
lease holds the scene, or when the caller's own lease points at a revision the scene has moved
past. A user edit never blocks and never refreshes an agent's lease — that is the signal.

## M19 · Chat transcript record — `docs/14-CHAT-SURFACE-PLAN.md` · CHT-100…118

**`bhippi-app::chat`** — `ToolActivity` carries `command`, `output`, `exit_code`,
`elapsed_ms`, `truncated` and `changes: Vec<TurnFileChange>`. `ChatTurnView` carries
`worked_ms`, `changes: Option<TurnChanges>` and `notices: Vec<TurnNotice>`.
`finish_tool_with(turn, tool, state, ToolResult)` is the recording close; `cap_tool_output`
enforces `TOOL_OUTPUT_CAP` at capture, UTF-8-safely, keeping both ends; `line_change` computes
real additions/deletions by LCS; `TurnChanges::from_tools` folds per-file.
`undo_chat_turn` / `chat_turn_undoable` restore a turn's writes from a budgeted, session-scoped
snapshot (`TURN_UNDO_BUDGET`), evicting whole turns rather than parts of one.

**`ui/src/components/turnGrouping.ts`** — the pure half: `groupTools`, `labelFor`,
`formatDuration`. `TurnActivity.tsx` draws what it decides. The webview computes no counts,
no durations and no truncation (INV-051).
