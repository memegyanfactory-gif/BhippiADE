PRAGMA foreign_keys = ON;

CREATE TABLE sessions (
  id TEXT PRIMARY KEY,
  seed_topic TEXT NOT NULL,
  tier TEXT NOT NULL CHECK (tier IN ('X2','X6','X12','X24')),
  origin TEXT NOT NULL CHECK (origin IN ('manual','timer','ticker','skill')),
  ticker_event_id TEXT REFERENCES ticker_events(id),
  status TEXT NOT NULL,
  stage_cursor TEXT,
  domain_score REAL,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  tokens_used INTEGER NOT NULL DEFAULT 0,
  wall_secs INTEGER NOT NULL DEFAULT 0,
  error TEXT
);

CREATE TABLE nodes (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  parent_id TEXT REFERENCES nodes(id),
  hop INTEGER NOT NULL,
  kind TEXT NOT NULL,
  label TEXT NOT NULL,
  summary TEXT,
  status TEXT NOT NULL,
  novelty REAL,
  relevance REAL,
  authority REAL,
  priority REAL,
  embedding BLOB,
  created_at TEXT NOT NULL
);

CREATE TABLE edges (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  from_node TEXT NOT NULL REFERENCES nodes(id),
  to_node TEXT NOT NULL REFERENCES nodes(id),
  relation TEXT NOT NULL,
  weight REAL NOT NULL DEFAULT 1.0,
  evidence_dot TEXT REFERENCES dots(id)
);

CREATE TABLE sources (
  id TEXT PRIMARY KEY,
  url TEXT NOT NULL,
  canonical_url TEXT NOT NULL UNIQUE,
  domain TEXT NOT NULL,
  title TEXT,
  author TEXT,
  published_at TEXT,
  fetched_at TEXT NOT NULL,
  http_status INTEGER,
  content_hash TEXT,
  simhash INTEGER,
  word_count INTEGER,
  extracted_path TEXT,
  trust_tier INTEGER,
  paywalled INTEGER NOT NULL DEFAULT 0,
  license TEXT,
  lang TEXT
);

CREATE TABLE dots (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  claim TEXT NOT NULL,
  claim_type TEXT NOT NULL,
  source_id TEXT NOT NULL REFERENCES sources(id),
  char_start INTEGER,
  char_end INTEGER,
  observed_at TEXT NOT NULL,
  confidence REAL NOT NULL,
  corroborations INTEGER NOT NULL DEFAULT 0,
  contradicted_by TEXT,
  embedding BLOB
);

CREATE TABLE source_registry (
  domain TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  trust_tier INTEGER NOT NULL,
  categories TEXT,
  feed_url TEXT,
  robots_note TEXT,
  enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE images (
  id TEXT PRIMARY KEY,
  session_id TEXT REFERENCES sessions(id) ON DELETE CASCADE,
  origin_url TEXT NOT NULL,
  page_url TEXT,
  license TEXT NOT NULL,
  license_url TEXT,
  attribution TEXT,
  width INTEGER,
  height INTEGER,
  phash TEXT,
  caption_model TEXT,
  caption TEXT,
  alt_text TEXT NOT NULL,
  relevance REAL,
  focal_x REAL,
  focal_y REAL,
  variants TEXT,
  status TEXT NOT NULL
);

CREATE TABLE memory_gists (
  id TEXT PRIMARY KEY,
  session_id TEXT REFERENCES sessions(id),
  scope TEXT NOT NULL,
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  key_claims TEXT,
  entities TEXT,
  created_at TEXT NOT NULL,
  last_used_at TEXT,
  use_count INTEGER NOT NULL DEFAULT 0,
  decay_score REAL NOT NULL DEFAULT 1.0,
  embedding BLOB
);

CREATE TABLE entities (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  aliases TEXT,
  summary TEXT,
  first_seen TEXT,
  last_seen TEXT,
  mention_count INTEGER NOT NULL DEFAULT 0,
  embedding BLOB
);

CREATE TABLE entity_links (
  from_entity TEXT NOT NULL REFERENCES entities(id),
  to_entity TEXT NOT NULL REFERENCES entities(id),
  relation TEXT NOT NULL,
  weight REAL NOT NULL DEFAULT 1.0,
  evidence TEXT,
  PRIMARY KEY (from_entity, to_entity, relation)
);

CREATE TABLE chat_turns (
  id TEXT PRIMARY KEY,
  session_id TEXT REFERENCES sessions(id),
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TEXT NOT NULL,
  gisted INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE ticker_events (
  id TEXT PRIMARY KEY,
  cluster_id TEXT NOT NULL,
  headline TEXT NOT NULL,
  url TEXT NOT NULL,
  domain TEXT NOT NULL,
  published_at TEXT NOT NULL,
  first_seen_at TEXT NOT NULL,
  category TEXT,
  domain_score REAL,
  burst_count INTEGER NOT NULL DEFAULT 1,
  velocity REAL,
  priority REAL,
  state TEXT NOT NULL,
  session_id TEXT REFERENCES sessions(id)
);

CREATE TABLE posts (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  slug TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  dek TEXT,
  body_md TEXT NOT NULL,
  body_html TEXT,
  hero_image_id TEXT REFERENCES images(id),
  primary_kw TEXT,
  keywords TEXT,
  meta_desc TEXT,
  reading_mins INTEGER,
  word_count INTEGER,
  seo_score INTEGER,
  fact_score INTEGER,
  status TEXT NOT NULL,
  published_at TEXT,
  updated_at TEXT,
  deploy_ref TEXT,
  prompt_hashes TEXT
);

CREATE TABLE skills (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  version TEXT NOT NULL,
  kind TEXT NOT NULL,
  manifest TEXT NOT NULL,
  body_path TEXT NOT NULL,
  autonomy TEXT NOT NULL,
  created_by TEXT NOT NULL,
  eval_score REAL,
  runs INTEGER NOT NULL DEFAULT 0,
  wins INTEGER NOT NULL DEFAULT 0,
  last_run_at TEXT,
  UNIQUE(name, version)
);

CREATE TABLE providers (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  vendor TEXT NOT NULL,
  model TEXT NOT NULL,
  endpoint TEXT,
  detected_via TEXT,
  ctx_window INTEGER,
  supports_vision INTEGER NOT NULL DEFAULT 0,
  supports_tools INTEGER NOT NULL DEFAULT 0,
  avg_latency_ms INTEGER,
  health TEXT,
  enabled INTEGER NOT NULL DEFAULT 1,
  last_checked TEXT
);

CREATE INDEX idx_nodes_frontier ON nodes(session_id, status, priority DESC);
CREATE INDEX idx_dots_node ON dots(node_id);
CREATE INDEX idx_dots_session ON dots(session_id);
CREATE UNIQUE INDEX ux_sources_canon ON sources(canonical_url);
CREATE INDEX idx_sources_domain ON sources(domain, fetched_at DESC);
CREATE INDEX idx_sources_simhash ON sources(simhash);
CREATE INDEX idx_ticker_cluster ON ticker_events(cluster_id, published_at DESC);
CREATE INDEX idx_ticker_state ON ticker_events(state, priority DESC);
CREATE INDEX idx_posts_status ON posts(status, published_at DESC);
CREATE INDEX idx_gists_decay ON memory_gists(decay_score DESC, last_used_at DESC);
CREATE INDEX idx_images_session ON images(session_id, status);
