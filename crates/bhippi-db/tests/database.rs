use bhippi_db::{Database, NewSession, StageArtifact};
use bhippi_types::{Origin, SessionId, Stage, Tier};
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
    assert_eq!(database.sessions().count().await, Ok(0));
    assert_eq!(database.nodes().count().await, Ok(0));
    assert_eq!(database.jobs().count().await, Ok(0));

    database.close().await;
    remove_database_files(&path);
}

#[tokio::test]
async fn stage_artifact_and_cursor_commit_atomically() {
    let path = test_database_path("stage");
    let database = Database::connect(&path)
        .await
        .unwrap_or_else(|error| panic!("stage database must open: {error}"));
    let session_id = SessionId::new();
    database
        .sessions()
        .create(&NewSession {
            id: session_id,
            seed_topic: "local AI inference".to_owned(),
            tier: Tier::X6,
            origin: Origin::Manual,
            ticker_event_id: None,
            started_at: Utc::now(),
        })
        .await
        .unwrap_or_else(|error| panic!("session must be created: {error}"));

    database
        .sessions()
        .advance_stage(
            session_id,
            Stage::Planning,
            Stage::Expanding,
            StageArtifact::Charter("{\"scope\":\"technology\"}".to_owned()),
            Utc::now(),
        )
        .await
        .unwrap_or_else(|error| panic!("valid stage transition must commit: {error}"));

    let stale = database
        .sessions()
        .advance_stage(
            session_id,
            Stage::Planning,
            Stage::Writing,
            StageArtifact::Blueprint("must roll back".to_owned()),
            Utc::now(),
        )
        .await;
    assert!(stale.is_err());

    let resume = database
        .sessions()
        .resume_point(session_id)
        .await
        .unwrap_or_else(|error| panic!("resume point must load: {error}"))
        .unwrap_or_else(|| panic!("session must exist"));
    assert_eq!(resume.stage, Stage::Expanding);
    assert_eq!(resume.stage_cursor.as_deref(), Some("expanding"));
    assert_eq!(
        resume.charter.as_deref(),
        Some("{\"scope\":\"technology\"}")
    );
    assert_eq!(resume.blueprint, None);

    database.close().await;
    remove_database_files(&path);
}
