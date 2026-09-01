PRAGMA foreign_keys = ON;

-- Project Brain (Phase B). A versioned, persistent index of a project's code shape:
-- projects, modules, files and symbols. Every row carries a stable id, a content hash
-- (blake3) and a per-project source_revision so unrelated edits never churn ids and
-- stale/gone-without-change entries stay detectable.

CREATE TABLE brain_projects (
  id              TEXT PRIMARY KEY,              -- stable ProjectId (hash of canonical path)
  path            TEXT NOT NULL UNIQUE,          -- canonical absolute path
  source_revision INTEGER NOT NULL DEFAULT 0,    -- bumped each time the tree changes
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);

CREATE TABLE brain_modules (
  id              TEXT PRIMARY KEY,              -- stable ModuleId
  project_id      TEXT NOT NULL REFERENCES brain_projects(id) ON DELETE CASCADE,
  name            TEXT NOT NULL,                 -- fully-qualified module path
  source_of_truth TEXT NOT NULL DEFAULT 'index',
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  UNIQUE (project_id, name)
);

CREATE TABLE brain_files (
  id              TEXT PRIMARY KEY,              -- stable FileId
  project_id      TEXT NOT NULL REFERENCES brain_projects(id) ON DELETE CASCADE,
  rel_path        TEXT NOT NULL,                 -- path relative to project root
  content_hash    TEXT NOT NULL,                 -- blake3 of file content
  source_revision INTEGER NOT NULL DEFAULT 0,    -- revision this snapshot was seen at
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  stale           INTEGER NOT NULL DEFAULT 0,
  UNIQUE (project_id, rel_path)
);

CREATE TABLE brain_symbols (
  id              TEXT PRIMARY KEY,              -- stable SymbolId (survives line shifts)
  project_id      TEXT NOT NULL REFERENCES brain_projects(id) ON DELETE CASCADE,
  file_id         TEXT NOT NULL REFERENCES brain_files(id) ON DELETE CASCADE,
  kind            TEXT NOT NULL,                 -- function | class | method | variable | ...
  name            TEXT NOT NULL,
  qualified_name  TEXT NOT NULL,                 -- Path::to::symbol
  signature       TEXT,                          -- normalized signature (stable text)
  start_line      INTEGER,
  end_line        INTEGER,
  start_col       INTEGER,
  end_col         INTEGER,
  content_hash    TEXT NOT NULL,                 -- blake3 of the symbol's normalized body
  source_revision INTEGER NOT NULL DEFAULT 0,
  parent_symbol   TEXT REFERENCES brain_symbols(id),  -- NULL for top-level
  source_of_truth TEXT NOT NULL DEFAULT 'index',
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  stale           INTEGER NOT NULL DEFAULT 0,
  supersedes      TEXT                           -- prior SymbolId this row replaces, if any
);

CREATE INDEX idx_brain_files_project      ON brain_files(project_id, stale);
CREATE INDEX idx_brain_symbols_project_name ON brain_symbols(project_id, name);
CREATE INDEX idx_brain_symbols_file       ON brain_symbols(file_id);
CREATE INDEX idx_brain_symbols_qualified  ON brain_symbols(project_id, qualified_name);
CREATE INDEX idx_brain_symbols_stale      ON brain_symbols(project_id, stale);