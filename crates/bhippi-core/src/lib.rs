//! Session orchestration, budgets, scheduling, and automation policy.

#![forbid(unsafe_code)]

mod bus;
mod config;
mod context;
mod design;
mod logging;
mod orchestration;
mod replay;
mod secrets;
mod usage;

pub use bhippi_skills as skills;
pub use bhippi_skills::{Skill, SkillStore};
pub use bus::{EventBus, EventReceiver};
pub use config::{
    AppConfig, AutomationConfig, AutomationMode, BhippiConfig, BudgetConfig, ConfigStore,
    DomainConfig, EngineConfig, EnginePermissionMode, ProjectRecord, ProvidersConfig,
    PublishConfig, PublishTarget, ResearchConfig, Routing, Theme, TickerConfig, WorkspaceConfig,
};
pub use context::{
    estimate_history_tokens, estimate_text_tokens, sum_totals, ContextCategory, ContextLog,
    ContextManifest, ContextSample, ContextSampleStore, ContextTotals, ESTIMATED_BYTES_PER_TOKEN,
    RETAINED_SAMPLES,
};
pub use design::{
    DesignAnswer, DesignDomain, DesignEpisode, DesignError, DesignKb, DesignLesson, DesignModule,
    DesignPack, DesignQuery, DesignRequest, DesignSection, DesignSelectError, EpisodeReaction,
    LessonBook, LessonDraft, LessonError, LessonStatus, PackedSection, SearchHit, SearchQuery,
    TasteAvoid, TasteChange, TasteOrigin, TastePin, TasteProfile, TasteSignal,
    DESIGN_EPISODE_FORMAT, DESIGN_KB_FORMAT, DESIGN_KB_MAJOR, LESSON_FORMAT, TASTE_FORMAT,
};
pub use logging::{LoggingGuard, SecretRedactor};
pub use orchestration::{
    evaluate_budget, evaluate_token_quality, AgentArtifact, ArtifactLimits, BudgetDecision,
    BudgetRule, CacheInvalidation, CapabilityCacheKey, ContextBudgetManifest, ContextPressure,
    EvidenceRef, GenericArtifactRef, ProjectState, RegressionPolicy, StablePrefixManifest,
    TaskCheckpoint, TokenQualityDecision, TokenQualityEvidence, AGENT_ARTIFACT_FORMAT,
    CONTEXT_BUDGET_FORMAT, PROJECT_STATE_FORMAT, TASK_CHECKPOINT_FORMAT,
};
pub use replay::{
    ReplayBundle, ReplayDumper, ReplayExchange, ReplayExchangeRecord, ReplayManifest, ReplayPrompt,
    ReplayPromptRecord, SessionReplay,
};
pub use secrets::{OsKeychain, SecretStore};
pub use usage::{ModelTally, ProviderTally, UsageDay, UsageLedger, UsageStore, RETAINED_DAYS};
