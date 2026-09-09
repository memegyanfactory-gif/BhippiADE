//! The live channel from Bhippi to the embedded Godot editor (GAD-170, ADR-0050).
//!
//! The studio viewport **is** a real Godot editor window re-parented into Bhippi's own
//! window (ADR-0045), and the agent's typed actions land as file writes underneath it. Godot
//! only rescans its filesystem when its window gains focus — and a window that is a child of
//! Bhippi's window may never gain one — so until now the editor sat on whatever it happened
//! to be showing while the agent built a level next to it. The user watched an empty viewport
//! and had to take Bhippi's word for it that anything was happening.
//!
//! This module is Bhippi's half of the fix: one small file, `.bhippi/live/editor.json`,
//! rewritten after every applied batch. `addons/bhippi_studio/plugin.gd` polls it and, when
//! the sequence number moves, rescans the filesystem, opens or reloads the scene the batch
//! touched and selects the nodes it wrote. The editor then shows the work as it happens.
//!
//! Three properties this file has to have, and why:
//!
//! - **Hidden from Godot.** `.bhippi/` starts with a dot, and Godot's `EditorFileSystem`
//!   skips dot-directories — so a file rewritten several times a second never triggers a
//!   reimport, never grows a `.import` sibling and never reaches an export.
//! - **Written whole or not at all.** The addon reads it on a timer, so a half-written file
//!   would be read as a truncated one. [`announce`] writes a sibling temp file and renames
//!   it over the target, which is atomic on both platforms Bhippi runs on.
//! - **Sequenced, not timestamped.** The addon replays nothing it has already applied, and
//!   a monotonic counter says that without depending on two clocks agreeing. A signal from a
//!   previous session is *read* at editor startup (to know which scene to open) but never
//!   replayed as if it had just happened.
//!
//! Pure and headless: nothing here spawns Godot or talks to the editor. It writes a file.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// The directory the live channel lives in, project-relative.
pub const LIVE_DIR_REL: &str = ".bhippi/live";
/// The signal file, project-relative. Forward slashes: this is also what the addon joins.
pub const LIVE_SIGNAL_REL: &str = ".bhippi/live/editor.json";
/// The temp file [`announce`] renames over the signal.
pub const LIVE_SIGNAL_TMP_REL: &str = ".bhippi/live/editor.json.tmp";
/// The schema version the addon checks. A signal it does not recognise is ignored, which is
/// how an older addon inside a user's project fails quiet rather than wrong.
pub const LIVE_SIGNAL_VERSION: u32 = 1;
/// How often the addon reads the signal, in milliseconds. Mirrored by `studio_plugin.gd`;
/// the round-trip test in `scaffold` pins the two together.
pub const LIVE_POLL_MS: u32 = 250;
/// The most nodes one signal asks the editor to select. A batch that adds a hundred nodes
/// would otherwise leave the Inspector showing a hundred-node multi-selection, which says
/// less than showing the first few.
pub const LIVE_FOCUS_MAX: usize = 8;

/// What Bhippi tells the editor about one applied batch.
///
/// Every path is project-relative with forward slashes — the addon prefixes `res://` — and
/// every node path is relative to the scene root (`"."` is the root itself), which is the
/// same shape [`super::action::GodotAction::node_path`] produces.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct LiveSignal {
    /// [`LIVE_SIGNAL_VERSION`] at the time of writing.
    pub version: u32,
    /// Monotonic per project. The addon applies a signal only when this moves forward.
    pub seq: u64,
    /// `user` | `agent` — the same word the journal row carries.
    pub actor: String,
    /// The batch's own label, so the editor can say whose change it just showed.
    pub label: String,
    /// The journal transaction this signal belongs to.
    pub txn_id: String,
    /// The scene the editor should be looking at, when the batch touched one.
    pub scene: Option<String>,
    /// Every file the batch wrote, so the addon can narrow its rescan if it ever wants to.
    pub changed_files: Vec<String>,
    /// The nodes to select once the scene is open, capped at [`LIVE_FOCUS_MAX`].
    pub focus_nodes: Vec<String>,
}

/// One announcement, before a sequence number is assigned to it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveEdit {
    pub actor: String,
    pub label: String,
    pub txn_id: String,
    pub scene: Option<String>,
    pub changed_files: Vec<String>,
    pub focus_nodes: Vec<String>,
}

/// Where the signal lives inside `root`.
#[must_use]
pub fn signal_path(root: &Path) -> PathBuf {
    root.join(LIVE_SIGNAL_REL)
}

/// The signal currently on disk, or `None` when there is none, it cannot be read, or it does
/// not parse.
///
/// A signal that does not parse is not an error the caller can act on: the next [`announce`]
/// overwrites it. Refusing to write because the previous write is unreadable would leave the
/// editor stuck on a corrupt file for the rest of the session.
#[must_use]
pub fn read_signal(root: &Path) -> Option<LiveSignal> {
    let text = std::fs::read_to_string(signal_path(root)).ok()?;
    let signal: LiveSignal = serde_json::from_str(&text).ok()?;
    (signal.version == LIVE_SIGNAL_VERSION).then_some(signal)
}

/// The scene the editor should be showing, given the files a batch changed.
///
/// The **first** `.tscn` in the batch's own order, not "the only one": a batch that creates
/// a scene and then instances it into another has two, and the one the agent started with is
/// the one the sentence was about. `None` when the batch touched no scene at all — a pure
/// script or project-settings batch leaves the editor where it is rather than jumping it
/// somewhere arbitrary.
#[must_use]
pub fn focus_scene(changed_files: &[String]) -> Option<String> {
    changed_files
        .iter()
        .find(|path| path.ends_with(".tscn"))
        .cloned()
}

/// Write the next signal. Returns what was written, with the sequence number it was given.
///
/// The sequence continues from whatever is on disk, so it survives a restart of Bhippi
/// without the addon ever seeing it go backwards.
pub fn announce(root: &Path, edit: &LiveEdit) -> Result<LiveSignal> {
    let seq = read_signal(root).map_or(1, |previous| previous.seq.saturating_add(1));
    let mut focus_nodes = edit.focus_nodes.clone();
    focus_nodes.truncate(LIVE_FOCUS_MAX);
    let signal = LiveSignal {
        version: LIVE_SIGNAL_VERSION,
        seq,
        actor: edit.actor.clone(),
        label: edit.label.clone(),
        txn_id: edit.txn_id.clone(),
        scene: edit.scene.clone(),
        changed_files: edit.changed_files.clone(),
        focus_nodes,
    };
    write_signal(root, &signal)?;
    Ok(signal)
}

fn write_signal(root: &Path, signal: &LiveSignal) -> Result<()> {
    let directory = root.join(LIVE_DIR_REL);
    std::fs::create_dir_all(&directory).map_err(|error| io(&directory, &error))?;
    let text = serde_json::to_string(signal).map_err(|error| EngineError::Io {
        operation: "live signal",
        path: LIVE_SIGNAL_REL.to_owned(),
        reason: error.to_string(),
        hint: Some("This is a Bhippi bug: the live signal must always serialise.".to_owned()),
    })?;
    // Whole or not at all: the addon reads this on a timer and would otherwise read a
    // truncated file. `rename` replaces the destination on Windows as well as on Unix.
    let temporary = root.join(LIVE_SIGNAL_TMP_REL);
    std::fs::write(&temporary, text.as_bytes()).map_err(|error| io(&temporary, &error))?;
    let target = signal_path(root);
    std::fs::rename(&temporary, &target).map_err(|error| {
        let _ignored = std::fs::remove_file(&temporary);
        io(&target, &error)
    })
}

fn io(path: &Path, error: &std::io::Error) -> EngineError {
    EngineError::Io {
        operation: "live signal",
        path: path.display().to_string(),
        reason: error.to_string(),
        hint: Some(
            "The editor follows this file to show the agent's work; check the project folder \
             is writable."
                .to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        announce, focus_scene, read_signal, signal_path, LiveEdit, LIVE_FOCUS_MAX, LIVE_SIGNAL_REL,
        LIVE_SIGNAL_VERSION,
    };
    use std::path::PathBuf;

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("bhippi-godot-live-{name}"));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("temp root");
            Self(root)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn edit(label: &str, scene: &str) -> LiveEdit {
        LiveEdit {
            actor: "agent".to_owned(),
            label: label.to_owned(),
            txn_id: "01J".to_owned(),
            scene: Some(scene.to_owned()),
            changed_files: vec![scene.to_owned()],
            focus_nodes: vec!["Player".to_owned()],
        }
    }

    #[test]
    fn the_first_signal_starts_at_one_and_every_later_one_moves_forward() {
        let root = TempRoot::new("seq");
        assert!(read_signal(&root.0).is_none(), "nothing is announced yet");
        let first = announce(&root.0, &edit("Add Player", "scenes/main.tscn")).expect("first");
        assert_eq!(first.seq, 1);
        assert_eq!(first.version, LIVE_SIGNAL_VERSION);
        let second = announce(&root.0, &edit("Add Lamp", "scenes/main.tscn")).expect("second");
        assert_eq!(second.seq, 2);
        // Read back: the addon sees exactly what was written, including the label.
        let read = read_signal(&root.0).expect("a signal on disk");
        assert_eq!(read, second);
        assert_eq!(read.label, "Add Lamp");
    }

    #[test]
    fn the_signal_hides_from_godot_and_leaves_no_temp_file_behind() {
        let root = TempRoot::new("hidden");
        announce(&root.0, &edit("Add Player", "scenes/main.tscn")).expect("announced");
        assert_eq!(LIVE_SIGNAL_REL, ".bhippi/live/editor.json");
        assert!(
            LIVE_SIGNAL_REL.starts_with('.'),
            "Godot's EditorFileSystem skips dot-directories; the signal must live in one"
        );
        assert!(signal_path(&root.0).is_file());
        assert!(
            !root.0.join(".bhippi/live/editor.json.tmp").exists(),
            "the temp file is renamed over the target, never left beside it"
        );
    }

    #[test]
    fn a_corrupt_signal_is_ignored_and_the_next_announcement_replaces_it() {
        let root = TempRoot::new("corrupt");
        announce(&root.0, &edit("Add Player", "scenes/main.tscn")).expect("announced");
        std::fs::write(signal_path(&root.0), b"{ not json").expect("corrupt it");
        assert!(read_signal(&root.0).is_none());
        // The counter restarts rather than the write failing: a stuck editor is worse than
        // a sequence that repeats a number no addon is still holding.
        let next = announce(&root.0, &edit("Add Lamp", "scenes/main.tscn")).expect("next");
        assert_eq!(next.seq, 1);
        assert!(read_signal(&root.0).is_some());
    }

    #[test]
    fn a_signal_from_a_future_version_is_not_read_as_this_one() {
        let root = TempRoot::new("version");
        announce(&root.0, &edit("Add Player", "scenes/main.tscn")).expect("announced");
        let text = std::fs::read_to_string(signal_path(&root.0)).expect("read");
        let bumped = text.replace(
            &format!("\"version\":{LIVE_SIGNAL_VERSION}"),
            "\"version\":99",
        );
        assert_ne!(bumped, text, "the version field must be in the JSON");
        std::fs::write(signal_path(&root.0), bumped).expect("write");
        assert!(read_signal(&root.0).is_none());
    }

    #[test]
    fn the_selection_is_capped_so_one_batch_cannot_fill_the_inspector() {
        let root = TempRoot::new("cap");
        let mut wide = edit("Build the level", "scenes/main.tscn");
        wide.focus_nodes = (0..64).map(|index| format!("Tile{index}")).collect();
        let signal = announce(&root.0, &wide).expect("announced");
        assert_eq!(signal.focus_nodes.len(), LIVE_FOCUS_MAX);
        assert_eq!(signal.focus_nodes[0], "Tile0");
    }

    #[test]
    fn the_scene_to_show_is_the_first_the_batch_touched_and_a_scriptless_batch_names_none() {
        assert_eq!(
            focus_scene(&[
                "scripts/player.gd".to_owned(),
                "scenes/level.tscn".to_owned(),
                "scenes/main.tscn".to_owned(),
            ]),
            Some("scenes/level.tscn".to_owned()),
            "the scene the batch started with, not the only one"
        );
        assert_eq!(
            focus_scene(&["scripts/player.gd".to_owned(), "project.godot".to_owned()]),
            None,
            "a batch that touched no scene leaves the editor where it is"
        );
    }
}
