//! Usage metering as the chrome and Settings render it.
//!
//! One rule keeps the gauge honest: **`limit_tokens` and `fraction` are always measured
//! over the same window as `total_tokens`**. The daily cap from `[budget]` is scaled by
//! the number of days in the window, so a monthly view is not a daily ring in disguise.
//!
//! Costs are estimates from `bhippi_providers::pricing` — list prices for each vendor's
//! default mid-tier model. Unmetered backends (subscription CLIs, local servers, the
//! offline demo) report `metered: false` and a zero cost, which is the truth, not a gap.

use bhippi_core::{BudgetConfig, UsageLedger};
use bhippi_providers::{AccountUsage, AccountUsageStatus, PlanWindow, ProviderInfo, ProviderKind};
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::time::Instant;

/// The shortest history the chart ever draws. A one-day window still wants a month of
/// context behind it, or the panel answers "how much today" with a single column.
const MIN_CHART_DAYS: i64 = 30;

/// Account probes are intentionally outside fast provider detection (INV-062). This cache
/// keeps automatic UI refreshes fresh without spawning four vendor CLIs every 15 seconds.
const ACCOUNT_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Default)]
pub struct AccountUsageCache {
    entries: BTreeMap<String, AccountUsage>,
    last_probe: Option<Instant>,
}

impl AccountUsageCache {
    /// Refreshes all installed CLI accounts when stale, or immediately for a user click.
    pub async fn refresh(&mut self, providers: &[ProviderInfo], force: bool) {
        let has_installed_cli = providers
            .iter()
            .any(|row| row.kind == ProviderKind::Cli && row.installed);
        if !has_installed_cli {
            // Startup detection may still be running. Do not suppress the next UI poll
            // for a minute just because the first request raced an empty registry.
            self.last_probe = None;
            return;
        }
        let due = self
            .last_probe
            .is_none_or(|last| last.elapsed() >= ACCOUNT_REFRESH_INTERVAL);
        if !force && !due {
            return;
        }
        let fresh = bhippi_providers::probe_accounts(providers).await;
        for (id, snapshot) in fresh {
            let merged = self.entries.get(&id).map_or(snapshot.clone(), |old| {
                merge_account_snapshot(old, snapshot)
            });
            self.entries.insert(id, merged);
        }
        self.last_probe = Some(Instant::now());
    }

    /// Merges a rolling-window event emitted by a real provider turn.
    pub fn merge_live_limits(
        &mut self,
        provider_id: &str,
        session: Option<PlanWindow>,
        weekly: Option<PlanWindow>,
    ) {
        let entry = self
            .entries
            .entry(provider_id.to_owned())
            .or_insert_with(|| AccountUsage {
                account_name: None,
                plan: None,
                status: AccountUsageStatus::Live,
                session: None,
                weekly: None,
                note: String::new(),
                refreshed_at: Utc::now(),
            });
        if session.is_some() {
            entry.session = session;
        }
        if weekly.is_some() {
            entry.weekly = weekly;
        }
        entry.status = AccountUsageStatus::Live;
        entry.note = "Live vendor report from the most recent turn.".to_owned();
        entry.refreshed_at = Utc::now();
    }

    #[must_use]
    pub fn snapshot(&self) -> BTreeMap<String, AccountUsage> {
        self.entries.clone()
    }
}

fn merge_account_snapshot(old: &AccountUsage, mut fresh: AccountUsage) -> AccountUsage {
    let same_account = old.account_name == fresh.account_name
        || old.account_name.is_none()
        || fresh.account_name.is_none();
    if !same_account {
        return fresh;
    }
    if fresh.account_name.is_none() {
        fresh.account_name.clone_from(&old.account_name);
    }
    if fresh.plan.is_none() {
        fresh.plan.clone_from(&old.plan);
    }
    if fresh.session.is_none() {
        fresh.session.clone_from(&old.session);
    }
    if fresh.weekly.is_none() {
        fresh.weekly.clone_from(&old.weekly);
    }
    if fresh.session.is_some() || fresh.weekly.is_some() {
        if matches!(
            fresh.status,
            AccountUsageStatus::Authenticated | AccountUsageStatus::NotReported
        ) {
            fresh.status = AccountUsageStatus::Live;
            fresh.note.clone_from(&old.note);
            fresh.refreshed_at = old.refreshed_at;
        } else if fresh.status == AccountUsageStatus::Unavailable {
            fresh.note = format!("{} Showing the last vendor snapshot.", fresh.note);
            fresh.refreshed_at = old.refreshed_at;
        }
    }
    fresh
}

/// The span a usage figure covers.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UsageWindow {
    #[default]
    Day,
    Week,
    Month,
    /// The whole retained history (`bhippi_core::RETAINED_DAYS`).
    Quarter,
}

impl UsageWindow {
    /// Days the window covers, including today.
    const fn days(self) -> i64 {
        match self {
            Self::Day => 1,
            Self::Week => 7,
            Self::Month => 30,
            Self::Quarter => 90,
        }
    }

    /// Days the chart draws. Never fewer than the window it sits under, or the graph
    /// would contradict the totals printed above it.
    const fn chart_days(self) -> i64 {
        if self.days() > MIN_CHART_DAYS {
            self.days()
        } else {
            MIN_CHART_DAYS
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Day => "Today",
            Self::Week => "Last 7 days",
            Self::Month => "Last 30 days",
            Self::Quarter => "Last 90 days",
        }
    }
}

/// One provider's spend inside the requested window.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProviderUsage {
    pub id: String,
    pub label: String,
    pub kind: ProviderKind,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub turns: u32,
    /// Estimated list-price spend in USD. Always `0.0` when `metered` is false.
    pub cost_usd: f64,
    /// True when this vendor bills per token, so the dollar figure carries meaning.
    pub metered: bool,
    /// The ceiling for this window, or `None` when the provider is uncapped — the ring
    /// then renders as an empty track rather than as instantly full.
    pub limit_tokens: Option<u64>,
    /// `0.0..=1.0` of `limit_tokens` already spent. `0.0` when uncapped.
    pub fraction: f64,
    /// True when the provider is enabled and reachable right now.
    pub available: bool,
    /// `0.0..=1.0` of the window's tokens that ran through this provider.
    pub share_of_tokens: f64,
    /// `0.0..=1.0` of the window's estimated cost. Always `0.0` for an unmetered
    /// backend, which spends tokens but no money.
    pub share_of_cost: f64,
    /// Fixed slot in the chart's categorical palette, by catalogue position — so a
    /// provider keeps its colour when another one drops out of the window (ADR-0011).
    pub color_slot: u8,
    /// Current account balance in USD. This is populated from the balance field
    /// in the usage ledger when available.
    pub balance_usd: Option<f64>,
    /// Signed-in identity and real provider plan windows. A missing weekly value is never
    /// replaced with Bhippi's local cap.
    pub account: Option<AccountUsage>,
    /// Per-model breakdown for this provider in the window.
    pub models: Vec<ModelUsage>,
}

/// One model's spend inside the requested window for one provider.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ModelUsage {
    pub id: String,
    pub label: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub turns: u32,
    pub cost_usd: f64,
}

/// One provider's slice of one day, for the chart's per-provider series.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct DayProviderPoint {
    pub id: String,
    pub total_tokens: u64,
    pub cost_usd: f64,
}

/// One column of the usage chart.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct UsageDayPoint {
    /// `YYYY-MM-DD`, local time.
    pub date: String,
    pub total_tokens: u64,
    pub cost_usd: f64,
    /// Only the providers that actually spent something that day — a zero row would be
    /// a line drawn along the axis for a backend that was not even running.
    pub providers: Vec<DayProviderPoint>,
}

/// Everything the gauge, its drop-up, and Settings › Usage need in one read.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct UsageSummary {
    pub window: UsageWindow,
    pub window_label: String,
    pub active_provider_id: String,
    /// The provider the ring reports on — present even when it has spent nothing.
    pub active: ProviderUsage,
    /// Every provider with history in the window, plus every enabled one, heaviest first.
    pub providers: Vec<ProviderUsage>,
    pub total_tokens: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub total_turns: u32,
    /// Mean tokens per answered turn in the window; `0` when nothing was answered.
    pub tokens_per_turn: u64,
    /// The window's span as the header prints it, e.g. `28 Jul – 26 Aug`.
    pub range_label: String,
    /// Seconds until the daily window rolls over at local midnight.
    pub resets_in_seconds: u64,
    /// Oldest to newest, zero-filled — `window.chart_days()` entries.
    pub days: Vec<UsageDayPoint>,
}

/// Builds the summary from the ledger, the budget, and the current detection rows.
///
/// Pure apart from the `now` it is handed, so the shape is unit-testable without a clock.
#[must_use]
#[cfg(test)]
pub fn summarise(
    ledger: &UsageLedger,
    budget: &BudgetConfig,
    providers: &[ProviderInfo],
    active_provider_id: &str,
    window: UsageWindow,
    now: DateTime<Local>,
) -> UsageSummary {
    summarise_with_accounts(
        ledger,
        budget,
        providers,
        active_provider_id,
        window,
        now,
        &BTreeMap::new(),
    )
}

/// Builds the same ledger summary and attaches independently refreshed vendor accounts.
#[must_use]
pub fn summarise_with_accounts(
    ledger: &UsageLedger,
    budget: &BudgetConfig,
    providers: &[ProviderInfo],
    active_provider_id: &str,
    window: UsageWindow,
    now: DateTime<Local>,
    accounts: &BTreeMap<String, AccountUsage>,
) -> UsageSummary {
    let today = now.date_naive();
    let first = today - Duration::days(window.days() - 1);
    let tallies = ledger.tally_between(&iso(first), &iso(today));

    let mut ids: Vec<String> = tallies.keys().cloned().collect();
    for row in providers
        .iter()
        .filter(|row| row.enabled || row.installed || row.offered)
    {
        if !ids.contains(&row.id) {
            ids.push(row.id.clone());
        }
    }
    if !ids.iter().any(|id| id == active_provider_id) {
        ids.push(active_provider_id.to_owned());
    }

    let mut rows: Vec<ProviderUsage> = ids
        .iter()
        .map(|id| {
            let tally = tallies.get(id).cloned().unwrap_or_default();
            let info = providers.iter().find(|row| row.id == *id);
            let price = bhippi_providers::pricing(id);
            let limit = budget
                .cap_for(id)
                .map(|cap| cap.saturating_mul(window.days().max(1) as u64));
            let total = tally.total_tokens();

            // Build per-model breakdown
            let models: Vec<ModelUsage> = tally
                .models
                .into_iter()
                .map(|(model_id, model_tally)| ModelUsage {
                    id: model_id.clone(),
                    label: model_id,
                    input_tokens: model_tally.input_tokens,
                    output_tokens: model_tally.output_tokens,
                    total_tokens: model_tally.total_tokens(),
                    turns: model_tally.turns,
                    cost_usd: micros_to_usd(model_tally.cost_micros),
                })
                .collect();

            ProviderUsage {
                id: id.clone(),
                label: info.map_or_else(|| fallback_label(id), |row| row.label.clone()),
                kind: info.map_or(ProviderKind::Cli, |row| row.kind),
                input_tokens: tally.input_tokens,
                output_tokens: tally.output_tokens,
                total_tokens: total,
                turns: tally.turns,
                cost_usd: micros_to_usd(tally.cost_micros),
                metered: price.is_some(),
                limit_tokens: limit,
                fraction: limit.map_or(0.0, |cap| fraction(total, cap)),
                available: info.is_some_and(|row| row.enabled && row.installed),
                // Shares are filled in below, once the window totals are known.
                share_of_tokens: 0.0,
                share_of_cost: 0.0,
                color_slot: color_slot(id),
                balance_usd: tally.balance_micros.map(micros_to_usd),
                account: accounts.get(id).cloned(),
                models,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.total_tokens
            .cmp(&a.total_tokens)
            .then_with(|| a.label.cmp(&b.label))
    });

    let window_tokens = rows
        .iter()
        .fold(0u64, |sum, row| sum.saturating_add(row.total_tokens));
    let window_cost: f64 = rows.iter().map(|row| row.cost_usd).sum();
    for row in &mut rows {
        row.share_of_tokens = share(row.total_tokens as f64, window_tokens as f64);
        row.share_of_cost = share(row.cost_usd, window_cost);
    }

    let active = rows
        .iter()
        .find(|row| row.id == active_provider_id)
        .cloned()
        .unwrap_or_else(|| empty_row(active_provider_id, budget, window));

    let chart_days = window.chart_days();
    let chart_start = today - Duration::days(chart_days - 1);
    let days: Vec<UsageDayPoint> = (0..chart_days)
        .map(|offset| {
            let date = chart_start + Duration::days(offset);
            let key = iso(date);
            let row = ledger.day(&key);
            UsageDayPoint {
                total_tokens: row.map_or(0, bhippi_core::UsageDay::total_tokens),
                cost_usd: micros_to_usd(row.map_or(0, bhippi_core::UsageDay::cost_micros)),
                providers: row.map_or_else(Vec::new, |day| {
                    day.providers
                        .iter()
                        .filter(|(_, tally)| tally.total_tokens() > 0)
                        .map(|(id, tally)| DayProviderPoint {
                            id: id.clone(),
                            total_tokens: tally.total_tokens(),
                            cost_usd: micros_to_usd(tally.cost_micros),
                        })
                        .collect()
                }),
                date: key,
            }
        })
        .collect();

    let total_turns = rows
        .iter()
        .fold(0u32, |sum, row| sum.saturating_add(row.turns));
    UsageSummary {
        window,
        window_label: window.label().to_owned(),
        active_provider_id: active_provider_id.to_owned(),
        active,
        total_tokens: window_tokens,
        total_input_tokens: rows
            .iter()
            .fold(0, |sum, row| sum.saturating_add(row.input_tokens)),
        total_output_tokens: rows
            .iter()
            .fold(0, |sum, row| sum.saturating_add(row.output_tokens)),
        total_cost_usd: window_cost,
        total_turns,
        tokens_per_turn: if total_turns == 0 {
            0
        } else {
            window_tokens / u64::from(total_turns)
        },
        range_label: range_label(first, today),
        resets_in_seconds: seconds_to_midnight(now),
        providers: rows,
        days,
    }
}

/// How many hues the chart palette has (ADR-0011).
const SERIES_SLOTS: usize = 8;

/// The chart's categorical slot for one provider, `0..SERIES_SLOTS`.
///
/// Keyed on catalogue position, never on how much the provider spent: a colour that
/// moved when a quieter backend dropped out of the window would make two different
/// providers look like the same one across two screenshots (ADR-0011).
///
/// The catalogue is longer than the palette, so slots wrap. Two backends that wrap onto
/// the same hue stay distinguishable because every series is also direct-labelled with
/// its own logo and name — colour is never the only signal. The demo row takes the last
/// slot outright rather than wrapping onto a real vendor it commonly appears beside.
///
/// The demo takes violet, not the last slot: the last slot is red, and a red series
/// sitting a few pixels from the error colour reads as a warning about a backend whose
/// whole job is to be uneventful.
const DEMO_SLOT: u8 = 6;

fn color_slot(id: &str) -> u8 {
    if id == "demo" {
        return DEMO_SLOT;
    }
    bhippi_providers::CATALOG
        .iter()
        .position(|entry| entry.id == id)
        .and_then(|index| u8::try_from(index % SERIES_SLOTS).ok())
        .unwrap_or(0)
}

fn share(part: f64, whole: f64) -> f64 {
    if whole <= 0.0 {
        return 0.0;
    }
    (part / whole).clamp(0.0, 1.0)
}

/// `28 Jul – 26 Aug`, or just the one date when the window is a single day.
fn range_label(first: NaiveDate, last: NaiveDate) -> String {
    let pretty = |date: NaiveDate| date.format("%-d %b").to_string();
    if first == last {
        pretty(last)
    } else {
        format!("{} – {}", pretty(first), pretty(last))
    }
}

fn empty_row(id: &str, budget: &BudgetConfig, window: UsageWindow) -> ProviderUsage {
    ProviderUsage {
        id: id.to_owned(),
        label: fallback_label(id),
        kind: ProviderKind::Demo,
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        turns: 0,
        cost_usd: 0.0,
        metered: bhippi_providers::pricing(id).is_some(),
        limit_tokens: budget
            .cap_for(id)
            .map(|cap| cap.saturating_mul(window.days().max(1) as u64)),
        fraction: 0.0,
        available: false,
        share_of_tokens: 0.0,
        share_of_cost: 0.0,
        color_slot: color_slot(id),
        balance_usd: None,
        account: None,
        models: Vec::new(),
    }
}

fn fallback_label(id: &str) -> String {
    bhippi_providers::spec(id).map_or_else(|| id.to_owned(), |spec| spec.label.to_owned())
}

fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn fraction(used: u64, cap: u64) -> f64 {
    if cap == 0 {
        return 0.0;
    }
    (used as f64 / cap as f64).clamp(0.0, 1.0)
}

fn micros_to_usd(micros: u64) -> f64 {
    micros as f64 / 1_000_000.0
}

/// Seconds until the next local midnight; falls back to a whole day if the local
/// calendar has no such instant (a DST gap), which is the safe over-estimate.
fn seconds_to_midnight(now: DateTime<Local>) -> u64 {
    let tomorrow = now.date_naive() + Duration::days(1);
    let Some(midnight) = tomorrow.and_hms_opt(0, 0, 0) else {
        return 86_400;
    };
    match Local.from_local_datetime(&midnight).single() {
        Some(next) => u64::try_from((next - now).num_seconds()).unwrap_or(86_400),
        None => 86_400,
    }
}

/// The local calendar date the ledger should record a turn under.
#[must_use]
pub fn today_key(now: DateTime<Local>) -> String {
    iso(now.date_naive())
}

/// Estimated cost of one call in micro-dollars, zero for unmetered backends.
#[must_use]
pub fn cost_micros(provider_id: &str, input_tokens: u64, output_tokens: u64) -> u64 {
    bhippi_providers::pricing(provider_id)
        .map_or(0, |price| price.cost_micros(input_tokens, output_tokens))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhippi_core::ProviderTally;

    pub(super) fn ledger_with(days: &[(&str, &str, u64, u64, u64)]) -> UsageLedger {
        let mut ledger = UsageLedger::default();
        for (date, id, input, output, micros) in days {
            ledger.record(
                date,
                id,
                ProviderTally {
                    input_tokens: *input,
                    output_tokens: *output,
                    cost_micros: *micros,
                    turns: 1,
                    balance_micros: None,
                    models: std::collections::BTreeMap::new(),
                },
            );
        }
        ledger
    }

    pub(super) fn at(date: &str) -> DateTime<Local> {
        let naive = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .unwrap_or_else(|error| panic!("the test date must parse: {error}"))
            .and_hms_opt(12, 0, 0)
            .unwrap_or_else(|| panic!("noon must exist on {date}"));
        Local
            .from_local_datetime(&naive)
            .single()
            .unwrap_or_else(|| panic!("noon on {date} must be unambiguous"))
    }

    #[test]
    fn the_ring_measures_the_active_provider_against_its_cap() {
        let ledger = ledger_with(&[("2026-08-26", "openai", 400_000, 100_000, 1_500_000)]);
        let budget = BudgetConfig {
            daily_token_cap: 1_000_000,
            ..BudgetConfig::default()
        };
        let summary = summarise(
            &ledger,
            &budget,
            &[],
            "openai",
            UsageWindow::Day,
            at("2026-08-26"),
        );

        assert_eq!(summary.active.total_tokens, 500_000);
        assert_eq!(summary.active.limit_tokens, Some(1_000_000));
        assert!((summary.active.fraction - 0.5).abs() < f64::EPSILON);
        assert!(summary.active.metered);
        assert!((summary.active.cost_usd - 1.5).abs() < 1e-9);
    }

    #[test]
    fn a_wider_window_scales_the_cap_with_it() {
        let ledger = ledger_with(&[
            ("2026-08-24", "openai", 500_000, 0, 0),
            ("2026-08-25", "openai", 500_000, 0, 0),
            ("2026-08-26", "openai", 500_000, 0, 0),
        ]);
        let budget = BudgetConfig {
            daily_token_cap: 1_000_000,
            ..BudgetConfig::default()
        };
        let summary = summarise(
            &ledger,
            &budget,
            &[],
            "openai",
            UsageWindow::Week,
            at("2026-08-26"),
        );

        assert_eq!(summary.active.total_tokens, 1_500_000);
        assert_eq!(summary.active.limit_tokens, Some(7_000_000));
        assert!(summary.active.fraction < 0.25);
    }

    #[test]
    fn an_uncapped_provider_reports_no_limit_instead_of_a_full_ring() {
        let ledger = ledger_with(&[("2026-08-26", "ollama", 10_000, 10_000, 0)]);
        let mut budget = BudgetConfig::default();
        budget.provider_token_caps.insert("ollama".to_owned(), 0);
        let summary = summarise(
            &ledger,
            &budget,
            &[],
            "ollama",
            UsageWindow::Day,
            at("2026-08-26"),
        );

        assert_eq!(summary.active.limit_tokens, None);
        assert!((summary.active.fraction - 0.0).abs() < f64::EPSILON);
        assert!(!summary.active.metered, "a local server bills nothing");
    }

    #[test]
    fn the_active_provider_is_present_even_with_no_history() {
        let summary = summarise(
            &UsageLedger::default(),
            &BudgetConfig::default(),
            &[],
            "claude",
            UsageWindow::Day,
            at("2026-08-26"),
        );
        assert_eq!(summary.active.id, "claude");
        assert_eq!(summary.active.total_tokens, 0);
        assert_eq!(summary.active.label, "Claude Code");
        assert_eq!(summary.days.len(), MIN_CHART_DAYS as usize);
        assert_eq!(summary.days.len(), 30);
    }

    #[test]
    fn the_chart_is_zero_filled_and_ends_today() {
        let ledger = ledger_with(&[("2026-08-26", "openai", 1, 1, 0)]);
        let summary = summarise(
            &ledger,
            &BudgetConfig::default(),
            &[],
            "openai",
            UsageWindow::Month,
            at("2026-08-26"),
        );
        let last = summary
            .days
            .last()
            .unwrap_or_else(|| panic!("the chart is never empty"));
        assert_eq!(last.date, "2026-08-26");
        assert_eq!(last.total_tokens, 2);
        assert!(summary
            .days
            .iter()
            .take(29)
            .all(|day| day.total_tokens == 0));
    }

    #[test]
    fn rows_are_ordered_heaviest_first() {
        let ledger = ledger_with(&[
            ("2026-08-26", "openai", 10, 10, 0),
            ("2026-08-26", "anthropic", 900, 100, 0),
        ]);
        let summary = summarise(
            &ledger,
            &BudgetConfig::default(),
            &[],
            "openai",
            UsageWindow::Day,
            at("2026-08-26"),
        );
        assert_eq!(summary.providers[0].id, "anthropic");
        assert_eq!(summary.total_tokens, 1_020);
    }

    #[test]
    fn a_single_day_window_prints_one_date_not_a_range() {
        let summary = summarise(
            &UsageLedger::default(),
            &BudgetConfig::default(),
            &[],
            "demo",
            UsageWindow::Day,
            super::tests::at("2026-08-26"),
        );
        assert_eq!(summary.range_label, "26 Aug");
    }

    /// Providers that have recorded per-model activity get those models broken out.
    #[test]
    fn model_level_breakdown_reflects_per_provider_models() {
        let mut ledger = UsageLedger::default();
        let date = "2026-08-26";

        // Two models on openai provider
        let openai_tally_with_models = ProviderTally {
            input_tokens: 500,
            output_tokens: 200,
            cost_micros: 1_500_000,
            turns: 3,
            balance_micros: None,
            models: [
                (
                    "claude-3-opus-20240229",
                    bhippi_core::ModelTally {
                        input_tokens: 300,
                        output_tokens: 100,
                        cost_micros: 900_000,
                        turns: 1,
                    },
                ),
                (
                    "claude-3-haiku-20240307",
                    bhippi_core::ModelTally {
                        input_tokens: 200,
                        output_tokens: 100,
                        cost_micros: 600_000,
                        turns: 2,
                    },
                ),
            ]
            .to_vec()
            .into_iter()
            .map(|(id, m)| (id.to_owned(), m))
            .collect(),
        };
        ledger.record(date, "openai", openai_tally_with_models);

        let budget = BudgetConfig::default();
        let summary = summarise(
            &ledger,
            &budget,
            &[],
            "openai",
            UsageWindow::Day,
            super::tests::at("2026-08-26"),
        );

        assert_eq!(summary.active.total_tokens, 700); // 300+100 + 200+100 from models
        assert_eq!(summary.active.turns, 3);

        let openai_row = summary
            .providers
            .iter()
            .find(|row| row.id == "openai")
            .expect("openai should have a row");
        assert_eq!(openai_row.models.len(), 2, "openai should have 2 models");
        assert_eq!(openai_row.models[0].label, "claude-3-haiku-20240307");
        assert_eq!(openai_row.models[0].input_tokens, 200);
        assert_eq!(openai_row.models[0].output_tokens, 100);
        assert_eq!(openai_row.models[0].total_tokens, 300);
        assert_eq!(openai_row.models[0].turns, 2);
        assert!((openai_row.models[0].cost_usd - 0.6).abs() < 1e-9);

        assert_eq!(openai_row.models[1].label, "claude-3-opus-20240229");
        assert_eq!(openai_row.models[1].input_tokens, 300);
        assert_eq!(openai_row.models[1].output_tokens, 100);
        assert_eq!(openai_row.models[1].total_tokens, 400);
        assert_eq!(openai_row.models[1].turns, 1);
        assert!((openai_row.models[1].cost_usd - 0.9).abs() < 1e-9);
    }

    #[test]
    fn vendor_weekly_usage_is_attached_without_reusing_the_local_cap() {
        let account = AccountUsage {
            account_name: Some("owner@example.com".to_owned()),
            plan: Some("plus".to_owned()),
            status: AccountUsageStatus::Live,
            session: None,
            weekly: Some(PlanWindow {
                used_fraction: 0.42,
                resets_at: Some(1_788_650_119),
                duration_minutes: Some(10_080),
            }),
            note: "live".to_owned(),
            refreshed_at: Utc::now(),
        };
        let accounts = BTreeMap::from([("codex".to_owned(), account)]);
        let budget = BudgetConfig {
            daily_token_cap: 1_000_000,
            ..BudgetConfig::default()
        };
        let summary = summarise_with_accounts(
            &UsageLedger::default(),
            &budget,
            &[],
            "codex",
            UsageWindow::Week,
            at("2026-08-26"),
            &accounts,
        );

        assert_eq!(summary.active.limit_tokens, Some(7_000_000));
        assert_eq!(
            summary
                .active
                .account
                .as_ref()
                .and_then(|row| row.weekly.as_ref())
                .map(|window| window.used_fraction),
            Some(0.42)
        );
    }

    #[test]
    fn an_account_switch_never_inherits_the_previous_accounts_windows() {
        let old = AccountUsage {
            account_name: Some("old@example.com".to_owned()),
            plan: Some("pro".to_owned()),
            status: AccountUsageStatus::Live,
            session: None,
            weekly: Some(PlanWindow {
                used_fraction: 0.9,
                resets_at: Some(10),
                duration_minutes: Some(10_080),
            }),
            note: "old".to_owned(),
            refreshed_at: Utc::now(),
        };
        let fresh = AccountUsage {
            account_name: Some("new@example.com".to_owned()),
            plan: Some("plus".to_owned()),
            status: AccountUsageStatus::Authenticated,
            session: None,
            weekly: None,
            note: "new".to_owned(),
            refreshed_at: Utc::now(),
        };

        let merged = merge_account_snapshot(&old, fresh);
        assert_eq!(merged.account_name.as_deref(), Some("new@example.com"));
        assert_eq!(merged.weekly, None);
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;

    /// The graph must never be shorter than the totals printed above it: a 90-day
    /// window over a 30-day chart shows two thirds of the spend as if it never happened.
    #[test]
    fn a_wider_window_widens_the_chart_with_it() {
        for (window, expected) in [
            (UsageWindow::Day, 30),
            (UsageWindow::Week, 30),
            (UsageWindow::Month, 30),
            (UsageWindow::Quarter, 90),
        ] {
            let summary = summarise(
                &UsageLedger::default(),
                &BudgetConfig::default(),
                &[],
                "demo",
                window,
                super::tests::at("2026-08-26"),
            );
            assert_eq!(summary.days.len(), expected, "{window:?} chart span");
        }
    }

    /// Shares are what make the breakdown readable, and they must add up.
    #[test]
    fn shares_are_measured_against_the_window_and_sum_to_one() {
        let ledger = super::tests::ledger_with(&[
            ("2026-08-26", "openai", 750, 0, 3_000_000),
            ("2026-08-26", "anthropic", 250, 0, 1_000_000),
        ]);
        let summary = summarise(
            &ledger,
            &BudgetConfig::default(),
            &[],
            "openai",
            UsageWindow::Day,
            super::tests::at("2026-08-26"),
        );

        let openai = summary
            .providers
            .iter()
            .find(|row| row.id == "openai")
            .unwrap_or_else(|| panic!("openai must be present"));
        assert!((openai.share_of_tokens - 0.75).abs() < 1e-9);
        assert!((openai.share_of_cost - 0.75).abs() < 1e-9);

        let summed: f64 = summary
            .providers
            .iter()
            .map(|row| row.share_of_tokens)
            .sum();
        assert!((summed - 1.0).abs() < 1e-9, "shares summed to {summed}");
        assert_eq!(summary.total_input_tokens, 1_000);
        assert_eq!(summary.total_output_tokens, 0);
        assert_eq!(summary.tokens_per_turn, 500, "1000 tokens over two turns");
    }

    /// A colour belongs to the provider, not to its position in the table — otherwise
    /// the same series changes colour whenever a quieter backend drops out.
    #[test]
    fn colour_slots_follow_the_provider_not_its_rank() {
        let quiet = super::tests::ledger_with(&[("2026-08-26", "opencode", 10, 0, 0)]);
        let busy = super::tests::ledger_with(&[
            ("2026-08-26", "opencode", 10, 0, 0),
            ("2026-08-26", "claude", 9_000, 0, 0),
        ]);
        let slot = |ledger: &UsageLedger| {
            summarise(
                ledger,
                &BudgetConfig::default(),
                &[],
                "opencode",
                UsageWindow::Day,
                super::tests::at("2026-08-26"),
            )
            .providers
            .iter()
            .find(|row| row.id == "opencode")
            .map(|row| row.color_slot)
        };
        assert_eq!(slot(&quiet), slot(&busy));
        assert_ne!(super::color_slot("claude"), super::color_slot("opencode"));

        // Every slot must index the palette that actually exists.
        for entry in bhippi_providers::CATALOG {
            assert!(
                (super::color_slot(entry.id) as usize) < super::SERIES_SLOTS,
                "{} has no colour",
                entry.id
            );
        }
        // The demo answers beside a real vendor constantly; it must not borrow its hue.
        for id in ["claude", "codex", "opencode", "grok", "kimi"] {
            assert_ne!(
                super::color_slot("demo"),
                super::color_slot(id),
                "demo collides with {id}"
            );
        }
    }

    #[test]
    fn a_single_day_window_prints_one_date_not_a_range() {
        let summary = summarise(
            &UsageLedger::default(),
            &BudgetConfig::default(),
            &[],
            "demo",
            UsageWindow::Day,
            super::tests::at("2026-08-26"),
        );
        assert_eq!(summary.range_label, "26 Aug");

        let month = summarise(
            &UsageLedger::default(),
            &BudgetConfig::default(),
            &[],
            "demo",
            UsageWindow::Month,
            super::tests::at("2026-08-26"),
        );
        assert!(month.range_label.contains('–'), "{}", month.range_label);
    }
}
