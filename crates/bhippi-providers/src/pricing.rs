//! List prices per provider, in USD per million tokens.
//!
//! These are *estimates* for each vendor's default mid-tier model, not a bill. They
//! exist so the usage panel can put an order-of-magnitude number next to a token count;
//! every surface that renders them must label the figure as estimated.
//!
//! A provider with no entry is **not free** — it is *not metered per token*: subscription
//! CLIs (Claude Code, Codex, opencode, Grok, Kimi), local servers, and the offline demo
//! all bill nothing per call, so their cost is genuinely zero and is shown as `—`.

/// Per-million-token list price for one provider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pricing {
    pub input_per_mtok_usd: f64,
    pub output_per_mtok_usd: f64,
}

impl Pricing {
    /// Cost of one call in whole micro-dollars (USD × 1e6), so ledgers stay integral.
    #[must_use]
    pub fn cost_micros(&self, input_tokens: u64, output_tokens: u64) -> u64 {
        let input = (input_tokens as f64 / 1e6) * self.input_per_mtok_usd;
        let output = (output_tokens as f64 / 1e6) * self.output_per_mtok_usd;
        let micros = (input + output) * 1e6;
        if micros.is_finite() && micros > 0.0 {
            micros.round() as u64
        } else {
            0
        }
    }
}

const PRICES: &[(&str, Pricing)] = &[
    // Claude Sonnet 4.6 — $3 / $15 per MTok (Aug 2026).
    (
        "anthropic",
        Pricing {
            input_per_mtok_usd: 3.00,
            output_per_mtok_usd: 15.00,
        },
    ),
    // GPT-4o — $2.50 / $10 per MTok (Aug 2026).
    (
        "openai",
        Pricing {
            input_per_mtok_usd: 2.50,
            output_per_mtok_usd: 10.00,
        },
    ),
    // Grok 4.6 flagship — $2 / $6 per MTok (Aug 2026).
    (
        "xai",
        Pricing {
            input_per_mtok_usd: 2.00,
            output_per_mtok_usd: 6.00,
        },
    ),
    // moonshot-v1-128k — $2 / $5 per MTok (Aug 2026).
    (
        "moonshot",
        Pricing {
            input_per_mtok_usd: 2.00,
            output_per_mtok_usd: 5.00,
        },
    ),
    // LLaMA 3.1 70B — $0.59 / $0.79 per MTok (Aug 2026).
    (
        "groq",
        Pricing {
            input_per_mtok_usd: 0.59,
            output_per_mtok_usd: 0.79,
        },
    ),
    // OpenRouter (GPT-4o passthrough) — $2.50 / $10 per MTok (Aug 2026).
    (
        "openrouter",
        Pricing {
            input_per_mtok_usd: 2.50,
            output_per_mtok_usd: 10.00,
        },
    ),
];

/// The list price for one provider id, or `None` when nothing is billed per token.
#[must_use]
pub fn pricing(provider_id: &str) -> Option<Pricing> {
    PRICES
        .iter()
        .find(|(id, _)| *id == provider_id)
        .map(|(_, price)| *price)
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
        }
    }

    #[test]
    fn metered_backends_cost_what_the_table_says() {
        let price =
            pricing("anthropic").unwrap_or_else(|| panic!("anthropic must carry a list price"));
        // 1M in + 1M out at 3.00/15.00 = $18.00 = 18_000_000 micro-dollars.
        assert_eq!(price.cost_micros(1_000_000, 1_000_000), 18_000_000);
        let openai = pricing("openai").unwrap_or_else(|| panic!("openai must carry a list price"));
        // 1M in + 1M out at 2.50/10.00 = $12.50 = 12_500_000 micro-dollars.
        assert_eq!(openai.cost_micros(1_000_000, 1_000_000), 12_500_000);
        assert_eq!(openai.cost_micros(0, 0), 0);
    }

    #[test]
    fn every_priced_row_is_a_real_catalogue_id() {
        for (id, _) in PRICES {
            assert!(crate::spec(id).is_some(), "{id} is not in the catalogue");
        }
    }
}
