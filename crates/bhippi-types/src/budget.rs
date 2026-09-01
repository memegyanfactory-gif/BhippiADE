use serde::{Deserialize, Serialize};
use specta::Type;
use std::ops::RangeInclusive;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "UPPERCASE")]
pub enum Tier {
    X2,
    X6,
    X12,
    X24,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierBudget {
    pub max_hop: u8,
    pub expansions: u8,
    pub branch: u8,
    pub sources: RangeInclusive<u16>,
    pub min_tier2: u16,
    pub min_primary: u16,
    pub target_dots: u16,
    pub counter_passes: u8,
    pub timeline: bool,
    pub entity_deep_dives: u8,
    pub wall: Duration,
    pub tokens: u64,
    pub words: RangeInclusive<u16>,
}

impl Tier {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X2 => "X2",
            Self::X6 => "X6",
            Self::X12 => "X12",
            Self::X24 => "X24",
        }
    }

    /// Returns the hard ceiling and quality-floor contract from specification section 10.1.
    #[must_use]
    pub const fn budget(self) -> TierBudget {
        match self {
            Self::X2 => TierBudget {
                max_hop: 2,
                expansions: 2,
                branch: 3,
                sources: 8..=14,
                min_tier2: 3,
                min_primary: 1,
                target_dots: 30,
                counter_passes: 0,
                timeline: false,
                entity_deep_dives: 0,
                wall: Duration::from_secs(3 * 60),
                tokens: 60_000,
                words: 700..=1_000,
            },
            Self::X6 => TierBudget {
                max_hop: 3,
                expansions: 6,
                branch: 4,
                sources: 25..=40,
                min_tier2: 8,
                min_primary: 3,
                target_dots: 100,
                counter_passes: 1,
                timeline: false,
                entity_deep_dives: 2,
                wall: Duration::from_secs(10 * 60),
                tokens: 250_000,
                words: 1_200..=1_800,
            },
            Self::X12 => TierBudget {
                max_hop: 4,
                expansions: 12,
                branch: 5,
                sources: 60..=90,
                min_tier2: 20,
                min_primary: 8,
                target_dots: 250,
                counter_passes: 2,
                timeline: true,
                entity_deep_dives: 5,
                wall: Duration::from_secs(30 * 60),
                tokens: 700_000,
                words: 2_000..=3_000,
            },
            Self::X24 => TierBudget {
                max_hop: 5,
                expansions: 24,
                branch: 6,
                sources: 120..=200,
                min_tier2: 40,
                min_primary: 16,
                target_dots: 500,
                counter_passes: 3,
                timeline: true,
                entity_deep_dives: 10,
                wall: Duration::from_secs(90 * 60),
                tokens: 1_600_000,
                words: 3_000..=5_000,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Tier, TierBudget};

    fn row(tier: Tier) -> String {
        let TierBudget {
            max_hop,
            expansions,
            branch,
            sources,
            min_tier2,
            min_primary,
            target_dots,
            counter_passes,
            timeline,
            entity_deep_dives,
            wall,
            tokens,
            words,
        } = tier.budget();

        format!(
            "{max_hop}|{expansions}|{branch}|{}-{}|{min_tier2}|{min_primary}|{target_dots}|{counter_passes}|{timeline}|{entity_deep_dives}|{}|{tokens}|{}-{}",
            sources.start(),
            sources.end(),
            wall.as_secs(),
            words.start(),
            words.end()
        )
    }

    #[test]
    fn budget_snapshot_matches_specification_section_10_1() {
        assert_eq!(
            row(Tier::X2),
            "2|2|3|8-14|3|1|30|0|false|0|180|60000|700-1000"
        );
        assert_eq!(
            row(Tier::X6),
            "3|6|4|25-40|8|3|100|1|false|2|600|250000|1200-1800"
        );
        assert_eq!(
            row(Tier::X12),
            "4|12|5|60-90|20|8|250|2|true|5|1800|700000|2000-3000"
        );
        assert_eq!(
            row(Tier::X24),
            "5|24|6|120-200|40|16|500|3|true|10|5400|1600000|3000-5000"
        );
    }
}
