//! Game-engine workbench commands (ADR-0020, ENG-003, ENG-100…109).
//!
//! Scene editing goes through [`session::EngineSessions`]: one in-memory document per open
//! scene, one undo stack shared by the user and the agent, one journal row per applied
//! transaction (INV-070, INV-071). The webview holds no scene state of its own.

pub mod bridge;
pub mod content;
pub mod hud_session;
pub mod observation;
pub mod query_bridge;
pub mod session;
pub mod telemetry;

pub use telemetry::{
    engine_clear_play_stats, engine_console_rows, engine_record_console,
    engine_record_console_source, engine_record_play_stats, EngineConsoleRow, EnginePlayStats,
};

pub use observation::{
    engine_submit_playtest, engine_submit_screenshot, EnginePlaytestRequested,
    EngineScreenshotRequested,
};

use crate::commands::{required_project_path, AppError};
use bhippi_engine::action::EngineAction;
use bhippi_engine::api::{EntityQuery, SceneQueries};
use bhippi_engine::asset::AssetIndex;
use bhippi_engine::document::SceneDocument;
use bhippi_engine::manifest::{load_manifest, GameManifest};
use bhippi_engine::mindmap;
use bhippi_engine::query;
use bhippi_engine::scaffold;
use bhippi_types::{AssetId, EngineActor, EntityId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use session::{
    AppliedBatch, AppliedEdit, EngineBatchResult, EngineEditResult, EngineSceneState,
    EngineSessions, JournalFacts,
};
use specta::Type;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Mutex, OnceLock};
use tauri_specta::Event;

fn engine_error(error: bhippi_engine::EngineError) -> AppError {
    AppError {
        message: error.to_string(),
        hint: error.hint().map(str::to_owned),
    }
}

/// The project-root-relative location a game was (or would be) scaffolded at.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub struct EngineGameView {
    pub name: String,
    pub version: String,
    pub default_scene: String,
    pub engine_track: String,
    pub targets: Vec<String>,
    pub scene_exists: bool,
    pub hud_scene: Option<String>,
    pub levels: Vec<String>,
    pub scenes: Vec<String>,
}

impl EngineGameView {
    fn from_manifest(manifest: &GameManifest, root: &Path) -> Self {
        let engine_track = match manifest.game.engine_track {
            bhippi_engine::EngineTrack::Rust => "rust",
            bhippi_engine::EngineTrack::Scripted => "scripted",
        };
        let mut scenes = list_scene_files(root);
        if !scenes
            .iter()
            .any(|path| path == &manifest.game.default_scene)
        {
            scenes.insert(0, manifest.game.default_scene.clone());
        }
        Self {
            name: manifest.game.name.clone(),
            version: manifest.game.version.clone(),
            default_scene: manifest.game.default_scene.clone(),
            engine_track: engine_track.to_owned(),
            targets: manifest
                .enabled_targets()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            scene_exists: root.join(&manifest.game.default_scene).is_file(),
            hud_scene: manifest.game.hud_scene.clone(),
            levels: if manifest.game.levels.is_empty() {
                scenes
                    .iter()
                    .filter(|path| path.contains("level_") || path.ends_with("level.bscn.json"))
                    .cloned()
                    .collect()
            } else {
                manifest.game.levels.clone()
            },
            scenes,
        }
    }
}

fn list_scene_files(root: &Path) -> Vec<String> {
    let dir = root.join("assets").join("scenes");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut scenes: Vec<String> = entries
        .filter_map(std::result::Result::ok)
        .filter(|entry| {
            entry
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".bscn.json"))
        })
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            format!("assets/scenes/{name}")
        })
        .collect();
    scenes.sort();
    scenes
}

/// Replaces `../dir`-style tricks with a plain error and strips a single trailing slash, so
/// a crafted input never points the scaffold outside the active project.
fn sanitise_folder_name(folder_name: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(raw) = folder_name else {
        return Ok(None);
    };
    let name = raw.trim().trim_end_matches(['/', '\\']);
    if name.is_empty() {
        return Err(AppError::plain("Give the game folder a name."));
    }
    let allowed = name
        .chars()
        .all(|ch| ch.is_alphanumeric() || matches!(ch, ' ' | '_' | '-'));
    if !allowed || name.contains("..") {
        return Err(AppError {
            message:
                "The folder name can only use letters, digits, spaces, dashes and underscores."
                    .to_owned(),
            hint: Some("Pick a name like \"my-game\" and try again.".to_owned()),
        });
    }
    Ok(Some(name.to_owned()))
}

/// The directory that owns the active project's game. Prefers the project root itself;
/// falls back to a single game folder one level down (the New Game flow's home). Multiple
/// candidates mean the caller must pick, so this returns `Ok(None)` for ambiguity.
fn game_root(project_root: &Path) -> Result<Option<PathBuf>, AppError> {
    if bhippi_engine::manifest::manifest_path(project_root).is_file() {
        return Ok(Some(project_root.to_path_buf()));
    }
    let mut candidates = Vec::new();
    let Ok(mut entries) = std::fs::read_dir(project_root) else {
        return Ok(None);
    };
    while let Some(entry) = entries.next().transpose().map_err(|error| AppError {
        message: format!("Cannot read the project folder: {error}"),
        hint: Some("Check the folder is readable.".to_owned()),
    })? {
        let path = entry.path();
        if path.is_dir() && bhippi_engine::manifest::manifest_path(&path).is_file() {
            candidates.push(path);
        }
    }
    Ok(if candidates.len() == 1 {
        candidates.pop()
    } else {
        None
    })
}

/// The engine bound for the active project: whether it has a playable game, and where.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub struct EngineStatus {
    pub game_root: Option<String>,
    pub game: Option<EngineGameView>,
}

#[tauri::command]
#[specta::specta]
pub async fn get_engine_status(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<EngineStatus, AppError> {
    let project_root = required_project_path(&state).await?;
    let root = PathBuf::from(&project_root);
    let Some(game_dir) = game_root(&root)? else {
        return Ok(EngineStatus {
            game_root: None,
            game: None,
        });
    };
    let Some(manifest) = load_manifest(&game_dir).map_err(engine_error)? else {
        return Ok(EngineStatus {
            game_root: None,
            game: None,
        });
    };
    Ok(EngineStatus {
        game_root: Some(game_dir.to_string_lossy().into_owned()),
        game: Some(EngineGameView::from_manifest(&manifest, &game_dir)),
    })
}

/// Scaffolds a new game into the active project and reports the result. With no folder
/// name the manifest is written at the project root; otherwise into `<root>/<name>`
/// (non-game projects keep their code and game side by side).
#[tauri::command]
#[specta::specta]
pub async fn engine_create_game_manifest(
    state: tauri::State<'_, crate::Runtime>,
    folder_name: Option<String>,
    force: bool,
) -> Result<EngineStatus, AppError> {
    let project_root = required_project_path(&state).await?;
    let root = PathBuf::from(&project_root);
    let name = sanitise_folder_name(folder_name.as_deref())?;
    let game_dir = match name {
        Some(name) => {
            let dir = root.join(&name);
            if dir == root
                || !dir
                    .strip_prefix(&root)
                    .map(|rest| {
                        rest.components()
                            .all(|component| matches!(component, std::path::Component::Normal(_)))
                    })
                    .unwrap_or(false)
            {
                return Err(AppError::plain(
                    "The game folder must live inside the project.",
                ));
            }
            dir
        }
        None => root.clone(),
    };

    let display_name = game_dir
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("My Game");
    let written = scaffold::write_project(&game_dir, display_name, force).map_err(engine_error)?;
    tracing::info!(files = written.len(), root = %game_dir.display(), "game scaffold written");

    let Some(manifest) = load_manifest(&game_dir).map_err(engine_error)? else {
        return Err(AppError::plain("The scaffold did not produce a manifest."));
    };
    Ok(EngineStatus {
        game_root: Some(game_dir.to_string_lossy().into_owned()),
        game: Some(EngineGameView::from_manifest(&manifest, &game_dir)),
    })
}

/// Structured scene snapshot the AI and the editor share (engine plan §76–78).
#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct EngineSceneQuery {
    pub scene_path: String,
    pub name: String,
    pub entity_count: u32,
    pub digest: String,
    pub hierarchy: Vec<query::HierarchyEntry>,
}

/// One applied transaction, broadcast so panels patch what changed instead of reloading
/// the scene (ENG-107). `touched` is the entity list the viewport re-reads; everything
/// else is what the toast and the history row render.
#[derive(Clone, Debug, Deserialize, Serialize, Type, Event)]
pub struct EngineSceneChanged {
    pub scene_path: String,
    pub summary: String,
    pub txn_id: String,
    /// `user` | `agent`
    pub actor: String,
    pub label: String,
    pub touched: Vec<EntityId>,
    pub entity_count: u32,
    pub dirty: bool,
    pub revision: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct EngineSceneDiff {
    pub scene_path: String,
    pub mine_json: String,
    pub disk_json: String,
}

fn resolve_scene_path(game_dir: &Path, scene_rel: Option<&str>) -> Result<PathBuf, AppError> {
    let manifest = load_manifest(game_dir)
        .map_err(engine_error)?
        .ok_or_else(|| AppError::plain("This project has no game manifest."))?;
    let rel = scene_rel
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .unwrap_or(manifest.game.default_scene.as_str());
    if rel.contains("..") {
        return Err(AppError::plain("That scene path leaves the game folder."));
    }
    Ok(game_dir.join(rel.replace('\\', "/")))
}

fn load_scene_file(path: &Path) -> Result<SceneDocument, AppError> {
    let text = std::fs::read_to_string(path).map_err(|error| AppError {
        message: format!("Could not read the scene: {error}"),
        hint: Some("Save the scene from the Engine pane first.".to_owned()),
    })?;
    SceneDocument::parse_lenient(&text).map_err(engine_error)
}

/// Resolve every reference-shaped field in an action payload from a human/AI form (a plain
/// name, or a `scene:/Path#ULID`) to a real `EntityId`.
///
/// Every field that can hold a reference has to be listed here. `look_at` takes a `target`
/// and the multi-entity verbs take an `entities` array — missing those meant a model writing
/// `"target": "Player"`, exactly as the prompt tells it to, got back "invalid length" from
/// serde instead of a resolved id.
fn rewrite_entity_refs(doc: &SceneDocument, value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    for key in ["entity", "parent", "target", "source"] {
        let Some(token) = object.get(key).and_then(Value::as_str).map(str::to_owned) else {
            continue;
        };
        if let Some(id) = resolve_entity_token(doc, &token) {
            object.insert(key.to_owned(), Value::String(id.to_string()));
        }
    }
    for key in ["entities", "targets"] {
        let Some(list) = object.get(key).and_then(Value::as_array).cloned() else {
            continue;
        };
        let resolved: Vec<Value> = list
            .into_iter()
            .map(|item| match item.as_str() {
                Some(token) => resolve_entity_token(doc, token)
                    .map_or(item.clone(), |id| Value::String(id.to_string())),
                None => item,
            })
            .collect();
        object.insert(key.to_owned(), Value::Array(resolved));
    }
}

/// One reference to an id. Already-valid ULIDs pass through untouched; a name that matches
/// nothing is left alone so the action layer reports it rather than this silently dropping it.
fn resolve_entity_token(doc: &SceneDocument, token: &str) -> Option<bhippi_types::EntityId> {
    if token.is_empty() || bhippi_types::EntityId::from_str(token).is_ok() {
        return None;
    }
    query::find_by_name(doc, token)
        .into_iter()
        .next()
        .or_else(|| doc.resolve_ref(token))
}

/// Apply one `EngineAction` JSON document against a scene on disk. Shared by IPC and chat.
/// The process-wide open-scene store (ENG-100). One map, so a gizmo drag in the webview
/// and an `<engine_action>` from a model mutate the *same* document instead of two copies
/// of one file racing each other (INV-070). Guarded by a plain `std::sync::Mutex` because
/// every critical section is synchronous file/CPU work; the async journal write happens
/// after the guard is dropped.
pub fn sessions() -> &'static Mutex<EngineSessions> {
    static SESSIONS: OnceLock<Mutex<EngineSessions>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(EngineSessions::new()))
}

fn sessions_lock() -> Result<std::sync::MutexGuard<'static, EngineSessions>, AppError> {
    sessions().lock().map_err(|_| AppError {
        message: "The engine session store is poisoned.".to_owned(),
        hint: Some("Restart the app; an earlier engine edit panicked.".to_owned()),
    })
}

/// The journal database, registered once at startup. Journaling is a *record*, not a gate:
/// when it is unavailable the edit still applies and the result simply carries no revision.
static JOURNAL_DB: OnceLock<bhippi_db::Database> = OnceLock::new();

/// Called during app setup so `apply_action_in_workspace` can journal from the chat path
/// without threading a database handle through the chat engine.
pub fn register_journal_db(database: bhippi_db::Database) {
    let _ignored = JOURNAL_DB.set(database);
}

fn registered_projects() -> &'static Mutex<std::collections::BTreeSet<String>> {
    static REGISTERED: OnceLock<Mutex<std::collections::BTreeSet<String>>> = OnceLock::new();
    REGISTERED.get_or_init(|| Mutex::new(std::collections::BTreeSet::new()))
}

/// Write one applied transaction into `engine_journal` (INV-071). Returns the revision, or
/// `None` when there is no database — never an error the caller has to handle, because a
/// missing ledger must not roll back a change the user can already see.
pub async fn journal_edit(game_dir: &Path, facts: &JournalFacts) -> Option<i64> {
    let database = JOURNAL_DB.get()?;
    let project_path = game_dir.to_string_lossy().replace('\\', "/");
    let now = chrono::Utc::now();

    let needs_register = registered_projects()
        .lock()
        .map(|seen| !seen.contains(&project_path))
        .unwrap_or(true);
    if needs_register {
        let manifest = load_manifest(game_dir).ok().flatten();
        let record = bhippi_db::EngineProjectRecord {
            project_path: project_path.clone(),
            game_id: manifest
                .as_ref()
                .map(|manifest| manifest.game.id.to_string())
                .unwrap_or_default(),
            game_name: manifest
                .as_ref()
                .map(|manifest| manifest.game.name.clone())
                .unwrap_or_else(|| "Untitled".to_owned()),
            version: manifest
                .as_ref()
                .map(|manifest| manifest.game.version.clone())
                .unwrap_or_else(|| "0.0.0".to_owned()),
            default_scene: manifest
                .as_ref()
                .map(|manifest| manifest.game.default_scene.clone())
                .unwrap_or_default(),
            engine_track: manifest
                .as_ref()
                .map(|manifest| match manifest.game.engine_track {
                    bhippi_engine::EngineTrack::Rust => "rust",
                    bhippi_engine::EngineTrack::Scripted => "scripted",
                })
                .unwrap_or("rust")
                .to_owned(),
            targets_json: manifest
                .as_ref()
                .map(|manifest| {
                    serde_json::to_string(&manifest.enabled_targets()).unwrap_or_default()
                })
                .unwrap_or_else(|| "[]".to_owned()),
            scene_count: list_scene_files(game_dir).len() as i64,
        };
        if let Err(error) = database.engine().upsert_project(&record, &now).await {
            tracing::warn!(%error, project = %project_path, "engine project not registered; transaction not journaled");
            return None;
        }
        if let Ok(mut seen) = registered_projects().lock() {
            seen.insert(project_path.clone());
        }
    }

    let entry = bhippi_db::NewJournalEntry {
        txn_id: facts.txn_id.clone(),
        actor: facts.actor.clone(),
        label: facts.label.clone(),
        scene_rel_path: facts.scene_rel_path.clone(),
        ops_json: facts.ops_json.clone(),
        inverse_json: facts.inverse_json.clone(),
        touched_json: facts.touched_json.clone(),
        op_count: facts.op_count,
    };
    match database.engine().append(&project_path, &entry, &now).await {
        Ok(revision) => Some(revision),
        Err(error) => {
            tracing::warn!(%error, project = %project_path, "engine transaction not journaled");
            None
        }
    }
}

/// The game folder for a workspace, or a typed error explaining there is no game.
pub fn game_dir_of(workspace: &str) -> Result<PathBuf, AppError> {
    let root = PathBuf::from(workspace);
    game_root(&root)?.ok_or_else(|| AppError {
        message: "This project is not a game.".to_owned(),
        hint: Some("Create a game from the Engine pane first.".to_owned()),
    })
}

/// Resolve the scene to act on: the named one, else the manifest's `default_scene`.
fn scene_rel_of(game_dir: &Path, scene_rel: Option<&str>) -> Result<String, AppError> {
    if let Some(rel) = scene_rel.map(str::trim).filter(|rel| !rel.is_empty()) {
        return session::safe_scene_path(game_dir, rel).map(|(_, rel)| rel);
    }
    let manifest = load_manifest(game_dir)
        .map_err(engine_error)?
        .ok_or_else(|| AppError::plain("This project has no game manifest."))?;
    session::safe_scene_path(game_dir, &manifest.game.default_scene).map(|(_, rel)| rel)
}

/// Turn one raw batch payload into a step, resolving entity references against the scene as
/// the batch has built it so far.
///
/// Reference resolution happens **before** the content/scene split, because content actions
/// name entities too — `create_prefab` takes the entity to capture, and a model writes
/// `"entity": "Player"` there exactly as it does everywhere else.
pub fn resolve_batch_step(
    doc: &SceneDocument,
    raw: &Value,
) -> Result<session::BatchStep, AppError> {
    let mut payload = raw.clone();
    rewrite_entity_refs(doc, &mut payload);
    if let Some(action) = content::parse_content_action(&payload) {
        return Ok(session::BatchStep::Content(Box::new(action)));
    }
    // A payload that names a content kind but does not deserialise is a malformed content
    // action, not an unknown scene action. Saying "unknown variant create_material" would
    // send the model looking for the wrong mistake.
    if let Some(kind) = payload.get("kind").and_then(Value::as_str) {
        if content::CONTENT_KINDS.contains(&kind) {
            return Err(AppError {
                message: format!("the {kind} payload is missing or mistyped a required field"),
                hint: Some(
                    "create_material needs name; create_shader needs name and source; \
                     create_prefab needs name and entity; set_asset_license needs path and license."
                        .to_owned(),
                ),
            });
        }
    }
    parse_resolved_action(payload).map(|action| session::BatchStep::Scene(Box::new(action)))
}

fn parse_resolved_action(payload: Value) -> Result<EngineAction, AppError> {
    serde_json::from_value(payload).map_err(|error| AppError {
        message: format!("Unknown engine action: {error}"),
        hint: Some("Ask for the verb list, or check the spelling of `kind`.".to_owned()),
    })
}

/// Parse an `EngineAction` document, resolving human/AI entity references (a name or a
/// `scene:/Path#ULID`) against the *live* scene rather than the file on disk.
fn parse_action(doc: &SceneDocument, action_json: &str) -> Result<EngineAction, AppError> {
    let mut payload: Value = serde_json::from_str(action_json).map_err(|error| AppError {
        message: format!("That engine action is not valid JSON: {error}"),
        hint: Some("Use {\"kind\":\"spawn\",\"template\":\"cube\"}.".to_owned()),
    })?;
    rewrite_entity_refs(doc, &mut payload);
    serde_json::from_value(payload).map_err(|error| AppError {
        message: format!("Unknown engine action: {error}"),
        hint: Some("Check the engine verb list. Outliner folders use create_organizer_folder, rename_organizer_folder, move_organizer_folder, delete_organizer_folder, and move_entity_to_organizer_folder.".to_owned()),
    })
}

/// Apply one `EngineAction` JSON document through the session store. Shared by the IPC
/// commands and the chat bridge, so an agent edit and a user edit are the same write.
///
/// `autosave` is on for agent edits (a model has no Save button and its work must survive
/// the turn) and off for interactive editing, where the user still owns when the file
/// changes.
pub fn apply_action_in_workspace(
    workspace: &str,
    scene_rel: Option<&str>,
    action_json: &str,
    actor: EngineActor,
    label: &str,
    autosave: bool,
) -> Result<AppliedEdit, AppError> {
    let game_dir = game_dir_of(workspace)?;
    let rel = scene_rel_of(&game_dir, scene_rel)?;
    let mut store = sessions_lock()?;
    store.open(&game_dir, &rel)?;
    let doc = store.document(&game_dir, &rel).ok_or_else(|| AppError {
        message: format!("{rel} is not open."),
        hint: Some("Open the scene first.".to_owned()),
    })?;
    let action = parse_action(doc, action_json)?;
    store.apply_action(&game_dir, &rel, &action, actor, label, autosave)
}

/// Apply a batch as one transaction (ENG-111).
///
/// The whole point is atomicity: "build me a warehouse" is thirty actions and one undo. A
/// batch that fails anywhere writes nothing and comes back with the per-action envelope so
/// the caller (or the model) can repair it.
pub fn apply_batch_in_workspace(
    workspace: &str,
    scene_rel: Option<&str>,
    label: &str,
    raw_actions: &[Value],
    actor: EngineActor,
    autosave: bool,
) -> Result<AppliedBatch, AppError> {
    apply_batch_as(
        workspace,
        scene_rel,
        label,
        raw_actions,
        actor,
        autosave,
        None,
        None,
    )
}

/// The same, with the ENG-192 coordination arguments spelled out: which agent is committing
/// and which revision it planned against.
#[allow(clippy::too_many_arguments)]
pub fn apply_batch_as(
    workspace: &str,
    scene_rel: Option<&str>,
    label: &str,
    raw_actions: &[Value],
    actor: EngineActor,
    autosave: bool,
    owner: Option<&str>,
    base_revision: Option<u32>,
) -> Result<AppliedBatch, AppError> {
    let game_dir = game_dir_of(workspace)?;
    // ENG-190: the capability gate. This is the single choke point both agent paths go
    // through, and it is deliberately *not* applied to `EngineActor::User` — a switch that
    // stops the person using the editor is a bug, not a permission.
    if matches!(actor, EngineActor::Agent) {
        let verdict = capability_verdict(&game_dir, raw_actions)?;
        if let Some(refusal) = verdict.refusal() {
            return Err(AppError {
                message: refusal,
                hint: Some(
                    "Nothing was written. Ask the user to allow the capability, or achieve                      the same result without it."
                        .to_owned(),
                ),
            });
        }
    }
    let rel = scene_rel_of(&game_dir, scene_rel)?;
    let mut store = sessions_lock()?;
    store.open(&game_dir, &rel)?;
    store.apply_batch(
        session::BatchRequest {
            game_dir: &game_dir,
            rel_path: &rel,
            label,
            actions: raw_actions,
            actor,
            autosave,
            owner,
            base_revision,
        },
        resolve_batch_step,
    )
}

/// What this project's `[agent]` policy says about a batch (ENG-190).
///
/// A project with no manifest is not a game project, so there is nothing to gate and the
/// shipped defaults apply — the batch will fail for want of a scene long before this
/// matters.
pub fn capability_verdict(
    game_dir: &std::path::Path,
    raw_actions: &[Value],
) -> Result<bhippi_engine::capability::CapabilityVerdict, AppError> {
    let policy = load_manifest(game_dir)
        .map_err(engine_error)?
        .map(|manifest| manifest.agent)
        .unwrap_or_default();
    let kinds: Vec<String> = raw_actions
        .iter()
        .map(|action| {
            action
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_owned()
        })
        .collect();
    Ok(bhippi_engine::capability::evaluate(&policy, &kinds))
}

/// Read this project's agent capability policy, for the settings panel and the prompt.
#[tauri::command]
#[specta::specta]
pub async fn engine_agent_capabilities(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<EngineCapabilityRow>, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let policy = load_manifest(&game_dir)
        .map_err(engine_error)?
        .map(|manifest| manifest.agent)
        .unwrap_or_default();
    Ok(policy
        .effective()
        .into_iter()
        .map(|(capability, decision)| EngineCapabilityRow {
            capability: capability.as_str().to_owned(),
            decision: decision.as_str().to_owned(),
            doc: capability.doc().to_owned(),
            is_default: decision == capability.default_decision(),
        })
        .collect())
}

/// One row of the Agent permissions panel.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineCapabilityRow {
    pub capability: String,
    pub decision: String,
    pub doc: String,
    /// Whether this is still the shipped default, so the panel can say what was changed.
    pub is_default: bool,
}

/// Change one capability and write it back to `Bhippi.game.toml` (ENG-190).
///
/// The manifest is rewritten from the parsed document, so a hand-added comment elsewhere in
/// the file is lost — which is why this is the only writer, and why the panel says the file
/// is the source of truth.
#[tauri::command]
#[specta::specta]
pub async fn engine_set_agent_capability(
    state: tauri::State<'_, crate::Runtime>,
    capability: String,
    decision: String,
) -> Result<Vec<EngineCapabilityRow>, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let parsed_capability = bhippi_engine::capability::Capability::from_name(&capability)
        .ok_or_else(|| AppError {
            message: format!("`{capability}` is not a capability."),
            hint: Some(format!(
                "Capabilities are: {}.",
                bhippi_engine::capability::Capability::ALL
                    .map(bhippi_engine::capability::Capability::as_str)
                    .join(", ")
            )),
        })?;
    let parsed_decision =
        bhippi_engine::capability::Decision::from_name(&decision).ok_or_else(|| AppError {
            message: format!("`{decision}` is not a decision."),
            hint: Some("Use allow, ask or deny.".to_owned()),
        })?;

    let mut manifest = load_manifest(&game_dir)
        .map_err(engine_error)?
        .ok_or_else(|| AppError::plain("This project has no game manifest."))?;
    manifest.agent.set(parsed_capability, parsed_decision);
    let path = game_dir.join(bhippi_engine::GAME_MANIFEST_FILE);
    // One writer for this file (`scaffold::format_manifest`), so the panel and the scaffold
    // cannot disagree about its shape.
    let text = bhippi_engine::scaffold::format_manifest(&manifest);
    std::fs::write(&path, text).map_err(|error| AppError {
        message: format!("Could not write {}: {error}", path.display()),
        hint: Some("Check the file is not read-only or open elsewhere.".to_owned()),
    })?;

    engine_agent_capabilities(state).await
}

/// Apply an agent batch, declaring who is committing and what it planned against (ENG-192).
///
/// The chat bridge uses this so two agents in the same project cannot silently overwrite one
/// another: the second one is refused with a rebase prompt naming the revision it missed.
pub async fn apply_agent_batch_as(
    workspace: &str,
    scene_rel: Option<&str>,
    payload: &str,
    owner: Option<&str>,
    base_revision: Option<u32>,
) -> Result<EngineBatchResult, AppError> {
    let game_dir = game_dir_of(workspace)?;
    let (label, actions) = parse_batch_payload(payload)?;
    let applied = apply_batch_as(
        workspace,
        scene_rel,
        &format!("ai:{label}"),
        &actions,
        EngineActor::Agent,
        true,
        owner,
        base_revision,
    )?;
    let mut result = applied.result;
    if let (Some(edit), Some(facts)) = (result.edit.as_mut(), applied.journal.as_ref()) {
        edit.revision = journal_edit(&game_dir, facts).await;
    }
    Ok(result)
}

/// Parse an `<engine_batch>` payload into its label and raw action list.
pub fn parse_batch_payload(payload: &str) -> Result<(String, Vec<Value>), AppError> {
    let value: Value = serde_json::from_str(payload).map_err(|error| AppError {
        message: format!("That engine batch is not valid JSON: {error}"),
        hint: Some(
            "Use {\"label\":\"what this change is\",\"actions\":[{\"kind\":\"spawn\",...}]}."
                .to_owned(),
        ),
    })?;
    let label = value
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or("ai:engine_batch")
        .to_owned();
    let actions = value
        .get("actions")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| AppError {
            message: "The engine batch has no \"actions\" array.".to_owned(),
            hint: Some("Add \"actions\": [ … ] with at least one action.".to_owned()),
        })?;
    Ok((label, actions))
}

/// Apply an agent batch and journal it. The chat bridge's batch entry point.
pub async fn apply_agent_batch(
    workspace: &str,
    scene_rel: Option<&str>,
    payload: &str,
) -> Result<EngineBatchResult, AppError> {
    let game_dir = game_dir_of(workspace)?;
    let (label, actions) = parse_batch_payload(payload)?;
    let applied = apply_batch_in_workspace(
        workspace,
        scene_rel,
        &format!("ai:{label}"),
        &actions,
        EngineActor::Agent,
        true,
    )?;
    let mut result = applied.result;
    if let (Some(edit), Some(facts)) = (result.edit.as_mut(), applied.journal.as_ref()) {
        edit.revision = journal_edit(&game_dir, facts).await;
    }
    Ok(result)
}

/// Apply one agent action as a one-action batch, so a single `<engine_action>` and an
/// `<engine_batch>` come back through the same envelope and the caller needs only one
/// code path for both protocol forms.
pub async fn apply_agent_single(
    workspace: &str,
    scene_rel: Option<&str>,
    action_json: &str,
    owner: Option<&str>,
) -> Result<EngineBatchResult, AppError> {
    let action: Value = serde_json::from_str(action_json).map_err(|error| AppError {
        message: format!("That engine action is not valid JSON: {error}"),
        hint: Some("Use {\"kind\":\"spawn\",\"template\":\"cube\"}.".to_owned()),
    })?;
    let label = action
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("engine_action")
        .to_owned();
    let game_dir = game_dir_of(workspace)?;
    let applied = apply_batch_as(
        workspace,
        scene_rel,
        &format!("ai:{label}"),
        std::slice::from_ref(&action),
        EngineActor::Agent,
        true,
        owner,
        None,
    )?;
    let mut result = applied.result;
    if let (Some(edit), Some(facts)) = (result.edit.as_mut(), applied.journal.as_ref()) {
        edit.revision = journal_edit(&game_dir, facts).await;
    }
    Ok(result)
}

/// Apply an agent action and journal it (single-transaction path used by tests and by
/// callers that need the raw edit result rather than the batch envelope).
pub async fn apply_agent_action(
    workspace: &str,
    scene_rel: Option<&str>,
    action_json: &str,
) -> Result<EngineEditResult, AppError> {
    let game_dir = game_dir_of(workspace)?;
    let applied = apply_action_in_workspace(
        workspace,
        scene_rel,
        action_json,
        EngineActor::Agent,
        "ai:engine_action",
        true,
    )?;
    let mut result = applied.result;
    result.revision = journal_edit(&game_dir, &applied.journal).await;
    Ok(result)
}

pub fn query_scene_in_workspace(
    workspace: &str,
    scene_rel: Option<&str>,
) -> Result<EngineSceneQuery, AppError> {
    let game_dir = game_dir_of(workspace)?;
    let rel = scene_rel_of(&game_dir, scene_rel)?;
    // Prefer the open session: what the AI is told about the scene must be what the editor
    // is actually holding, including edits the user has not saved yet.
    let mut store = sessions_lock()?;
    store.open(&game_dir, &rel)?;
    let doc = store.document(&game_dir, &rel).ok_or_else(|| AppError {
        message: format!("{rel} is not open."),
        hint: Some("Open the scene first.".to_owned()),
    })?;
    Ok(EngineSceneQuery {
        scene_path: rel.clone(),
        name: doc.name.clone(),
        entity_count: doc.entity_count() as u32,
        digest: mindmap::digest_text(doc, 0),
        hierarchy: query::hierarchy(doc),
    })
}

/// The state of an open scene without touching the disk — used to tell the model what the
/// user currently has selected.
#[must_use]
pub fn open_scene_state(workspace: &str, scene_rel: Option<&str>) -> Option<EngineSceneState> {
    let game_dir = game_dir_of(workspace).ok()?;
    let rel = scene_rel_of(&game_dir, scene_rel).ok()?;
    let store = sessions_lock().ok()?;
    store.state(&game_dir, &rel)
}

/// The last `limit` journal rows for a game, newest first. Empty when there is no journal —
/// context is a courtesy, never a reason to fail a turn.
pub async fn recent_journal(game_dir: &Path, limit: u32) -> Vec<bhippi_db::JournalRecord> {
    let Some(database) = JOURNAL_DB.get() else {
        return Vec::new();
    };
    let project_path = game_dir.to_string_lossy().replace('\\', "/");
    database
        .engine()
        .list(&project_path, None, i64::from(limit))
        .await
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub async fn engine_query_scene(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
) -> Result<EngineSceneQuery, AppError> {
    let project_root = required_project_path(&state).await?;
    query_scene_in_workspace(&project_root, scene.as_deref())
}

/// Broadcast a scene edit and hand the result back to the caller.
fn announce(app: &tauri::AppHandle, result: &EngineEditResult) {
    let _ignored = EngineSceneChanged {
        scene_path: result.scene_path.clone(),
        summary: result.summary.clone(),
        txn_id: result.txn_id.clone(),
        actor: result.actor.clone(),
        label: result.label.clone(),
        touched: result.touched.clone(),
        entity_count: result.state.entity_count,
        dirty: result.state.dirty,
        revision: result.state.revision,
    }
    .emit(app);
}

/// Apply one action as the **user** (the editor's own edits: Add, Delete, Duplicate,
/// Inspector fields, weather). Journaled like every other transaction.
#[tauri::command]
#[specta::specta]
pub async fn engine_apply_action(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    action_json: String,
    scene: Option<String>,
    label: Option<String>,
) -> Result<EngineEditResult, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let applied = apply_action_in_workspace(
        &project_root,
        scene.as_deref(),
        &action_json,
        EngineActor::User,
        label.as_deref().unwrap_or("edit"),
        false,
    )?;
    let mut result = applied.result;
    result.revision = journal_edit(&game_dir, &applied.journal).await;
    announce(&app, &result);
    Ok(result)
}

/// Apply a batch of actions as one transaction, one journal row, one undo step.
#[tauri::command]
#[specta::specta]
pub async fn engine_apply_batch(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    label: String,
    actions_json: String,
    scene: Option<String>,
) -> Result<EngineBatchResult, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let actions: Vec<Value> = serde_json::from_str(&actions_json).map_err(|error| AppError {
        message: format!("The action list is not valid JSON: {error}"),
        hint: Some("Pass a JSON array of actions.".to_owned()),
    })?;
    let applied = apply_batch_in_workspace(
        &project_root,
        scene.as_deref(),
        &label,
        &actions,
        EngineActor::User,
        false,
    )?;
    let mut result = applied.result;
    if let Some(edit) = result.edit.as_mut() {
        if let Some(facts) = applied.journal.as_ref() {
            edit.revision = journal_edit(&game_dir, facts).await;
        }
        announce(&app, edit);
    }
    Ok(result)
}

/// Open a scene into the session store and return its state. Opening twice is free — the
/// second call returns the live document, unsaved edits included.
#[tauri::command]
#[specta::specta]
pub async fn engine_open_scene(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let mut store = sessions_lock()?;
    store.open(&game_dir, &rel)
}

/// Throw away the in-memory document and re-read the file. The escape hatch when the scene
/// changed underneath an open session and the user chooses "Take disk" (ENG-108).
#[tauri::command]
#[specta::specta]
pub async fn engine_reload_scene(
    state: tauri::State<'_, crate::Runtime>,
    scene: String,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, Some(&scene))?;
    let mut store = sessions_lock()?;
    store.reload(&game_dir, &rel)
}

/// Return both conflict sides without mutating either one (ENG-108 Diff).
#[tauri::command]
#[specta::specta]
pub async fn engine_scene_diff(
    state: tauri::State<'_, crate::Runtime>,
    scene: String,
) -> Result<EngineSceneDiff, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, Some(&scene))?;
    let mine_json = sessions_lock()?
        .document(&game_dir, &rel)
        .ok_or_else(|| AppError {
            message: format!("{rel} is not open."),
            hint: Some("Open the scene before comparing it with the disk copy.".to_owned()),
        })?
        .dump()
        .map_err(engine_error)?;
    let disk_json = std::fs::read_to_string(game_dir.join(&rel)).map_err(|error| AppError {
        message: format!("Could not read the disk side of the conflict: {error}"),
        hint: Some("Keep your edits until the file becomes readable again.".to_owned()),
    })?;
    Ok(EngineSceneDiff {
        scene_path: rel,
        mine_json,
        disk_json,
    })
}

/// Close a scene. A dirty scene refuses unless `discard` is set, so no stray navigation
/// can silently lose work.
#[tauri::command]
#[specta::specta]
pub async fn engine_close_scene(
    state: tauri::State<'_, crate::Runtime>,
    scene: String,
    discard: bool,
) -> Result<(), AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, Some(&scene))?;
    let mut store = sessions_lock()?;
    store.close(&game_dir, &rel, discard)
}

#[tauri::command]
#[specta::specta]
pub async fn engine_save_scene(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let mut store = sessions_lock()?;
    store.save(&game_dir, &rel)
}

#[tauri::command]
#[specta::specta]
pub async fn engine_save_all(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<String>, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let mut store = sessions_lock()?;
    store.save_all(&game_dir)
}

/// Undo the last transaction on a scene — user edits and agent batches share one stack, so
/// Ctrl+Z reverses "what the AI just did" exactly like it reverses a drag.
#[tauri::command]
#[specta::specta]
pub async fn engine_undo(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let next = {
        let mut store = sessions_lock()?;
        store.undo(&game_dir, &rel)?
    };
    let _ignored = EngineSceneChanged {
        scene_path: next.scene_path.clone(),
        summary: "undo".to_owned(),
        txn_id: String::new(),
        actor: "user".to_owned(),
        label: "undo".to_owned(),
        touched: Vec::new(),
        entity_count: next.entity_count,
        dirty: next.dirty,
        revision: next.revision,
    }
    .emit(&app);
    Ok(next)
}

#[tauri::command]
#[specta::specta]
pub async fn engine_redo(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let next = {
        let mut store = sessions_lock()?;
        store.redo(&game_dir, &rel)?
    };
    let _ignored = EngineSceneChanged {
        scene_path: next.scene_path.clone(),
        summary: "redo".to_owned(),
        txn_id: String::new(),
        actor: "user".to_owned(),
        label: "redo".to_owned(),
        touched: Vec::new(),
        entity_count: next.entity_count,
        dirty: next.dirty,
        revision: next.revision,
    }
    .emit(&app);
    Ok(next)
}

/// Start an interactive edit. A gizmo drag calls this once, records while dragging, and
/// commits on release — one undo entry for the whole drag (ENG-102).
#[tauri::command]
#[specta::specta]
pub async fn engine_begin_interaction(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    label: String,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let mut store = sessions_lock()?;
    store.open(&game_dir, &rel)?;
    store.begin_interaction(&game_dir, &rel, &label)
}

#[tauri::command]
#[specta::specta]
pub async fn engine_record_interaction(
    state: tauri::State<'_, crate::Runtime>,
    action_json: String,
    scene: Option<String>,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let mut store = sessions_lock()?;
    let doc = store.document(&game_dir, &rel).ok_or_else(|| AppError {
        message: format!("{rel} is not open."),
        hint: Some("Open the scene first.".to_owned()),
    })?;
    let action = parse_action(doc, &action_json)?;
    store.record_interaction(&game_dir, &rel, &action)
}

#[tauri::command]
#[specta::specta]
pub async fn engine_commit_interaction(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
) -> Result<Option<EngineEditResult>, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let applied = {
        let mut store = sessions_lock()?;
        store.commit_interaction(&game_dir, &rel)?
    };
    let Some(applied) = applied else {
        return Ok(None);
    };
    let mut result = applied.result;
    result.revision = journal_edit(&game_dir, &applied.journal).await;
    announce(&app, &result);
    Ok(Some(result))
}

#[tauri::command]
#[specta::specta]
pub async fn engine_cancel_interaction(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let mut store = sessions_lock()?;
    store.cancel_interaction(&game_dir, &rel)
}

#[tauri::command]
#[specta::specta]
pub async fn engine_set_selection(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    selection: Vec<EntityId>,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let mut store = sessions_lock()?;
    store.open(&game_dir, &rel)?;
    store.set_selection(&game_dir, &rel, selection)
}

/// One row of the transaction journal, as the Engine pane's history list renders it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineHistoryEntry {
    pub revision: i64,
    pub txn_id: String,
    /// `user` | `agent`
    pub actor: String,
    pub issued_at: String,
    pub label: String,
    pub scene_path: String,
    pub op_count: i64,
    pub touched: Vec<EntityId>,
}

/// The journal for this game, newest first. This is the honest answer to "what did the
/// agent change?" (INV-071) and it outlives the process.
#[tauri::command]
#[specta::specta]
pub async fn engine_history(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<EngineHistoryEntry>, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let Some(database) = JOURNAL_DB.get() else {
        return Ok(Vec::new());
    };
    let project_path = game_dir.to_string_lossy().replace('\\', "/");
    let scene_rel = match scene.as_deref() {
        Some(scene) => Some(scene_rel_of(&game_dir, Some(scene))?),
        None => None,
    };
    let rows = database
        .engine()
        .list(
            &project_path,
            scene_rel.as_deref(),
            i64::from(limit.unwrap_or(100)),
        )
        .await
        .map_err(|error| AppError {
            message: format!("Could not read the engine journal: {error}"),
            hint: Some("Run `bhippi doctor` and retry.".to_owned()),
        })?;
    Ok(rows
        .into_iter()
        .map(|row| EngineHistoryEntry {
            revision: row.revision,
            txn_id: row.txn_id,
            actor: row.actor,
            issued_at: row.issued_at,
            label: row.label.unwrap_or_default(),
            scene_path: row.scene_rel_path,
            op_count: row.op_count,
            touched: serde_json::from_str(&row.touched_json).unwrap_or_default(),
        })
        .collect())
}

/// Undo one journalled change as a single operation (ENG-189).
///
/// The unit is the transaction, and a batch is already one transaction (ENG-111), so
/// "undo everything the agent just did" is one row here rather than N presses of Ctrl+Z.
/// It works across restarts because the inverse comes from the journal rather than from the
/// in-memory undo stack.
///
/// The revert is applied as a **new** transaction by the user, so it is itself undoable and
/// appears in the history — reverting is a decision, and decisions get taken back too.
#[tauri::command]
#[specta::specta]
pub async fn engine_undo_journalled(
    state: tauri::State<'_, crate::Runtime>,
    txn_id: String,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let database = JOURNAL_DB.get().ok_or_else(|| AppError {
        message: "The engine journal is not available.".to_owned(),
        hint: Some("Reopen the project; the journal opens with it.".to_owned()),
    })?;
    let project_path = game_dir.to_string_lossy().replace('\\', "/");

    // Scan back far enough to cover a session's worth of work. The history panel only ever
    // offers rows it has itself listed, so a deeper scan would find nothing the user can ask
    // for.
    let rows = database
        .engine()
        .list(&project_path, None, 500)
        .await
        .map_err(|error| AppError {
            message: format!("Could not read the engine journal: {error}"),
            hint: Some("Run `bhippi doctor` and retry.".to_owned()),
        })?;
    let row = rows
        .into_iter()
        .find(|row| row.txn_id == txn_id)
        .ok_or_else(|| AppError {
            message: format!("No journalled change with id {txn_id}."),
            hint: Some("Refresh the history; it may have scrolled out of the window.".to_owned()),
        })?;

    let mut store = sessions_lock()?;
    store.open(&game_dir, &row.scene_rel_path)?;
    let state = store.revert_journalled(
        &game_dir,
        &row.scene_rel_path,
        &row.label.clone().unwrap_or_else(|| "change".to_owned()),
        &row.inverse_json,
    )?;
    drop(store);
    Ok(state)
}

/// The world Play runs: Main + one level + the HUD, composed in Rust (ENG-105 / ENG-170).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EnginePlayWorld {
    pub scene_path: String,
    pub level_path: Option<String>,
    pub hud_path: Option<String>,
    /// The composed `bhippi-scene@1` world, ready to render.
    pub document_json: String,
    /// The `bhippi-hud@1` overlay drawn on top, when the game has one. Separate from the
    /// world because the HUD is 2D and edited as a HUD, not merged in as 3D entities.
    pub hud_json: Option<String>,
    /// Widgets pre-resolved to canvas pixels, so the overlay renderer places nothing itself.
    pub hud_widgets: Vec<hud_session::HudWidgetView>,
    /// The reference resolution the widget rects are expressed in.
    pub hud_reference: [f32; 2],
    /// Runtime gravity comes from the manifest, never from renderer constants.
    pub gravity: [f32; 3],
    /// Validated, hand-editable named input actions and axes.
    pub input: bhippi_engine::input::InputDocument,
    /// Ordered level paths available to runtime level travel.
    pub levels: Vec<String>,
    /// Gameplay scripts compiled for this world (ADR-0030): one entry per entity whose
    /// `ScriptRef` resolved and compiled. The webview VM executes these; it never parses.
    pub scripts: Vec<EngineCompiledScript>,
    /// Scripts that would not compile. Play still starts — those entities simply run
    /// unscripted — and the fault lands in the Output Log with its file and line, because a
    /// game that refuses to start over one prop's typo is worse than a located error.
    pub script_faults: Vec<bhippi_engine::script::ScriptFault>,
}

/// One entity's compiled gameplay script.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize, specta::Type)]
pub struct EngineCompiledScript {
    pub entity: String,
    pub path: String,
    pub program: bhippi_engine::script::ScriptProgram,
}

/// Compile every `ScriptRef` in a composed world.
///
/// Reading happens once per play, not per frame — the whole point of ADR-0030's split. A
/// missing file and a broken file are both faults with a path, never a silent no-op.
fn compile_world_scripts(
    game_dir: &std::path::Path,
    world: &bhippi_engine::SceneDocument,
) -> (
    Vec<EngineCompiledScript>,
    Vec<bhippi_engine::script::ScriptFault>,
) {
    let mut compiled = Vec::new();
    let mut faults = Vec::new();
    for entity in &world.entities {
        let Some(payload) = entity.components.get("ScriptRef") else {
            continue;
        };
        let Some(path) = payload.get("script").and_then(|value| value.as_str()) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let full = game_dir.join(path);
        let source = match std::fs::read_to_string(&full) {
            Ok(text) => text,
            Err(error) => {
                faults.push(bhippi_engine::script::ScriptFault {
                    file: path.to_owned(),
                    line: 0,
                    column: 0,
                    message: format!("`{}` could not be read: {error}", entity.name),
                    hint: Some(format!(
                        "Create {path}, or clear the ScriptRef on `{}`.",
                        entity.name
                    )),
                });
                continue;
            }
        };
        match bhippi_engine::script::compile(path, &source) {
            Ok(program) => compiled.push(EngineCompiledScript {
                entity: entity.id.to_string(),
                path: path.to_owned(),
                program,
            }),
            Err(fault) => faults.push(fault),
        }
    }
    (compiled, faults)
}

/// Compose the playable world for a scene. Play on **Main** runs the whole game; play on a
/// level runs that map plus the HUD; opening the HUD alone plays nothing but itself, so its
/// widgets can be rearranged without a level in the way.
#[tauri::command]
#[specta::specta]
pub async fn engine_play_world(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
) -> Result<EnginePlayWorld, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let manifest = load_manifest(&game_dir).map_err(engine_error)?;
    let gravity = manifest
        .as_ref()
        .map_or([0.0, -9.81, 0.0], |value| value.physics.gravity);
    let levels = manifest
        .as_ref()
        .map_or_else(Vec::new, |value| value.game.levels.clone());
    let input_path = game_dir.join(bhippi_engine::input::DEFAULT_INPUT_PATH);
    let input = if input_path.is_file() {
        let text = std::fs::read_to_string(&input_path).map_err(|error| AppError {
            message: format!("Could not read {}: {error}", input_path.display()),
            hint: Some("Restore assets/input.json or fix its file permissions.".to_owned()),
        })?;
        bhippi_engine::input::InputDocument::parse(&text).map_err(engine_error)?
    } else {
        bhippi_engine::input::InputDocument::default()
    };

    let mut store = sessions_lock()?;
    store.open(&game_dir, &rel)?;
    let opened = store
        .document(&game_dir, &rel)
        .ok_or_else(|| AppError::plain("The scene could not be opened."))?
        .clone();

    let kind = opened.settings.kind;
    let level_path = if matches!(kind, bhippi_engine::document::SceneKind::Main) {
        opened.settings.levels.first().cloned().or_else(|| {
            manifest
                .as_ref()
                .and_then(|m| m.game.levels.first().cloned())
        })
    } else {
        None
    };

    // A missing level is not fatal — the game still plays, minus that layer. The pane says
    // so rather than refusing to start.
    let level = match level_path.as_deref() {
        Some(path) => match store.open(&game_dir, path) {
            Ok(_) => store.document(&game_dir, path).cloned(),
            Err(error) => {
                tracing::warn!(path, message = %error.message, "level missing; playing without it");
                None
            }
        },
        None => None,
    };
    // A level played directly (including runtime travel) still keeps Main persistent.
    // This mirrors Main Play's composition instead of dropping game-mode entities merely
    // because the caller named the destination level.
    let persistent_main = if matches!(kind, bhippi_engine::document::SceneKind::Level) {
        manifest.as_ref().and_then(|value| {
            let main_path = value.game.default_scene.as_str();
            if main_path == rel {
                None
            } else {
                store.open(&game_dir, main_path).ok()?;
                store.document(&game_dir, main_path).cloned()
            }
        })
    } else {
        None
    };
    drop(store);

    let world = match kind {
        bhippi_engine::document::SceneKind::Main => {
            bhippi_engine::compose::compose_play(Some(&opened), level.as_ref())
        }
        // Opening the HUD scene alone is an edit mode, not a game: nothing 3D to compose.
        bhippi_engine::document::SceneKind::Hud => bhippi_engine::compose::compose_play(None, None),
        _ => bhippi_engine::compose::compose_play(persistent_main.as_ref(), Some(&opened)),
    }
    .map_err(engine_error)?;

    // The HUD overlay. Read through the session store so unsaved widget edits show up the
    // moment the user presses Play — editing a button and not seeing it is the bug this
    // whole phase exists to remove.
    let hud_rel_path = opened
        .settings
        .hud
        .clone()
        .filter(|path| path.ends_with(".hud.json"))
        .unwrap_or_else(|| hud_path_of(&game_dir));
    let (hud_json, hud_widgets, hud_reference) = if game_dir.join(&hud_rel_path).is_file() {
        let mut huds = huds_lock()?;
        match huds.open(&game_dir, &hud_rel_path) {
            Ok(state) => (Some(state.document_json), state.widgets, state.reference),
            Err(error) => {
                tracing::warn!(path = %hud_rel_path, message = %error.message, "HUD unreadable; playing without it");
                (None, Vec::new(), [1920.0, 1080.0])
            }
        }
    } else {
        (None, Vec::new(), [1920.0, 1080.0])
    };

    // A level played directly is its own level path, so runtime travel and the transport
    // bar both know which map is live.
    let level_path = level_path
        .or_else(|| matches!(kind, bhippi_engine::document::SceneKind::Level).then(|| rel.clone()));

    let (scripts, script_faults) = compile_world_scripts(&game_dir, &world);

    Ok(EnginePlayWorld {
        scene_path: rel,
        level_path,
        hud_path: hud_json.as_ref().map(|_| hud_rel_path),
        document_json: world.dump().map_err(engine_error)?,
        hud_json,
        hud_widgets,
        hud_reference,
        gravity,
        input,
        levels,
        scripts,
        script_faults,
    })
}

/// Replay the validated crash snapshot into the live session. The authored file remains
/// untouched until Save, so recovery itself is reversible by choosing Take disk.
#[tauri::command]
#[specta::specta]
pub async fn engine_recover_scene(
    state: tauri::State<'_, crate::Runtime>,
    scene: String,
) -> Result<EngineSceneState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, Some(&scene))?;
    let mut store = sessions_lock()?;
    store.recover(&game_dir, &rel)
}

/// How much the agent may change without asking (ENG-116). `ask` shows a plan card for
/// every change, `auto` (the default) asks only before deletions, `autonomous` never asks.
#[tauri::command]
#[specta::specta]
pub async fn engine_permission_mode(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<String, AppError> {
    let config = state.config.load().await.map_err(AppError::from)?;
    Ok(match config.engine.permission_mode {
        bhippi_core::EnginePermissionMode::Ask => "ask",
        bhippi_core::EnginePermissionMode::Auto => "auto",
        bhippi_core::EnginePermissionMode::Autonomous => "autonomous",
    }
    .to_owned())
}

#[tauri::command]
#[specta::specta]
pub async fn set_engine_permission_mode(
    state: tauri::State<'_, crate::Runtime>,
    mode: String,
) -> Result<(), AppError> {
    let parsed = match mode.as_str() {
        "ask" => bhippi_core::EnginePermissionMode::Ask,
        "auto" => bhippi_core::EnginePermissionMode::Auto,
        "autonomous" => bhippi_core::EnginePermissionMode::Autonomous,
        other => {
            return Err(AppError {
                message: format!("unknown engine permission mode {other:?}"),
                hint: Some("Use ask, auto or autonomous.".to_owned()),
            })
        }
    };
    let mut config = state.config.load().await.map_err(AppError::from)?;
    config.engine.permission_mode = parsed;
    state.config.save(&config).await.map_err(AppError::from)
}

/// Run the content gates over the whole game (ENG-128).
///
/// The same checks the build runs, surfaced before someone tries to build: a level named in
/// the manifest that is not on disk, a HUD path that does not resolve, an invented weather
/// id, a component pointing at an asset that was never imported.
#[tauri::command]
#[specta::specta]
pub async fn engine_check_content(
    state: tauri::State<'_, crate::Runtime>,
    release: bool,
) -> Result<bhippi_engine::gates::GateReport, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let manifest = load_manifest(&game_dir)
        .map_err(engine_error)?
        .ok_or_else(|| AppError::plain("This project has no game manifest."))?;

    // Check what the editor is actually holding, unsaved edits included — a gate that only
    // sees the last save would clear a scene the user has since broken.
    let mut scenes: Vec<(String, SceneDocument)> = Vec::new();
    {
        let mut store = sessions_lock()?;
        for rel in list_scene_files(&game_dir) {
            if store.open(&game_dir, &rel).is_ok() {
                if let Some(doc) = store.document(&game_dir, &rel) {
                    scenes.push((rel, doc.clone()));
                }
            }
        }
    }

    let mut report = bhippi_engine::gates::check_project(&game_dir, &manifest, &scenes);
    if let Ok(index) = AssetIndex::scan(&game_dir) {
        report
            .findings
            .extend(bhippi_engine::gates::check_assets(&index, &scenes, release).findings);
    }
    Ok(report)
}

/// The process-wide open-HUD store, the HUD counterpart of [`sessions`].
pub fn hud_sessions() -> &'static Mutex<hud_session::HudSessions> {
    static HUDS: OnceLock<Mutex<hud_session::HudSessions>> = OnceLock::new();
    HUDS.get_or_init(|| Mutex::new(hud_session::HudSessions::new()))
}

fn huds_lock() -> Result<std::sync::MutexGuard<'static, hud_session::HudSessions>, AppError> {
    hud_sessions().lock().map_err(|_| AppError {
        message: "The HUD session store is poisoned.".to_owned(),
        hint: Some("Restart the app; an earlier HUD edit panicked.".to_owned()),
    })
}

/// Where this game's HUD document lives. Prefers the manifest's `hud_scene` when it already
/// points at a `.hud.json`; otherwise the conventional path.
pub fn hud_path_of(game_dir: &Path) -> String {
    load_manifest(game_dir)
        .ok()
        .flatten()
        .and_then(|manifest| manifest.game.hud_scene)
        .filter(|path| path.ends_with(".hud.json"))
        .unwrap_or_else(|| DEFAULT_HUD_PATH.to_owned())
}

/// The conventional HUD location for a game (ENG-139).
pub const DEFAULT_HUD_PATH: &str = "assets/ui/hud_main.hud.json";

/// Open the game's HUD document for editing.
#[tauri::command]
#[specta::specta]
pub async fn hud_open(
    state: tauri::State<'_, crate::Runtime>,
    path: Option<String>,
) -> Result<hud_session::HudState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = hud_rel(&game_dir, path.as_deref())?;
    let mut store = huds_lock()?;
    store.open(&game_dir, &rel)
}

fn hud_rel(game_dir: &Path, path: Option<&str>) -> Result<String, AppError> {
    match path.map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => session::safe_scene_path(game_dir, path).map(|(_, rel)| rel),
        None => Ok(hud_path_of(game_dir)),
    }
}

/// Apply one HUD edit. Both the Details panel and the AI come through here.
#[tauri::command]
#[specta::specta]
pub async fn hud_apply(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    action_json: String,
    path: Option<String>,
) -> Result<hud_session::HudState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = hud_rel(&game_dir, path.as_deref())?;
    let action: bhippi_engine::hud_action::HudAction =
        serde_json::from_str(&action_json).map_err(|error| AppError {
            message: format!("That HUD action is not valid: {error}"),
            hint: Some(
                "kind must be add_widget, remove_widget, rename_widget, set_prop, set_style, \
                 set_rect, set_bind, set_action, reparent_widget, reorder_widget, set_visible, \
                 set_locked or set_canvas."
                    .to_owned(),
            ),
        })?;
    let next = {
        let mut store = huds_lock()?;
        store.apply(&game_dir, &rel, &action)?
    };
    announce_hud(&app, &next, &action.to_label());
    Ok(next)
}

/// Apply a whole form as one undo step.
#[tauri::command]
#[specta::specta]
pub async fn hud_apply_many(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    actions_json: String,
    label: String,
    path: Option<String>,
) -> Result<hud_session::HudState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = hud_rel(&game_dir, path.as_deref())?;
    let actions: Vec<bhippi_engine::hud_action::HudAction> = serde_json::from_str(&actions_json)
        .map_err(|error| AppError {
            message: format!("Those HUD actions are not valid: {error}"),
            hint: Some("Pass a JSON array of HUD actions.".to_owned()),
        })?;
    let next = {
        let mut store = huds_lock()?;
        store.apply_many(&game_dir, &rel, &actions, &label)?
    };
    announce_hud(&app, &next, &label);
    Ok(next)
}

macro_rules! hud_command {
    ($name:ident, $method:ident, $label:literal) => {
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            app: tauri::AppHandle,
            state: tauri::State<'_, crate::Runtime>,
            path: Option<String>,
        ) -> Result<hud_session::HudState, AppError> {
            let project_root = required_project_path(&state).await?;
            let game_dir = game_dir_of(&project_root)?;
            let rel = hud_rel(&game_dir, path.as_deref())?;
            let next = {
                let mut store = huds_lock()?;
                store.$method(&game_dir, &rel)?
            };
            announce_hud(&app, &next, $label);
            Ok(next)
        }
    };
}

hud_command!(hud_undo, undo, "undo");
hud_command!(hud_redo, redo, "redo");
hud_command!(hud_save, save, "save");
hud_command!(hud_reload, reload, "reload");

#[tauri::command]
#[specta::specta]
pub async fn hud_select(
    state: tauri::State<'_, crate::Runtime>,
    widget: Option<String>,
    path: Option<String>,
) -> Result<hud_session::HudState, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = hud_rel(&game_dir, path.as_deref())?;
    let mut store = huds_lock()?;
    store.select(&game_dir, &rel, widget)
}

/// The widget catalog the Add menu and the Details panel render from.
#[tauri::command]
#[specta::specta]
pub async fn hud_widget_catalog() -> Result<Vec<hud_session::HudWidgetKindView>, AppError> {
    Ok(hud_session::widget_catalog())
}

/// One HUD edit, broadcast so a second pane (or the chat dock) sees it land.
#[derive(Clone, Debug, Deserialize, Serialize, Type, Event)]
pub struct HudChanged {
    pub path: String,
    pub label: String,
    pub revision: u32,
    pub dirty: bool,
}

fn announce_hud(app: &tauri::AppHandle, state: &hud_session::HudState, label: &str) {
    let _ignored = HudChanged {
        path: state.path.clone(),
        label: label.to_owned(),
        revision: state.revision,
        dirty: state.dirty,
    }
    .emit(app);
}

/// One editable field of a component, as the Details panel renders it (ENG-142).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineFieldView {
    pub name: String,
    /// `f32` · `vec3` · `vec4` · `i32` · `bool` · `enum` · `string` · `asset` · `color` · `json`
    pub kind: String,
    pub doc: String,
    pub min: Option<f32>,
    pub max: Option<f32>,
    /// Present for `enum`.
    pub options: Vec<String>,
    /// Present for `asset` — which kind of asset the picker should offer.
    pub asset_kind: Option<String>,
    /// Registry-owned value written by Reset.
    pub default_value: serde_json::Value,
}

/// One component and its fields.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineComponentView {
    pub name: String,
    pub doc: String,
    /// The Details panel groups by this: Transform, Rendering, Physics, Audio, Gameplay,
    /// Scripting, Editor.
    pub category: String,
    pub fields: Vec<EngineFieldView>,
}

/// Which accordion a component belongs in. Grouping is presentation, so it lives here
/// rather than in the engine's registry — the registry is about what a component *is*.
fn category_of(name: &str) -> &'static str {
    match name {
        "Transform" => "Transform",
        "MeshRenderer"
        | "SkinnedMeshRenderer"
        | "MaterialOverride"
        | "ShaderRef"
        | "Light"
        | "Camera"
        | "ParticleEmitter"
        | "WeatherVolume" => "Rendering",
        "RigidBody" | "Collider" | "CharacterController" | "NavAgent" => "Physics",
        "AudioSource" | "AudioListener" => "Audio",
        "AnimationPlayer" | "UiDocument" | "Tag" => "Gameplay",
        "ScriptRef" => "Scripting",
        _ => "Editor",
    }
}

/// The component registry, as the Details panel and the Add Component menu render it.
///
/// One source of truth: the same `schema::registry()` the validator uses, so a field the
/// panel offers is a field the engine will accept, and a component the menu lists is one
/// that exists (ENG-142).
#[tauri::command]
#[specta::specta]
pub async fn engine_component_schema() -> Result<Vec<EngineComponentView>, AppError> {
    use bhippi_engine::schema::FieldKind;
    Ok(bhippi_engine::schema::registry()
        .into_iter()
        .map(|component| EngineComponentView {
            name: component.name.to_owned(),
            doc: component.doc.to_owned(),
            category: category_of(component.name).to_owned(),
            fields: component
                .fields
                .iter()
                .map(|field| {
                    let (kind, min, max, options, asset_kind) = match field.kind {
                        FieldKind::F32 { min, max } => ("f32", min, max, Vec::new(), None),
                        FieldKind::Unbounded => ("f32", None, None, Vec::new(), None),
                        FieldKind::Vec3 { min, max } => ("vec3", min, max, Vec::new(), None),
                        FieldKind::Vec4 => ("vec4", None, None, Vec::new(), None),
                        FieldKind::I32 => ("i32", None, None, Vec::new(), None),
                        FieldKind::Bool => ("bool", None, None, Vec::new(), None),
                        FieldKind::Enum(values) => (
                            "enum",
                            None,
                            None,
                            values.iter().map(|value| (*value).to_owned()).collect(),
                            None,
                        ),
                        FieldKind::String => ("string", None, None, Vec::new(), None),
                        FieldKind::AssetRef(kind) => {
                            ("asset", None, None, Vec::new(), Some(kind.to_string()))
                        }
                        FieldKind::Color => ("color", None, None, Vec::new(), None),
                        FieldKind::Json => ("json", None, None, Vec::new(), None),
                    };
                    EngineFieldView {
                        name: field.name.to_owned(),
                        kind: kind.to_owned(),
                        doc: field.doc.to_owned(),
                        min,
                        max,
                        options,
                        asset_kind,
                        default_value: bhippi_engine::schema::field_default(component.name, field),
                    }
                })
                .collect(),
        })
        .collect())
}

/// Every asset in the project, for the Content Browser and the asset pickers (ENG-143).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineAssetView {
    pub id: String,
    pub path: String,
    pub name: String,
    pub kind: String,
    pub license: String,
    pub size_bytes: u64,
}

#[tauri::command]
#[specta::specta]
pub async fn engine_list_assets(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<EngineAssetView>, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let index = AssetIndex::scan(&game_dir).map_err(engine_error)?;
    Ok(index
        .assets
        .iter()
        .map(|(id, record)| EngineAssetView {
            id: id.to_string(),
            path: record.path_rel.clone(),
            name: record
                .path_rel
                .rsplit('/')
                .next()
                .unwrap_or(&record.path_rel)
                .to_owned(),
            kind: record.kind.to_string(),
            license: record.license.to_string(),
            size_bytes: record.size_bytes,
        })
        .collect())
}

/// One fully-resolved material, ready for the renderer to build (ENG-162).
///
/// Everything a draw call needs, already looked up: the `.mat.json` parsed, `asset:` texture
/// references resolved to files on disk, defaults filled in. The webview builds a material
/// from this and makes no decisions — which is what stops "what the AI generated" and "what
/// the user sees" from being two different things (F8).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RenderMaterial {
    /// The reference entities point at (a `.mat.json` path, or `asset:<ulid>`).
    pub key: String,
    pub name: String,
    pub base_color: [f32; 3],
    pub roughness: f32,
    pub metallic: f32,
    pub emissive: [f32; 3],
    pub emissive_strength: f32,
    pub normal_strength: f32,
    pub tiling: [f32; 2],
    pub offset: [f32; 2],
    /// `opaque` · `mask` · `blend`
    pub alpha_mode: String,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
    /// Absolute paths, for the webview to turn into asset URLs. Absent maps are `null`.
    pub albedo: Option<String>,
    pub normal: Option<String>,
    pub roughness_map: Option<String>,
    pub metallic_map: Option<String>,
    pub ao: Option<String>,
    pub emissive_map: Option<String>,
}

/// One mesh the scene references, resolved to something loadable.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RenderMesh {
    pub key: String,
    /// `builtin` · `file` · `missing`
    pub source: String,
    /// For `builtin`: the primitive name. For `file`: the absolute path.
    pub value: String,
}

/// Everything the viewport needs to draw the open scene truthfully.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RenderManifest {
    pub scene_path: String,
    pub materials: Vec<RenderMaterial>,
    pub meshes: Vec<RenderMesh>,
    /// References the scene points at that do not resolve — the viewport draws a marked
    /// placeholder for these rather than a plausible grey box that hides the problem.
    pub missing: Vec<String>,
}

fn resolve_texture(game_dir: &Path, index: &AssetIndex, reference: &str) -> Option<String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return None;
    }
    let rel = match reference.strip_prefix("asset:") {
        Some(id) => bhippi_types::AssetId::from_str(id)
            .ok()
            .and_then(|id| index.get(id).map(|record| record.path_rel.clone()))?,
        None => reference.to_owned(),
    };
    let path = game_dir.join(&rel);
    path.is_file()
        .then(|| path.to_string_lossy().replace('\\', "/"))
}

fn material_from_document(
    game_dir: &Path,
    index: &AssetIndex,
    key: &str,
    doc: &bhippi_engine::material::MaterialDocument,
) -> RenderMaterial {
    let map = |slot: &str| {
        doc.maps
            .get(slot)
            .and_then(|value| value.as_deref())
            .and_then(|reference| resolve_texture(game_dir, index, reference))
    };
    RenderMaterial {
        key: key.to_owned(),
        name: doc.name.clone(),
        base_color: doc.params.base_color,
        roughness: doc.params.roughness,
        metallic: doc.params.metallic,
        emissive: doc.params.emissive,
        emissive_strength: doc.params.emissive_strength,
        normal_strength: doc.params.normal_strength,
        tiling: doc.params.tiling,
        offset: doc.params.offset,
        alpha_mode: match doc.params.alpha_mode {
            bhippi_engine::material::AlphaMode::Opaque => "opaque",
            bhippi_engine::material::AlphaMode::Mask => "mask",
            bhippi_engine::material::AlphaMode::Blend => "blend",
        }
        .to_owned(),
        alpha_cutoff: doc.params.alpha_cutoff,
        double_sided: doc.params.double_sided,
        albedo: map("albedo"),
        normal: map("normal"),
        roughness_map: map("roughness"),
        metallic_map: map("metallic"),
        ao: map("ao"),
        emissive_map: map("emissive"),
    }
}

/// Resolve every mesh and material the open scene references (ENG-160/162).
#[tauri::command]
#[specta::specta]
pub async fn engine_render_manifest(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
) -> Result<RenderManifest, AppError> {
    let project_root = required_project_path(&state).await?;
    let game_dir = game_dir_of(&project_root)?;
    let rel = scene_rel_of(&game_dir, scene.as_deref())?;
    let doc = {
        let mut store = sessions_lock()?;
        store.open(&game_dir, &rel)?;
        store
            .document(&game_dir, &rel)
            .ok_or_else(|| AppError::plain("The scene could not be opened."))?
            .clone()
    };
    let index = AssetIndex::scan(&game_dir).unwrap_or_default();

    let mut mesh_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut material_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entity in &doc.entities {
        for component in ["MeshRenderer", "SkinnedMeshRenderer"] {
            let Some(payload) = entity.components.get(component) else {
                continue;
            };
            if let Some(mesh) = payload.get("mesh").and_then(Value::as_str) {
                if !mesh.is_empty() {
                    mesh_keys.insert(mesh.to_owned());
                }
            }
            if let Some(list) = payload.get("materials").and_then(Value::as_array) {
                for item in list.iter().filter_map(Value::as_str) {
                    if !item.is_empty() {
                        material_keys.insert(item.to_owned());
                    }
                }
            }
        }
    }

    let mut missing = Vec::new();
    let meshes = mesh_keys
        .into_iter()
        .map(|key| {
            if let Some(builtin) = bhippi_engine::mesh::builtin_from_reference(&key) {
                return RenderMesh {
                    key,
                    source: "builtin".to_owned(),
                    value: builtin.as_str().to_owned(),
                };
            }
            match resolve_texture(&game_dir, &index, &key) {
                Some(path) => RenderMesh {
                    key,
                    source: "file".to_owned(),
                    value: path,
                },
                None => {
                    missing.push(key.clone());
                    RenderMesh {
                        key,
                        source: "missing".to_owned(),
                        value: String::new(),
                    }
                }
            }
        })
        .collect();

    let mut materials = Vec::new();
    for key in material_keys {
        // A material reference is either a `.mat.json` path or an `asset:` id pointing at
        // one. Anything else is a dangling reference, which the gates already block on — the
        // viewport just needs to know not to draw it as if it were fine.
        let Some(path) = resolve_texture(&game_dir, &index, &key) else {
            missing.push(key);
            continue;
        };
        match std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| bhippi_engine::material::MaterialDocument::parse(&text).ok())
        {
            Some(doc) => materials.push(material_from_document(&game_dir, &index, &key, &doc)),
            None => missing.push(key),
        }
    }

    Ok(RenderManifest {
        scene_path: rel,
        materials,
        meshes,
        missing,
    })
}

/// The weather presets, straight from the engine registry — the picker renders this
/// instead of keeping its own copy of the numbers (ENG-105).
#[tauri::command]
#[specta::specta]
pub async fn engine_weather_presets() -> Result<Vec<bhippi_engine::weather::WeatherPreset>, AppError>
{
    Ok(bhippi_engine::weather::presets())
}

/// The spawn palette, straight from the engine scaffold — the Add menu renders this
/// instead of building entities in TypeScript (ENG-105, INV-073).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineTemplateView {
    pub name: String,
    pub label: String,
    pub kind: String,
}

#[tauri::command]
#[specta::specta]
pub async fn engine_templates() -> Result<Vec<EngineTemplateView>, AppError> {
    Ok(scaffold::templates()
        .into_iter()
        .map(|spec| EngineTemplateView {
            name: spec.name,
            label: spec.label,
            kind: match spec.kind {
                scaffold::TemplateKind::Visual => "visual",
                scaffold::TemplateKind::Camera => "camera",
                scaffold::TemplateKind::Gameplay => "gameplay",
            }
            .to_owned(),
        })
        .collect())
}

/// Load the scene document and its asset index for the SEC 7.4 query surface (ADR-0027).
/// The asset index is scanned from the game folder so asset-backed queries can resolve
/// record metadata the same way the World Brain does.
fn load_query_inputs(
    workspace: &str,
    scene_rel: Option<&str>,
) -> Result<(SceneDocument, AssetIndex), AppError> {
    let root = PathBuf::from(workspace);
    let game_dir = game_root(&root)?.ok_or_else(|| AppError {
        message: "This project is not a game.".to_owned(),
        hint: Some("Create a game from the Engine pane first.".to_owned()),
    })?;
    let scene_path = resolve_scene_path(&game_dir, scene_rel)?;
    let doc = load_scene_file(&scene_path)?;
    let index = AssetIndex::scan(&game_dir).map_err(engine_error)?;
    Ok((doc, index))
}

/// Pick the query facade at the requested expansion, borrowing the loaded scene + index.
fn pick_queries<'a>(doc: &'a SceneDocument, index: &'a AssetIndex, deep: bool) -> SceneQueries<'a> {
    let queries = SceneQueries::with_assets(doc, index);
    if deep {
        queries.deep()
    } else {
        queries.compact()
    }
}

// ── IPC views ────────────────────────────────────────────────────────────────────────────
// The engine library DTOs (bhippi-engine/api.rs) omit `None` fields with
// `#[serde(skip_serializing_if)]`, which specta's unified IPC mode cannot represent. These
// mirrors repeat the same shapes with plain `Option` fields so the query surface exports to
// the webview unchanged in meaning (nullable, not omitted). They are thin conversions over
// the engine DTOs and hold no business logic (INV-032 / INV-073).

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQuerySceneView {
    pub id: bhippi_types::SceneId,
    pub name: String,
    pub kind: bhippi_engine::document::SceneKind,
    pub entity_count: u32,
    pub root_count: u32,
    pub settings: Option<bhippi_engine::document::SceneSettings>,
    pub hierarchy: Option<Vec<bhippi_engine::query::HierarchyEntry>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryEntityView {
    pub id: EntityId,
    pub name: String,
    pub parent: Option<EntityId>,
    pub tags: Vec<String>,
    pub stable_path: String,
    pub component_names: Vec<String>,
    pub components: Option<BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryEntityRef {
    pub id: EntityId,
    pub name: String,
    pub parent: Option<EntityId>,
    pub stable_path: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryComponentsView {
    pub entity: EntityId,
    pub names: Vec<String>,
    pub payloads: Option<BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryChildrenView {
    pub entity: EntityId,
    pub ids: Vec<EntityId>,
    pub entries: Option<Vec<EngineQueryEntityRef>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryParentView {
    pub entity: EntityId,
    pub parent: Option<EngineQueryEntityRef>,
    pub parent_components: Option<BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryScriptsView {
    pub entity: EntityId,
    pub script: Option<String>,
    pub hooks: Option<Value>,
    pub config: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryAssetUser {
    pub entity: EntityId,
    pub name: String,
    pub stable_path: String,
    pub components: Vec<String>,
    pub payloads: Option<BTreeMap<String, Value>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryAssetUsersView {
    pub asset: AssetId,
    pub record: Option<bhippi_engine::asset::AssetRecord>,
    pub users: Vec<EngineQueryAssetUser>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryAssetDependency {
    pub asset: AssetId,
    pub record: Option<bhippi_engine::asset::AssetRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryAssetDependenciesView {
    pub asset: AssetId,
    pub record: Option<bhippi_engine::asset::AssetRecord>,
    pub dependencies: Vec<EngineQueryAssetDependency>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryMaterialGraphView {
    pub material: AssetId,
    pub record: Option<bhippi_engine::asset::AssetRecord>,
    pub users: Vec<EngineQueryAssetUser>,
    pub textures: Vec<AssetId>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryShaderView {
    pub shader: AssetId,
    pub record: Option<bhippi_engine::asset::AssetRecord>,
    pub users: Vec<EngineQueryAssetUser>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryAnimationGraphView {
    pub entity: EntityId,
    pub clip: Option<AssetId>,
    pub clip_record: Option<bhippi_engine::asset::AssetRecord>,
    pub mesh: Option<AssetId>,
    pub co_referenced: Vec<AssetId>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineQueryPhysicsView {
    pub entity: EntityId,
    pub body_kind: Option<String>,
    pub mass: Option<f64>,
    pub lock_rotation: Option<bool>,
    pub collider_shape: Option<String>,
    pub sensor: Option<bool>,
    pub has_character_controller: bool,
    pub extras: Option<BTreeMap<String, Value>>,
}

// ── conversions ──────────────────────────────────────────────────────────────────────────

impl From<bhippi_engine::SceneView> for EngineQuerySceneView {
    fn from(view: bhippi_engine::SceneView) -> Self {
        Self {
            id: view.id,
            name: view.name,
            kind: view.kind,
            entity_count: view.entity_count as u32,
            root_count: view.root_count as u32,
            settings: view.settings,
            hierarchy: view.hierarchy,
        }
    }
}

impl From<bhippi_engine::EntityView> for EngineQueryEntityView {
    fn from(view: bhippi_engine::EntityView) -> Self {
        Self {
            id: view.id,
            name: view.name,
            parent: view.parent,
            tags: view.tags,
            stable_path: view.stable_path,
            component_names: view.component_names,
            components: view.components,
        }
    }
}

impl From<bhippi_engine::EntityRef> for EngineQueryEntityRef {
    fn from(view: bhippi_engine::EntityRef) -> Self {
        Self {
            id: view.id,
            name: view.name,
            parent: view.parent,
            stable_path: view.stable_path,
        }
    }
}

impl From<bhippi_engine::ComponentsView> for EngineQueryComponentsView {
    fn from(view: bhippi_engine::ComponentsView) -> Self {
        Self {
            entity: view.entity,
            names: view.names,
            payloads: view.payloads,
        }
    }
}

impl From<bhippi_engine::ChildrenView> for EngineQueryChildrenView {
    fn from(view: bhippi_engine::ChildrenView) -> Self {
        Self {
            entity: view.entity,
            ids: view.ids,
            entries: view.entries.map(|entries| {
                entries
                    .into_iter()
                    .map(EngineQueryEntityRef::from)
                    .collect()
            }),
        }
    }
}

impl From<bhippi_engine::ParentView> for EngineQueryParentView {
    fn from(view: bhippi_engine::ParentView) -> Self {
        Self {
            entity: view.entity,
            parent: view.parent.map(EngineQueryEntityRef::from),
            parent_components: view.parent_components,
        }
    }
}

impl From<bhippi_engine::ScriptsView> for EngineQueryScriptsView {
    fn from(view: bhippi_engine::ScriptsView) -> Self {
        Self {
            entity: view.entity,
            script: view.script,
            hooks: view.hooks,
            config: view.config,
        }
    }
}

impl From<bhippi_engine::AssetUser> for EngineQueryAssetUser {
    fn from(view: bhippi_engine::AssetUser) -> Self {
        Self {
            entity: view.entity,
            name: view.name,
            stable_path: view.stable_path,
            components: view.components,
            payloads: view.payloads,
        }
    }
}

impl From<bhippi_engine::AssetUsersView> for EngineQueryAssetUsersView {
    fn from(view: bhippi_engine::AssetUsersView) -> Self {
        Self {
            asset: view.asset,
            record: view.record,
            users: view
                .users
                .into_iter()
                .map(EngineQueryAssetUser::from)
                .collect(),
        }
    }
}

impl From<bhippi_engine::AssetDependency> for EngineQueryAssetDependency {
    fn from(view: bhippi_engine::AssetDependency) -> Self {
        Self {
            asset: view.asset,
            record: view.record,
        }
    }
}

impl From<bhippi_engine::AssetDependenciesView> for EngineQueryAssetDependenciesView {
    fn from(view: bhippi_engine::AssetDependenciesView) -> Self {
        Self {
            asset: view.asset,
            record: view.record,
            dependencies: view
                .dependencies
                .into_iter()
                .map(EngineQueryAssetDependency::from)
                .collect(),
        }
    }
}

impl From<bhippi_engine::MaterialGraphView> for EngineQueryMaterialGraphView {
    fn from(view: bhippi_engine::MaterialGraphView) -> Self {
        Self {
            material: view.material,
            record: view.record,
            users: view
                .users
                .into_iter()
                .map(EngineQueryAssetUser::from)
                .collect(),
            textures: view.textures,
        }
    }
}

impl From<bhippi_engine::ShaderView> for EngineQueryShaderView {
    fn from(view: bhippi_engine::ShaderView) -> Self {
        Self {
            shader: view.shader,
            record: view.record,
            users: view
                .users
                .into_iter()
                .map(EngineQueryAssetUser::from)
                .collect(),
        }
    }
}

impl From<bhippi_engine::AnimationGraphView> for EngineQueryAnimationGraphView {
    fn from(view: bhippi_engine::AnimationGraphView) -> Self {
        Self {
            entity: view.entity,
            clip: view.clip,
            clip_record: view.clip_record,
            mesh: view.mesh,
            co_referenced: view.co_referenced,
        }
    }
}

impl From<bhippi_engine::PhysicsView> for EngineQueryPhysicsView {
    fn from(view: bhippi_engine::PhysicsView) -> Self {
        Self {
            entity: view.entity,
            body_kind: view.body_kind,
            mass: view.mass,
            lock_rotation: view.lock_rotation,
            collider_shape: view.collider_shape,
            sensor: view.sensor,
            has_character_controller: view.has_character_controller,
            extras: view.extras,
        }
    }
}

// ── commands ─────────────────────────────────────────────────────────────────────────────

/// `scene.get(id)` — summary of a scene via the SEC 7.4 engine query API.
#[tauri::command]
#[specta::specta]
pub async fn engine_query_scene_view(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    deep: bool,
) -> Result<EngineQuerySceneView, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep).get().into())
}

/// `scene.get_entity(id)` — one entity projection (compact or deep).
#[tauri::command]
#[specta::specta]
pub async fn engine_query_entity(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    entity_id: EntityId,
    deep: bool,
) -> Result<Option<EngineQueryEntityView>, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_entity(entity_id)
        .map(EngineQueryEntityView::from))
}

/// `scene.find_entities(query)` — entities matching a filter.
#[tauri::command]
#[specta::specta]
pub async fn engine_query_find_entities(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    filter: EntityQuery,
    deep: bool,
) -> Result<Vec<EngineQueryEntityRef>, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .find_entities(&filter)
        .into_iter()
        .map(EngineQueryEntityRef::from)
        .collect())
}

/// `scene.get_components(id)` — component names, plus payloads in deep mode.
#[tauri::command]
#[specta::specta]
pub async fn engine_query_components(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    entity_id: EntityId,
    deep: bool,
) -> Result<Option<EngineQueryComponentsView>, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_components(entity_id)
        .map(EngineQueryComponentsView::from))
}

/// `scene.get_children(id)` — immediate children (entries in deep mode).
#[tauri::command]
#[specta::specta]
pub async fn engine_query_children(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    entity_id: EntityId,
    deep: bool,
) -> Result<Option<EngineQueryChildrenView>, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_children(entity_id)
        .map(EngineQueryChildrenView::from))
}

/// `scene.get_parent(id)` — the parent (with component payloads in deep mode).
#[tauri::command]
#[specta::specta]
pub async fn engine_query_parent(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    entity_id: EntityId,
    deep: bool,
) -> Result<Option<EngineQueryParentView>, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_parent(entity_id)
        .map(EngineQueryParentView::from))
}

/// `scene.get_scripts(id)` — the entity's `ScriptRef` binding.
#[tauri::command]
#[specta::specta]
pub async fn engine_query_scripts(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    entity_id: EntityId,
    deep: bool,
) -> Result<Option<EngineQueryScriptsView>, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_scripts(entity_id)
        .map(EngineQueryScriptsView::from))
}

/// `scene.get_asset_users(asset_id)` — entities whose components reference the asset.
#[tauri::command]
#[specta::specta]
pub async fn engine_query_asset_users(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    asset_id: AssetId,
    deep: bool,
) -> Result<EngineQueryAssetUsersView, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_asset_users(asset_id)
        .into())
}

/// `scene.get_asset_dependencies(asset_id)` — assets that ship alongside this one.
#[tauri::command]
#[specta::specta]
pub async fn engine_query_asset_dependencies(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    asset_id: AssetId,
    deep: bool,
) -> Result<EngineQueryAssetDependenciesView, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_asset_dependencies(asset_id)
        .into())
}

/// `scene.get_material_graph(material_id)` — material users + co-shipped textures.
#[tauri::command]
#[specta::specta]
pub async fn engine_query_material_graph(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    material_id: AssetId,
    deep: bool,
) -> Result<EngineQueryMaterialGraphView, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_material_graph(material_id)
        .into())
}

/// `scene.get_shader(shader_id)` — shader users.
#[tauri::command]
#[specta::specta]
pub async fn engine_query_shader(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    shader_id: AssetId,
    deep: bool,
) -> Result<EngineQueryShaderView, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_shader(shader_id)
        .into())
}

/// `scene.get_animation_graph(id)` — the entity's animation clip + mesh + co-references.
#[tauri::command]
#[specta::specta]
pub async fn engine_query_animation_graph(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    entity_id: EntityId,
    deep: bool,
) -> Result<Option<EngineQueryAnimationGraphView>, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_animation_graph(entity_id)
        .map(EngineQueryAnimationGraphView::from))
}

/// `scene.get_physics(id)` — the physics projection (ADR-0026).
#[tauri::command]
#[specta::specta]
pub async fn engine_query_physics(
    state: tauri::State<'_, crate::Runtime>,
    scene: Option<String>,
    entity_id: EntityId,
    deep: bool,
) -> Result<Option<EngineQueryPhysicsView>, AppError> {
    let project_root = required_project_path(&state).await?;
    let (doc, index) = load_query_inputs(&project_root, scene.as_deref())?;
    Ok(pick_queries(&doc, &index, deep)
        .get_physics(entity_id)
        .map(EngineQueryPhysicsView::from))
}
