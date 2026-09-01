use crate::db_error;
use bhippi_types::Result;
use sqlx::{Row, SqlitePool};

const REQUIRED_INDEXES: [&str; 12] = [
    "idx_nodes_frontier",
    "idx_dots_node",
    "idx_dots_session",
    "ux_sources_canon",
    "idx_sources_domain",
    "idx_sources_simhash",
    "idx_ticker_cluster",
    "idx_ticker_state",
    "idx_posts_status",
    "idx_gists_decay",
    "idx_images_session",
    "idx_skillruns_skill",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForeignKeyViolation {
    pub table: String,
    pub row_id: Option<i64>,
    pub parent: String,
    pub foreign_key_index: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoctorReport {
    pub migration_count: u64,
    pub missing_indexes: Vec<String>,
    pub foreign_key_violations: Vec<ForeignKeyViolation>,
}

impl DoctorReport {
    pub(crate) async fn inspect(pool: &SqlitePool) -> Result<Self> {
        let migration_count = sqlx::query_scalar!("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .map_err(|error| db_error(error, "count schema migrations"))?;
        let index_rows =
            sqlx::query!("SELECT name AS `name!: String` FROM sqlite_master WHERE type = 'index'")
                .fetch_all(pool)
                .await
                .map_err(|error| db_error(error, "inspect database indexes"))?;
        let actual_indexes = index_rows
            .into_iter()
            .map(|row| row.name)
            .collect::<std::collections::BTreeSet<_>>();
        let missing_indexes = REQUIRED_INDEXES
            .into_iter()
            .filter(|name| !actual_indexes.contains(*name))
            .map(str::to_owned)
            .collect();

        let rows = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(pool)
            .await
            .map_err(|error| db_error(error, "check foreign keys"))?;
        let foreign_key_violations = rows
            .into_iter()
            .map(|row| ForeignKeyViolation {
                table: row.get("table"),
                row_id: row.get("rowid"),
                parent: row.get("parent"),
                foreign_key_index: row.get("fkid"),
            })
            .collect();

        let migration_count =
            u64::try_from(migration_count).map_err(|error| bhippi_types::BhippiError::Db {
                reason: format!("count schema migrations: {error}"),
                retryable: false,
                hint: Some("Run `bhippi doctor` against a fresh database.".to_owned()),
            })?;

        Ok(Self {
            migration_count,
            missing_indexes,
            foreign_key_violations,
        })
    }

    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.migration_count >= 3
            && self.missing_indexes.is_empty()
            && self.foreign_key_violations.is_empty()
    }
}
