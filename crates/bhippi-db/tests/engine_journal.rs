//! ENG-103 / INV-071: every applied transaction is journaled with actor + label, and the
//! journal is what "what did the agent change?" and cross-session undo read from.

use bhippi_db::{Database, EngineProjectRecord, NewJournalEntry};
use bhippi_types::SessionId;
use chrono::Utc;
use std::path::{Path, PathBuf};

fn test_database_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("bhippi-{label}-{}.db", SessionId::new()))
}

fn remove_database_files(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

fn project(path: &str) -> EngineProjectRecord {
    EngineProjectRecord {
        project_path: path.to_owned(),
        game_id: "01JC7B0KZ0TCVZVWY5YE2H3ZZQ".to_owned(),
        game_name: "Demo".to_owned(),
        version: "0.1.0".to_owned(),
        default_scene: "assets/scenes/main.bscn.json".to_owned(),
        engine_track: "rust".to_owned(),
        targets_json: "[\"windows\"]".to_owned(),
        scene_count: 3,
    }
}

fn entry(txn: &str, actor: &str, label: &str, scene: &str) -> NewJournalEntry {
    NewJournalEntry {
        txn_id: txn.to_owned(),
        actor: actor.to_owned(),
        label: label.to_owned(),
        scene_rel_path: scene.to_owned(),
        ops_json: "[{\"op\":\"rename\"}]".to_owned(),
        inverse_json: "[{\"op\":\"rename\"}]".to_owned(),
        touched_json: "[\"01JD0000000000000000000000\"]".to_owned(),
        op_count: 1,
    }
}

#[tokio::test]
async fn journal_numbers_revisions_and_answers_who_changed_what() {
    let path = test_database_path("engine-journal");
    let database = Database::connect(&path)
        .await
        .unwrap_or_else(|error| panic!("database must open: {error}"));
    let engine = database.engine();
    let now = Utc::now();
    let root = "C:/projects/demo";

    engine
        .upsert_project(&project(root), &now)
        .await
        .unwrap_or_else(|error| panic!("project must register: {error}"));

    assert_eq!(engine.latest_revision(root).await, Ok(0));

    let level = "assets/scenes/level_01.bscn.json";
    let main = "assets/scenes/main.bscn.json";
    let first = engine
        .append(root, &entry("t1", "user", "rename entity", level), &now)
        .await
        .unwrap_or_else(|error| panic!("append must work: {error}"));
    let second = engine
        .append(root, &entry("t2", "agent", "ai:engine_action", level), &now)
        .await
        .unwrap_or_else(|error| panic!("append must work: {error}"));
    let third = engine
        .append(root, &entry("t3", "agent", "ai:engine_action", main), &now)
        .await
        .unwrap_or_else(|error| panic!("append must work: {error}"));

    assert_eq!((first, second, third), (1, 2, 3), "revisions are monotonic");
    assert_eq!(engine.latest_revision(root).await, Ok(3));

    // Newest first, and the whole applied record round-trips.
    let all = engine
        .list(root, None, 10)
        .await
        .unwrap_or_else(|error| panic!("list must work: {error}"));
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].revision, 3);
    assert_eq!(all[0].txn_id, "t3");
    assert_eq!(all[0].actor, "agent");
    assert_eq!(all[0].label.as_deref(), Some("ai:engine_action"));
    assert_eq!(all[0].scene_rel_path, main);
    assert_eq!(all[0].op_count, 1);
    assert!(all[0].inverse_json.contains("rename"), "inverse persisted");

    // Per-scene paging is what the Engine pane's history list reads.
    let for_level = engine
        .list(root, Some(level), 10)
        .await
        .unwrap_or_else(|error| panic!("scene list must work: {error}"));
    assert_eq!(for_level.len(), 2);
    assert!(for_level.iter().all(|row| row.scene_rel_path == level));

    // Both actors are visible — the point of INV-071.
    let counts = engine
        .actor_counts(root)
        .await
        .unwrap_or_else(|error| panic!("counts must work: {error}"));
    assert_eq!(
        counts,
        vec![("agent".to_owned(), 2), ("user".to_owned(), 1)]
    );

    database.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn a_reopened_project_keeps_its_history_and_journals_survive_the_upsert() {
    let path = test_database_path("engine-journal-reopen");
    let database = Database::connect(&path)
        .await
        .unwrap_or_else(|error| panic!("database must open: {error}"));
    let engine = database.engine();
    let now = Utc::now();
    let root = "C:/projects/demo";
    let scene = "assets/scenes/level_01.bscn.json";

    engine
        .upsert_project(&project(root), &now)
        .await
        .unwrap_or_else(|error| panic!("register: {error}"));
    engine
        .append(root, &entry("t1", "user", "spawn cube", scene), &now)
        .await
        .unwrap_or_else(|error| panic!("append: {error}"));

    // Reopening the project refreshes the cached manifest facts; it must not wipe history.
    let mut refreshed = project(root);
    refreshed.game_name = "Demo Renamed".to_owned();
    refreshed.scene_count = 4;
    engine
        .upsert_project(&refreshed, &now)
        .await
        .unwrap_or_else(|error| panic!("re-register: {error}"));

    assert_eq!(engine.latest_revision(root).await, Ok(1));
    let next = engine
        .append(root, &entry("t2", "agent", "ai:engine_action", scene), &now)
        .await
        .unwrap_or_else(|error| panic!("append: {error}"));
    assert_eq!(next, 2, "revision continues across sessions");

    database.close().await;
    remove_database_files(&path);
}
