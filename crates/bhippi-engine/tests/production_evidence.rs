#![allow(clippy::expect_used)]

use bhippi_engine::production_evidence::{
    compare_regression, evaluate_release_proof, BenchmarkRun, BenchmarkSuite,
    CapabilityEvidenceBinding, CapabilityMatrixInput, EvidenceArtifact, EvidenceKind,
    EvidenceState, MaturityDimension, Measurement, MetricId, ProductionEvidenceError,
    ProductionEvidenceManifest, RegressionFloor, ReleaseProofRequest, ResilienceKind,
    ResilienceResult, ScreenshotComparison, EVIDENCE_MANIFEST_FORMAT, RELEASE_PROOF_FORMAT,
    RESILIENCE_RESULT_FORMAT,
};
use bhippi_engine::registry::CapabilityRegistry;
use std::collections::{BTreeMap, BTreeSet};

fn run(id: &str, fps: Option<u64>, cpu_micros: Option<u64>) -> BenchmarkRun {
    let mut measurements = BTreeMap::new();
    if let Some(value) = fps {
        measurements.insert(
            MetricId::FramesPerSecond,
            Measurement {
                value,
                sample_count: 600,
                evidence_id: format!("{id}-fps"),
            },
        );
    }
    if let Some(value) = cpu_micros {
        measurements.insert(
            MetricId::CpuFrameMicros,
            Measurement {
                value,
                sample_count: 600,
                evidence_id: format!("{id}-cpu"),
            },
        );
    }
    BenchmarkRun {
        descriptor_id: "static-1000".to_owned(),
        descriptor_digest: "fixture-descriptor-digest".to_owned(),
        run_id: id.to_owned(),
        host_fingerprint: "synthetic-test-host".to_owned(),
        platform: "test".to_owned(),
        backend: "test-backend".to_owned(),
        build_id: format!("test-{id}"),
        authored_tree_hash: "test-tree".to_owned(),
        measurements,
    }
}

fn artifact(id: &str, kind: EvidenceKind, capabilities: &[&str]) -> EvidenceArtifact {
    EvidenceArtifact {
        id: id.to_owned(),
        kind,
        relative_path: format!("synthetic/{id}.json"),
        digest: format!("synthetic-digest-{id}"),
        build_id: "test-build".to_owned(),
        authored_tree_hash: "test-tree".to_owned(),
        host_fingerprint: "synthetic-host".to_owned(),
        platform: "test".to_owned(),
        capability_ids: capabilities
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
    }
}

#[test]
fn frozen_descriptor_and_empty_evidence_fixtures_are_versioned_and_honest() {
    let suite: BenchmarkSuite = serde_json::from_str(include_str!(
        "../../../tests/fixtures/engine/production/benchmark-suite-v1.json"
    ))
    .expect("suite fixture parses");
    suite.validate().expect("descriptor suite validates");
    assert!(suite
        .descriptors
        .iter()
        .all(|descriptor| descriptor.source_digest.is_none()));

    let evidence: ProductionEvidenceManifest = serde_json::from_str(include_str!(
        "../../../tests/fixtures/engine/production/empty-evidence-manifest-v1.json"
    ))
    .expect("evidence fixture parses");
    evidence.validate().expect("empty evidence is valid");
    assert!(evidence.artifacts.is_empty());
}

#[test]
fn higher_and_lower_regression_floors_use_integer_basis_point_math() {
    let floors = vec![
        RegressionFloor {
            metric: MetricId::FramesPerSecond,
            minimum: Some(55),
            maximum: None,
            maximum_regression_basis_points: 500,
        },
        RegressionFloor {
            metric: MetricId::CpuFrameMicros,
            minimum: None,
            maximum: Some(18_000),
            maximum_regression_basis_points: 1_000,
        },
    ];
    let passing = compare_regression(
        &floors,
        &run("base", Some(60), Some(15_000)),
        &run("candidate", Some(58), Some(16_000)),
    )
    .expect("comparable runs");
    assert_eq!(passing.state, EvidenceState::Passed);

    let failing = compare_regression(
        &floors,
        &run("base", Some(60), Some(15_000)),
        &run("candidate", Some(54), Some(18_001)),
    )
    .expect("comparable runs");
    assert_eq!(failing.state, EvidenceState::Failed);
    assert!(failing
        .comparisons
        .iter()
        .all(|comparison| comparison.state == EvidenceState::Failed));
}

#[test]
fn missing_or_incomparable_benchmark_evidence_never_passes() {
    let floors = [RegressionFloor {
        metric: MetricId::FramesPerSecond,
        minimum: Some(55),
        maximum: None,
        maximum_regression_basis_points: 500,
    }];
    let missing = compare_regression(
        &floors,
        &run("base", Some(60), None),
        &run("candidate", None, None),
    )
    .expect("missing measurements produce a report");
    assert_eq!(missing.state, EvidenceState::NotMeasured);

    let baseline = run("base", Some(60), None);
    let mut other_host = run("candidate", Some(60), None);
    other_host.host_fingerprint = "different-host".to_owned();
    let blocked = compare_regression(&floors, &baseline, &other_host).expect("blocked report");
    assert_eq!(blocked.state, EvidenceState::Blocked);
}

#[test]
fn resilience_results_require_real_attempt_identity_and_evidence_to_pass() {
    let missing = ResilienceResult {
        format: RESILIENCE_RESULT_FORMAT.to_owned(),
        case_id: "soak-editor".to_owned(),
        kind: ResilienceKind::Soak,
        state: EvidenceState::Passed,
        attempted: true,
        host_fingerprint: None,
        build_id: None,
        authored_tree_hash: None,
        iterations: 1,
        duration_micros: 60_000_000,
        injected_faults: 0,
        recovered_faults: 0,
        evidence_ids: Vec::new(),
        reason: None,
    };
    assert!(matches!(
        missing.validate(),
        Err(ProductionEvidenceError::MissingEvidence(_))
    ));

    let not_measured = ResilienceResult {
        state: EvidenceState::NotMeasured,
        attempted: false,
        iterations: 0,
        duration_micros: 0,
        ..missing
    };
    not_measured.validate().expect("honest absence is valid");
}

#[test]
fn evidence_manifest_covers_contract_and_typed_screenshot_artifacts() {
    let artifacts = vec![
        artifact("serde", EvidenceKind::SerializationContract, &[]),
        artifact("api", EvidenceKind::PublicApiContract, &[]),
        artifact("shot-base", EvidenceKind::ScreenshotBaseline, &[]),
        artifact("shot-now", EvidenceKind::ScreenshotCandidate, &[]),
        artifact("shot-diff", EvidenceKind::ScreenshotDiff, &[]),
    ];
    let comparison = ScreenshotComparison {
        id: "engine-shell".to_owned(),
        baseline_evidence_id: "shot-base".to_owned(),
        candidate_evidence_id: "shot-now".to_owned(),
        diff_evidence_id: "shot-diff".to_owned(),
        changed_pixels: 5,
        total_pixels: 10_000,
        maximum_changed_basis_points: 10,
    };
    let manifest = ProductionEvidenceManifest {
        format: EVIDENCE_MANIFEST_FORMAT.to_owned(),
        manifest_id: "synthetic-manifest".to_owned(),
        artifacts,
        screenshot_comparisons: vec![comparison.clone()],
    };
    manifest.validate().expect("typed artifact set validates");
    assert_eq!(
        manifest.screenshot_state(&comparison).expect("state"),
        EvidenceState::Passed
    );

    let mut unsafe_manifest = manifest;
    unsafe_manifest.artifacts[0].relative_path = "../outside.json".to_owned();
    assert!(matches!(
        unsafe_manifest.validate(),
        Err(ProductionEvidenceError::UnsafePath(_))
    ));
}

#[test]
fn capability_matrix_uses_registry_rows_but_only_evidence_can_prove_dimensions() {
    let registry = CapabilityRegistry::core().expect("core registry");
    let capability = "component.transform";
    let evidence = ProductionEvidenceManifest {
        format: EVIDENCE_MANIFEST_FORMAT.to_owned(),
        manifest_id: "synthetic-capability-evidence".to_owned(),
        artifacts: vec![artifact(
            "transform-contract-test",
            EvidenceKind::PublicApiContract,
            &[capability],
        )],
        screenshot_comparisons: Vec::new(),
    };
    let matrix = CapabilityMatrixInput::regenerate(
        &registry,
        &evidence,
        &[CapabilityEvidenceBinding {
            capability_id: capability.to_owned(),
            dimension: MaturityDimension::Tested,
            evidence_ids: vec!["transform-contract-test".to_owned()],
        }],
    )
    .expect("matrix input regenerates");
    let row = matrix
        .rows
        .iter()
        .find(|row| row.capability_id == capability)
        .expect("transform row");
    assert_eq!(
        row.dimensions[&MaturityDimension::Tested].state,
        EvidenceState::Passed
    );
    assert_eq!(
        row.dimensions[&MaturityDimension::RuntimeProven].state,
        EvidenceState::NotMeasured,
        "a registry declaration alone is not production evidence"
    );
    assert_eq!(row.dimensions.len(), 7);
}

#[test]
fn final_release_proof_blocks_missing_stale_and_unproven_inputs() {
    let registry = CapabilityRegistry::core().expect("core registry");
    let evidence = ProductionEvidenceManifest {
        format: EVIDENCE_MANIFEST_FORMAT.to_owned(),
        manifest_id: "no-final-golden".to_owned(),
        artifacts: vec![artifact("stale-launch", EvidenceKind::Launch, &[])],
        screenshot_comparisons: Vec::new(),
    };
    let matrix = CapabilityMatrixInput::regenerate(&registry, &evidence, &[])
        .expect("unproven matrix still represents truth");
    let request = ReleaseProofRequest {
        format: RELEASE_PROOF_FORMAT.to_owned(),
        authored_tree_hash: "current-tree".to_owned(),
        build_id: "current-build".to_owned(),
        platform: "test".to_owned(),
        required_capabilities: vec!["component.transform".to_owned()],
        required_evidence: BTreeSet::from([
            EvidenceKind::Save,
            EvidenceKind::Export,
            EvidenceKind::Launch,
        ]),
    };
    let decision = evaluate_release_proof(&request, &matrix, &evidence).expect("decision");
    assert_eq!(decision.state, EvidenceState::Blocked);
    assert!(decision.blockers.len() >= 5);
    assert!(decision.evidence_ids.is_empty());
}
