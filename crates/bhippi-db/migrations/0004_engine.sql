PRAGMA foreign_keys = ON;

-- Engine workbench (ADR-0020). P2 journaling: one row per game project, plus the
-- transaction journal the scene editor writes through. The manifest itself stays on
-- disk as plain text; this table is a cache of its stable facts for the ledger UI.

CREATE TABLE engine_projects (
  project_path   TEXT PRIMARY KEY,
  game_id        TEXT NOT NULL,
  game_name      TEXT NOT NULL,
  version        TEXT NOT NULL,
  default_scene  TEXT NOT NULL,
  engine_track   TEXT NOT NULL CHECK (engine_track IN ('rust','scripted')),
  targets_json   TEXT NOT NULL DEFAULT '[]',
  scene_count    INTEGER NOT NULL DEFAULT 0,
  first_seen_at  TEXT NOT NULL,
  last_loaded_at TEXT NOT NULL
);

-- Every accepted engine transaction, monotonically numbered per project so the UI and
-- the AI can page backwards. Ops are stored as the original JSON-LD-style op list,
-- which is also the replay source for crash recovery.

CREATE TABLE engine_journal (
  project_path TEXT NOT NULL REFERENCES engine_projects(project_path) ON DELETE CASCADE,
  revision     INTEGER NOT NULL,
  txn_id       TEXT NOT NULL,
  actor        TEXT NOT NULL CHECK (actor IN ('user','agent')),
  issued_at    TEXT NOT NULL,
  label        TEXT,
  ops_json     TEXT NOT NULL,
  PRIMARY KEY (project_path, revision)
);