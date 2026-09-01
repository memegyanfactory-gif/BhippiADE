use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Planning,
    Expanding,
    Synthesising,
    FactCheck,
    Writing,
    Imaging,
    Seo,
    Review,
    Publishing,
    Done,
    Failed,
    Cancelled,
    Rejected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Manual,
    Timer,
    Ticker,
    Skill,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TaskClass {
    Planner,
    Expander,
    Extractor,
    Classifier,
    Vision,
    Writer,
    Editor,
    SkillAuthor,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    Concept,
    Entity,
    Claim,
    Question,
    Counterpoint,
    Timeline,
    Metric,
    SourceCluster,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    Causes,
    Enables,
    CompetesWith,
    PartOf,
    Contradicts,
    Precedes,
    FundedBy,
    BuiltOn,
    BenchmarksAgainst,
}

impl Stage {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Expanding => "expanding",
            Self::Synthesising => "synthesising",
            Self::FactCheck => "fact_check",
            Self::Writing => "writing",
            Self::Imaging => "imaging",
            Self::Seo => "seo",
            Self::Review => "review",
            Self::Publishing => "publishing",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Rejected => "rejected",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "planning" => Some(Self::Planning),
            "expanding" => Some(Self::Expanding),
            "synthesising" => Some(Self::Synthesising),
            "fact_check" => Some(Self::FactCheck),
            "writing" => Some(Self::Writing),
            "imaging" => Some(Self::Imaging),
            "seo" => Some(Self::Seo),
            "review" => Some(Self::Review),
            "publishing" => Some(Self::Publishing),
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Failed | Self::Cancelled | Self::Rejected
        )
    }
}

impl Origin {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Timer => "timer",
            Self::Ticker => "ticker",
            Self::Skill => "skill",
        }
    }
}
