PRAGMA foreign_keys = ON;

-- World Brain asset graph (ADR-0025, plan SEC. 7.2). A persistent mirror of the
-- engine's in-memory AssetIndex: one row per imported asset under assets/, keyed by
-- the engine's stable AssetId ULID so the AI can address and reverse-lookup assets
-- across sessions without re-scanning the filesystem. Reverse usage ("what uses this
-- asset?") is materialised as the quoted scene ids and stored as deterministic JSON.

CREATE TABLE brain_assets (
  asset_id         TEXT PRIMARY KEY,              -- stable AssetId (engine ULID)
  project_id       TEXT NOT NULL REFERENCES brain_projects(id) ON DELETE CASCADE,
  rel_path         TEXT NOT NULL,                 -- path under assets/, project-relative
  kind             TEXT NOT NULL DEFAULT 'other', -- mesh|skeleton|texture|material|audio|...
  hash             TEXT NOT NULL,                 -- blake3 content digest
  license          TEXT NOT NULL DEFAULT 'unknown',-- spdx id, or 'unknown'
  size_bytes       INTEGER NOT NULL DEFAULT 0,
  used_by_scenes_json TEXT NOT NULL DEFAULT '[]', -- quoted SceneIds that reference it
  source_revision  INTEGER NOT NULL DEFAULT 0,    -- project revision this snapshot was seen at
  created_at       TEXT NOT NULL,
  updated_at       TEXT NOT NULL,
  UNIQUE (project_id, rel_path)
);

CREATE INDEX idx_brain_assets_project  ON brain_assets(project_id);
CREATE INDEX idx_brain_assets_kind     ON brain_assets(project_id, kind);
CREATE INDEX idx_brain_assets_path     ON brain_assets(project_id, rel_path);
