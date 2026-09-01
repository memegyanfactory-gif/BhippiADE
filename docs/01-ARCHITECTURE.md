# Bhippi — System Architecture
**Doc:** `01-ARCHITECTURE.md` · **Derives from:** `00-SPEC-v1.0.md` §3, §4, §5
**Status:** authoritative · **Change control:** ADR required (`docs/adr/`)

This document turns the spec's layer diagram into buildable structure: what each crate
owns, which crate may call which, how work moves through the process, how state is
committed, and where the seams are. If the spec and this document disagree on *intent*,
the spec wins. If they disagree on *structure*, this document wins and the spec gets a
patch note.

---

## 1. Architectural shape in one paragraph

Bhippi is a **single-process, single-writer, event-driven desktop application**. One Tokio
runtime hosts every subsystem. A thin Tauri/React shell renders and takes input; it holds
no business state. A central **orchestrator** owns the only mutable session state machine
and is the only component allowed to advance a session's stage. Everything else is a
library that takes an input, does one job, and returns a value — no subsystem reaches
sideways into another subsystem's tables or another subsystem's provider client.
Persistence is SQLite in WAL mode with a **repository layer as the sole write path**, and
every stage transition is a committed checkpoint, which is what makes a run resumable
after a crash.

---

## 2. Layer model

```
L5  PRESENTATION   ui/ (React)  ·  themes/  ·  generated site
L4  APPLICATION    bhippi-app (Tauri commands, events, tray, single-instance, CLI)
L3  ORCHESTRATION  bhippi-core (session FSM, job queue, budget guard, event bus,
                                scheduler, kill switch, automation policy)
L2  CAPABILITY     research · harvest · memory · vision · writer · seo · ticker ·
                   skills · publish · engine · engine-build   (libraries: input in, value out)
                   engine-viewport is a leaf **binary** (S) that links the Bevy stack;
                   it is the only crate allowed to (ADR-0020)
L1  PLATFORM       bhippi-providers (LLM access)   ·   bhippi-db (persistence)
L0  FOUNDATION     bhippi-types (ids, errors, events, domain enums, budgets)
```

**Rule L-1 — downward calls only.** A crate may depend on crates in strictly lower layers.
No sideways dependency between L2 capability crates. No upward dependency, ever.

**Rule L-2 — L2 crates hold no session truth.** They may hold caches and clients, but
session state lives in SQLite and is read/written through L1.

**Rule L-3 — only L3 writes session state.** A capability crate returns a value; the
orchestrator decides what it means and commits it. This is why a capability crate can be
tested with zero database.

**Rule L-4 — L5 has no logic.** No scoring, no filtering, no formatting decisions, no
retry, no derived business values in TypeScript. If the UI needs a number, Rust computes
it and sends it.

---

## 3. Crate graph (allowed edges)

```
                         +---------------+
                         |  bhippi-app   |  L4
                         +-------+-------+
                                 |
                         +-------v-------+
                         |  bhippi-core  |  L3
                         +-------+-------+
+----------+----------+----+-----+----------+----------+---------+
       v          v          v          v          v          v         v
    research    harvest     memory     vision     writer      seo     ticker  skills  publish   (L2)
       |          |          |          |          |         |         |       |        |
       +----------+-----+----+----------+----------+---------+---------+-------+--------+
                        v
           +-----------------------+   +-------------+
           |   bhippi-providers    |   |  bhippi-db  |     L1
           +-----------+-----------+   +------+------+
                       +--------+-------------+
                                v
                        +---------------+
                        | bhippi-types  |                  L0
                        +---------------+

   bhippi-engine → bhippi-db · bhippi-engine-build → bhippi-engine · bhippi-app → engine + engine-build
   bhippi-engine-viewport → bhippi-engine (types-only feature)  + Bevy stack   (details §3.1, ADR-0020)
```

`bhippi-types` is a crate this document **adds** to the spec's list. Reason: `SessionId`,
`Tier`, `Budget`, `TaskClass`, `BhippiError`, and every event payload are needed by every
crate, and without a shared L0 the capability crates would be forced into sideways
dependencies just to share them. It contains **types and pure functions only** — no IO, no
tokio, no sqlx.

### 3.1 The dependency table (enforced in CI)

| Crate | May depend on | Must never depend on |
|---|---|---|
| `bhippi-types` | serde, thiserror, ulid, chrono | anything else in the workspace |
| `bhippi-db` | types | any L2/L3/L4 crate |
| `bhippi-providers` | types | db, any L2 crate |
| `bhippi-harvest` | types, db (blob store only) | research, writer, core |
| `bhippi-research` | types, providers, harvest, memory | writer, seo, publish, core |
| `bhippi-memory` | types, providers (embeddings), db, engine (scene graph, ADR-0024) | research, writer, core |
| `bhippi-vision` | types, providers, harvest (fetch only) | research, writer, core |
| `bhippi-writer` | types, providers | harvest, research, db |
| `bhippi-seo` | types, providers | harvest, publish |
| `bhippi-ticker` | types, harvest (fetch only), db | research, core |
| `bhippi-skills` | types, providers | core |
| `bhippi-publish` | types, seo | research, writer, core |
| `bhippi-core` | all of the above | ui |
| `bhippi-engine` | types, db (ADR-0020) | providers, any renderer/windowing crate |
| `bhippi-engine-build` | types, engine (ADR-0020) | providers, renderer/windowing crates |
| `bhippi-app` | core, types, providers + db (ADR-0008), engine + engine-build (ADR-0020) | any L2 crate directly other than engine/engine-build |
| `bhippi-engine-viewport` | engine via `types-only` feature only (ADR-0020); Bevy stack | any other workspace crate |

`bhippi-research` depending on `harvest` and `memory` is the one deliberate exception to
Rule L-1's "no sideways" clause, because the hop loop *is* the composition of discover →
harvest → extract. It is allowed in that direction only, and is called out here so it is
not copied as precedent.

**CI enforcement:** `tests/architecture.rs` parses every `Cargo.toml` and fails on an edge
not present in the table above. Adding an edge requires an ADR.

---

## 4. Process and concurrency model

### 4.1 One runtime, four pools

| Pool | Kind | Size | Work |
|---|---|---|---|
| Async default | tokio multi-thread | `min(cores, 8)` | orchestration, IO, provider streaming |
| Network semaphore | permits | `max_parallel_fetches` (6) | global fetch cap |
| Per-host governor | rate limiter | 0.5 rps / 1 conn | politeness |
| Blocking pool | `spawn_blocking` | tokio default | image encode, embeddings, simhash, layout physics, PDF, Tantivy commit |

**Rule C-1 — nothing CPU-bound on the async pool.** Image resize, AVIF/WebP encode,
`fastembed`, blake3/simhash over multi-MB text, Barnes-Hut layout, and Tantivy commits go
through `spawn_blocking`.

**Rule C-2 — exactly one research session at a time.** Enforced by a single permit held by
the orchestrator for the session's lifetime. Publishing, image work, and ticker polling may
overlap the *next* session's planning; a second expand loop may not start.

**Rule C-3 — single DB writer.** All writes funnel through repository methods on one
write connection; reads use a pool. Dots and sources insert in batches of 64. No `INSERT`
inside a loop.

**Rule C-4 — every network await has a timeout and a cancellation token.** Cancel
propagates from the kill switch to every in-flight future within 3 s.

### 4.2 Cancellation topology

```
kill_switch()  --->  CancellationToken (root)
                       |-- automation scheduler token
                       |-- session token   (cancel = graceful stop at checkpoint)
                       |     |-- discover token
                       |     |-- fetch tokens (per request)
                       |     +-- provider stream tokens
                       +-- ticker poll token
```

A cancelled stage **rolls back to its last checkpoint** and never leaves a half-written
stage: `status = 'cancelled'`, `stage_cursor` intact, temp dirs removed.

---

## 5. The session state machine

The spine of the product. `bhippi-core::session::Machine` owns it; nothing else may write
`sessions.status`.

```
                     +-----------+
    create --------->|  planning |
                     +-----+-----+
              domain gate  |  pass      fail --> rejected  (terminal, user message)
                           v
                     +-----------+   <--- primary resume point
                     | expanding |   (loop; checkpoint after EVERY expansion)
                     +-----+-----+
                           | frontier exhausted OR budget hit
                           v
                   +---------------+
                   | synthesising  |
                   +-------+-------+
                           v
                   +---------------+   fact_score < 70
                   |  fact_check   |-------------------+
                   +-------+-------+                   |
                           v                           |
                     +-----------+                     |
                     |  writing  |                     |
                     +-----+-----+                     |
                           v                           |
                     +-----------+                     |
                     |  imaging  |                     |
                     +-----+-----+                     |
                           v                           v
                     +---------+  gate on /     +--------------+
                     |   seo   |--thin evidence->|    review    |
                     +----+----+                 +------+-------+
                          | gate off                    | approve
                          v                             |
                   +--------------+  <------------------+
                   |  publishing  |
                   +------+-------+
                          v
                      +--------+     any stage error --> failed (resumable)
                      |  done  |     user cancel      --> cancelled
                      +--------+
```

**Checkpoint contract [INV-020]:** each arrow is one SQLite transaction that writes the
stage output *and* advances `stage_cursor` atomically. Restart replays from `stage_cursor`,
never from the beginning, and never re-fetches a URL already in `sources` for that session.

**Stage outputs (the resume payloads):**

| Stage | Committed artifact | Resume behaviour |
|---|---|---|
| planning | charter JSON on the session row | re-plan is cheap; safe to redo |
| expanding | nodes + dots + edges + sources, per expansion | continue from frontier |
| synthesising | blueprint JSON | redo from mind map |
| fact_check | fact report + `fact_score` | redo (deterministic parts cached) |
| writing | section drafts, hooks, headline set | resume per section |
| imaging | image rows + variants on disk | skip already-resolved images |
| seo | keywords, metadata, `post.json` | redo |
| publishing | temp bundle → verify → atomic swap → `deploy_ref` | either old or new site, never half |

---

## 6. Event bus

One broadcast bus in `bhippi-core`, typed in `bhippi-types::events`.

```rust
#[derive(Clone, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    SessionStageChanged  { session: SessionId, from: Stage, to: Stage },
    MindmapDelta         { session: SessionId, nodes: Vec<NodeDelta>, edges: Vec<EdgeDelta>, merged: u16 },
    DotAdded             { session: SessionId, dots: Vec<NodeDotDelta>, merged: u16 },
    SourceFetched        { session: SessionId, source: SourceSummary },
    ProviderHealth       { provider: ProviderId, health: Health },
    TickerEvent          { event: TickerEventSummary },
    AutomationTick       { next_run: Option<Timestamp>, queue_depth: u32 },
    PublishProgress      { post: PostId, step: PublishStep, pct: u8 },
    BudgetWarning        { scope: BudgetScope, used: u64, cap: u64 },
    ErrorRaised          { code: ErrorCode, message: String, hint: Option<String>, session: Option<SessionId> },
    SkillPendingApproval { skill: SkillId, capabilities: Vec<Capability> },
    ResyncRequired       { session: Option<SessionId>, reason: ResyncReason },
}
```

**Bus rules:**
1. Events are **facts about the past**, never commands. No component acts on an event to
   mutate another component's state; the orchestrator subscribes and decides.
2. Payloads are **self-contained summaries**, not IDs the UI must resolve with a follow-up
   call. One event, one render.
3. **Coalescing [INV-021, ADR-0003]:** `MindmapDelta` and `DotAdded` share a paced 50 ms
   output lane that caps their combined emission at 20/s. Batches merge; nothing is dropped
   silently — a coalesced batch carries `merged` and every dot retains its `node` id.
4. A slow or absent UI must never back-pressure the engine. Channel capacity 1024; on lag
   the UI gets a resync marker and refetches the map.
5. Every event is also a `tracing` event carrying the session id, so logs and UI tell the
   same story.

---

## 7. Error model

```rust
#[derive(Debug, thiserror::Error)]
pub enum BhippiError {
    #[error("provider {id} unavailable: {reason}")]
    Provider { id: String, reason: String, retryable: bool, hint: Option<String> },
    #[error("budget exceeded: {scope}")]
    Budget { scope: BudgetScope, used: u64, cap: u64 },
    #[error("topic out of scope (score {score:.2} < {threshold:.2})")]
    OutOfScope { score: f32, threshold: f32 },
    #[error("gate blocked publication: {gate}")]
    Gate { gate: GateName, detail: String },
    #[error("fetch failed for {url}: {kind}")]
    Fetch { url: String, kind: FetchErrorKind, retryable: bool },
    #[error("data: {0}")]
    Db(#[from] sqlx::Error),
    #[error("invariant violated: {0}")]
    Invariant(&'static str),
}
```

**Rules:**
- Libraries return typed errors (`thiserror`); only `bhippi-app`'s top level uses `anyhow`.
- Every error carries `retryable` and, where the user can act, a `hint` the UI shows
  verbatim (spec §19.4: *"Ollama isn't responding on :11434 — start it, or switch routing
  to Cloud."*).
- `Invariant` is a bug, not a condition: log at ERROR, dump a replay bundle, fail the
  session. Never caught and continued.
- No `unwrap()` / `expect()` outside `#[cfg(test)]`. Clippy denies both.

---

## 8. Persistence architecture

### 8.1 Storage surfaces

| Surface | Path | Contents |
|---|---|---|
| Database | `~/.bhippi/bhippi.db` (+ `-wal`, `-shm`) | all rows; vectors via `sqlite-vec` |
| FTS index | `~/.bhippi/index/tantivy/` | dots + posts full text |
| Blob store | `~/.bhippi/blobs/<bb>/<hash>` | extracted text, raw HTML, PDFs, images, variants |
| Site output | `~/.bhippi/site/` | built static site (atomic swap target) |
| Replay dumps | `~/.bhippi/replay/<session_id>/` | prompts, inputs, outputs |
| Logs | `~/.bhippi/logs/` | JSON rolling, 7 days, scrubbed |
| Config | `~/.bhippi/config.toml` | never secrets |
| Secrets | OS keychain | API keys, deploy tokens |

Blob paths are **content-addressed** (`blake3` hex, 2-char shard prefix): the same bytes
fetched twice cost one file. `bhippi doctor` finds and removes orphans.

### 8.2 Repository layer

`bhippi-db` exposes repositories, not a connection: `SessionRepo`, `NodeRepo`, `DotRepo`,
`SourceRepo`, `ImageRepo`, `MemoryRepo`, `TickerRepo`, `PostRepo`, `SkillRepo`,
`ProviderRepo`, `JobRepo`. Each method is one intention (`commit_expansion(...)`), takes a
transaction where it spans tables, and is compile-time checked with `sqlx::query!`.

**No SQL string exists outside `bhippi-db`.** Review rejects it.

### 8.3 Migration discipline

Forward-only, numbered, idempotent, never edited after merge. A migration that moves data
ships with a dry-run count surfaced in `bhippi doctor`. Schema version is asserted at
startup; a DB newer than the binary refuses to open with a clear message rather than
migrating downward.

---

## 9. Provider abstraction seam

The only crate that knows an LLM exists is `bhippi-providers`. Capability crates receive an
`Arc<dyn ProviderRouter>` and ask for a `TaskClass`, never for a vendor.

```rust
pub trait ProviderRouter: Send + Sync {
    async fn complete(&self, task: TaskClass, req: CompletionRequest) -> Result<Stream<Delta>>;
    async fn complete_json<T: DeserializeOwned + JsonSchema>(&self, task: TaskClass, req: CompletionRequest) -> Result<T>;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    fn pinned(&self, task: TaskClass) -> Option<ProviderId>;   // supports Editor != Writer
}
```

`complete_json` owns schema validation, exactly one repair round-trip on invalid JSON, then
rejection. Capability crates never hand-parse model output.

**Writer/Editor split [INV-008]:** the router records the `ProviderId` used for `Writer` on
the session and refuses to return that same instance for `Editor` while another healthy
provider exists.

---

## 10. Extension seams (the only supported ones)

| Seam | Trait | Implementations in v1 |
|---|---|---|
| LLM backend | `Provider` | CLI, API, local server adapters |
| Web search | `SearchBackend` | SearXNG (default), Brave, Tavily, DDG |
| Deploy | `DeployTarget` | local dir, git/Pages, Netlify, Cloudflare, WordPress |
| Renderer | `SiteRenderer` | static (minijinja), React SSG |
| Skill body | `SkillRuntime` | Rhai, WASM/WASI p2 |
| Feed source | rows in `source_registry` | seed + user |

Anything else pluggable in v1 is scope creep. A new seam needs an ADR.

---

## 11. Untrusted-input boundary

Everything crossing this line is hostile until proven otherwise: fetched HTML/PDF/JSON,
feed content, image bytes, skill bodies, search results.

```
network --> [ size cap · content-type check · charset normalise ]
        --> [ extraction (no eval; JS only inside sandboxed chromiumoxide) ]
        --> [ prompt wrapping: <UNTRUSTED_DATA> ... </UNTRUSTED_DATA> ]
        --> [ schema-constrained extraction: injected text cannot change output shape ]
        --> [ imperative-pattern filter on every dot ]
        --> trusted store
```

Image bytes are decoded with dimension and pixel-count caps **before** allocation
(decompression-bomb defence). Skills never cross this boundary in the other direction: no
DB handle, no keychain, no provider client.

---

## 12. Repository layout (authoritative)

```
bhippi/
├── Cargo.toml                     # workspace, shared lints, MSRV 1.79
├── rust-toolchain.toml
├── deny.toml                      # licence + advisory policy
├── crates/
│   ├── bhippi-types/              # L0
│   ├── bhippi-db/                 # L1
│   ├── bhippi-providers/          # L1
│   ├── bhippi-harvest/            # L2
│   ├── bhippi-research/           # L2
│   ├── bhippi-memory/             # L2
│   ├── bhippi-vision/             # L2
│   ├── bhippi-writer/             # L2
│   ├── bhippi-seo/                # L2
│   ├── bhippi-ticker/             # L2
│   ├── bhippi-skills/             # L2
│   ├── bhippi-publish/            # L2
│   ├── bhippi-engine/             # L2  (game-engine editor domain — ADR-0020)
│   ├── bhippi-engine-build/       # L2  (build orchestration — ADR-0020)
│   ├── bhippi-engine-viewport/    # S   (leaf Bevy binary, editor sim + game — ADR-0020)
│   ├── bhippi-core/               # L3
│   └── bhippi-app/                # L4  (Tauri bin + CLI bin)
├── ui/                            # L5 React shell
│   ├── src/screens/{Research,Automation,Library,Settings}/
│   ├── src/components/
│   ├── src/lib/ipc.ts             # GENERATED — never hand-edited
│   └── src/lib/tokens.css
├── themes/bhippi-default/         # blog theme (templates, tokens, islands)
├── prompts/                       # versioned prompt files, hash-pinned at use
├── schemas/                       # JSON Schemas referenced by prompts + skills
├── skills/{builtin,user}/
├── migrations/
├── seeds/source_registry.json
├── docs/                          # this documentation set
└── tests/{fixtures,golden,e2e,redteam}/
```

### 12.1 Two binaries, one core

`bhippi-app` produces `bhippi` (CLI, spec Appendix D) and `bhippi-desktop` (Tauri). Both are
thin clients of `bhippi-core`. **Any behaviour reachable only from the GUI is a bug** — the
CLI is how golden-topic and soak suites drive the engine.

---

## 13. Build, test, release topology

```
PR:      fmt -> clippy -D warnings -> architecture test -> unit -> fixture (offline)
         -> e2e (headless, temp dir) -> ui typecheck -> theme build -> lighthouse (10 posts)
Nightly: golden topics (20 x 4 tiers, pinned local model) -> soak subset (8 h) -> red team
Release: build matrix (macOS arm64/x64, Windows x64, Linux x64) -> sign -> checksum -> notes
```

Unit and fixture tests run with **the network disabled at process level**. A test that needs
the network is an e2e test and lives in `tests/e2e`.

---

## 14. What this architecture explicitly refuses

- A second process, a daemon, or any IPC server other than Tauri's.
- Business logic in TypeScript, including "just this one filter".
- A generic plugin system beyond §10's seams.
- A second persistence engine (no Redis, no embedded KV alongside SQLite).
- Cross-crate global singletons other than the tracing subscriber and the config handle.
- Cloud anything as a hard dependency — a full X6 run must complete offline, on a local
  model, with zero keys.

---

## 15. Open architectural questions (decide before the sprint that needs them)

| # | Question | Needed by | Owner |
|---|---|---|---|
| A1 | Does the topical classifier live in `harvest` or `core`? | S2 | Core |
| A2 | Barnes-Hut layout in `core` or its own `bhippi-layout` crate? | S4 | Frontend |
| A3 | Tantivy commit cadence — per expansion or per session? | S3 | Core |
| A4 | `sqlite-vec` vs. in-process brute force below 50k vectors | S5 | Agents |
| A5 | React SSG renderer: shared `post.json` loader crate, or a JS build step? | S8 | Frontend |

Answer each with an ADR in `docs/adr/`, then delete the row.
