//! Application status surfaced to the chrome (title bar pill, status bar).

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AppStatus {
    pub version: String,
    /// Label of the default answerer right now.
    pub active_provider: String,
    pub active_provider_id: String,
    /// True when the default answerer is the labelled offline demo (ADR-0006 §4).
    pub demo_mode: bool,
    /// Every detection row, enabled or not — Settings › Providers renders these.
    pub providers: Vec<bhippi_providers::ProviderInfo>,
    /// Rows the chat picker may show: enabled **and** usable (ADR-0006).
    pub chat_options: Vec<bhippi_providers::ProviderInfo>,
    pub tokens_today: u64,
    /// The model the user last picked per provider, so the composer opens where they left
    /// it rather than resetting to a default they did not choose.
    pub last_model: std::collections::BTreeMap<String, String>,
    /// The provider the user last picked, when it is still enabled and reachable. `None`
    /// leaves the composer on `active_provider_id` — a backend that has gone away is
    /// never silently swapped for another one.
    pub last_provider: Option<String>,
}
