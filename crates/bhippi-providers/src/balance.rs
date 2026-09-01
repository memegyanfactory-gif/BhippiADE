//! Account balance queries for billing-based LLM providers.
//!
//! Each provider exposes an HTTP endpoint where an authenticated caller can read
//! the current account balance. The result is converted to USD and recorded in
//! the usage ledger so the Settings panel can show it without hitting the
//! network on every read.
//!
//! Network errors are not fatal: a balance query that fails is logged and the
//! previously recorded balance is left in place.

use std::env;

/// The result of a single balance query, in USD. `None` means the provider
/// does not expose balance information for the current account.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Balance {
    pub usd: f64,
}

/// One provider's balance endpoint and how to authenticate against it.
#[derive(Clone, Debug)]
pub struct BalanceEndpoint {
    pub url: String,
    pub auth_header: String,
    pub kind: BalanceKind,
}

/// How a provider's balance endpoint expects to be parsed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BalanceKind {
    /// `{"total_usd": 24.30}` — OpenAI-compatible.
    OpenAiUsd,
    /// `{"data": {"credit_balance": "24.30"}}` — OpenRouter.
    OpenRouter,
    /// `{"accountBalance": 24.30}` — xAI / Grok.
    Xai,
    /// `{"balance": 24.30, "is_active": true}` — Anthropic (console).
    Anthropic,
    /// `{"data": {"available_balance": "24.30", "voucher": "0"}}` — Moonshot.
    Moonshot,
}

impl BalanceKind {
    /// The JSON path the figure lives at.
    fn path(self) -> &'static [&'static str] {
        match self {
            Self::OpenAiUsd => &["total_usd"],
            Self::OpenRouter => &["data", "credit_balance"],
            Self::Xai => &["accountBalance"],
            Self::Anthropic => &["balance"],
            Self::Moonshot => &["data", "available_balance"],
        }
    }

    /// The response value is a string rather than a number.
    fn is_string(self) -> bool {
        matches!(self, Self::OpenRouter | Self::Moonshot)
    }
}

impl BalanceEndpoint {
    /// Resolves the balance endpoint for one provider, or `None` if the provider
    /// does not support balance queries.
    #[must_use]
    pub fn for_provider(provider_id: &str) -> Option<Self> {
        match provider_id {
            "openai" => Some(Self {
                url: "https://api.openai.com/dashboard/billing/credit_grants".to_owned(),
                auth_header: bearer_header("OPENAI_API_KEY"),
                kind: BalanceKind::OpenAiUsd,
            }),
            "openrouter" => Some(Self {
                url: "https://openrouter.ai/api/v1/credits".to_owned(),
                auth_header: bearer_header("OPENROUTER_API_KEY"),
                kind: BalanceKind::OpenRouter,
            }),
            "xai" => Some(Self {
                url: "https://api.x.ai/v1/api-key".to_owned(),
                auth_header: bearer_header("XAI_API_KEY"),
                kind: BalanceKind::Xai,
            }),
            "anthropic" => Some(Self {
                url: "https://api.anthropic.com/v1/organizations/me".to_owned(),
                auth_header: anthropic_header("ANTHROPIC_API_KEY"),
                kind: BalanceKind::Anthropic,
            }),
            "moonshot" => Some(Self {
                url: "https://api.moonshot.cn/v1/users/me/balance".to_owned(),
                auth_header: bearer_header("MOONSHOT_API_KEY"),
                kind: BalanceKind::Moonshot,
            }),
            "groq" => Some(Self {
                url: "https://api.groq.com/openai/v1/dashboard/billing/credit_grants".to_owned(),
                auth_header: bearer_header("GROQ_API_KEY"),
                kind: BalanceKind::OpenAiUsd,
            }),
            _ => None,
        }
    }
}

fn bearer_header(env_key: &str) -> String {
    format!("Bearer {}", env::var(env_key).unwrap_or_default())
}

fn anthropic_header(env_key: &str) -> String {
    format!("Bearer {}", env::var(env_key).unwrap_or_default())
}

/// Parses the provider's balance response into USD.
///
/// Returns `Ok(None)` when the response shape is unrecognised; the caller treats
/// that as "no balance available" rather than an error.
pub fn parse_response(kind: BalanceKind, body: &str) -> Result<Option<f64>, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|error| format!("cannot parse balance response: {error}"))?;
    let mut current = &value;
    for segment in kind.path() {
        current = current
            .get(*segment)
            .ok_or_else(|| format!("missing `{}` in response", kind.path().join(".")))?;
    }
    let usd = if kind.is_string() {
        current
            .as_str()
            .ok_or_else(|| "expected a string".to_owned())?
            .parse::<f64>()
            .map_err(|error| format!("cannot parse number: {error}"))?
    } else {
        current
            .as_f64()
            .ok_or_else(|| "expected a number".to_owned())?
    };
    if usd.is_finite() && usd >= 0.0 {
        Ok(Some(usd))
    } else {
        Ok(None)
    }
}

/// Sends the balance query with a 5 second timeout. Failures are non-fatal.
pub async fn fetch(endpoint: &BalanceEndpoint) -> Option<Balance> {
    let client = reqwest_client();
    let request = client
        .get(&endpoint.url)
        .header("Authorization", &endpoint.auth_header)
        .header("anthropic-version", "2023-06-01")
        .timeout(std::time::Duration::from_secs(5));
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::debug!(%error, "balance request failed");
            return None;
        }
    };
    if !response.status().is_success() {
        tracing::debug!(status = %response.status(), "balance endpoint returned non-2xx");
        return None;
    }
    let body = match response.text().await {
        Ok(body) => body,
        Err(error) => {
            tracing::debug!(%error, "cannot read balance body");
            return None;
        }
    };
    match parse_response(endpoint.kind, &body) {
        Ok(Some(usd)) => Some(Balance { usd }),
        Ok(None) => None,
        Err(error) => {
            tracing::debug!(%error, "cannot parse balance");
            None
        }
    }
}

/// Fetches the balance for one provider. Returns `None` for providers that
/// don't support balance queries or when the query fails.
pub async fn fetch_for_provider(provider_id: &str) -> Option<Balance> {
    let endpoint = BalanceEndpoint::for_provider(provider_id)?;
    fetch(&endpoint).await
}

fn reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("bhippi/0.1 (+balance-query)")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_response() {
        let body = r#"{"total_granted_usd": 100.0, "total_used_usd": 75.7, "total_available_usd": 24.30, "total_usd": 24.30}"#;
        assert_eq!(
            parse_response(BalanceKind::OpenAiUsd, body),
            Ok(Some(24.30))
        );
    }

    #[test]
    fn parses_openrouter_response() {
        let body =
            r#"{"data": {"total_credits": 100.0, "total_usage": 75.7, "credit_balance": "24.30"}}"#;
        assert_eq!(
            parse_response(BalanceKind::OpenRouter, body),
            Ok(Some(24.30))
        );
    }

    #[test]
    fn parses_xai_response() {
        let body = r#"{"accountId": "abc", "accountBalance": 24.30, "securitiesAccount": null}"#;
        assert_eq!(parse_response(BalanceKind::Xai, body), Ok(Some(24.30)));
    }

    #[test]
    fn parses_anthropic_response() {
        let body = r#"{"balance": 24.30, "is_active": true}"#;
        assert_eq!(
            parse_response(BalanceKind::Anthropic, body),
            Ok(Some(24.30))
        );
    }

    #[test]
    fn parses_moonshot_response() {
        let body = r#"{"code": 0, "data": {"available_balance": "24.30", "voucher": "0", "cash": "24.30"}}"#;
        assert_eq!(parse_response(BalanceKind::Moonshot, body), Ok(Some(24.30)));
    }

    #[test]
    fn rejects_missing_field() {
        let body = r#"{"data": {}}"#;
        assert!(parse_response(BalanceKind::OpenRouter, body).is_err());
    }

    #[test]
    fn returns_none_for_unparseable_balance() {
        let body = r#"{"total_usd": -1.0}"#;
        assert_eq!(parse_response(BalanceKind::OpenAiUsd, body), Ok(None));
    }

    #[test]
    fn endpoints_cover_all_billing_providers() {
        for id in [
            "openai",
            "openrouter",
            "xai",
            "anthropic",
            "moonshot",
            "groq",
        ] {
            assert!(
                BalanceEndpoint::for_provider(id).is_some(),
                "{id} needs a balance endpoint"
            );
        }
        for id in [
            "claude", "codex", "grok", "kimi", "ollama", "demo", "opencode",
        ] {
            assert!(
                BalanceEndpoint::for_provider(id).is_none(),
                "{id} does not have a balance endpoint"
            );
        }
    }
}
