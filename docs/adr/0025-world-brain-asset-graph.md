# ADR-0025: World Brain asset graph persistence

Date: 2026-09-01 · Status: accepted · Supersedes: (none — extends ADR-0024, plan SEC. 7.2)

## Context

ADR-0024 built the World Brain's persistent scene graph (`brain_scenes` +
`brain_entities`). Plan SEC. 7.2 requires the matching **asset graph**: index materials,
shaders, textures, meshes, animations, skeletons, audio, asset dependencies and reverse
usage ("what uses this asset?"), so the AI can address and reverse-look-up a project's
assets across sessions the same way it queries scenes.

The engine already builds an in-memory `bhippi-engine::asset::AssetIndex`:
`AssetRecord { id, path_rel, kind, hash, license, size_bytes, used_by_scenes }`, plus
`AssetKind` (Mesh, Texture, Material, Audio, Animation, Scene, Script, Prefab, Ui, Font,
Shader, Other), `LicenseState`, a blake3 content hash and a `refresh_usage` pass that
collects `asset:xxxx` references from scene component payloads into `used_by_scenes`.
That index is scoped to the running editor and persisted only to `.bhippi/engine/asset-index.json`;
it is not queryable through the DB the research agent already uses.

## Decision

Persist the asset graph into `bhippi-db` as a durable World Brain mirror, exactly as
ADR-0024 did for scenes:

- Add migration `0009_brain_assets.sql` with `brain_assets(asset_id, project_id, rel_path,
  kind, hash, license, size_bytes, used_by_scenes_json, source_revision, created_at,
  updated_at)`, keyed by the engine's stable `AssetId` ULID, project-scoped,
  `UNIQUE (project_id, rel_path)`. Reverse usage is materialised as quoted `SceneId`s in
  `used_by_scenes_json`.
- Extend `AssetKind` with `Skeleton` (covers the 7.2 "index skeletons" item; the other
  7.2 kinds already exist).
- Add `BrainRepo` asset methods (storage primitives only): `asset_by_path`, `asset_by_id`,
  `assets_by_project`, `assets_by_kind`, `replace_project_assets` (replace-all per project
  in one transaction — the engine rebuilds the whole index on scan, so replace-all is the
  correct incremental unit). Add a `bhippi_db::AssetRecord`.
- Add `bhippi-memory::WorldBrain` asset methods:
  - `index_asset_index(&AssetIndex, source_revision)` — persists the whole index, replacing
    the prior snapshot, carrying `record.used_by_scenes` through.
  - `project_assets()`, `asset_by_id`, `asset_by_path`, `assets_by_kind`.
  - `asset_reverse_usage(asset_id)` — decodes `used_by_scenes_json` and resolves each
    `SceneId` to its persisted scene name.
- The engine's in-memory `AssetIndex` stays the authority for live editing; the World Brain
  asset rows are the durable mirror.

No new crate: the World Brain stays in `bhippi-memory` for now (the ADR-0024 note about a
possible 7.2/7.3 split is revisited only if a third graph forces it).

## Consequences

- Easier: a project's assets are queryable persistently by kind and by reverse usage; the
  AI can answer "what uses this material?" and "which textures are in this project?"
  without parsing `assets/` or the sidecars.
- Harder: an asset-graph snapshot is a whole-project replace (cheap for typical asset
  counts); `brain_assets` grows with the imported pack.
- Document updates:
  - Plan `ui/BHIPPI_TOKEN_ENGINE_IMPLEMENTATION_PLAN.md` SEC. 7.2 — check the asset items.
  - `docs/PROGRESS.md` — session log + status line.
  - No architecture edge changes: `bhippi-memory → {db, engine, types}` already allowed.

## Alternatives considered

- **New `bhippi-world` crate for the asset graph:** rejected for now — one struct added to
  `bhippi-memory` does not justify the split; revisit before 7.3 if physics grows it.
- **Store each asset dependency as a row (`brain_asset_deps`):** rejected — the existing
  `used_by_scenes` cross-reference is sufficient for 7.2's reverse-usage item; a true asset↔
  asset edge table is deferred unless a future query needs it.
- **Recompute reverse usage live from scene rows:** rejected — materialising quoted scene
  ids keeps reverse usage a lookup, matching the plan's "find things to edit" intent.
