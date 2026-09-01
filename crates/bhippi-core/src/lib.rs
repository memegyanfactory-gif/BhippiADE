//! Session orchestration, budgets, scheduling, and automation policy.

#![forbid(unsafe_code)]

mod bus;
mod config;
mod context;
mod logging;
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
pub use logging::{LoggingGuard, SecretRedactor};
pub use replay::{
    ReplayBundle, ReplayDumper, ReplayExchange, ReplayExchangeRecord, ReplayManifest, ReplayPrompt,
    ReplayPromptRecord, SessionReplay,
};
pub use secrets::{OsKeychain, SecretStore};
pub use usage::{ModelTally, ProviderTally, UsageDay, UsageLedger, UsageStore, RETAINED_DAYS};
