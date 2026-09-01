//! Wire-level provider types: capabilities, messages, requests, and streamed deltas.
//!
//! These types cross the IPC boundary, so every one carries `specta::Type` and is mirrored
//! into `ui/src/lib/ipc.ts` by the generated bindings (INV-032).

use bhippi_types::{Health, TaskClass, Timestamp};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::time::Duration;

/// One vendor-owned rolling allowance window.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PlanWindow {
    /// Fraction consumed, in the closed range `0.0..=1.0`.
    pub used_fraction: f32,
    /// Unix seconds for the next reset when the vendor reports it.
    pub resets_at: Option<i64>,
    /// Window length from the vendor, used to distinguish short and weekly buckets.
    pub duration_minutes: Option<u64>,
}

/// How much account information a provider made available without reading credentials.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AccountUsageStatus {
    /// Identity and rolling windows came from a vendor-owned status protocol.
    Live,
    /// The provider confirmed the account, but exposes no numerical plan allowance.
    Authenticated,
    /// The CLI is present but the provider does not expose this information.
    NotReported,
    /// The provider explicitly reported that no account is signed in.
    SignedOut,
    /// A supported account probe failed; the previous good snapshot may still be shown.
    Unavailable,
}

/// Signed-in account identity plus vendor-reported plan usage.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AccountUsage {
    /// Email, workspace, or provider-owned account label. Never a credential value.
    pub account_name: Option<String>,
    pub plan: Option<String>,
    pub status: AccountUsageStatus,
    pub session: Option<PlanWindow>,
    pub weekly: Option<PlanWindow>,
    /// Plain-language reason for a missing value; the UI never invents one.
    pub note: String,
    pub refreshed_at: Timestamp,
}

/// What a backend can do, resolved once per model and cached for 24 h (spec §8.2).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Capabilities {
    pub context_window: u32,
    pub vision: bool,
    pub tools: bool,
    pub streaming: bool,
    pub tokens_per_second: Option<f32>,
    pub cost_class: CostClass,
}

/// Cost bucket used by routing and the budget guard.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CostClass {
    FreeLocal,
    Cheap,
    Standard,
    Premium,
}

/// Author of a message in a completion conversation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// One conversation message. Image parts arrive in S6; text-only until then.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    #[must_use]
    pub const fn user(content: String) -> Self {
        Self {
            role: Role::User,
            content,
        }
    }

    #[must_use]
    pub const fn assistant(content: String) -> Self {
        Self {
            role: Role::Assistant,
            content,
        }
    }
}

/// One inference call as defined by spec §8.3.
#[derive(Clone, Debug, Serialize, Type)]
pub struct CompletionRequest {
    /// Routing hint only — adapters must not branch behaviour on it.
    pub task: TaskClass,
    pub system: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Structured-output schema when supported; validated by the caller, never the prompt alone.
    pub json_schema: Option<serde_json::Value>,
    pub timeout: Duration,
    /// The exact model the user picked. `None` means "whatever this backend defaults to" —
    /// adapters must then send no model at all rather than guessing one, so a vendor's own
    /// default stays the default (ADR-0006: never a silent swap).
    pub model: Option<String>,
    /// Canonical project directory for coding-agent CLIs. Non-CLI providers receive the
    /// same boundary in the system context but do not access the filesystem themselves.
    pub workspace: Option<String>,
    /// Local image files attached to this inference call. CLI adapters pass these through
    /// their native image option when one exists; other vision-capable coding agents receive
    /// a constrained read-only path in the prompt.
    pub image_paths: Vec<String>,
    /// Narrows coding-agent tools for a Computer Use decision. The desktop controller owns
    /// input execution; the provider must never substitute a shell command for an action.
    pub computer_use: bool,
}

impl CompletionRequest {
    /// A sane default request for `task`; callers override fields explicitly.
    #[must_use]
    pub fn new(task: TaskClass, system: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            task,
            system: system.into(),
            messages,
            max_tokens: 2048,
            temperature: 0.7,
            json_schema: None,
            timeout: Duration::from_secs(120),
            model: None,
            workspace: None,
            image_paths: Vec::new(),
            computer_use: false,
        }
    }

    /// Pins the model for this call. An empty or blank name is treated as "no choice".
    #[must_use]
    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model.filter(|name| !name.trim().is_empty());
        self
    }

    /// Locks a request to a canonical project directory.
    #[must_use]
    pub fn with_workspace(mut self, workspace: Option<String>) -> Self {
        self.workspace = workspace.filter(|path| !path.trim().is_empty());
        self
    }

    /// Attaches local images for a vision-capable provider.
    #[must_use]
    pub fn with_images(mut self, image_paths: Vec<String>) -> Self {
        self.image_paths = image_paths
            .into_iter()
            .filter(|path| !path.trim().is_empty())
            .collect();
        self
    }

    /// Marks this as a desktop-decision call so adapters can suppress shell/edit tools.
    #[must_use]
    pub const fn for_computer_use(mut self) -> Self {
        self.computer_use = true;
        self
    }
}

/// A chunk of a completion stream (BHP-010). Provider streams carry model output only —
/// tool activity and permission requests are engine facts emitted one layer up (ADR-0006).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Delta {
    Text {
        delta: String,
    },
    Thinking {
        delta: String,
    },
    /// One step the backend ran, as the backend itself named it.
    ///
    /// ADR-0006 reserved tool activity for the engine because the demo provider was the
    /// only thing that produced any. Real coding CLIs announce every file they read and
    /// every command they run in their own event stream, and dropping that on the floor
    /// left the activity dock permanently empty on exactly the providers people use. The
    /// engine still owns permissions and its own steps; this carries only what the
    /// vendor reported about itself.
    Step {
        id: String,
        /// The shared verb vocabulary — read, edited, ran, searched, fetched, planned.
        verb: String,
        title: String,
        detail: String,
        /// False while the step is still running.
        done: bool,
    },
    Usage {
        input_tokens: u64,
        output_tokens: u64,
    },
    /// Where the account stands against its plan's rolling windows, as the vendor
    /// reported it mid-turn. Drives the live limit gauge and the pre-emptive warning.
    Limit {
        /// `allowed`, `allowed_warning`, or `rejected`.
        status: String,
        /// Fraction of the short rolling window consumed, 0.0 – 1.0.
        session_used: Option<f32>,
        session_resets_at: Option<i64>,
        /// Fraction of the weekly allowance consumed, 0.0 – 1.0.
        weekly_used: Option<f32>,
        weekly_resets_at: Option<i64>,
    },
    Done {
        stop_reason: StopReason,
    },
}

/// Why a stream ended.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Completed,
    MaxTokens,
    Cancelled,
    Failed,
}

/// A detected provider row for Settings › Providers (subset until S1 probing lands).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProviderInfo {
    pub id: String,
    pub label: String,
    pub kind: ProviderKind,
    pub models: Vec<String>,
    pub health: Health,
    /// True when detection found only the credential, not a reachable backend.
    pub offered: bool,
    pub detected_at: Timestamp,
    /// CLI on PATH · server reachable · credential present · demo always.
    pub installed: bool,
    /// CLI `--version` output, trimmed. Servers/clouds report no version yet.
    pub version: Option<String>,
    /// User preference — only enabled providers appear in the chat picker.
    pub enabled: bool,
    /// True when the backend pins a model with its own flag yet names no known list —
    /// the composer then offers a free-text field rather than guessing vendor ids.
    pub accepts_custom_model: bool,
    /// The active TCP port discovered during server detection.
    #[serde(default)]
    pub detected_port: Option<u16>,
}

impl ProviderInfo {
    /// Whether this backend can actually answer a prompt right now.
    ///
    /// `installed` alone is not that question, and conflating the two is what let a local
    /// LLM server that was merely *present on disk* be picked as the default. A local
    /// server answers prompts only while it is listening on a port; a CLI answers
    /// whenever its launcher exists; the demo always answers.
    #[must_use]
    pub fn usable(&self) -> bool {
        match self.kind {
            ProviderKind::Demo => true,
            // Reachability, not presence. Nothing else here is a promise the backend
            // can keep, and `detected_port` is set only by a probe that got an answer.
            ProviderKind::LocalServer => {
                self.detected_port.is_some() && matches!(self.health, Health::Healthy { .. })
            }
            ProviderKind::Cli | ProviderKind::CloudApi => self.installed,
        }
    }

    /// True when the backend is on this machine but not currently answering.
    ///
    /// Distinct from "absent": the fix is to start it, not to install it, and the two
    /// deserve different words in Settings.
    #[must_use]
    pub fn installed_but_idle(&self) -> bool {
        self.kind == ProviderKind::LocalServer && self.offered && !self.usable()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Cli,
    CloudApi,
    LocalServer,
    Demo,
}

/// The stream returned by [`crate::Provider::complete`].
pub type DeltaStream = futures_core::stream::BoxStream<'static, bhippi_types::Result<Delta>>;

/// Parses Ollama's NDJSON chat stream body into deltas. Exposed for fixture tests.
///
/// Each line is either a chat message chunk or the terminal summary carrying eval counts.
/// Returns the deltas produced by `text` and whether the terminal line was present.
#[must_use]
pub fn parse_ollama_ndjson(text: &str) -> (Vec<Delta>, bool) {
    let mut deltas = Vec::new();
    let mut done = false;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("error").is_some() {
            continue;
        }
        if value.get("done").and_then(serde_json::Value::as_bool) == Some(true) {
            deltas.push(Delta::Usage {
                input_tokens: value
                    .get("prompt_eval_count")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default(),
                output_tokens: value
                    .get("eval_count")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default(),
            });
            deltas.push(Delta::Done {
                stop_reason: StopReason::Completed,
            });
            done = true;
            continue;
        }
        if let Some(piece) = value
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(serde_json::Value::as_str)
        {
            if !piece.is_empty() {
                deltas.push(Delta::Text {
                    delta: piece.to_owned(),
                });
            }
        }
    }
    (deltas, done)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_ollama_ndjson, Capabilities, CompletionRequest, Delta, Message, ProviderInfo,
        ProviderKind, StopReason,
    };
    use bhippi_types::{Health, TaskClass};
    use chrono::Utc;
    use std::time::Duration;

    #[test]
    fn ndjson_parse_extracts_text_then_usage_and_done() {
        let body = concat!(
            "{\"message\":{\"role\":\"assistant\",\"content\":\"Hel\"},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\"lo\"},\"done\":false}\n",
            "{\"done\":true,\"prompt_eval_count\":9,\"eval_count\":21}\n",
        );

        let (deltas, done) = parse_ollama_ndjson(body);

        assert!(done);
        assert_eq!(
            deltas,
            vec![
                Delta::Text {
                    delta: "Hel".to_owned()
                },
                Delta::Text {
                    delta: "lo".to_owned()
                },
                Delta::Usage {
                    input_tokens: 9,
                    output_tokens: 21
                },
                Delta::Done {
                    stop_reason: StopReason::Completed
                },
            ]
        );
    }

    #[test]
    fn ndjson_parse_skips_blank_and_malformed_lines_without_done() {
        let (deltas, done) = parse_ollama_ndjson("\nnot-json\n{\"message\":{\"content\":\"x\"}}\n");
        assert!(!done);
        assert_eq!(deltas.len(), 1);
    }

    #[test]
    fn completion_request_defaults_are_sane() {
        let req = CompletionRequest::new(
            TaskClass::Planner,
            "system",
            vec![Message::user("q".to_owned())],
        );
        assert_eq!(req.timeout, Duration::from_secs(120));
        assert!(req.json_schema.is_none());
        let caps = Capabilities {
            context_window: 1,
            vision: false,
            tools: false,
            streaming: true,
            tokens_per_second: None,
            cost_class: super::CostClass::FreeLocal,
        };
        assert!(caps.streaming);
    }

    #[test]
    fn provider_info_carries_detected_port() {
        let info = ProviderInfo {
            id: "bionic".to_owned(),
            label: "Bionic".to_owned(),
            kind: ProviderKind::LocalServer,
            models: vec!["qwen3.8-27b".to_owned()],
            health: Health::Healthy { latency_ms: 5 },
            offered: false,
            detected_at: Utc::now(),
            installed: true,
            version: None,
            enabled: true,
            accepts_custom_model: true,
            detected_port: Some(1234),
        };
        assert_eq!(info.detected_port, Some(1234));
    }
}
