# Bhippi — Data Model
**Doc:** `03-DATA-MODEL.md` · **Derives from:** spec §7 · **Status:** authoritative
**Owner:** Core · **Crate:** `bhippi-db`

Spec §7 defines the core tables. This document (a) restates the rules that govern all of
them, (b) adds the tables the spec's behaviour requires but did not spell out, and (c) fixes
the migration and index plan. **The spec's table definitions are canonical for their
columns; the additions here are canonical for theirs.**

---

## 1. Global rules

| Rule | Detail |
|---|---|
| Engine | SQLite, WAL, `foreign_keys = ON`, `synchronous = NORMAL`, `busy_timeout = 5000` |
| IDs | ULID, 26 chars, `TEXT PRIMARY KEY`, sortable by creation |
| Time | UTC ISO-8601 strings, always. No local time, no epoch ints |
| Writes | one writer connection behind the repository layer; reads from a pool |
| Batching | dots/sources insert in chunks of 64 inside one transaction |
| Deletes | cascade from `sessions`; blobs are removed by `doctor`, not by triggers |
| Vectors | `sqlite-vec` virtual tables, one per embedded entity kind |
| FTS | Tantivy on disk (not FTS5) — dots and post bodies |
| Migrations | forward-only, numbered, idempotent, never edited after merge |

**Embedding dimension is pinned** by `fastembed`'s `bge-small-en-v1.5` (384). Changing the
embedding model is a migration that re-embeds, not a config toggle — record it as an ADR.

---

## 2. Table map

### 2.1 From the spec (canonical there)

`sessions` · `nodes` · `edges` · `dots` · `sources` · `source_registry` · `images` ·
`memory_gists` · `entities` · `entity_links` · `chat_turns` · `ticker_events` · `posts` ·
`skills` · `providers`

### 2.2 Added here (migration `0002_operations.sql`)

These exist because the spec's *behaviour* requires persisted state that §7 does not define.

```sql
-- Job queue: automation survives a crash (spec §16.2)
CREATE TABLE jobs (
  id           TEXT PRIMARY KEY,
  kind         TEXT NOT NULL,      -- research|refresh|publish|image|gist|skill_eval|deploy
  payload      TEXT NOT NULL,      -- JSON
  priority     REAL NOT NULL DEFAULT 0,
  attempts     INTEGER NOT NULL DEFAULT 0,
  not_before   TEXT,
  state        TEXT NOT NULL,      -- queued|running|done|failed|dead
  last_error   TEXT,
  created_at   TEXT NOT NULL,
  updated_at   TEXT NOT NULL
);
CREATE INDEX idx_jobs_ready ON jobs(state, not_before, priority DESC);

-- Dead letters surface in the UI as inspectable, requeueable cards
CREATE TABLE dead_letters (
  id           TEXT PRIMARY KEY,
  job_id       TEXT NOT NULL,
  kind         TEXT NOT NULL,
  payload      TEXT NOT NULL,
  error        TEXT NOT NULL,
  attempts     INTEGER NOT NULL,
  created_at   TEXT NOT NULL,
  acknowledged INTEGER NOT NULL DEFAULT 0
);

-- Prompt provenance: a published post must be reproducible (spec §5 rule)
CREATE TABLE prompt_versions (
  hash         TEXT PRIMARY KEY,   -- blake3 of the file
  prompt_id    TEXT NOT NULL,      -- e.g. research.planner
  version      INTEGER NOT NULL,
  task_class   TEXT NOT NULL,
  path         TEXT NOT NULL,
  first_seen   TEXT NOT NULL
);

-- Internal-link insertions are revertible (spec §14.4)
CREATE TABLE link_edits (
  id           TEXT PRIMARY KEY,
  from_post    TEXT NOT NULL REFERENCES posts(id),
  to_post      TEXT NOT NULL REFERENCES posts(id),
  anchor_text  TEXT NOT NULL,
  block_index  INTEGER NOT NULL,
  char_offset  INTEGER NOT NULL,
  inserted_at  TEXT NOT NULL,
  reverted_at  TEXT
);

-- 301 map so slug changes never break a URL
CREATE TABLE redirects (
  from_slug    TEXT PRIMARY KEY,
  to_slug      TEXT NOT NULL,
  created_at   TEXT NOT NULL
);

-- Every deploy, for rollback and history
CREATE TABLE deploys (
  id           TEXT PRIMARY KEY,
  target       TEXT NOT NULL,      -- local|git-pages|netlify|cloudflare|wordpress
  deploy_ref   TEXT NOT NULL,
  bundle_hash  TEXT NOT NULL,
  post_count   INTEGER NOT NULL,
  started_at   TEXT NOT NULL,
  finished_at  TEXT,
  status       TEXT NOT NULL,      -- building|verifying|live|failed|rolled-back
  report       TEXT                -- JSON verify report
);

-- Prompt-injection and hostile-source incidents, visible in the UI (spec §20)
CREATE TABLE incidents (
  id           TEXT PRIMARY KEY,
  kind         TEXT NOT NULL,      -- suspicious_source|licence_block|skill_quarantine|budget_breach
  session_id   TEXT REFERENCES sessions(id),
  source_id    TEXT REFERENCES sources(id),
  detail       TEXT NOT NULL,
  created_at   TEXT NOT NULL,
  acknowledged INTEGER NOT NULL DEFAULT 0
);

-- Learning loops (spec §11.4) need somewhere to learn into
CREATE TABLE domain_stats (
  domain            TEXT PRIMARY KEY,
  dots_contributed  INTEGER DEFAULT 0,
  corroboration_rate REAL,
  contradiction_rate REAL,
  extraction_quality REAL,
  avg_latency_ms    INTEGER,
  learned_trust     REAL DEFAULT 0,   -- bounded [-1, +1] tier delta
  updated_at        TEXT NOT NULL
);

CREATE TABLE query_stats (
  id           TEXT PRIMARY KEY,
  pattern      TEXT NOT NULL,       -- normalised search phrasing
  uses         INTEGER DEFAULT 0,
  tier1_hits   INTEGER DEFAULT 0,
  tier2_hits   INTEGER DEFAULT 0,
  zero_yield   INTEGER DEFAULT 0,
  score        REAL,
  updated_at   TEXT NOT NULL
);

CREATE TABLE interest_weights (
  key          TEXT PRIMARY KEY,    -- entity id or subtopic slug
  kind         TEXT NOT NULL,       -- entity|subtopic|category
  weight       REAL NOT NULL DEFAULT 0,
  last_event   TEXT,                -- opened|published|edited|skipped|rejected
  updated_at   TEXT NOT NULL
);

-- Style memory (spec §11.4.4): accepted vs edited hooks and headlines
CREATE TABLE style_prefs (
  id           TEXT PRIMARY KEY,
  kind         TEXT NOT NULL,       -- hook|headline|dek
  candidate    TEXT NOT NULL,
  chosen       INTEGER NOT NULL,    -- 0|1
  edited_to    TEXT,
  strategy     TEXT,
  post_id      TEXT REFERENCES posts(id),
  created_at   TEXT NOT NULL
);

-- Skill run audit (spec §17.3)
CREATE TABLE skill_runs (
  id           TEXT PRIMARY KEY,
  skill_id     TEXT NOT NULL REFERENCES skills(id),
  version      TEXT NOT NULL,
  input_hash   TEXT NOT NULL,
  duration_ms  INTEGER NOT NULL,
  status       TEXT NOT NULL,       -- ok|schema_fail|timeout|error|quarantined
  shadow_win   INTEGER,             -- 1 = beat baseline, 0 = lost, NULL = not shadowed
  created_at   TEXT NOT NULL
);

-- Per-session metrics (spec §22) so quality regressions are diffable
CREATE TABLE session_metrics (
  session_id     TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
  fetches        INTEGER, cache_hits INTEGER, bytes INTEGER,
  tokens_json    TEXT,              -- {provider: {task_class: n}}
  stage_ms_json  TEXT,              -- {stage: ms}
  dots_per_source REAL,
  primary_ratio  REAL,
  fact_score     INTEGER,
  lint_failures  INTEGER,
  publish_ms     INTEGER
);
```

### 2.3 Column additions to spec tables (migration `0003`)

| Table | Column | Why |
|---|---|---|
| `sessions` | `charter TEXT` | resume from planning without re-planning |
| `sessions` | `blueprint TEXT` | resume from synthesis |
| `sessions` | `writer_provider TEXT` | enforce Editor ≠ Writer across restarts |
| `sessions` | `flags TEXT` | JSON: `thin_evidence`, `held_for_review`, `refresh_of` |
| `posts` | `disclosure TEXT NOT NULL` | machine-readable AI disclosure, non-removable |
| `posts` | `correction TEXT` | retraction/correction notice text |
| `sources` | `learned_trust_at_fetch REAL` | reproduce a run's trust decisions |

---

## 3. Index plan

```sql
CREATE INDEX idx_nodes_frontier   ON nodes(session_id, status, priority DESC);   -- spec
CREATE INDEX idx_dots_node        ON dots(node_id);                              -- spec
CREATE INDEX idx_dots_session     ON dots(session_id);
CREATE UNIQUE INDEX ux_sources_canon ON sources(canonical_url);                  -- spec
CREATE INDEX idx_sources_domain   ON sources(domain, fetched_at DESC);
CREATE INDEX idx_sources_simhash  ON sources(simhash);
CREATE INDEX idx_ticker_cluster   ON ticker_events(cluster_id, published_at DESC);
CREATE INDEX idx_ticker_state     ON ticker_events(state, priority DESC);
CREATE INDEX idx_posts_status     ON posts(status, published_at DESC);
CREATE INDEX idx_gists_decay      ON memory_gists(decay_score DESC, last_used_at DESC);
CREATE INDEX idx_images_session   ON images(session_id, status);
CREATE INDEX idx_skillruns_skill  ON skill_runs(skill_id, created_at DESC);
```

Vector tables (`sqlite-vec`): `vec_nodes`, `vec_dots`, `vec_gists`, `vec_entities`,
`vec_posts` — each `(rowid, embedding float[384])` joined on the owning table's id.

---

## 4. Blob store

```
~/.bhippi/blobs/<first-2-hex>/<blake3-hex>[.ext]
```

Content-addressed and therefore deduplicated for free. Kinds: `html` (raw), `txt`
(extracted markdown), `pdf`, `img` (original), `var` (encoded variant), `mm` (mind map
export). The DB stores paths, never bytes. Orphan scan and reclaim live in `bhippi doctor`.

**Retention:** raw HTML 30 days (extracted text is permanent) · rejected image candidates
7 days · replay dumps 30 days · logs 7 days.

---

## 5. Repository surface (`bhippi-db`)

| Repo | Representative intentions |
|---|---|
| `SessionRepo` | `create`, `advance_stage(tx, id, from, to, artifact)`, `resume_point`, `record_metrics` |
| `NodeRepo` | `insert_children`, `frontier_top(n)`, `mark_status`, `positions` |
| `DotRepo` | `insert_batch`, `by_node`, `contradictions`, `resolve_provenance` |
| `SourceRepo` | `upsert_canonical`, `seen_index(session)`, `add_corroboration` |
| `ImageRepo` | `insert_candidate`, `approve`, `reject(reason)`, `variants_for` |
| `MemoryRepo` | `hybrid_search`, `put_gist`, `decay_tick`, `forget` |
| `TickerRepo` | `upsert_event`, `cluster_members`, `mark_state` |
| `PostRepo` | `draft`, `set_scores`, `publish`, `retract`, `similar(embedding)` |
| `SkillRepo` | `list`, `set_autonomy`, `record_run`, `pending_approvals` |
| `ProviderRepo` | `upsert_detected`, `set_health`, `set_enabled` |
| `JobRepo` | `enqueue`, `claim_next`, `complete`, `fail`, `dead_letter` |

`advance_stage` is the **only** way `sessions.status` and `stage_cursor` change, and it
takes the stage artifact in the same transaction. That single method is what makes
`INV-020` (crash-resumability) true.

---

## 6. `bhippi doctor` checks

1. Schema version and migration integrity.
2. Every index in §3 present.
3. Foreign-key check (`PRAGMA foreign_key_check`).
4. Blob orphans (rows without files, files without rows) and reclaimable bytes.
5. Vector table row-count parity with owning tables.
6. Tantivy index openable and in sync.
7. Provider health, feed health, circuit-breaker states.
8. Disk usage against the 3 GB / 1000-session budget.
9. Unacknowledged incidents and dead letters.
10. Quarterly `VACUUM` recommendation.

Output: a one-page report, exit code non-zero on any failure, suitable for CI.
