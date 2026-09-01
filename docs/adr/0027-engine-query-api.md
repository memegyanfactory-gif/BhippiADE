# ADR-0027: Engine query API

Date: 2026-09-01 · Status: accepted · Supersedes: (none — new engine-layer API, plan SEC. 7.4)

## Context

Plan SEC 7.4 requires an **engine query API** — a deterministic, read-only facade over the
in-memory `SceneDocument` and `AssetIndex` that answers Scenic lookup questions the AI and
the inspector both need:

- `scene.get(id)`, `scene.get_entity(id)`, `scene.find_entities(query)`
- `scene.get_components(entity_id)`, `scene.get_children(entity_id)`, `scene.get_parent(entity_id)`
- `scene.get_scripts(entity_id)`
- `scene.get_asset_dependencies(asset_id)`, `scene.get_asset_users(asset_id)`
- `scene.get_material_graph(material_id)`, `scene.get_shader(shader_id)`
- `scene.get_animation_graph(entity_id)`, `scene.get_physics(entity_id)`

Every query must be **deterministic** and support a **compact representation** and a
**deeper expansion mode**.

The crate already has `query.rs` (hierarchy, `snapshot`, `find_by_name`, `find_with_component`,
`search_paths`) for the Hierarchy panel and mind map. What is missing is a single, uniform,
compound query surface that also crosses the asset index (material/shader/animation/users/deps)
and the physics components — the pieces the World Brain mirrors to PostgreSQL (SEC 7.1–7.3).

## Decision

- Add a new pure engine module `crates/bhippi-engine/src/api.rs`, `SceneQueries<'a>` — a
  borrowed facade over `&SceneDocument` and `Option<&AssetIndex>` with an `Expansion`
  (`compact` | `deep`) carried on the facade and switchable per call via `compact()`/`deep()`.
- Implement all 13 methods of SEC 7.4 as deterministic projections. No mutation, no DB, no
  side effects — a pure read layer, so the webview/AI never parses raw component JSON
  (INV-073 spirit: the shell computes nothing).
- **Expansion semantics** (uniform): `compact` returns identity/order/scalar facts;
  `deep` additionally returns full `component` payloads (and resolved asset records) via
  `Option` fields that are omitted when `None` (so compact output stays small and stable).
  Both modes share the same deterministic ordering: entities in authoring order, assets in
  `BTreeMap` key order, JSON maps as `BTreeMap`.
- **`find_entities`** accepts a `EntityQuery { name, tag, has_component, parent, roots_only }`
  filter and returns matching `EntityRef`s in authoring order.
- **Asset queries**:
  - `get_asset_users(asset_id)` → the entities whose components reference the asset, each
    with stable path and the referencing component names (deep adds payloads).
  - `get_asset_dependencies(asset_id)` → the set of *other* asset ids referenced by the
    same entities that reference this asset (i.e. what ships alongside it in the scene —
    e.g. a mesh and its materials, a material and its albedo map). The engine scans asset
    files only for kind/path/hash/license (ADR-0025), not for their internal references, so
    the *scene graph* is the only dependency source that is both present and honest.
  - `get_material_graph` / `get_shader` / `get_animation_graph` resolve `asset:` references
    to Material / Shader / Animation assets (via `MeshRenderer.materials`,
    `MaterialOverride`, `ShaderRef.shader`, `AnimationPlayer.clip`) into users + co-refs.
- **`get_physics(entity_id)`** returns the same body/collider/character-controller scalar
  projection the World Brain persists (`RigidBody` kind/mass/lock_rotation, `Collider`
  shape/sensor, `CharacterController` presence; ADR-0026), plus deep component payloads.
- DTOs derive `Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type` for future IPC
  reuse. Because `f64` scalar fields (`mass`) appear, DTOs carrying them derive `PartialEq`
  but not `Eq`.

## Consequences

- Easier: one deterministic API answers scene/entity/component/hierarchy/script/asset/
  material/shader/animation/physics lookups; the World Brain and future IPC both target it
  instead of ad-hoc component parsing.
- Harder: nothing persisted; it is a read-only projection over already-present in-memory
  state, so there is no migration, no schema change, no unlock/undo interaction. Asset
  "dependency" is defined from the scene graph (co-shipped refs), documented above, not from
  reading material/shader files.
- No new crate; `bhippi-engine` deps unchanged. No `bhippi-memory`/`bhippi-db`/architecture
  changes for this slice.
- Scope (per user): API + tests only — no IPC, no World Brain wiring, no UI. Those are
  follow-ups.
- Document updates:
  - Plan SEC. 7.4 — check `[x]` (deterministic; compact; deep-exansion).
  - `docs/PROGRESS.md` — status line + session-log row.

## Alternatives considered

- **Extend `query.rs` in place:** rejected — `query.rs` is the Hierarchy-panel/mind-map
  projection with its own entry points; SEC 7.4 is a distinct compound query surface, and a
  new module keeps both simple.
- **Build a trait/`Entity`-object index:** rejected — over-engineering for a read-only
  borrowed facade; a concrete struct over `&SceneDocument` is enough (matches "simple" rule).
- **Read asset-file internals for dependencies:** rejected — the scan consciously records
  only kind/path/hash/license (ADR-0025); reading `.mat`/`.shader.json` to resolve
  dependency graphs expands the scan's surface and is out of scope for an in-memory query API.
