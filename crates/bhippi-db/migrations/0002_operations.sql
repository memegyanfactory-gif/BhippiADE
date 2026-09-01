CREATE TABLE jobs (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  payload TEXT NOT NULL,
  priority REAL NOT NULL DEFAULT 0,
  attempts INTEGER NOT NULL DEFAULT 0,
  not_before TEXT,
  state TEXT NOT NULL,
  last_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_jobs_ready ON jobs(state, not_before, priority DESC);

CREATE TABLE dead_letters (
  id TEXT PRIMARY KEY,
  job_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  payload TEXT NOT NULL,
  error TEXT NOT NULL,
  attempts INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  acknowledged INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE prompt_versions (
  hash TEXT PRIMARY KEY,
  prompt_id TEXT NOT NULL,
  version INTEGER NOT NULL,
  task_class TEXT NOT NULL,
  path TEXT NOT NULL,
  first_seen TEXT NOT NULL
);

CREATE TABLE link_edits (
  id TEXT PRIMARY KEY,
  from_post TEXT NOT NULL REFERENCES posts(id),
  to_post TEXT NOT NULL REFERENCES posts(id),
  anchor_text TEXT NOT NULL,
  block_index INTEGER NOT NULL,
  char_offset INTEGER NOT NULL,
  inserted_at TEXT NOT NULL,
  reverted_at TEXT
);

CREATE TABLE redirects (
  from_slug TEXT PRIMARY KEY,
  to_slug TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE deploys (
  id TEXT PRIMARY KEY,
  target TEXT NOT NULL,
  deploy_ref TEXT NOT NULL,
  bundle_hash TEXT NOT NULL,
  post_count INTEGER NOT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  status TEXT NOT NULL,
  report TEXT
);

CREATE TABLE incidents (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  session_id TEXT REFERENCES sessions(id),
  source_id TEXT REFERENCES sources(id),
  detail TEXT NOT NULL,
  created_at TEXT NOT NULL,
  acknowledged INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE domain_stats (
  domain TEXT PRIMARY KEY,
  dots_contributed INTEGER NOT NULL DEFAULT 0,
  corroboration_rate REAL,
  contradiction_rate REAL,
  extraction_quality REAL,
  avg_latency_ms INTEGER,
  learned_trust REAL NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL
);

CREATE TABLE query_stats (
  id TEXT PRIMARY KEY,
  pattern TEXT NOT NULL,
  uses INTEGER NOT NULL DEFAULT 0,
  tier1_hits INTEGER NOT NULL DEFAULT 0,
  tier2_hits INTEGER NOT NULL DEFAULT 0,
  zero_yield INTEGER NOT NULL DEFAULT 0,
  score REAL,
  updated_at TEXT NOT NULL
);

CREATE TABLE interest_weights (
  key TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  weight REAL NOT NULL DEFAULT 0,
  last_event TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE style_prefs (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  candidate TEXT NOT NULL,
  chosen INTEGER NOT NULL,
  edited_to TEXT,
  strategy TEXT,
  post_id TEXT REFERENCES posts(id),
  created_at TEXT NOT NULL
);

CREATE TABLE skill_runs (
  id TEXT PRIMARY KEY,
  skill_id TEXT NOT NULL REFERENCES skills(id),
  version TEXT NOT NULL,
  input_hash TEXT NOT NULL,
  duration_ms INTEGER NOT NULL,
  status TEXT NOT NULL,
  shadow_win INTEGER,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_skillruns_skill ON skill_runs(skill_id, created_at DESC);

CREATE TABLE session_metrics (
  session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
  fetches INTEGER,
  cache_hits INTEGER,
  bytes INTEGER,
  tokens_json TEXT,
  stage_ms_json TEXT,
  dots_per_source REAL,
  primary_ratio REAL,
  fact_score INTEGER,
  lint_failures INTEGER,
  publish_ms INTEGER
);
