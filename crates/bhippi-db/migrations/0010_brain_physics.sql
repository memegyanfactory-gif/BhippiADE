PRAGMA foreign_keys = ON;

-- World Brain physics graph (ADR-0026, plan SEC. 7.3). A persistent projection of the
-- physics bodies and colliders authored onto entities (RigidBody / Collider /
-- CharacterController components). One row per entity that carries any physics component,
-- keyed by the entity's stable ULID so the AI can query "which entities are dynamic?"
-- or "what colliders are in this scene?" across sessions without parsing component_json.
--
-- Collision layers, the collision matrix, joints/constraints and navigation (SEC 7.3
-- items 3-6) have no engine data model yet -- they belong to the physics backend
-- (Avian, ENG-053, P5) -- so only the body/collider projection is persisted here.

CREATE TABLE brain_physics_bodies (
  entity_id       TEXT PRIMARY KEY REFERENCES brain_entities(entity_id) ON DELETE CASCADE,
  project_id      TEXT NOT NULL REFERENCES brain_projects(id) ON DELETE CASCADE,
  scene_id        TEXT NOT NULL REFERENCES brain_scenes(scene_id) ON DELETE CASCADE,
  body_kind       TEXT,              -- static | dynamic | kinematic | null (collider-only entity)
  mass            REAL,              -- kg for dynamic bodies, optional
  lock_rotation   INTEGER,           -- 0/1 for RigidBody, optional
  collider_shape  TEXT,              -- shape descriptor json (cuboid/sphere/capsule/mesh/...), optional
  sensor          INTEGER,           -- 0/1 collider is a sensor, optional
  has_character_controller INTEGER NOT NULL DEFAULT 0, -- entity carries CharacterController
  extras_json     TEXT NOT NULL DEFAULT '{}',          -- other physics-ish component payloads
  source_revision INTEGER NOT NULL DEFAULT 0,          -- project revision this snapshot was seen at
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL
);

CREATE INDEX idx_brain_physics_project  ON brain_physics_bodies(project_id);
CREATE INDEX idx_brain_physics_scene    ON brain_physics_bodies(project_id, scene_id);
CREATE INDEX idx_brain_physics_body_kind ON brain_physics_bodies(project_id, body_kind);
