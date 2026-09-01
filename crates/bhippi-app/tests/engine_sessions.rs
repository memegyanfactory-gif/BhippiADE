//! ENG-109 — the Phase 0 acceptance tests.
//!
//! The defect these exist to prevent: the editor used to hold the scene in webview state
//! and write the whole file, while the agent went through `EngineTransaction`. Whoever
//! wrote last won, and the two had separate undo. These tests drive the real session store
//! the way the IPC commands and the chat bridge do, and assert the properties that make
//! INV-070 true rather than aspirational.

// Tests may panic on purpose: `expect` states a precondition, and a panic here is a failing
// test rather than a crashed app. The workspace-wide `deny` stands in shipping code.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_app::engine::session::{BatchRequest, EngineSessions};
use bhippi_engine::action::EngineAction;
use bhippi_engine::document::SceneDocument;
use bhippi_engine::scaffold;
use bhippi_types::{EngineActor, EntityId};
use std::path::{Path, PathBuf};

fn temp_game(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bhippi-engine-{label}-{}", EntityId::new()));
    std::fs::create_dir_all(dir.join("assets/scenes")).expect("scene folder");
    let scene = scaffold::starter_scene();
    std::fs::write(
        dir.join("assets/scenes/level_01.bscn.json"),
        scene.dump().expect("dump"),
    )
    .expect("write scene");
    dir
}

const LEVEL: &str = "assets/scenes/level_01.bscn.json";

fn read_scene(game_dir: &Path) -> SceneDocument {
    let text = std::fs::read_to_string(game_dir.join(LEVEL)).expect("scene readable");
    SceneDocument::parse_lenient(&text).expect("scene parses")
}

fn spawn(template: &str, name: &str) -> EngineAction {
    EngineAction::Spawn {
        template: template.to_owned(),
        at: None,
        parent: None,
        name: Some(name.to_owned()),
    }
}

fn first_named(sessions: &EngineSessions, game_dir: &Path, name: &str) -> EntityId {
    sessions
        .document(game_dir, LEVEL)
        .expect("scene open")
        .entities
        .iter()
        .find(|entity| entity.name == name)
        .unwrap_or_else(|| panic!("no entity named {name}"))
        .id
}

#[test]
fn crash_snapshot_is_offered_replayed_and_cleared_only_after_save() {
    let game_dir = temp_game("recovery");
    let mut first_process = EngineSessions::new();
    let edited = first_process
        .apply_action(
            &game_dir,
            LEVEL,
            &spawn("cube", "RecoveredCrate"),
            EngineActor::User,
            "unsaved recovery edit",
            false,
        )
        .expect("edit applies");
    assert!(edited.result.state.recovery_available);
    assert!(!read_scene(&game_dir)
        .entities
        .iter()
        .any(|entity| entity.name == "RecoveredCrate"));

    // A new store represents reopening after the app was killed.
    let mut reopened = EngineSessions::new();
    let opened = reopened.open(&game_dir, LEVEL).expect("disk scene opens");
    assert!(opened.recovery_available);
    let recovered = reopened
        .recover(&game_dir, LEVEL)
        .expect("snapshot replays");
    assert!(recovered.dirty);
    assert!(reopened
        .document(&game_dir, LEVEL)
        .expect("recovered scene")
        .entities
        .iter()
        .any(|entity| entity.name == "RecoveredCrate"));

    let saved = reopened.save(&game_dir, LEVEL).expect("recovery saves");
    assert!(!saved.recovery_available);
    assert!(read_scene(&game_dir)
        .entities
        .iter()
        .any(|entity| entity.name == "RecoveredCrate"));
    let _ = std::fs::remove_dir_all(&game_dir);
}

/// The headline property: an agent edit lands while the user has unsaved work open, and
/// the user's work is still there afterwards. Before Phase 0 this test could not pass —
/// the agent wrote the file and the pane reloaded over the top of the user's buffer.
#[test]
fn an_agent_edit_does_not_discard_the_users_unsaved_work() {
    let game_dir = temp_game("interleave");
    let mut sessions = EngineSessions::new();

    // The user adds a prop and does NOT save.
    sessions
        .apply_action(
            &game_dir,
            LEVEL,
            &spawn("cube", "UserCrate"),
            EngineActor::User,
            "add cube",
            false,
        )
        .expect("user edit applies");
    assert!(sessions.is_dirty(&game_dir, LEVEL), "user edit is unsaved");
    assert!(
        !read_scene(&game_dir)
            .entities
            .iter()
            .any(|entity| entity.name == "UserCrate"),
        "an unsaved user edit must not be on disk yet"
    );

    // The agent edits the same scene. It autosaves, as the chat bridge does.
    sessions
        .apply_action(
            &game_dir,
            LEVEL,
            &spawn("light", "AgentLamp"),
            EngineActor::Agent,
            "ai:engine_action",
            true,
        )
        .expect("agent edit applies");

    // Both edits are in the live document, and the agent's save carried the user's
    // in-progress work to disk rather than overwriting it.
    let live = sessions.document(&game_dir, LEVEL).expect("scene open");
    assert!(live.entities.iter().any(|e| e.name == "UserCrate"));
    assert!(live.entities.iter().any(|e| e.name == "AgentLamp"));
    let on_disk = read_scene(&game_dir);
    assert!(
        on_disk.entities.iter().any(|e| e.name == "UserCrate"),
        "the user's unsaved entity survived the agent's write"
    );
    assert!(on_disk.entities.iter().any(|e| e.name == "AgentLamp"));

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// One undo stack for both actors: Ctrl+Z after an agent edit reverses the agent's edit,
/// not the user's, and keeps going back through the user's own history.
#[test]
fn undo_spans_user_and_agent_edits_in_order() {
    let game_dir = temp_game("undo");
    let mut sessions = EngineSessions::new();

    sessions
        .apply_action(
            &game_dir,
            LEVEL,
            &spawn("cube", "UserCrate"),
            EngineActor::User,
            "add cube",
            false,
        )
        .expect("user edit");
    let after_user = sessions
        .apply_action(
            &game_dir,
            LEVEL,
            &spawn("light", "AgentLamp"),
            EngineActor::Agent,
            "ai:add a lamp",
            true,
        )
        .expect("agent edit");

    // The undo affordance names the agent's batch — this is what makes "Undo AI Change"
    // legible instead of a bare arrow.
    assert_eq!(
        after_user.result.state.undo_label.as_deref(),
        Some("ai:add a lamp")
    );

    let state = sessions.undo(&game_dir, LEVEL).expect("undo agent edit");
    let live = sessions.document(&game_dir, LEVEL).expect("open");
    assert!(
        !live.entities.iter().any(|e| e.name == "AgentLamp"),
        "undo reversed the agent's edit"
    );
    assert!(
        live.entities.iter().any(|e| e.name == "UserCrate"),
        "and left the user's edit alone"
    );
    assert_eq!(state.undo_label.as_deref(), Some("add cube"));

    sessions.undo(&game_dir, LEVEL).expect("undo user edit");
    let live = sessions.document(&game_dir, LEVEL).expect("open");
    assert!(!live.entities.iter().any(|e| e.name == "UserCrate"));

    // And redo walks back up the same stack.
    sessions.redo(&game_dir, LEVEL).expect("redo");
    let live = sessions.document(&game_dir, LEVEL).expect("open");
    assert!(live.entities.iter().any(|e| e.name == "UserCrate"));

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A gizmo drag records many ops and must land as exactly one undo step (ENG-102).
#[test]
fn an_interactive_drag_is_one_undo_step() {
    let game_dir = temp_game("drag");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");
    let player = first_named(&sessions, &game_dir, "Player");

    sessions
        .begin_interaction(&game_dir, LEVEL, "move Player")
        .expect("begin");
    for step in 1..=8 {
        let x = step as f32;
        sessions
            .record_interaction(
                &game_dir,
                LEVEL,
                &EngineAction::SetTransform {
                    entity: player,
                    pos: Some([x, 1.0, 0.0]),
                    rot: None,
                    scale: None,
                },
            )
            .expect("record drag frame");
    }
    let applied = sessions
        .commit_interaction(&game_dir, LEVEL)
        .expect("commit")
        .expect("the drag moved something");
    assert_eq!(applied.result.op_count, 8, "every frame is in the record");
    assert_eq!(applied.result.label, "move Player");

    let moved = sessions.document(&game_dir, LEVEL).expect("open");
    assert_eq!(
        moved.entity(player).expect("player").components["Transform"]["pos"][0],
        8.0
    );

    // One undo puts it back where the drag started — not eight.
    sessions.undo(&game_dir, LEVEL).expect("undo the drag");
    let back = sessions.document(&game_dir, LEVEL).expect("open");
    assert_eq!(
        back.entity(player).expect("player").components["Transform"]["pos"][0],
        0.0
    );
    assert!(
        !back.entities.is_empty()
            && sessions
                .state(&game_dir, LEVEL)
                .is_some_and(|s| !s.can_undo),
        "the drag was the only entry on the stack"
    );

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// Cancelling a drag rolls it back without leaving an undo entry behind.
#[test]
fn a_cancelled_drag_leaves_no_trace() {
    let game_dir = temp_game("cancel");
    let mut sessions = EngineSessions::new();
    sessions.open(&game_dir, LEVEL).expect("open");
    let player = first_named(&sessions, &game_dir, "Player");
    let before = sessions
        .document(&game_dir, LEVEL)
        .expect("open")
        .entity(player)
        .expect("player")
        .components["Transform"]
        .clone();

    sessions
        .begin_interaction(&game_dir, LEVEL, "move Player")
        .expect("begin");
    sessions
        .record_interaction(
            &game_dir,
            LEVEL,
            &EngineAction::SetTransform {
                entity: player,
                pos: Some([12.0, 3.0, -4.0]),
                rot: None,
                scale: None,
            },
        )
        .expect("record");
    let state = sessions
        .cancel_interaction(&game_dir, LEVEL)
        .expect("cancel");

    assert!(!state.can_undo, "a cancelled drag is not undo history");
    let after = sessions
        .document(&game_dir, LEVEL)
        .expect("open")
        .entity(player)
        .expect("player")
        .components["Transform"]
        .clone();
    assert_eq!(after, before, "the scene is exactly as it was");

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// Closing a scene with unsaved work must refuse unless the caller says discard.
#[test]
fn a_dirty_scene_refuses_to_close_silently() {
    let game_dir = temp_game("close");
    let mut sessions = EngineSessions::new();
    sessions
        .apply_action(
            &game_dir,
            LEVEL,
            &spawn("cube", "UserCrate"),
            EngineActor::User,
            "add cube",
            false,
        )
        .expect("edit");

    let refused = sessions
        .close(&game_dir, LEVEL, false)
        .expect_err("a dirty scene must not close silently");
    assert!(refused.hint.is_some(), "the refusal explains the way out");

    sessions
        .close(&game_dir, LEVEL, true)
        .expect("discarding is allowed when asked for explicitly");
    let reopened = sessions.open(&game_dir, LEVEL).expect("reopen");
    assert!(!reopened.dirty);
    assert!(
        !sessions
            .document(&game_dir, LEVEL)
            .expect("open")
            .entities
            .iter()
            .any(|entity| entity.name == "UserCrate"),
        "the discarded edit is gone"
    );

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// Saving writes the live document and clears the dirty flag; the file round-trips
/// through the strict parser, so the editor can never persist a scene the engine would
/// then refuse to load.
#[test]
fn save_writes_a_document_the_strict_parser_accepts() {
    let game_dir = temp_game("save");
    let mut sessions = EngineSessions::new();
    sessions
        .apply_action(
            &game_dir,
            LEVEL,
            &EngineAction::SetWeather {
                weather: "storm".to_owned(),
            },
            EngineActor::User,
            "set weather storm",
            false,
        )
        .expect("weather edit");
    let saved = sessions.save(&game_dir, LEVEL).expect("save");
    assert!(!saved.dirty);
    assert_eq!(saved.settings.weather.as_deref(), Some("storm"));

    let text = std::fs::read_to_string(game_dir.join(LEVEL)).expect("read");
    let strict = SceneDocument::parse(&text).expect("strict parse of a saved scene");
    assert_eq!(strict.settings.weather.as_deref(), Some("storm"));

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// A file rewritten underneath an unsaved session is reported, not merged and not
/// silently overwritten (ENG-108).
#[test]
fn an_outside_rewrite_under_unsaved_work_is_reported_as_a_conflict() {
    let game_dir = temp_game("conflict");
    let mut sessions = EngineSessions::new();
    sessions
        .apply_action(
            &game_dir,
            LEVEL,
            &spawn("cube", "UserCrate"),
            EngineActor::User,
            "add cube",
            false,
        )
        .expect("edit");
    assert!(
        !sessions
            .state(&game_dir, LEVEL)
            .expect("open")
            .disk_conflict,
        "no conflict before anyone touches the file"
    );

    // Something outside the app rewrites the scene.
    let mut outside = read_scene(&game_dir);
    outside.name = "rewritten_by_someone_else".to_owned();
    std::fs::write(game_dir.join(LEVEL), outside.dump().expect("dump")).expect("outside write");

    assert!(
        sessions
            .state(&game_dir, LEVEL)
            .expect("open")
            .disk_conflict,
        "the pane must be told before it saves over someone else's work"
    );

    // Taking the disk copy is an explicit choice, and it clears the conflict.
    let reloaded = sessions.reload(&game_dir, LEVEL).expect("reload");
    assert!(!reloaded.disk_conflict);
    assert!(!reloaded.dirty);
    assert_eq!(reloaded.name, "rewritten_by_someone_else");

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// Every applied transaction produces the facts the journal row is built from (INV-071).
#[test]
fn an_applied_edit_carries_its_journal_facts() {
    let game_dir = temp_game("journal-facts");
    let mut sessions = EngineSessions::new();
    let applied = sessions
        .apply_action(
            &game_dir,
            LEVEL,
            &spawn("cube", "UserCrate"),
            EngineActor::Agent,
            "ai:engine_action",
            true,
        )
        .expect("edit");

    assert_eq!(applied.journal.actor, "agent");
    assert_eq!(applied.journal.label, "ai:engine_action");
    assert_eq!(applied.journal.scene_rel_path, LEVEL);
    assert_eq!(applied.journal.op_count, 1);
    assert!(!applied.journal.txn_id.is_empty());
    // The inverse is stored so undo can outlive the process.
    assert!(applied.journal.inverse_json.contains("delete"));
    let touched: Vec<EntityId> =
        serde_json::from_str(&applied.journal.touched_json).expect("touched ids");
    assert_eq!(touched.len(), 1);

    let _ = std::fs::remove_dir_all(&game_dir);
}

/// ENG-189 — "Undo AI change" takes a whole agent batch back as one operation.
///
/// The inverse comes from the journal row rather than the in-memory undo stack, so this is
/// the path that still works after a restart. It is applied as a fresh user transaction, so
/// the revert is itself undoable — which is the property a silent rollback would lose.
#[test]
fn a_journalled_agent_batch_is_reverted_as_one_undoable_operation() {
    let dir = temp_game("revert");
    let mut sessions = EngineSessions::default();
    let opened = sessions.open(&dir, LEVEL).expect("open");
    let before = opened.entity_count;

    let applied = sessions
        .apply_batch(
            BatchRequest {
                game_dir: &dir,
                rel_path: LEVEL,
                label: "build a wall",
                actions: &[
                    serde_json::json!({ "kind": "spawn", "template": "cube" }),
                    serde_json::json!({ "kind": "spawn", "template": "cube" }),
                    serde_json::json!({ "kind": "spawn", "template": "cube" }),
                ],
                actor: EngineActor::Agent,
                autosave: true,
                owner: Some("agent-a"),
                base_revision: None,
            },
            bhippi_app::engine::resolve_batch_step,
        )
        .expect("the agent builds");
    assert_eq!(applied.result.state.entity_count, before + 3);
    let facts = applied.journal.expect("an applied batch is journaled");

    // One revert, not three: a batch is one transaction (ENG-111), so its inverse is one op
    // list and one press.
    let state = sessions
        .revert_journalled(&dir, LEVEL, &facts.label, &facts.inverse_json)
        .expect("the journalled inverse applies");
    assert_eq!(state.entity_count, before, "the whole batch came back out");
    assert!(state
        .undo_label
        .unwrap_or_default()
        .contains("undo AI change"));

    // …and the revert is itself on the undo stack, because changing your mind twice is
    // allowed.
    let restored = sessions.undo(&dir, LEVEL).expect("undo the revert");
    assert_eq!(restored.entity_count, before + 3);

    let _ = std::fs::remove_dir_all(&dir);
}
