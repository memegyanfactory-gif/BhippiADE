//! Tier budget contracts rendered in Research and Settings › Research (spec §10.1).
//! Numbers live here, never in TypeScript (R3).

use bhippi_types::{Tier, TierBudget};
use serde::{Deserialize, Serialize};
use specta::Type;

const ALL_TIERS: [Tier; 4] = [Tier::X2, Tier::X6, Tier::X12, Tier::X24];

/// The depth-ladder contract for one tier, IPC-shaped.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct TierBudgetView {
    pub tier: String,
    pub expansions: u8,
    pub branch: u8,
    pub sources_min: u16,
    pub sources_max: u16,
    pub min_tier2: u16,
    pub min_primary: u16,
    pub target_dots: u16,
    pub counter_passes: u8,
    pub timeline: bool,
    pub entity_deep_dives: u8,
    pub wall_minutes: u64,
    pub tokens: u64,
    pub words_min: u16,
    pub words_max: u16,
}

#[must_use]
fn view(tier: Tier, budget: TierBudget) -> TierBudgetView {
    TierBudgetView {
        tier: tier.as_str().to_owned(),
        expansions: budget.expansions,
        branch: budget.branch,
        sources_min: *budget.sources.start(),
        sources_max: *budget.sources.end(),
        min_tier2: budget.min_tier2,
        min_primary: budget.min_primary,
        target_dots: budget.target_dots,
        counter_passes: budget.counter_passes,
        timeline: budget.timeline,
        entity_deep_dives: budget.entity_deep_dives,
        wall_minutes: budget.wall.as_secs() / 60,
        tokens: budget.tokens,
        words_min: *budget.words.start(),
        words_max: *budget.words.end(),
    }
}

/// Every tier's budget, ordered X2 → X24.
#[must_use]
pub fn tier_budget_views() -> Vec<TierBudgetView> {
    ALL_TIERS.map(|tier| view(tier, tier.budget())).to_vec()
}

#[cfg(test)]
mod tests {
    use super::tier_budget_views;

    #[test]
    fn views_cover_all_four_tiers_in_order() {
        let views = tier_budget_views();
        let names: Vec<&str> = views.iter().map(|view| view.tier.as_str()).collect();
        assert_eq!(names, ["X2", "X6", "X12", "X24"]);
        assert_eq!(views[1].expansions, 6);
        assert_eq!(views[3].sources_max, 200);
    }
}
