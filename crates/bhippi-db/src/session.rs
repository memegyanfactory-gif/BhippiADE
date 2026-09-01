use crate::db_error;
use crate::repositories::RepoDb;
use bhippi_types::{
    BhippiError, Origin, ProviderId, Result, SessionId, Stage, TickerEventId, Tier, Timestamp,
};

#[derive(Clone, Debug)]
pub struct NewSession {
    pub id: SessionId,
    pub seed_topic: String,
    pub tier: Tier,
    pub origin: Origin,
    pub ticker_event_id: Option<TickerEventId>,
    pub started_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResumePoint {
    pub stage: Stage,
    pub stage_cursor: Option<String>,
    pub charter: Option<String>,
    pub blueprint: Option<String>,
    pub writer_provider: Option<String>,
    pub flags: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StageArtifact {
    None,
    Charter(String),
    Blueprint(String),
    WriterProvider(ProviderId),
    Flags(String),
}

#[derive(Clone)]
pub struct SessionRepo {
    db: RepoDb,
}

impl SessionRepo {
    pub(crate) const fn new(db: RepoDb) -> Self {
        Self { db }
    }

    pub async fn create(&self, session: &NewSession) -> Result<()> {
        let id = session.id.to_string();
        let ticker_event_id = session.ticker_event_id.map(|value| value.to_string());
        let started_at = session.started_at.to_rfc3339();
        let tier = session.tier.as_str();
        let origin = session.origin.as_str();
        sqlx::query!(
            r#"INSERT INTO sessions
               (id, seed_topic, tier, origin, ticker_event_id, status, stage_cursor, started_at)
               VALUES (?, ?, ?, ?, ?, 'planning', 'planning', ?)"#,
            id,
            session.seed_topic,
            tier,
            origin,
            ticker_event_id,
            started_at
        )
        .execute(&self.db.writer)
        .await
        .map_err(|error| db_error(error, "create session"))?;
        Ok(())
    }

    pub async fn advance_stage(
        &self,
        id: SessionId,
        from: Stage,
        to: Stage,
        artifact: StageArtifact,
        now: Timestamp,
    ) -> Result<()> {
        let mut transaction = self
            .db
            .writer
            .begin()
            .await
            .map_err(|error| db_error(error, "begin stage transition"))?;
        let id = id.to_string();

        match artifact {
            StageArtifact::None => {}
            StageArtifact::Charter(charter) => {
                sqlx::query!("UPDATE sessions SET charter = ? WHERE id = ?", charter, id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| db_error(error, "store research charter"))?;
            }
            StageArtifact::Blueprint(blueprint) => {
                sqlx::query!(
                    "UPDATE sessions SET blueprint = ? WHERE id = ?",
                    blueprint,
                    id
                )
                .execute(&mut *transaction)
                .await
                .map_err(|error| db_error(error, "store writing blueprint"))?;
            }
            StageArtifact::WriterProvider(provider) => {
                let provider = provider.to_string();
                sqlx::query!(
                    "UPDATE sessions SET writer_provider = ? WHERE id = ?",
                    provider,
                    id
                )
                .execute(&mut *transaction)
                .await
                .map_err(|error| db_error(error, "store writer provider"))?;
            }
            StageArtifact::Flags(flags) => {
                serde_json::from_str::<serde_json::Value>(&flags).map_err(|error| {
                    BhippiError::Db {
                        reason: format!("validate session flags: {error}"),
                        retryable: false,
                        hint: Some("Provide session flags as a JSON object.".to_owned()),
                    }
                })?;
                sqlx::query!("UPDATE sessions SET flags = ? WHERE id = ?", flags, id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| db_error(error, "store session flags"))?;
            }
        }

        let finished_at = to.is_terminal().then(|| now.to_rfc3339());
        let to_stage = to.as_str();
        let from_stage = from.as_str();
        let result = sqlx::query!(
            "UPDATE sessions SET status = ?, stage_cursor = ?, finished_at = ? WHERE id = ? AND status = ?",
            to_stage,
            to_stage,
            finished_at,
            id,
            from_stage
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| db_error(error, "advance session stage"))?;

        if result.rows_affected() != 1 {
            return Err(BhippiError::Db {
                reason: format!(
                    "advance session stage: expected {} but the persisted stage changed",
                    from.as_str()
                ),
                retryable: true,
                hint: Some("Reload the session and resume from its persisted stage.".to_owned()),
            });
        }

        transaction
            .commit()
            .await
            .map_err(|error| db_error(error, "commit stage transition"))
    }

    pub async fn resume_point(&self, id: SessionId) -> Result<Option<ResumePoint>> {
        let id = id.to_string();
        let row = sqlx::query!(
            "SELECT status, stage_cursor, charter, blueprint, writer_provider, flags FROM sessions WHERE id = ?",
            id
        )
        .fetch_optional(&self.db.readers)
        .await
        .map_err(|error| db_error(error, "load session resume point"))?;

        row.map(|row| {
            let stage = Stage::parse(&row.status).ok_or_else(|| BhippiError::Db {
                reason: format!("load session resume point: unknown stage {}", row.status),
                retryable: false,
                hint: Some("Run `bhippi doctor` to inspect the session row.".to_owned()),
            })?;
            Ok(ResumePoint {
                stage,
                stage_cursor: row.stage_cursor,
                charter: row.charter,
                blueprint: row.blueprint,
                writer_provider: row.writer_provider,
                flags: row.flags,
            })
        })
        .transpose()
    }

    pub async fn count(&self) -> Result<u64> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM sessions")
            .fetch_one(&self.db.readers)
            .await
            .map_err(|error| db_error(error, "count sessions"))?;
        u64::try_from(count).map_err(|error| BhippiError::Db {
            reason: format!("count sessions: {error}"),
            retryable: false,
            hint: Some("Run `bhippi doctor` to inspect database integrity.".to_owned()),
        })
    }
}
