//! Godot studio journal database integration and project path resolution (ADR-0043, GAD-103).
//!
//! With the webview engine retired, this module preserves only what `godot_commands`,
//! `godot_bridge`, and `godot_versions` need: the single-writer journal database registration,
//! transaction recording, and recent journal queries.

use crate::AppError;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// The facts needed to journal a transaction into `engine_journal`.
#[derive(Clone, Debug)]
pub struct JournalFacts {
    pub scene_rel_path: String,
    pub txn_id: String,
    pub actor: String,
    pub label: String,
    pub ops_json: String,
    pub inverse_json: String,
    pub touched_json: String,
    pub op_count: i64,
}

pub mod session {
    pub use super::JournalFacts;
}

static JOURNAL_DB: OnceLock<bhippi_db::Database> = OnceLock::new();

/// Called during app setup so the Godot studio commands can journal applied batches.
pub fn register_journal_db(database: bhippi_db::Database) {
    let _ignored = JOURNAL_DB.set(database);
}

fn registered_projects() -> &'static Mutex<BTreeSet<String>> {
    static REGISTERED: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    REGISTERED.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// Write one applied transaction into `engine_journal`.
pub async fn journal_edit(game_dir: &Path, facts: &JournalFacts) -> Option<i64> {
    let database = JOURNAL_DB.get()?;
    let project_path = game_dir.to_string_lossy().replace('\\', "/");
    let now = chrono::Utc::now();

    let needs_register = registered_projects()
        .lock()
        .map(|seen| !seen.contains(&project_path))
        .unwrap_or(true);
    if needs_register {
        let manifest = bhippi_engine::manifest::load_manifest(game_dir)
            .ok()
            .flatten();
        let record = bhippi_db::EngineProjectRecord {
            project_path: project_path.clone(),
            game_id: manifest
                .as_ref()
                .map(|m| m.game.id.to_string())
                .unwrap_or_default(),
            game_name: manifest
                .as_ref()
                .map(|m| m.game.name.clone())
                .unwrap_or_else(|| "Untitled".to_owned()),
            version: manifest
                .as_ref()
                .map(|m| m.game.version.clone())
                .unwrap_or_else(|| "0.0.0".to_owned()),
            default_scene: manifest
                .as_ref()
                .map(|m| m.game.default_scene.clone())
                .unwrap_or_default(),
            engine_track: manifest
                .as_ref()
                .map(|m| match m.game.engine_track {
                    bhippi_engine::EngineTrack::Rust => "rust",
                    bhippi_engine::EngineTrack::Scripted => "scripted",
                })
                .unwrap_or("rust")
                .to_owned(),
            targets_json: manifest
                .as_ref()
                .map(|m| serde_json::to_string(&m.enabled_targets()).unwrap_or_default())
                .unwrap_or_else(|| "[]".to_owned()),
            scene_count: 0,
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
            tracing::warn!(%error, "transaction applied but not journaled");
            None
        }
    }
}

/// Retrieve the most recent journal records for a game project.
pub async fn recent_journal(game_dir: &Path, limit: u32) -> Vec<bhippi_db::JournalRecord> {
    let Some(database) = JOURNAL_DB.get() else {
        return Vec::new();
    };
    let project_path = game_dir.to_string_lossy().replace('\\', "/");
    database
        .engine()
        .list(&project_path, None, limit as i64)
        .await
        .unwrap_or_default()
}

/// Resolves a workspace path to a recognized game project directory.
pub fn game_dir_of(workspace: &str) -> Result<PathBuf, AppError> {
    let root = PathBuf::from(workspace.trim());
    if !root.exists() {
        return Err(AppError {
            message: format!("Workspace `{}` does not exist.", root.display()),
            hint: Some("Open a valid folder in the workspace switcher.".to_owned()),
        });
    }
    if root.join("project.godot").is_file()
        || root.join(bhippi_engine::GAME_MANIFEST_FILE).is_file()
    {
        return Ok(root);
    }
    Err(AppError {
        message: format!(
            "`{}` is not a recognised Bhippi game project.",
            root.display()
        ),
        hint: Some(
            "Create a project or select an existing one with project.godot or Bhippi.game.toml."
                .to_owned(),
        ),
    })
}
