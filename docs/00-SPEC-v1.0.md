# BHIPPI — Deep Research & Autonomous Publishing Engine
### Engineering Specification v1.0
**Owner:** Tech Lead · **Audience:** Core Engineering, Agents, Frontend, Content-Ops, QA
**Status:** Approved for build · **Target:** Desktop app (Rust) + generated static blog

---

## 0. How to read this document

Every numbered module (`M1`–`M12`) is a work package with an owner squad, hard acceptance criteria, and ticket IDs (`BHP-xxx`). If something is not written here, it is **not in v1**. If you disagree with a decision, open a `DECISION-CHANGE` issue — do not silently deviate. Sections marked **[HARD REQ]** are non-negotiable and will fail code review if missing.

Read in this order: §1 → §3 (architecture) → your own module → §14 (sprints).

---

## 1. Mission & product definition

**Bhippi** is a desktop application that autonomously researches technology and AI topics to a controllable depth, builds a persistent knowledge graph from what it learns, and publishes SEO-optimised, image-rich blog posts to a static site — either on demand, on a schedule, or reactively when a live news ticker detects a breaking story.

Three operating postures, one engine:

| Posture | Trigger | Human involvement |
|---|---|---|
| **Manual research** | User types a topic, picks depth (X2/X6/X12/X24) | Full — reviews mind map, decides whether to publish |
| **Timer automation** | Every N minutes, engine picks the next topic from the interest graph | Optional review gate |
| **Ticker automation** | A qualifying breaking story appears in the live ticker | Optional review gate; can be fully autonomous |

**Domain lock [HARD REQ]:** Bhippi only researches and publishes **technology and artificial intelligence**. Everything else is rejected by a topical classifier at ingestion. No exceptions, no "close enough" categories. Semiconductors, robotics, dev tooling, AI research, consumer tech, cloud infra, cybersecurity, space-tech, and the business of tech companies are in scope. Politics, sports, entertainment, health, and finance are out of scope unless the story is fundamentally about a technology or an AI system.

### 1.1 Design principles

1. **Local-first.** The app must be fully functional with zero cloud API keys, driven entirely by a local LLM. Cloud providers are an accelerator, never a dependency.
2. **Everything is inspectable.** Every published sentence traces back to a source node in the mind map. No black boxes.
3. **The engine improves itself.** Sessions become memory; memory becomes better research; repeated procedures become skills.
4. **Minimal surface.** Four screens total. If a feature needs a fifth screen, it needs a better design.
5. **Never publish what you cannot defend.** Confidence, sourcing, and licence provenance gate publication — not vibes.

### 1.2 Non-goals for v1

- Multi-user / team accounts, cloud sync, or a hosted SaaS backend.
- Video, podcast, or social-media generation.
- Non-English output (architecture must not block it; the feature ships in v2).
- Mobile clients.
- Paid-content scraping, paywall bypass, or credential-based crawling of any kind.

---

## 2. Glossary

| Term | Meaning |
|---|---|
| **Session** | One research run, from a seed topic to a finished artifact. Has a persistent ID, mind map, and gist. |
| **Node** | A concept, entity, claim, or question in the mind map. |
| **Dot** | An evidence point: one extracted fact attached to a node, with a source, timestamp and confidence. |
| **Hop** | One expansion step from a node to its children. |
| **Depth tier** | The user-chosen research intensity: X2, X6, X12, X24. |
| **Gist** | A compressed, structured summary of a session, written into long-term memory. |
| **Ticker event** | A deduplicated, scored, classified breaking story from the feed layer. |
| **Skill** | A versioned, sandboxed, reusable procedure the engine can author, evaluate, and invoke. |
| **Provider** | Any LLM backend — cloud CLI, cloud API, or local server. |

---

## 3. System architecture

### 3.1 Layer diagram

```
┌──────────────────────────────────────────────────────────────────────────┐
│  SHELL  ·  Tauri v2 window  ·  4 screens + ticker strip + settings        │
│  React 18 + TypeScript (thin — rendering & input only, zero logic)        │
└───────────────────────────────▲──────────────────────────────────────────┘
                                │  Tauri IPC commands + typed event stream
┌───────────────────────────────▼──────────────────────────────────────────┐
│  ORCHESTRATOR  (bhippi-core)                                             │
│  Session lifecycle · job queue · budget guard · kill switch · event bus  │
└──┬────────────┬────────────┬────────────┬────────────┬───────────────┬───┘
   │            │            │            │            │               │
┌──▼───────┐┌───▼──────┐┌────▼─────┐┌─────▼────┐┌──────▼─────┐┌────────▼────┐
│ RESEARCH ││ HARVEST  ││  VISION  ││  WRITER  ││   TICKER   ││    SKILLS   │
│  ENGINE  ││  crawl+  ││  image   ││  compose ││  feeds +   ││  registry + │
│ planner/ ││ extract+ ││ under-   ││  + SEO   ││  burst     ││  sandbox +  │
│ expander ││  dedupe  ││ stand +  ││  + hooks ││  detect    ││  evaluator  │
│ synth    ││          ││  crop    ││          ││            ││             │
└──┬───────┘└───┬──────┘└────┬─────┘└─────┬────┘└──────┬─────┘└────────┬────┘
   │            │            │            │            │               │
┌──▼────────────▼────────────▼────────────▼────────────▼───────────────▼───┐
│  PROVIDER LAYER  ·  auto-detect · capability probe · routing · fallback   │
│  Claude · Codex · Grok · Kimi · OpenCode · Ollama · LM Studio · llama.cpp │
└──────────────────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────────────────┐
│  PERSISTENCE  ·  SQLite (WAL) + sqlite-vec + Tantivy FTS + blob store     │
│  sessions · nodes · dots · sources · images · memory · skills · posts     │
└──────────────────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────────────────┐
│  PUBLISHER  ·  static site generator → HTML/React bundle → deploy adapter │
└──────────────────────────────────────────────────────────────────────────┘
```

### 3.2 The one data flow that matters

```
seed topic ──► PLAN ──► EXPAND (hop n) ──► HARVEST ──► EXTRACT dots
                 ▲                                          │
                 │                                          ▼
                 └────── frontier scoring ◄──── MIND MAP (nodes + dots + edges)
                                                            │
                     ┌──────────────────────────────────────┤
                     ▼                                      ▼
                 SYNTHESISE ──► fact-check gate ──► WRITE ──► IMAGE PIPELINE
                                                              │
                                    MEMORY ◄── GIST ◄─────────┤
                                                              ▼
                                                    SEO ──► PUBLISH ──► DEPLOY
```

Every arrow is an event on the bus. Every stage is resumable — if the app is killed mid-run, `session.resume()` picks up at the last committed stage.

---

## 4. Technology decisions [HARD REQ]

**Language: Rust (2021 edition, MSRV 1.79).** All logic — crawling, research, scoring, memory, generation orchestration, publishing — lives in Rust. TypeScript is permitted **only** inside the Tauri webview for rendering.

**Workspace crates:**

| Crate | Responsibility |
|---|---|
| `bhippi-core` | Orchestrator, session lifecycle, event bus, job queue, budget guard |
| `bhippi-providers` | LLM detection, capability probing, routing, streaming, fallback |
| `bhippi-harvest` | HTTP client, robots, rate limiting, readability extraction, dedupe |
| `bhippi-research` | Planner, expander, frontier scorer, mind map, synthesiser |
| `bhippi-memory` | Embeddings, vector store, entity graph, gist writer, retrieval |
| `bhippi-vision` | Image search, licence filter, captioning, saliency crop, encoding |
| `bhippi-writer` | Article composition, hook engine, structure, style enforcement |
| `bhippi-seo` | Keywords, metadata, schema.org, sitemap, RSS, internal links |
| `bhippi-ticker` | Feed poller, canonicalisation, burst detection, relevance scoring |
| `bhippi-skills` | Skill manifest, sandbox runtime, evaluator, registry |
| `bhippi-publish` | Static site generation, theme rendering, deploy adapters |
| `bhippi-db` | Schema, migrations, repositories, FTS + vector indexes |
| `bhippi-app` | Tauri shell, IPC command surface, tray, single-instance guard |

**Locked dependencies:**

| Concern | Crate | Note |
|---|---|---|
| Async runtime | `tokio` (full) | One multi-thread runtime, shared |
| HTTP | `reqwest` (rustls, gzip, brotli) | No native-tls |
| HTML parse | `scraper` + `dom_smoothie` | Readability-style main-content extraction |
| Headless browse | `chromiumoxide` | **Fallback only** — JS-rendered pages, §6.4 |
| Robots | `texting_robots` | Mandatory, no bypass path in code |
| Rate limit | `governor` | Per-host quota |
| Cache | `moka` (async) + on-disk blob store | |
| DB | `sqlx` (sqlite, WAL) | Compile-time checked queries |
| Vector | `sqlite-vec` | Embedded, no external service |
| Embeddings | `fastembed` (bge-small-en-v1.5) | Local, CPU, no API key |
| Full-text | `tantivy` | Local search over dots and posts |
| Feeds | `feed-rs` | RSS/Atom/JSON Feed |
| Images | `image` + `fast_image_resize` + `webp` | AVIF via `ravif` |
| Templates | `minijinja` | Static HTML generation |
| Sandbox | `wasmtime` (WASI p2) + `rhai` | Skills, §11 |
| Serialisation | `serde` + `serde_json` + `toml` | |
| Errors | `thiserror` (libs) + `anyhow` (bin) | |
| Logging | `tracing` + `tracing-subscriber` + rolling file | JSON in prod |
| Scheduling | `tokio-cron-scheduler` | |
| Desktop | `tauri` v2 | |

**Forbidden in v1:** any ORM other than sqlx, any JS runtime in the backend, `unwrap()` in non-test code (clippy-enforced), blocking IO on the async runtime, and vendored binaries not listed above.

---

## 5. Repository layout

```
bhippi/
├── Cargo.toml                  # workspace
├── crates/
│   ├── bhippi-core/
│   ├── bhippi-providers/
│   ├── bhippi-harvest/
│   ├── bhippi-research/
│   ├── bhippi-memory/
│   ├── bhippi-vision/
│   ├── bhippi-writer/
│   ├── bhippi-seo/
│   ├── bhippi-ticker/
│   ├── bhippi-skills/
│   ├── bhippi-publish/
│   ├── bhippi-db/
│   └── bhippi-app/             # Tauri binary
├── ui/                         # React + Vite (thin shell)
│   ├── src/screens/{Research,Automation,Library,Settings}/
│   ├── src/components/
│   └── src/lib/ipc.ts          # generated from Rust types
├── themes/
│   └── bhippi-default/         # blog theme: templates + tokens + islands
├── prompts/                    # versioned prompt templates (*.md, hash-pinned)
├── skills/
│   ├── builtin/
│   └── user/
├── migrations/
├── docs/
└── tests/
    ├── fixtures/               # frozen HTML pages, feeds, images
    └── e2e/
```

**Rule:** prompts are files, never string literals in code. Each prompt file carries a `version:` header and is hash-pinned in the DB when used, so a published post can always be reproduced.

---

## 6. Configuration

Single file: `~/.bhippi/config.toml`. Secrets never live here — they go to the OS keychain via `keyring`.

```toml
[app]
data_dir      = "~/.bhippi"
theme         = "dark"          # dark | light | system
telemetry     = false           # always default false, no exceptions

[research]
default_tier  = "X6"            # X2 | X6 | X12 | X24
max_parallel_fetches = 6
per_host_rps  = 0.5
respect_robots = true           # locked true; setting is display-only
language      = "en"

[domain]
scope         = ["technology", "artificial-intelligence"]
reject_threshold = 0.62         # topical classifier cutoff

[providers]
auto_detect   = true
offline_mode  = false           # true = local providers only
routing       = "balanced"      # quality | balanced | cheap | local-only

[automation]
enabled       = false
mode          = "off"           # off | timer | ticker | both
interval_mins = 60
review_gate   = true            # false = fully autonomous publish
daily_post_cap = 6
quiet_hours   = ["23:30", "07:00"]

[ticker]
poll_secs     = 120
burst_sources = 3               # N independent sources = breaking
auto_trigger_score = 78         # 0-100

[publish]
target        = "static"        # static | github-pages | netlify | cloudflare | wordpress
site_url      = "https://bhippi.example"
out_dir       = "~/.bhippi/site"

[budget]
daily_token_cap    = 2_000_000
daily_wall_secs    = 14_400
per_session_usd_cap = 0.00      # 0 = local-only spend guard
```

---

## 7. Data model

SQLite, WAL mode, foreign keys ON. All timestamps UTC ISO-8601. All IDs are ULIDs (sortable, 26 chars).

```sql
-- ─────────────────────────── SESSIONS & MIND MAP ───────────────────────────
CREATE TABLE sessions (
  id            TEXT PRIMARY KEY,
  seed_topic    TEXT NOT NULL,
  tier          TEXT NOT NULL CHECK (tier IN ('X2','X6','X12','X24')),
  origin        TEXT NOT NULL CHECK (origin IN ('manual','timer','ticker','skill')),
  ticker_event_id TEXT REFERENCES ticker_events(id),
  status        TEXT NOT NULL,     -- planning|expanding|synthesising|writing|
                                   -- imaging|review|publishing|done|failed|cancelled
  stage_cursor  TEXT,              -- resume point
  domain_score  REAL,              -- topical classifier confidence
  started_at    TEXT NOT NULL,
  finished_at   TEXT,
  tokens_used   INTEGER DEFAULT 0,
  wall_secs     INTEGER DEFAULT 0,
  error         TEXT
);

CREATE TABLE nodes (
  id            TEXT PRIMARY KEY,
  session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  parent_id     TEXT REFERENCES nodes(id),
  hop           INTEGER NOT NULL,          -- 0 = seed
  kind          TEXT NOT NULL,             -- concept|entity|claim|question|counterpoint|
                                           -- timeline|metric|source-cluster
  label         TEXT NOT NULL,
  summary       TEXT,
  status        TEXT NOT NULL,             -- frontier|expanding|explored|pruned|dead-end
  novelty       REAL,                      -- 0-1 vs. existing memory
  relevance     REAL,                      -- 0-1 vs. seed topic
  authority     REAL,                      -- 0-1 source quality of its dots
  priority      REAL,                      -- computed frontier score
  embedding     BLOB,
  created_at    TEXT NOT NULL
);
CREATE INDEX idx_nodes_frontier ON nodes(session_id, status, priority DESC);

CREATE TABLE edges (
  id            TEXT PRIMARY KEY,
  session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  from_node     TEXT NOT NULL REFERENCES nodes(id),
  to_node       TEXT NOT NULL REFERENCES nodes(id),
  relation      TEXT NOT NULL,   -- causes|enables|competes-with|part-of|contradicts|
                                 -- precedes|funded-by|built-on|benchmarks-against
  weight        REAL DEFAULT 1.0,
  evidence_dot  TEXT REFERENCES dots(id)
);

CREATE TABLE dots (                          -- one extracted evidence point
  id            TEXT PRIMARY KEY,
  session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  node_id       TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  claim         TEXT NOT NULL,              -- one atomic fact, ≤ 240 chars
  claim_type    TEXT NOT NULL,              -- fact|number|quote|opinion|prediction|spec
  source_id     TEXT NOT NULL REFERENCES sources(id),
  char_start    INTEGER, char_end INTEGER,  -- provenance offsets in extracted text
  observed_at   TEXT NOT NULL,              -- when the source published/updated
  confidence    REAL NOT NULL,              -- 0-1
  corroborations INTEGER DEFAULT 0,         -- independent sources agreeing
  contradicted_by TEXT,                     -- JSON array of dot ids
  embedding     BLOB
);
CREATE INDEX idx_dots_node ON dots(node_id);

-- ─────────────────────────── SOURCES & CRAWL ───────────────────────────────
CREATE TABLE sources (
  id            TEXT PRIMARY KEY,
  url           TEXT NOT NULL,
  canonical_url TEXT NOT NULL UNIQUE,
  domain        TEXT NOT NULL,
  title         TEXT,
  author        TEXT,
  published_at  TEXT,
  fetched_at    TEXT NOT NULL,
  http_status   INTEGER,
  content_hash  TEXT,          -- blake3 of extracted text
  simhash       INTEGER,       -- near-dupe detection
  word_count    INTEGER,
  extracted_path TEXT,         -- blob store path
  trust_tier    INTEGER,       -- 1 = primary, 2 = tier-1 press, 3 = general, 4 = weak
  paywalled     INTEGER DEFAULT 0,
  license       TEXT,
  lang          TEXT
);

CREATE TABLE source_registry (            -- curated reputation table, ships seeded
  domain        TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  trust_tier    INTEGER NOT NULL,
  categories    TEXT,          -- JSON array
  feed_url      TEXT,
  robots_note   TEXT,
  enabled       INTEGER DEFAULT 1
);

-- ─────────────────────────── IMAGES ────────────────────────────────────────
CREATE TABLE images (
  id            TEXT PRIMARY KEY,
  session_id    TEXT REFERENCES sessions(id) ON DELETE CASCADE,
  origin_url    TEXT NOT NULL,
  page_url      TEXT,
  license       TEXT NOT NULL,           -- cc0|cc-by|cc-by-sa|press-kit|owned|unknown
  license_url   TEXT,
  attribution   TEXT,
  width INTEGER, height INTEGER,
  phash         TEXT,                    -- perceptual hash, dedupe
  caption_model TEXT,                    -- vision understanding output
  caption       TEXT,
  alt_text      TEXT NOT NULL,
  relevance     REAL,                    -- 0-1 vs. the section it serves
  focal_x REAL, focal_y REAL,            -- saliency centre, 0-1
  variants      TEXT,                    -- JSON: {"hero_16x9":"...", "og_1200x630":"..."}
  status        TEXT NOT NULL            -- candidate|approved|rejected
);

-- ─────────────────────────── MEMORY ────────────────────────────────────────
CREATE TABLE memory_gists (
  id            TEXT PRIMARY KEY,
  session_id    TEXT REFERENCES sessions(id),
  scope         TEXT NOT NULL,           -- session|topic|entity|global
  title         TEXT NOT NULL,
  body          TEXT NOT NULL,           -- structured markdown, ≤ 1200 tokens
  key_claims    TEXT,                    -- JSON array of dot ids
  entities      TEXT,                    -- JSON array of entity ids
  created_at    TEXT NOT NULL,
  last_used_at  TEXT,
  use_count     INTEGER DEFAULT 0,
  decay_score   REAL DEFAULT 1.0,
  embedding     BLOB
);

CREATE TABLE entities (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  kind          TEXT NOT NULL,           -- company|model|person|product|technology|
                                         -- standard|lab|chip|framework
  aliases       TEXT,                    -- JSON array
  summary       TEXT,
  first_seen    TEXT, last_seen TEXT,
  mention_count INTEGER DEFAULT 0,
  embedding     BLOB
);
CREATE TABLE entity_links (
  from_entity TEXT NOT NULL REFERENCES entities(id),
  to_entity   TEXT NOT NULL REFERENCES entities(id),
  relation    TEXT NOT NULL,
  weight      REAL DEFAULT 1.0,
  evidence    TEXT,
  PRIMARY KEY (from_entity, to_entity, relation)
);

CREATE TABLE chat_turns (                 -- conversational memory in the app
  id          TEXT PRIMARY KEY,
  session_id  TEXT REFERENCES sessions(id),
  role        TEXT NOT NULL,
  content     TEXT NOT NULL,
  created_at  TEXT NOT NULL,
  gisted      INTEGER DEFAULT 0
);

-- ─────────────────────────── TICKER ────────────────────────────────────────
CREATE TABLE ticker_events (
  id             TEXT PRIMARY KEY,
  cluster_id     TEXT NOT NULL,          -- same story across outlets
  headline       TEXT NOT NULL,
  url            TEXT NOT NULL,
  domain         TEXT NOT NULL,
  published_at   TEXT NOT NULL,
  first_seen_at  TEXT NOT NULL,
  category       TEXT,                   -- ai-research|chips|devtools|security|...
  domain_score   REAL,                   -- tech/AI relevance
  burst_count    INTEGER DEFAULT 1,      -- independent outlets in cluster
  velocity       REAL,                   -- outlets per hour
  priority       REAL,                   -- 0-100 composite
  state          TEXT NOT NULL,          -- new|shown|triggered|covered|ignored|expired
  session_id     TEXT REFERENCES sessions(id)
);

-- ─────────────────────────── POSTS ─────────────────────────────────────────
CREATE TABLE posts (
  id            TEXT PRIMARY KEY,
  session_id    TEXT NOT NULL REFERENCES sessions(id),
  slug          TEXT NOT NULL UNIQUE,
  title         TEXT NOT NULL,
  dek           TEXT,
  body_md       TEXT NOT NULL,
  body_html     TEXT,
  hero_image_id TEXT REFERENCES images(id),
  primary_kw    TEXT,
  keywords      TEXT,                    -- JSON array with volume/difficulty
  meta_desc     TEXT,
  reading_mins  INTEGER,
  word_count    INTEGER,
  seo_score     INTEGER,                 -- 0-100
  fact_score    INTEGER,                 -- 0-100
  status        TEXT NOT NULL,           -- draft|review|scheduled|published|retracted
  published_at  TEXT,
  updated_at    TEXT,
  deploy_ref    TEXT,                    -- commit sha / deploy id
  prompt_hashes TEXT                     -- JSON: which prompt versions produced it
);

-- ─────────────────────────── SKILLS & PROVIDERS ────────────────────────────
CREATE TABLE skills (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  version       TEXT NOT NULL,
  kind          TEXT NOT NULL,           -- prompt|script|composite
  manifest      TEXT NOT NULL,           -- TOML
  body_path     TEXT NOT NULL,
  autonomy      TEXT NOT NULL,           -- proposed|trial|enabled|disabled|quarantined
  created_by    TEXT NOT NULL,           -- user|engine
  eval_score    REAL,
  runs          INTEGER DEFAULT 0,
  wins          INTEGER DEFAULT 0,
  last_run_at   TEXT,
  UNIQUE(name, version)
);

CREATE TABLE providers (
  id            TEXT PRIMARY KEY,
  kind          TEXT NOT NULL,           -- cli|api|local
  vendor        TEXT NOT NULL,           -- claude|codex|grok|kimi|opencode|ollama|
                                         -- lmstudio|llamacpp|vllm|jan|custom
  model         TEXT NOT NULL,
  endpoint      TEXT,
  detected_via  TEXT,                    -- path|env|port|config|manual
  ctx_window    INTEGER,
  supports_vision INTEGER DEFAULT 0,
  supports_tools  INTEGER DEFAULT 0,
  avg_latency_ms  INTEGER,
  health        TEXT,                    -- ok|degraded|down|unauthorised
  enabled       INTEGER DEFAULT 1,
  last_checked  TEXT
);
```

**Migrations:** `sqlx migrate`. Every migration is forward-only and idempotent. Ship a `bhippi doctor` command that verifies schema, indexes, and blob-store integrity.

---

## 8. M1 — Provider layer  ·  `bhippi-providers`
**Squad:** Agents · **Tickets:** BHP-010…BHP-024

The engine must never care which LLM it is talking to. One trait, many backends, automatic discovery.

### 8.1 Auto-detection [HARD REQ]

Run on app start and on demand from Settings. Four detection strategies, executed concurrently, results merged:

**a) CLI detection — scan `PATH` for known binaries**

| Binary | Vendor | Invocation contract |
|---|---|---|
| `claude` | Claude Code | `claude -p "<prompt>" --output-format stream-json` |
| `codex` | Codex CLI | non-interactive exec mode, JSON output |
| `opencode` | OpenCode | `opencode run --format json` |
| `grok` | Grok CLI | vendor CLI if installed |
| `kimi` | Kimi CLI | vendor CLI if installed |
| `ollama` | Ollama | also probed as a server |

For each hit: record absolute path, run `--version`, and execute a 5-token capability ping with a 10 s timeout. A CLI that fails the ping is registered `health = down` and excluded from routing, not deleted.

**b) Config/credential detection** — presence of `~/.claude/`, `~/.codex/`, `~/.config/opencode/`, and equivalent vendor config dirs; read model names only, **never read or copy credential values into Bhippi's own storage**.

**c) Environment variables** — `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `XAI_API_KEY`, `MOONSHOT_API_KEY`, `GROQ_API_KEY`, `OPENROUTER_API_KEY`. Presence only ⇒ offer an API provider. Values stay in the process env; if the user pastes a key in Settings, it goes to the OS keychain.

**d) Local server probe [HARD REQ]** — concurrent TCP + HTTP probe of loopback:

| Port | Service | Probe |
|---|---|---|
| 11434 | Ollama | `GET /api/tags` → model list |
| 1234 | LM Studio | `GET /v1/models` |
| 8080 | llama.cpp server | `GET /props` then `/v1/models` |
| 8000 | vLLM | `GET /v1/models` |
| 1337 | Jan | `GET /v1/models` |
| 5000 | text-generation-webui | `GET /v1/models` |
| custom | user-defined | user supplies base URL |

Probe budget: 400 ms per port, all in parallel, total ≤ 1.5 s. Never block app start on detection — emit `providers.discovered` events as they land and let the UI fill in.

### 8.2 Capability probing

For every detected model, resolve and cache: context window, vision support, tool/function-call support, streaming support, approximate tok/s (measured on a fixed 200-token benchmark prompt), and cost class (`free-local`, `cheap`, `standard`, `premium`). Re-probe every 24 h or on user request. Store in `providers`.

### 8.3 The trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn caps(&self) -> &Capabilities;
    async fn complete(&self, req: CompletionRequest)
        -> Result<BoxStream<'static, Result<Delta, ProviderError>>>;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> { /* default: unsupported */ }
    async fn health(&self) -> Health;
}

pub struct CompletionRequest {
    pub task: TaskClass,          // routing hint
    pub system: String,
    pub messages: Vec<Message>,   // may include image parts
    pub max_tokens: u32,
    pub temperature: f32,
    pub json_schema: Option<Value>,  // structured output when supported
    pub timeout: Duration,
}
```

### 8.4 Routing policy

`TaskClass` → provider preference, resolved at call time against health, caps, and the `routing` setting:

| TaskClass | Needs | Preference order (routing = balanced) |
|---|---|---|
| `Planner` | strong reasoning, 32k+ ctx | premium cloud → best local ≥ 14B → any |
| `Expander` | cheap, fast, structured | local → cheap cloud |
| `Extractor` | structured JSON, high volume | local (always, if available) |
| `Classifier` | tiny, fast | local small model → embeddings-only fallback |
| `Vision` | image input | vision-capable local (llava/qwen-vl) → vision cloud |
| `Writer` | best prose quality | premium cloud → best local |
| `Editor` | critique, factuality | different provider than Writer, **[HARD REQ]** |
| `SkillAuthor` | code + reasoning | premium cloud → best local |

**Rules:**
- `routing = "local-only"` or `offline_mode = true` ⇒ never touch the network for inference. If no local provider exists, fail loudly with an actionable error, never silently degrade to cloud.
- Editor must not be the same provider instance as Writer when two or more providers are available. Self-review by the same model is not review.
- Fallback chain: on error or timeout, retry once on the same provider with exponential backoff, then the next candidate, max 3 providers. Log every hop.
- Budget guard rejects the call before it is issued if the daily cap is exceeded.

### 8.5 Acceptance criteria

- [ ] Fresh machine with only Ollama running: app detects it, runs a full X6 session, publishes a post, never makes an inference network call.
- [ ] All six CLI vendors detected when installed; absent ones do not produce errors, only `not_found` state.
- [ ] Killing a provider mid-session results in a clean fallback with a visible event, not a failed session.
- [ ] Provider list in Settings reflects live health within 5 s of a change.

---

## 9. M2 — Harvest layer  ·  `bhippi-harvest`
**Squad:** Core · **Tickets:** BHP-030…BHP-048

### 9.1 Fetch policy [HARD REQ]

- `robots.txt` is fetched, cached for 12 h, and **obeyed**. There is no override flag, no user setting, no debug bypass. Code review rejects any PR introducing one.
- Identify honestly: `User-Agent: BhippiBot/1.0 (+https://bhippi.example/bot)`.
- Per-host rate limit via `governor`: default 0.5 rps, 1 concurrent connection per host, honour `Crawl-delay`.
- Global concurrency: `max_parallel_fetches` (default 6).
- Timeouts: 8 s connect, 20 s total. 3 retries on 5xx/timeout with jittered backoff; 0 retries on 4xx.
- `429`/`503 Retry-After` respected exactly; host enters cooldown.
- Max 4 MB per document; abort streaming past that.
- **Paywall handling:** detect paywall/metered markers (`isAccessibleForFree: False`, known selectors, truncated body heuristics). On detection: record `paywalled = 1`, use only the freely available abstract/metadata, and **stop**. No archive mirrors, no cookie tricks, no reader-mode bypass. The post cites the headline and links out.

### 9.2 Discovery — how the engine "goes on the web"

Four discovery channels, used in this priority order:

1. **Registered feeds** — RSS/Atom from `source_registry` (fastest, cleanest, always tried first).
2. **Search** — pluggable search backend behind a `SearchBackend` trait: Brave Search API, SearXNG (self-hosted, key-free — the local-first default), Tavily, or DuckDuckGo HTML. User configures in Settings; SearXNG is documented as the zero-key path.
3. **Link following** — outbound links from an already-harvested page, filtered by relevance and trust tier, respecting depth budget.
4. **Primary-source jump [HARD REQ]** — whenever a news article references a paper, benchmark, filing, changelog, model card, or official blog post, the engine must attempt to fetch the *primary* source and prefer its dots over the secondary reporting. This is the single biggest quality lever in the product.

### 9.3 Extraction pipeline

```
raw HTML ──► charset normalise ──► boilerplate strip (dom_smoothie)
   ──► structured metadata (JSON-LD, OpenGraph, <time>, byline heuristics)
   ──► main text (markdown-normalised, headings preserved)
   ──► tables → markdown tables ; code blocks preserved
   ──► image candidates (src, srcset, alt, caption, figure context)
   ──► outbound link inventory (url, anchor text, rel)
   ──► blob store write + content_hash + simhash
```

**JS-rendered fallback:** if extracted text < 400 chars and the page has ≥ 8 script tags, retry once through `chromiumoxide` headless with a 15 s budget and JS enabled. Cap: 15 % of a session's fetches may use the headless path (it is slow); beyond that, skip and mark the source `thin`.

**PDF handling:** arXiv and vendor whitepapers are first-class. Extract text via `pdf-extract`; keep page numbers as provenance offsets.

### 9.4 Deduplication

Three layers, cheapest first:
1. **Canonical URL** — strip UTM/fbclid/gclid, resolve `rel=canonical`, normalise scheme/host/trailing slash, follow redirect chains (max 5).
2. **Content hash** — blake3 of normalised extracted text ⇒ exact dupe.
3. **Simhash** — 64-bit, Hamming distance ≤ 3 ⇒ near-dupe (syndicated wire copy). Keep the highest `trust_tier` copy, link the rest as corroborations.

### 9.5 Source trust registry

Ships seeded and user-editable in Settings. Tiers:

| Tier | Meaning | Examples of category (not exhaustive) |
|---|---|---|
| **1 — Primary** | The thing itself | arXiv, ACL/NeurIPS proceedings, company research blogs, model cards, GitHub releases/changelogs, SEC/EU filings, standards bodies, official docs |
| **2 — Tier-1 press** | Established tech newsrooms with corrections policies | major technology desks and wire services |
| **3 — General** | Reputable but secondary aggregation | mainstream outlets' tech sections, established trade press |
| **4 — Weak** | Signal, not evidence | forums, social posts, unsigned blogs, content farms |

**Rules:** Tier-4 sources may *suggest* a lead but may **never** be the sole support for a published claim. A number, benchmark, price, or date requires at least one tier ≤ 2 source. Contradictions between tiers are surfaced in the post, not silently resolved.

### 9.6 Acceptance criteria

- [ ] Given a fixture set of 50 frozen pages, extraction produces ≥ 92 % main-content F1 against hand-labelled ground truth.
- [ ] A host returning `429` is backed off correctly and never re-hit inside `Retry-After`.
- [ ] A `robots.txt` disallow results in zero requests to that path, verified by test.
- [ ] Syndicated copies of one wire story collapse into a single source with N corroborations.

---

## 10. M3 — Research engine  ·  `bhippi-research`
**Squad:** Agents + Core · **Tickets:** BHP-060…BHP-092
**This is the heart of the product. Build it carefully; everything else is plumbing.**

### 10.1 The depth ladder — X2 / X6 / X12 / X24 [HARD REQ]

The tier is a **budget contract**, not a vibe. The user picks one number and the engine guarantees the shape of the run.

| | **X2 — Brief** | **X6 — Standard** | **X12 — Deep** | **X24 — Exhaustive** |
|---|---|---|---|---|
| Max hop depth | 2 | 3 | 4 | 5 |
| Frontier expansions | 2 | 6 | 12 | 24 |
| Branch factor per hop | 3 | 4 | 5 | 6 |
| Target sources | 8–14 | 25–40 | 60–90 | 120–200 |
| Min tier ≤ 2 sources | 3 | 8 | 20 | 40 |
| Min primary (tier 1) | 1 | 3 | 8 | 16 |
| Target dots | 30 | 100 | 250 | 500 |
| Counter-evidence pass | no | yes ×1 | yes ×2 | yes ×3 |
| Timeline reconstruction | no | no | yes | yes |
| Entity deep-dives | 0 | 2 | 5 | 10 |
| Wall-clock budget | 3 min | 10 min | 30 min | 90 min |
| Token budget | 60k | 250k | 700k | 1.6M |
| Article length | 700–1000 w | 1200–1800 w | 2000–3000 w | 3000–5000 w |

**Enforcement:** budgets are hard ceilings enforced by the orchestrator, and floors are quality gates. If a run finishes under its floor (e.g. X12 found only 4 primary sources), the session ends in `status = done` but the post is flagged `thin_evidence` and — if `review_gate = false` — is **held for review instead of auto-publishing**. Never publish an under-evidenced post silently.

### 10.2 Stage 1 — Plan

Input: seed topic (+ ticker event context if applicable) + retrieved memory (§11).
Provider: `TaskClass::Planner`. Structured JSON output.

The planner produces a **research charter**:

```json
{
  "canonical_topic": "…",
  "domain_check": {"in_scope": true, "score": 0.94, "category": "ai-research"},
  "framing": "What the reader should understand after reading",
  "known_from_memory": ["gist_id …"],
  "open_questions": [
    {"q": "…", "why_it_matters": "…", "expected_source_kind": "primary|press|docs"}
  ],
  "seed_entities": [{"name": "…", "kind": "company|model|…"}],
  "expected_controversies": ["…"],
  "search_queries": {"broad": ["…"], "narrow": ["…"], "primary_hunt": ["…"]},
  "success_criteria": ["…"]
}
```

**Domain gate:** if `in_scope == false` or `score < reject_threshold`, abort immediately with a clear user-facing message. Log it. Do not "try anyway".

### 10.3 Stage 2 — Expand (the hop loop)

This is a **best-first search over a frontier**, not a fixed tree walk.

```
frontier ← [seed node]
while expansions_used < tier.expansions
      && depth ≤ tier.max_hop
      && budget_ok():

    node ← argmax(priority) from frontier where status = 'frontier'
    node.status ← 'expanding'

    ├─ discover(node)      → candidate URLs (feeds, search, links, primary jump)
    ├─ harvest(candidates) → sources (parallel, rate-limited)
    ├─ extract_dots(node, sources)   → atomic claims w/ provenance
    ├─ derive_children(node, dots)   → new nodes (concepts, entities, questions,
    │                                   contradictions, metrics, timeline points)
    ├─ score(children)     → novelty × relevance × authority × gap-fill
    ├─ dedupe_against(mind_map ∪ memory)
    └─ push(children → frontier, capped at tier.branch_factor)

    node.status ← 'explored' | 'dead-end'
    emit(MindMapDelta)   // UI animates the new dots and edges live
```

**Frontier priority formula** (tune constants in `research.toml`, do not hardcode):

```
priority = 0.35·relevance
         + 0.25·novelty            // 1 - max_cosine(node, memory ∪ explored)
         + 0.20·gap_fill           // does it answer an open charter question?
         + 0.10·authority_potential
         + 0.10·recency_pressure   // higher for ticker-origin sessions
         − 0.15·cost_estimate      // predicted fetch+token cost, normalised
```

**Anti-drift guard [HARD REQ]:** every child node must hold cosine ≥ 0.45 to the seed embedding *or* be explicitly justified as a required counterpoint/prerequisite. This is what stops "AI chips" from wandering into "Taiwanese cuisine" by hop 4. Nodes failing the guard are pruned with reason `drift`.

**Loop guard:** a node whose label normalises to an already-explored label is rejected. Max 3 sibling nodes may share a parent's entity.

**Counter-evidence pass:** at the tiers that require it, the engine spawns explicit contrarian queries for the strongest claims (`"<claim>" criticism|limitations|failed to replicate|benchmark contamination`). Findings become `counterpoint` nodes. A post that presents only the sunny case is a defect.

### 10.4 Stage 3 — Dot extraction

Provider: `TaskClass::Extractor`, JSON schema enforced, run in batches over extracted text.

Rules:
- One dot = one atomic, checkable claim, ≤ 240 chars, **paraphrased in the engine's own words**.
- Direct quotes are permitted only where exact wording carries meaning (a stated commitment, a defined term, a benchmark name), must be under 15 words, and **at most one quote per source** — enforced in code, not by prompt alone. Anything longer is rejected at validation and re-extracted as paraphrase.
- Every dot carries `source_id` + character offsets. A dot without provenance is dropped, not repaired.
- Numbers, dates, versions, prices, and benchmark scores are extracted into typed fields so they can be cross-checked arithmetically.
- Contradiction detection: new dot vs. existing dots with cosine > 0.82 but conflicting typed values ⇒ create a `contradicts` edge and mark both for the fact-check gate.

### 10.5 Stage 4 — Synthesis

Input: the full mind map + memory gists. Provider: `TaskClass::Planner`.

Output: an **article blueprint** (not prose yet):

```json
{
  "angle": "the one non-obvious thing this article says",
  "why_now": "…",
  "reader_payoff": "…",
  "sections": [
    {"h2": "…", "purpose": "…", "dot_ids": ["…"], "needs_image": true,
     "image_intent": "diagram|product|person|chart|abstract",
     "open_loop": "question this section plants for the next"}
  ],
  "timeline": [...],            // X12+
  "contradictions_to_surface": [...],
  "confidence_map": {"section_1": 0.91, ...},
  "unknowns": ["what we could not establish"]
}
```

**[HARD REQ]** `unknowns` is never empty for X12/X24. Saying what is not known is a feature.

### 10.6 Stage 5 — Fact-check gate (blocking)

Runs before writing. Provider: `TaskClass::Editor` (different model than Writer where possible).

| Check | Rule | Fail action |
|---|---|---|
| Provenance | Every dot in the blueprint resolves to a live source row | Drop the dot |
| Corroboration | Numbers/dates/benchmarks need ≥ 2 independent sources or 1 tier-1 primary | Downgrade to attributed claim (“X says…”) |
| Recency | Claim older than the topic's volatility window (AI models: 90 days) is flagged | Add temporal qualifier |
| Contradiction | Unresolved conflicts must appear in the article, not be picked arbitrarily | Force a “what's disputed” passage |
| Arithmetic | Percentages, growth rates, and unit conversions recomputed in Rust | Correct or drop |
| Hallucination sweep | Every named entity/product/version must appear in ≥ 1 source text | Remove |
| Licence | Every quote under the length cap; every image licence resolved | Block publish |

Output: `fact_score` 0–100. Below 70 ⇒ mandatory human review regardless of settings.

### 10.7 Mind map — data structure and rendering [HARD REQ]

The mind map is a **first-class persisted artifact**, not a visualisation afterthought.

- Nodes are dots on a force-directed canvas; size = number of evidence dots; ring = authority; fill = status (frontier / exploring / explored / pruned).
- Edges are typed and labelled; contradiction edges render distinctly.
- The map builds **live** during the run — this is the product's signature moment. Each `MindMapDelta` event animates in a new dot with a short trace line from its parent. The user watches the research think.
- Interactions: click node → side panel with its dots, sources, and confidence; hover edge → relation + evidence; `Space` → pause the run; drag a node to the "focus" well → boost its priority and the engine re-plans the frontier around it; right-click → prune subtree.
- Export: PNG, SVG, and `mindmap.json` (schema versioned).
- Layout runs in Rust (`fdg`-style Barnes-Hut) and streams positions to the UI; the webview only paints. **No layout physics in JavaScript.**

### 10.8 Acceptance criteria

- [ ] X2 completes in ≤ 3 min and X24 in ≤ 90 min on the reference machine (8-core, 16 GB, local 8B model).
- [ ] Every tier meets its source and primary-source floors on 20 golden topics, or correctly flags `thin_evidence`.
- [ ] Anti-drift: on the golden set, ≤ 2 % of nodes at hop ≥ 3 are judged off-topic by human raters.
- [ ] Killing the app mid-expansion and restarting resumes the same session with no duplicate fetches.
- [ ] Every claim in a generated post is traceable to a dot → source → URL in one click.

---

## 11. M4 — Memory & the settings mind map  ·  `bhippi-memory`
**Squad:** Agents · **Tickets:** BHP-100…BHP-118

The system must get measurably better the more it runs. Memory is how.

### 11.1 Three tiers

| Tier | Contents | Lifetime | Retrieval |
|---|---|---|---|
| **Working** | Current session mind map, open charter, recent chat turns | Session | Direct |
| **Episodic** | One gist per session + per significant chat thread | 180 days, decayed | Vector + FTS hybrid |
| **Semantic** | Entity graph, canonical facts, source reputation learned over time | Permanent, versioned | Graph walk + vector |

### 11.2 Gist writer

On session completion (and every 30 chat turns), produce a gist:

```markdown
## <Topic> — <date> · tier X12 · fact_score 86
**Angle:** …
**Established:** (5–9 bullets, each linked to dot ids)
**Disputed:** …
**Unknown / to revisit:** …
**Entities touched:** …
**Sources that paid off:** domain → what it was good for
**Dead ends:** queries and paths that produced nothing (so we don't repeat them)
```

≤ 1200 tokens. Embedded and indexed. **Dead ends are mandatory** — negative knowledge saves the most time on future runs.

### 11.3 Retrieval into a new run

At Plan stage, retrieve: top-8 gists by hybrid score (0.6 vector + 0.4 BM25), the entity subgraph within 2 hops of the seed entities, and any canonical facts with `last_verified` inside the volatility window. Inject as a compact `PRIOR KNOWLEDGE` block with explicit staleness markers. **The planner is instructed to treat memory as a prior to verify, never as ground truth to repeat.** Anything from memory that reaches the article must be re-verified against a live source in this run.

### 11.4 Learning loops

1. **Source reputation** — track per-domain: dots contributed, corroboration rate, contradiction rate, extraction quality, latency. Adjust an internal `learned_trust` delta (bounded ±1 tier) that biases discovery over time.
2. **Query effectiveness** — record which search phrasings yielded tier ≤ 2 sources; reuse winning patterns, retire losers.
3. **Topic interest graph** — entities and subtopics gain weight from user engagement (opened, published, edited, shared) and lose weight on skip/reject. This graph is what Timer automation draws from (§13.2).
4. **Style memory** — accepted vs. edited hooks and headlines feed a preference file used by the Writer.

### 11.5 The Settings mind map [HARD REQ]

Settings → **Mind** shows the *global* knowledge map, distinct from the per-session map:

- **Constellation view:** entities as nodes, sized by mention count, coloured by kind, clustered by domain (AI research / chips / devtools / security / consumer / infra).
- **Session ribbon:** a horizontal time axis of every session; hover shows its gist; click drops the session's nodes onto the constellation so the user sees what that run added.
- **Coverage heat:** areas with many entities but few verified facts render dim — these are visible gaps, and they feed the Timer's topic picker.
- **Memory inspector:** searchable list of gists with `use_count`, `decay_score`, last used; per-item actions: pin (never decay), edit, delete, "re-verify now".
- **Controls:** decay half-life, max memory size, "forget topic X", full export (`memory.json`), and a hard **Wipe memory** with typed confirmation.

Decay: `decay_score *= 0.5^(days_idle / half_life)`; boosted on use. Below 0.15 and unpinned ⇒ archived (not deleted) after 180 days.

### 11.6 Acceptance criteria

- [ ] Second run on a related topic shows ≥ 30 % fewer redundant fetches than the first, measured on a fixed pair set.
- [ ] Gists never exceed 1200 tokens; retrieval block never exceeds 6 % of the planner's context window.
- [ ] Deleting an entity removes it from the graph, gists referencing it, and the FTS/vector indexes atomically.
- [ ] No memory-sourced claim appears in a published post without a live source from the current run.

---

## 12. M5 — Image pipeline  ·  `bhippi-vision`
**Squad:** Core + Agents · **Tickets:** BHP-130…BHP-152

Images are not decoration. Each one must earn its place against the section it serves.

### 12.1 Sourcing — licence-first [HARD REQ]

Candidates come only from, in priority order:

1. **Official press kits & company newsrooms** — product shots, logos, architecture diagrams, with press-use terms recorded.
2. **Open-licence repositories** — Wikimedia Commons (CC), Unsplash, Pexels, Openverse. Store licence + attribution string verbatim.
3. **Paper figures under fair-use quotation** — only with a caption naming paper, authors, and year, only when the figure is the subject of discussion, and only from open-access sources (arXiv, ACL Anthology).
4. **Generated diagrams** — the engine renders its own charts and diagrams (SVG, from dot data) when no licensed asset fits. Often the best answer.

**Absolutely excluded:** any image whose licence cannot be resolved to a named permission, hotlinked assets from news articles, stock-agency watermarked previews, paparazzi/celebrity photography, and any image of a private individual. `license = 'unknown'` ⇒ `status = 'rejected'`, full stop. The publisher refuses to build a post containing a rejected image.

### 12.2 Understanding [HARD REQ]

Every candidate goes through a vision model (`TaskClass::Vision`) that returns structured JSON:

```json
{
  "description": "what is literally in the frame",
  "contains_text": true, "ocr": "…",
  "subject_bbox": [x, y, w, h],       // normalised 0-1, main subject
  "safe_crop_region": [x, y, w, h],   // region that must survive any crop
  "image_kind": "product|diagram|chart|screenshot|portrait|abstract|logo|photo",
  "quality": {"sharpness": 0.0-1.0, "is_upscaled": false, "has_watermark": false},
  "relevance_to_section": 0.0-1.0,
  "suggested_alt": "≤ 125 chars, describes function not appearance",
  "concerns": ["contains a person's face", "text will be unreadable at width 400"]
}
```

Rejection rules: `relevance < 0.55`, watermark present, `sharpness < 0.35`, upscaling artefacts, unreadable text after downscale, or any `concerns` entry involving identifiable private individuals.

### 12.3 Cropping — saliency + intent aware

```
original ──► EXIF strip + orientation fix
   ──► phash dedupe (Hamming ≤ 6 vs. existing site images)
   ──► saliency map (Rust: edge density + colour variance + centre bias)
   ──► reconcile with vision model's subject_bbox and safe_crop_region
   ──► focal point (focal_x, focal_y)
   ──► crop set generated around focal point, never cutting safe_crop_region:
         hero_16x9      1600×900
         card_4x3       800×600
         og_1200x630    (title-safe zone respected)
         inline_3x2     1200×800
         thumb_1x1      400×400
   ──► resize (Lanczos3) ──► encode AVIF q60 + WebP q78 + JPEG q82 fallback
   ──► srcset widths [400, 800, 1200, 1600]
   ──► write blob + variants JSON
```

**Rules:** never upscale beyond source resolution; if the source is smaller than the target crop, drop to the next size or reject. Diagrams and screenshots are **never** cropped — they are letterboxed on a token-coloured background, because cropping a diagram destroys its meaning. Portraits crop with headroom, never at the neck.

### 12.4 Placement

One hero (mandatory), one image per 400–600 words, never two images adjacent without text between. Every image gets: alt text (functional, ≤ 125 chars), a caption when it adds context, and an attribution line rendered under it when the licence requires. Lazy-load everything below the fold; hero is preloaded and LCP-optimised.

### 12.5 Acceptance criteria

- [ ] 100 % of published images have a resolved licence and attribution string in the DB.
- [ ] No published crop cuts through a `safe_crop_region` — verified by an automated geometric test.
- [ ] Hero LCP ≤ 1.8 s on simulated 4G in the built site.
- [ ] A post whose images all fail licence resolution still builds, using engine-generated diagrams instead.

---

## 13. M6 — Writer  ·  `bhippi-writer`
**Squad:** Agents + Content-Ops · **Tickets:** BHP-160…BHP-184

### 13.1 Composition pipeline

Section-by-section, never one giant prompt. Each section receives only its own dots plus a 200-token running context of what has already been said (prevents repetition without blowing the window).

```
blueprint ──► headline set (12 candidates) ──► hook (5 candidates)
   ──► section drafts (parallel where independent, sequential where narrative)
   ──► transition weld pass (open loops closed/opened correctly)
   ──► editor pass (different provider): factual, structural, style
   ──► style enforcement (Rust linter, §13.4)
   ──► final assembly + markdown normalisation
```

### 13.2 The hook engine [HARD REQ]

The first 40 words decide whether the post is read. Generate **five** openings across distinct strategies, score them, pick one, keep the rest in the DB for the style-memory loop.

| Strategy | Shape | Use when |
|---|---|---|
| **Concrete anomaly** | A specific, surprising fact with a number | There is a strong verified stat |
| **Stakes reversal** | The thing everyone assumes, then what actually happened | There is a real contradiction in the dots |
| **Cold open scene** | A moment in time — a commit, a demo, a filing | The story has a datable event |
| **Direct question** | A question the reader is already half-asking | Topic is confusing/contested |
| **Cost line** | What this changes for the reader specifically | Practical/devtools topics |

Scoring (0–100, computed in Rust + one editor call): specificity (contains a verified number/name/date), curiosity gap (asks something it will answer), zero-cliché (blocklist below), reading ease, and truthfulness (the hook's claim must map to a dot with confidence ≥ 0.8).

**Banned openers, enforced by lint — the build fails, not warns:** "In today's fast-paced world", "In the ever-evolving landscape", "It's no secret that", "Imagine a world where", "buckle up", "game-changer", "revolutionise/revolutionary", "delve", "unleash", "seismic shift", "at the end of the day", "the rest is history", and any opening that restates the headline.

### 13.3 Structure

```
H1 headline (≤ 62 chars, primary keyword naturally placed, no clickbait promise
             the body does not keep)
Dek (1 sentence, ≤ 155 chars — doubles as meta description)
HOOK — 40–70 words, no throat-clearing
"What happened" — the news/thesis in ≤ 120 words, so a skimmer can leave informed
─ hero image ─
H2 sections (3–7), each:
    · opens with its own micro-hook or open loop
    · 2–4 short paragraphs (≤ 4 sentences each)
    · at least one concrete artefact: number, benchmark, code block, quote, chart
    · closes a loop or opens the next
Pull-out box: "What's disputed" (mandatory when contradictions exist)
Pull-out box: "What we still don't know" (mandatory X12+)
"Why it matters" — reader payoff, no filler
Sources — numbered, with publish dates and tier badges
Methodology footer — tier used, sources examined, session id, model(s) used
```

### 13.4 Style linter (Rust, deterministic, runs before editor)

Hard failures: banned-phrase hits · average sentence length > 24 words · paragraph > 5 sentences · passive-voice ratio > 20 % · any quote ≥ 15 words · > 1 quote from the same source · > 2 consecutive sentences starting with the same word · em-dash density > 1 per 120 words · heading not in sentence case · claim without a resolvable dot id · promotional adjective stack ("powerful, seamless, cutting-edge").

Warnings: reading grade > 12 · keyword density outside 0.6–1.6 % · section without a concrete artefact · > 25 % of sentences beginning with a subordinate clause.

### 13.5 Voice for Bhippi

Plain, confident, technically literate. Explains a mechanism rather than asserting an outcome. Comfortable saying "we don't know". Never breathless. Never "as an AI". Assumes a reader who works in or near tech and does not need "AI stands for artificial intelligence", but who has not read the paper.

### 13.6 Acceptance criteria

- [ ] Zero style-linter hard failures in any published post.
- [ ] Human raters prefer engine hooks over a baseline lead-paragraph in ≥ 70 % of blind pairs on 30 posts.
- [ ] Every paragraph maps to ≥ 1 dot; orphan paragraphs fail the build.

---

## 14. M7 — SEO & publishing  ·  `bhippi-seo`, `bhippi-publish`
**Squad:** Content-Ops + Frontend · **Tickets:** BHP-190…BHP-216

### 14.1 Keyword work

- Extract candidates from: dots, entity names, search queries that succeeded, ticker headline cluster, and search-suggest scrapes (`suggest` endpoints, respecting robots).
- Score each: intent match, competition proxy (result count + how many tier-1 domains rank), specificity, and internal-corpus gap (do we already own this term?).
- Pick 1 primary + 3–5 secondary + 5–10 semantic/entity terms.
- Placement contract: primary in H1, dek, first 100 words, one H2, URL slug, and image alt where honest. **No stuffing** — density outside 0.6–1.6 % fails the lint.
- Slug: `kebab-case`, ≤ 60 chars, stopwords stripped, date-free (so evergreen posts can be updated in place).

### 14.2 Metadata emitted per post

`<title>` (≤ 60 chars) · meta description (= dek, ≤ 155) · canonical · OpenGraph (og:image = the 1200×630 crop) · Twitter card · `article:published_time` / `modified_time` · author (Bhippi editorial) · JSON-LD `Article` + `BreadcrumbList` + `FAQPage` when the post contains a genuine Q&A block + `ImageObject` with licence + `Organization`.

**AI-disclosure [HARD REQ]:** every post carries a machine-readable and human-visible disclosure that it was researched and drafted by an automated system, with the review status (auto-published vs. human-reviewed). This is both an honesty requirement and increasingly a platform requirement. Do not make it removable in the UI.

### 14.3 Site-level SEO

`sitemap.xml` (auto, lastmod accurate) · `rss.xml` + `feed.json` · `robots.txt` · 301 map for slug changes · internal linking (§14.4) · breadcrumbs · tag/category pages with real intro copy, never bare lists · pagination with `rel=prev/next` · 404 page with search.

### 14.4 Internal linking [HARD REQ]

On publish, the linker runs over the whole corpus: embed the new post, find the 5 most related existing posts, insert 2–4 contextual links in the new post with descriptive anchor text (never "click here"), and insert 1–2 links **into older posts** pointing at the new one where a relevant passage exists. Every insertion is recorded so it can be reverted. This is what turns a pile of posts into a site that ranks.

### 14.5 Output targets

**Default: static HTML + CSS with islands.** The generator (`minijinja` + Rust) emits fully static pages; interactive pieces (search, mind-map viewer, ticker archive) are small hydrated islands loaded on demand. Rationale: fastest LCP, best crawlability, no framework tax on an article page.

**Also supported: React bundle.** The same content model emits a Vite + React 18 site (file-based routes, pre-rendered via SSG) for when the user wants app-like behaviour. Both renderers consume identical `post.json` — the content model is the contract, the renderer is swappable. **[HARD REQ]** No content logic may live in either template layer.

**Deploy adapters:** local directory (default) · Git commit + push (GitHub Pages) · Netlify API · Cloudflare Pages · WordPress REST (`/wp-json/wp/v2/posts`, with media upload). Each adapter implements:

```rust
#[async_trait] pub trait DeployTarget {
    async fn preflight(&self) -> Result<()>;            // creds, connectivity, quota
    async fn publish(&self, bundle: &SiteBundle) -> Result<DeployRef>;
    async fn rollback(&self, to: &DeployRef) -> Result<()>;
}
```

Every publish is atomic: build to a temp dir, verify (§14.6), swap, record `deploy_ref`. Rollback must restore the previous state in one command.

### 14.6 Pre-publish verification (blocking)

Build fails — not warns — on any of: broken internal link · missing image variant · unresolved image licence · style-linter hard failure · `fact_score < 70` without human approval · duplicate slug · missing meta description · missing AI disclosure · Lighthouse SEO < 95 or Performance < 90 on the built page (headless run in CI).

### 14.7 Blog theme spec — `themes/bhippi-default`

Minimal, reading-first, dark-primary. Content column 68ch. System font stack for body with one distinctive display face for headlines (self-hosted, subset, `font-display: swap`). Single accent colour used only for links, the ticker's live dot, and the reading-progress rail. No gradients, no shadows, no card decoration.

Components: header (wordmark + search + theme toggle) · **live ticker strip** (same feed as the app, optional) · article page (progress rail, sticky ToC on desktop ≥ 1100 px, sources block, mind-map embed toggle, related posts) · archive/list · tag page · about + methodology page **[HARD REQ]** explaining how posts are produced.

Budgets: ≤ 40 KB CSS, ≤ 25 KB JS on an article route, zero third-party scripts, zero cookies, no analytics by default.

---

## 15. M8 — Ticker  ·  `bhippi-ticker`
**Squad:** Core · **Tickets:** BHP-220…BHP-246

A live band of breaking tech/AI news across the top of the app — and the trigger for reactive publishing.

### 15.1 Feed layer

Sources come from `source_registry` where `feed_url IS NOT NULL`, seeded with reputable technology and AI outlets, primary research feeds (arXiv cs.AI / cs.LG / cs.CL listings), major lab and vendor engineering blogs, standards-body and regulator feeds, and high-signal aggregators (Hacker News front page API, GitHub trending). Users add or disable feeds in Settings; every feed shows its trust tier and last-fetch health.

Polling: `poll_secs` (default 120), staggered so all feeds never fire together, ETag/`If-Modified-Since` honoured, exponential backoff on failure, per-feed circuit breaker after 5 consecutive errors (surface it in Settings, do not hide it).

Optional API backends (user-supplied keys): a news API and GDELT for broader coverage. **Never required.** With zero keys the RSS layer alone must produce a working ticker.

### 15.2 Pipeline

```
poll ──► parse (feed-rs) ──► canonicalise URL ──► drop if seen
   ──► domain classifier: is this tech/AI?  (score < reject_threshold ⇒ discard)
   ──► category assign (ai-research | chips | devtools | security | infra |
                        consumer | robotics | space | policy-of-tech | business-of-tech)
   ──► cluster: title simhash + entity overlap + 6h window ⇒ cluster_id
   ──► burst detection: distinct domains in cluster ≥ burst_sources
   ──► velocity: outlets/hour over the last 3h
   ──► priority score
   ──► emit TickerEvent → UI strip + automation evaluator
```

**Priority score (0–100):**

```
priority = 28·source_trust_max        // tier 1 primary announcement scores high alone
         + 24·burst_normalised        // how many independent outlets
         + 18·velocity_normalised     // how fast it is spreading
         + 14·interest_match          // vs. the user's topic interest graph (§11.4)
         + 10·novelty                 // not already covered by an existing post
         +  6·primary_available       // a paper/changelog/filing exists to anchor on
         − 20·recently_covered_penalty
```

### 15.3 UI behaviour

A single 36 px strip under the title bar. Left: a pulsing dot + `LIVE` when polling is healthy, amber when a feed is failing. Then horizontally scrolling items: `[category] headline · source · relative time`. Speed 40 px/s, **pauses on hover and on focus**, respects `prefers-reduced-motion` by switching to a static rotating list. High-priority items (≥ 78) render with the accent colour and do not scroll past until seen once.

Click → detail popover: headline, cluster members with their outlets, primary source if detected, and three buttons: **Research now** (opens Research pre-filled with a tier picker), **Watch topic** (adds to interest graph), **Ignore** (suppresses the cluster for 72 h). Right side of the strip: a pause toggle and a counter of today's auto-triggered sessions.

### 15.4 Trigger contract [HARD REQ]

An event auto-triggers a session only when **all** hold: `priority ≥ auto_trigger_score` · `burst_count ≥ burst_sources` **or** the single source is tier-1 primary · `domain_score ≥ reject_threshold` · no existing post covers the cluster · daily post cap not reached · not inside quiet hours · budget guard green · no session already running for that cluster. Debounce: a cluster must be stable for 5 minutes before triggering — this prevents chasing a headline that gets corrected.

Ticker-origin sessions default to tier **X6** (speed matters for news), configurable, with `recency_pressure` boosted in the frontier formula and a mandatory primary-source hunt before writing.

### 15.5 Acceptance criteria

- [ ] With 25 feeds configured, steady-state CPU < 2 % and memory delta < 40 MB.
- [ ] The same wire story from 6 outlets produces exactly one cluster and one candidate session.
- [ ] A corrected/retracted headline within the debounce window does not produce a post.
- [ ] Killing the network for 10 minutes degrades the strip to amber and recovers with no duplicates.

---

## 16. M9 — Automation & scheduler  ·  `bhippi-core`
**Squad:** Core · **Tickets:** BHP-250…BHP-272

One screen, three switches, and a set of guardrails that make unattended operation safe.

### 16.1 Modes

**A · One-shot automation** — the big button. Topic in, finished post out: research → images → write → SEO → build → (review or publish). Progress renders as the live mind map plus a stage rail.

**B · Timer automation** — every `interval_mins`, the topic picker selects the next subject and runs the full chain. Picker logic, in order:
1. Highest-priority uncovered ticker cluster from the last 24 h.
2. Highest-weight gap in the coverage heat map (many entities, few verified facts).
3. A scheduled follow-up: a post older than 30 days whose entities have new activity ⇒ **update the existing post** rather than writing a near-duplicate (`refresh` mode: re-verify, amend, bump `modified_time`).
4. An explicit user queue (drag topics in from anywhere in the app).
Picker must never select a topic covered in the last 14 days unless mode is `refresh`.

**C · Ticker automation** — reactive, per §15.4.

Modes B and C can run together (`mode = "both"`); they share the daily cap and a single global queue.

### 16.2 Scheduler internals

`tokio-cron-scheduler` drives ticks; a bounded work queue (capacity 32) holds `Job { kind, payload, priority, attempts, not_before }`. **Exactly one research session runs at a time** — parallel sessions thrash the crawler and the local GPU. Publishing and image work may overlap the next session's planning.

Persistence: the queue lives in SQLite so a crash resumes cleanly. Jobs are idempotent; retries use `attempts` with backoff and a dead-letter table after 3 failures, surfaced in the UI as a card the user can inspect and requeue.

### 16.3 Guardrails [HARD REQ]

| Guard | Behaviour |
|---|---|
| Daily post cap | Hard stop at `daily_post_cap`; queue holds the rest |
| Quiet hours | No publishing (research may continue if `research_in_quiet = true`) |
| Budget guard | Token/wall/spend caps checked before every provider call; on breach, pause automation and notify |
| Duplicate guard | Slug + embedding similarity ≥ 0.93 against the corpus ⇒ refresh instead of new post |
| Review gate | When on, posts land in `review` and the tray badge counts them |
| Thin-evidence gate | Below floors ⇒ forced review regardless of settings |
| Kill switch | One click (and a global shortcut) stops everything, cancels in-flight jobs, leaves the DB consistent |
| Crash loop guard | 3 failed sessions in a row ⇒ automation disables itself and reports why |

### 16.4 Review queue UI

Posts awaiting review show: rendered preview, `fact_score`, `seo_score`, thin-evidence flags, contradictions surfaced, image licence summary, and a diff view for `refresh` updates. Actions: publish · edit (opens a minimal markdown editor with live lint) · send back for deeper research (re-runs at the next tier up, reusing the existing mind map) · reject with a reason (the reason feeds style/interest memory).

### 16.5 Acceptance criteria

- [ ] 72-hour unattended soak with timer + ticker on: no crash, no duplicate posts, caps respected exactly, memory growth < 150 MB.
- [ ] Kill switch stops all work within 3 s and leaves no orphaned rows or temp dirs.
- [ ] Power loss during publish leaves either the previous or the new site, never a half-written one.

---

## 17. M10 — Skills system  ·  `bhippi-skills`
**Squad:** Agents + Core · **Tickets:** BHP-280…BHP-308

The engine can author, test, and adopt its own reusable procedures. This is the self-upgrading loop — and the highest-risk subsystem in the product, so it ships with gates.

### 17.1 What a skill is

A versioned folder with a manifest. Three kinds:

| Kind | Body | Example |
|---|---|---|
| `prompt` | A parameterised prompt template | "Extract benchmark tables from a model card" |
| `script` | Rhai script (sandboxed) or WASM module (WASI p2) | "Normalise GPU spec units", "Parse arXiv listing page" |
| `composite` | A declarative pipeline of other skills + engine stages | "Model-launch coverage: primary hunt → benchmark table → competitor delta" |

```toml
# skills/user/benchmark-table-extractor/skill.toml
name        = "benchmark-table-extractor"
version     = "0.3.0"
kind        = "prompt"
created_by  = "engine"
description = "Pull benchmark rows out of model cards and papers into typed rows."
triggers    = ["model card", "eval results", "benchmark", "MMLU", "SWE-bench"]
inputs      = [{ name = "source_text", type = "string" },
               { name = "model_name",  type = "string" }]
output_schema = "schemas/benchmark_rows.json"
provider_hint = "Extractor"
autonomy    = "trial"          # proposed | trial | enabled | disabled | quarantined
capabilities = []              # script skills declare: net, fs_read, fs_write — default none
eval_set    = "evals/benchmark-table/*.json"
min_score   = 0.85
```

### 17.2 Lifecycle [HARD REQ]

```
OBSERVE      engine notices a procedure repeated ≥ 5 times across sessions with a
             stable shape (same input kind → same output kind)
   ▼
PROPOSE      SkillAuthor drafts manifest + body + an eval set of ≥ 10 cases drawn
             from real past sessions (inputs and known-good outputs)
   ▼
EVALUATE     run against the eval set in the sandbox; score = correctness ×
             schema-validity × latency-improvement vs. the ad-hoc baseline
   ▼
TRIAL        auto-enabled at 'trial' only if score ≥ min_score; used on real work but
             shadow-compared against the baseline for 20 runs
   ▼
ENABLE       promoted only when trial win-rate ≥ 60 % AND (autonomy_gate satisfied)
   ▼
MONITOR      rolling win-rate; 3 consecutive failures or score drop > 15 % ⇒ auto-
             quarantine + notify
```

**Autonomy gate:** `prompt` skills with no capabilities may auto-promote. **`script` skills, and any skill requesting `net` or `fs_write`, always require explicit user approval in Settings → Skills before leaving `trial`.** No exceptions, no "silent mode". The user gets a diff view of exactly what the engine wrote.

### 17.3 Sandbox [HARD REQ]

- Rhai: no filesystem, no network, no host functions beyond a whitelisted stdlib; 200 ms CPU budget; 8 MB memory; operation counter to kill runaway loops.
- WASM (`wasmtime`, WASI p2): capability-based — the module receives only the preopens and the outbound-host allowlist its manifest declared and the user approved; fuel-metered; 2 s wall; 64 MB memory.
- Skills never see: API keys, the keychain, the DB handle, the user's filesystem outside the session scratch dir, or raw provider clients. They call back into the engine through a narrow, audited host API.
- Every skill invocation is logged with inputs (hashed), duration, result status, and version.

### 17.4 Skills UI (Settings → Skills)

Two panes. Left: registry list with name, version, kind, autonomy state, win-rate sparkline, last run. Right: detail — manifest, body (syntax highlighted, read-only unless editing), eval results table, run history, capability requests rendered as an explicit permission list.

Actions: create skill (guided: describe intent → engine drafts → user edits → eval runs → enable) · edit + bump version · run against a test input · promote/demote autonomy · quarantine · delete · export/import as a `.bhippi-skill` folder.

**Pending approvals** surface as a badge on the Settings icon. Nothing dangerous activates while the user is not looking.

### 17.5 Acceptance criteria

- [ ] An engine-authored skill cannot reach the network or the filesystem without a recorded user approval — proven by a red-team test in CI.
- [ ] A deliberately broken skill is quarantined within 3 runs and never blocks a session (the baseline path always remains).
- [ ] Skill invocation adds < 15 ms overhead over a direct call.

---

## 18. M11 — Settings panel
**Squad:** Frontend + Core · **Tickets:** BHP-320…BHP-344

One modal, seven tabs, left rail navigation, changes apply immediately and persist on blur (no global Save button, no dialog that can be lost).

| Tab | Contents |
|---|---|
| **Providers** | Detected providers grouped by kind (CLI / API / Local), each row: vendor icon, model, context window, vision/tools badges, measured tok/s, health dot, enable toggle. `Re-scan` button with live progress. `Add manually` (base URL + model + optional key → keychain). Routing policy selector (Quality / Balanced / Cheap / Local-only) with a plain-language explanation of what each does. `Offline mode` master switch. Per-TaskClass override table for power users, collapsed by default. |
| **Research** | Default tier with the full budget table rendered so the user sees exactly what each tier buys. Anti-drift threshold. Counter-evidence toggle. Language. Search backend (SearXNG / Brave / Tavily / DDG) + test button. Per-host rate limit. Concurrency. |
| **Ticker** | Feed table (name, tier, category, last fetch, health, enable). Add feed by URL with auto-discovery of `<link rel=alternate>`. Poll interval. Burst threshold. Auto-trigger score with a live histogram of the last 200 events showing how many would have triggered — so the number means something. Category filters. |
| **Automation** | Mode (Off / Timer / Ticker / Both). Interval. Daily cap. Quiet hours. Review gate. Refresh-vs-new policy. Budget caps. Kill switch. A plain-English summary sentence that updates live: "Bhippi will research and publish up to 4 posts a day, between 07:00 and 23:30, with your review before publishing." |
| **Mind** | The global memory map and inspector (§11.5). |
| **Skills** | The skill registry (§17.4). |
| **Publishing** | Site name, URL, author identity, theme (Static / React), deploy target + credentials (keychain), build + preview button, deploy history with rollback, SEO defaults, disclosure text preview (non-removable). |

Plus a footer strip: data directory path (click to open), DB size, `Run doctor`, export everything, and version + update check.

---

## 19. M12 — UI/UX specification
**Squad:** Frontend · **Tickets:** BHP-350…BHP-386

### 19.1 Direction

**Instrument, not dashboard.** The reference object is a piece of lab equipment: a dark, quiet, high-density surface where the only thing that moves is the thing that is actually happening. The mind map building itself in real time is the signature moment of the product — everything else in the UI is deliberately still so that motion means something.

### 19.2 Tokens

```
--bg          #0B0C0E   canvas
--surface     #141518   panels
--surface-2   #1B1D21   raised
--line        #26282D   hairlines (1px, never shadows)
--text        #E8E9EB
--text-dim    #8A8F98
--text-faint  #565B64
--accent      #4ADE9B   live/active only: ticker dot, running node, progress rail
--warn        #E3B341
--error       #E06C6C
--radius      4px       (6px on modals; nothing rounder)
--space       4px grid, steps 4/8/12/16/24/32/48
```

Type: UI in `Inter` / system sans at 13 px base, 1.5 line-height. Data, URLs, and IDs in `JetBrains Mono` 12 px. Headings 15/18/22 px, weight 500 — never 700, never uppercase except 10 px tracked eyebrows. Exactly two type sizes per screen region.

Motion: 120 ms ease-out for state, 200 ms for panels, spring only for mind-map node entry. Everything respects `prefers-reduced-motion`. No skeleton shimmer — use a hairline progress rail instead.

### 19.3 Screens

**Chrome (persistent):** 36 px ticker strip · 44 px title bar (wordmark, 4 screen tabs, session status pill, settings gear) · status bar (active provider, tokens used today, queue depth, kill switch).

**1 · Research** — the default screen.
```
┌ ticker ────────────────────────────────────────────────────────────────┐
├ Research | Automation | Library | Settings ────────────── ● running ────┤
│                                                                        │
│   ┌ topic input ─────────────────────────────────────────────────┐     │
│   │  What should Bhippi research?                          [↵]   │     │
│   └──────────────────────────────────────────────────────────────┘     │
│      ( X2 )  ( X6 )  ( X12 )  ( X24 )        ← segmented, X6 default    │
│      12 expansions · ~60-90 sources · ~30 min · 2000-3000 words         │
│                                                                        │
│   ┌ mind map canvas ─────────────────────────┐ ┌ inspector ───────┐    │
│   │                                          │ │ node: "MoE       │    │
│   │        ○───●───○                         │ │ routing collapse"│    │
│   │       ╱    │    ╲                        │ │ 7 dots           │    │
│   │      ●     ●     ○  ← frontier (dim)     │ │ ─────────────────│    │
│   │       ╲   ╱                              │ │ • 3.2× fewer …   │    │
│   │        ● ●  ← contradiction edge (red)   │ │   arxiv.org  T1  │    │
│   │                                          │ │ • Vendor claims… │    │
│   └──────────────────────────────────────────┘ └──────────────────┘    │
│   stage rail: plan ▓▓ expand ▓▓▓▓▓░░ synth ░ facts ░ write ░ publish ░  │
│   47 sources · 168 dots · 9 primary · 4 contradictions · 6:12 elapsed   │
└────────────────────────────────────────────────────────────────────────┘
```
Tier chips show their budget contract on hover — the user always knows what they are buying. `Space` pauses, `Esc` cancels with confirmation, `F` focuses a node, `/` searches dots.

**2 · Automation** — mode switches, the plain-English summary sentence, next-run countdown, the queue (drag to reorder), today's runs with outcomes, the review queue, and the dead-letter card if anything failed.

**3 · Library** — published and draft posts as a dense table (title, date, tier, words, fact score, SEO score, status, views if a target reports them). Row click → preview + metadata + "open mind map that produced this" + refresh/retract actions.

**4 · Settings** — §18.

### 19.4 Empty, loading, and error states

Empty Research: the input, the tier row, and one line — "Type a topic, or pick a story from the ticker." Nothing else. Empty Library: "Nothing published yet. Run a research session or turn on automation." Errors are single-line, specific, and carry the fix: "Ollama isn't responding on :11434 — start it, or switch routing to Cloud." Never a toast that disappears before it is read; errors persist in the status bar until dismissed.

### 19.5 Accessibility floor [HARD REQ]

Keyboard reachable everywhere, visible focus rings on the accent colour, AA contrast minimum (verify the dim tokens), the ticker pausable and reduced-motion aware, the mind map fully navigable as a tree list for screen readers (`role="tree"` mirror of the graph), no colour-only meaning (contradiction edges also dashed).

---

## 20. Security, privacy & secrets

- Secrets in the OS keychain via `keyring` only. Never in `config.toml`, never in logs, never in the DB, never in a crash report. A pre-commit hook + CI scan blocks accidental key strings.
- Provider CLIs are invoked with an explicit argv (never a shell string), a scrubbed environment, and a timeout. No interpolation of untrusted text into command lines.
- All fetched content is **untrusted data**. Prompt-injection defence: fetched text is wrapped in a delimited data block with an instruction that content inside is data, never instructions; the extractor runs with a JSON schema so an injected instruction cannot change the output shape; any dot whose claim text contains imperative patterns aimed at the system ("ignore previous", "publish this as", "run the following") is dropped and logged as a `suspicious_source` incident visible in the UI.
- Skills sandboxing per §17.3.
- Telemetry off by default and off in fact — no network call exists for it in v1.
- The DB is local. `Export everything` produces a portable zip; `Wipe` is real (VACUUM + blob delete), not a flag.
- Update channel: signed releases, checksum verified, user-initiated.

---

## 21. Editorial integrity & legal compliance [HARD REQ]

This product publishes automatically. That makes the following engineering requirements, not policy preferences:

1. **Copyright.** Paraphrase by default. Quotes under 15 words, at most one per source, enforced in code at extraction *and* at lint. Never reproduce an article's structure, section order, or narrative flow. Never reproduce lyrics, poetry, or long passages. Summaries must be substantially shorter and fully reworded. Wire-service text is never republished.
2. **Images.** Licence resolved or the image does not ship (§12.1).
3. **Paywalls and robots.** Obeyed, always, with no bypass path in the codebase (§9.1).
4. **Attribution.** Every source linked, dated, and tiered. When a scoop belongs to one outlet, the post says so and links it prominently — credit is both the ethical and the SEO-correct move.
5. **AI disclosure.** Non-removable, machine-readable and visible (§14.2).
6. **Corrections.** A retraction/correction workflow: `posts.status = 'retracted'` renders a visible correction notice and preserves the original text struck through; the URL is never silently rewritten.
7. **Defamation surface.** Claims about named people or companies that are negative and uncorroborated are blocked at the fact-check gate. Tier-4 sources can never support them.
8. **Person imagery.** No images of identifiable private individuals; public figures only from press kits or open-licence archives.

A post that cannot satisfy all eight is held, not published. Build this into the gate, not into a checklist somebody is supposed to remember.

---

## 22. Observability

- `tracing` spans per stage with the session id as a field; JSON rolling logs at `~/.bhippi/logs`, 7-day retention, secrets scrubbed.
- Live metrics surfaced in the UI status bar and stored per session: fetches, cache hit rate, bytes, tokens by provider and task class, wall time by stage, dots per source, primary-source ratio, fact score, lint failures, publish latency.
- `bhippi doctor`: schema check, index integrity, blob orphan scan, provider health, feed health, disk usage, and a one-page report.
- Session replay: given a session id, dump the exact prompts (with pinned versions), inputs, and outputs to a folder for debugging. **This is how we debug quality regressions — build it in sprint 1, not at the end.**

---

## 23. Testing strategy

| Layer | What | Bar |
|---|---|---|
| Unit | Scoring, dedupe, simhash, budget math, crop geometry, lint rules | ≥ 80 % line coverage on `bhippi-research`, `bhippi-harvest`, `bhippi-seo` |
| Fixture | 50 frozen HTML pages, 20 feeds, 30 images, 10 PDFs in `tests/fixtures` | Extraction F1 ≥ 0.92; zero network in these tests |
| Golden topics | 20 seed topics × 4 tiers, run nightly against a pinned local model | Source/primary floors met; drift ≤ 2 %; no lint failures |
| Contract | Provider trait conformance suite each backend must pass | All backends green or explicitly `unsupported` |
| E2E | Headless: topic → published site in a temp dir; ticker fixture → auto-publish | Green on every PR |
| Soak | 72 h automation run | §16.5 criteria |
| Red team | Prompt injection corpus, malicious skill, hostile robots, zip-bomb image, 500 MB page | All blocked, none crash |
| Site | Lighthouse on 10 generated posts | SEO ≥ 95, Perf ≥ 90, A11y ≥ 95 |

CI: `fmt` → `clippy -D warnings` → test → fixture → e2e → build matrix (macOS arm64/x64, Windows x64, Linux x64). Nightly: golden topics + soak subset.

---

## 24. Performance budgets [HARD REQ]

| Metric | Budget |
|---|---|
| Cold start to interactive | ≤ 1.2 s |
| Provider detection (all strategies) | ≤ 1.5 s, non-blocking |
| Idle CPU (ticker on, 25 feeds) | < 2 % |
| Idle RSS | < 220 MB |
| Peak RSS during X24 | < 900 MB |
| Mind map render, 500 nodes | ≥ 55 fps |
| DB after 1000 sessions | < 3 GB incl. blobs |
| Published article page | ≤ 120 KB total, LCP ≤ 1.8 s on 4G |

---

## 25. IPC surface (Tauri commands)

Typed via `specta` → generated `ui/src/lib/ipc.ts`. **Hand-written TS types for IPC are forbidden.**

```rust
// commands
research_start(topic: String, tier: Tier, opts: ResearchOpts) -> SessionId
research_pause(id) / research_resume(id) / research_cancel(id)
research_focus_node(id, node_id)          // user boosts a branch mid-run
session_get(id) -> SessionDetail
mindmap_get(id) -> MindMap
mindmap_export(id, format) -> PathBuf
post_preview(session_id) -> PostPreview
post_publish(post_id) -> DeployRef
post_refresh(post_id) -> SessionId
post_retract(post_id, reason)
review_queue() -> Vec<PostSummary>
ticker_stream() -> (event channel)
ticker_trigger(event_id, tier) -> SessionId
automation_set(config) / automation_status() -> AutomationStatus
providers_scan() -> Vec<ProviderInfo>
providers_set_enabled(id, bool) / providers_add_manual(spec)
memory_search(query) -> Vec<Gist>
memory_graph(filter) -> EntityGraph
memory_forget(target)
skills_list() / skills_create(intent) / skills_eval(id) / skills_set_autonomy(id, level)
settings_get() / settings_patch(json)
doctor_run() -> DoctorReport
kill_switch()

// events (server → UI)
session.stage_changed · mindmap.delta · dot.added · source.fetched
provider.health · ticker.event · automation.tick · publish.progress
budget.warning · error.raised · skill.pending_approval
```

Backpressure: `mindmap.delta` and `dot.added` are coalesced to at most 20 emissions/second; the UI never receives an unthrottled firehose.

---

## 26. Delivery plan

### 26.1 Squads

| Squad | People | Owns |
|---|---|---|
| **Core** | 2 Rust engineers | orchestrator, harvest, ticker, scheduler, db, publish |
| **Agents** | 2 Rust/ML engineers | providers, research engine, memory, vision, writer, skills |
| **Frontend** | 1 engineer + design support | Tauri shell, 4 screens, mind map renderer, theme |
| **Content-Ops** | 1 (part-time) | source registry, prompt tuning, style guide, golden topics, SEO QA |
| **QA/DevEx** | 1 | CI, fixtures, soak, red team, release pipeline |

### 26.2 Sprints (2 weeks each)

**S0 · Foundations** — workspace, crates, CI, `bhippi-db` + migrations, config + keychain, event bus, `tracing`, session replay dumper, Tauri shell with empty screens and the token system.
*Exit:* app opens, DB migrates, logs write, CI green on three platforms.
`BHP-001…009`

**S1 · Providers** — detection (all four strategies), capability probe, trait + streaming, routing, fallback, budget guard, Settings → Providers tab.
*Exit:* a prompt runs end-to-end on Ollama and on one CLI provider, with a visible fallback when one is killed.
`BHP-010…024`

**S2 · Harvest** — HTTP client, robots, rate limiting, extraction, PDF, dedupe (3 layers), blob store, source registry seed, fixture suite.
*Exit:* extraction F1 ≥ 0.92 on fixtures; robots honoured under test.
`BHP-030…048`

**S3 · Research engine I** — planner, charter, expander loop, dot extraction, frontier scoring, anti-drift, persistence + resume.
*Exit:* X2 and X6 produce complete mind maps on golden topics within budget.
`BHP-060…078`

**S4 · Research engine II + mind map UI** — X12/X24 budgets, counter-evidence pass, timeline reconstruction, synthesis blueprint, fact-check gate, Rust layout engine + live canvas + inspector.
*Exit:* the live map is watchable and smooth at 500 nodes; fact gate blocks a seeded bad claim.
`BHP-079…092`

**S5 · Memory** — embeddings, vector + FTS hybrid retrieval, gist writer, entity graph, decay, learning loops, Settings → Mind.
*Exit:* the redundant-fetch reduction target is met on the paired set.
`BHP-100…118`

**S6 · Vision** — sourcing with licence gates, vision understanding, saliency crop, variant encoding, placement, attribution rendering.
*Exit:* every image in a generated post has a licence row and a geometry-verified crop.
`BHP-130…152`

**S7 · Writer + SEO** — composition pipeline, hook engine, style linter, editor pass, keywords, metadata, schema, internal linker.
*Exit:* zero hard lint failures across 30 generated posts; hook A/B target met.
`BHP-160…184` · `BHP-190…202`

**S8 · Publish + theme** — static generator, React renderer, deploy adapters, atomic publish + rollback, pre-publish verification, `bhippi-default` theme, methodology page.
*Exit:* Lighthouse SEO ≥ 95 / Perf ≥ 90 on 10 posts; rollback restores previous site.
`BHP-203…216`

**S9 · Ticker + automation** — feed layer, clustering, burst detection, priority, ticker strip UI, trigger contract, scheduler, guardrails, review queue, kill switch.
*Exit:* fixture wire-story test yields one session; 24 h soak clean.
`BHP-220…272`

**S10 · Skills** — manifest, registry, sandbox (Rhai + WASM), authoring flow, evaluator, autonomy gates, Skills UI.
*Exit:* red-team suite green; a real engine-authored skill beats its baseline in trial.
`BHP-280…308`

**S11 · Hardening + beta** — 72 h soak, perf budgets, accessibility pass, error copy pass, doctor, installers + signing, docs.
*Exit:* all §24 budgets met; zero P0/P1 open.
`BHP-320…386`

### 26.3 Definition of done (every ticket)

Code + tests + docs in the same PR · `clippy -D warnings` clean · no `unwrap()` outside tests · errors typed and actionable · tracing spans added · IPC types regenerated if the surface changed · acceptance criteria from this document demonstrably met (link the test) · reviewed by someone outside your squad.

---

## 27. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Local models too weak for the Writer role | Poor prose kills the product | Writer defaults to the best available provider; ship a "local quality mode" with tighter blueprints, shorter sections, and more aggressive lint; document a recommended local model floor (≥ 14B for writing) |
| Research drifts off-topic at depth | Incoherent articles | Anti-drift cosine guard, charter-anchored gap scoring, human raters on the golden set every sprint |
| Auto-publishing something wrong | Reputational and legal harm | Fact gate, corroboration floors, thin-evidence hold, review gate on by default, one-click retraction with visible correction |
| Crawling gets the user blocked | Product stops working | Conservative rate limits, honest UA, robots obeyed, feeds preferred over scraping, per-host circuit breakers |
| Copyright/licence exposure | Takedowns | Enforced quote caps in code, licence-gated images, paraphrase-first extraction, prominent source credit |
| Prompt injection from a hostile page | Engine manipulated into publishing attacker text | Data-delimited prompts, schema-constrained extraction, imperative-pattern filter, suspicious-source incidents surfaced |
| Self-authored skills degrade quality | Silent, compounding regressions | Eval before trial, shadow comparison, win-rate monitoring, auto-quarantine, capability approval gates |
| SQLite contention at scale | Stalls | WAL, single-writer discipline through a repository layer, batched inserts, quarterly VACUUM in doctor |
| Scope creep into a general research tool | Never ships | Domain lock is a hard requirement; new categories require a written decision change |

---

## Appendix A — `post.json` content model (renderer contract)

```json
{
  "schema": "bhippi.post/1",
  "id": "01J…", "slug": "…", "title": "…", "dek": "…",
  "published_at": "…", "updated_at": null,
  "tier": "X12", "session_id": "01J…",
  "hook": "…",
  "blocks": [
    {"type": "paragraph", "md": "…", "dots": ["01J…"]},
    {"type": "heading", "level": 2, "text": "…"},
    {"type": "image", "image_id": "01J…", "variant": "inline_3x2",
     "caption": "…", "alt": "…", "attribution": "…"},
    {"type": "chart", "spec": {...}, "source_dots": ["…"]},
    {"type": "code", "lang": "python", "text": "…", "source_id": "…"},
    {"type": "callout", "variant": "disputed|unknown|context", "md": "…"},
    {"type": "quote", "text": "≤15 words", "attribution": "…", "source_id": "…"}
  ],
  "sources": [{"n": 1, "title": "…", "url": "…", "domain": "…",
               "published_at": "…", "tier": 1}],
  "seo": {"primary_kw": "…", "secondary": ["…"], "meta_desc": "…",
          "og_image": "…", "jsonld": {...}},
  "scores": {"fact": 86, "seo": 93},
  "disclosure": {"generated": true, "reviewed_by_human": false,
                 "model_roles": {"writer": "…", "editor": "…"}},
  "mindmap_ref": "mindmaps/01J….json"
}
```

## Appendix B — Prompt template header (all files in `prompts/`)

```markdown
---
id: research.planner
version: 4
task_class: Planner
output: json
schema: schemas/charter.json
max_tokens: 2000
temperature: 0.3
notes: Domain gate lives here — do not duplicate it downstream.
---
```

## Appendix C — Source registry seed shape

```json
{"domain":"arxiv.org","name":"arXiv","trust_tier":1,
 "categories":["ai-research","primary"],
 "feed_url":"…","robots_note":"API preferred over HTML scraping","enabled":true}
```

Seeding is Content-Ops' deliverable in S2: ~120 domains across tiers 1–3, every entry with a category, a feed where one exists, and a note on its robots/API policy. Tier-4 domains are listed explicitly so the engine knows to treat them as leads, not evidence.

## Appendix D — CLI (headless parity)

```
bhippi research "<topic>" --tier X12 [--publish] [--dry-run]
bhippi ticker watch
bhippi automation {on|off|status}
bhippi publish <post-id> | bhippi rollback <deploy-ref>
bhippi memory {search|export|forget} …
bhippi skills {list|eval|approve} …
bhippi doctor | bhippi replay <session-id>
```

Everything the GUI can do, the CLI can do. The GUI is a client of the same core.

---

**End of specification v1.0.**
Questions, objections, and decision changes go in the tracker with the `DECISION-CHANGE` label. Do not build around a disagreement silently — raise it, we settle it, we update this document and bump the version.
