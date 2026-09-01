//! The token ledger: per-provider, per-day tallies persisted beside `config.toml`.
//!
//! Kept deliberately small — one JSON file of daily rollups, never a row per turn — so
//! the usage gauge can be answered from one read and the file stays a few kilobytes
//! after a year. Costs are stored in whole micro-dollars so repeated addition cannot
//! drift the way summed floats do.

use bhippi_types::{BhippiError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// How many days of history the ledger keeps. Older rows are dropped on write.
pub const RETAINED_DAYS: usize = 90;

/// One provider's spend inside one day.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProviderTally {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// USD x 1_000_000. Zero for unmetered backends (subscription CLIs, local, demo).
    pub cost_micros: u64,
    pub turns: u32,
    /// Current account balance in USD x 1_000_000 (for API providers with billing).
    /// This is the most recent balance recorded for this provider.
    pub balance_micros: Option<u64>,
    /// Per-model spend for this provider on this day. Keyed by model name; absent
    /// entries mean the model was never used that day.
    pub models: BTreeMap<String, ModelTally>,
}

impl ProviderTally {
    #[must_use]
    pub const fn total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }

    fn absorb(&mut self, other: Self) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cost_micros = self.cost_micros.saturating_add(other.cost_micros);
        self.turns = self.turns.saturating_add(other.turns);
        // Last balance wins — we keep the most recent snapshot, not a sum.
        if other.balance_micros.is_some() {
            self.balance_micros = other.balance_micros;
        }
        // Per-model rows are merged.
        for (model_id, model_tally) in other.models {
            self.models.entry(model_id).or_default().absorb(model_tally);
        }
    }
}

/// One model's spend inside one day for one provider.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelTally {
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// USD x 1_000_000.
    pub cost_micros: u64,
    pub turns: u32,
}

impl ModelTally {
    #[must_use]
    pub const fn total_tokens(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }

    fn absorb(&mut self, other: Self) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cost_micros = self.cost_micros.saturating_add(other.cost_micros);
        self.turns = self.turns.saturating_add(other.turns);
    }
}

/// Every provider's spend on one calendar day (local time, `YYYY-MM-DD`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UsageDay {
    pub date: String,
    pub providers: BTreeMap<String, ProviderTally>,
}

impl UsageDay {
    #[must_use]
    pub fn total_tokens(&self) -> u64 {
        self.providers
            .values()
            .fold(0, |sum, tally| sum.saturating_add(tally.total_tokens()))
    }

    #[must_use]
    pub fn cost_micros(&self) -> u64 {
        self.providers
            .values()
            .fold(0, |sum, tally| sum.saturating_add(tally.cost_micros))
    }
}

/// The whole ledger: days ascending, oldest first.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UsageLedger {
    pub days: Vec<UsageDay>,
}

impl UsageLedger {
    /// Adds one turn's tally into `date`, creating the day row when it is new.
    pub fn record(&mut self, date: &str, provider_id: &str, tally: ProviderTally) {
        if !self.days.iter().any(|day| day.date == date) {
            self.days.push(UsageDay {
                date: date.to_owned(),
                providers: BTreeMap::new(),
            });
            self.days.sort_by(|a, b| a.date.cmp(&b.date));
        }
        if let Some(day) = self.days.iter_mut().find(|day| day.date == date) {
            day.providers
                .entry(provider_id.to_owned())
                .or_default()
                .absorb(tally);
        }
        self.prune();
    }

    /// Records an account balance snapshot for a provider on a given day.
    /// This does not count as a "turn" and only updates the balance field.
    pub fn record_balance(&mut self, date: &str, provider_id: &str, balance_micros: u64) {
        if !self.days.iter().any(|day| day.date == date) {
            self.days.push(UsageDay {
                date: date.to_owned(),
                providers: BTreeMap::new(),
            });
            self.days.sort_by(|a, b| a.date.cmp(&b.date));
        }
        if let Some(day) = self.days.iter_mut().find(|day| day.date == date) {
            day.providers
                .entry(provider_id.to_owned())
                .or_default()
                .balance_micros = Some(balance_micros);
        }
        self.prune();
    }

    /// Sums every provider tally across the inclusive date range `from..=to`.
    #[must_use]
    pub fn tally_between(&self, from: &str, to: &str) -> BTreeMap<String, ProviderTally> {
        let mut out: BTreeMap<String, ProviderTally> = BTreeMap::new();
        for day in self
            .days
            .iter()
            .filter(|day| day.date.as_str() >= from && day.date.as_str() <= to)
        {
            for (id, tally) in &day.providers {
                out.entry(id.clone()).or_default().absorb(tally.clone());
            }
        }
        out
    }

    /// The stored row for one date, if the app recorded anything that day.
    #[must_use]
    pub fn day(&self, date: &str) -> Option<&UsageDay> {
        self.days.iter().find(|day| day.date == date)
    }

    fn prune(&mut self) {
        if self.days.len() > RETAINED_DAYS {
            let excess = self.days.len() - RETAINED_DAYS;
            self.days.drain(0..excess);
        }
    }
}

/// Reads and writes `usage.json`. Writes are atomic (temp file then rename) and
/// serialised by an internal lock, so two turns finishing together cannot interleave a
/// read-modify-write and lose one of the tallies.
#[derive(Debug)]
pub struct UsageStore {
    path: PathBuf,
    lock: tokio::sync::Mutex<()>,
}

impl UsageStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: tokio::sync::Mutex::new(()),
        }
    }

    /// `~/.bhippi/usage.json`, next to the config file.
    ///
    /// # Errors
    /// Fails when neither `HOME` nor `USERPROFILE` is set.
    pub fn default_path() -> Result<PathBuf> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .ok_or_else(|| usage_error("home directory is unavailable"))?;
        Ok(PathBuf::from(home).join(".bhippi").join("usage.json"))
    }

    /// Loads the ledger. A missing file is an empty ledger, not an error; a *corrupt*
    /// file is reported so the user finds out rather than silently losing history.
    ///
    /// # Errors
    /// Fails when the file exists but cannot be read or parsed.
    pub async fn load(&self) -> Result<UsageLedger> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => serde_json::from_str::<UsageLedger>(&text).map_err(|error| {
                usage_error(format!("cannot parse {}: {error}", self.path.display()))
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(UsageLedger::default())
            }
            Err(error) => Err(usage_error(format!(
                "cannot read {}: {error}",
                self.path.display()
            ))),
        }
    }

    /// Adds one turn to the ledger and returns the ledger as it now stands on disk.
    ///
    /// # Errors
    /// Fails when the ledger cannot be read back or written.
    pub async fn record(
        &self,
        date: &str,
        provider_id: &str,
        tally: ProviderTally,
    ) -> Result<UsageLedger> {
        let _guard = self.lock.lock().await;
        let mut ledger = self.load().await?;
        ledger.record(date, provider_id, tally);
        self.write(&ledger).await?;
        Ok(ledger)
    }

    /// Clears history — everything, or one provider's rows across every day.
    ///
    /// # Errors
    /// Fails when the ledger cannot be read back or written.
    pub async fn clear(&self, provider_id: Option<&str>) -> Result<UsageLedger> {
        let _guard = self.lock.lock().await;
        let ledger = match provider_id {
            None => UsageLedger::default(),
            Some(id) => {
                let mut current = self.load().await?;
                for day in &mut current.days {
                    day.providers.remove(id);
                }
                current.days.retain(|day| !day.providers.is_empty());
                current
            }
        };
        self.write(&ledger).await?;
        Ok(ledger)
    }

    async fn write(&self, ledger: &UsageLedger) -> Result<()> {
        let text = serde_json::to_string_pretty(ledger)
            .map_err(|error| usage_error(format!("cannot encode the usage ledger: {error}")))?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| usage_error(format!("cannot create {}: {error}", parent.display())))?;
        let temp = self.path.with_extension("json.tmp");
        tokio::fs::write(&temp, text)
            .await
            .map_err(|error| usage_error(format!("cannot write {}: {error}", temp.display())))?;
        tokio::fs::rename(&temp, &self.path)
            .await
            .map_err(|error| {
                usage_error(format!("cannot replace {}: {error}", self.path.display()))
            })?;
        Ok(())
    }
}

fn usage_error(reason: impl Into<String>) -> BhippiError {
    BhippiError::Config {
        reason: reason.into(),
        hint: Some("Delete ~/.bhippi/usage.json to start a fresh ledger.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(input: u64, output: u64, micros: u64) -> ProviderTally {
        ProviderTally {
            input_tokens: input,
            output_tokens: output,
            cost_micros: micros,
            turns: 1,
            balance_micros: None,
            models: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn repeated_turns_accumulate_into_one_day_row() {
        let mut ledger = UsageLedger::default();
        ledger.record("2026-08-26", "anthropic", tally(100, 50, 900));
        ledger.record("2026-08-26", "anthropic", tally(10, 5, 90));
        ledger.record("2026-08-26", "ollama", tally(7, 3, 0));

        assert_eq!(ledger.days.len(), 1);
        let day = ledger
            .day("2026-08-26")
            .unwrap_or_else(|| panic!("the recorded day must exist"));
        let anthropic = day
            .providers
            .get("anthropic")
            .unwrap_or_else(|| panic!("the recorded provider must exist"));
        assert_eq!(anthropic.input_tokens, 110);
        assert_eq!(anthropic.output_tokens, 55);
        assert_eq!(anthropic.cost_micros, 990);
        assert_eq!(anthropic.turns, 2);
        assert_eq!(day.total_tokens(), 175);
    }

    #[test]
    fn history_is_capped_and_stays_ordered() {
        let mut ledger = UsageLedger::default();
        for index in 1..=(RETAINED_DAYS + 10) {
            let date = format!("2026-{:02}-{:02}", 1 + index / 28, 1 + index % 28);
            ledger.record(&date, "openai", tally(1, 1, 1));
        }
        assert!(ledger.days.len() <= RETAINED_DAYS);
        let mut sorted = ledger.days.clone();
        sorted.sort_by(|a, b| a.date.cmp(&b.date));
        assert_eq!(sorted, ledger.days);
    }

    #[test]
    fn a_range_sums_every_day_it_covers() {
        let mut ledger = UsageLedger::default();
        ledger.record("2026-08-24", "openai", tally(10, 10, 5));
        ledger.record("2026-08-25", "openai", tally(20, 20, 5));
        ledger.record("2026-08-26", "openai", tally(40, 40, 5));

        let window = ledger.tally_between("2026-08-25", "2026-08-26");
        let openai = window
            .get("openai")
            .unwrap_or_else(|| panic!("the summed provider must exist"));
        assert_eq!(openai.total_tokens(), 120);
        assert_eq!(openai.turns, 2);
    }

    #[tokio::test]
    async fn a_missing_file_reads_empty_and_records_survive_a_reload() {
        let dir = std::env::temp_dir().join(format!("bhippi-usage-{}", std::process::id()));
        let path = dir.join("usage.json");
        let _ignored = tokio::fs::remove_dir_all(&dir).await;
        let store = UsageStore::new(&path);

        assert_eq!(store.load().await, Ok(UsageLedger::default()));
        store
            .record("2026-08-26", "openai", tally(5, 5, 12))
            .await
            .unwrap_or_else(|error| panic!("recording must succeed: {error}"));
        let reloaded = store
            .load()
            .await
            .unwrap_or_else(|error| panic!("reloading must succeed: {error}"));
        let day = reloaded
            .day("2026-08-26")
            .unwrap_or_else(|| panic!("the recorded day must survive a reload"));
        assert_eq!(day.total_tokens(), 10);

        store
            .clear(Some("openai"))
            .await
            .unwrap_or_else(|error| panic!("clearing must succeed: {error}"));
        let after = store
            .load()
            .await
            .unwrap_or_else(|error| panic!("reloading must succeed: {error}"));
        assert!(after.days.is_empty());
        let _ignored = tokio::fs::remove_dir_all(&dir).await;
    }
}
