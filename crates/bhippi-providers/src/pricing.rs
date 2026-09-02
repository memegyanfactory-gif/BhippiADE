//! List prices in USD per million tokens.
//!
//! # Why this is per-model and not per-provider
//!
//! It used to be one price per vendor, applied to every model that vendor served. That
//! is wrong by a factor of five in normal use: Claude Haiku 4.5 is $1/$5 per MTok and
//! Claude Opus 5 is $5/$25, and the old table charged both at Sonnet 4.6's $3/$15. A
//! user who ran a day of Haiku saw a figure three times the real one, and a day of Opus
//! saw a figure well under it. Model ids are matched first now, and the provider default
//! is only the fallback for a model we have no published rate for.
//!
//! # What the number means
//!
//! These are **list prices for a plain input/output turn**. Cached reads and cache writes
//! are billed at different rates, and this crate is not told how many of a turn's tokens
//! were cache hits — the `Usage` we record carries only `input_tokens` and
//! `output_tokens` — so a cache-heavy turn is over-estimated rather than silently priced
//! at a rate nobody can verify. That is a deliberate bias: over-reporting spend is the
//! safe direction for a budget gauge.
//!
//! Every surface that renders a figure from this module must say whether it came from an
//! exact model match or a provider default — see [`Basis`]. A number the user cannot tell
//! apart from a bill is worse than no number.
//!
//! A provider with no entry at all is **not free** — it is *not metered per token*:
//! subscription CLIs (Claude Code, Codex, opencode, Grok, Kimi), local servers, and the
//! offline demo bill nothing per call, so their cost is genuinely zero.

/// Where a price came from, so the UI can label the figure honestly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    /// The exact model id carried a published rate.
    Model,
    /// No rate for this model; the vendor's default-model rate was used instead.
    ProviderDefault,
}

/// Per-million-token list price.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pricing {
    pub input_per_mtok_usd: f64,
    pub output_per_mtok_usd: f64,
    pub basis: Basis,
}

impl Pricing {
    /// Cost of one call in whole micro-dollars (USD x 1e6), so ledgers stay integral.
    ///
    /// Rounds half-away-from-zero at the micro-dollar, which is 1e-6 of a dollar: far
    /// below the smallest figure any surface renders, so rounding here can never move a
    /// displayed cent.
    #[must_use]
    pub fn cost_micros(&self, input_tokens: u64, output_tokens: u64) -> u64 {
        let input = (input_tokens as f64 / 1e6) * self.input_per_mtok_usd;
        let output = (output_tokens as f64 / 1e6) * self.output_per_mtok_usd;
        let micros = (input + output) * 1e6;
        if micros.is_finite() && micros > 0.0 {
            // `as u64` saturates at u64::MAX on overflow and truncates toward zero, so
            // rounding first keeps a 0.6-micro call from being recorded as free.
            micros.round() as u64
        } else {
            0
        }
    }

    const fn model(input: f64, output: f64) -> Self {
        Self {
            input_per_mtok_usd: input,
            output_per_mtok_usd: output,
            basis: Basis::Model,
        }
    }

    const fn provider_default(input: f64, output: f64) -> Self {
        Self {
            input_per_mtok_usd: input,
            output_per_mtok_usd: output,
            basis: Basis::ProviderDefault,
        }
    }
}

/// Published per-model rates.
///
/// Matched against the model id the turn actually ran on. Anthropic's published list
/// prices as of 2026-06; the other vendors are covered by [`PROVIDER_DEFAULTS`] because
/// we do not carry a verified per-model table for them, and a guessed rate would be worse
/// than an honestly-labelled default.
const MODEL_PRICES: &[(&str, Pricing)] = &[
    // ── Anthropic ────────────────────────────────────────────────────────────
    ("claude-fable-5-1", Pricing::model(10.00, 50.00)),
    ("claude-fable-5", Pricing::model(10.00, 50.00)),
    ("claude-opus-5", Pricing::model(5.00, 25.00)),
    ("claude-opus-4-8", Pricing::model(5.00, 25.00)),
    ("claude-opus-4-7", Pricing::model(5.00, 25.00)),
    ("claude-opus-4-6", Pricing::model(5.00, 25.00)),
    ("claude-sonnet-5", Pricing::model(2.00, 10.00)),
    ("claude-sonnet-4-6", Pricing::model(3.00, 15.00)),
    ("claude-haiku-4-5", Pricing::model(1.00, 5.00)),
];

/// The vendor's default-model rate, used when a model id matches no row above.
const PROVIDER_DEFAULTS: &[(&str, Pricing)] = &[
    // Claude Sonnet 4.6 — the default model for a bare `anthropic` provider.
    ("anthropic", Pricing::provider_default(3.00, 15.00)),
    // GPT-4o.
    ("openai", Pricing::provider_default(2.50, 10.00)),
    // Grok 4.6 flagship.
    ("xai", Pricing::provider_default(2.00, 6.00)),
    // moonshot-v1-128k.
    ("moonshot", Pricing::provider_default(2.00, 5.00)),
    // LLaMA 3.1 70B.
    ("groq", Pricing::provider_default(0.59, 0.79)),
    // OpenRouter, GPT-4o passthrough.
    ("openrouter", Pricing::provider_default(2.50, 10.00)),
];

/// Strips a dated snapshot suffix so `claude-opus-5-20260101` matches `claude-opus-5`.
///
/// Vendors ship the same model under a bare id and a dated one at the same price. Without
/// this, every dated id silently fell through to the provider default.
fn canonical_model(model_id: &str) -> &str {
    let trimmed = model_id.trim();
    // A trailing `-YYYYMMDD` (or `@YYYYMMDD`, which Vertex uses) is a snapshot marker.
    for separator in ['-', '@'] {
        if let Some((head, tail)) = trimmed.rsplit_once(separator) {
            if tail.len() == 8 && tail.bytes().all(|byte| byte.is_ascii_digit()) {
                return head;
            }
        }
    }
    trimmed
}

/// The list price for one provider, preferring an exact model match.
///
/// `model_id` is whatever the turn reported running on; `None` (or an id we do not
/// publish a rate for) falls back to the provider's default-model rate.
#[must_use]
pub fn pricing_for(provider_id: &str, model_id: Option<&str>) -> Option<Pricing> {
    if let Some(model) = model_id {
        let canonical = canonical_model(model);
        if !canonical.is_empty() {
            if let Some(price) = MODEL_PRICES
                .iter()
                .find(|(id, _)| id.eq_ignore_ascii_case(canonical))
                .map(|(_, price)| *price)
            {
                // A model rate only applies when the provider is actually metered.
                // `claude` (the subscription CLI) also reports Claude model ids, and
                // billing a subscription turn per token would invent spend that the
                // user is never charged.
                if is_metered(provider_id) {
                    return Some(price);
                }
                return None;
            }
        }
    }
    provider_default(provider_id)
}

/// The vendor's default-model rate, ignoring any model id.
#[must_use]
pub fn provider_default(provider_id: &str) -> Option<Pricing> {
    PROVIDER_DEFAULTS
        .iter()
        .find(|(id, _)| *id == provider_id)
        .map(|(_, price)| *price)
}

/// Whether this backend bills per token at all.
#[must_use]
pub fn is_metered(provider_id: &str) -> bool {
    provider_default(provider_id).is_some()
}

/// Back-compatible provider-only lookup.
#[must_use]
pub fn pricing(provider_id: &str) -> Option<Pricing> {
    provider_default(provider_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_and_local_backends_are_unmetered() {
        for id in [
            "claude", "codex", "opencode", "grok", "kimi", "ollama", "demo",
        ] {
            assert!(pricing(id).is_none(), "{id} must not be priced per token");
            assert!(!is_metered(id), "{id} must not be metered");
        }
    }

    #[test]
    fn a_subscription_cli_reporting_a_model_id_is_still_free() {
        // Claude Code streams real Anthropic model ids. Matching one must not start
        // charging a flat-rate subscription per token.
        assert!(pricing_for("claude", Some("claude-opus-5")).is_none());
    }

    #[test]
    fn metered_backends_cost_what_the_table_says() {
        let price = provider_default("anthropic").expect("anthropic must carry a list price");
        // 1M in + 1M out at 3.00/15.00 = $18.00 = 18_000_000 micro-dollars.
        assert_eq!(price.cost_micros(1_000_000, 1_000_000), 18_000_000);
        let openai = provider_default("openai").expect("openai must carry a list price");
        assert_eq!(openai.cost_micros(1_000_000, 1_000_000), 12_500_000);
        assert_eq!(openai.cost_micros(0, 0), 0);
    }

    #[test]
    fn model_rates_beat_the_provider_default() {
        // The bug this table exists to fix: Haiku billed at Sonnet's rate.
        let haiku =
            pricing_for("anthropic", Some("claude-haiku-4-5")).expect("haiku must be priced");
        assert_eq!(haiku.basis, Basis::Model);
        assert_eq!(haiku.cost_micros(1_000_000, 1_000_000), 6_000_000);

        let opus = pricing_for("anthropic", Some("claude-opus-5")).expect("opus must be priced");
        assert_eq!(opus.cost_micros(1_000_000, 1_000_000), 30_000_000);

        // Same tokens, 5x the cost. The old table returned 18_000_000 for both.
        assert!(opus.cost_micros(1_000_000, 1_000_000) > haiku.cost_micros(1_000_000, 1_000_000));
    }

    #[test]
    fn an_unknown_model_falls_back_and_says_so() {
        let fallback = pricing_for("anthropic", Some("claude-something-unreleased"))
            .expect("must fall back to the provider default");
        assert_eq!(fallback.basis, Basis::ProviderDefault);
        assert_eq!(fallback, provider_default("anthropic").expect("default"));

        let absent = pricing_for("anthropic", None).expect("no model id still prices");
        assert_eq!(absent.basis, Basis::ProviderDefault);
    }

    #[test]
    fn dated_snapshots_match_their_base_model() {
        let dated = pricing_for("anthropic", Some("claude-opus-5-20260101")).expect("priced");
        let bare = pricing_for("anthropic", Some("claude-opus-5")).expect("priced");
        assert_eq!(dated, bare);
        assert_eq!(dated.basis, Basis::Model);

        // Vertex writes the separator as `@`.
        let vertex = pricing_for("anthropic", Some("claude-opus-4-6@20251101")).expect("priced");
        assert_eq!(vertex.basis, Basis::Model);
    }

    #[test]
    fn a_model_id_that_is_not_a_snapshot_is_not_truncated() {
        // `claude-haiku-4-5` ends in `-5`, not an 8-digit date: it must survive intact.
        assert_eq!(canonical_model("claude-haiku-4-5"), "claude-haiku-4-5");
        assert_eq!(canonical_model("gpt-4o-2024-08-06"), "gpt-4o-2024-08-06");
    }

    #[test]
    fn sub_cent_calls_are_never_rounded_away_to_free() {
        let haiku = pricing_for("anthropic", Some("claude-haiku-4-5")).expect("priced");
        // 900 in + 300 out at 1.00/5.00 = $0.0024 = 2400 micro-dollars.
        assert_eq!(haiku.cost_micros(900, 300), 2_400);
        // One single input token still records a micro-dollar rather than zero.
        assert_eq!(haiku.cost_micros(1, 0), 1);
    }

    #[test]
    fn every_priced_provider_is_a_real_catalogue_id() {
        for (id, _) in PROVIDER_DEFAULTS {
            assert!(crate::spec(id).is_some(), "{id} is not in the catalogue");
        }
    }

    #[test]
    fn no_model_row_is_listed_twice() {
        for (index, (id, _)) in MODEL_PRICES.iter().enumerate() {
            assert!(
                !MODEL_PRICES[index + 1..]
                    .iter()
                    .any(|(other, _)| other.eq_ignore_ascii_case(id)),
                "{id} appears twice in MODEL_PRICES"
            );
        }
    }
}
