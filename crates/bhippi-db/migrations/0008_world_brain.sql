PRAGMA foreign_keys = ON;

-- World Brain (ADR-0024, plan SEC. 7.1). A persistent mirror of the engine's scene
-- graph: one row per `.bscn.json` scene plus one row per entity, keyed by the engine's
-- stable ULID ids so the AI can address world elements across sessions without parsing
-- the serialized file. Entity component payloads are stored as deterministic JSON.

CREATE TABLE brain_scenes (
  project_id     TEXT NOT NULL REFERENCES brain_projects(id) ON DELETE CASCADE,
  scene_id       TEXT PRIMARY KEY,              -- stable SceneId (engine ULID)
  rel_path       TEXT NOT NULL,                 -- path to the .bscn.json, project-relative
  name           TEXT NOT NULL,                 -- scene name, e.g. level_01
  kind           TEXT NOT NULL DEFAULT 'level', -- main | level | hud | empty
  entity_count   INTEGER NOT NULL DEFAULT 0,
  settings_json  TEXT NOT NULL DEFAULT '{}',    -- SceneSettings (deterministic JSON)
  source_revision INTEGER NOT NULL DEFAULT 0,   -- project revision this snapshot was seen at
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  UNIQUE (project_id, rel_path)
);

CREATE TABLE brain_entities (
  entity_id         TEXT PRIMARY KEY,           -- stable EntityId (engine ULID)
  project_id        TEXT NOT NULL REFERENCES brain_projects(id) ON DELETE CASCADE,
  scene_id          TEXT NOT NULL REFERENCES brain_scenes(scene_id) ON DELETE CASCADE,
  name              TEXT NOT NULL,
  parent_id         TEXT REFERENCES brain_entities(entity_id) ON DELETE SET NULL,
  tags_json         TEXT NOT NULL DEFAULT '[]',
  component_names_json TEXT NOT NULL DEFAULT '[]',
  component_json    TEXT NOT NULL DEFAULT '{}', -- full component payload map (deterministic JSON)
  source_revision   INTEGER NOT NULL DEFAULT 0,
  created_at        TEXT NOT NULL,
  updated_at        TEXT NOT NULL
);

CREATE INDEX idx_brain_scenes_project     ON brain_scenes(project_id);
CREATE INDEX idx_brain_entities_scene     ON brain_entities(scene_id);
CREATE INDEX idx_brain_entities_project   ON brain_entities(project_id);
CREATE INDEX idx_brain_entities_parent    ON brain_entities(parent_id);
CREATE INDEX idx_brain_entities_name      ON brain_entities(project_id, name);
