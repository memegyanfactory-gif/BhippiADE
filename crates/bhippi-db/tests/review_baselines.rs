//! The review ledger: first touch wins, and a workspace only ever sees its own rows.
//!
//! The rule under test is the one that makes the Review Changes panel mean anything: the
//! baseline is what a file held before Bhippi *started*, so a turn that rewrites the same
//! file five times still reports it against its original content rather than against its
//! own fourth draft.

use bhippi_db::{Database, NewReviewBaseline};
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

fn baseline(project: &str, rel: &str, before: Option<&str>) -> NewReviewBaseline {
    NewReviewBaseline {
        file_path: format!("{project}/{rel}"),
        project_path: project.to_owned(),
        rel_path: rel.to_owned(),
        existed: before.is_some(),
        before_text: before.map(str::to_owned),
    }
}

#[tokio::test]
async fn the_first_touch_is_the_baseline_and_later_writes_never_replace_it() {
    let path = test_database_path("review-ledger");
    let database = Database::connect(&path)
        .await
        .unwrap_or_else(|error| panic!("database must open: {error}"));
    let review = database.review();
    let now = Utc::now();
    let project = "C:/games/demo";

    let first = review
        .record(
            &baseline(project, "scripts/game.gd", Some("extends Node\n")),
            &now,
        )
        .await
        .unwrap_or_else(|error| panic!("the first record must land: {error}"));
    assert!(first, "the first touch is the baseline");

    let second = review
        .record(
            &baseline(
                project,
                "scripts/game.gd",
                Some("extends Node\nfunc _ready(): pass\n"),
            ),
            &now,
        )
        .await
        .unwrap_or_else(|error| panic!("a second record must not fail: {error}"));
    assert!(!second, "a later write is not the baseline");

    // A file Bhippi created: nothing was there, and the review must call it an addition.
    review
        .record(&baseline(project, "scenes/main.tscn", None), &now)
        .await
        .unwrap_or_else(|error| panic!("a creation must record: {error}"));
    review
        .record(
            &baseline("C:/games/other", "scripts/game.gd", Some("x\n")),
            &now,
        )
        .await
        .unwrap_or_else(|error| panic!("another project must record: {error}"));

    let rows = review
        .list(project)
        .await
        .unwrap_or_else(|error| panic!("list must work: {error}"));
    assert_eq!(rows.len(), 2, "only this workspace's rows are listed");
    assert_eq!(rows[0].rel_path, "scenes/main.tscn");
    assert!(!rows[0].existed);
    assert_eq!(rows[0].before_text, None);
    assert_eq!(rows[1].rel_path, "scripts/game.gd");
    assert!(rows[1].existed);
    assert_eq!(
        rows[1].before_text.as_deref(),
        Some("extends Node\n"),
        "the original text survives every later write"
    );

    let cleared = review
        .clear(project)
        .await
        .unwrap_or_else(|error| panic!("clear must work: {error}"));
    assert_eq!(cleared, 2);
    assert!(review.list(project).await.unwrap_or_default().is_empty());
    assert_eq!(
        review
            .list("C:/games/other")
            .await
            .unwrap_or_default()
            .len(),
        1,
        "clearing one workspace leaves another alone"
    );

    database.close().await;
    remove_database_files(&path);
}
