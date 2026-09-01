//! Open-scene sessions: the engine's single source of truth while a project is open
//! (ENG-100…102, INV-070).
//!
//! Before this, the Engine pane held the scene in React state and wrote the whole file
//! with `write_workspace_file`, while the AI went through `EngineTransaction` — two write
//! paths racing each other over one file. Now every mutation, from a gizmo drag or from a
//! model, lands here: one in-memory document per open scene, one undo stack, one dirty
//! flag, one revision counter. The webview renders what this hands it and computes nothing
//! (INV-073).

use super::content::{ContentAction, FileChange};
use crate::commands::AppError;
use bhippi_engine::action::{EngineAction, EngineActionBatch};
use bhippi_engine::document::{SceneDocument, SceneKind, SceneSettings};
use bhippi_engine::transaction::{EngineTransaction, Op, Session, UndoStack};
use bhippi_types::TransactionId as TxnId;
use bhippi_types::{EngineActor, EntityId, TransactionId};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use std::time::SystemTime;

/// Identifies one open scene: which game folder, which scene inside it.
pub type SceneKey = (String, String);

/// What the file looked like the last time we read or wrote it. A cheap stamp is enough to
/// notice that something outside the app rewrote the scene under an unsaved edit (ENG-108);
/// we do not try to merge, we tell the user and let them choose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiskStamp {
    pub len: u64,
    pub modified: Option<SystemTime>,
}

impl DiskStamp {
    fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

/// One scene held open by the editor.
pub struct OpenScene {
    /// The game folder this scene lives under — kept so a state push can notice that the
    /// file changed on disk without the caller having to pass the root back in.
    pub game_dir: PathBuf,
    pub rel_path: String,
    pub doc: SceneDocument,
    pub undo: UndoStack,
    pub dirty: bool,
    /// Bumped on every applied transaction. The webview compares it to decide whether the
    /// state it is holding is still current.
    pub revision: u32,
    pub stamp: Option<DiskStamp>,
    pub selection: Vec<EntityId>,
    /// A live interactive edit (a gizmo drag). Ops apply immediately; the undo entry is
    /// pushed once on commit, so a drag is one undo step and not one per frame.
    pub interaction: Option<LiveInteraction>,
    /// The transaction most recently applied to this scene — the source of the edit
    /// result and the journal row, kept so the caller does not have to reach into the
    /// undo stack (undo/redo move entries between its halves).
    last: Option<EngineTransaction>,
    /// Files written by each transaction on the undo side, keyed by transaction id
    /// (ENG-122). Keeping these beside the engine's `UndoStack` rather than inside it is
    /// what lets `bhippi-engine` stay filesystem-free while "create a material and assign
    /// it" is still a single Ctrl+Z.
    undo_files: BTreeMap<TxnId, Vec<FileChange>>,
    redo_files: BTreeMap<TxnId, Vec<FileChange>>,
    /// File changes staged by the batch currently being committed.
    pending_files: Vec<FileChange>,
    /// Who has claimed this scene, and what it was at when they claimed it (ENG-192).
    /// A lease is advisory between *agents*; the user is never locked out of their own
    /// editor, and a stale lease expires rather than wedging the scene forever.
    lease: Option<SceneLease>,
}

/// One agent's claim on a scene (ENG-192).
#[derive(Clone, Debug)]
pub struct SceneLease {
    pub owner: String,
    pub taken_at: std::time::Instant,
    /// The revision the owner last saw. A committed batch refreshes it.
    pub revision: u32,
}

/// How long an unrefreshed lease survives. Long enough that a slow model keeps its claim
/// across a think, short enough that a crashed agent does not lock a scene until restart.
pub const SCENE_LEASE_TTL: std::time::Duration = std::time::Duration::from_secs(120);

pub struct LiveInteraction {
    pub session: Session,
    pub label: String,
}

impl OpenScene {
    /// Claim this scene for a batch (ENG-192).
    ///
    /// Two failures are worth different messages. **Someone else holds it** means a second
    /// agent is mid-task; refusing is right, and the refusal names the holder. **The scene
    /// moved** means the caller planned against a revision that no longer exists — the
    /// dangerous case, because applying anyway silently overwrites whatever changed. Both
    /// refuse; neither overwrites.
    ///
    /// The user is never blocked. A lease is a coordination device between agents, and an
    /// editor that locks the person out of their own file to protect a background agent has
    /// its priorities inverted — a user edit simply breaks the lease.
    fn claim(
        &mut self,
        actor: EngineActor,
        owner: Option<&str>,
        base_revision: Option<u32>,
    ) -> Result<(), AppError> {
        if !matches!(actor, EngineActor::Agent) {
            // The user wins outright — and the lease is deliberately *left in place*, still
            // recording the revision the agent last saw. That is the whole signal: on its
            // next batch the holder finds its lease pointing at a revision that no longer
            // exists and is sent to re-read, instead of committing a plan built on a scene
            // the user has since changed.
            return Ok(());
        }

        if let Some(base) = base_revision {
            if base != self.revision {
                return Err(AppError {
                    message: format!(
                        "This scene has moved since you read it: you planned against revision                          {base}, it is now at {}.",
                        self.revision
                    ),
                    hint: Some(
                        "Re-read the scene and resend the batch against what is there now.                          Nothing was written."
                            .to_owned(),
                    ),
                });
            }
        }

        let claimant = owner.unwrap_or("agent");
        if let Some(existing) = &self.lease {
            let live = existing.taken_at.elapsed() < SCENE_LEASE_TTL;
            if existing.owner != claimant && live {
                return Err(AppError {
                    message: format!(
                        "`{}` is editing {} right now.",
                        existing.owner, self.rel_path
                    ),
                    hint: Some(format!(
                        "Wait for it to finish, work on another scene, or retry in {} s.                          Nothing was written.",
                        SCENE_LEASE_TTL
                            .saturating_sub(existing.taken_at.elapsed())
                            .as_secs()
                            .max(1)
                    )),
                });
            }
            if existing.owner == claimant && existing.revision != self.revision {
                let seen = existing.revision;
                // Refresh before refusing, so the very next attempt — after the model has
                // re-read the scene — succeeds instead of reporting the same staleness
                // forever.
                self.lease = Some(SceneLease {
                    owner: claimant.to_owned(),
                    taken_at: std::time::Instant::now(),
                    revision: self.revision,
                });
                return Err(AppError {
                    message: format!(
                        "{} changed under you: you last saw revision {seen}, it is now at {}.",
                        self.rel_path, self.revision
                    ),
                    hint: Some(
                        "Someone else edited this scene. Re-read it and resend. Nothing was                          written."
                            .to_owned(),
                    ),
                });
            }
        }

        self.lease = Some(SceneLease {
            owner: claimant.to_owned(),
            taken_at: std::time::Instant::now(),
            revision: self.revision,
        });
        Ok(())
    }

    /// Point the lease at the revision the scene is now at — but **only** when the commit
    /// came from the holder itself.
    ///
    /// Refreshing on every commit would be the bug this whole mechanism exists to prevent:
    /// a user edit would quietly bring the agent's lease up to date, and the agent would go
    /// on to apply a plan built before that edit, having been told nothing.
    fn refresh_lease_for(&mut self, actor: EngineActor, owner: Option<&str>) {
        if !matches!(actor, EngineActor::Agent) {
            return;
        }
        let claimant = owner.unwrap_or("agent");
        let revision = self.revision;
        if let Some(lease) = self.lease.as_mut().filter(|lease| lease.owner == claimant) {
            lease.revision = revision;
            lease.taken_at = std::time::Instant::now();
        }
    }

    /// Whether an agent other than `owner` currently holds this scene.
    #[must_use]
    pub fn held_by_other(&self, owner: &str) -> Option<&str> {
        self.lease
            .as_ref()
            .filter(|lease| lease.owner != owner && lease.taken_at.elapsed() < SCENE_LEASE_TTL)
            .map(|lease| lease.owner.as_str())
    }

    fn recovery_path(game_dir: &Path, rel_path: &str) -> PathBuf {
        let file = rel_path.replace(['/', '\\'], "__");
        game_dir
            .join(".bhippi/engine/autosave")
            .join(format!("{file}.autosave.json"))
    }

    fn write_recovery(&self) -> Result<(), AppError> {
        let path = Self::recovery_path(&self.game_dir, &self.rel_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| AppError {
                message: format!("Could not create the engine recovery folder: {error}"),
                hint: Some("Check that .bhippi is writable.".to_owned()),
            })?;
        }
        let dumped = self.doc.dump().map_err(super::engine_error)?;
        std::fs::write(&path, dumped).map_err(|error| AppError {
            message: format!("Could not write the recovery snapshot: {error}"),
            hint: Some("Save the scene now and check that .bhippi is writable.".to_owned()),
        })
    }

    fn clear_recovery(&self) {
        let path = Self::recovery_path(&self.game_dir, &self.rel_path);
        if path.is_file() {
            let _ignored = std::fs::remove_file(path);
        }
    }

    fn recovery_available(&self) -> bool {
        Self::recovery_path(&self.game_dir, &self.rel_path).is_file()
    }

    fn load(game_dir: &Path, rel_path: &str) -> Result<Self, AppError> {
        let path = game_dir.join(rel_path);
        let text = std::fs::read_to_string(&path).map_err(|error| AppError {
            message: format!("Could not read {rel_path}: {error}"),
            hint: Some("Save the scene from the Engine pane first.".to_owned()),
        })?;
        let doc = SceneDocument::parse_lenient(&text).map_err(super::engine_error)?;
        Ok(Self {
            game_dir: game_dir.to_path_buf(),
            rel_path: rel_path.to_owned(),
            doc,
            undo: UndoStack::new(),
            dirty: false,
            revision: 0,
            lease: None,
            stamp: DiskStamp::of(&path),
            selection: Vec::new(),
            interaction: None,
            last: None,
            undo_files: BTreeMap::new(),
            redo_files: BTreeMap::new(),
            pending_files: Vec::new(),
        })
    }

    /// True when the file on disk no longer matches what we last read or wrote.
    #[must_use]
    pub fn disk_changed(&self) -> bool {
        let current = DiskStamp::of(&self.game_dir.join(&self.rel_path));
        match (self.stamp, current) {
            (Some(known), Some(now)) => known != now,
            // A scene we have never written and that does not exist is not a conflict.
            (None, None) => false,
            _ => true,
        }
    }

    /// The state the UI renders from.
    #[must_use]
    pub fn state(&self) -> EngineSceneState {
        EngineSceneState {
            scene_path: self.rel_path.clone(),
            name: self.doc.name.clone(),
            kind: self.doc.settings.kind,
            settings: self.doc.settings.clone(),
            entity_count: self.doc.entity_count() as u32,
            dirty: self.dirty,
            can_undo: self.undo.can_undo(),
            can_redo: self.undo.can_redo(),
            undo_label: self.undo.peek_undo().map(|txn| txn.label.clone()),
            redo_label: self.undo.peek_redo().map(|txn| txn.label.clone()),
            revision: self.revision,
            selection: self.selection.clone(),
            // ENG-108: something rewrote the file while we hold unsaved edits. We do not
            // merge and we do not silently win — the pane offers Keep mine / Take disk.
            disk_conflict: self.dirty && self.disk_changed(),
            recovery_available: self.recovery_available(),
            // The document itself crosses as its own canonical text. `serde_json::Value`
            // has no honest specta representation (it exports an externally-tagged union
            // that does not match the bytes actually sent), and `bhippi-scene@1` is
            // already the documented, deterministic contract — so the webview parses this
            // once per push instead of reading a fictional type.
            document_json: self.doc.dump().unwrap_or_default(),
        }
    }

    fn write(&mut self, game_dir: &Path) -> Result<(), AppError> {
        let path = game_dir.join(&self.rel_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| AppError {
                message: format!("Could not create the scene folder: {error}"),
                hint: Some("Check the project is writable.".to_owned()),
            })?;
        }
        let dumped = self.doc.dump().map_err(super::engine_error)?;
        std::fs::write(&path, dumped).map_err(|error| AppError {
            message: format!("Could not save {}: {error}", self.rel_path),
            hint: Some("Check the scene file is writable.".to_owned()),
        })?;
        self.stamp = DiskStamp::of(&path);
        self.dirty = false;
        self.clear_recovery();
        Ok(())
    }
}

/// The scene state the webview renders. Everything here is derived — the webview owns no
/// scene state of its own and never writes a scene file.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineSceneState {
    pub scene_path: String,
    pub name: String,
    pub kind: SceneKind,
    pub settings: SceneSettings,
    pub entity_count: u32,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    pub revision: u32,
    pub selection: Vec<EntityId>,
    pub disk_conflict: bool,
    /// A crash-safe snapshot exists under `.bhippi/engine/autosave/` and can be replayed.
    pub recovery_available: bool,
    pub document_json: String,
}

/// What one applied transaction did — returned to the caller and broadcast as an event so
/// panels patch the touched entities instead of reloading the scene (ENG-107, INV-076).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineEditResult {
    pub scene_path: String,
    pub txn_id: String,
    /// `user` | `agent`
    pub actor: String,
    pub label: String,
    pub summary: String,
    pub op_count: u32,
    pub touched: Vec<EntityId>,
    pub state: EngineSceneState,
    /// Set when the transaction was journaled; `None` means the journal was unavailable
    /// (the edit still applied — the ledger is a record, not a gate).
    pub revision: Option<i64>,
}

/// The per-action result envelope (ENG-112).
///
/// A model that gets back only "failed" learns nothing. This carries the index that broke,
/// the engine's own message and hint, and — when the action named a component — the schema
/// excerpt for it, so the repair round has the real field list instead of a guess.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineActionOutcome {
    pub index: u32,
    pub ok: bool,
    /// The action's own one-line label ("spawn cube"), or its `kind` when it never parsed.
    pub label: String,
    pub message: String,
    pub hint: Option<String>,
    pub schema_excerpt: Option<String>,
}

/// What a batch did. `applied` is false when nothing was written — a batch is all-or-
/// nothing, because a half-built level is worse than none.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineBatchResult {
    pub applied: bool,
    pub label: String,
    pub scene_path: String,
    pub outcomes: Vec<EngineActionOutcome>,
    /// Present when the batch applied.
    pub edit: Option<EngineEditResult>,
    /// The scene state either way, so a caller that failed still renders current truth.
    pub state: EngineSceneState,
}

impl EngineBatchResult {
    /// The one-line summary the Activity Dock shows.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.applied {
            format!("{} · {} actions", self.label, self.outcomes.len())
        } else {
            let failed = self
                .outcomes
                .iter()
                .find(|outcome| !outcome.ok)
                .map(|outcome| format!("action {} — {}", outcome.index + 1, outcome.message))
                .unwrap_or_else(|| "no actions".to_owned());
            format!("{} rejected: {failed}", self.label)
        }
    }
}

/// The facts a caller needs to journal a transaction, handed back so the (async) database
/// write happens after the lock on the session map has been dropped.
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

/// One applied edit, before journaling.
pub struct AppliedEdit {
    pub result: EngineEditResult,
    pub journal: JournalFacts,
}

/// One step of a batch: an edit to the scene, or a write to an asset file. Both commit in
/// the same transaction, which is what makes "create a material and put it on the floor" a
/// single change and a single Ctrl+Z.
pub enum BatchStep {
    Scene(Box<EngineAction>),
    Content(Box<ContentAction>),
}

impl BatchStep {
    #[must_use]
    pub fn to_label(&self) -> String {
        match self {
            Self::Scene(action) => action.to_label(),
            Self::Content(action) => action.to_label(),
        }
    }
}

/// Everything one batch application needs. Grouped rather than passed as a long argument
/// list, because five of the six fields are easy to transpose at a call site.
pub struct BatchRequest<'a> {
    pub game_dir: &'a Path,
    pub rel_path: &'a str,
    /// What the user sees on the Undo button.
    pub label: &'a str,
    pub actions: &'a [serde_json::Value],
    pub actor: EngineActor,
    /// Agent batches autosave (a model has no Save button); interactive ones do not.
    pub autosave: bool,
    /// Who is committing, when the caller is an agent (ENG-192). Two agents sharing a
    /// project use this to take a lease on a scene; `None` means "no claim", which is what
    /// a single-agent setup and every user edit pass.
    pub owner: Option<&'a str>,
    /// The revision the caller planned against. When the scene has moved on since, the
    /// batch is refused with a rebase prompt instead of overwriting someone else's work.
    pub base_revision: Option<u32>,
}

/// One applied batch, before journaling. `journal` is `None` when the batch was rejected
/// and therefore wrote nothing — there is no transaction to record.
#[derive(Debug)]
pub struct AppliedBatch {
    pub result: EngineBatchResult,
    pub journal: Option<JournalFacts>,
}

/// Every scene the editor currently holds open, keyed by game folder + relative path.
#[derive(Default)]
pub struct EngineSessions {
    scenes: BTreeMap<SceneKey, OpenScene>,
}

impl EngineSessions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn key(game_dir: &Path, rel_path: &str) -> SceneKey {
        (
            game_dir.to_string_lossy().replace('\\', "/"),
            rel_path.replace('\\', "/"),
        )
    }

    /// Open a scene (loading it from disk on first touch) and return its state.
    pub fn open(&mut self, game_dir: &Path, rel_path: &str) -> Result<EngineSceneState, AppError> {
        Ok(self.entry(game_dir, rel_path)?.state())
    }

    /// Re-read a scene from disk, discarding unsaved edits and its undo history.
    pub fn reload(
        &mut self,
        game_dir: &Path,
        rel_path: &str,
    ) -> Result<EngineSceneState, AppError> {
        if let Some(scene) = self.scenes.get(&Self::key(game_dir, rel_path)) {
            scene.clear_recovery();
        }
        self.scenes.remove(&Self::key(game_dir, rel_path));
        self.open(game_dir, rel_path)
    }

    /// Replace the in-memory scene with the validated crash snapshot. Recovery never writes
    /// the authored file; the user reviews the dirty document and chooses Save.
    pub fn recover(
        &mut self,
        game_dir: &Path,
        rel_path: &str,
    ) -> Result<EngineSceneState, AppError> {
        let scene = self.entry(game_dir, rel_path)?;
        let path = OpenScene::recovery_path(game_dir, rel_path);
        let text = std::fs::read_to_string(&path).map_err(|error| AppError {
            message: format!("Could not read the recovery snapshot: {error}"),
            hint: Some("Choose Take disk if the snapshot is no longer usable.".to_owned()),
        })?;
        scene.doc = SceneDocument::parse(&text).map_err(super::engine_error)?;
        scene.undo = UndoStack::new();
        scene.dirty = true;
        scene.revision = scene.revision.wrapping_add(1);
        scene.write_recovery()?;
        Ok(scene.state())
    }

    /// Drop a scene. `discard` must be true to abandon unsaved work — otherwise a dirty
    /// scene refuses to close, so a stray close can never lose an edit.
    pub fn close(
        &mut self,
        game_dir: &Path,
        rel_path: &str,
        discard: bool,
    ) -> Result<(), AppError> {
        let key = Self::key(game_dir, rel_path);
        if let Some(scene) = self.scenes.get(&key) {
            if scene.dirty && !discard {
                return Err(AppError {
                    message: format!("{rel_path} has unsaved changes."),
                    hint: Some("Save it, or close again to discard.".to_owned()),
                });
            }
        }
        if discard {
            if let Some(scene) = self.scenes.get(&key) {
                scene.clear_recovery();
            }
        }
        self.scenes.remove(&key);
        Ok(())
    }

    /// Forget every scene under a project root, including a game folder nested inside it.
    /// Called when a project is removed from the workspace — the one moment dropping
    /// unsaved editor state is what the user asked for. Switching projects deliberately
    /// does *not* do this: come back and your unsaved level is still there.
    pub fn close_project(&mut self, project_root: &Path) {
        let prefix = project_root.to_string_lossy().replace('\\', "/");
        let nested = format!("{prefix}/");
        self.scenes
            .retain(|(root, _), _| root != &prefix && !root.starts_with(&nested));
    }

    fn entry(&mut self, game_dir: &Path, rel_path: &str) -> Result<&mut OpenScene, AppError> {
        let key = Self::key(game_dir, rel_path);
        if !self.scenes.contains_key(&key) {
            let scene = OpenScene::load(game_dir, rel_path)?;
            self.scenes.insert(key.clone(), scene);
        }
        self.scenes.get_mut(&key).ok_or_else(|| AppError {
            message: format!("{rel_path} is not open."),
            hint: Some("Open the scene first.".to_owned()),
        })
    }

    /// The state of an open scene, or a placeholder when it somehow is not open. Used on
    /// the batch failure paths, where the live borrow of the scene has already been given up.
    fn state_or_empty(&self, game_dir: &Path, rel_path: &str) -> EngineSceneState {
        self.state(game_dir, rel_path)
            .unwrap_or_else(|| EngineSceneState {
                scene_path: rel_path.to_owned(),
                name: String::new(),
                kind: bhippi_engine::document::SceneKind::Empty,
                settings: bhippi_engine::document::SceneSettings::default(),
                entity_count: 0,
                dirty: false,
                can_undo: false,
                can_redo: false,
                undo_label: None,
                redo_label: None,
                revision: 0,
                selection: Vec::new(),
                disk_conflict: false,
                recovery_available: false,
                document_json: String::new(),
            })
    }

    /// The state of an already-open scene without touching the disk.
    pub fn state(&self, game_dir: &Path, rel_path: &str) -> Option<EngineSceneState> {
        self.scenes
            .get(&Self::key(game_dir, rel_path))
            .map(OpenScene::state)
    }

    /// Is this scene open with unsaved edits? Used to decide whether an on-disk change
    /// underneath us is a conflict worth reporting.
    #[must_use]
    pub fn is_dirty(&self, game_dir: &Path, rel_path: &str) -> bool {
        self.scenes
            .get(&Self::key(game_dir, rel_path))
            .is_some_and(|scene| scene.dirty)
    }

    /// Every open scene under a game folder that has unsaved work.
    #[must_use]
    pub fn dirty_scenes(&self, game_dir: &Path) -> Vec<String> {
        let prefix = game_dir.to_string_lossy().replace('\\', "/");
        self.scenes
            .iter()
            .filter(|((root, _), scene)| root == &prefix && scene.dirty)
            .map(|((_, rel), _)| rel.clone())
            .collect()
    }

    /// Apply one already-parsed action as its own transaction.
    ///
    /// `autosave` writes the scene straight back to disk. The AI edits with it on (an
    /// agent has no Save button and its work must survive the turn); interactive editing
    /// leaves it off so the user still owns when the file changes.
    pub fn apply_action(
        &mut self,
        game_dir: &Path,
        rel_path: &str,
        action: &EngineAction,
        actor: EngineActor,
        label: &str,
        autosave: bool,
    ) -> Result<AppliedEdit, AppError> {
        let scene = self.entry(game_dir, rel_path)?;
        if scene.interaction.is_some() {
            return Err(AppError {
                message: "An interactive edit is still in progress.".to_owned(),
                hint: Some("Finish the drag before applying another change.".to_owned()),
            });
        }
        let ops = action.into_ops(&scene.doc).map_err(super::engine_error)?;
        scene.commit_ops(ops, actor, label)?;
        if autosave {
            scene.write(game_dir)?;
        }
        scene.applied_edit(actor)
    }

    /// Apply a whole batch as **one** transaction (ENG-111).
    ///
    /// `resolve` is handed each raw action payload together with the scene as it stands
    /// *after* the preceding actions, so `{"entity":"Crate"}` resolves against a crate the
    /// batch itself just spawned. Nothing is written unless every action lowers cleanly.
    /// Apply a journalled transaction's stored inverse as a fresh, undoable transaction
    /// (ENG-189).
    ///
    /// Deliberately **not** a rollback: reverting an agent's change is itself a change the
    /// user should be able to take back, so it lands on the same undo stack, gets its own
    /// journal row, and reads as "Undo AI change: <label>" in the history. A silent rewind
    /// that could not itself be undone would be the more surprising behaviour by far.
    pub fn revert_journalled(
        &mut self,
        game_dir: &Path,
        rel_path: &str,
        label: &str,
        inverse_json: &str,
    ) -> Result<EngineSceneState, AppError> {
        let ops: Vec<Op> = serde_json::from_str(inverse_json).map_err(|error| AppError {
            message: format!("That journal entry's inverse could not be read: {error}"),
            hint: Some(
                "The journal row may predate the current op format; undo it from the undo                  stack instead."
                    .to_owned(),
            ),
        })?;
        if ops.is_empty() {
            return Err(AppError {
                message: "That change recorded no inverse, so it cannot be undone.".to_owned(),
                hint: Some(
                    "Asset-only changes are journaled but not undoable; delete the file                      directly if that is what you want."
                        .to_owned(),
                ),
            });
        }
        let scene = self.entry(game_dir, rel_path)?;
        scene.commit_ops(ops, EngineActor::User, &format!("undo AI change: {label}"))?;
        scene.write(game_dir)?;
        Ok(scene.state())
    }

    pub fn apply_batch(
        &mut self,
        request: BatchRequest<'_>,
        mut resolve: impl FnMut(&SceneDocument, &serde_json::Value) -> Result<BatchStep, AppError>,
    ) -> Result<AppliedBatch, AppError> {
        let BatchRequest {
            game_dir,
            rel_path,
            label,
            actions: raw_actions,
            actor,
            autosave,
            owner,
            base_revision,
        } = request;
        let scene = self.entry(game_dir, rel_path)?;
        if scene.interaction.is_some() {
            return Err(AppError {
                message: "An interactive edit is still in progress.".to_owned(),
                hint: Some("Finish the drag before applying a batch.".to_owned()),
            });
        }
        scene.claim(actor, owner, base_revision)?;
        if raw_actions.is_empty() {
            return Err(AppError {
                message: "The batch contains no actions.".to_owned(),
                hint: Some(
                    "Add at least one action, or say what you decided not to do.".to_owned(),
                ),
            });
        }

        // Resolve, lower and validate the whole batch against a scratch copy first. This is
        // the dry run: it produces the outcome envelope and never touches the live document.
        //
        // The ops it produces are the ones that get committed. Lowering a second time
        // against the live document would look tidier and be wrong: `spawn` mints a fresh
        // `EntityId` on every lowering, so the second pass would create entities that the
        // batch's own later actions — resolved against the first pass — no longer refer to.
        let mut scratch = scene.doc.clone();
        let mut ops: Vec<Op> = Vec::new();
        let mut outcomes: Vec<EngineActionOutcome> = Vec::with_capacity(raw_actions.len());
        // Files written during the dry run are applied immediately, because a scene action
        // later in the batch may legitimately reference the asset a content action just
        // created. `rollback` puts them back if anything fails.
        let mut files: Vec<FileChange> = Vec::new();
        let rollback = |files: &[FileChange]| {
            for file in files.iter().rev() {
                if let Err(error) = file.revert(game_dir) {
                    tracing::warn!(path = %file.rel_path, message = %error.message, "could not roll back an asset write");
                }
            }
        };

        for (index, raw) in raw_actions.iter().enumerate() {
            let step = match resolve(&scratch, raw) {
                Ok(step) => step,
                Err(error) => {
                    rollback(&files);
                    outcomes.push(EngineActionOutcome {
                        index: index as u32,
                        ok: false,
                        label: raw
                            .get("kind")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("unknown")
                            .to_owned(),
                        message: error.message,
                        hint: error.hint,
                        schema_excerpt: None,
                    });
                    return Ok(AppliedBatch {
                        result: EngineBatchResult {
                            applied: false,
                            label: label.to_owned(),
                            scene_path: rel_path.to_owned(),
                            outcomes,
                            edit: None,
                            state: scene.state(),
                        },
                        journal: None,
                    });
                }
            };
            let action = match step {
                BatchStep::Content(content) => match content.prepare(game_dir, &scratch) {
                    Ok(outcome) => {
                        if let Err(error) = outcome.file.apply(game_dir) {
                            rollback(&files);
                            outcomes.push(EngineActionOutcome {
                                index: index as u32,
                                ok: false,
                                label: outcome.label,
                                message: error.message,
                                hint: error.hint,
                                schema_excerpt: None,
                            });
                            return Ok(AppliedBatch {
                                result: EngineBatchResult {
                                    applied: false,
                                    label: label.to_owned(),
                                    scene_path: rel_path.to_owned(),
                                    outcomes,
                                    edit: None,
                                    state: self.state_or_empty(game_dir, rel_path),
                                },
                                journal: None,
                            });
                        }
                        outcomes.push(EngineActionOutcome {
                            index: index as u32,
                            ok: true,
                            label: outcome.label,
                            message: format!("wrote {}", outcome.asset_ref),
                            hint: None,
                            schema_excerpt: None,
                        });
                        files.push(outcome.file);
                        ops.extend(outcome.ops);
                        continue;
                    }
                    Err(error) => {
                        rollback(&files);
                        outcomes.push(EngineActionOutcome {
                            index: index as u32,
                            ok: false,
                            label: content.to_label(),
                            message: error.message,
                            hint: error.hint,
                            schema_excerpt: None,
                        });
                        return Ok(AppliedBatch {
                            result: EngineBatchResult {
                                applied: false,
                                label: label.to_owned(),
                                scene_path: rel_path.to_owned(),
                                outcomes,
                                edit: None,
                                state: self.state_or_empty(game_dir, rel_path),
                            },
                            journal: None,
                        });
                    }
                },
                BatchStep::Scene(action) => *action,
            };
            let single = EngineActionBatch {
                label: action.to_label(),
                actions: vec![action.clone()],
            };
            match single.lower(&scratch) {
                Ok(lowered) => {
                    let mut staged = bhippi_engine::EngineTransaction {
                        id: bhippi_types::TransactionId::new(),
                        label: action.to_label(),
                        actor,
                        ops: lowered.clone(),
                        inverse: Vec::new(),
                        touched: Vec::new(),
                        scene: None,
                    };
                    if let Err(error) = staged.apply(&mut scratch) {
                        rollback(&files);
                        outcomes.push(failed_outcome(index, &action, &error));
                        return Ok(AppliedBatch {
                            result: EngineBatchResult {
                                applied: false,
                                label: label.to_owned(),
                                scene_path: rel_path.to_owned(),
                                outcomes,
                                edit: None,
                                state: scene.state(),
                            },
                            journal: None,
                        });
                    }
                    outcomes.push(EngineActionOutcome {
                        index: index as u32,
                        ok: true,
                        label: action.to_label(),
                        message: "ok".to_owned(),
                        hint: None,
                        schema_excerpt: None,
                    });
                    ops.extend(lowered);
                }
                Err(failure) => {
                    rollback(&files);
                    outcomes.push(failed_outcome(index, &action, &failure.error));
                    return Ok(AppliedBatch {
                        result: EngineBatchResult {
                            applied: false,
                            label: label.to_owned(),
                            scene_path: rel_path.to_owned(),
                            outcomes,
                            edit: None,
                            state: scene.state(),
                        },
                        journal: None,
                    });
                }
            }
        }

        // The dry run passed. Commit the ops it produced — in order, as one transaction
        // against the live document, which is still exactly what `scratch` started from
        // because the session lock has not been released.
        let display_label = if label.trim().is_empty() {
            format!("{} engine actions", outcomes.len())
        } else {
            label.to_owned()
        };
        scene.pending_files = files;
        scene.commit_ops(ops, actor, &display_label)?;
        // The holder's own commit is not interference: point its lease at where it just
        // left the scene, so its next batch is not refused for its own work (ENG-192).
        scene.refresh_lease_for(actor, owner);
        if autosave {
            scene.write(game_dir)?;
        }
        let applied = scene.applied_edit(actor)?;
        Ok(AppliedBatch {
            result: EngineBatchResult {
                applied: true,
                label: display_label,
                scene_path: rel_path.to_owned(),
                outcomes,
                state: applied.result.state.clone(),
                edit: Some(applied.result),
            },
            // The batch is one transaction, so it journals exactly like a single edit —
            // full ops and inverse included, which is what makes it replayable.
            journal: Some(applied.journal),
        })
    }

    /// Begin an interactive edit (a gizmo drag, a slider). Ops recorded after this apply
    /// live but produce **one** undo entry when committed.
    pub fn begin_interaction(
        &mut self,
        game_dir: &Path,
        rel_path: &str,
        label: &str,
    ) -> Result<EngineSceneState, AppError> {
        let scene = self.entry(game_dir, rel_path)?;
        if scene.interaction.is_some() {
            return Err(AppError {
                message: "An interactive edit is already in progress.".to_owned(),
                hint: Some("Commit or cancel it first.".to_owned()),
            });
        }
        scene.interaction = Some(LiveInteraction {
            session: Session::begin(label.to_owned(), EngineActor::User),
            label: label.to_owned(),
        });
        Ok(scene.state())
    }

    /// Record one action into the live interaction, applying it immediately.
    pub fn record_interaction(
        &mut self,
        game_dir: &Path,
        rel_path: &str,
        action: &EngineAction,
    ) -> Result<EngineSceneState, AppError> {
        let scene = self.entry(game_dir, rel_path)?;
        let ops = action.into_ops(&scene.doc).map_err(super::engine_error)?;
        let Some(interaction) = scene.interaction.as_mut() else {
            return Err(AppError {
                message: "No interactive edit is in progress.".to_owned(),
                hint: Some("Call engine_begin_interaction first.".to_owned()),
            });
        };
        for op in ops {
            interaction
                .session
                .record(&mut scene.doc, op)
                .map_err(super::engine_error)?;
        }
        scene.dirty = true;
        scene.revision = scene.revision.wrapping_add(1);
        scene.write_recovery()?;
        Ok(scene.state())
    }

    /// Finish an interactive edit: one transaction, one undo entry, one journal row.
    /// A drag that never moved anything commits nothing and is not an error.
    pub fn commit_interaction(
        &mut self,
        game_dir: &Path,
        rel_path: &str,
    ) -> Result<Option<AppliedEdit>, AppError> {
        let scene = self.entry(game_dir, rel_path)?;
        let Some(interaction) = scene.interaction.take() else {
            return Err(AppError {
                message: "No interactive edit is in progress.".to_owned(),
                hint: Some("Call engine_begin_interaction first.".to_owned()),
            });
        };
        if !interaction.session.is_dirty() {
            return Ok(None);
        }
        let txn = interaction
            .session
            .commit(&scene.doc)
            .map_err(super::engine_error)?;
        scene.last = Some(txn.clone());
        scene.undo.push(txn);
        scene.dirty = true;
        scene.revision = scene.revision.wrapping_add(1);
        scene.write_recovery()?;
        scene.applied_edit(EngineActor::User).map(Some)
    }

    /// Abandon a live interaction by undoing what it applied so far.
    pub fn cancel_interaction(
        &mut self,
        game_dir: &Path,
        rel_path: &str,
    ) -> Result<EngineSceneState, AppError> {
        let scene = self.entry(game_dir, rel_path)?;
        let Some(interaction) = scene.interaction.take() else {
            return Ok(scene.state());
        };
        if interaction.session.is_dirty() {
            // Commit into a throwaway stack, then immediately reverse it — the session
            // already applied its ops live, so this is the only way back.
            if let Ok(txn) = interaction.session.commit(&scene.doc) {
                let mut stack = UndoStack::new();
                stack.push(txn);
                let _ = stack.undo(&mut scene.doc);
            }
        }
        scene.revision = scene.revision.wrapping_add(1);
        if scene.dirty {
            scene.write_recovery()?;
        }
        Ok(scene.state())
    }

    /// Undo the last transaction on a scene — the same stack for user and agent edits, so
    /// Ctrl+Z reverses an AI batch exactly like it reverses a drag.
    pub fn undo(&mut self, game_dir: &Path, rel_path: &str) -> Result<EngineSceneState, AppError> {
        let scene = self.entry(game_dir, rel_path)?;
        // Read the id before undoing: the transaction moves to the redo line keeping it.
        let id = scene.undo.peek_undo().map(|txn| txn.id);
        scene
            .undo
            .undo(&mut scene.doc)
            .map_err(super::engine_error)?;
        if let Some(id) = id {
            if let Some(files) = scene.undo_files.remove(&id) {
                // Reverse order, so a batch that wrote a file and then overwrote it lands
                // back on the bytes it started from.
                for file in files.iter().rev() {
                    file.revert(game_dir)?;
                }
                scene.redo_files.insert(id, files);
            }
        }
        scene.dirty = true;
        scene.revision = scene.revision.wrapping_add(1);
        scene.write_recovery()?;
        Ok(scene.state())
    }

    pub fn redo(&mut self, game_dir: &Path, rel_path: &str) -> Result<EngineSceneState, AppError> {
        let scene = self.entry(game_dir, rel_path)?;
        // `EngineTransaction::redo` mints a fresh id, so the file ledger has to be re-keyed
        // from the old id to the new one.
        let old = scene.undo.peek_redo().map(|txn| txn.id);
        scene
            .undo
            .redo(&mut scene.doc)
            .map_err(super::engine_error)?;
        let new = scene.undo.peek_undo().map(|txn| txn.id);
        if let (Some(old), Some(new)) = (old, new) {
            if let Some(files) = scene.redo_files.remove(&old) {
                for file in &files {
                    file.apply(game_dir)?;
                }
                scene.undo_files.insert(new, files);
            }
        }
        scene.dirty = true;
        scene.revision = scene.revision.wrapping_add(1);
        scene.write_recovery()?;
        Ok(scene.state())
    }

    /// Write one open scene to disk.
    pub fn save(&mut self, game_dir: &Path, rel_path: &str) -> Result<EngineSceneState, AppError> {
        let scene = self.entry(game_dir, rel_path)?;
        scene.write(game_dir)?;
        Ok(scene.state())
    }

    /// Write every dirty scene under a game folder; returns what was written.
    pub fn save_all(&mut self, game_dir: &Path) -> Result<Vec<String>, AppError> {
        let mut written = Vec::new();
        for rel in self.dirty_scenes(game_dir) {
            self.save(game_dir, &rel)?;
            written.push(rel);
        }
        Ok(written)
    }

    /// Replace the selection. Selection is engine state so the AI can read what the user
    /// is looking at (`get_selection`) and act on "this one".
    pub fn set_selection(
        &mut self,
        game_dir: &Path,
        rel_path: &str,
        selection: Vec<EntityId>,
    ) -> Result<EngineSceneState, AppError> {
        let scene = self.entry(game_dir, rel_path)?;
        scene.selection = selection
            .into_iter()
            .filter(|id| scene.doc.entity(*id).is_some())
            .collect();
        Ok(scene.state())
    }

    /// A borrowed document for the read-only query surface.
    #[must_use]
    pub fn document(&self, game_dir: &Path, rel_path: &str) -> Option<&SceneDocument> {
        self.scenes
            .get(&Self::key(game_dir, rel_path))
            .map(|scene| &scene.doc)
    }
}

impl OpenScene {
    /// Apply ops as one transaction and push it on the undo stack.
    fn commit_ops(
        &mut self,
        ops: Vec<Op>,
        actor: EngineActor,
        label: &str,
    ) -> Result<(), AppError> {
        let id = TransactionId::new();
        let mut txn = EngineTransaction {
            id,
            label: label.to_owned(),
            actor,
            ops: stamp_provenance(ops, actor, id),
            inverse: Vec::new(),
            touched: Vec::new(),
            scene: None,
        };
        txn.apply(&mut self.doc).map_err(super::engine_error)?;
        let id = txn.id;
        self.last = Some(txn.clone());
        let staged = std::mem::take(&mut self.pending_files);
        let recorded = !txn.ops.is_empty();
        self.undo.push(txn);
        if !staged.is_empty() {
            if recorded {
                self.undo_files.insert(id, staged);
            } else {
                // `UndoStack` skips a transaction with no scene ops, so an asset-only
                // change has no undo entry to hang files from. It is still applied and
                // journaled; it is simply not on the undo stack — the same place Unreal
                // leaves "create asset". Pairing the creation with the assignment that
                // uses it (the normal case) puts both back on one Ctrl+Z.
                tracing::debug!(
                    files = staged.len(),
                    "asset-only transaction: journaled, not undoable"
                );
            }
        }
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
        self.write_recovery()?;
        Ok(())
    }

    /// Build the result + journal facts for the transaction just applied.
    fn applied_edit(&self, actor: EngineActor) -> Result<AppliedEdit, AppError> {
        let txn = self.last.as_ref().ok_or_else(|| AppError {
            message: "No transaction was applied.".to_owned(),
            hint: Some("Report this as an engine bug.".to_owned()),
        })?;
        let actor_name = match actor {
            EngineActor::User => "user",
            EngineActor::Agent => "agent",
            // `engine_journal.actor` only admits user/agent; system work (scaffolds,
            // migrations) is recorded as the app acting for the user, not as a third
            // party the history list would have to explain.
            EngineActor::System => "user",
        };
        let summary = format!(
            "{} ({} ops, {} entities touched)",
            txn.label,
            txn.ops.len(),
            txn.touched.len()
        );
        Ok(AppliedEdit {
            journal: JournalFacts {
                scene_rel_path: self.rel_path.clone(),
                txn_id: txn.id.to_string(),
                actor: actor_name.to_owned(),
                label: txn.label.clone(),
                ops_json: serde_json::to_string(&txn.ops).unwrap_or_else(|_| "[]".to_owned()),
                inverse_json: serde_json::to_string(&txn.inverse)
                    .unwrap_or_else(|_| "[]".to_owned()),
                touched_json: serde_json::to_string(&txn.touched)
                    .unwrap_or_else(|_| "[]".to_owned()),
                op_count: txn.ops.len() as i64,
            },
            result: EngineEditResult {
                scene_path: self.rel_path.clone(),
                txn_id: txn.id.to_string(),
                actor: actor_name.to_owned(),
                label: txn.label.clone(),
                summary,
                op_count: txn.ops.len() as u32,
                touched: txn.touched.clone(),
                state: self.state(),
                revision: None,
            },
        })
    }
}

/// Resolve a scene path against a game folder, refusing anything that escapes it.
pub fn safe_scene_path(game_dir: &Path, rel: &str) -> Result<(PathBuf, String), AppError> {
    let normalised = rel.trim().replace('\\', "/");
    if normalised.is_empty() {
        return Err(AppError::plain("No scene path was given."));
    }
    if normalised.contains("..") || Path::new(&normalised).is_absolute() {
        return Err(AppError::plain("That scene path leaves the game folder."));
    }
    Ok((game_dir.join(&normalised), normalised))
}

/// Build the rejection envelope for one action, attaching the schema for the component it
/// named so the model's next attempt has the real field list.
fn failed_outcome(
    index: usize,
    action: &EngineAction,
    error: &bhippi_engine::EngineError,
) -> EngineActionOutcome {
    let schema_excerpt = action
        .component_name()
        .and_then(bhippi_engine::schema::excerpt);
    EngineActionOutcome {
        index: index as u32,
        ok: false,
        label: action.to_label(),
        message: error.to_string(),
        hint: error.hint().map(str::to_owned),
        schema_excerpt,
    }
}

/// Record who created each spawned entity, and in which transaction (ENG-127).
///
/// Stamped here rather than in the action layer for one reason: the transaction id does not
/// exist until this point. Doing it in one place also means every spawn is covered — a new
/// verb that spawns something cannot forget to opt in.
fn stamp_provenance(ops: Vec<Op>, actor: EngineActor, txn: TransactionId) -> Vec<Op> {
    let created_by = match actor {
        EngineActor::User => "user",
        EngineActor::Agent => "agent",
        EngineActor::System => "system",
    };
    let at = chrono::Utc::now().to_rfc3339();
    ops.into_iter()
        .map(|op| match op {
            Op::Spawn { mut entity, parent } => {
                // A prefab instance or an import may already carry provenance from where it
                // came from; the first author is the interesting one, so it is not replaced.
                entity
                    .components
                    .entry("Provenance".to_owned())
                    .or_insert_with(|| {
                        serde_json::json!({
                            "created_by": created_by,
                            "txn": txn.to_string(),
                            "at": at,
                        })
                    });
                Op::Spawn { entity, parent }
            }
            other => other,
        })
        .collect()
}
