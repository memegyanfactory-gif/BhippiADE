//! Persistence, migrations, repositories, and indexes.

#![forbid(unsafe_code)]

mod brain;
mod database;
mod doctor;
mod engine;
mod repositories;
mod session;

pub use brain::{
    AssetRecord, BrainRepo, EntityRecord, FileScan, ModuleCardRecord, PhysicsBodyRecord,
    SceneRecord, SymbolRecord,
};

pub use database::Database;
pub use doctor::{DoctorReport, ForeignKeyViolation};
pub use engine::{EngineProjectRecord, EngineRepo, JournalRecord, NewJournalEntry};
pub use repositories::{
    DotRepo, ImageRepo, JobRepo, MemoryRepo, NodeRepo, PostRepo, ProviderRepo, SkillRepo,
    SourceRepo, TickerRepo,
};
pub use session::{NewSession, ResumePoint, SessionRepo, StageArtifact};

use bhippi_types::BhippiError;

fn db_error(error: sqlx::Error, operation: &'static str) -> BhippiError {
    BhippiError::Db {
        reason: format!("{operation}: {error}"),
        retryable: !matches!(
            error,
            sqlx::Error::Configuration(_) | sqlx::Error::Migrate(_)
        ),
        hint: Some("Run `bhippi doctor` and retry the operation.".to_owned()),
    }
}
