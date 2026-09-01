use serde::{Deserialize, Serialize};
use specta::Type;
use std::fmt;
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum BhippiError {
    #[error("provider {id} unavailable: {reason}")]
    Provider {
        /// The catalogue id or vendor label — whatever the user would recognise in
        /// Settings. Never a generated id: this string is read by a human first.
        id: String,
        reason: String,
        retryable: bool,
        hint: Option<String>,
    },
    #[error("budget exceeded: {scope}")]
    Budget {
        scope: BudgetScope,
        used: u64,
        cap: u64,
    },
    #[error("topic out of scope (score {score:.2} < {threshold:.2})")]
    OutOfScope { score: f32, threshold: f32 },
    #[error("gate blocked publication: {gate}")]
    Gate { gate: GateName, detail: String },
    #[error("fetch failed for {url}: {kind}")]
    Fetch {
        url: String,
        kind: FetchErrorKind,
        retryable: bool,
        hint: Option<String>,
    },
    #[error("data: {reason}")]
    Db {
        reason: String,
        retryable: bool,
        hint: Option<String>,
    },
    #[error("configuration: {reason}")]
    Config {
        reason: String,
        hint: Option<String>,
    },
    #[error("secret store: {reason}")]
    Secret {
        reason: String,
        hint: Option<String>,
    },
    #[error("{operation} at {path}: {reason}")]
    Io {
        operation: &'static str,
        path: String,
        reason: String,
        retryable: bool,
        hint: Option<String>,
    },
    #[error("invariant violated: {code}")]
    Invariant { code: &'static str },
}

impl BhippiError {
    #[must_use]
    pub const fn retryable(&self) -> bool {
        match self {
            Self::Provider { retryable, .. }
            | Self::Fetch { retryable, .. }
            | Self::Db { retryable, .. }
            | Self::Io { retryable, .. } => *retryable,
            Self::Budget { .. }
            | Self::OutOfScope { .. }
            | Self::Gate { .. }
            | Self::Config { .. }
            | Self::Secret { .. }
            | Self::Invariant { .. } => false,
        }
    }

    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Provider { hint, .. }
            | Self::Fetch { hint, .. }
            | Self::Db { hint, .. }
            | Self::Config { hint, .. }
            | Self::Secret { hint, .. }
            | Self::Io { hint, .. } => hint.as_deref(),
            Self::Budget { .. }
            | Self::OutOfScope { .. }
            | Self::Gate { .. }
            | Self::Invariant { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BudgetScope {
    DailyTokens,
    DailyWallTime,
    SessionTokens,
    SessionWallTime,
    SessionCost,
}

impl fmt::Display for BudgetScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::DailyTokens => "daily tokens",
            Self::DailyWallTime => "daily wall time",
            Self::SessionTokens => "session tokens",
            Self::SessionWallTime => "session wall time",
            Self::SessionCost => "session cost",
        };
        formatter.write_str(value)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GateName {
    Copyright,
    Images,
    CrawlPolicy,
    Attribution,
    Disclosure,
    Corrections,
    Defamation,
    PersonImagery,
    Facts,
    Style,
    Publish,
}

impl fmt::Display for GateName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FetchErrorKind {
    InvalidUrl,
    RobotsDenied,
    Paywalled,
    Timeout,
    TooLarge,
    HttpStatus,
    Decode,
    Cancelled,
}

impl fmt::Display for FetchErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::BhippiError;

    #[test]
    fn actionable_provider_error_exposes_retry_and_hint() {
        let error = BhippiError::Provider {
            id: "Claude Code".to_owned(),
            reason: "not responding".to_owned(),
            retryable: true,
            hint: Some("Start the provider or choose another one.".to_owned()),
        };

        assert!(error.retryable());
        assert_eq!(
            error.hint(),
            Some("Start the provider or choose another one.")
        );
    }

    /// The message a user reads must name the backend they chose. It once carried a freshly
    /// generated ULID, which told them nothing and looked like a crash.
    #[test]
    fn the_message_names_the_provider_a_human_would_recognise() {
        let error = BhippiError::Provider {
            id: "Claude Code".to_owned(),
            reason: "the CLI answered with nothing".to_owned(),
            retryable: true,
            hint: None,
        };

        assert_eq!(
            error.to_string(),
            "provider Claude Code unavailable: the CLI answered with nothing"
        );
    }
}
