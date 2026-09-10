use crate::{BudgetScope, ProviderId, SessionId, SkillId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

pub type Timestamp = DateTime<Utc>;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    ProviderHealth {
        provider: ProviderId,
        health: Health,
    },
    AutomationTick {
        next_run: Option<Timestamp>,
        queue_depth: u32,
    },
    BudgetWarning {
        scope: BudgetScope,
        used: u64,
        cap: u64,
    },
    ErrorRaised {
        code: ErrorCode,
        message: String,
        hint: Option<String>,
        session: Option<SessionId>,
    },
    SkillPendingApproval {
        skill: SkillId,
        capabilities: Vec<Capability>,
    },
    ResyncRequired {
        session: Option<SessionId>,
        reason: ResyncReason,
    },
    Engine {
        event: crate::EngineEvent,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Health {
    Healthy { latency_ms: u32 },
    Degraded { reason: String },
    Unavailable { reason: String },
    Disabled,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    ProviderUnavailable,
    BudgetExceeded,
    OutOfScope,
    GateBlocked,
    FetchFailed,
    Data,
    Configuration,
    SecretStore,
    Io,
    InvariantViolated,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Net,
    FsRead,
    FsWrite,
    Script,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ResyncReason {
    CoalescerOverloaded,
    SubscriberLagged,
}

impl Event {
    #[must_use]
    pub const fn session_id(&self) -> Option<SessionId> {
        match self {
            Self::ErrorRaised { session, .. } | Self::ResyncRequired { session, .. } => *session,
            Self::ProviderHealth { .. }
            | Self::AutomationTick { .. }
            | Self::BudgetWarning { .. }
            | Self::SkillPendingApproval { .. }
            | Self::Engine { .. } => None,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::ProviderHealth { .. } => "provider_health",
            Self::AutomationTick { .. } => "automation_tick",
            Self::BudgetWarning { .. } => "budget_warning",
            Self::ErrorRaised { .. } => "error_raised",
            Self::SkillPendingApproval { .. } => "skill_pending_approval",
            Self::ResyncRequired { .. } => "resync_required",
            Self::Engine { .. } => "engine",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Event, Health};
    use crate::ProviderId;

    #[test]
    fn event_variants_have_stable_snake_case_tags() {
        let event = Event::ProviderHealth {
            provider: ProviderId::new(),
            health: Health::Healthy { latency_ms: 42 },
        };
        let Ok(value) = serde_json::to_value(event) else {
            panic!("event must serialize");
        };

        assert_eq!(value["kind"], "provider_health");
        assert_eq!(value["health"]["status"], "healthy");
    }
}
