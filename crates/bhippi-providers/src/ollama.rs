//! Ollama native adapter (BHP-018, first slice): `/api/chat` NDJSON streaming over loopback.
//!
//! The probe budget follows spec §8.1d: 400 ms per port, never blocking app start.

use crate::model::{
    parse_ollama_ndjson, Capabilities, CompletionRequest, CostClass, Delta, DeltaStream, StopReason,
};
use crate::provider::Provider;
use async_trait::async_trait;
use bhippi_types::{BhippiError, Health, Result};
use futures_util::{FutureExt, StreamExt};
use std::time::Duration;

/// Per-request probe budget from spec §8.1d.
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(400);

pub struct OllamaProvider {
    base_url: String,
    client: reqwest::Client,
    model: String,
    caps: Capabilities,
}

/// Reads one chunk at a time off the wire, buffering partial NDJSON lines.
struct StreamState {
    response: reqwest::Response,
    buffer: String,
    finished: bool,
}

impl OllamaProvider {
    #[must_use]
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
            model: model.into(),
            caps: Capabilities {
                context_window: 8_192,
                vision: false,
                tools: false,
                streaming: true,
                tokens_per_second: None,
                cost_class: CostClass::FreeLocal,
            },
        }
    }

    fn endpoint(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }

    fn unavailable(&self, reason: String) -> BhippiError {
        BhippiError::Provider {
            id: "Ollama".to_owned(),
            reason,
            retryable: true,
            hint: Some(format!(
                "Start Ollama or check it answers on {} — `GET /api/tags` must respond.",
                self.base_url
            )),
        }
    }

    /// Lists installed models. `Err` when Ollama is unreachable within the probe budget.
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let response = self
            .client
            .get(self.endpoint("/api/tags"))
            .timeout(PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|error| self.unavailable(error.to_string()))?;
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|error| self.unavailable(error.to_string()))?;
        Ok(value
            .get("models")
            .and_then(serde_json::Value::as_array)
            .map(|models| {
                models
                    .iter()
                    .filter_map(|model| model.get("name").and_then(serde_json::Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default())
    }

    fn request_body(&self, req: &CompletionRequest) -> serde_json::Value {
        let messages: Vec<_> = req
            .messages
            .iter()
            .map(|message| {
                serde_json::json!({
                    "role": match message.role {
                        crate::model::Role::System => "system",
                        crate::model::Role::User => "user",
                        crate::model::Role::Assistant => "assistant",
                    },
                    "content": message.content,
                })
            })
            .collect();
        let mut body = serde_json::json!({
            "model": req.model.as_deref().unwrap_or(&self.model),
            "messages": messages,
            "stream": true,
            "options": {
                "temperature": req.temperature,
                "num_predict": req.max_tokens,
            },
        });
        if !req.system.trim().is_empty() {
            body["system"] = serde_json::Value::String(req.system.clone());
        }
        body
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn id(&self) -> &str {
        "ollama"
    }

    fn caps(&self) -> &Capabilities {
        &self.caps
    }

    async fn complete(&self, req: CompletionRequest) -> Result<DeltaStream> {
        let response = self
            .client
            .post(self.endpoint("/api/chat"))
            .json(&self.request_body(&req))
            .timeout(req.timeout)
            .send()
            .await
            .map_err(|error| self.unavailable(error.to_string()))?;
        if !response.status().is_success() {
            return Err(self.unavailable(format!(
                "Ollama answered HTTP {}",
                response.status().as_u16()
            )));
        }

        let stream = futures_util::stream::unfold(
            StreamState {
                response,
                buffer: String::new(),
                finished: false,
            },
            |mut state| {
                async move {
                    loop {
                        if let Some(break_at) = state.buffer.find('\n') {
                            let line: String = state.buffer.drain(..=break_at).collect();
                            let (deltas, done) = parse_ollama_ndjson(line.trim());
                            state.finished |= done;
                            if !deltas.is_empty() {
                                let items = deltas.into_iter().map(Ok).collect::<Vec<_>>();
                                return Some((futures_util::stream::iter(items), state));
                            }
                            continue;
                        }
                        match state.response.chunk().await {
                            Ok(Some(bytes)) => {
                                state.buffer.push_str(&String::from_utf8_lossy(&bytes));
                            }
                            Ok(None) => {
                                if state.finished {
                                    return None;
                                }
                                state.finished = true;
                                let tail: Vec<Result<Delta>> = vec![Ok(Delta::Done {
                                    stop_reason: StopReason::Completed,
                                })];
                                return Some((futures_util::stream::iter(tail), state));
                            }
                            Err(error) => {
                                state.finished = true;
                                let failure: Vec<Result<Delta>> = vec![
                                    Err(BhippiError::Provider {
                                        id: "Ollama".to_owned(),
                                        reason: error.to_string(),
                                        retryable: true,
                                        hint: Some(
                                            "Reconnect Ollama in Settings › Providers and retry."
                                                .to_owned(),
                                        ),
                                    }),
                                    Ok(Delta::Done {
                                        stop_reason: StopReason::Failed,
                                    }),
                                ];
                                return Some((futures_util::stream::iter(failure), state));
                            }
                        }
                    }
                }
                .boxed()
            },
        );

        Ok(stream.flatten().boxed())
    }

    async fn health(&self) -> Health {
        let started = std::time::Instant::now();
        match self.list_models().await {
            Ok(_) => Health::Healthy {
                latency_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
            },
            Err(_) => Health::Unavailable {
                reason: format!("no answer on {} within the probe budget", self.base_url),
            },
        }
    }

    fn offline_capable(&self) -> bool {
        true
    }
}
