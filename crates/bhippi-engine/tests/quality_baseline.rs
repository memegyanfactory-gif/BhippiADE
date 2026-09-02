//! Deterministic quality-baseline and CI regression semantics.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::game_quality::{
    GameQualityEvaluation, QualityDimension, QualityEvidence, QualityEvidenceKind,
    QualityMeasurement,
};
use bhippi_engine::game_quality_baseline::{
    compare_quality_run, evaluate_static_corpus, GameQualityBaseline, GameQualityRun,
    QualityRegressionPolicy,
};
use bhippi_engine::game_quality_corpus::GameQualityCorpus;
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/engine/quality")
}

fn corpus() -> GameQualityCorpus {
    let text = std::fs::read_to_string(fixture_root().join("quality-corpus-v1.json"))
        .expect("committed corpus");
    GameQualityCorpus::parse(&text).expect("valid corpus")
}

fn committed_baseline() -> GameQualityBaseline {
    let text = std::fs::read_to_string(fixture_root().join("quality-baseline-v1.json"))
        .expect("committed baseline");
    GameQualityBaseline::parse(&text).expect("valid committed baseline")
}

fn scored_run(corpus: &GameQualityCorpus, score: u8) -> GameQualityRun {
    let mut run = evaluate_static_corpus(corpus, &fixture_root()).expect("static run");
    for case in &mut run.cases {
        let measurements = QualityDimension::all()
            .into_iter()
            .map(|dimension| {
                QualityMeasurement::measured(
                    dimension,
                    score,
                    1.0,
                    vec![QualityEvidence {
                        kind: QualityEvidenceKind::ScenarioAssertion,
                        address: format!("{}/deterministic", case.id),
                        summary: "frozen deterministic evidence".to_owned(),
                        artifact_hash: None,
                    }],
                )
                .expect("measured evidence")
            })
            .collect();
        case.evaluation =
            GameQualityEvaluation::from_measurements(measurements).expect("complete rubric");
    }
    run
}

#[test]
fn static_corpus_run_is_canonical_and_keeps_quality_unmeasured() {
    let corpus = corpus();
    let run = evaluate_static_corpus(&corpus, &fixture_root()).expect("static corpus evaluates");
    run.validate_against(&corpus).expect("run matches corpus");
    assert_eq!(
        run.cases
            .iter()
            .map(|case| case.blocker_codes.clone())
            .collect::<Vec<_>>(),
        corpus
            .cases
            .iter()
            .map(|case| case.expected_finding_codes.clone())
            .collect::<Vec<_>>()
    );
    assert!(run
        .cases
        .iter()
        .all(|case| case.evaluation.deterministic_score.is_none()));
    let dumped = run.dump().expect("run dumps");
    assert_eq!(GameQualityRun::parse(&dumped).expect("run parses"), run);
}

#[test]
fn committed_baseline_accepts_the_current_frozen_corpus_run() {
    let corpus = corpus();
    let candidate = evaluate_static_corpus(&corpus, &fixture_root()).expect("static corpus");
    let baseline = committed_baseline();
    baseline
        .validate_against(&corpus)
        .expect("baseline matches exact corpus");
    let comparison =
        compare_quality_run(&corpus, &baseline, &candidate).expect("baseline comparison");
    assert!(comparison.passed);
    assert!(comparison.cases.iter().all(|case| case.passed));
}

#[test]
fn baseline_round_trip_and_unchanged_candidate_pass() {
    let corpus = corpus();
    let run = scored_run(&corpus, 80);
    let baseline = GameQualityBaseline::record(&corpus, &run, QualityRegressionPolicy::default())
        .expect("baseline records");
    let dumped = baseline.dump().expect("baseline dumps");
    assert_eq!(
        GameQualityBaseline::parse(&dumped).expect("baseline parses"),
        baseline
    );
    let comparison = compare_quality_run(&corpus, &baseline, &run).expect("comparison");
    assert!(comparison.passed);
    assert!(comparison.cases.iter().all(|case| case.passed));
    comparison.validate().expect("comparison validates");
}

#[test]
fn every_case_blocks_new_or_disappearing_defect_oracles() {
    let corpus = corpus();
    let run = scored_run(&corpus, 80);
    let baseline = GameQualityBaseline::record(&corpus, &run, QualityRegressionPolicy::default())
        .expect("baseline");

    let mut new_blocker = run.clone();
    new_blocker.cases[0].blocker_codes = vec!["BHP-GD-999".to_owned()];
    let compared =
        compare_quality_run(&corpus, &baseline, &new_blocker).expect("new blocker compares");
    assert!(!compared.passed);
    assert_eq!(compared.cases[0].new_blockers, ["BHP-GD-999"]);

    let mut missing_oracle = run;
    missing_oracle.cases[4].blocker_codes.clear();
    let compared =
        compare_quality_run(&corpus, &baseline, &missing_oracle).expect("oracle compares");
    assert!(!compared.passed);
    assert_eq!(
        compared.cases[4].missing_expected_blockers,
        ["BHP-GD-302", "BHP-GD-303", "BHP-GD-306", "BHP-GD-307"]
    );
}

#[test]
fn newly_unmeasured_and_material_score_regressions_block_but_small_drift_does_not() {
    let corpus = corpus();
    let run = scored_run(&corpus, 80);
    let baseline = GameQualityBaseline::record(&corpus, &run, QualityRegressionPolicy::default())
        .expect("baseline");

    let mut missing = run.clone();
    missing.cases[1].evaluation = GameQualityEvaluation::from_measurements(
        QualityDimension::all()
            .into_iter()
            .filter(|dimension| *dimension != QualityDimension::VisualLegibility)
            .map(|dimension| {
                QualityMeasurement::measured(
                    dimension,
                    80,
                    1.0,
                    vec![QualityEvidence {
                        kind: QualityEvidenceKind::Observation,
                        address: "capture://fixed".to_owned(),
                        summary: "fixed observation".to_owned(),
                        artifact_hash: None,
                    }],
                )
                .expect("measurement")
            })
            .collect(),
    )
    .expect("partial evaluation");
    let compared = compare_quality_run(&corpus, &baseline, &missing).expect("missing compares");
    assert!(!compared.passed);
    assert_eq!(
        compared.cases[1].newly_unmeasured_dimensions,
        [QualityDimension::VisualLegibility]
    );
    assert!(compared.cases[1].aggregate_became_unmeasured);

    let material = scored_run(&corpus, 70);
    let compared = compare_quality_run(&corpus, &baseline, &material).expect("drop compares");
    assert!(!compared.passed);
    assert!(!compared.cases[0].regressions.is_empty());

    let small = scored_run(&corpus, 78);
    let compared = compare_quality_run(&corpus, &baseline, &small).expect("small drift compares");
    assert!(compared.passed);
}

#[test]
fn corpus_drift_invalidates_run_and_baseline_instead_of_relabeling_them() {
    let corpus = corpus();
    let run = scored_run(&corpus, 80);
    let baseline = GameQualityBaseline::record(&corpus, &run, QualityRegressionPolicy::default())
        .expect("baseline");
    let mut drifted = corpus.clone();
    drifted.cases[0].prompt.blake3 = "0".repeat(64);
    assert!(run.validate_against(&drifted).is_err());
    assert!(baseline.validate_against(&drifted).is_err());
}

#[test]
fn comparison_rejects_forged_or_duplicate_deltas() {
    let corpus = corpus();
    let baseline_run = scored_run(&corpus, 80);
    let baseline =
        GameQualityBaseline::record(&corpus, &baseline_run, QualityRegressionPolicy::default())
            .expect("baseline");
    let candidate = scored_run(&corpus, 70);
    let comparison = compare_quality_run(&corpus, &baseline, &candidate).expect("comparison");

    let mut forged = comparison.clone();
    forged.cases[0].regressions[0].regression_basis_points = 1;
    assert!(forged.validate().is_err());

    let mut duplicate = comparison;
    duplicate.cases[0]
        .newly_unmeasured_dimensions
        .push(QualityDimension::Bootability);
    assert!(duplicate.validate().is_err());
}
