//! OpenAI-compatible adapter (first slice of BHP-017): LM Studio, llama.cpp server,
//! vLLM, Jan and text-generation-webui all speak `/v1/chat/completions` SSE on loopback.

use crate::model::{
    Capabilities, CompletionRequest, CostClass, Delta, DeltaStream, Role as MessageRole, StopReason,
};
use crate::provider::Provider;
use async_trait::async_trait;
use bhippi_types::{BhippiError, Health, Result};
use futures_util::{FutureExt, StreamExt};

pub struct OpenAiCompatProvider {
    id: String,
    label: String,
    base_url: String,
    model: String,
    client: reqwest::Client,
    caps: Capabilities,
}

struct SseState {
    response: reqwest::Response,
    buffer: String,
    finished: bool,
}

impl OpenAiCompatProvider {
    #[must_use]
    pub fn new(id: &str, label: &str, port: u16, model: impl Into<String>) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            base_url: format!("http://127.0.0.1:{port}"),
            model: model.into(),
            client: reqwest::Client::new(),
            caps: Capabilities {
                context_window: 16_384,
                vision: false,
                tools: false,
                streaming: true,
                tokens_per_second: None,
                cost_class: CostClass::FreeLocal,
            },
        }
    }

    fn error(&self, reason: String) -> BhippiError {
        BhippiError::Provider {
            id: self.label.clone(),
            reason,
            retryable: true,
            hint: Some(format!(
                "Check that {} is serving on {} — the endpoint must answer `/v1/models`.",
                self.label, self.base_url
            )),
        }
    }

    /// Lists models from `GET /v1/models` (`data[].id`). Used by detection too.
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let response = self
            .client
            .get(format!("{}/v1/models", self.base_url))
            .timeout(crate::detect::PROBE_TIMEOUT)
            .send()
            .await
            .map_err(|error| self.error(error.to_string()))?;
        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|error| self.error(error.to_string()))?;
        Ok(value
            .get("data")
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(serde_json::Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default())
    }

    fn request_body(&self, req: &CompletionRequest) -> serde_json::Value {
        let mut messages = Vec::with_capacity(req.messages.len() + 1);
        if !req.system.trim().is_empty() {
            messages.push(serde_json::json!({ "role": "system", "content": req.system }));
        }
        for message in &req.messages {
            messages.push(serde_json::json!({
                "role": match message.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                },
                "content": message.content,
            }));
        }
        serde_json::json!({
            "model": req.model.as_deref().unwrap_or(&self.model),
            "messages": messages,
            "stream": true,
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
        })
    }
}

// Local alias so the request-body match stays exhaustive.

/// Pulls one content delta out of an SSE data payload. Pure + tested.
#[must_use]
pub fn parse_sse_data(payload: &str) -> Option<Delta> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    if value.get("error").is_some() {
        return None;
    }
    if let Some(usage) = value.get("usage") {
        let input = usage
            .get("prompt_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let output = usage
            .get("completion_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if input > 0 || output > 0 {
            return Some(Delta::Usage {
                input_tokens: input,
                output_tokens: output,
            });
        }
    }
    let choice = value
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())?;
    let delta = choice.get("delta")?;

    if let Some(reasoning) = delta
        .get("reasoning_content")
        .and_then(serde_json::Value::as_str)
    {
        if !reasoning.is_empty() {
            return Some(Delta::Thinking {
                delta: reasoning.to_owned(),
            });
        }
    }

    if let Some(piece) = delta.get("content").and_then(serde_json::Value::as_str) {
        if !piece.is_empty() {
            return Some(Delta::Text {
                delta: piece.to_owned(),
            });
        }
    }
    None
}

#[async_trait]
impl Provider for OpenAiCompatProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn caps(&self) -> &Capabilities {
        &self.caps
    }

    async fn complete(&self, req: CompletionRequest) -> Result<DeltaStream> {
        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .header("Authorization", "Bearer local")
            .json(&self.request_body(&req))
            .timeout(req.timeout)
            .send()
            .await
            .map_err(|error| self.error(error.to_string()))?;
        if !response.status().is_success() {
            return Err(self.error(format!("answered HTTP {}", response.status().as_u16())));
        }

        // The stream must be 'static, so capture plain data instead of `&self`.
        let label = self.label.clone();
        let base_url = self.base_url.clone();
        let error_of = move |reason: String| BhippiError::Provider {
            id: label.clone(),
            reason,
            retryable: true,
            hint: Some(format!(
                "Check that the server is serving on {base_url} — it must answer `/v1/models`."
            )),
        };

        let stream = futures_util::stream::unfold(
            SseState {
                response,
                buffer: String::new(),
                finished: false,
            },
            move |mut state| {
                // Cloned per poll so the boxed 'static future owns its own copy.
                let error_of = error_of.clone();
                async move {
                    loop {
                        if let Some(break_at) = state.buffer.find('\n') {
                            let line: String = state.buffer.drain(..=break_at).collect();
                            let line = line.trim();
                            if let Some(payload) = line.strip_prefix("data:") {
                                let payload = payload.trim();
                                if payload == "[DONE]" {
                                    state.finished = true;
                                    let tail: Vec<bhippi_types::Result<Delta>> =
                                        vec![Ok(Delta::Done {
                                            stop_reason: StopReason::Completed,
                                        })];
                                    return Some((futures_util::stream::iter(tail), state));
                                }
                                if let Some(delta) = parse_sse_data(payload) {
                                    let items = vec![Ok::<_, BhippiError>(delta)];
                                    return Some((futures_util::stream::iter(items), state));
                                }
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
                                let tail: Vec<bhippi_types::Result<Delta>> =
                                    vec![Ok(Delta::Done {
                                        stop_reason: StopReason::Completed,
                                    })];
                                return Some((futures_util::stream::iter(tail), state));
                            }
                            Err(error) => {
                                state.finished = true;
                                let failure: Vec<bhippi_types::Result<Delta>> = vec![
                                    Err(error_of(error.to_string())),
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
                reason: format!("no answer on {}", self.base_url),
            },
        }
    }

    fn offline_capable(&self) -> bool {
        true
    }
}
