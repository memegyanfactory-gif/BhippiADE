use crate::db_error;
use bhippi_types::Result;
use sqlx::SqlitePool;

#[derive(Clone)]
pub(crate) struct RepoDb {
    pub(crate) writer: SqlitePool,
    pub(crate) readers: SqlitePool,
}

impl RepoDb {
    pub(crate) const fn new(writer: SqlitePool, readers: SqlitePool) -> Self {
        Self { writer, readers }
    }
}

macro_rules! count_repo {
    ($name:ident, $query:literal, $operation:literal) => {
        #[derive(Clone)]
        pub struct $name {
            db: RepoDb,
        }

        impl $name {
            pub(crate) const fn new(db: RepoDb) -> Self {
                Self { db }
            }

            pub async fn count(&self) -> Result<u64> {
                let count = sqlx::query_scalar!($query)
                    .fetch_one(&self.db.readers)
                    .await
                    .map_err(|error| db_error(error, $operation))?;
                u64::try_from(count).map_err(|error| bhippi_types::BhippiError::Db {
                    reason: format!("{}: {}", $operation, error),
                    retryable: false,
                    hint: Some("Run `bhippi doctor` to inspect database integrity.".to_owned()),
                })
            }
        }
    };
}

count_repo!(NodeRepo, "SELECT COUNT(*) FROM nodes", "count nodes");
count_repo!(DotRepo, "SELECT COUNT(*) FROM dots", "count dots");
count_repo!(SourceRepo, "SELECT COUNT(*) FROM sources", "count sources");
count_repo!(ImageRepo, "SELECT COUNT(*) FROM images", "count images");
count_repo!(
    MemoryRepo,
    "SELECT COUNT(*) FROM memory_gists",
    "count memory gists"
);
count_repo!(
    TickerRepo,
    "SELECT COUNT(*) FROM ticker_events",
    "count ticker events"
);
count_repo!(PostRepo, "SELECT COUNT(*) FROM posts", "count posts");
count_repo!(SkillRepo, "SELECT COUNT(*) FROM skills", "count skills");
count_repo!(
    ProviderRepo,
    "SELECT COUNT(*) FROM providers",
    "count providers"
);
count_repo!(JobRepo, "SELECT COUNT(*) FROM jobs", "count jobs");
