use crate::{
    BudgetScope, DotId, NodeId, NodeKind, PostId, ProviderId, Relation, SessionId, SkillId,
    SourceId, Stage, TickerEventId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

pub type Timestamp = DateTime<Utc>;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    SessionStageChanged {
        session: SessionId,
        from: Stage,
        to: Stage,
    },
    MindmapDelta {
        session: SessionId,
        nodes: Vec<NodeDelta>,
        edges: Vec<EdgeDelta>,
        merged: u16,
    },
    DotAdded {
        session: SessionId,
        dots: Vec<NodeDotDelta>,
        merged: u16,
    },
    SourceFetched {
        session: SessionId,
        source: SourceSummary,
    },
    ProviderHealth {
        provider: ProviderId,
        health: Health,
    },
    TickerEvent {
        event: TickerEventSummary,
    },
    AutomationTick {
        next_run: Option<Timestamp>,
        queue_depth: u32,
    },
    PublishProgress {
        post: PostId,
        step: PublishStep,
        pct: u8,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct NodeDelta {
    pub id: NodeId,
    pub parent_id: Option<NodeId>,
    pub kind: NodeKind,
    pub label: String,
    pub status: NodeStatus,
    pub relevance: Option<f32>,
    pub priority: Option<f32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EdgeDelta {
    pub from: NodeId,
    pub to: NodeId,
    pub relation: Relation,
    pub weight: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct DotSummary {
    pub id: DotId,
    pub claim: String,
    pub source_id: SourceId,
    pub confidence: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct NodeDotDelta {
    pub node: NodeId,
    pub dot: DotSummary,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SourceSummary {
    pub id: SourceId,
    pub title: Option<String>,
    pub url: String,
    pub trust_tier: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TickerEventSummary {
    pub id: TickerEventId,
    pub headline: String,
    pub priority: f32,
    pub source_count: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Frontier,
    Expanding,
    Explored,
    Pruned,
    DeadEnd,
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
pub enum PublishStep {
    Build,
    Verify,
    Swap,
    Record,
    Complete,
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
            Self::SessionStageChanged { session, .. }
            | Self::MindmapDelta { session, .. }
            | Self::DotAdded { session, .. }
            | Self::SourceFetched { session, .. } => Some(*session),
            Self::ErrorRaised { session, .. } | Self::ResyncRequired { session, .. } => *session,
            Self::ProviderHealth { .. }
            | Self::TickerEvent { .. }
            | Self::AutomationTick { .. }
            | Self::PublishProgress { .. }
            | Self::BudgetWarning { .. }
            | Self::SkillPendingApproval { .. }
            | Self::Engine { .. } => None,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::SessionStageChanged { .. } => "session_stage_changed",
            Self::MindmapDelta { .. } => "mindmap_delta",
            Self::DotAdded { .. } => "dot_added",
            Self::SourceFetched { .. } => "source_fetched",
            Self::ProviderHealth { .. } => "provider_health",
            Self::TickerEvent { .. } => "ticker_event",
            Self::AutomationTick { .. } => "automation_tick",
            Self::PublishProgress { .. } => "publish_progress",
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
    use super::{Event, PublishStep};
    use crate::PostId;

    #[test]
    fn event_variants_have_stable_snake_case_tags() {
        let event = Event::PublishProgress {
            post: PostId::new(),
            step: PublishStep::Verify,
            pct: 40,
        };
        let Ok(value) = serde_json::to_value(event) else {
            panic!("event must serialize");
        };

        assert_eq!(value["kind"], "publish_progress");
        assert_eq!(value["step"], "verify");
    }
}
