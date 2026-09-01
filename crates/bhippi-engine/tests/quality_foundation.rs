//! Frozen Phase 9 schema semantics. These fixtures are intentionally network-free and make
//! an incompatible scenario/rubric change visible in code review.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::game_quality::{
    GameQualityEvaluation, QualityDimension, QualityEvidence, QualityEvidenceKind,
    QualityMeasurement, QualityMeasurementStatus,
};
use bhippi_engine::game_test_plan::{GameTestPlan, MANDATORY_SMOKE_SCENARIO};
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/engine/quality")
        .join(name);
    std::fs::read_to_string(path).expect("quality fixture is committed")
}

#[test]
fn game_test_plan_v1_fixture_round_trips_byte_stably() {
    let expected = fixture("game-test-plan-v1.json");
    let plan = GameTestPlan::parse(&expected).expect("v1 test plan parses");
    let dumped = format!("{}\n", plan.dump().expect("plan serialises"));
    assert_eq!(dumped, expected);

    assert_eq!(plan.scenarios[0].input.len(), 3);
    assert_eq!(plan.scenarios[0].checkpoints.len(), 2);
}

#[test]
fn absent_plan_gets_the_engine_smoke_but_empty_or_future_plans_block() {
    let level = "assets/scenes/main.bscn.json";
    let smoke = GameTestPlan::resolve(None, level).expect("smoke is supplied");
    assert_eq!(smoke.scenarios[0].name, MANDATORY_SMOKE_SCENARIO);
    assert_eq!(smoke.scenarios[0].initial_level, level);
    assert_eq!(smoke.scenarios[0].seed, 0);
    assert_eq!(smoke.scenarios[0].checkpoints.len(), 1);

    let future = r#"{"format":"bhippi-game-test-plan@2","scenarios":[]}"#;
    let error = GameTestPlan::parse(future).expect_err("future major blocks");
    assert!(error.hint().is_some());

    let empty = r#"{"format":"bhippi-game-test-plan@1","scenarios":[]}"#;
    assert!(GameTestPlan::parse(empty).is_err());
}

#[test]
fn unordered_schedule_and_vacuous_assertions_are_rejected() {
    let mut plan = GameTestPlan::parse(&fixture("game-test-plan-v1.json")).expect("fixture");
    plan.scenarios[0].input[1].at_ms = 700;
    plan.scenarios[0].input[2].at_ms = 600;
    assert!(plan.validate().is_err());

    let mut plan = GameTestPlan::parse(&fixture("game-test-plan-v1.json")).expect("fixture");
    plan.scenarios[0].checkpoints[0].assertions.clear();
    let error = plan
        .validate()
        .expect_err("empty checkpoint proves nothing");
    assert!(error.hint().is_some());
}

#[test]
fn rubric_fixture_is_canonical_and_missing_dimensions_are_not_measured() {
    let evidence = QualityEvidence {
        kind: QualityEvidenceKind::ScenarioAssertion,
        address: "warehouse_key_door/initial_level_loaded".to_owned(),
        summary: "The fixed-seed runtime loaded the declared initial level.".to_owned(),
        artifact_hash: Some("blake3:7e50f".to_owned()),
    };
    let bootability =
        QualityMeasurement::measured(QualityDimension::Bootability, 100, 1.0, vec![evidence])
            .expect("evidence-backed score");
    let evaluation =
        GameQualityEvaluation::from_measurements(vec![bootability]).expect("rubric fills gaps");

    assert_eq!(evaluation.measurements.len(), 10);
    assert!(evaluation.deterministic_score.is_none());
    assert!(evaluation.measurements[1..]
        .iter()
        .all(|measurement| measurement.status == QualityMeasurementStatus::NotMeasured));

    let expected = fixture("quality-rubric-v1.json");
    let dumped = format!("{}\n", evaluation.dump().expect("evaluation serialises"));
    assert_eq!(dumped, expected);
    assert_eq!(
        GameQualityEvaluation::parse(&expected).expect("golden parses"),
        evaluation
    );
}

#[test]
fn score_without_evidence_and_forged_aggregate_are_rejected() {
    let error =
        QualityMeasurement::measured(QualityDimension::VisualLegibility, 95, 0.8, Vec::new())
            .expect_err("a model cannot self-score without evidence");
    assert!(error.hint().is_some());

    let mut evaluation =
        GameQualityEvaluation::parse(&fixture("quality-rubric-v1.json")).expect("fixture");
    evaluation.deterministic_score = Some(100);
    assert!(evaluation.validate().is_err());
}
