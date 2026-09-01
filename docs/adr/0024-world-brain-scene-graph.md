# ADR-0024: World Brain scene graph persistence

Date: 2026-09-01 · Status: accepted
Supersedes: (none — new subsystem, plan SEC. 7)

## Context

The engine already keeps a scene as an in-memory `bhippi-engine::document::SceneDocument`
with a deterministic `.bscn.json` format (`bhippi-scene@1`) and offers an in-memory query
surface (`query.rs`). That model never survives a restart and is scoped to one open scene:
there is no persistent knowledge graph of the project's *world* that the AI can query the
same way the Project Brain makes code queryable.

Plan SEC. 7 (World Brain) requires a **persistent** scene graph indexed into `bhippi-db` —
scenes, entities, hierarchy, components — so a project's world is addressable by stable ULID
across sessions, alongside the structural/semantic index built in B1–B8.

`bhippi-memory` (the natural home for persistent high-level indexing, and where `ProjectBrain`
lives) currently depends only on `bhippi-db`, `bhippi-providers`, `bhippi-types`. Indexing the
engine's scene shape means reading `bhippi-engine::SceneDocument`.

## Decision

- Add a **directional dependency edge** `bhippi-memory → bhippi-engine`. This is an L2→L1
  edge and matches how `bhippi-memory` already consumes `bhippi-db`/`bhippi-providers`
  (concrete engine types, not a Bevy link — `bhippi-engine` stays the only Bevy-gating crate).
- Add migration `0008_world_brain.sql` with two tables under the existing `brain_*` namespace:
  - `brain_scenes(project_id, scene_id, rel_path, name, kind, entity_count, settings_json,
    source_revision, created_at, updated_at)` — one row per `.bscn.json`, keyed by the
    engine's stable `SceneId` ULID, project-scoped, `UNIQUE (project_id, rel_path)`.
  - `brain_entities(entity_id, project_id, scene_id, name, parent_id, tags_json,
    component_names_json, component_json, source_revision, created_at, updated_at)` — keyed
    by the engine's stable `EntityId` ULID, project-scoped, with a `scene_id` FK so a scene
    replacement can cascade. Component payloads are stored as deterministic JSON.
- Add `BrainRepo` world methods in `bhippi-db` (storage primitives only, no logic):
  `scene_by_path`, `list_scenes`, `upsert_scene`, `replace_scene_entities`, `scene_entities`,
  `entity_by_id`, `remove_scene`.
- Add `bhippi-memory::WorldBrain` (mirrors `ProjectBrain`):
  - `index_scene_document(&rel_path, &SceneDocument)` — persists the scene row and **replaces
    that scene's entities** (authoring order is canonical, so per-scene replace is the correct
    incremental unit), bumping the project revision once when anything changed.
  - `project_scenes()`, `scene_entities(scene_id)`, `scene_hierarchy(scene_id)` (parent-before-
    child, deterministic), `find_entity(scene_id, name)`.
- The existing in-memory `bhippi-engine::query` helpers stay the authority for live editing;
  the World Brain is the durable mirror.

## Consequences

- Easier: a project's world is queryable persistently; hierarchy/components survive restart;
  the AI can address entities by stable ULID without parsing `.bscn.json` text.
- Harder: `bhippi-memory` gains a small dependency on the engine crate; scene replacement is
  an all-or-nothing per-scene write (cheap for typical scene sizes).
- Document updates:
  - `crates/bhippi-types/tests/architecture.rs` — add `"bhippi-engine"` to `bhippi-memory`
    allowed deps.
  - `docs/01-ARCHITECTURE.md` — note the `bhippi-memory → bhippi-engine` edge.
  - `ui/BHIPPI_TOKEN_ENGINE_IMPLEMENTATION_PLAN.md` — check SEC. 7.1 scene graph items this
    covers.

## Alternatives considered

- **World Brain in `bhippi-app`:** rejected — persistence belongs in a memory crate (same
  reason `ProjectBrain` lives in `bhippi-memory`), and the shell should not grow indexing logic.
- **New `bhippi-world` crate:** rejected for now — a dedicated crate adds boilerplate for one
  struct; fold into `bhippi-memory` and split later if the asset/physics graphs (7.2/7.3) grow it.
- **Reuse `bhippi-engine` as the store:** rejected — scenes are in-memory/authoring-time, not
  durable across sessions or queryable through the DB the research agent already uses.
