//! Shared, IO-free domain types for Bhippi.

#![forbid(unsafe_code)]

mod budget;
mod design;
mod domain;
mod engine;
mod error;
mod events;
mod ids;

pub use budget::{Tier, TierBudget};
pub use design::{
    DesignSurface, DESIGN_CONTEXT_TOKEN_BUDGET, DESIGN_INDEX_TOKEN_BUDGET,
    DESIGN_LESSONS_MAX_APPROVED, DESIGN_LESSON_MAX_RULE_BYTES, DESIGN_LESSON_MIN_EVIDENCE,
    DESIGN_LESSON_TOKEN_BUDGET, DESIGN_MAX_SECTIONS_PER_TURN, DESIGN_MEMORY_TOKEN_BUDGET,
    DESIGN_QUERY_ANSWER_TOKEN_BUDGET, DESIGN_QUERY_MAX_ROUNDS, DESIGN_SEARCH_MAX_HITS,
    TASTE_PROFILE_MAX_PINS, TASTE_PROFILE_TOKEN_BUDGET,
};
pub use domain::{NodeKind, Origin, Relation, Stage, TaskClass};
pub use engine::{
    EngineActor, EngineEvent, EngineLogLevel, EngineTransactionSummary, EntityTransformPatch,
    PlayState, ENGINE_AUTONOMY_MAX_ROUNDS, ENGINE_CONTEXT_TOKEN_BUDGET,
    ENGINE_GAME_DEBUG_RETAINED_RUNS, ENGINE_OBSERVATION_TIMEOUT_SECS,
    ENGINE_PLAYTEST_FIXED_DELTA_SECONDS, ENGINE_PLAYTEST_MAX_FRAMES_PER_STEP,
    ENGINE_PLAYTEST_MAX_KEYS_PER_STEP, ENGINE_PLAYTEST_MAX_KEY_CODE_BYTES,
    ENGINE_PLAYTEST_MAX_STEPS, ENGINE_SCREENSHOT_MAX_BYTES, ENGINE_SCREENSHOT_MAX_DIMENSION,
};
pub use error::{BhippiError, BudgetScope, FetchErrorKind, GateName};
pub use events::{
    Capability, DotSummary, EdgeDelta, ErrorCode, Event, Health, NodeDelta, NodeDotDelta,
    NodeStatus, PublishStep, ResyncReason, SourceSummary, TickerEventSummary, Timestamp,
};
pub use ids::{
    AssetId, BuildId, DotId, EntityId, FileId, GameId, ImageId, ModuleId, NodeId, PostId,
    ProjectId, ProviderId, SceneId, SessionId, SkillId, SourceId, SymbolId, TickerEventId,
    TransactionId,
};

/// The result type shared across Bhippi library crates.
pub type Result<T> = std::result::Result<T, BhippiError>;
