use serde::{Deserialize, Serialize};
use specta::Type;

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
