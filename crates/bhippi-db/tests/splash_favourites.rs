//! Saved splash screens (GAD-161): they outlive the game they were made in, and saving the
//! same one twice is one entry rather than two.

use bhippi_db::{Database, NewSplashFavourite};
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

fn favourite(id: &str, name: &str, origin: &str) -> NewSplashFavourite {
    NewSplashFavourite {
        id: id.to_owned(),
        name: name.to_owned(),
        spec_json: format!("{{\"title\":\"{name}\"}}"),
        origin: origin.to_owned(),
    }
}

#[tokio::test]
async fn favourites_list_newest_first_and_re_saving_updates_rather_than_duplicates() {
    let path = test_database_path("splash-favourites");
    let database = Database::connect(&path)
        .await
        .unwrap_or_else(|error| panic!("database must open: {error}"));
    let splash = database.splash();

    let earlier = Utc::now();
    let later = earlier + chrono::Duration::seconds(30);

    splash
        .save(
            &favourite("01FIRST", "Greenwood", "C:/games/demo"),
            &earlier,
        )
        .await
        .unwrap_or_else(|error| panic!("the first must save: {error}"));
    splash
        .save(
            &favourite("01SECOND", "Neon Drift", "C:/games/other"),
            &later,
        )
        .await
        .unwrap_or_else(|error| panic!("the second must save: {error}"));

    let rows = splash
        .list(10)
        .await
        .unwrap_or_else(|error| panic!("list must work: {error}"));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "Neon Drift", "newest first");
    assert_eq!(rows[1].name, "Greenwood");
    assert_eq!(
        rows[1].origin, "C:/games/demo",
        "where it came from is kept"
    );

    // Favouriting the same splash again is something a person does by accident, and two
    // identical rows is the result nobody wants.
    splash
        .save(
            &favourite("01FIRST", "Greenwood II", "C:/games/demo"),
            &earlier,
        )
        .await
        .unwrap_or_else(|error| panic!("re-saving must work: {error}"));
    let rows = splash.list(10).await.unwrap_or_default();
    assert_eq!(rows.len(), 2, "re-saving updates rather than duplicating");
    assert!(rows.iter().any(|row| row.name == "Greenwood II"));

    assert!(splash.remove("01FIRST").await.unwrap_or(false));
    assert!(
        !splash.remove("01FIRST").await.unwrap_or(true),
        "forgetting something already gone reports that it was not there"
    );
    assert_eq!(splash.list(10).await.unwrap_or_default().len(), 1);

    // The limit is honoured, so a long list never floods the panel.
    assert!(splash.list(0).await.unwrap_or_default().is_empty());

    database.close().await;
    remove_database_files(&path);
}
