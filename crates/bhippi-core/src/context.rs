//! Per-turn context telemetry: what a request actually carries, by category.
//!
//! This is the measurement layer of the Token Engine ("Phase A — Measurement
//! first" in `ui/BHIPPI_TOKEN_ENGINE_IMPLEMENTATION_PLAN.md`). For every answered
//! turn it records an estimate of the prompt the model saw, split into the
//! architectural categories the Token Engine later optimises: system rules,
//! workspace/repository context, project rules, skills, computer use, engine
//! state, multi-provider handoff notes, conversation history, task directives,
//! the reserved response budget, and tool definitions.
//!
//! Estimates use the same `bytes / 4` rule of thumb as `bhippi_app::chat`, so a
//! number here means exactly what the existing context-window guard meant. The
//! point is comparability between turns and categories, not a vendor tokeniser:
//! a per-vendor tokeniser would be exact, out of date the week a vendor changes
//! it, and would not change which category is eating the context window.
//!
//! This telemetry is strictly local (INV-039). The store persists counts and
//! metadata only — never message text, source code, secrets, or prompts.

use bhippi_types::{BhippiError, Result, Timestamp};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// How many recently-recorded samples the store keeps. The daily usage ledger caps
/// by day; a context sample is *per turn*, which is far more numerous, so the cap is
/// a count rather than a calendar. 2_000 samples is a bit over three months at a
/// busy five-turns-a-day pace and stays a few hundred kilobytes of JSON.
pub const RETAINED_SAMPLES: usize = 2_000;

/// A slice of the prompt the model saw. Each variant is one place the Token Engine
/// can later spend tokens better, so categories are the accounting ledger's columns.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ContextCategory {
    /// The static chat system prompt (who Bhippi is, how it answers).
    #[default]
    System,
    /// Everything injected about the repository: the workspace prompt plus the
    /// engine-state block. Fixed overhead per turn before any tooling improves it.
    Workspace,
    /// The project's own `.bhippi/rules.md`, rendered through the rules prompt.
    ProjectRules,
    /// Activated skill directives (`@skill` prompts).
    Skills,
    /// Computer Use authorisation/denial notes and the computer-use prompt.
    ComputerUse,
    /// The live engine map (scene, entity count, digest).
    Engine,
    /// The multi-provider conversation-handoff note.
    Handoff,
    /// The full message history.
    Conversation,
    /// Per-turn directives: effort and the design system brief.
    TaskDirectives,
    /// The output budget reserved for the answer (`max_tokens`).
    ReservedResponse,
    /// Tool schemas active in the request. Zero in the current architecture, which
    /// injects no tool definitions into requests (measured report, Phase A3).
    ToolSchemas,
    /// The bounded `bhippi-project-state@1` projection included for this turn.
    ProjectState,
    /// Compact registry cards included before any full contract is retrieved.
    CapabilityIndex,
    /// Capability contracts retrieved by id for the active task.
    RetrievedContracts,
    /// Contract bytes loaded from the active task cache.
    CacheLoad,
    /// Repair evidence and focused failure context retained after a failed attempt.
    Repair,
    /// A turn the no-model fast path answered on its own (GAD-035): the parameter edit was
    /// resolved, lowered and applied in Rust and no provider was called. The row exists so
    /// "how many follow-ups never reached a provider" is a measured number rather than a
    /// claim — every other category on such a sample is zero, and that is the point.
    FastPath,
    /// The design base: the always-on index plus the sections Rust selected for this turn
    /// and any `design_query` answers (ADR-0046).
    DesignBase,
    /// The design memory: the rendered taste profile and the approved lessons that matched
    /// this turn (ADR-0046).
    DesignMemory,
}

impl ContextCategory {
    /// A snake_case id for reports and IPC views.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Workspace => "workspace",
            Self::ProjectRules => "project_rules",
            Self::Skills => "skills",
            Self::ComputerUse => "computer_use",
            Self::Engine => "engine",
            Self::Handoff => "handoff",
            Self::Conversation => "conversation",
            Self::TaskDirectives => "task_directives",
            Self::ReservedResponse => "reserved_response",
            Self::ToolSchemas => "tool_schemas",
            Self::ProjectState => "project_state",
            Self::CapabilityIndex => "capability_index",
            Self::RetrievedContracts => "retrieved_contracts",
            Self::CacheLoad => "cache_load",
            Self::Repair => "repair",
            Self::FastPath => "fast_path",
            Self::DesignBase => "design_base",
            Self::DesignMemory => "design_memory",
        }
    }
}

/// Four bytes per token: the long-standing rule of thumb for English prose and
/// code, and the same heuristic the chat context-window guard already runs on.
pub const ESTIMATED_BYTES_PER_TOKEN: u64 = 4;

#[must_use]
pub fn estimate_text_tokens(text: &str) -> u64 {
    u64::try_from(text.len()).map_or(u64::MAX, |bytes| bytes / ESTIMATED_BYTES_PER_TOKEN)
}

/// The conversation slice of [`estimate`]: message *content* plus a fixed 8 bytes of
/// per-message framing, exactly as `bhippi_app::chat::estimate_tokens` counts it, so
/// category sums stay comparable with the existing window guard.
///
/// [`estimate`]: crate::context::estimate_text_tokens
#[must_use]
pub fn estimate_history_tokens(messages: &[&str]) -> u64 {
    let bytes: usize = messages.iter().map(|message| message.len() + 8).sum();
    u64::try_from(bytes).map_or(u64::MAX, |bytes| bytes / ESTIMATED_BYTES_PER_TOKEN)
}

/// A per-turn, per-category accounting of the request: which slices of the prompt
/// cost how many estimated tokens. Builders are additive so `bhippi_app::chat` can
/// fill it from the same strings it is already assembling.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContextManifest {
    categories: BTreeMap<ContextCategory, u64>,
}

impl ContextManifest {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the estimated tokens of `text` under `category`.
    pub fn add_text(&mut self, category: ContextCategory, text: &str) -> &mut Self {
        self.add_estimate(category, estimate_text_tokens(text))
    }

    /// Adds a confirmed token count under `category` (used where the bytes are not
    /// the whole story, such as the response budget or the message-framing overhead).
    pub fn add_estimate(&mut self, category: ContextCategory, tokens: u64) -> &mut Self {
        *self.categories.entry(category).or_insert(0) = self
            .categories
            .get(&category)
            .copied()
            .unwrap_or(0)
            .saturating_add(tokens);
        self
    }

    /// Records the conversation-history estimate.
    pub fn with_history(&mut self, messages: &[&str]) -> &mut Self {
        self.add_estimate(
            ContextCategory::Conversation,
            estimate_history_tokens(messages),
        )
    }

    /// The estimated tokens under one category; zero when nothing was recorded.
    #[must_use]
    pub fn category(&self, category: ContextCategory) -> u64 {
        self.categories.get(&category).copied().unwrap_or(0)
    }

    /// Sum of every category's estimate.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.categories
            .values()
            .fold(0u64, |sum, tokens| sum.saturating_add(*tokens))
    }

    /// The category carrying the most estimated tokens, and its count.
    #[must_use]
    pub fn largest(&self) -> Option<(ContextCategory, u64)> {
        self.categories
            .iter()
            .max_by_key(|(_, tokens)| **tokens)
            .map(|(category, tokens)| (*category, *tokens))
    }

    /// `0.0..=1.0` of the manifest's total attributed to `category`.
    #[must_use]
    pub fn share(&self, category: ContextCategory) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        (self.category(category) as f64 / total as f64).clamp(0.0, 1.0)
    }

    /// All category subtotals, for persistence and reports.
    #[must_use]
    pub fn categories(&self) -> &BTreeMap<ContextCategory, u64> {
        &self.categories
    }
}

/// One recorded turn. The point of the row is the shape of the prompt that was
/// sent, not its bytes — so only counts and metadata are persisted, never content.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextSample {
    pub turn_id: String,
    pub conversation_id: String,
    pub project: String,
    pub at: Timestamp,
    pub provider_id: String,
    pub model: Option<String>,
    /// Estimated prompt tokens per category.
    pub categories: BTreeMap<ContextCategory, u64>,
    /// The same number `bhippi_app`'s context-window guard computed (`estimate_tokens`):
    /// system plus history plus the reserved response. Kept separately from the category
    /// map so the guard's number is preserved exactly even as category accounting refines.
    pub estimated_total: u64,
    /// Messages in the history slice.
    pub history_messages: u32,
    /// The answer budget reserved for this turn (`max_tokens`).
    pub reserved_output: u64,
    /// The provider's context window in tokens; `0` when uncapped or unknown.
    pub context_window_tokens: u64,
    /// True when the assembled prompt already filled the provider's window.
    pub over_window: bool,
    /// True when a multi-provider handoff note was present in this turn's prompt.
    pub handoff: bool,
    /// How many separate requests the turn drove. Computer Use iterates the loop; a
    /// plain chat turn is one request, and later phases will split multi-step turns.
    pub stream_requests: u32,
    /// Provider-reported input tokens when available. Estimates remain separate.
    pub measured_input_tokens: Option<u64>,
    /// Per-turn material-contract cache accounting; zero means no cache activity.
    pub cache_hits: u32,
    pub cache_misses: u32,
    pub cache_bytes_loaded: u64,
}

/// Error status and an estimate that still made it out the door are both admissible.
/// Anything that was recorded is truth from the moment it was recorded.
impl Default for ContextSample {
    fn default() -> Self {
        Self {
            turn_id: String::new(),
            conversation_id: String::new(),
            project: String::new(),
            at: Utc::now(),
            provider_id: String::new(),
            model: None,
            categories: BTreeMap::new(),
            estimated_total: 0,
            history_messages: 0,
            reserved_output: 0,
            context_window_tokens: 0,
            over_window: false,
            handoff: false,
            stream_requests: 1,
            measured_input_tokens: None,
            cache_hits: 0,
            cache_misses: 0,
            cache_bytes_loaded: 0,
        }
    }
}

/// The whole recorded history of context samples, newest first.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ContextLog {
    /// Newest first, so the store is a bounded append.
    pub samples: Vec<ContextSample>,
}

impl ContextLog {
    /// Drops samples beyond [`RETAINED_SAMPLES`], keeping the newest.
    pub fn prune(&mut self) {
        let excess = self.samples.len().saturating_sub(RETAINED_SAMPLES);
        if excess > 0 {
            self.samples.truncate(RETAINED_SAMPLES);
        }
    }

    /// Samples recorded at or after `since`, oldest first — the natural window order.
    #[must_use]
    pub fn since(&self, since: DateTime<Utc>) -> Vec<&ContextSample> {
        let mut rows: Vec<&ContextSample> = self
            .samples
            .iter()
            .filter(|sample| sample.at >= since)
            .collect();
        rows.sort_by_key(|sample| sample.at);
        rows
    }
}

/// Aggregates across a window of samples. Pure so it is testable without a clock,
/// and reused by the IPC summary in `bhippi_app`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContextTotals {
    pub samples: usize,
    pub estimated_total: u64,
    pub reserved_output: u64,
    pub over_window: usize,
    pub handoff: usize,
    pub measured_input_tokens: u64,
    pub measured_samples: usize,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_bytes_loaded: u64,
    pub by_category: BTreeMap<ContextCategory, u64>,
}

/// Sums every sample recorded at or after `since`.
#[must_use]
pub fn sum_totals(log: &ContextLog, since: DateTime<Utc>) -> ContextTotals {
    let mut totals = ContextTotals::default();
    for sample in log.since(since) {
        totals.samples = totals.samples.saturating_add(1);
        totals.estimated_total = totals
            .estimated_total
            .saturating_add(sample.estimated_total);
        totals.reserved_output = totals
            .reserved_output
            .saturating_add(sample.reserved_output);
        if sample.over_window {
            totals.over_window = totals.over_window.saturating_add(1);
        }
        if sample.handoff {
            totals.handoff = totals.handoff.saturating_add(1);
        }
        if let Some(tokens) = sample.measured_input_tokens {
            totals.measured_input_tokens = totals.measured_input_tokens.saturating_add(tokens);
            totals.measured_samples = totals.measured_samples.saturating_add(1);
        }
        totals.cache_hits = totals
            .cache_hits
            .saturating_add(u64::from(sample.cache_hits));
        totals.cache_misses = totals
            .cache_misses
            .saturating_add(u64::from(sample.cache_misses));
        totals.cache_bytes_loaded = totals
            .cache_bytes_loaded
            .saturating_add(sample.cache_bytes_loaded);
        for (category, tokens) in &sample.categories {
            *totals.by_category.entry(*category).or_insert(0) = totals
                .by_category
                .get(category)
                .copied()
                .unwrap_or(0)
                .saturating_add(*tokens);
        }
    }
    totals
}

/// Reads and writes `context.json`. Writes are atomic (temp file then rename) and
/// serialised by an internal lock, so two turns finishing together cannot interleave
/// a read-modify-write and lose one of the samples.
#[derive(Debug)]
pub struct ContextSampleStore {
    path: PathBuf,
    lock: tokio::sync::Mutex<()>,
}

impl ContextSampleStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: tokio::sync::Mutex::new(()),
        }
    }

    /// `~/.bhippi/context.json`, next to the config file and the usage ledger.
    ///
    /// # Errors
    /// Fails when neither `HOME` nor `USERPROFILE` is set.
    pub fn default_path() -> Result<PathBuf> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .ok_or_else(|| context_error("home directory is unavailable"))?;
        Ok(PathBuf::from(home).join(".bhippi").join("context.json"))
    }

    /// Loads the log. A missing file is an empty log, not an error; a *corrupt* file
    /// is reported so the user finds out rather than silently losing history.
    ///
    /// # Errors
    /// Fails when the file exists but cannot be read or parsed.
    pub async fn load(&self) -> Result<ContextLog> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => serde_json::from_str::<ContextLog>(&text).map_err(|error| {
                context_error(format!("cannot parse {}: {error}", self.path.display()))
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ContextLog::default()),
            Err(error) => Err(context_error(format!(
                "cannot read {}: {error}",
                self.path.display()
            ))),
        }
    }

    /// Records one turn's sample newest-first, pruning anything past the cap.
    ///
    /// # Errors
    /// Fails when the log cannot be read back or written.
    pub async fn record(&self, sample: ContextSample) -> Result<ContextLog> {
        let _guard = self.lock.lock().await;
        let mut log = self.load().await?;
        log.samples.insert(0, sample);
        log.prune();
        self.write(&log).await?;
        Ok(log)
    }

    /// Clears the whole context history.
    ///
    /// # Errors
    /// Fails when the empty log cannot be written.
    pub async fn clear(&self) -> Result<()> {
        let _guard = self.lock.lock().await;
        self.write(&ContextLog::default()).await
    }

    async fn write(&self, log: &ContextLog) -> Result<()> {
        let text = serde_json::to_string_pretty(log)
            .map_err(|error| context_error(format!("cannot encode the context log: {error}")))?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            context_error(format!("cannot create {}: {error}", parent.display()))
        })?;
        let temp = self.path.with_extension("json.tmp");
        tokio::fs::write(&temp, text)
            .await
            .map_err(|error| context_error(format!("cannot write {}: {error}", temp.display())))?;
        tokio::fs::rename(&temp, &self.path)
            .await
            .map_err(|error| {
                context_error(format!("cannot replace {}: {error}", self.path.display()))
            })?;
        Ok(())
    }
}

fn context_error(reason: impl Into<String>) -> BhippiError {
    BhippiError::Config {
        reason: reason.into(),
        hint: Some("Delete ~/.bhippi/context.json to start a fresh log.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        turn_id: &str,
        at: &str,
        total: u64,
        categories: &[(ContextCategory, u64)],
    ) -> ContextSample {
        let mut row = ContextSample {
            turn_id: turn_id.to_owned(),
            conversation_id: "c1".to_owned(),
            project: "p".to_owned(),
            at: DateTime::<Utc>::from_timestamp(unix(at), 0)
                .unwrap_or_else(|| panic!("the test instant must exist")),
            provider_id: "demo".to_owned(),
            estimated_total: total,
            ..ContextSample::default()
        };
        for (category, tokens) in categories {
            *row.categories.entry(*category).or_insert(0) += *tokens;
        }
        row
    }

    fn unix(iso: &str) -> i64 {
        iso.parse::<DateTime<Utc>>()
            .unwrap_or_else(|error| panic!("the test timestamp must parse: {error}"))
            .timestamp()
    }

    #[test]
    fn the_estimator_counts_four_bytes_to_a_token() {
        assert_eq!(estimate_text_tokens(""), 0);
        assert_eq!(estimate_text_tokens("aaaa"), 1);
        assert_eq!(estimate_text_tokens("aaaaaaaa"), 2);
        assert_eq!(
            estimate_text_tokens("aaa"),
            0,
            "partial tokens round down like the guard"
        );
        assert_eq!(estimate_history_tokens(&[]), 0);
        assert_eq!(
            estimate_history_tokens(&["aaaa"]),
            3,
            "an 8-byte framing is added per message"
        );
    }

    #[test]
    fn a_manifest_sums_categories_and_names_the_largest() {
        let mut manifest = ContextManifest::new();
        manifest.add_text(ContextCategory::System, "aaaa");
        manifest.add_text(ContextCategory::Conversation, "aaaaaaaa");
        manifest.add_estimate(ContextCategory::ReservedResponse, 2_048);

        assert_eq!(manifest.category(ContextCategory::System), 1);
        assert_eq!(manifest.category(ContextCategory::Conversation), 2);
        assert_eq!(manifest.total(), 2_051);
        assert_eq!(
            manifest.largest(),
            Some((ContextCategory::ReservedResponse, 2_048))
        );
        assert!(
            (manifest.share(ContextCategory::ReservedResponse) - 2_048.0 / 2_051.0).abs() < 1e-9
        );
    }

    #[test]
    fn history_overhead_is_counted_once_per_message() {
        let messages = ["bbbb", "cccc", "dddd"];
        assert_eq!(estimate_history_tokens(&messages), 9); // 3 * (4 + 8) / 4
    }

    #[tokio::test]
    async fn a_missing_file_reads_empty_and_records_survive_a_reload() {
        let dir = std::env::temp_dir().join(format!("bhippi-context-{}", std::process::id()));
        let path = dir.join("context.json");
        let _ignored = tokio::fs::remove_dir_all(&dir).await;
        let store = ContextSampleStore::new(&path);

        assert_eq!(store.load().await, Ok(ContextLog::default()));
        store
            .record(sample("t1", "2026-08-26T10:00:00Z", 900, &[]))
            .await
            .unwrap_or_else(|error| panic!("recording must succeed: {error}"));
        let reloaded = store
            .load()
            .await
            .unwrap_or_else(|error| panic!("reloading must succeed: {error}"));
        assert_eq!(reloaded.samples.len(), 1);
        assert_eq!(reloaded.samples[0].turn_id, "t1");
        assert_eq!(reloaded.samples[0].estimated_total, 900);

        store
            .clear()
            .await
            .unwrap_or_else(|error| panic!("clearing must succeed: {error}"));
        assert_eq!(
            store
                .load()
                .await
                .unwrap_or_else(|error| panic!("reloading after a clear must succeed: {error}")),
            ContextLog::default()
        );
        let _ignored = tokio::fs::remove_dir_all(&dir).await;
    }

    #[test]
    fn the_log_keeps_only_the_newest_and_orders_windows_oldest_first() {
        let mut log = ContextLog::default();
        let total = RETAINED_SAMPLES + 25;
        // Newest first, exactly as the store records.
        for index in (0..total).rev() {
            let day = index % 28 + 1;
            log.samples.push(sample(
                &format!("t{index}"),
                &format!("2026-08-{:02}T10:00:00Z", day),
                10,
                &[],
            ));
        }
        log.prune();
        assert_eq!(log.samples.len(), RETAINED_SAMPLES);
        assert_eq!(
            log.samples[0].turn_id, "t2024",
            "the newest sample survives the cap"
        );
        assert_eq!(
            log.samples.last().map(|sample| sample.turn_id.as_str()),
            Some("t25"),
            "the 25 oldest samples (t0..t24) are dropped"
        );

        let cutoff = DateTime::<Utc>::from_timestamp(unix("2026-08-20T00:00:00Z"), 0)
            .unwrap_or_else(|| panic!("the cutoff instant must exist"));
        let window = log.since(cutoff);
        assert!(
            window.windows(2).all(|pair| pair[0].at <= pair[1].at),
            "window rows run oldest first"
        );
        assert!(
            window
                .iter()
                .all(|sample| sample.at >= cutoff && sample.at < cutoff + chrono::Duration::days(9)),
            "only samples inside the requested window are returned"
        );
    }

    #[test]
    fn sum_totals_aggregates_only_inside_the_window() {
        let mut log = ContextLog::default();
        log.samples.push(sample(
            "old",
            "2026-08-01T10:00:00Z",
            100,
            &[(ContextCategory::System, 20)],
        ));
        log.samples.push(sample(
            "new",
            "2026-08-26T10:00:00Z",
            50,
            &[(ContextCategory::System, 10), (ContextCategory::Handoff, 5)],
        ));
        let cutoff = DateTime::<Utc>::from_timestamp(unix("2026-08-20T00:00:00Z"), 0)
            .unwrap_or_else(|| panic!("the cutoff instant must exist"));
        log.samples[1].measured_input_tokens = Some(47);
        log.samples[1].cache_hits = 2;
        log.samples[1].cache_misses = 1;
        log.samples[1].cache_bytes_loaded = 512;
        let totals = sum_totals(&log, cutoff);
        assert_eq!(totals.samples, 1);
        assert_eq!(totals.estimated_total, 50);
        assert_eq!(totals.by_category.get(&ContextCategory::System), Some(&10));
        assert_eq!(totals.by_category.get(&ContextCategory::Handoff), Some(&5));
        assert_eq!(totals.by_category.get(&ContextCategory::Conversation), None);
        assert_eq!(totals.measured_input_tokens, 47);
        assert_eq!(totals.measured_samples, 1);
        assert_eq!(totals.cache_hits, 2);
        assert_eq!(totals.cache_misses, 1);
        assert_eq!(totals.cache_bytes_loaded, 512);
    }
}
