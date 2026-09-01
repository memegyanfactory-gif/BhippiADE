//! Project Brain commands (plan SEC. 5/6/9.3).
//!
//! Exposes the `bhippi-memory` Project Brain (structural index, embeddings, module
//! cards, ranked search) through typed IPC so the UI can show index status, trigger
//! a manual re-index ("Index DB"), browse module knowledge cards, and search
//! symbols. All indexing/search logic lives in Rust; these commands only translate.

use crate::commands::AppError;
use bhippi_db::{AssetRecord, EntityRecord, PhysicsBodyRecord, SceneRecord};
use bhippi_memory::{IndexResult, ModuleCard, ProjectBrain, WorldBrain};
use bhippi_types::{AssetId, EntityId, SceneId};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct BrainStatus {
    pub indexed: bool,
    pub revision: i64,
    pub symbol_count: u64,
    pub module_names: Vec<String>,
    pub embedding_model: Option<String>,
    pub index_version: String,
}

/// View of a [`bhippi_memory::IndexResult`] crossing IPC (plain numeric fields).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Type)]
pub struct IndexReport {
    pub files_scanned: usize,
    pub files_changed: usize,
    pub files_removed: usize,
    pub symbols_counted: u64,
    pub revision: i64,
}

/// Serializable search hit; mirrors the fields UI consumers need and drops the
/// raw embedding blob (never shipped over IPC).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SymbolHit {
    pub kind: String,
    pub name: String,
    pub qualified_name: String,
    pub signature: Option<String>,
    pub start_line: Option<i64>,
    pub end_line: Option<i64>,
    pub stale: bool,
}

/// A module knowledge card as shipped to the UI (PublicModuleCard conflicts with
/// the app's own conceptual naming, so this stays literal: ModuleCardView).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ModuleCardView {
    pub module_name: String,
    pub entry_points: Vec<String>,
    pub public_symbols: Vec<String>,
    pub symbol_count: usize,
    pub description: Option<String>,
    pub description_origin: Option<String>,
    pub card_revision: i64,
    pub token_estimate: usize,
}

fn to_index_report(result: &IndexResult) -> IndexReport {
    IndexReport {
        files_scanned: result.files_scanned,
        files_changed: result.files_changed,
        files_removed: result.files_removed,
        symbols_counted: result.symbols_counted,
        revision: result.revision,
    }
}

fn to_module_view(card: &ModuleCard) -> ModuleCardView {
    ModuleCardView {
        module_name: card.module_name.clone(),
        entry_points: card.entry_points.clone(),
        public_symbols: card.public_symbols.clone(),
        symbol_count: card.symbol_count,
        description: card.description.clone(),
        description_origin: card.description_origin.clone(),
        card_revision: card.card_revision,
        token_estimate: bhippi_memory::module_card_token_estimate(card),
    }
}

/// Resolve the active project directory if one is set.
async fn active_project_path(state: &crate::Runtime) -> Result<PathBuf, AppError> {
    let config = state.config.load().await.map_err(AppError::from)?;
    let saved = config
        .workspace
        .active_project
        .ok_or_else(|| AppError::plain("Select a project before using the Project Brain."))?;
    let path = PathBuf::from(&saved);
    if !path.is_dir() {
        return Err(AppError::plain(
            "The active project folder is no longer available (it may have been moved).",
        ));
    }
    Ok(path)
}

async fn open_brain(state: &crate::Runtime, root: PathBuf) -> Result<ProjectBrain, AppError> {
    let db = state
        .brain_db
        .as_ref()
        .as_ref()
        .ok_or_else(|| AppError::plain("The Project Brain database is unavailable."))?;
    ProjectBrain::new(db.clone(), root)
        .await
        .map_err(AppError::from)
}

async fn status(state: &crate::Runtime, root: PathBuf) -> Result<BrainStatus, AppError> {
    let project = open_brain(state, root).await?;
    let revision = project.project_revision().await.map_err(AppError::from)?;
    let symbol_count = project.count_symbols().await.map_err(AppError::from)?;
    let module_names = project.module_names().await.map_err(AppError::from)?;
    let embedding_model = project
        .embedding_model_used()
        .await
        .map_err(AppError::from)?;
    Ok(BrainStatus {
        indexed: symbol_count > 0,
        revision,
        symbol_count,
        module_names,
        embedding_model,
        index_version: bhippi_providers::EMBEDDING_MODEL.to_owned(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn project_brain_status(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<BrainStatus, AppError> {
    let root = active_project_path(state.inner()).await?;
    status(state.inner(), root).await
}

#[tauri::command]
#[specta::specta]
pub async fn rebuild_project_brain(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<IndexReport, AppError> {
    let root = active_project_path(state.inner()).await?;
    let project = open_brain(state.inner(), root).await?;
    let report = project.reindex_tree(&[]).await.map_err(AppError::from)?;
    Ok(to_index_report(&report))
}

#[tauri::command]
#[specta::specta]
pub async fn list_project_module_cards(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<ModuleCardView>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let project = open_brain(state.inner(), root).await?;
    let cards = project
        .project_module_cards()
        .await
        .map_err(AppError::from)?;
    Ok(cards.iter().map(to_module_view).collect())
}

#[tauri::command]
#[specta::specta]
pub async fn get_project_module_card(
    state: tauri::State<'_, crate::Runtime>,
    rel_path: String,
) -> Result<Option<ModuleCardView>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let project = open_brain(state.inner(), root).await?;
    let card = project
        .module_card(&rel_path)
        .await
        .map_err(AppError::from)?;
    Ok(card.as_ref().map(to_module_view))
}

#[tauri::command]
#[specta::specta]
pub async fn search_project_symbols(
    state: tauri::State<'_, crate::Runtime>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SymbolHit>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let project = open_brain(state.inner(), root).await?;
    let hits = project
        .search(&query, limit.unwrap_or(20).clamp(1, 50))
        .await
        .map_err(AppError::from)?;
    Ok(hits
        .into_iter()
        .map(|s| SymbolHit {
            kind: s.kind,
            name: s.name,
            qualified_name: s.qualified_name,
            signature: s.signature,
            start_line: s.start_line,
            end_line: s.end_line,
            stale: s.stale,
        })
        .collect())
}

// ── World Brain (plan SEC. 7.1, ADR-0024) ────────────────────────────────

/// A persisted scene row as shipped to the UI: stable ULID id, path, kind and the
/// number of entities, without the raw JSON payloads the AI inspects separately.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Type)]
pub struct SceneView {
    pub scene_id: SceneId,
    pub rel_path: String,
    pub name: String,
    pub kind: String,
    pub entity_count: i64,
    pub source_revision: i64,
}

/// A persisted entity row shipped to the UI, with its stable hierarchy address
/// (`scene:/Parent/Child#ULID`) resolved on the Rust side.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SceneEntityView {
    pub entity_id: EntityId,
    pub name: String,
    pub parent_id: Option<EntityId>,
    pub tags: Vec<String>,
    pub component_names: Vec<String>,
    pub stable_path: String,
}

fn to_scene_view(scene: &SceneRecord) -> SceneView {
    SceneView {
        scene_id: scene.scene_id,
        rel_path: scene.rel_path.clone(),
        name: scene.name.clone(),
        kind: scene.kind.clone(),
        entity_count: scene.entity_count,
        source_revision: scene.source_revision,
    }
}

fn to_entity_view(entity: &EntityRecord, stable_path: String) -> SceneEntityView {
    let tags = serde_json::from_str(&entity.tags_json).unwrap_or_default();
    let component_names = serde_json::from_str(&entity.component_names_json).unwrap_or_default();
    SceneEntityView {
        entity_id: entity.entity_id,
        name: entity.name.clone(),
        parent_id: entity.parent_id,
        tags,
        component_names,
        stable_path,
    }
}

async fn open_world_brain(
    state: &crate::Runtime,
    root: &std::path::Path,
) -> Result<WorldBrain, AppError> {
    let db = state
        .brain_db
        .as_ref()
        .as_ref()
        .ok_or_else(|| AppError::plain("The Project Brain database is unavailable."))?;
    WorldBrain::new(db.clone(), root)
        .await
        .map_err(AppError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn world_brain_status(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<SceneView>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    let scenes = world.project_scenes().await.map_err(AppError::from)?;
    Ok(scenes.iter().map(to_scene_view).collect())
}

#[tauri::command]
#[specta::specta]
pub async fn world_brain_scene_entities(
    state: tauri::State<'_, crate::Runtime>,
    scene_id: SceneId,
) -> Result<Vec<SceneEntityView>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    let entities = world
        .scene_entities(scene_id)
        .await
        .map_err(AppError::from)?;
    let paths = world.scene_paths(scene_id).await.map_err(AppError::from)?;
    let path_map: std::collections::HashMap<EntityId, String> = paths.into_iter().collect();
    Ok(entities
        .iter()
        .map(|e| {
            to_entity_view(
                e,
                path_map
                    .get(&e.entity_id)
                    .cloned()
                    .unwrap_or_else(|| e.entity_id.to_string()),
            )
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn world_brain_find_entity(
    state: tauri::State<'_, crate::Runtime>,
    scene_id: SceneId,
    name: String,
) -> Result<Vec<SceneEntityView>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    let found = world
        .find_entity(scene_id, &name)
        .await
        .map_err(AppError::from)?;
    let paths = world.scene_paths(scene_id).await.map_err(AppError::from)?;
    let path_map: std::collections::HashMap<EntityId, String> = paths.into_iter().collect();
    Ok(found
        .iter()
        .map(|e| {
            to_entity_view(
                e,
                path_map
                    .get(&e.entity_id)
                    .cloned()
                    .unwrap_or_else(|| e.entity_id.to_string()),
            )
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn world_brain_index_scene(
    state: tauri::State<'_, crate::Runtime>,
    rel_path: String,
    source_revision: i64,
) -> Result<SceneView, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    // Re-parse and snapshot whatever scene currently lives at `rel_path` under the
    // project root, mirroring the engine's on-disk scene to the World Brain.
    let abs = root.join(&rel_path);
    let text = std::fs::read_to_string(&abs).map_err(|error| {
        AppError::plain(format!("Cannot read scene file {}: {error}", abs.display()))
    })?;
    let doc = bhippi_engine::SceneDocument::parse(&text).map_err(|error| {
        AppError::plain(format!("Invalid scene file {}: {error}", abs.display()))
    })?;
    world
        .index_scene_document(&rel_path, &doc, source_revision)
        .await
        .map_err(AppError::from)?;
    let scene = world
        .scene_by_path(&rel_path)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::plain("Scene was indexed but could not be re-read."))?;
    Ok(to_scene_view(&scene))
}

// ── World Brain asset graph (plan SEC. 7.2, ADR-0025) ─────────────────────

/// A persisted asset row as shipped to the UI: stable ULID id, path, kind and licence,
/// without the raw JSON reverse-usage payload (fetched per-asset on demand).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Type)]
pub struct AssetView {
    pub asset_id: AssetId,
    pub rel_path: String,
    pub kind: String,
    pub license: String,
    pub size_bytes: i64,
    pub source_revision: i64,
}

fn to_asset_view(asset: &AssetRecord) -> AssetView {
    AssetView {
        asset_id: asset.asset_id,
        rel_path: asset.rel_path.clone(),
        kind: asset.kind.clone(),
        license: asset.license.clone(),
        size_bytes: asset.size_bytes,
        source_revision: asset.source_revision,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn world_brain_assets(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<AssetView>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    let assets = world.project_assets().await.map_err(AppError::from)?;
    Ok(assets.iter().map(to_asset_view).collect())
}

#[tauri::command]
#[specta::specta]
pub async fn world_brain_assets_by_kind(
    state: tauri::State<'_, crate::Runtime>,
    kind: String,
) -> Result<Vec<AssetView>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    let assets = world.assets_by_kind(&kind).await.map_err(AppError::from)?;
    Ok(assets.iter().map(to_asset_view).collect())
}

/// Scenes that reference an asset — the answer to "what uses this asset?".
#[tauri::command]
#[specta::specta]
pub async fn world_brain_asset_usage(
    state: tauri::State<'_, crate::Runtime>,
    asset_id: AssetId,
) -> Result<Vec<String>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    world
        .asset_reverse_usage(asset_id)
        .await
        .map_err(AppError::from)
}

/// Re-scan the project's `assets/` tree with the engine's indexer and persist the result
/// into the World Brain, replacing the previous asset snapshot. Returns the number of
/// assets indexed.
#[tauri::command]
#[specta::specta]
pub async fn world_brain_index_assets(
    state: tauri::State<'_, crate::Runtime>,
    source_revision: i64,
) -> Result<usize, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    let index = bhippi_engine::asset::AssetIndex::scan(&root)
        .map_err(|error| AppError::plain(format!("Cannot scan the project's assets: {error}")))?;
    world
        .index_asset_index(&index, source_revision)
        .await
        .map_err(AppError::from)?;
    Ok(index.count())
}

/// A persisted World Brain physics body/collider (SEC 7.3, ADR-0026). `body_kind` is
/// one of static/dynamic/kinematic (or null for a collider-only entity); `collider_shape`
/// is the authored shape descriptor. `mass` is in kg for dynamic bodies.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct PhysicsBodyView {
    pub entity_id: EntityId,
    pub scene_id: SceneId,
    pub body_kind: Option<String>,
    pub mass: Option<f64>,
    pub lock_rotation: Option<i64>,
    pub collider_shape: Option<String>,
    pub sensor: Option<i64>,
    pub has_character_controller: bool,
    pub source_revision: i64,
}

fn to_physics_view(body: &PhysicsBodyRecord) -> PhysicsBodyView {
    PhysicsBodyView {
        entity_id: body.entity_id,
        scene_id: body.scene_id,
        body_kind: body.body_kind.clone(),
        mass: body.mass,
        lock_rotation: body.lock_rotation,
        collider_shape: body.collider_shape.clone(),
        sensor: body.sensor,
        has_character_controller: body.has_character_controller,
        source_revision: body.source_revision,
    }
}

/// Every rigid body / collider recorded across the project's scenes — the physics
/// graph answer for "which entities are dynamic bodies?", "what colliders exist?".
#[tauri::command]
#[specta::specta]
pub async fn world_brain_physics(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<PhysicsBodyView>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    let bodies = world.project_physics().await.map_err(AppError::from)?;
    Ok(bodies.iter().map(to_physics_view).collect())
}

/// The rigid bodies / colliders belonging to one scene.
#[tauri::command]
#[specta::specta]
pub async fn world_brain_physics_by_scene(
    state: tauri::State<'_, crate::Runtime>,
    scene_id: SceneId,
) -> Result<Vec<PhysicsBodyView>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    let bodies = world
        .scene_physics(scene_id)
        .await
        .map_err(AppError::from)?;
    Ok(bodies.iter().map(to_physics_view).collect())
}

/// The physics record for one entity, if it carries any physics component.
#[tauri::command]
#[specta::specta]
pub async fn world_brain_physics_by_entity(
    state: tauri::State<'_, crate::Runtime>,
    entity_id: EntityId,
) -> Result<Option<PhysicsBodyView>, AppError> {
    let root = active_project_path(state.inner()).await?;
    let world = open_world_brain(state.inner(), &root).await?;
    Ok(world
        .physics_by_entity(entity_id)
        .await
        .map_err(AppError::from)?
        .map(|body| to_physics_view(&body)))
}
