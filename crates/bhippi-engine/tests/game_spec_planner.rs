//! Frozen GameSpec and registry-first composition planning contracts.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::game_spec::{
    compose_plan, CompositionDecision, FactCertainty, GameSpec, PlanNodePayload,
};
use bhippi_engine::registry::{CapabilityKind, CapabilityRegistry, CostClass};
use std::path::PathBuf;

fn fixture() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/engine/game_spec/survival-v1.json");
    std::fs::read_to_string(path).expect("GameSpec fixture is committed")
}

fn spec() -> GameSpec {
    GameSpec::parse(&fixture()).expect("fixture parses")
}

#[test]
fn game_spec_v1_round_trips_and_future_major_blocks() {
    let parsed = spec();
    assert_eq!(
        GameSpec::parse(&parsed.dump().expect("dump")).expect("reparse"),
        parsed
    );

    let future = fixture().replacen("bhippi-game-spec@1", "bhippi-game-spec@2", 1);
    let error = GameSpec::parse(&future).expect_err("unknown major blocks");
    assert!(error.hint().is_some());
}

#[test]
fn ambiguity_and_material_questions_are_explicit_not_guessed() {
    let mut invalid = spec();
    invalid.genre.certainty = FactCertainty::Ambiguous;
    invalid.genre.alternatives = vec!["survival".to_owned()];
    assert!(invalid.validate().is_err());

    let mut unresolved = spec();
    unresolved.open_questions[0].resolved = None;
    let error = compose_plan(&unresolved, &CapabilityRegistry::core().expect("registry"))
        .expect_err("material ambiguity blocks planning");
    assert!(error.message.contains("unresolved"));
}

#[test]
fn planner_is_deterministic_registry_bound_and_emits_a_valid_dag() {
    let registry = CapabilityRegistry::core().expect("registry");
    let first = compose_plan(&spec(), &registry).expect("plan");
    let second = compose_plan(&spec(), &registry).expect("same plan");
    assert_eq!(first, second);
    assert_eq!(first.registry_hash, registry.hash);
    assert_eq!(first.project_state_delta.registry_hash, registry.hash);
    assert!(first.estimated_card_tokens <= 2000);
    assert!(first.queries.iter().all(|query| query.estimated_tokens > 0));
    assert!(first
        .nodes
        .iter()
        .any(|node| matches!(node.payload, PlanNodePayload::BudgetGuard { .. })));
    assert!(first
        .nodes
        .iter()
        .any(|node| matches!(node.payload, PlanNodePayload::TestScenario { .. })));
    assert!(first.nodes.iter().any(|node| {
        node.decision
            .as_ref()
            .is_some_and(|decision| decision.strategy == CompositionDecision::Integrate)
    }));
    first.validate(2000).expect("DAG validates");
}

#[test]
fn plan_cards_block_at_the_declared_turn_budget() {
    let mut tiny = spec();
    tiny.constraints.budgets.turn_tokens = 1;
    let error = compose_plan(&tiny, &CapabilityRegistry::core().expect("registry"))
        .expect_err("token budget blocks before generation");
    assert!(error.message.contains("token"));
    assert!(error.hint.contains("split"));

    let mut valid =
        compose_plan(&spec(), &CapabilityRegistry::core().expect("registry")).expect("normal plan");
    valid.estimated_card_tokens = 0;
    assert!(
        valid.validate(2000).is_err(),
        "token totals cannot be forged"
    );
}

#[test]
fn incompatible_preference_fails_before_writes_and_returns_registry_alternatives() {
    let registry = CapabilityRegistry::core().expect("registry");
    let mut incompatible = spec();
    incompatible.mechanics.truncate(1);
    incompatible.mechanics[0].statement = "build package export windows web".to_owned();
    incompatible.mechanics[0].preferred_capabilities = vec!["export.windows".to_owned()];
    incompatible.mechanics[0].maturity = Default::default();
    incompatible.constraints.budgets.max_capability_cost = None;
    incompatible.acceptance_mechanics[0].requirement_ids = vec!["player_motion".to_owned()];
    let error = compose_plan(&incompatible, &registry).expect_err("platform mismatch blocks");
    assert!(error.message.contains("incompatible"));
    assert!(!error.alternatives.is_empty());
}

#[test]
fn missing_maturity_adapts_and_missing_capability_builds_only_inside_extension_budget() {
    let registry = CapabilityRegistry::core().expect("registry");
    let mut adaptive = spec();
    adaptive.mechanics.truncate(1);
    adaptive.mechanics[0].preferred_capabilities.clear();
    adaptive.mechanics[0].maturity.production_ready = true;
    adaptive.acceptance_mechanics[0].requirement_ids = vec!["player_motion".to_owned()];
    let plan = compose_plan(&adaptive, &registry).expect("partial registry match adapts");
    assert!(plan.nodes.iter().any(|node| {
        node.decision
            .as_ref()
            .is_some_and(|decision| decision.strategy == CompositionDecision::Adapt)
    }));

    let mut novel = adaptive;
    novel.mechanics[0].statement = "quantum entropy grapple lattice".to_owned();
    novel.mechanics[0].maturity = Default::default();
    novel.constraints.budgets.max_new_extensions = 1;
    let plan = compose_plan(&novel, &registry).expect("one bounded extension allowed");
    assert!(plan
        .nodes
        .iter()
        .any(|node| matches!(node.payload, PlanNodePayload::ProjectExtension { .. })));

    novel.constraints.budgets.max_new_extensions = 0;
    assert!(compose_plan(&novel, &registry).is_err());
}

#[test]
fn registered_extensions_record_wrap_instead_of_pretending_they_are_core() {
    let core = CapabilityRegistry::core().expect("registry");
    let mut extension = core.entries[0].clone();
    extension.id = "extension.quantum_grapple".to_owned();
    extension.name = "Quantum Grapple".to_owned();
    extension.kind = CapabilityKind::Extension;
    extension.cost = CostClass::Low;
    extension.available = true;
    extension.unavailable_reason = None;
    extension.category = "gameplay".to_owned();
    extension.purpose = "quantum entropy grapple lattice".to_owned();
    extension.keywords = vec![
        "quantum".to_owned(),
        "entropy".to_owned(),
        "grapple".to_owned(),
        "lattice".to_owned(),
    ];
    extension.platforms = vec!["web".to_owned(), "windows".to_owned()];
    extension.relations.clear();
    let registry = CapabilityRegistry::build(vec![extension]).expect("extension registry");

    let mut wrapped = spec();
    wrapped.mechanics.truncate(1);
    wrapped.mechanics[0].statement = "quantum entropy grapple lattice".to_owned();
    wrapped.mechanics[0].preferred_capabilities.clear();
    wrapped.mechanics[0].maturity = Default::default();
    wrapped.acceptance_mechanics[0].requirement_ids = vec!["player_motion".to_owned()];
    let plan = compose_plan(&wrapped, &registry).expect("wrapper plan");
    assert!(
        plan.nodes.iter().any(|node| {
            node.decision
                .as_ref()
                .is_some_and(|decision| decision.strategy == CompositionDecision::Wrap)
        }),
        "{plan:#?}"
    );
}
