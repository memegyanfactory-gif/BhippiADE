//! The one trait every LLM backend implements (spec §8.3). The engine never asks for a
//! vendor — routing resolves a [`TaskClass`] against health and capabilities later in S1.

use crate::model::{Capabilities, CompletionRequest, DeltaStream};
use async_trait::async_trait;
use bhippi_types::{Health, Result};

#[async_trait]
pub trait Provider: Send + Sync {
    /// Stable identifier used in logs, replay dumps, and Settings rows.
    fn id(&self) -> &str;

    fn caps(&self) -> &Capabilities;

    /// Streams a completion. Implementations must honour `req.timeout` and surface
    /// cancellation by ending the stream with [`bhippi_types::BhippiError::Provider`].
    async fn complete(&self, req: CompletionRequest) -> Result<DeltaStream>;

    /// Embeddings default to unsupported; vector-capable backends override in S5.
    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Err(bhippi_types::BhippiError::Provider {
            id: self.id().to_owned(),
            reason: "embeddings unsupported".to_owned(),
            retryable: false,
            hint: Some("Route embeddings to a provider that supports them.".to_owned()),
        })
    }

    async fn health(&self) -> Health;

    /// True when this backend needs no network at all (offline path is the product).
    fn offline_capable(&self) -> bool {
        false
    }

    /// Reports the current account balance in USD for billing-based providers.
    /// `None` indicates that this provider does not expose a balance endpoint
    /// or that balance tracking is not applicable.
    async fn account_balance(&self) -> Result<Option<f64>> {
        Ok(None)
    }
}
