//! The review ledger: what a file held before Bhippi first touched it.
//!
//! `bhippi-app::review` used to answer "what changed?" with `git diff`, and a game Bhippi
//! builds is not a git repository — so a whole game could be written and the Review Changes
//! panel would say the workspace was clean. Scanning the project instead only moves the
//! problem: every file reads as an addition, nothing is ever a deletion, and the number
//! never means "what this session did".
//!
//! This repo is the answer that does not need git. Every write path records what a file
//! held the first time it touches it, and a review is that baseline compared against the
//! disk. Facts only — the diff itself is computed in the app.

use crate::db_error;
use crate::repositories::RepoDb;
use bhippi_types::{Result, Timestamp};

/// One file's state before Bhippi's first write to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewBaseline {
    /// Absolute, forward-slashed.
    pub file_path: String,
    /// Workspace-relative, forward-slashed — the name the review prints.
    pub rel_path: String,
    /// False for a file Bhippi created: there was nothing to keep.
    pub existed: bool,
    /// The pre-write text. `None` for a creation, or for a file that was not text.
    pub before_text: Option<String>,
}

/// A baseline on its way in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewReviewBaseline {
    pub file_path: String,
    pub project_path: String,
    pub rel_path: String,
    pub existed: bool,
    pub before_text: Option<String>,
}

#[derive(Clone)]
pub struct ReviewRepo {
    db: RepoDb,
}

impl ReviewRepo {
    pub(crate) const fn new(db: RepoDb) -> Self {
        Self { db }
    }

    /// Keep a file's pre-write state.
    ///
    /// First touch wins: a row that already exists is left exactly as it was, because the
    /// baseline is the state before Bhippi *started*, not before its latest step. A turn
    /// that edits one file five times still reports one file against one baseline.
    ///
    /// Returns whether this call was the first touch.
    pub async fn record(&self, baseline: &NewReviewBaseline, now: &Timestamp) -> Result<bool> {
        let now = now.to_rfc3339();
        let existed = i64::from(baseline.existed);
        let result = sqlx::query!(
            r#"INSERT OR IGNORE INTO review_baselines
                 (file_path, project_path, rel_path, existed, before_text, recorded_at)
               VALUES (?, ?, ?, ?, ?, ?)"#,
            baseline.file_path,
            baseline.project_path,
            baseline.rel_path,
            existed,
            baseline.before_text,
            now,
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "record review baseline"))?;
        Ok(result.rows_affected() == 1)
    }

    /// Every baseline recorded for one workspace, ordered by the name the review prints.
    pub async fn list(&self, project_path: &str) -> Result<Vec<ReviewBaseline>> {
        let rows = sqlx::query!(
            r#"SELECT file_path, rel_path, existed, before_text
               FROM review_baselines
               WHERE project_path = ?
               ORDER BY rel_path"#,
            project_path
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "list review baselines"))?;
        Ok(rows
            .into_iter()
            .map(|row| ReviewBaseline {
                file_path: row.file_path,
                rel_path: row.rel_path,
                existed: row.existed == 1,
                before_text: row.before_text,
            })
            .collect())
    }

    /// Drop one workspace's ledger — what "start the review from here" means.
    pub async fn clear(&self, project_path: &str) -> Result<u64> {
        let result = sqlx::query!(
            "DELETE FROM review_baselines WHERE project_path = ?",
            project_path
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "clear review baselines"))?;
        Ok(result.rows_affected())
    }
}
