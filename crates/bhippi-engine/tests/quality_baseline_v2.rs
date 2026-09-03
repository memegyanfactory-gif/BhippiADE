//! Tests for Phase 8 GAD-130 & GAD-131: Corpus v2 and Quality Baseline v2 on Godot path.

use bhippi_engine::game_quality::QualityMeasurementStatus;
use bhippi_engine::game_quality_baseline::{
    compare_quality_run, evaluate_static_corpus, GameQualityBaseline,
};
use bhippi_engine::game_quality_corpus::{GameQualityCorpus, CANONICAL_GAME_COUNT_V2};
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/engine/quality")
}

fn corpus_v2() -> GameQualityCorpus {
    let text = std::fs::read_to_string(fixture_root().join("quality-corpus-v2.json"))
        .expect("quality-corpus-v2.json exists");
    GameQualityCorpus::parse(&text).expect("valid corpus-v2")
}

fn baseline_v2() -> GameQualityBaseline {
    let text = std::fs::read_to_string(fixture_root().join("quality-baseline-v2.json"))
        .expect("quality-baseline-v2.json exists");
    GameQualityBaseline::parse(&text).expect("valid baseline-v2")
}

#[test]
fn gad_130_ten_corpus_games_across_all_archetypes_verify_at_fixture_root() {
    let corpus = corpus_v2();
    assert_eq!(corpus.cases.len(), CANONICAL_GAME_COUNT_V2);
    assert_eq!(corpus.cases.len(), 10);

    // Verify all 10 archetypes are covered
    let genres: Vec<&str> = corpus.cases.iter().map(|c| c.genre.as_str()).collect();
    assert!(genres.contains(&"endless_runner"));
    assert!(genres.contains(&"exploration"));
    assert!(genres.contains(&"fps_arena"));
    assert!(genres.contains(&"platformer_2d"));
    assert!(genres.contains(&"platformer_3d"));
    assert!(genres.contains(&"puzzle_physics"));
    assert!(genres.contains(&"racing_kart"));
    assert!(genres.contains(&"survival"));
    assert!(genres.contains(&"top_down_action"));
    assert!(genres.contains(&"tower_defense"));

    // Verify all files on disk match Blake3 hashes exactly
    corpus
        .verify_at(&fixture_root())
        .expect("corpus files match blake3 hashes");
}

#[test]
fn gad_131_quality_baseline_v2_evaluates_and_compares_cleanly() {
    let corpus = corpus_v2();
    let baseline = baseline_v2();

    baseline
        .validate_against(&corpus)
        .expect("baseline matches corpus");

    let run = evaluate_static_corpus(&corpus, &fixture_root()).expect("static evaluation succeeds");
    assert_eq!(run.cases.len(), 10);

    let comparison = compare_quality_run(&corpus, &baseline, &run).expect("comparison succeeds");
    assert!(
        comparison.passed,
        "clean evaluation must pass baseline comparison"
    );
}

#[test]
fn gad_131_unmeasured_dimensions_stay_not_measured_and_never_become_zero() {
    let corpus = corpus_v2();
    let run = evaluate_static_corpus(&corpus, &fixture_root()).expect("static evaluation succeeds");

    for case in &run.cases {
        for measurement in &case.evaluation.measurements {
            // In static evaluation, visual/runtime dimensions have no observations
            // and must stay NotMeasured rather than defaulting or converting to 0.
            if measurement.status == QualityMeasurementStatus::NotMeasured {
                assert!(
                    measurement.score.is_none(),
                    "unmeasured dimension {:?} must have score None, got {:?}",
                    measurement.dimension,
                    measurement.score
                );
                assert_ne!(
                    measurement.score,
                    Some(0),
                    "unmeasured dimension {:?} must NEVER be zero (INV-086/GAD-131)",
                    measurement.dimension
                );
            }
        }
        // When unmeasured dimensions exist, complete aggregate score must be None
        assert!(
            case.evaluation.deterministic_score.is_none(),
            "deterministic score must be None when any dimension is unmeasured"
        );
    }
}

#[test]
fn gad_131_new_blocker_code_causes_comparison_failure() {
    let corpus = corpus_v2();
    let baseline = baseline_v2();
    let mut run =
        evaluate_static_corpus(&corpus, &fixture_root()).expect("static evaluation succeeds");

    // Inject a fake defect blocker into the first case
    run.cases[0].blocker_codes.push("BHP-GD-499".to_owned());
    run.cases[0].blocker_codes.sort();

    let comparison = compare_quality_run(&corpus, &baseline, &run).expect("comparison succeeds");
    assert!(
        !comparison.passed,
        "injected blocker code must cause comparison failure"
    );
    assert_eq!(
        comparison.cases[0].new_blockers,
        vec!["BHP-GD-499".to_owned()]
    );
}
