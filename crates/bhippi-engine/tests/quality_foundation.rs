//! Frozen Phase 9 schema semantics. These fixtures are intentionally network-free and make
//! an incompatible scenario/rubric change visible in code review.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::game_inspector;
use bhippi_engine::game_quality::{
    GameQualityEvaluation, QualityDimension, QualityEvidence, QualityEvidenceKind,
    QualityMeasurement, QualityMeasurementStatus,
};
use bhippi_engine::game_quality_corpus::GameQualityCorpus;
use bhippi_engine::game_test_plan::{GameTestPlan, MANDATORY_SMOKE_SCENARIO};
use bhippi_engine::manifest::parse_manifest;
use bhippi_engine::SceneDocument;
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/engine/quality")
}

fn fixture(name: &str) -> String {
    let path = fixture_root().join(name);
    std::fs::read_to_string(path).expect("quality fixture is committed")
}

#[test]
fn five_game_quality_corpus_is_content_addressed_and_structurally_authored() {
    let expected = fixture("quality-corpus-v1.json");
    let corpus = GameQualityCorpus::parse(&expected).expect("v1 corpus parses");
    corpus
        .verify_at(&fixture_root())
        .expect("every frozen corpus artifact matches its reviewed digest");
    assert_eq!(
        corpus
            .cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<Vec<_>>(),
        [
            "warehouse-key-door",
            "platformer-checkpoint",
            "top-down-collection",
            "hud-logic-puzzle",
            "broken-recovery"
        ]
    );
    assert_eq!(
        corpus.cases[4].expected_finding_codes,
        ["BHP-GD-302", "BHP-GD-303", "BHP-GD-306", "BHP-GD-307"]
    );

    for case in &corpus.cases {
        let authored_root = fixture_root().join(format!("corpus-v1/{}/authored", case.id));
        let manifest = std::fs::read_to_string(authored_root.join("Bhippi.game.toml"))
            .expect("authored manifest");
        let manifest =
            parse_manifest(&manifest).expect("corpus manifests remain valid engine documents");
        let scene = std::fs::read_to_string(authored_root.join("assets/scenes/main.bscn.json"))
            .expect("authored scene");
        let scene =
            SceneDocument::parse(&scene).expect("corpus scenes remain valid engine documents");
        let findings = game_inspector::inspect(
            &manifest,
            &[(manifest.game.default_scene.clone(), scene)],
            &[],
            None,
            &[],
        );
        assert_eq!(
            findings
                .iter()
                .map(|finding| finding.code.as_str())
                .collect::<Vec<_>>(),
            case.expected_finding_codes,
            "{} diagnostic oracle drifted",
            case.id
        );
    }

    let dumped = format!("{}\n", corpus.dump().expect("corpus serialises"));
    assert_eq!(dumped, expected);
}

#[test]
fn corpus_drift_and_cross_case_paths_are_blocking_errors() {
    let mut corpus =
        GameQualityCorpus::parse(&fixture("quality-corpus-v1.json")).expect("v1 corpus parses");
    corpus.cases[0].prompt.blake3 = "0".repeat(64);
    let error = corpus
        .verify_at(&fixture_root())
        .expect_err("artifact drift must block the benchmark");
    assert!(error.hint().is_some());

    let mut corpus =
        GameQualityCorpus::parse(&fixture("quality-corpus-v1.json")).expect("v1 corpus parses");
    corpus.cases[0].prompt.path = "corpus-v1/broken-recovery/prompt.txt".to_owned();
    assert!(corpus.validate().is_err());
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
