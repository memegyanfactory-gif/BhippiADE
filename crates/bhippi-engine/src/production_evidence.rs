//! Evidence-gated production benchmark and release-proof contracts (Phase 24).
//!
//! These types describe work that a real host runner must perform. A descriptor is not a
//! benchmark result, a declared capability is not production proof, and a missing capture is
//! always `not_measured` or `blocked` rather than an inferred pass.

use crate::registry::{CapabilityMaturity, CapabilityRegistry};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const BENCHMARK_SUITE_FORMAT: &str = "bhippi-benchmark-suite@1";
pub const RESILIENCE_RESULT_FORMAT: &str = "bhippi-resilience-result@1";
pub const EVIDENCE_MANIFEST_FORMAT: &str = "bhippi-production-evidence@1";
pub const CAPABILITY_MATRIX_INPUT_FORMAT: &str = "bhippi-capability-matrix-input@1";
pub const RELEASE_PROOF_FORMAT: &str = "bhippi-release-proof@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkClass {
    StaticEntities,
    DynamicEntities,
    AnimatedCharacters,
    AiCrowd,
    TerrainStreamingLoading,
    HeavyVfx,
    HeavyLighting,
    HeavyHud,
    HeavyPhysics,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MetricId {
    FramesPerSecond,
    OnePercentLowFramesPerSecond,
    CpuFrameMicros,
    GpuFrameMicros,
    PhysicsMicros,
    NavigationMicros,
    AnimationMicros,
    GameplayAiMicros,
    ResidentBytes,
    VramBytes,
    DrawCalls,
    LoadMicros,
}

impl MetricId {
    #[must_use]
    pub fn direction(self) -> MetricDirection {
        match self {
            Self::FramesPerSecond | Self::OnePercentLowFramesPerSecond => {
                MetricDirection::HigherIsBetter
            }
            Self::CpuFrameMicros
            | Self::GpuFrameMicros
            | Self::PhysicsMicros
            | Self::NavigationMicros
            | Self::AnimationMicros
            | Self::GameplayAiMicros
            | Self::ResidentBytes
            | Self::VramBytes
            | Self::DrawCalls
            | Self::LoadMicros => MetricDirection::LowerIsBetter,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MetricDirection {
    HigherIsBetter,
    LowerIsBetter,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct BenchmarkSceneDescriptor {
    pub id: String,
    pub class: BenchmarkClass,
    pub source_relative_path: String,
    pub source_digest: Option<String>,
    pub deterministic_seed: u64,
    pub workload: BTreeMap<String, u64>,
    pub warmup_frames: u32,
    pub measured_frames: u32,
    pub required_metrics: BTreeSet<MetricId>,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct BenchmarkSuite {
    pub format: String,
    pub suite_id: String,
    pub descriptors: Vec<BenchmarkSceneDescriptor>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct Measurement {
    pub value: u64,
    pub sample_count: u64,
    pub evidence_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct BenchmarkRun {
    pub descriptor_id: String,
    pub descriptor_digest: String,
    pub run_id: String,
    pub host_fingerprint: String,
    pub platform: String,
    pub backend: String,
    pub build_id: String,
    pub authored_tree_hash: String,
    pub measurements: BTreeMap<MetricId, Measurement>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RegressionFloor {
    pub metric: MetricId,
    /// Absolute inclusive floor for higher-is-better metrics.
    pub minimum: Option<u64>,
    /// Absolute inclusive ceiling for lower-is-better metrics.
    pub maximum: Option<u64>,
    /// Maximum relative regression from the baseline, in basis points (10_000 = 100%).
    pub maximum_regression_basis_points: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceState {
    Passed,
    Failed,
    NotMeasured,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct MetricComparison {
    pub metric: MetricId,
    pub state: EvidenceState,
    pub baseline: Option<u64>,
    pub candidate: Option<u64>,
    pub evidence_ids: Vec<String>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RegressionReport {
    pub descriptor_id: String,
    pub state: EvidenceState,
    pub comparisons: Vec<MetricComparison>,
}

impl BenchmarkSuite {
    pub fn validate(&self) -> Result<(), ProductionEvidenceError> {
        require_format(&self.format, BENCHMARK_SUITE_FORMAT)?;
        require_text(&self.suite_id, "suite_id")?;
        if self.descriptors.is_empty() {
            return Err(ProductionEvidenceError::Empty("descriptors"));
        }
        let mut ids = BTreeSet::new();
        for descriptor in &self.descriptors {
            descriptor.validate()?;
            if !ids.insert(descriptor.id.as_str()) {
                return Err(ProductionEvidenceError::Duplicate(descriptor.id.clone()));
            }
        }
        Ok(())
    }
}

impl BenchmarkSceneDescriptor {
    pub fn validate(&self) -> Result<(), ProductionEvidenceError> {
        require_text(&self.id, "descriptor_id")?;
        validate_relative_path(&self.source_relative_path)?;
        if self.source_digest.as_deref().is_some_and(str::is_empty) {
            return Err(ProductionEvidenceError::Empty("source_digest"));
        }
        if self.workload.is_empty() {
            return Err(ProductionEvidenceError::Empty("workload"));
        }
        if self.warmup_frames == 0 || self.measured_frames == 0 {
            return Err(ProductionEvidenceError::InvalidCount("frames"));
        }
        if self.required_metrics.is_empty() {
            return Err(ProductionEvidenceError::Empty("required_metrics"));
        }
        Ok(())
    }
}

impl BenchmarkRun {
    pub fn validate(&self) -> Result<(), ProductionEvidenceError> {
        for (field, value) in [
            ("descriptor_id", self.descriptor_id.as_str()),
            ("descriptor_digest", self.descriptor_digest.as_str()),
            ("run_id", self.run_id.as_str()),
            ("host_fingerprint", self.host_fingerprint.as_str()),
            ("platform", self.platform.as_str()),
            ("backend", self.backend.as_str()),
            ("build_id", self.build_id.as_str()),
            ("authored_tree_hash", self.authored_tree_hash.as_str()),
        ] {
            require_text(value, field)?;
        }
        for measurement in self.measurements.values() {
            if measurement.sample_count == 0 {
                return Err(ProductionEvidenceError::InvalidCount("sample_count"));
            }
            require_text(&measurement.evidence_id, "evidence_id")?;
        }
        Ok(())
    }
}

/// Compare two runs only when they describe the same workload on the same host/backend.
/// A missing metric remains `not_measured`; it never becomes zero or a pass.
pub fn compare_regression(
    floors: &[RegressionFloor],
    baseline: &BenchmarkRun,
    candidate: &BenchmarkRun,
) -> Result<RegressionReport, ProductionEvidenceError> {
    baseline.validate()?;
    candidate.validate()?;
    let compatible = baseline.descriptor_id == candidate.descriptor_id
        && baseline.descriptor_digest == candidate.descriptor_digest
        && baseline.host_fingerprint == candidate.host_fingerprint
        && baseline.platform == candidate.platform
        && baseline.backend == candidate.backend;
    if !compatible {
        return Ok(RegressionReport {
            descriptor_id: candidate.descriptor_id.clone(),
            state: EvidenceState::Blocked,
            comparisons: floors
                .iter()
                .map(|floor| MetricComparison {
                    metric: floor.metric,
                    state: EvidenceState::Blocked,
                    baseline: None,
                    candidate: None,
                    evidence_ids: Vec::new(),
                    reason: Some("baseline and candidate environments are not comparable".into()),
                })
                .collect(),
        });
    }
    let mut seen = BTreeSet::new();
    let mut comparisons = Vec::with_capacity(floors.len());
    for floor in floors {
        if !seen.insert(floor.metric) {
            return Err(ProductionEvidenceError::Duplicate(format!(
                "regression floor {:?}",
                floor.metric
            )));
        }
        if floor.maximum_regression_basis_points > 10_000 {
            return Err(ProductionEvidenceError::InvalidBasisPoints);
        }
        if floor.metric.direction() == MetricDirection::HigherIsBetter && floor.maximum.is_some()
            || floor.metric.direction() == MetricDirection::LowerIsBetter && floor.minimum.is_some()
        {
            return Err(ProductionEvidenceError::WrongFloorDirection(floor.metric));
        }
        let Some(before) = baseline.measurements.get(&floor.metric) else {
            comparisons.push(not_measured(
                floor.metric,
                "baseline measurement is missing",
            ));
            continue;
        };
        let Some(after) = candidate.measurements.get(&floor.metric) else {
            comparisons.push(not_measured(
                floor.metric,
                "candidate measurement is missing",
            ));
            continue;
        };
        let relative_pass = relative_floor_passes(
            floor.metric.direction(),
            before.value,
            after.value,
            floor.maximum_regression_basis_points,
        );
        let absolute_pass = match floor.metric.direction() {
            MetricDirection::HigherIsBetter => floor.minimum.is_none_or(|min| after.value >= min),
            MetricDirection::LowerIsBetter => floor.maximum.is_none_or(|max| after.value <= max),
        };
        let passed = relative_pass && absolute_pass;
        comparisons.push(MetricComparison {
            metric: floor.metric,
            state: if passed {
                EvidenceState::Passed
            } else {
                EvidenceState::Failed
            },
            baseline: Some(before.value),
            candidate: Some(after.value),
            evidence_ids: vec![before.evidence_id.clone(), after.evidence_id.clone()],
            reason: (!passed).then(|| "absolute or relative regression floor failed".into()),
        });
    }
    let state = aggregate_states(comparisons.iter().map(|item| item.state));
    Ok(RegressionReport {
        descriptor_id: candidate.descriptor_id.clone(),
        state,
        comparisons,
    })
}

fn relative_floor_passes(
    direction: MetricDirection,
    baseline: u64,
    candidate: u64,
    tolerance_bps: u16,
) -> bool {
    let base = u128::from(baseline);
    let next = u128::from(candidate);
    let tolerance = u128::from(tolerance_bps);
    match direction {
        MetricDirection::HigherIsBetter => {
            next.saturating_mul(10_000) >= base.saturating_mul(10_000 - tolerance)
        }
        MetricDirection::LowerIsBetter => {
            next.saturating_mul(10_000) <= base.saturating_mul(10_000 + tolerance)
        }
    }
}

fn not_measured(metric: MetricId, reason: &str) -> MetricComparison {
    MetricComparison {
        metric,
        state: EvidenceState::NotMeasured,
        baseline: None,
        candidate: None,
        evidence_ids: Vec::new(),
        reason: Some(reason.to_owned()),
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ResilienceKind {
    Mutation,
    Soak,
    FaultRecovery,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ResilienceResult {
    pub format: String,
    pub case_id: String,
    pub kind: ResilienceKind,
    pub state: EvidenceState,
    pub attempted: bool,
    pub host_fingerprint: Option<String>,
    pub build_id: Option<String>,
    pub authored_tree_hash: Option<String>,
    pub iterations: u64,
    pub duration_micros: u64,
    pub injected_faults: u64,
    pub recovered_faults: u64,
    pub evidence_ids: Vec<String>,
    pub reason: Option<String>,
}

impl ResilienceResult {
    pub fn validate(&self) -> Result<(), ProductionEvidenceError> {
        require_format(&self.format, RESILIENCE_RESULT_FORMAT)?;
        require_text(&self.case_id, "case_id")?;
        if self.recovered_faults > self.injected_faults {
            return Err(ProductionEvidenceError::ImpossibleRecovery);
        }
        match self.state {
            EvidenceState::Passed | EvidenceState::Failed => {
                if !self.attempted || self.iterations == 0 || self.evidence_ids.is_empty() {
                    return Err(ProductionEvidenceError::MissingEvidence(
                        self.case_id.clone(),
                    ));
                }
                for value in [
                    self.host_fingerprint.as_deref(),
                    self.build_id.as_deref(),
                    self.authored_tree_hash.as_deref(),
                ] {
                    if value.is_none_or(|value| value.trim().is_empty()) {
                        return Err(ProductionEvidenceError::MissingEvidence(
                            self.case_id.clone(),
                        ));
                    }
                }
                if self.kind == ResilienceKind::Soak && self.duration_micros == 0 {
                    return Err(ProductionEvidenceError::InvalidCount("duration_micros"));
                }
                if self.kind == ResilienceKind::FaultRecovery && self.injected_faults == 0 {
                    return Err(ProductionEvidenceError::InvalidCount("injected_faults"));
                }
            }
            EvidenceState::NotMeasured => {
                if self.attempted || self.iterations != 0 || !self.evidence_ids.is_empty() {
                    return Err(ProductionEvidenceError::ContradictoryState);
                }
            }
            EvidenceState::Blocked => {
                if self.reason.as_deref().is_none_or(str::is_empty) {
                    return Err(ProductionEvidenceError::Empty("blocked_reason"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Benchmark,
    SerializationContract,
    MigrationContract,
    PublicApiContract,
    DeterministicScene,
    DeterministicPhysics,
    DeterministicGameplay,
    MechanicIntegration,
    ScreenshotBaseline,
    ScreenshotCandidate,
    ScreenshotDiff,
    Mutation,
    Soak,
    FaultRecovery,
    Save,
    Export,
    Launch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct EvidenceArtifact {
    pub id: String,
    pub kind: EvidenceKind,
    pub relative_path: String,
    pub digest: String,
    pub build_id: String,
    pub authored_tree_hash: String,
    pub host_fingerprint: String,
    pub platform: String,
    #[serde(default)]
    pub capability_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ScreenshotComparison {
    pub id: String,
    pub baseline_evidence_id: String,
    pub candidate_evidence_id: String,
    pub diff_evidence_id: String,
    pub changed_pixels: u64,
    pub total_pixels: u64,
    pub maximum_changed_basis_points: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ProductionEvidenceManifest {
    pub format: String,
    pub manifest_id: String,
    pub artifacts: Vec<EvidenceArtifact>,
    #[serde(default)]
    pub screenshot_comparisons: Vec<ScreenshotComparison>,
}

impl ProductionEvidenceManifest {
    pub fn validate(&self) -> Result<(), ProductionEvidenceError> {
        require_format(&self.format, EVIDENCE_MANIFEST_FORMAT)?;
        require_text(&self.manifest_id, "manifest_id")?;
        let mut ids = BTreeSet::new();
        let mut kinds = BTreeMap::new();
        for artifact in &self.artifacts {
            for (field, value) in [
                ("artifact_id", artifact.id.as_str()),
                ("digest", artifact.digest.as_str()),
                ("build_id", artifact.build_id.as_str()),
                ("authored_tree_hash", artifact.authored_tree_hash.as_str()),
                ("host_fingerprint", artifact.host_fingerprint.as_str()),
                ("platform", artifact.platform.as_str()),
            ] {
                require_text(value, field)?;
            }
            validate_relative_path(&artifact.relative_path)?;
            if !ids.insert(artifact.id.as_str()) {
                return Err(ProductionEvidenceError::Duplicate(artifact.id.clone()));
            }
            kinds.insert(artifact.id.as_str(), artifact.kind);
        }
        let mut comparisons = BTreeSet::new();
        for comparison in &self.screenshot_comparisons {
            if !comparisons.insert(comparison.id.as_str()) {
                return Err(ProductionEvidenceError::Duplicate(comparison.id.clone()));
            }
            if comparison.total_pixels == 0
                || comparison.changed_pixels > comparison.total_pixels
                || comparison.maximum_changed_basis_points > 10_000
            {
                return Err(ProductionEvidenceError::InvalidScreenshotComparison);
            }
            for (id, expected) in [
                (
                    comparison.baseline_evidence_id.as_str(),
                    EvidenceKind::ScreenshotBaseline,
                ),
                (
                    comparison.candidate_evidence_id.as_str(),
                    EvidenceKind::ScreenshotCandidate,
                ),
                (
                    comparison.diff_evidence_id.as_str(),
                    EvidenceKind::ScreenshotDiff,
                ),
            ] {
                if kinds.get(id) != Some(&expected) {
                    return Err(ProductionEvidenceError::MissingEvidence(id.to_owned()));
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn artifact(&self, id: &str) -> Option<&EvidenceArtifact> {
        self.artifacts.iter().find(|artifact| artifact.id == id)
    }

    pub fn screenshot_state(
        &self,
        comparison: &ScreenshotComparison,
    ) -> Result<EvidenceState, ProductionEvidenceError> {
        self.validate()?;
        let changed = u128::from(comparison.changed_pixels).saturating_mul(10_000);
        let allowed = u128::from(comparison.total_pixels)
            .saturating_mul(u128::from(comparison.maximum_changed_basis_points));
        Ok(if changed <= allowed {
            EvidenceState::Passed
        } else {
            EvidenceState::Failed
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MaturityDimension {
    Documented,
    Implemented,
    Tested,
    EditorAccessible,
    AiAccessible,
    RuntimeProven,
    ProductionReady,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilityEvidenceBinding {
    pub capability_id: String,
    pub dimension: MaturityDimension,
    pub evidence_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct DimensionTruth {
    pub declared: bool,
    pub state: EvidenceState,
    pub evidence_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilityMatrixRow {
    pub capability_id: String,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub dimensions: BTreeMap<MaturityDimension, DimensionTruth>,
    pub declared_platforms: Vec<String>,
    pub proven_platforms: Vec<String>,
    pub budget_evidence: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilityMatrixInput {
    pub format: String,
    pub registry_hash: String,
    pub evidence_manifest_id: String,
    pub rows: Vec<CapabilityMatrixRow>,
}

impl CapabilityMatrixInput {
    pub fn regenerate(
        registry: &CapabilityRegistry,
        evidence: &ProductionEvidenceManifest,
        bindings: &[CapabilityEvidenceBinding],
    ) -> Result<Self, ProductionEvidenceError> {
        evidence.validate()?;
        let mut grouped = BTreeMap::<(String, MaturityDimension), Vec<String>>::new();
        for binding in bindings {
            let entry = registry.describe(&binding.capability_id).ok_or_else(|| {
                ProductionEvidenceError::UnknownCapability(binding.capability_id.clone())
            })?;
            if !dimension_declared(&entry.maturity, binding.dimension) {
                return Err(ProductionEvidenceError::RegistryEvidenceMismatch {
                    capability: binding.capability_id.clone(),
                    dimension: binding.dimension,
                });
            }
            if binding.evidence_ids.is_empty() {
                return Err(ProductionEvidenceError::MissingEvidence(
                    binding.capability_id.clone(),
                ));
            }
            for id in &binding.evidence_ids {
                let artifact = evidence
                    .artifact(id)
                    .ok_or_else(|| ProductionEvidenceError::MissingEvidence(id.clone()))?;
                if !artifact.capability_ids.contains(&binding.capability_id) {
                    return Err(ProductionEvidenceError::EvidenceSubjectMismatch {
                        evidence: id.clone(),
                        capability: binding.capability_id.clone(),
                    });
                }
            }
            let values = grouped
                .entry((binding.capability_id.clone(), binding.dimension))
                .or_default();
            values.extend(binding.evidence_ids.clone());
            values.sort();
            values.dedup();
        }

        let rows = registry
            .entries
            .iter()
            .map(|entry| {
                let dimensions = MaturityDimension::ALL
                    .into_iter()
                    .map(|dimension| {
                        let declared = dimension_declared(&entry.maturity, dimension);
                        let evidence_ids = grouped
                            .get(&(entry.id.clone(), dimension))
                            .cloned()
                            .unwrap_or_default();
                        (
                            dimension,
                            DimensionTruth {
                                declared,
                                state: if declared && !evidence_ids.is_empty() {
                                    EvidenceState::Passed
                                } else {
                                    EvidenceState::NotMeasured
                                },
                                evidence_ids,
                            },
                        )
                    })
                    .collect();
                CapabilityMatrixRow {
                    capability_id: entry.id.clone(),
                    available: entry.available,
                    unavailable_reason: entry.unavailable_reason.clone(),
                    dimensions,
                    declared_platforms: entry.platforms.clone(),
                    proven_platforms: entry.maturity.proven_platforms.clone(),
                    budget_evidence: entry.maturity.budget_evidence.clone(),
                    limitations: entry.limitations.clone(),
                }
            })
            .collect();
        Ok(Self {
            format: CAPABILITY_MATRIX_INPUT_FORMAT.to_owned(),
            registry_hash: registry.hash.clone(),
            evidence_manifest_id: evidence.manifest_id.clone(),
            rows,
        })
    }
}

impl MaturityDimension {
    pub const ALL: [Self; 7] = [
        Self::Documented,
        Self::Implemented,
        Self::Tested,
        Self::EditorAccessible,
        Self::AiAccessible,
        Self::RuntimeProven,
        Self::ProductionReady,
    ];
}

fn dimension_declared(maturity: &CapabilityMaturity, dimension: MaturityDimension) -> bool {
    match dimension {
        MaturityDimension::Documented => maturity.documented,
        MaturityDimension::Implemented => maturity.implemented,
        MaturityDimension::Tested => maturity.tested,
        MaturityDimension::EditorAccessible => maturity.editor_accessible,
        MaturityDimension::AiAccessible => maturity.ai_accessible,
        MaturityDimension::RuntimeProven => maturity.runtime_proven,
        MaturityDimension::ProductionReady => maturity.production_ready,
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ReleaseProofRequest {
    pub format: String,
    pub authored_tree_hash: String,
    pub build_id: String,
    pub platform: String,
    pub required_capabilities: Vec<String>,
    pub required_evidence: BTreeSet<EvidenceKind>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ReleaseProofDecision {
    pub state: EvidenceState,
    pub evidence_ids: Vec<String>,
    pub blockers: Vec<String>,
}

/// Evaluate final-golden inputs. This never creates a golden and cannot pass stale artifacts.
pub fn evaluate_release_proof(
    request: &ReleaseProofRequest,
    matrix: &CapabilityMatrixInput,
    evidence: &ProductionEvidenceManifest,
) -> Result<ReleaseProofDecision, ProductionEvidenceError> {
    require_format(&request.format, RELEASE_PROOF_FORMAT)?;
    evidence.validate()?;
    let mut blockers = Vec::new();
    let mut evidence_ids = Vec::new();
    for capability in &request.required_capabilities {
        let Some(row) = matrix
            .rows
            .iter()
            .find(|row| row.capability_id == *capability)
        else {
            blockers.push(format!("required capability `{capability}` is absent"));
            continue;
        };
        if !row.available {
            blockers.push(format!("required capability `{capability}` is unavailable"));
        }
        for dimension in [
            MaturityDimension::RuntimeProven,
            MaturityDimension::ProductionReady,
        ] {
            if row
                .dimensions
                .get(&dimension)
                .is_none_or(|truth| truth.state != EvidenceState::Passed)
            {
                blockers.push(format!(
                    "required capability `{capability}` lacks {dimension:?} evidence"
                ));
            }
        }
    }
    for kind in &request.required_evidence {
        let matching = evidence.artifacts.iter().filter(|artifact| {
            artifact.kind == *kind
                && artifact.authored_tree_hash == request.authored_tree_hash
                && artifact.build_id == request.build_id
                && artifact.platform == request.platform
        });
        let ids = matching
            .map(|artifact| artifact.id.clone())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            blockers.push(format!("fresh {kind:?} evidence is missing"));
        } else {
            evidence_ids.extend(ids);
        }
    }
    evidence_ids.sort();
    evidence_ids.dedup();
    Ok(ReleaseProofDecision {
        state: if blockers.is_empty() {
            EvidenceState::Passed
        } else {
            EvidenceState::Blocked
        },
        evidence_ids,
        blockers,
    })
}

fn aggregate_states(states: impl IntoIterator<Item = EvidenceState>) -> EvidenceState {
    let states = states.into_iter().collect::<Vec<_>>();
    if states.contains(&EvidenceState::Blocked) {
        EvidenceState::Blocked
    } else if states.contains(&EvidenceState::Failed) {
        EvidenceState::Failed
    } else if states.is_empty() || states.contains(&EvidenceState::NotMeasured) {
        EvidenceState::NotMeasured
    } else {
        EvidenceState::Passed
    }
}

fn require_format(actual: &str, expected: &'static str) -> Result<(), ProductionEvidenceError> {
    if actual == expected {
        Ok(())
    } else {
        Err(ProductionEvidenceError::UnsupportedFormat(
            actual.to_owned(),
        ))
    }
}

fn require_text(value: &str, field: &'static str) -> Result<(), ProductionEvidenceError> {
    if value.trim().is_empty() {
        Err(ProductionEvidenceError::Empty(field))
    } else {
        Ok(())
    }
}

fn validate_relative_path(path: &str) -> Result<(), ProductionEvidenceError> {
    let normalized = path.replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(":/")
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == ".." || part == ".")
    {
        Err(ProductionEvidenceError::UnsafePath(path.to_owned()))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum ProductionEvidenceError {
    #[error("unsupported production-evidence format `{0}`")]
    UnsupportedFormat(String),
    #[error("production-evidence field `{0}` cannot be empty")]
    Empty(&'static str),
    #[error("production-evidence count `{0}` must be greater than zero")]
    InvalidCount(&'static str),
    #[error("duplicate production-evidence id `{0}`")]
    Duplicate(String),
    #[error("evidence path must be relative and traversal-free: `{0}`")]
    UnsafePath(String),
    #[error("basis points must be between 0 and 10000")]
    InvalidBasisPoints,
    #[error("regression floor uses the wrong absolute bound for {0:?}")]
    WrongFloorDirection(MetricId),
    #[error("recovered fault count exceeds injected fault count")]
    ImpossibleRecovery,
    #[error("result state contradicts its measurements")]
    ContradictoryState,
    #[error("required evidence `{0}` is missing")]
    MissingEvidence(String),
    #[error("screenshot comparison counts or threshold are invalid")]
    InvalidScreenshotComparison,
    #[error("capability `{0}` is not in the registry")]
    UnknownCapability(String),
    #[error("registry does not declare {dimension:?} for capability `{capability}`")]
    RegistryEvidenceMismatch {
        capability: String,
        dimension: MaturityDimension,
    },
    #[error("evidence `{evidence}` is not bound to capability `{capability}`")]
    EvidenceSubjectMismatch {
        evidence: String,
        capability: String,
    },
}
