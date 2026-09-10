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

/// The database that is actually registered, which is not necessarily the one you just
/// handed to `register_journal_db` — it is a `OnceLock`, so the first caller wins and every
/// later one is silently ignored. Anything that registers a database *and then wants to read
/// what was written through it* must come back through here, or it will query a handle
/// nothing is writing to.
#[must_use]
pub fn journal_db() -> Option<&'static bhippi_db::Database> {
    JOURNAL_DB.get()
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

/// The largest file whose pre-write text is kept for a review.
///
/// A generated 40 MB asset is still one changed file; holding a copy of it so the panel can
/// print a line count would cost far more than the number is worth. Over this, the file is
/// still recorded — it changed, and saying so matters — but with no text to diff against.
const REVIEW_BASELINE_CAP: usize = 4 * 1024 * 1024;

/// Record what a file held before Bhippi first touched it (the review ledger).
///
/// Called from every write path, before the write. The ledger is what lets Review Changes
/// answer "what did this session change?" in a project that is not a git repository — which
/// is most games Bhippi builds. First touch wins, so calling this on every write is correct
/// and cheap: the second call for a file is a no-op insert.
///
/// A failure here never fails a write. The file is what matters; the ledger row is how we
/// describe it afterwards, and a review that under-reports is better than a refused edit.
pub async fn record_review_baseline(
    project_dir: &Path,
    file_path: &Path,
    rel_path: &str,
    previous: Option<&str>,
) {
    let Some(database) = JOURNAL_DB.get() else {
        return;
    };
    let baseline = bhippi_db::NewReviewBaseline {
        file_path: file_path.to_string_lossy().replace('\\', "/"),
        project_path: project_dir.to_string_lossy().replace('\\', "/"),
        rel_path: rel_path.replace('\\', "/"),
        existed: previous.is_some(),
        before_text: previous
            .filter(|text| text.len() <= REVIEW_BASELINE_CAP)
            .map(str::to_owned),
    };
    if let Err(error) = database
        .review()
        .record(&baseline, &chrono::Utc::now())
        .await
    {
        tracing::warn!(%error, path = %rel_path, "file written but its review baseline was not recorded");
    }
}

/// Every review baseline recorded for a workspace.
pub async fn review_baselines(project_dir: &Path) -> Vec<bhippi_db::ReviewBaseline> {
    let Some(database) = JOURNAL_DB.get() else {
        return Vec::new();
    };
    let project_path = project_dir.to_string_lossy().replace('\\', "/");
    database
        .review()
        .list(&project_path)
        .await
        .unwrap_or_default()
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
