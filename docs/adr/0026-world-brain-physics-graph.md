# ADR-0026: World Brain physics graph persistence

Date: 2026-09-01 · Status: accepted · Supersedes: (none — extends ADR-0024/0025, plan SEC. 7.3)

## Context

Plan SEC. 7.3 requires a **physics graph**: index rigid bodies, colliders, collision
layers, the collision matrix, joints/constraints and navigation information, so the AI can
answer "which entities are dynamic bodies?", "what collides with what?" across sessions.

The engine represents physics as per-entity **components** authored into
`SceneDocument.entities[].components` — `RigidBody` (`kind`, `mass`, `lock_rotation`),
`Collider` (`shape`, `sensor`) and `CharacterController` (`height`, `radius`, `max_slope`).
These are already persisted — as opaque `component_json` on `brain_entities` (SEC 7.1,
ADR-0024). Collision layers, the collision matrix, joints/constraints and navigation have
**no data model anywhere in the engine**: they belong to the physics backend (Avian,
ENG-053 in build-order phase P5, ADR-0020), which is not yet built.

## Decision

- Persist the physics **body/collider** projection — items 1 and 2 of SEC 7.3 — as a
  first-class, queryable World Brain table, derived from the entity components already
  snapshotted by `WorldBrain::index_scene_document`.
- Add migration `0010_brain_physics.sql` with `brain_physics_bodies(entity_id, project_id,
  scene_id, body_kind, mass, lock_rotation, collider_shape, sensor,
  has_character_controller, extras_json, source_revision, created_at, updated_at)`. Keyed
  by `entity_id` (FK to `brain_entities` ON DELETE CASCADE), one row per entity that carries
  any of `RigidBody` / `Collider` / `CharacterController`.
- Add `BrainRepo` physics methods (storage primitives): `replace_scene_physics`
  (per-scene replace in one transaction, after `replace_scene_entities`), `physics_bodies_by_project`,
  `physics_bodies_by_scene`, `physics_body_by_entity`. Add `bhippi_db::PhysicsBodyRecord`.
- Extend `bhippi-memory::WorldBrain`:
  - `index_scene_document` derives and persists the physics rows for the same scene's
    entities (the engine's component JSON is the authority; the physics table is a
    projection).
  - Queries: `project_physics()`, `scene_physics(scene_id)`, `physics_by_entity(entity_id)`.
- **Items 3–6 (collision layers, collision matrix, joints/constraints, navigation) are
  blocked on the physics backend** (ENG-053/Avian). Until that lands there is no data source
  to index, so they are marked `blocked` in the plan, not silently skipped or invented.

## Consequences

- Easier: the AI can query rigid bodies and colliders ("which entities are dynamic?",
  "what sensor colliders exist in this scene?") without parsing `component_json`.
- Harder: adding physics rows makes scene indexing a third per-scene transaction (scene
  upsert → entity replace → physics replace); the physics projection duplicates body data
  that also lives in `component_json` (kept because it is the query-friendly shape the AI
  can address directly).
- No new crate; no architecture edge changes (`bhippi-memory → {db, engine, types}` already
  allowed).
- Document updates:
  - Plan SEC. 7.3 — check items 1–2, mark 3–6 `blocked` with the reason.
  - `docs/PROGRESS.md` — status line + session-log row.

## Alternatives considered

- **Index layers/matrix/joints/nav now by inventing engine data models:** rejected — the
  physics backend (Avian, ENG-053) does not exist, so any model would be invented ahead of
  the engine track and reworked; scope rules forbid building ahead.
- **Re-query physics from `component_json` live:** rejected for joints/layers/matrix (no
  source) and, even for bodies, a first-class table is cheaper and matches the 7.1/7.2
  "durable mirror" pattern.
- **Separate `brain_physics` crate:** rejected — one struct added to `bhippi-memory`, same
  reasoning as ADR-0025's asset-graph split.
