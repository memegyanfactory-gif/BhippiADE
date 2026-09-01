//! The engine transaction journal (ADR-0020, INV-071, ENG-103).
//!
//! `0004_engine.sql` created `engine_projects` and `engine_journal` and nothing wrote to
//! them, so "what did the agent change?" had no answer and undo could not outlive the
//! process. This repo is the writer. It stores facts only: the ops as they were applied,
//! their captured inverse, the actor and the label. No engine logic lives here — replaying
//! a row is `bhippi-engine`'s job.

use crate::db_error;
use crate::repositories::RepoDb;
use bhippi_types::{Result, Timestamp};

/// The stable half of a game manifest, cached for the journal's foreign key and the
/// ledger UI. The manifest on disk stays the source of truth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineProjectRecord {
    pub project_path: String,
    pub game_id: String,
    pub game_name: String,
    pub version: String,
    pub default_scene: String,
    /// `rust` | `scripted`
    pub engine_track: String,
    pub targets_json: String,
    pub scene_count: i64,
}

/// One journaled transaction, newest first when listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalRecord {
    pub revision: i64,
    pub txn_id: String,
    /// `user` | `agent`
    pub actor: String,
    pub issued_at: String,
    pub label: Option<String>,
    pub scene_rel_path: String,
    pub ops_json: String,
    pub inverse_json: String,
    pub touched_json: String,
    pub op_count: i64,
}

/// A transaction on its way into the journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewJournalEntry {
    pub txn_id: String,
    pub actor: String,
    pub label: String,
    pub scene_rel_path: String,
    pub ops_json: String,
    pub inverse_json: String,
    pub touched_json: String,
    pub op_count: i64,
}

#[derive(Clone)]
pub struct EngineRepo {
    db: RepoDb,
}

impl EngineRepo {
    pub(crate) const fn new(db: RepoDb) -> Self {
        Self { db }
    }

    /// Register (or refresh) the game project the journal hangs off. Must run before the
    /// first `append` for a path — `engine_journal.project_path` is a foreign key.
    pub async fn upsert_project(
        &self,
        record: &EngineProjectRecord,
        now: &Timestamp,
    ) -> Result<()> {
        let now = now.to_rfc3339();
        sqlx::query!(
            r#"INSERT INTO engine_projects
                 (project_path, game_id, game_name, version, default_scene, engine_track,
                  targets_json, scene_count, first_seen_at, last_loaded_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(project_path) DO UPDATE SET
                 game_id        = excluded.game_id,
                 game_name      = excluded.game_name,
                 version        = excluded.version,
                 default_scene  = excluded.default_scene,
                 engine_track   = excluded.engine_track,
                 targets_json   = excluded.targets_json,
                 scene_count    = excluded.scene_count,
                 last_loaded_at = excluded.last_loaded_at"#,
            record.project_path,
            record.game_id,
            record.game_name,
            record.version,
            record.default_scene,
            record.engine_track,
            record.targets_json,
            record.scene_count,
            now,
            now,
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "upsert engine project"))?;
        Ok(())
    }

    /// Append one applied transaction and return its revision. The revision is allocated
    /// inside the same transaction as the insert, so two concurrent writers cannot land on
    /// the same number.
    pub async fn append(
        &self,
        project_path: &str,
        entry: &NewJournalEntry,
        now: &Timestamp,
    ) -> Result<i64> {
        let now = now.to_rfc3339();
        let mut tx = self
            .db
            .writer
            .begin()
            .await
            .map_err(|error| db_error(error, "begin engine journal append"))?;
        let next = sqlx::query_scalar!(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM engine_journal WHERE project_path = ?",
            project_path
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| db_error(error, "allocate engine journal revision"))?;
        sqlx::query!(
            r#"INSERT INTO engine_journal
                 (project_path, revision, txn_id, actor, issued_at, label, ops_json,
                  scene_rel_path, inverse_json, touched_json, op_count)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            project_path,
            next,
            entry.txn_id,
            entry.actor,
            now,
            entry.label,
            entry.ops_json,
            entry.scene_rel_path,
            entry.inverse_json,
            entry.touched_json,
            entry.op_count,
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| db_error(error, "append engine journal row"))?;
        tx.commit()
            .await
            .map_err(|error| db_error(error, "commit engine journal append"))?;
        Ok(next.into())
    }

    /// The most recent transactions for a project, newest first. `scene` narrows to one
    /// scene; `None` spans the whole game.
    pub async fn list(
        &self,
        project_path: &str,
        scene: Option<&str>,
        limit: i64,
    ) -> Result<Vec<JournalRecord>> {
        let rows = match scene {
            Some(scene) => sqlx::query!(
                r#"SELECT revision, txn_id, actor, issued_at, label, scene_rel_path,
                          ops_json, inverse_json, touched_json, op_count
                     FROM engine_journal
                    WHERE project_path = ? AND scene_rel_path = ?
                 ORDER BY revision DESC
                    LIMIT ?"#,
                project_path,
                scene,
                limit
            )
            .fetch_all(&self.db.readers)
            .await
            .map_err(|error| db_error(error, "list engine journal for scene"))?
            .into_iter()
            .map(|row| JournalRecord {
                revision: row.revision,
                txn_id: row.txn_id,
                actor: row.actor,
                issued_at: row.issued_at,
                label: row.label,
                scene_rel_path: row.scene_rel_path,
                ops_json: row.ops_json,
                inverse_json: row.inverse_json,
                touched_json: row.touched_json,
                op_count: row.op_count,
            })
            .collect(),
            None => sqlx::query!(
                r#"SELECT revision, txn_id, actor, issued_at, label, scene_rel_path,
                          ops_json, inverse_json, touched_json, op_count
                     FROM engine_journal
                    WHERE project_path = ?
                 ORDER BY revision DESC
                    LIMIT ?"#,
                project_path,
                limit
            )
            .fetch_all(&self.db.readers)
            .await
            .map_err(|error| db_error(error, "list engine journal"))?
            .into_iter()
            .map(|row| JournalRecord {
                revision: row.revision,
                txn_id: row.txn_id,
                actor: row.actor,
                issued_at: row.issued_at,
                label: row.label,
                scene_rel_path: row.scene_rel_path,
                ops_json: row.ops_json,
                inverse_json: row.inverse_json,
                touched_json: row.touched_json,
                op_count: row.op_count,
            })
            .collect(),
        };
        Ok(rows)
    }

    /// The highest revision recorded for a project (`0` when the journal is empty).
    pub async fn latest_revision(&self, project_path: &str) -> Result<i64> {
        let revision = sqlx::query_scalar!(
            "SELECT COALESCE(MAX(revision), 0) FROM engine_journal WHERE project_path = ?",
            project_path
        )
        .fetch_one(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "read latest engine revision"))?;
        Ok(revision.into())
    }

    /// How many transactions each actor contributed — the cheap answer to "how much of
    /// this scene did the agent write?".
    pub async fn actor_counts(&self, project_path: &str) -> Result<Vec<(String, i64)>> {
        let rows = sqlx::query!(
            r#"SELECT actor, COUNT(*) AS "count!: i64"
                 FROM engine_journal
                WHERE project_path = ?
             GROUP BY actor
             ORDER BY actor"#,
            project_path
        )
        .fetch_all(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "count engine journal actors"))?;
        Ok(rows.into_iter().map(|row| (row.actor, row.count)).collect())
    }
}
