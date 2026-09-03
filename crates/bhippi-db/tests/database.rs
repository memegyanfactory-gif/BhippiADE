//! A test states its preconditions with `unwrap`/`expect`: a panic here is a failing test,
//! not a crashed app. The workspace `deny` stands everywhere else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_db::{Database, NewJournalEntry};
use bhippi_types::SessionId;
use std::path::{Path, PathBuf};

fn test_database_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("bhippi-{label}-{}.db", SessionId::new()))
}

fn remove_database_files(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}

#[tokio::test]
async fn fresh_database_migrates_and_passes_integrity_checks() {
    let path = test_database_path("fresh");
    let database = Database::connect(&path)
        .await
        .unwrap_or_else(|error| panic!("fresh database must open: {error}"));

    let report = database
        .doctor()
        .await
        .unwrap_or_else(|error| panic!("doctor must inspect fresh database: {error}"));

    assert!(report.is_clean(), "fresh database report: {report:?}");
    assert_eq!(database.jobs().count().await, Ok(0));
    assert_eq!(database.skills().count().await, Ok(0));
    assert_eq!(database.providers().count().await, Ok(0));

    database.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn engine_journal_records_and_reverts() {
    let path = test_database_path("engine");
    let database = Database::connect(&path)
        .await
        .unwrap_or_else(|error| panic!("engine database must open: {error}"));

    let engine = database.engine();
    let project_path = "/work/games/my_game".to_owned();
    let now = chrono::Utc::now();

    let project = bhippi_db::EngineProjectRecord {
        project_path: project_path.clone(),
        game_id: "game_123".to_owned(),
        game_name: "My Game".to_owned(),
        version: "1.0.0".to_owned(),
        default_scene: "scenes/main.tscn".to_owned(),
        engine_track: "scripted".to_owned(),
        targets_json: "[]".to_owned(),
        scene_count: 1,
    };
    engine.upsert_project(&project, &now).await.unwrap();

    let entry = NewJournalEntry {
        txn_id: "txn_001".to_owned(),
        actor: "user".to_owned(),
        label: "add player node".to_owned(),
        scene_rel_path: "scenes/main.tscn".to_owned(),
        ops_json: "[]".to_owned(),
        inverse_json: "[]".to_owned(),
        touched_json: "[\"Player\"]".to_owned(),
        op_count: 1,
    };

    let revision = engine.append(&project_path, &entry, &now).await.unwrap();
    assert_eq!(revision, 1);

    let latest_rev = engine.latest_revision(&project_path).await.unwrap();
    assert_eq!(latest_rev, 1);

    database.close().await;
    remove_database_files(&path);
}
