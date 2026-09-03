//! Provider catalogue, detection, streaming adapters, and the one [`Provider`] trait.
//!
//! Detection follows spec §8.1 within its probe budget and never blocks app start.
//! Install/update recipes are explicit argv — never a shell string (INV-003) — and
//! credential values are presence-only, never read into storage (INV-002).

#![cfg_attr(
    test,
    allow(clippy::expect_used, clippy::unwrap_used),
    doc = "Tests may panic on purpose: `expect` is how a test states its precondition, and a panic there is a failing test rather than a crashed app. The workspace `deny` stands everywhere else."
)]

pub mod account;
pub mod asset_provider;
pub mod balance;
pub mod catalog;
pub mod cli;
mod command;
pub mod demo;
pub mod detect;
pub mod eject;
pub mod embedding;
pub mod fault;
pub mod model;
pub mod ollama;
pub mod openai_compat;
pub mod pricing;
pub mod provider;
pub mod transcript;
pub mod update;

pub use crate::asset_provider::{
    AssetCapability, AssetProvider, CloudText3DProvider, LocalImageProvider,
};

use crate::catalog::InstallSpec;
use crate::command::resolve_command;
use std::time::Duration;

pub use crate::account::{probe_account, probe_accounts};
pub use crate::balance::{fetch_for_provider, BalanceEndpoint};
pub use crate::catalog::{spec, CATALOG};
pub use crate::cli::CliProvider;
pub use crate::detect::{
    detect, detect_local_servers, detection_fingerprint, extract_model_names, merge_detection,
    parse_model_lines, parse_model_list, stamp_enabled,
};
pub use crate::eject::{eject, Ejected};
pub use crate::embedding::{
    cosine, decode, embed, encode, Embedding, EMBEDDING_DIM, EMBEDDING_MODEL,
};
pub use crate::fault::{advise, classify, Advice, FaultKind, Remedy};
pub use crate::model::McpServer;
pub use crate::model::{
    AccountUsage, AccountUsageStatus, Capabilities, CompletionRequest, Delta, DeltaStream, Message,
    PlanWindow, ProviderInfo, ProviderKind, Role, StopReason,
};
pub use crate::openai_compat::OpenAiCompatProvider;
pub use crate::pricing::{is_metered, pricing, pricing_for, Basis, Pricing};

/// Whether a backend can host an MCP server Bhippi attaches to a turn (SPA-202): Claude
/// Code reads `--mcp-config`, Codex takes `-c mcp_servers.*` overrides. The rest cannot,
/// and a server they cannot host is never pretended into the prompt.
#[must_use]
pub fn supports_mcp(provider_id: &str) -> bool {
    matches!(provider_id, "claude" | "codex")
}
pub use crate::provider::Provider;
pub use crate::update::{check as check_update, Verdict};

/// Hard ceiling for one install/update run; npm cold caches can be slow.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(900);

/// Runs an install **or** update recipe (they are the same command: reinstall latest).
///
/// Returns the last few output lines for the Settings progress card. Failures are
/// reported to the caller — the silent updater logs and moves on without surfacing.
pub async fn run_recipe(recipe: &InstallSpec) -> std::result::Result<String, String> {
    let resolved = resolve_command(recipe.program).ok_or_else(|| {
        format!(
            "{} is not available. Install it first, then restart Bhippi.",
            recipe.program
        )
    })?;
    let mut command = resolved.command();
    command
        .args(recipe.args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    let child = command.output();
    let output = tokio::time::timeout(INSTALL_TIMEOUT, child)
        .await
        .map_err(|_| "timed out after 900s".to_owned())?
        .map_err(|error| error.to_string())?;

    let mut tail = String::new();
    for stream in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(stream);
        let lines: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let start = lines.len().saturating_sub(3);
        for line in &lines[start..] {
            if !tail.is_empty() {
                tail.push_str(" · ");
            }
            tail.push_str(line.trim());
        }
    }
    let tail = tail.chars().take(400).collect::<String>();

    if output.status.success() {
        Ok(tail)
    } else {
        Err(if tail.is_empty() {
            format!("exited with {}", output.status)
        } else {
            format!("{} ({})", output.status, tail)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Delta, Message, Role, StopReason};
    use crate::model::CostClass;

    #[test]
    fn message_constructors_set_roles() {
        let user = Message::user("hi".to_owned());
        let assistant = Message::assistant("hello".to_owned());
        assert_eq!(user.role, Role::User);
        assert_eq!(assistant.role, Role::Assistant);
    }

    #[test]
    fn delta_tags_round_trip_through_json() {
        let delta = Delta::Text {
            delta: "hello".to_owned(),
        };
        let value = serde_json::to_value(&delta).unwrap_or_default();
        assert_eq!(value["kind"], "text");

        let done = Delta::Done {
            stop_reason: StopReason::Completed,
        };
        let value = serde_json::to_value(&done).unwrap_or_default();
        assert_eq!(value["kind"], "done");
        assert_eq!(value["stop_reason"], "completed");
        assert_ne!(CostClass::FreeLocal, CostClass::Premium);
    }
}
