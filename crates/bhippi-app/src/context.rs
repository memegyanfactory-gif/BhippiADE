//! Context telemetry as the UI renders it.
//!
//! Reads the per-turn sample log written by `ChatEngine` and turns it into the
//! numbers a Token Engine panel shows: how many estimated input tokens a prompt
//! carried and where the weight sat, provider by provider and category by category.
//! The samples themselves carry counts and metadata only — never content — so the
//! aggregation here never has anything secret to reprint (INV-039).

use bhippi_core::{ContextCategory, ContextLog, ContextSample};
use chrono::{DateTime, Duration, Local, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

/// How much history one summary covers. `Day` is the default the chart opens on.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ContextWindow {
    #[default]
    Day,
    Week,
    /// The whole retained history (`bhippi_core::RETAINED_SAMPLES`).
    All,
}

impl ContextWindow {
    /// Days the window covers, including today; `All` means no cutoff.
    const fn days(self) -> i64 {
        match self {
            Self::Day => 1,
            Self::Week => 7,
            Self::All => i64::MAX - 1,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Day => "Today",
            Self::Week => "Last 7 days",
            Self::All => "All history",
        }
    }
}

/// One category's weight inside the window.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ContextCategoryView {
    /// `ContextCategory::as_str()`.
    pub category: String,
    pub samples: u32,
    /// Mean estimated tokens per sample for this category.
    pub avg_tokens: u64,
    /// `0.0..=1.0` of the window's total attributed to this category.
    pub share: f64,
}

/// One provider's weight inside the window.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ContextProviderView {
    pub provider_id: String,
    pub samples: u32,
    pub avg_input: u64,
    pub avg_output: u64,
}

/// One day's samples, for the trend chart.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ContextDayPoint {
    /// `YYYY-MM-DD` (UTC).
    pub date: String,
    pub samples: u32,
    pub avg_input: u64,
}

/// Everything the context panel needs in one read.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ContextSummary {
    pub window: ContextWindow,
    pub window_label: String,
    pub samples: u32,
    /// Turns whose assembled prompt already filled the provider's window.
    pub turns_with_over_window: u32,
    /// Turns that carried a multi-provider handoff note.
    pub handoff_turns: u32,
    pub mean_total_input: u64,
    pub median_total_input: u64,
    pub max_total_input: u64,
    /// Mean answer budget reserved per sample (`max_tokens`), not the actual answer.
    pub mean_output_estimate: u64,
    /// Heaviest category first.
    pub categories: Vec<ContextCategoryView>,
    /// Every provider with samples in the window, heaviest first.
    pub providers: Vec<ContextProviderView>,
    /// Oldest to newest, one row per day that has samples.
    pub by_day: Vec<ContextDayPoint>,
}

/// Builds a summary from the telemetry log. Pure apart from the `now` it is handed,
/// so the shape is unit-testable without a clock.
#[must_use]
pub fn summarise(log: &ContextLog, window: ContextWindow, now: DateTime<Local>) -> ContextSummary {
    let now_utc: DateTime<Utc> = now.with_timezone(&Utc);
    let cutoff = match window {
        // Everything before any sample could have been recorded.
        ContextWindow::All => chrono::NaiveDate::from_ymd_opt(1970, 1, 1)
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|date| date.and_utc())
            .unwrap_or_else(|| now_utc),
        ContextWindow::Day | ContextWindow::Week => {
            let today = now_utc.date_naive();
            match (today - Duration::days(window.days() - 1)).and_hms_opt(0, 0, 0) {
                Some(start) => start.and_utc(),
                None => now_utc - Duration::days(window.days() - 1),
            }
        }
    };
    let rows = log.since(cutoff);

    let samples = u32::try_from(rows.len()).unwrap_or(u32::MAX);
    let mut over_window = 0u32;
    let mut handoff_turns = 0u32;
    let mut input_sum = 0u64;
    let mut output_sum = 0u64;
    let mut inputs: Vec<u64> = Vec::with_capacity(rows.len());
    let mut by_category: BTreeMap<ContextCategory, (u32, u64)> = BTreeMap::new();
    let mut by_provider: BTreeMap<String, (u32, u64, u64)> = BTreeMap::new();
    let mut by_day: BTreeMap<String, (u32, u64)> = BTreeMap::new();

    for sample in rows {
        if sample.over_window {
            over_window = over_window.saturating_add(1);
        }
        if sample.handoff {
            handoff_turns = handoff_turns.saturating_add(1);
        }
        input_sum = input_sum.saturating_add(sample.estimated_total);
        output_sum = output_sum.saturating_add(sample.reserved_output);
        inputs.push(sample.estimated_total);
        for (category, tokens) in &sample.categories {
            let entry = by_category.entry(*category).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(1);
            entry.1 = entry.1.saturating_add(*tokens);
        }
        let provider = by_provider
            .entry(sample.provider_id.clone())
            .or_insert((0, 0, 0));
        provider.0 = provider.0.saturating_add(1);
        provider.1 = provider.1.saturating_add(sample.estimated_total);
        provider.2 = provider.2.saturating_add(sample.reserved_output);
        let day = by_day.entry(day_key(sample)).or_insert((0, 0));
        day.0 = day.0.saturating_add(1);
        day.1 = day.1.saturating_add(sample.estimated_total);
    }

    let mean = |sum: u64| {
        if samples == 0 {
            0
        } else {
            sum / u64::from(samples)
        }
    };

    let mut categories: Vec<ContextCategoryView> = by_category
        .into_iter()
        .map(|(category, (count, tokens))| ContextCategoryView {
            category: category.as_str().to_owned(),
            samples: count,
            avg_tokens: if count == 0 {
                0
            } else {
                tokens / u64::from(count)
            },
            share: if input_sum == 0 {
                0.0
            } else {
                (tokens as f64 / input_sum as f64).clamp(0.0, 1.0)
            },
        })
        .collect();
    categories.sort_by(|a, b| {
        (b.avg_tokens * u64::from(b.samples)).cmp(&(a.avg_tokens * u64::from(a.samples)))
    });

    let mut providers: Vec<ContextProviderView> = by_provider
        .into_iter()
        .map(
            |(provider_id, (count, input, output))| ContextProviderView {
                provider_id,
                samples: count,
                avg_input: if count == 0 {
                    0
                } else {
                    input / u64::from(count)
                },
                avg_output: if count == 0 {
                    0
                } else {
                    output / u64::from(count)
                },
            },
        )
        .collect();
    providers.sort_by(|a, b| {
        b.samples
            .cmp(&a.samples)
            .then_with(|| b.avg_input.cmp(&a.avg_input))
    });

    let mut by_day: Vec<ContextDayPoint> = by_day
        .into_iter()
        .map(|(date, (count, tokens))| ContextDayPoint {
            date,
            samples: count,
            avg_input: if count == 0 {
                0
            } else {
                tokens / u64::from(count)
            },
        })
        .collect();
    by_day.sort_by(|a, b| a.date.cmp(&b.date));

    inputs.sort_unstable();
    let median = median(&inputs);

    ContextSummary {
        window,
        window_label: window.label().to_owned(),
        samples,
        turns_with_over_window: over_window,
        handoff_turns,
        mean_total_input: mean(input_sum),
        median_total_input: median,
        max_total_input: inputs.last().copied().unwrap_or(0),
        mean_output_estimate: mean(output_sum),
        categories,
        providers,
        by_day,
    }
}

fn day_key(sample: &ContextSample) -> String {
    sample.at.format("%Y-%m-%d").to_string()
}

fn median(sorted_values: &[u64]) -> u64 {
    let len = sorted_values.len();
    if len == 0 {
        return 0;
    }
    if len % 2 == 1 {
        sorted_values[len / 2]
    } else {
        (sorted_values[len / 2 - 1] + sorted_values[len / 2]) / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhippi_core::ContextCategory as C;
    use chrono::TimeZone;

    fn sample(day: &str, total: u64, provider: &str, categories: &[(C, u64)]) -> ContextSample {
        let at = format!("{day}T10:00:00Z")
            .parse::<DateTime<Utc>>()
            .unwrap_or_else(|error| panic!("the test timestamp must parse: {error}"));
        let mut row = ContextSample {
            turn_id: day.to_owned(),
            conversation_id: "c".to_owned(),
            project: "p".to_owned(),
            at,
            provider_id: provider.to_owned(),
            estimated_total: total,
            ..ContextSample::default()
        };
        for (category, tokens) in categories {
            *row.categories.entry(*category).or_insert(0) += *tokens;
        }
        row
    }

    fn at(iso: &str) -> DateTime<Local> {
        let naive = chrono::NaiveDate::parse_from_str(iso, "%Y-%m-%d")
            .unwrap_or_else(|error| panic!("the test date must parse: {error}"))
            .and_hms_opt(12, 0, 0)
            .unwrap_or_else(|| panic!("noon must exist on {iso}"));
        chrono::Local
            .from_local_datetime(&naive)
            .single()
            .unwrap_or_else(|| panic!("noon on {iso} must be unambiguous"))
    }

    #[test]
    fn a_window_aggregates_counts_providers_and_categories() {
        let mut log = ContextLog::default();
        log.samples.push(sample(
            "2026-08-26",
            1_000,
            "demo",
            &[(C::System, 100), (C::Conversation, 900)],
        ));
        log.samples.push(sample(
            "2026-08-25",
            2_000,
            "demo",
            &[(C::System, 100), (C::Conversation, 1_900)],
        ));

        let summary = summarise(&log, ContextWindow::Week, at("2026-08-26"));
        assert_eq!(summary.samples, 2);
        assert_eq!(summary.mean_total_input, 1_500);
        assert_eq!(summary.median_total_input, 1_500);
        assert_eq!(summary.max_total_input, 2_000);
        assert_eq!(summary.providers.len(), 1);
        assert_eq!(summary.providers[0].provider_id, "demo");
        assert_eq!(summary.categories.len(), 2);
        assert_eq!(summary.categories[0].category, "conversation");
        assert!((summary.categories[0].share - 0.93).abs() < 0.01);
        assert_eq!(summary.by_day.len(), 2);
        assert_eq!(summary.by_day[0].date, "2026-08-25");
    }

    #[test]
    fn the_window_filters_older_samples() {
        let mut log = ContextLog::default();
        log.samples.push(sample("2026-08-01", 9_000, "demo", &[]));
        log.samples.push(sample("2026-08-26", 500, "demo", &[]));
        let summary = summarise(&log, ContextWindow::Day, at("2026-08-26"));
        assert_eq!(summary.samples, 1);
        assert_eq!(summary.mean_total_input, 500);
    }

    #[test]
    fn an_empty_log_is_a_zeroed_summary() {
        let summary = summarise(&ContextLog::default(), ContextWindow::All, at("2026-08-26"));
        assert_eq!(summary.samples, 0);
        assert_eq!(summary.mean_total_input, 0);
        assert!(summary.categories.is_empty());
        assert!(summary.providers.is_empty());
        assert!(summary.by_day.is_empty());
    }
}
