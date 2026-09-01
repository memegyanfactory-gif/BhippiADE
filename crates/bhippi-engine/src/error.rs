use serde::{Deserialize, Serialize};
use specta::Type;
use std::fmt;
use thiserror::Error;

/// The engine's typed error surface. Every variant carries an actionable `hint()` —
/// a user-facing sentence that says what to do next (mirrors `BhippiError`; the engine
/// keeps its own so the library has no dependency on the core plumbing crate).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineFault {
    pub message: String,
    pub code: EngineErrorCode,
    pub hint: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EngineErrorCode {
    Manifest,
    Scene,
    Transaction,
    Schema,
    Asset,
    Io,
    NotFound,
    Gate,
    Action,
    Build,
}

#[derive(Clone, Debug, Error)]
pub enum EngineError {
    #[error("game manifest: {0}")]
    Manifest(String, Option<String>),
    #[error("scene: {0}")]
    Scene(String, Option<String>),
    #[error("transaction rejected: {0}")]
    Transaction(String, Option<String>),
    #[error("schema: {0}")]
    Schema(String, Option<String>),
    #[error("asset: {0}")]
    Asset(String, Option<String>),
    #[error("{operation} at {path}: {reason}")]
    Io {
        operation: &'static str,
        path: String,
        reason: String,
        hint: Option<String>,
    },
    #[error("not found: {0}")]
    NotFound(String, Option<String>),
    #[error("gate blocked: {0}")]
    Gate(String, Option<String>),
    #[error("action: {0}")]
    Action(String, Option<String>),
    #[error("build: {0}")]
    Build(String, Option<String>),
}

impl EngineError {
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Manifest(_, hint)
            | Self::Scene(_, hint)
            | Self::Transaction(_, hint)
            | Self::Schema(_, hint)
            | Self::Asset(_, hint)
            | Self::NotFound(_, hint)
            | Self::Gate(_, hint)
            | Self::Action(_, hint)
            | Self::Build(_, hint) => hint.as_deref(),
            Self::Io { hint, .. } => hint.as_deref(),
        }
    }

    /// The error the IPC layer can hand to the UI — JSON-safe, specta-typed.
    #[must_use]
    pub fn into_fault(self) -> EngineFault {
        let message = self.to_string();
        let hint = self.hint().map(str::to_owned);
        EngineFault {
            code: self.code(),
            message,
            hint,
        }
    }

    #[must_use]
    pub fn code(&self) -> EngineErrorCode {
        match self {
            Self::Manifest(..) => EngineErrorCode::Manifest,
            Self::Scene(..) => EngineErrorCode::Scene,
            Self::Transaction(..) => EngineErrorCode::Transaction,
            Self::Schema(..) => EngineErrorCode::Schema,
            Self::Asset(..) => EngineErrorCode::Asset,
            Self::Io { .. } => EngineErrorCode::Io,
            Self::NotFound(..) => EngineErrorCode::NotFound,
            Self::Gate(..) => EngineErrorCode::Gate,
            Self::Action(..) => EngineErrorCode::Action,
            Self::Build(..) => EngineErrorCode::Build,
        }
    }
}

impl fmt::Display for EngineErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Manifest => "manifest",
            Self::Scene => "scene",
            Self::Transaction => "transaction",
            Self::Schema => "schema",
            Self::Asset => "asset",
            Self::Io => "io",
            Self::NotFound => "not_found",
            Self::Gate => "gate",
            Self::Action => "action",
            Self::Build => "build",
        })
    }
}

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::{EngineError, EngineFault};

    #[test]
    fn faults_are_json_safe_with_a_hint() {
        let fault: EngineFault = EngineError::NotFound(
            "level_02".to_owned(),
            Some("Open an existing scene from the content drawer.".to_owned()),
        )
        .into_fault();

        assert_eq!(fault.code.to_string(), "not_found");
        assert!(fault.message.contains("level_02"));
        assert!(fault.hint.is_some());
    }
}
