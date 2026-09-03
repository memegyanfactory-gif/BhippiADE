//! Versions, game settings and publish (GAD-083, GAD-022/023, GAD-092/094).
//!
//! Three surfaces, one idea: **the journal is the history**, and everything here is a
//! projection of it or a write that goes back through it.
//!
//! * A **version** is a label plus the journal revision the project stood at. Reverting is
//!   a replay — the rows above that revision, inverted, applied newest-first as *one* new
//!   user transaction — so the revert appears in the journal like any other change and
//!   undoing it puts the newer work back. Nothing is ever restored from a copy on disk,
//!   because a copy goes stale the moment somebody opens the folder in Godot's own editor.
//! * **Game settings** are the manifest's `[game]` presentation fields and `[publish]`.
//!   They are validated in `bhippi-engine` and written through `render_manifest`, so the
//!   form and a hand-edited TOML file are held to exactly the same rules.
//! * **Publish** is the release gates, Godot's own web export, a credits page rendered from
//!   the asset sidecars, and a version recording where the artefact landed.
//!
//! Every rule in here is Rust's: which version is newest, which rows a revert replays,
//! whether a title is acceptable, what the credits say and which frame becomes the poster
//! (INV-073). The webview renders the answers.

use crate::commands::AppError;
use crate::godot_commands::{
    announce_process, apply_and_journal, claim_slot, display_of, engine_error, game_name, lock,
    release_slot, require_install, resolve_project, start_output_pump, ExportResult,
    GodotApplyHost, GodotRunKind, GodotRunState, GodotSessionStore,
};
use crate::godot_observe::{VisualPlaytestPlan, VisualStep};
use base64::Engine as _;
use bhippi_engine::godot::action::{GodotActionOutcome, GodotChangeSet, GodotFileChange};
use bhippi_engine::godot::credits::{collect_credits, render_credits_html, CREDITS_FILE};
use bhippi_engine::godot::export_presets::PresetTarget;
use bhippi_engine::godot::manifest::render_manifest;
use bhippi_engine::godot::versions::{
    check_label, load_versions, save_versions, GameVersion, VersionExport, MAX_VERSIONS,
};
use bhippi_engine::manifest::{
    load_manifest, manifest_path, validate_game_settings, validate_publish, GameManifest,
    GameSection, PublishSection,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

// ── limits ───────────────────────────────────────────────────────────────────────────

/// The most journal rows one revert replays. A version further back than this is refused
/// with the number rather than half-applied: a revert that silently stopped part-way would
/// leave the project in a state no version describes.
pub const MAX_REVERT_ROWS: usize = 500;
/// How long the poster capture lets the game settle before the shutter (GAD-092). A Godot
/// window is up before its first frame is drawn; a poster taken at zero is a grey rectangle.
pub const POSTER_SETTLE_MS: u64 = 1_500;
/// The whole poster run's budget. It has to cover Godot starting *and* the settle, because
/// the plan's clock starts when the process is spawned, not when the window appears.
pub const POSTER_MAX_MS: u64 = 25_000;
/// Where the poster lands, project-relative.
pub const POSTER_FILE: &str = ".bhippi/poster.png";
/// The most bytes of poster `game_card_info` hands the Games screen. A card is 320 px wide;
/// a 4K screenshot behind it is 8 MB of base64 nobody sees.
pub const MAX_CARD_POSTER_BYTES: usize = 2 * 1024 * 1024;

// ── typed replies ────────────────────────────────────────────────────────────────────

/// The Versions drawer's whole state.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct VersionsView {
    /// Newest first — the order is decided here, not in the webview.
    pub versions: Vec<GameVersion>,
    /// The project's latest journal revision. A version at this revision is the present.
    pub current_revision: i64,
    /// What the last write had to do that the caller did not ask for — dropping the oldest
    /// versions past the cap. `None` most of the time.
    pub notice: Option<String>,
    /// Why Revert is unavailable right now, when it is. `None` means it may run.
    pub revert_blocked: Option<String>,
}

/// The Game tab's form, as it crosses IPC.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GameSettings {
    /// Never empty: falls back to the project's `name` when no title is set.
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    /// Project-relative, forward slashes.
    pub poster: Option<String>,
    pub credits: bool,
    pub web_export_dir: String,
}

impl GameSettings {
    /// The projection of a manifest. The title fallback is the manifest's own rule.
    #[must_use]
    pub fn from_manifest(manifest: &GameManifest) -> Self {
        Self {
            title: manifest.game.display_title(),
            description: manifest.game.description.clone(),
            tags: manifest.game.tags.clone(),
            poster: manifest.game.poster.clone(),
            credits: manifest.publish.credits,
            web_export_dir: manifest.publish.web_export_dir.clone(),
        }
    }

    /// Fold the form back into a manifest, validating exactly as the parser does.
    ///
    /// Both halves run before anything is written: a settings save that wrote the `[game]`
    /// table and then refused the `[publish]` one would leave a file neither the form nor
    /// the person asked for.
    pub fn apply_to(&self, manifest: &mut GameManifest) -> Result<(), AppError> {
        // The form always shows an effective title, so an empty one is a field somebody
        // cleared rather than a request to fall back to the folder's name. Refused here
        // because the manifest itself cannot tell the two apart.
        if self.title.trim().is_empty() {
            return Err(AppError {
                message: "Give the game a title.".to_owned(),
                hint: Some("It is what players and the credits page see.".to_owned()),
            });
        }
        let game = GameSection {
            title: self.title.trim().to_owned(),
            description: self.description.trim().to_owned(),
            tags: self
                .tags
                .iter()
                .map(|tag| tag.trim().to_owned())
                .filter(|tag| !tag.is_empty())
                .collect(),
            poster: self
                .poster
                .as_ref()
                .map(|poster| poster.trim().replace('\\', "/"))
                .filter(|poster| !poster.is_empty()),
            ..manifest.game.clone()
        };
        let publish = PublishSection {
            credits: self.credits,
            web_export_dir: self.web_export_dir.trim().replace('\\', "/"),
        };
        validate_game_settings(&game).map_err(engine_error)?;
        validate_publish(&publish).map_err(engine_error)?;
        manifest.game = game;
        manifest.publish = publish;
        Ok(())
    }
}

/// What a web publish produced.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PublishResult {
    /// Absolute, for Reveal folder.
    pub output_dir: String,
    /// Absolute path of the page a browser opens.
    pub index_html: String,
    /// `None` when `[publish].credits` is off.
    pub credits_html: Option<String>,
    /// The version this publish recorded, when the journal was available.
    pub version_id: Option<String>,
    /// How many assets the credits page names.
    pub credited_assets: u32,
}

/// One Games-screen card, computed entirely inside the project root.
///
/// It exists so the Games screen never does path arithmetic: the workspace file API is
/// scoped to the *open* project, and a TypeScript join of "project path" and
/// `.bhippi/poster.png` would be exactly the business logic INV-073 keeps out of the
/// webview.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GameCardInfo {
    pub project: String,
    pub title: String,
    /// The poster as a `data:` URL the card can render straight into `<img src>`, or
    /// `None` when the game has no poster small enough (or of a type safe enough) to show.
    /// A URL rather than raw base64 because the media type is read from the file and the
    /// webview must not guess it.
    pub poster_data_url: Option<String>,
    /// Whether the folder holds a `project.godot`. The card's status pill reads this
    /// rather than sniffing for the file itself: the webview never does path arithmetic
    /// inside a project (INV-073), and `blocked_reason` cannot answer it — a Godot game
    /// with no engine installed is blocked *and* a Godot project.
    pub is_godot_project: bool,
    pub version_count: u32,
    /// RFC 3339 of the newest export recorded on any version.
    pub last_export: Option<String>,
    /// `None` when Play and Publish may run; otherwise the reason they may not, in words
    /// the card puts straight into a tooltip.
    pub blocked_reason: Option<String>,
}

// ── the revert planner ───────────────────────────────────────────────────────────────

/// One journal row, reduced to what a revert needs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalRow {
    pub revision: i64,
    pub label: String,
    pub inverse_json: String,
}

/// Plan a revert to `since_revision` from the rows above it.
///
/// **The algorithm, and why it is not a concatenation.** Applying each row's inverse in turn
/// would put the files back correctly — but the *inverse of that plan* would not put them
/// forward again, because a file touched by two rows would end up at the older of its two
/// forward states. Since the revert is journaled like any other change, and undoing it has
/// to work, the plan is collapsed **per file** instead: for each path, `before` is the
/// content the newest row left there (which is what is on disk now) and `after` is the
/// content the oldest row in the range found there (which is the state the version names).
/// One change per file, so `invert` on the result re-applies the whole range exactly.
///
/// Rows arrive newest first, which is the order `EngineRepo::list` returns them in.
pub(crate) fn plan_revert(
    rows_newest_first: &[JournalRow],
    since_revision: i64,
    label: &str,
) -> Result<GodotChangeSet, AppError> {
    let in_range: Vec<&JournalRow> = rows_newest_first
        .iter()
        .filter(|row| row.revision > since_revision)
        .collect();

    // The list came back full and still has not reached the version: the rows below the
    // window were never read, so the plan would be missing changes it must undo.
    if rows_newest_first.len() >= MAX_REVERT_ROWS
        && rows_newest_first
            .last()
            .is_some_and(|row| row.revision > since_revision)
    {
        return Err(AppError {
            message: format!(
                "That version is more than {MAX_REVERT_ROWS} changes back; Bhippi reverts up to \
                 {MAX_REVERT_ROWS} at once."
            ),
            hint: Some("Revert to a nearer version first, then to this one.".to_owned()),
        });
    }

    let mut current: Vec<(String, Option<Vec<u8>>)> = Vec::new();
    let mut target: std::collections::BTreeMap<String, Option<Vec<u8>>> =
        std::collections::BTreeMap::new();

    for row in &in_range {
        // The journal is shared with the pre-Godot engine. A row in the range whose inverse
        // is not a Godot change set cannot be replayed onto `.tscn` files, and skipping it
        // would produce a project that matches no version at all — so it stops the revert
        // and names itself.
        let Ok(inverse) = serde_json::from_str::<GodotChangeSet>(&row.inverse_json) else {
            return Err(AppError {
                message: format!(
                    "Revision {} (“{}”) is not a Godot change, so Bhippi cannot replay it.",
                    row.revision, row.label
                ),
                hint: Some(
                    "This project's history mixes two engines. Revert to a version above that \
                     change instead."
                        .to_owned(),
                ),
            });
        };
        for change in inverse.changes {
            if !current.iter().any(|(path, _)| path == &change.path) {
                // The newest row that touched this file: its inverse's `before` is that
                // row's forward `after`, which is the content on disk right now.
                current.push((change.path.clone(), change.before));
            }
            // Every row overwrites, so the oldest one in the range wins — its inverse's
            // `after` is the content the version names.
            target.insert(change.path, change.after);
        }
    }

    let mut changes: Vec<GodotFileChange> = Vec::new();
    for (path, before) in current {
        let after = target.get(&path).cloned().flatten();
        if after == before {
            // Written and written back inside the range: nothing to do, and a no-op change
            // in the plan would journal a file the revert did not actually alter.
            continue;
        }
        changes.push(GodotFileChange {
            path,
            before,
            after,
        });
    }

    if changes.is_empty() {
        return Err(AppError {
            message: format!("Nothing has changed since “{label}”."),
            hint: Some("This version already matches the project on disk.".to_owned()),
        });
    }

    let outcomes = changes
        .iter()
        .enumerate()
        .map(|(index, change)| GodotActionOutcome {
            index,
            ok: true,
            message: format!("restored {}", change.path),
            hint: None,
            node_path: None,
            needs_check: false,
        })
        .collect();

    Ok(GodotChangeSet {
        label: format!("Revert to {label}"),
        changes,
        outcomes,
    })
}

// ── journal access ───────────────────────────────────────────────────────────────────

/// The project key the journal is written under, matching `journal_edit`'s.
fn journal_key(root: &Path) -> String {
    root.to_string_lossy().replace('\\', "/")
}

/// The project's latest journal revision. `0` when there is no journal at all — which is
/// what a brand-new project's first version should record.
async fn latest_revision(state: &crate::Runtime, root: &Path) -> i64 {
    let Some(database) = state.brain_db.as_ref().as_ref() else {
        return 0;
    };
    database
        .engine()
        .latest_revision(&journal_key(root))
        .await
        .unwrap_or(0)
}

/// The rows above a revision, newest first.
async fn journal_rows(state: &crate::Runtime, root: &Path) -> Result<Vec<JournalRow>, AppError> {
    let database = state.brain_db.as_ref().as_ref().ok_or_else(|| AppError {
        message: "The change journal is unavailable.".to_owned(),
        hint: Some("Restart the app; versions read from the journal database.".to_owned()),
    })?;
    let limit = i64::try_from(MAX_REVERT_ROWS).unwrap_or(i64::MAX);
    let rows = database
        .engine()
        .list(&journal_key(root), None, limit)
        .await
        .map_err(|error| AppError {
            message: format!("Could not read the change journal: {error}"),
            hint: None,
        })?;
    Ok(rows
        .into_iter()
        .map(|row| JournalRow {
            revision: row.revision,
            label: row.label.unwrap_or_default(),
            inverse_json: row.inverse_json,
        })
        .collect())
}

/// Why a run stops a revert or a poster capture, when one does.
fn busy_reason(store: &GodotSessionStore, key: &str) -> Option<String> {
    let sessions = lock(store).ok()?;
    let session = sessions.get(key)?;
    session
        .is_busy()
        .then(|| session.running_kind().map(GodotRunKind::busy_message))
        .flatten()
        .map(str::to_owned)
}

// ── commands: versions ───────────────────────────────────────────────────────────────

/// The Versions drawer's list.
#[tauri::command]
#[specta::specta]
pub async fn godot_list_versions(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
) -> Result<VersionsView, AppError> {
    let root = resolve_project(&state, &project).await?;
    let file = load_versions(&root).map_err(engine_error)?;
    Ok(VersionsView {
        current_revision: latest_revision(&state, &root).await,
        revert_blocked: busy_reason(store.inner(), &display_of(&root)),
        versions: file.versions,
        notice: None,
    })
}

/// Name where the project stands right now.
#[tauri::command]
#[specta::specta]
pub async fn godot_create_version(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
    label: String,
) -> Result<VersionsView, AppError> {
    let root = resolve_project(&state, &project).await?;
    let label = check_label(&label).map_err(engine_error)?;
    let revision = latest_revision(&state, &root).await;
    let (file, dropped) = push_version(
        &root,
        GameVersion {
            id: ulid::Ulid::new().to_string(),
            label,
            created_at: chrono::Utc::now().to_rfc3339(),
            journal_revision: revision,
            export: None,
        },
    )?;
    Ok(VersionsView {
        versions: file.versions,
        current_revision: revision,
        notice: drop_notice(dropped),
        revert_blocked: busy_reason(store.inner(), &display_of(&root)),
    })
}

/// Add one version to the file on disk, pruning to the cap.
fn push_version(
    root: &Path,
    version: GameVersion,
) -> Result<(bhippi_engine::godot::versions::VersionsFile, usize), AppError> {
    let mut file = load_versions(root).map_err(engine_error)?;
    let dropped = file.push(version);
    save_versions(root, &file).map_err(engine_error)?;
    Ok((file, dropped))
}

fn drop_notice(dropped: usize) -> Option<String> {
    (dropped > 0).then(|| {
        format!(
            "{dropped} older version{} dropped: a project keeps its newest {MAX_VERSIONS}.",
            if dropped == 1 { " was" } else { "s were" }
        )
    })
}

/// Put the project back to a version, as one new undoable transaction.
#[tauri::command]
#[specta::specta]
pub async fn godot_revert_to(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
    version_id: String,
) -> Result<crate::godot_commands::GodotBatchResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    // A revert rewrites the files a running game has open. Refused rather than raced.
    if let Some(reason) = busy_reason(store.inner(), &key) {
        return Err(AppError {
            message: format!("Bhippi cannot revert while {reason}."),
            hint: Some("Stop it from the Godot pane's toolbar first.".to_owned()),
        });
    }

    let file = load_versions(&root).map_err(engine_error)?;
    let version = file.find(&version_id).ok_or_else(|| AppError {
        message: "That version is no longer in this project's list.".to_owned(),
        hint: Some("Reopen the Versions tab; the list may have been pruned.".to_owned()),
    })?;
    let rows = journal_rows(&state, &root).await?;
    let changeset = plan_revert(&rows, version.journal_revision, &version.label)?;
    apply_and_journal(GodotApplyHost { app: Some(&app) }, &root, changeset, "user")
        .await
        .map_err(|failure| failure.error)
}

/// Open the folder an export landed in, with whatever the OS uses for folders.
///
/// The share step is a **reveal**, not a zip: bundling would need a new dependency, and the
/// only zero-dependency route on Windows is a PowerShell call that would not work anywhere
/// else. A folder the user can drag is honest; a Windows-only Share button is not.
#[tauri::command]
#[specta::specta]
pub async fn godot_reveal_export(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    target: PresetTarget,
) -> Result<String, AppError> {
    let root = resolve_project(&state, &project).await?;
    let folder = export_dir(&root, target)?;
    if !folder.is_dir() {
        return Err(AppError {
            message: format!("Nothing has been exported to {} yet.", display_of(&folder)),
            hint: Some("Run Export ▾ → Publish web first.".to_owned()),
        });
    }
    crate::workspace::reveal_in_file_manager(&folder)?;
    Ok(display_of(&folder))
}

/// Package an export folder into a zip archive for sharing (GAD-125).
#[tauri::command]
#[specta::specta]
pub async fn godot_package_export(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    target: PresetTarget,
) -> Result<String, AppError> {
    let root = resolve_project(&state, &project).await?;
    let folder = export_dir(&root, target)?;
    if !folder.is_dir() {
        return Err(AppError {
            message: format!("Nothing has been exported to {} yet.", display_of(&folder)),
            hint: Some("Run Export first before creating a zip package.".to_owned()),
        });
    }
    let zip_name = match target {
        PresetTarget::Web => "web_export.zip",
        PresetTarget::Windows => "windows_export.zip",
    };
    let zip_dest = folder.join(zip_name);
    let engine_target = match target {
        PresetTarget::Web => bhippi_engine::godot::export::ExportTarget::Web,
        PresetTarget::Windows => bhippi_engine::godot::export::ExportTarget::WindowsDesktop,
    };
    let result_path =
        bhippi_engine::godot::export::package_export_zip(&root, engine_target, &zip_dest)
            .map_err(engine_error)?;
    Ok(display_of(&result_path))
}

/// The folder one target's artefacts land in, from the manifest where the manifest has a
/// say and from the export presets otherwise.
fn export_dir(root: &Path, target: PresetTarget) -> Result<PathBuf, AppError> {
    let relative = match target {
        PresetTarget::Web => load_manifest(root)
            .map_err(engine_error)?
            .map(|manifest| manifest.publish.web_export_dir)
            .unwrap_or_else(|| PublishSection::default().web_export_dir),
        PresetTarget::Windows => {
            bhippi_engine::godot::export_presets::WINDOWS_EXPORT_DIR.to_owned()
        }
    };
    validate_publish(&PublishSection {
        credits: true,
        web_export_dir: relative.clone(),
    })
    .map_err(engine_error)?;
    Ok(root.join(relative))
}

// ── commands: game settings ──────────────────────────────────────────────────────────

/// The Game tab's form.
#[tauri::command]
#[specta::specta]
pub async fn game_settings_get(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
) -> Result<GameSettings, AppError> {
    let root = resolve_project(&state, &project).await?;
    Ok(GameSettings::from_manifest(&require_manifest(&root)?))
}

/// Write the Game tab's form back through the manifest writer.
#[tauri::command]
#[specta::specta]
pub async fn game_settings_set(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    settings: GameSettings,
) -> Result<GameSettings, AppError> {
    let root = resolve_project(&state, &project).await?;
    let mut manifest = require_manifest(&root)?;
    settings.apply_to(&mut manifest)?;
    let text = render_manifest(&manifest);
    // The writer's own output is parsed back before it is trusted: `render_manifest` is
    // hand-written, and a settings save that produced a file the loader refuses would break
    // the project from inside the form meant to configure it.
    bhippi_engine::manifest::parse_manifest(&text).map_err(engine_error)?;
    std::fs::write(manifest_path(&root), text).map_err(|error| AppError {
        message: format!("Could not write Bhippi.game.toml: {error}"),
        hint: Some("Close the file in your editor and save again.".to_owned()),
    })?;
    Ok(GameSettings::from_manifest(&manifest))
}

fn require_manifest(root: &Path) -> Result<GameManifest, AppError> {
    load_manifest(root)
        .map_err(engine_error)?
        .ok_or_else(|| AppError {
            message: "This folder has no Bhippi.game.toml.".to_owned(),
            hint: Some("Open it as a game project, or create one from the launcher.".to_owned()),
        })
}

// ── commands: publish ────────────────────────────────────────────────────────────────

/// Export for the web, write the credits, and record the version (GAD-092).
#[tauri::command]
#[specta::specta]
pub async fn godot_publish_web(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
) -> Result<PublishResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let manifest = require_manifest(&root)?;

    // `godot_export` runs the release gates first and refuses on a blocker (INV-074), so an
    // unlicensed asset stops the publish before Godot is even started.
    let export: ExportResult = crate::godot_commands::godot_export(
        app.clone(),
        state.clone(),
        store.clone(),
        project.clone(),
        PresetTarget::Web,
    )
    .await?;
    if !export.ok {
        return Err(AppError {
            message: "The web export did not finish.".to_owned(),
            hint: Some("The Output log has Godot's own reason.".to_owned()),
        });
    }

    let index_html = PathBuf::from(&export.output_path);
    let output_dir = index_html
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(&manifest.publish.web_export_dir));

    let mut credited = 0_u32;
    let credits_html = if manifest.publish.credits {
        let credits = collect_credits(&root);
        credited = u32::try_from(credits.len()).unwrap_or(u32::MAX);
        let html = render_credits_html(
            &manifest.game.display_title(),
            &manifest.game.description,
            bhippi_engine::godot::detect::GODOT_PINNED_VERSION,
            &credits,
        );
        let path = output_dir.join(CREDITS_FILE);
        std::fs::write(&path, html).map_err(|error| AppError {
            message: format!("Could not write {CREDITS_FILE}: {error}"),
            hint: Some("Check the export folder is writable.".to_owned()),
        })?;
        Some(display_of(&path))
    } else {
        None
    };

    Ok(PublishResult {
        output_dir: display_of(&output_dir),
        index_html: display_of(&index_html),
        credits_html,
        // The export already recorded the version (GAD-094); a publish must not make a
        // second one for the same artefact.
        version_id: export.version_id,
        credited_assets: credited,
    })
}

/// Photograph the running game once and keep the frame as the project's poster.
#[tauri::command]
#[specta::specta]
pub async fn godot_capture_poster(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    viewport: tauri::State<'_, crate::godot_embed::GodotEmbedHost>,
    project: String,
) -> Result<String, AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    let store = store.inner().clone();
    let viewport = viewport.inner().clone();
    if let Some(reason) = busy_reason(&store, &key) {
        return Err(AppError {
            message: format!("Bhippi cannot take a poster while {reason}."),
            hint: Some("Stop it from the Godot pane's toolbar first.".to_owned()),
        });
    }
    let manifest = require_manifest(&root)?;
    let install = require_install(&state, &store, &key).await?;

    let plan = poster_plan();
    plan.validate()?;

    let (handle, signal) = crate::godot::stop_channel();
    let killer = handle.clone();
    claim_slot(&store, &key, GodotRunKind::VisualPlaytest, handle.clone())?;
    announce_process(
        &app,
        &key,
        GodotRunKind::VisualPlaytest,
        GodotRunState::Running,
        None,
    );
    let sender = start_output_pump(app.clone(), store.clone(), key.clone());
    let name = game_name(&root);
    // The poster is photographed inside the studio viewport like every other run (ADR-0045).
    let on_window: Box<crate::godot_observe::WindowHook> = {
        let app = app.clone();
        let viewport = viewport.clone();
        let key = key.clone();
        Box::new(move |window| {
            crate::godot_embed::adopt_foreign_window(
                &app,
                &viewport,
                &key,
                window.process_id,
                window.hwnd,
                handle.clone(),
            )
        })
    };
    let result = crate::godot_observe::run_visual_playtest(
        crate::godot_observe::VisualLaunch {
            root: &root,
            gui_exe: install.gui(),
            game_name: &name,
            stop: (killer, signal),
            lines: Some(sender),
            on_window: Some(on_window),
        },
        &plan,
    )
    .await;
    crate::godot_embed::release_foreign_window(&app, &viewport);
    release_slot(&store, &key, GodotRunKind::VisualPlaytest);
    announce_process(
        &app,
        &key,
        GodotRunKind::VisualPlaytest,
        GodotRunState::Exited,
        result.as_ref().ok().and_then(|result| result.exit),
    );

    let observed = result?;
    // The settled frame, not the opening one: the first capture is taken the moment the
    // window exists, which for most games is before anything has been drawn.
    let capture = observed.captures.last().ok_or_else(|| AppError {
        message: "The game window produced no frame to use as a poster.".to_owned(),
        hint: Some(
            observed
                .stopped_detail
                .clone()
                .unwrap_or_else(|| "Play the game once and try again.".to_owned()),
        ),
    })?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(capture.png_base64.as_bytes())
        .map_err(|error| AppError {
            message: format!("The captured frame was not readable: {error}"),
            hint: None,
        })?;

    let poster = root.join(POSTER_FILE);
    if let Some(parent) = poster.parent() {
        std::fs::create_dir_all(parent).map_err(|error| AppError {
            message: format!("Could not create the poster folder: {error}"),
            hint: Some("Check the project folder is writable.".to_owned()),
        })?;
    }
    std::fs::write(&poster, bytes).map_err(|error| AppError {
        message: format!("Could not write the poster: {error}"),
        hint: Some("Check the project folder is writable.".to_owned()),
    })?;

    let mut manifest = manifest;
    manifest.game.poster = Some(POSTER_FILE.to_owned());
    let text = render_manifest(&manifest);
    std::fs::write(manifest_path(&root), text).map_err(|error| AppError {
        message: format!("Could not record the poster in Bhippi.game.toml: {error}"),
        hint: Some("The image was written; set `[game] poster` by hand.".to_owned()),
    })?;
    Ok(POSTER_FILE.to_owned())
}

/// The one-step plan a poster capture runs: open, wait, photograph, close.
///
/// It lives in Rust for the reason every other plan does — the frame it produces is what
/// people see on the Games screen, and a UI that could rewrite the plan could rewrite what
/// the game appears to be.
#[must_use]
pub fn poster_plan() -> VisualPlaytestPlan {
    VisualPlaytestPlan {
        steps: vec![VisualStep {
            input: None,
            hold_ms: Some(POSTER_SETTLE_MS),
            note: Some("poster frame".to_owned()),
        }],
        capture_every_step: true,
        max_ms: POSTER_MAX_MS,
        telemetry: false,
    }
}

// ── the Games card ───────────────────────────────────────────────────────────────────

/// Everything one Games card shows, read inside the project root only.
#[tauri::command]
#[specta::specta]
pub async fn game_card_info(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
) -> Result<GameCardInfo, AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    let store_handle = store.inner().clone();
    let manifest = load_manifest(&root).ok().flatten();

    let title = manifest
        .as_ref()
        .map(|manifest| manifest.game.display_title())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| game_name(&root));

    // The manifest's poster when it names one, else the conventional path. Both are joined
    // to the root here, so no caller ever composes a project path itself.
    let poster_rel = manifest
        .as_ref()
        .and_then(|manifest| manifest.game.poster.clone())
        .unwrap_or_else(|| POSTER_FILE.to_owned());
    let poster_data_url = read_poster(&root, &poster_rel);

    let versions = load_versions(&root).unwrap_or_default();
    let is_godot_project = root
        .join(bhippi_engine::godot::action::PROJECT_FILE)
        .is_file();
    let godot_ready =
        is_godot_project && require_install(&state, &store_handle, &key).await.is_ok();
    let blocked_reason = card_blocked_reason(is_godot_project, godot_ready);

    Ok(GameCardInfo {
        project: key,
        title,
        poster_data_url,
        is_godot_project,
        version_count: u32::try_from(versions.versions.len()).unwrap_or(u32::MAX),
        last_export: versions
            .last_export()
            .map(|export| export.created_at.clone()),
        blocked_reason,
    })
}

/// Why Play, Snapshot and Publish may not run on this card, in the words the card puts
/// straight into a tooltip. `None` means they may.
///
/// Separated from the command so the sentence is testable without a Tauri state: the two
/// causes read alike on the card and must not be collapsed, because only one of them is
/// the user's to fix by installing something.
#[must_use]
fn card_blocked_reason(is_godot_project: bool, godot_ready: bool) -> Option<String> {
    if !is_godot_project {
        return Some("This folder is not a Godot game yet.".to_owned());
    }
    if !godot_ready {
        return Some(
            "Godot is not installed. Open the game and locate the engine in Settings.".to_owned(),
        );
    }
    None
}

/// The media type a poster file is served as, or `None` for an extension no browser is
/// asked to guess at.
///
/// The Tauri asset protocol is off, so a poster reaches the card as a `data:` URL and
/// something has to name its type. That something is here: a webview that picked the mime
/// from the file name would be deciding what the bytes are, which is a rule, and rules
/// live in Rust (INV-073).
#[must_use]
fn poster_media_type(relative: &str) -> Option<&'static str> {
    let extension = relative.rsplit('.').next()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Read a poster from inside the project as a ready-to-render `data:` URL, refusing a path
/// that leaves the project, a type no browser should be asked to guess, and a file too
/// large to put behind a card.
fn read_poster(root: &Path, relative: &str) -> Option<String> {
    if validate_publish(&PublishSection {
        credits: true,
        web_export_dir: relative.to_owned(),
    })
    .is_err()
    {
        return None;
    }
    let media_type = poster_media_type(relative)?;
    let path = root.join(relative);
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > MAX_CARD_POSTER_BYTES {
        return None;
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{media_type};base64,{encoded}"))
}

// ── the export hook (GAD-094) ────────────────────────────────────────────────────────

/// Record a version for an export that just succeeded.
///
/// Best-effort on purpose: the artefact exists whatever this does, and an export that
/// reported failure because its bookkeeping could not be written would be lying about the
/// thing the user actually asked for. A failure is logged and the export still succeeds.
pub(crate) async fn record_export_version(
    state: &crate::Runtime,
    root: &Path,
    target: PresetTarget,
    output: &Path,
) -> Option<String> {
    let now = chrono::Utc::now();
    let output_path = output
        .strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| display_of(output));
    let version = GameVersion {
        id: ulid::Ulid::new().to_string(),
        label: format!(
            "Export {} {}",
            target.preset_name(),
            now.format("%Y-%m-%d %H:%M")
        ),
        created_at: now.to_rfc3339(),
        journal_revision: latest_revision(state, root).await,
        export: Some(VersionExport {
            target: match target {
                PresetTarget::Web => "web".to_owned(),
                PresetTarget::Windows => "windows".to_owned(),
            },
            output_path,
            created_at: now.to_rfc3339(),
        }),
    };
    let id = version.id.clone();
    match push_version(root, version) {
        Ok(_) => Some(id),
        Err(error) => {
            tracing::warn!(message = %error.message, "the export version could not be recorded");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        card_blocked_reason, drop_notice, plan_revert, poster_plan, GameSettings, JournalRow,
        MAX_REVERT_ROWS, POSTER_SETTLE_MS,
    };
    use bhippi_engine::godot::action::{invert, GodotChangeSet, GodotFileChange};
    use bhippi_engine::godot::manifest::{godot_manifest, DEFAULT_MAIN_SCENE};
    use bhippi_engine::manifest::RenderPipeline;

    /// One journal row from a forward change set, exactly as `apply_and_journal` writes it.
    fn row(revision: i64, label: &str, changes: Vec<GodotFileChange>) -> JournalRow {
        let forward = GodotChangeSet {
            label: label.to_owned(),
            changes,
            outcomes: Vec::new(),
        };
        JournalRow {
            revision,
            label: label.to_owned(),
            inverse_json: serde_json::to_string(&invert(&forward)).expect("serialize"),
        }
    }

    fn change(path: &str, before: Option<&str>, after: Option<&str>) -> GodotFileChange {
        GodotFileChange {
            path: path.to_owned(),
            before: before.map(|text| text.as_bytes().to_vec()),
            after: after.map(|text| text.as_bytes().to_vec()),
        }
    }

    #[test]
    fn a_revert_collapses_one_change_per_file_and_its_own_inverse_re_applies_everything() {
        // v1 → A → B → C, versioned at revision 1. Reverting replays 4, 3 and 2.
        let rows = vec![
            row(
                4,
                "third",
                vec![change("scenes/main.tscn", Some("B"), Some("C"))],
            ),
            row(
                3,
                "second",
                vec![change("scripts/player.gd", None, Some("code"))],
            ),
            row(
                2,
                "first",
                vec![change("scenes/main.tscn", Some("A"), Some("B"))],
            ),
        ];
        let plan = plan_revert(&rows, 1, "v1").expect("a plan");
        assert_eq!(plan.label, "Revert to v1");
        assert_eq!(
            plan.changes.len(),
            2,
            "one change per file: {:?}",
            plan.changes
        );

        let scene = plan
            .changes
            .iter()
            .find(|change| change.path == "scenes/main.tscn")
            .expect("the scene");
        assert_eq!(
            scene.before.as_deref(),
            Some(b"C".as_slice()),
            "what is on disk now"
        );
        assert_eq!(
            scene.after.as_deref(),
            Some(b"A".as_slice()),
            "the version's state"
        );
        let script = plan
            .changes
            .iter()
            .find(|change| change.path == "scripts/player.gd")
            .expect("the script");
        assert_eq!(
            script.after, None,
            "a file created after the version is deleted again"
        );

        // The whole point of collapsing: undoing the revert puts the newest state back, not
        // an intermediate one.
        let undo = invert(&plan);
        let scene = undo
            .changes
            .iter()
            .find(|change| change.path == "scenes/main.tscn")
            .expect("the scene");
        assert_eq!(scene.after.as_deref(), Some(b"C".as_slice()));
        let script = undo
            .changes
            .iter()
            .find(|change| change.path == "scripts/player.gd")
            .expect("the script");
        assert_eq!(script.after.as_deref(), Some(b"code".as_slice()));
        assert!(plan.outcomes.iter().all(|outcome| outcome.ok));
    }

    #[test]
    fn a_file_written_and_written_back_inside_the_range_is_left_alone() {
        let rows = vec![
            row(
                3,
                "undid it",
                vec![change("scenes/main.tscn", Some("B"), Some("A"))],
            ),
            row(
                2,
                "did it",
                vec![change("scenes/main.tscn", Some("A"), Some("B"))],
            ),
        ];
        let error = plan_revert(&rows, 1, "v1").expect_err("a no-op revert is refused");
        assert!(error.message.contains("Nothing has changed since"));
        assert!(error.hint.is_some());
    }

    #[test]
    fn a_version_at_the_present_revision_reverts_nothing_and_says_so() {
        let rows = vec![row(2, "a", vec![change("a.tscn", None, Some("x"))])];
        let error = plan_revert(&rows, 2, "now").expect_err("nothing above the version");
        assert!(error.message.contains("Nothing has changed"));
    }

    #[test]
    fn a_row_that_is_not_a_godot_change_stops_the_revert_and_names_itself() {
        let rows = vec![
            row(
                3,
                "godot edit",
                vec![change("scenes/main.tscn", Some("A"), Some("B"))],
            ),
            JournalRow {
                revision: 2,
                label: "old engine edit".to_owned(),
                inverse_json: r#"{"ops":[{"kind":"spawn"}]}"#.to_owned(),
            },
        ];
        let error = plan_revert(&rows, 1, "v1").expect_err("a foreign row must block");
        assert!(error.message.contains("Revision 2"), "{}", error.message);
        assert!(error.message.contains("old engine edit"));
        assert!(error.hint.is_some());
        // A foreign row *below* the version is none of the revert's business.
        assert!(plan_revert(&rows, 2, "v2").is_ok());
    }

    #[test]
    fn a_version_further_back_than_the_window_is_refused_rather_than_half_applied() {
        let rows: Vec<JournalRow> = (0..MAX_REVERT_ROWS)
            .map(|index| {
                let revision = i64::try_from(MAX_REVERT_ROWS - index).unwrap_or_default() + 10;
                row(
                    revision,
                    "edit",
                    vec![change(&format!("scenes/s{index}.tscn"), None, Some("x"))],
                )
            })
            .collect();
        let error = plan_revert(&rows, 1, "ancient").expect_err("past the window");
        assert!(error.message.contains(&MAX_REVERT_ROWS.to_string()));
        assert!(error.hint.is_some());
        // The same full window is fine when the version is inside it.
        assert!(plan_revert(&rows, 500, "recent").is_ok());
    }

    // ── settings validation ───────────────────────────────────────────────────────

    fn settings() -> GameSettings {
        GameSettings {
            title: "Feather Quest".to_owned(),
            description: "Collect ten feathers.".to_owned(),
            tags: vec!["cozy".to_owned()],
            poster: Some(".bhippi/poster.png".to_owned()),
            credits: true,
            web_export_dir: "export/web".to_owned(),
        }
    }

    #[test]
    fn settings_round_trip_through_the_manifest_and_trim_what_the_form_left() {
        let mut manifest = godot_manifest("Demo", DEFAULT_MAIN_SCENE, RenderPipeline::D3d);
        let mut form = settings();
        form.title = "  Feather Quest  ".to_owned();
        form.tags = vec!["  cozy  ".to_owned(), "   ".to_owned()];
        form.apply_to(&mut manifest).expect("valid settings apply");
        assert_eq!(manifest.game.title, "Feather Quest");
        assert_eq!(
            manifest.game.tags,
            vec!["cozy"],
            "a blank tag is dropped, not stored"
        );

        let read_back = GameSettings::from_manifest(&manifest);
        assert_eq!(read_back.title, "Feather Quest");
        assert_eq!(read_back.poster.as_deref(), Some(".bhippi/poster.png"));
        assert!(read_back.credits);
    }

    #[test]
    fn every_settings_rule_refuses_with_a_hint_rather_than_silently_correcting() {
        let base = godot_manifest("Demo", DEFAULT_MAIN_SCENE, RenderPipeline::D3d);
        let cases: Vec<(&str, GameSettings)> = vec![
            (
                "an empty title",
                GameSettings {
                    title: "   ".to_owned(),
                    ..settings()
                },
            ),
            (
                "a title past the cap",
                GameSettings {
                    title: "x".repeat(81),
                    ..settings()
                },
            ),
            (
                "too many tags",
                GameSettings {
                    tags: (0..13).map(|index| format!("tag{index}")).collect(),
                    ..settings()
                },
            ),
            (
                "a tag past the cap",
                GameSettings {
                    tags: vec!["x".repeat(33)],
                    ..settings()
                },
            ),
            (
                "a poster outside the project",
                GameSettings {
                    poster: Some("../secrets.png".to_owned()),
                    ..settings()
                },
            ),
            (
                "an absolute poster",
                GameSettings {
                    poster: Some("C:/other/poster.png".to_owned()),
                    ..settings()
                },
            ),
            (
                "an export dir outside the project",
                GameSettings {
                    web_export_dir: "../web".to_owned(),
                    ..settings()
                },
            ),
            (
                "an empty export dir",
                GameSettings {
                    web_export_dir: "  ".to_owned(),
                    ..settings()
                },
            ),
        ];
        for (what, form) in cases {
            let mut manifest = base.clone();
            let error = form
                .apply_to(&mut manifest)
                .expect_err(&format!("{what} must be refused"));
            assert!(error.hint.is_some(), "{what} refused without a hint");
            assert_eq!(manifest, base, "{what} must not half-write the manifest");
        }
    }

    #[test]
    fn a_backslash_poster_path_is_normalised_rather_than_refused() {
        let mut manifest = godot_manifest("Demo", DEFAULT_MAIN_SCENE, RenderPipeline::D3d);
        let form = GameSettings {
            poster: Some(".bhippi\\poster.png".to_owned()),
            ..settings()
        };
        form.apply_to(&mut manifest)
            .expect("a Windows path is still inside the project");
        assert_eq!(manifest.game.poster.as_deref(), Some(".bhippi/poster.png"));
    }

    // ── small rules ───────────────────────────────────────────────────────────────

    #[test]
    fn the_drop_notice_counts_and_is_absent_when_nothing_was_dropped() {
        assert!(drop_notice(0).is_none());
        assert!(drop_notice(1)
            .unwrap_or_default()
            .contains("1 older version was"));
        assert!(drop_notice(3)
            .unwrap_or_default()
            .contains("3 older versions were"));
    }

    // ── the whole revert, through the real journal ────────────────────────────────
    //
    // Everything above is the planner in isolation. This drives the shipping path: a real
    // scaffolded project, three batches through `apply_batch_for`, a real SQLite journal,
    // and the revert applied and journaled by `apply_and_journal` — the same two functions
    // the Tauri command calls. Only the command wrapper (project resolution and the busy
    // check) is left out, because it needs a Tauri runtime and this does not.
    mod through_the_journal {
        use super::super::{plan_revert, push_version, JournalRow};
        use crate::godot_commands::{apply_batch_for, GodotApplyHost};
        use bhippi_engine::godot::action::{GodotAction, GodotActionBatch};
        use bhippi_engine::godot::scaffold::{write_project, ProjectTemplate};
        use bhippi_engine::godot::tscn::TscnValue;
        use bhippi_engine::godot::versions::GameVersion;
        use std::path::{Path, PathBuf};

        const SCENE: &str = "scenes/main.tscn";

        struct TempProject(PathBuf);

        impl TempProject {
            fn scaffolded() -> Self {
                let path =
                    std::env::temp_dir().join(format!("bhippi-revert-{}", ulid::Ulid::new()));
                write_project(&path, "Revert Demo", ProjectTemplate::Empty3D, false)
                    .expect("the scaffold writes a project");
                Self(path)
            }

            fn scene_bytes(&self) -> Vec<u8> {
                std::fs::read(self.0.join(SCENE)).expect("the main scene")
            }
        }

        impl Drop for TempProject {
            fn drop(&mut self) {
                let _ignored = std::fs::remove_dir_all(&self.0);
            }
        }

        /// The journal database this test binary writes into. `register_journal_db` is a
        /// `OnceLock`, so it is set once and every test in the binary shares it — which is
        /// fine, because the journal is keyed by project path and each test has its own.
        async fn journal() -> bhippi_db::Database {
            static PATH: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
            let path = PATH.get_or_init(|| {
                std::env::temp_dir().join(format!("bhippi-revert-journal-{}.db", ulid::Ulid::new()))
            });
            let database = bhippi_db::Database::connect(path)
                .await
                .expect("a temp journal database");
            crate::engine::register_journal_db(database.clone());
            database
        }

        fn add_node(name: &str) -> GodotActionBatch {
            GodotActionBatch::new(
                format!("add {name}"),
                vec![GodotAction::AddNode {
                    scene: SCENE.to_owned(),
                    parent: ".".to_owned(),
                    name: name.to_owned(),
                    type_: "Node3D".to_owned(),
                    properties: Vec::new(),
                    groups: Vec::new(),
                }],
            )
        }

        async fn rows(database: &bhippi_db::Database, root: &Path) -> Vec<JournalRow> {
            let key = root.to_string_lossy().replace('\\', "/");
            database
                .engine()
                .list(&key, None, 64)
                .await
                .expect("journal rows")
                .into_iter()
                .map(|row| JournalRow {
                    revision: row.revision,
                    label: row.label.unwrap_or_default(),
                    inverse_json: row.inverse_json,
                })
                .collect()
        }

        #[tokio::test]
        async fn a_revert_restores_the_versioned_bytes_exactly_and_undoing_it_puts_them_back() {
            let project = TempProject::scaffolded();
            let root = project.0.clone();
            let database = journal().await;
            let host = GodotApplyHost::default();

            // Batch 1, then the version: this is the state Revert has to reproduce.
            apply_batch_for(host, &root, &add_node("Alpha"), "user")
                .await
                .map_err(|failure| failure.error.message)
                .expect("batch 1");
            let after_first = project.scene_bytes();
            let revision_at_version = database
                .engine()
                .latest_revision(&root.to_string_lossy().replace('\\', "/"))
                .await
                .expect("a revision");
            assert_eq!(
                revision_at_version, 1,
                "the scaffold journals nothing of its own"
            );
            let (versions, dropped) = push_version(
                &root,
                GameVersion {
                    id: "01TESTVERSION".to_owned(),
                    label: "first light".to_owned(),
                    created_at: "2026-09-03T10:00:00Z".to_owned(),
                    journal_revision: revision_at_version,
                    export: None,
                },
            )
            .expect("the version is written");
            assert_eq!(dropped, 0);
            assert_eq!(versions.versions.len(), 1);

            // Batches 2 and 3 — the work the revert has to take back.
            apply_batch_for(host, &root, &add_node("Beta"), "user")
                .await
                .map_err(|failure| failure.error.message)
                .expect("batch 2");
            apply_batch_for(
                host,
                &root,
                &GodotActionBatch::new(
                    "name the root",
                    vec![GodotAction::SetProperty {
                        scene: SCENE.to_owned(),
                        path: "Beta".to_owned(),
                        property: "visible".to_owned(),
                        value: TscnValue::Bool(false),
                    }],
                ),
                "user",
            )
            .await
            .map_err(|failure| failure.error.message)
            .expect("batch 3");
            let after_third = project.scene_bytes();
            assert_ne!(after_third, after_first);

            let before_revert = rows(&database, &root).await;
            assert_eq!(before_revert.len(), 3, "three batches, three rows");

            // The revert itself, exactly as `godot_revert_to` performs it.
            let version = versions
                .find("01TESTVERSION")
                .cloned()
                .expect("the version is in the list");
            let plan = plan_revert(&before_revert, version.journal_revision, &version.label)
                .expect("a plan");
            assert_eq!(plan.label, "Revert to first light");
            let result = crate::godot_commands::apply_and_journal(host, &root, plan, "user")
                .await
                .map_err(|failure| failure.error.message)
                .expect("the revert applies");

            assert_eq!(
                project.scene_bytes(),
                after_first,
                "the scene is byte-identical to the versioned state"
            );
            let after_revert = rows(&database, &root).await;
            assert_eq!(
                after_revert.len(),
                4,
                "a revert is one new row, not one per replayed change"
            );
            assert_eq!(result.revision, Some(4));
            assert_eq!(after_revert[0].label, "Revert to first light");

            // …and the revert is itself undoable, which is the property the per-file
            // collapse in `plan_revert` exists to guarantee.
            let inverse = serde_json::from_str::<bhippi_engine::godot::action::GodotChangeSet>(
                &after_revert[0].inverse_json,
            )
            .expect("the revert's inverse is a Godot change set");
            crate::godot_commands::apply_and_journal(host, &root, inverse, "user")
                .await
                .map_err(|failure| failure.error.message)
                .expect("undoing the revert applies");
            assert_eq!(
                project.scene_bytes(),
                after_third,
                "undoing the revert restores the newest state, not an intermediate one"
            );
        }
    }

    #[test]
    fn the_poster_plan_takes_one_settled_frame_and_asks_for_no_input() {
        let plan = poster_plan();
        plan.validate().expect("the poster plan is valid");
        assert_eq!(plan.steps.len(), 1);
        assert!(
            plan.steps[0].input.is_none(),
            "a poster never plays the game"
        );
        assert_eq!(plan.steps[0].hold_ms, Some(POSTER_SETTLE_MS));
        assert!(!plan.telemetry, "a poster needs no probe");
        assert_eq!(
            plan.planned_captures(),
            2,
            "the opening frame and the settled one"
        );
        assert!(
            plan.max_ms > POSTER_SETTLE_MS + crate::godot_observe::WINDOW_WAIT_MS,
            "the budget has to cover Godot starting as well as the settle"
        );
    }

    // ── the Games card ───────────────────────────────────────────────────────────────

    #[test]
    fn a_folder_with_no_project_godot_is_named_as_such_and_not_as_a_missing_engine() {
        let reason = card_blocked_reason(false, false).expect("a folder with no game is blocked");
        assert!(
            reason.contains("not a Godot game"),
            "the card says what is actually wrong, not that Godot is missing: {reason}"
        );
        // The two causes never collapse: only one of them is fixed by installing anything.
        let missing = card_blocked_reason(true, false).expect("a game with no engine is blocked");
        assert!(missing.contains("Godot is not installed"), "{missing}");
        assert_ne!(reason, missing);
    }

    #[test]
    fn a_godot_game_with_its_engine_present_is_not_blocked() {
        assert_eq!(card_blocked_reason(true, true), None);
    }

    #[test]
    fn the_retired_engine_pane_is_not_named_in_the_card_s_advice() {
        let missing = card_blocked_reason(true, false).unwrap_or_default();
        assert!(
            !missing.contains("Engine pane"),
            "the pane is gone; the hint has to name somewhere that exists: {missing}"
        );
    }

    #[test]
    fn a_poster_carries_the_media_type_the_file_names_and_nothing_else() {
        assert_eq!(
            super::poster_media_type(".bhippi/poster.png"),
            Some("image/png")
        );
        assert_eq!(
            super::poster_media_type("art/cover.JPG"),
            Some("image/jpeg")
        );
        assert_eq!(
            super::poster_media_type("art/cover.jpeg"),
            Some("image/jpeg")
        );
        assert_eq!(
            super::poster_media_type("art/cover.webp"),
            Some("image/webp")
        );
        // Anything a browser would have to sniff is refused rather than guessed at.
        assert_eq!(super::poster_media_type("art/cover.svg"), None);
        assert_eq!(super::poster_media_type("art/cover.exe"), None);
        assert_eq!(super::poster_media_type("poster"), None);
    }

    #[test]
    fn a_poster_path_that_leaves_the_project_is_never_read() {
        let root = std::path::Path::new("C:/games/feathers");
        assert_eq!(super::read_poster(root, "../secrets.png"), None);
        assert_eq!(super::read_poster(root, "C:/other/poster.png"), None);
        // A path inside the project is allowed through to the filesystem, which has no
        // such file here — so the answer is still `None`, for the other reason.
        assert_eq!(super::read_poster(root, super::POSTER_FILE), None);
    }
}
