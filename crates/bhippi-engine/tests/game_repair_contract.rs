//! Phase 10 repair safety contracts. Execution and journal writes belong to the app layer;
//! these tests pin the pure decisions that must happen before and after those writes.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::action::{EngineAction, EngineActionBatch};
use bhippi_engine::game_debug::{
    EvaluationStatus, GameDebugFinding, GameDebugMode, GameDebugReport,
};
use bhippi_engine::game_quality::{
    GameQualityEvaluation, QualityDimension, QualityEvidence, QualityEvidenceKind,
    QualityMeasurement,
};
use bhippi_engine::game_repair::{
    assess_attempt, best_verified_state, compare_before_after, ensure_report_fresh,
    guard_candidate, DimensionChangeKind, FindingChangeKind, RepairAttempt, RepairGuardDecision,
    RepairPlan, RepairPolicy, RepairProposal, RepairStopReason, RepairVerificationDecision,
    VerifiedRepairState, REPAIR_PLAN_SCHEMA,
};

fn finding(code: &str, severity: &str) -> GameDebugFinding {
    GameDebugFinding {
        code: code.to_owned(),
        severity: severity.to_owned(),
        stage: "02_validate".to_owned(),
        address: "assets/scenes/main.bscn.json".to_owned(),
        message: format!("finding {code}"),
        evidence: "frozen evidence".to_owned(),
        reproduction: "run the same stage".to_owned(),
        repair: "apply a typed action batch".to_owned(),
    }
}

fn report(run: &str, hash: &str, findings: Vec<GameDebugFinding>) -> GameDebugReport {
    GameDebugReport {
        schema: "bhippi-game-debug@1".to_owned(),
        run_id: run.to_owned(),
        mode: GameDebugMode::Full,
        project: "warehouse".to_owned(),
        started_at: "2026-09-02T00:00:00Z".to_owned(),
        authored_tree_before: hash.to_owned(),
        authored_tree_after: hash.to_owned(),
        stages: Vec::new(),
        findings,
        quality: EvaluationStatus {
            status: "not_evaluated".to_owned(),
            reason: "test".to_owned(),
        },
        sandbox: EvaluationStatus {
            status: "not_evaluated".to_owned(),
            reason: "test".to_owned(),
        },
        runtime: None,
        test_plan: None,
        test_batch: None,
        artifacts: Vec::new(),
        repair_batch_id: None,
        outcome: "failed".to_owned(),
    }
}

fn proposal(code: &str, weather: &str) -> RepairProposal {
    RepairProposal {
        finding_codes: vec![code.to_owned()],
        batch: EngineActionBatch {
            label: format!("Repair {code}"),
            actions: vec![EngineAction::SetWeather {
                weather: weather.to_owned(),
            }],
        },
    }
}

fn attempt(
    id: &str,
    code: &str,
    patch: &str,
    before: &str,
    after: &str,
    unresolved: bool,
) -> RepairAttempt {
    RepairAttempt {
        attempt_id: id.to_owned(),
        finding_codes: vec![code.to_owned()],
        patch_hash: patch.to_owned(),
        before_hash: before.to_owned(),
        after_hash: after.to_owned(),
        unresolved_finding_codes: if unresolved {
            vec![code.to_owned()]
        } else {
            Vec::new()
        },
    }
}

fn quality(score: u8) -> GameQualityEvaluation {
    let measurement = QualityMeasurement::measured(
        QualityDimension::Bootability,
        score,
        1.0,
        vec![QualityEvidence {
            kind: QualityEvidenceKind::ScenarioAssertion,
            address: "smoke/loaded".to_owned(),
            summary: "initial level loaded".to_owned(),
            artifact_hash: None,
        }],
    )
    .expect("measured");
    GameQualityEvaluation::from_measurements(vec![measurement]).expect("quality")
}

#[test]
fn stale_or_mutating_report_cannot_justify_a_repair() {
    let fresh = report("run-1", "hash-a", vec![finding("BHP-GD-1", "blocker")]);
    ensure_report_fresh(&fresh, "hash-a").expect("fresh");

    let stale = ensure_report_fresh(&fresh, "hash-b").expect_err("stale blocks");
    assert!(stale.to_string().contains("stale"));
    assert!(stale.hint().is_some());

    let mut mutating = fresh;
    mutating.authored_tree_after = "hash-b".to_owned();
    assert!(ensure_report_fresh(&mutating, "hash-b").is_err());
}

#[test]
fn repair_plan_is_grouped_canonical_and_hashes_the_typed_batch() {
    let report = report(
        "run-1",
        "hash-a",
        vec![
            finding("BHP-GD-2", "warning"),
            finding("BHP-GD-1", "blocker"),
        ],
    );
    let policy = RepairPolicy {
        max_attempts_per_finding: 2,
    };
    let first = RepairPlan::build(
        &report,
        "hash-a",
        vec![
            proposal("BHP-GD-2", "overcast"),
            proposal("BHP-GD-1", "clear"),
        ],
        &[],
        policy,
    )
    .expect("plan");
    let second = RepairPlan::build(
        &report,
        "hash-a",
        vec![
            proposal("BHP-GD-1", "clear"),
            proposal("BHP-GD-2", "overcast"),
        ],
        &[],
        policy,
    )
    .expect("same semantic plan");
    assert_eq!(first.schema, REPAIR_PLAN_SCHEMA);
    assert_eq!(first, second, "proposal ordering is harmless");
    assert_eq!(first.items[0].finding_codes, ["BHP-GD-1"]);
    assert_eq!(first.items[0].patch_hash.len(), 64);

    let unknown = RepairPlan::build(
        &report,
        "hash-a",
        vec![proposal("invented", "clear")],
        &[],
        policy,
    )
    .expect_err("only real finding codes can be repaired");
    assert!(unknown.hint().is_some());
}

#[test]
fn identical_patch_and_attempt_cap_stop_before_another_write() {
    let codes = vec!["BHP-GD-1".to_owned()];
    let policy = RepairPolicy {
        max_attempts_per_finding: 2,
    };
    let history = vec![attempt(
        "attempt-1",
        "BHP-GD-1",
        "patch-a",
        "state-a",
        "state-b",
        true,
    )];
    assert_eq!(
        guard_candidate(&codes, "patch-a", &history, policy),
        RepairGuardDecision::Stop {
            reason: RepairStopReason::IdenticalPatch,
            codes: codes.clone(),
        }
    );

    let history = vec![
        history[0].clone(),
        attempt(
            "attempt-2",
            "BHP-GD-1",
            "patch-b",
            "state-b",
            "state-c",
            true,
        ),
    ];
    assert_eq!(
        guard_candidate(&codes, "patch-c", &history, policy),
        RepairGuardDecision::Stop {
            reason: RepairStopReason::AttemptCap,
            codes,
        }
    );
}

#[test]
fn no_progress_oscillation_and_post_attempt_cap_are_explicit() {
    let policy = RepairPolicy {
        max_attempts_per_finding: 2,
    };
    let first = attempt(
        "attempt-1",
        "BHP-GD-1",
        "patch-a",
        "state-a",
        "state-b",
        true,
    );
    let no_progress = attempt(
        "attempt-2",
        "BHP-GD-1",
        "patch-b",
        "state-b",
        "state-b",
        true,
    );
    assert!(matches!(
        assess_attempt(std::slice::from_ref(&first), &no_progress, policy),
        RepairGuardDecision::Stop {
            reason: RepairStopReason::NoProgress,
            ..
        }
    ));

    let oscillating = attempt(
        "attempt-2",
        "BHP-GD-1",
        "patch-c",
        "state-b",
        "state-a",
        true,
    );
    assert!(matches!(
        assess_attempt(std::slice::from_ref(&first), &oscillating, policy),
        RepairGuardDecision::Stop {
            reason: RepairStopReason::Oscillation,
            ..
        }
    ));

    let capped = attempt(
        "attempt-2",
        "BHP-GD-1",
        "patch-d",
        "state-b",
        "state-c",
        true,
    );
    assert!(matches!(
        assess_attempt(&[first], &capped, policy),
        RepairGuardDecision::Stop {
            reason: RepairStopReason::AttemptCap,
            ..
        }
    ));
}

#[test]
fn before_after_names_changes_and_new_blocker_requires_rollback() {
    let before = report(
        "before",
        "state-a",
        vec![
            finding("resolved", "blocker"),
            finding("worsened", "warning"),
            finding("same", "warning"),
        ],
    );
    let after = report(
        "after",
        "state-b",
        vec![
            finding("worsened", "blocker"),
            finding("same", "warning"),
            finding("new-warning", "warning"),
        ],
    );
    let comparison = compare_before_after(
        &before,
        &after,
        vec!["txn-1".to_owned()],
        Some(&quality(70)),
        Some(&quality(85)),
    )
    .expect("comparison");

    assert!(comparison
        .findings
        .iter()
        .any(|change| change.code == "resolved" && change.kind == FindingChangeKind::Resolved));
    assert!(comparison
        .findings
        .iter()
        .any(|change| change.code == "new-warning" && change.kind == FindingChangeKind::New));
    assert!(comparison
        .findings
        .iter()
        .any(|change| change.code == "worsened" && change.kind == FindingChangeKind::Regressed));
    assert_eq!(
        comparison.dimensions[0].kind,
        DimensionChangeKind::Comparable
    );
    assert_eq!(comparison.dimensions[0].delta, Some(15));
    assert_eq!(
        comparison.decision,
        RepairVerificationDecision::RollBack {
            restore_authored_hash: "state-a".to_owned(),
            blocker_codes: vec!["worsened".to_owned()],
        }
    );
}

#[test]
fn best_verified_state_prefers_safety_then_findings_then_quality() {
    let states = vec![
        VerifiedRepairState {
            report_run_id: "unsafe".to_owned(),
            authored_hash: "a".to_owned(),
            blocker_count: 1,
            finding_count: 1,
            deterministic_quality_score: Some(100),
        },
        VerifiedRepairState {
            report_run_id: "good".to_owned(),
            authored_hash: "b".to_owned(),
            blocker_count: 0,
            finding_count: 2,
            deterministic_quality_score: Some(80),
        },
        VerifiedRepairState {
            report_run_id: "best".to_owned(),
            authored_hash: "c".to_owned(),
            blocker_count: 0,
            finding_count: 2,
            deterministic_quality_score: Some(90),
        },
    ];
    assert_eq!(
        best_verified_state(&states).expect("state").report_run_id,
        "best"
    );
}
