//! The Demo provider: offline, deterministic, clearly labelled (ADR-0006).
//!
//! It exists so the Chat surface's full event protocol — thinking, streamed text, usage,
//! completion — can be exercised with zero backends installed. It never touches the network
//! and always identifies itself as `demo`, which `AppStatus` surfaces as a badge in the UI.

use crate::model::{Capabilities, CompletionRequest, CostClass, Delta, Role, StopReason};
use crate::provider::Provider;
use async_trait::async_trait;
use bhippi_types::{Health, Result};
use futures_util::StreamExt;
use std::time::Duration;

/// Pause between streamed pieces; slow enough to watch, fast enough to not annoy.
const PIECE_DELAY: Duration = Duration::from_millis(14);

pub struct DemoProvider {
    caps: Capabilities,
}

impl Default for DemoProvider {
    fn default() -> Self {
        Self {
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
}

#[async_trait]
impl Provider for DemoProvider {
    fn id(&self) -> &str {
        "demo"
    }

    fn caps(&self) -> &Capabilities {
        &self.caps
    }

    async fn complete(&self, req: CompletionRequest) -> Result<crate::model::DeltaStream> {
        let last_user = req
            .messages
            .iter()
            .rev()
            .find(|message| message.role == Role::User)
            .map(|message| message.content.clone())
            .unwrap_or_default();

        let reply = script_reply(&last_user);
        let mut pieces: Vec<bhippi_types::Result<Delta>> = vec![Ok(Delta::Thinking {
            delta: "Composing an answer from what Bhippi already knows…".to_owned(),
        })];
        pieces.extend(reply.split_inclusive(' ').map(|piece| {
            Ok(Delta::Text {
                delta: piece.to_owned(),
            })
        }));
        pieces.push(Ok(Delta::Usage {
            input_tokens: estimate_tokens(&req),
            output_tokens: reply.split_whitespace().count() as u64,
        }));
        pieces.push(Ok(Delta::Done {
            stop_reason: StopReason::Completed,
        }));

        let stream = futures_util::stream::iter(pieces).then(|item| async move {
            tokio::time::sleep(PIECE_DELAY).await;
            item
        });

        Ok(stream.boxed())
    }

    async fn health(&self) -> Health {
        Health::Healthy { latency_ms: 0 }
    }

    fn offline_capable(&self) -> bool {
        true
    }
}

/// Deterministic, useful copy for the demo. Topics stay inside technology/AI by design.
fn script_reply(question: &str) -> String {
    let question = question.trim();
    if question.is_empty() {
        return "Say anything and I will answer. I am the demo provider — connect Ollama in \
Settings › Providers and this thread starts running on a real model."
            .to_owned();
    }
    format!(
        "I hear you: “{question}”.\n\nRight now I am the **demo provider** — scripted, offline, \
and honest about it. Here is where Bhippi stands:\n\n\
- **Chat** works today. With [Ollama](https://ollama.com) running locally this same thread \
streams from your own model instead of me.\n\
- **Research** lands next: the engine will expand a topic into a live mind map, reading real \
sources and showing every step in this thread.\n\
- Everything is inspectable — each claim will trace to its source, and consequential actions \
will ask you first.\n\n\
Ask me about technology or AI, or open Settings › Providers to go live."
    )
}

fn estimate_tokens(req: &CompletionRequest) -> u64 {
    let words: usize = req
        .system
        .split_whitespace()
        .chain(
            req.messages
                .iter()
                .flat_map(|message| message.content.split_whitespace()),
        )
        .count();
    ((words as f32) * 1.3).ceil() as u64
}
