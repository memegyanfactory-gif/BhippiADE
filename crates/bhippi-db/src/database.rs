use crate::brain::BrainRepo;
use crate::db_error;
use crate::doctor::DoctorReport;
use crate::engine::EngineRepo;
use crate::repositories::{JobRepo, ProviderRepo, RepoDb, SkillRepo};
use bhippi_types::Result;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::Path;
use std::time::Duration;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Database {
    writer: SqlitePool,
    readers: SqlitePool,
}

impl Database {
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self> {
        let options = || {
            SqliteConnectOptions::new()
                .filename(path.as_ref())
                .create_if_missing(true)
                .foreign_keys(true)
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Normal)
                .busy_timeout(Duration::from_secs(5))
        };
        let writer = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options())
            .await
            .map_err(|error| db_error(error, "open database writer"))?;
        MIGRATOR
            .run(&writer)
            .await
            .map_err(|error| bhippi_types::BhippiError::Db {
                reason: format!("apply database migrations: {error}"),
                retryable: false,
                hint: Some("Run `bhippi doctor` before opening this database again.".to_owned()),
            })?;
        let readers = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options())
            .await
            .map_err(|error| db_error(error, "open database readers"))?;

        Ok(Self { writer, readers })
    }

    fn repos(&self) -> RepoDb {
        RepoDb::new(self.writer.clone(), self.readers.clone())
    }

    #[must_use]
    pub fn skills(&self) -> SkillRepo {
        SkillRepo::new(self.repos())
    }

    #[must_use]
    pub fn providers(&self) -> ProviderRepo {
        ProviderRepo::new(self.repos())
    }

    #[must_use]
    pub fn jobs(&self) -> JobRepo {
        JobRepo::new(self.repos())
    }

    #[must_use]
    pub fn brain(&self) -> BrainRepo {
        BrainRepo::new(self.repos())
    }

    #[must_use]
    pub fn engine(&self) -> EngineRepo {
        EngineRepo::new(self.repos())
    }

    pub async fn doctor(&self) -> Result<DoctorReport> {
        DoctorReport::inspect(&self.readers).await
    }

    pub async fn close(self) {
        self.readers.close().await;
        self.writer.close().await;
    }
}
