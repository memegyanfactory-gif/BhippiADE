//! Versioned, deterministic quality-baseline artifacts and regression comparison.
//!
//! The baseline never invents a score. It records only dimensions that were measured with a
//! valid `GameQualityEvaluation`, then blocks new blockers, disappearing defect oracles,
//! newly unmeasured required dimensions and material per-case score regressions.

use crate::document::SceneDocument;
use crate::error::{EngineError, Result};
use crate::game_inspector::{self, SemanticSeverity};
use crate::game_quality::{
    GameQualityEvaluation, QualityDimension, QualityMeasurementStatus, QUALITY_RUBRIC,
};
use crate::game_quality_corpus::{
    GameQualityCorpus, GAME_QUALITY_CORPUS_SCHEMA, GAME_QUALITY_CORPUS_SCHEMA_V2,
};
use crate::manifest::parse_manifest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

pub const QUALITY_RUN_SCHEMA: &str = "bhippi-game-quality-run@1";
pub const QUALITY_BASELINE_SCHEMA: &str = "bhippi-game-quality-baseline@1";
pub const QUALITY_COMPARISON_SCHEMA: &str = "bhippi-game-quality-comparison@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QualityRegressionPolicy {
    /// Small deterministic score movement below this many points is tolerated.
    pub maximum_absolute_drop: u8,
    /// Relative score drop in basis points. A regression blocks only when both limits fail.
    pub maximum_regression_basis_points: u16,
}

impl Default for QualityRegressionPolicy {
    fn default() -> Self {
        Self {
            maximum_absolute_drop: 3,
            maximum_regression_basis_points: 500,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GameQualityRun {
    pub schema: String,
    pub corpus_schema: String,
    pub corpus_digest: String,
    pub rubric: String,
    pub cases: Vec<GameQualityRunCase>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GameQualityRunCase {
    pub id: String,
    pub blocker_codes: Vec<String>,
    pub evaluation: GameQualityEvaluation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameQualityBaseline {
    pub schema: String,
    pub corpus_schema: String,
    pub corpus_digest: String,
    pub rubric: String,
    pub policy: QualityRegressionPolicy,
    pub cases: Vec<GameQualityBaselineCase>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameQualityBaselineCase {
    pub id: String,
    /// Exact deterministic defect oracle. Both new and unexpectedly missing blockers fail.
    pub blocker_codes: Vec<String>,
    /// Only evidence-backed measurements become required baseline dimensions.
    pub dimensions: Vec<QualityBaselineDimension>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deterministic_score: Option<u8>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QualityBaselineDimension {
    pub dimension: QualityDimension,
    pub score: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GameQualityComparison {
    pub schema: String,
    pub corpus_digest: String,
    pub baseline_digest: String,
    pub candidate_digest: String,
    pub passed: bool,
    pub cases: Vec<GameQualityCaseComparison>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameQualityCaseComparison {
    pub id: String,
    pub new_blockers: Vec<String>,
    pub missing_expected_blockers: Vec<String>,
    pub newly_unmeasured_dimensions: Vec<QualityDimension>,
    pub regressions: Vec<QualityScoreRegression>,
    pub aggregate_became_unmeasured: bool,
    pub passed: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QualityScoreRegression {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<QualityDimension>,
    pub baseline: u8,
    pub candidate: u8,
    pub absolute_drop: u8,
    pub regression_basis_points: u16,
}

impl GameQualityRun {
    pub fn parse(text: &str) -> Result<Self> {
        let run: Self = serde_json::from_str(text).map_err(|error| {
            baseline_error(
                &format!("invalid quality run: {error}"),
                &format!("Fix the JSON and keep schema {QUALITY_RUN_SCHEMA}."),
            )
        })?;
        run.validate()?;
        Ok(run)
    }

    pub fn validate(&self) -> Result<()> {
        let corpus_schema_ok = self.corpus_schema == GAME_QUALITY_CORPUS_SCHEMA
            || self.corpus_schema == GAME_QUALITY_CORPUS_SCHEMA_V2;
        if self.schema != QUALITY_RUN_SCHEMA
            || !corpus_schema_ok
            || self.rubric != QUALITY_RUBRIC
            || !valid_digest(&self.corpus_digest)
            || self.cases.is_empty()
        {
            return Err(baseline_error(
                "quality run has an incompatible identity or no cases",
                "Use the current run/corpus/rubric schemas and a canonical BLAKE3 corpus digest.",
            ));
        }
        let mut ids = BTreeSet::new();
        for case in &self.cases {
            require_id(&case.id)?;
            if !ids.insert(case.id.as_str()) {
                return Err(baseline_error(
                    &format!("quality run duplicates case {:?}", case.id),
                    "Store each corpus case exactly once in corpus order.",
                ));
            }
            validate_codes(&case.blocker_codes)?;
            case.evaluation.validate()?;
            if case.evaluation.rubric != self.rubric {
                return Err(baseline_error(
                    &format!("quality run case {:?} uses a different rubric", case.id),
                    "Evaluate every case with the run's exact rubric version.",
                ));
            }
        }
        Ok(())
    }

    pub fn validate_against(&self, corpus: &GameQualityCorpus) -> Result<()> {
        self.validate()?;
        corpus.validate()?;
        if self.corpus_digest != corpus_digest(corpus)?
            || self.cases.len() != corpus.cases.len()
            || self
                .cases
                .iter()
                .zip(&corpus.cases)
                .any(|(actual, expected)| actual.id != expected.id)
        {
            return Err(baseline_error(
                "quality run does not match the exact frozen corpus",
                "Regenerate the run from the committed corpus without dropping or reordering cases.",
            ));
        }
        Ok(())
    }

    pub fn dump(&self) -> Result<String> {
        self.validate()?;
        dump(self, "quality run")
    }
}

impl GameQualityBaseline {
    pub fn record(
        corpus: &GameQualityCorpus,
        run: &GameQualityRun,
        policy: QualityRegressionPolicy,
    ) -> Result<Self> {
        run.validate_against(corpus)?;
        validate_policy(policy)?;
        let cases = run
            .cases
            .iter()
            .map(|case| GameQualityBaselineCase {
                id: case.id.clone(),
                blocker_codes: case.blocker_codes.clone(),
                dimensions: case
                    .evaluation
                    .measurements
                    .iter()
                    .filter_map(|measurement| {
                        (measurement.status == QualityMeasurementStatus::Measured).then(|| {
                            measurement.score.map(|score| QualityBaselineDimension {
                                dimension: measurement.dimension,
                                score,
                            })
                        })?
                    })
                    .collect(),
                deterministic_score: case.evaluation.deterministic_score,
            })
            .collect();
        let baseline = Self {
            schema: QUALITY_BASELINE_SCHEMA.to_owned(),
            corpus_schema: run.corpus_schema.clone(),
            corpus_digest: run.corpus_digest.clone(),
            rubric: run.rubric.clone(),
            policy,
            cases,
        };
        baseline.validate_against(corpus)?;
        Ok(baseline)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let baseline: Self = serde_json::from_str(text).map_err(|error| {
            baseline_error(
                &format!("invalid quality baseline: {error}"),
                &format!("Fix the JSON and keep schema {QUALITY_BASELINE_SCHEMA}."),
            )
        })?;
        baseline.validate()?;
        Ok(baseline)
    }

    pub fn validate(&self) -> Result<()> {
        let corpus_schema_ok = self.corpus_schema == GAME_QUALITY_CORPUS_SCHEMA
            || self.corpus_schema == GAME_QUALITY_CORPUS_SCHEMA_V2;
        if self.schema != QUALITY_BASELINE_SCHEMA
            || !corpus_schema_ok
            || self.rubric != QUALITY_RUBRIC
            || !valid_digest(&self.corpus_digest)
            || self.cases.is_empty()
        {
            return Err(baseline_error(
                "quality baseline has an incompatible identity or no cases",
                "Use the current baseline/corpus/rubric schemas and canonical corpus digest.",
            ));
        }
        validate_policy(self.policy)?;
        let mut ids = BTreeSet::new();
        for case in &self.cases {
            require_id(&case.id)?;
            if !ids.insert(case.id.as_str()) {
                return Err(baseline_error(
                    &format!("quality baseline duplicates case {:?}", case.id),
                    "Store each corpus case exactly once in corpus order.",
                ));
            }
            validate_codes(&case.blocker_codes)?;
            let mut previous = None;
            for dimension in &case.dimensions {
                if dimension.score > 100
                    || previous.is_some_and(|value| value >= dimension.dimension)
                {
                    return Err(baseline_error(
                        &format!(
                            "quality baseline case {:?} has invalid dimension floors",
                            case.id
                        ),
                        "Store unique dimensions in rubric order with scores from 0 through 100.",
                    ));
                }
                previous = Some(dimension.dimension);
            }
            if case.deterministic_score.is_some_and(|score| score > 100) {
                return Err(baseline_error(
                    &format!(
                        "quality baseline case {:?} has an invalid aggregate",
                        case.id
                    ),
                    "Use an evidence-derived aggregate from 0 through 100.",
                ));
            }
        }
        Ok(())
    }

    pub fn validate_against(&self, corpus: &GameQualityCorpus) -> Result<()> {
        self.validate()?;
        corpus.validate()?;
        if self.corpus_digest != corpus_digest(corpus)?
            || self.cases.len() != corpus.cases.len()
            || self
                .cases
                .iter()
                .zip(&corpus.cases)
                .any(|(actual, expected)| actual.id != expected.id)
        {
            return Err(baseline_error(
                "quality baseline does not match the exact frozen corpus",
                "Record a new reviewed baseline when the corpus intentionally changes.",
            ));
        }
        Ok(())
    }

    pub fn dump(&self) -> Result<String> {
        self.validate()?;
        dump(self, "quality baseline")
    }
}

impl GameQualityComparison {
    pub fn parse(text: &str) -> Result<Self> {
        let comparison: Self = serde_json::from_str(text).map_err(|error| {
            baseline_error(
                &format!("invalid quality comparison: {error}"),
                &format!("Fix the JSON and keep schema {QUALITY_COMPARISON_SCHEMA}."),
            )
        })?;
        comparison.validate()?;
        Ok(comparison)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != QUALITY_COMPARISON_SCHEMA
            || !valid_digest(&self.corpus_digest)
            || !valid_digest(&self.baseline_digest)
            || !valid_digest(&self.candidate_digest)
            || self.cases.is_empty()
        {
            return Err(baseline_error(
                "quality comparison has an incompatible identity or no cases",
                "Use the current comparison schema and canonical evidence digests.",
            ));
        }
        let mut ids = BTreeSet::new();
        for case in &self.cases {
            require_id(&case.id)?;
            if !ids.insert(case.id.as_str()) {
                return Err(baseline_error(
                    &format!("quality comparison duplicates case {:?}", case.id),
                    "Store each compared corpus case exactly once.",
                ));
            }
            validate_codes(&case.new_blockers)?;
            validate_codes(&case.missing_expected_blockers)?;
            if case
                .new_blockers
                .iter()
                .any(|code| case.missing_expected_blockers.contains(code))
                || !strictly_ordered_dimensions(&case.newly_unmeasured_dimensions)
            {
                return Err(baseline_error(
                    &format!(
                        "quality comparison case {:?} has contradictory or duplicate deltas",
                        case.id
                    ),
                    "Keep blocker sets disjoint and store each unmeasured dimension once in rubric order.",
                ));
            }
            let mut regression_dimensions = BTreeSet::new();
            let mut aggregate_regressions = 0_u8;
            for regression in &case.regressions {
                match regression.dimension {
                    Some(dimension) => {
                        if !regression_dimensions.insert(dimension)
                            || case.newly_unmeasured_dimensions.contains(&dimension)
                        {
                            return Err(baseline_error(
                                &format!(
                                    "quality comparison case {:?} duplicates a dimension delta",
                                    case.id
                                ),
                                "A dimension is either unmeasured or regressed exactly once.",
                            ));
                        }
                    }
                    None => aggregate_regressions = aggregate_regressions.saturating_add(1),
                }
                let expected_drop = regression.baseline.saturating_sub(regression.candidate);
                let expected_basis_points = (u32::from(expected_drop).saturating_mul(10_000)
                    / u32::from(regression.baseline.max(1)))
                    as u16;
                if regression.baseline <= regression.candidate
                    || regression.absolute_drop != expected_drop
                    || regression.regression_basis_points != expected_basis_points
                {
                    return Err(baseline_error(
                        &format!(
                            "quality comparison case {:?} has a forged score delta",
                            case.id
                        ),
                        "Derive every absolute and relative drop from the compared scores.",
                    ));
                }
            }
            if aggregate_regressions > 1
                || (case.aggregate_became_unmeasured && aggregate_regressions != 0)
            {
                return Err(baseline_error(
                    &format!(
                        "quality comparison case {:?} duplicates its aggregate delta",
                        case.id
                    ),
                    "The aggregate is either unmeasured or regressed at most once.",
                ));
            }
            let expected_case_passed = case.new_blockers.is_empty()
                && case.missing_expected_blockers.is_empty()
                && case.newly_unmeasured_dimensions.is_empty()
                && case.regressions.is_empty()
                && !case.aggregate_became_unmeasured;
            if case.passed != expected_case_passed {
                return Err(baseline_error(
                    &format!(
                        "quality comparison case {:?} contradicts its evidence",
                        case.id
                    ),
                    "Derive pass/fail and score drops from the compared artifacts.",
                ));
            }
        }
        if self.passed != self.cases.iter().all(|case| case.passed) {
            return Err(baseline_error(
                "quality comparison outcome contradicts its cases",
                "Pass only when every canonical case passes independently.",
            ));
        }
        Ok(())
    }

    pub fn dump(&self) -> Result<String> {
        self.validate()?;
        dump(self, "quality comparison")
    }
}

/// Compare every case independently. An aggregate can never hide a broken canonical game.
pub fn compare_quality_run(
    corpus: &GameQualityCorpus,
    baseline: &GameQualityBaseline,
    candidate: &GameQualityRun,
) -> Result<GameQualityComparison> {
    baseline.validate_against(corpus)?;
    candidate.validate_against(corpus)?;
    let mut cases = Vec::with_capacity(baseline.cases.len());
    for (expected, actual) in baseline.cases.iter().zip(&candidate.cases) {
        let expected_codes = expected
            .blocker_codes
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let actual_codes = actual
            .blocker_codes
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let new_blockers: Vec<_> = actual_codes.difference(&expected_codes).cloned().collect();
        let missing_expected_blockers: Vec<_> =
            expected_codes.difference(&actual_codes).cloned().collect();
        let mut newly_unmeasured_dimensions = Vec::new();
        let mut regressions = Vec::new();
        for floor in &expected.dimensions {
            let measurement = actual
                .evaluation
                .measurements
                .iter()
                .find(|measurement| measurement.dimension == floor.dimension);
            match measurement {
                Some(measurement) if measurement.status == QualityMeasurementStatus::Measured => {
                    if let Some(score) = measurement.score {
                        if let Some(regression) = material_regression(
                            Some(floor.dimension),
                            floor.score,
                            score,
                            baseline.policy,
                        ) {
                            regressions.push(regression);
                        }
                    } else {
                        newly_unmeasured_dimensions.push(floor.dimension);
                    }
                }
                _ => newly_unmeasured_dimensions.push(floor.dimension),
            }
        }
        let aggregate_became_unmeasured = expected.deterministic_score.is_some()
            && actual.evaluation.deterministic_score.is_none();
        if let (Some(before), Some(after)) = (
            expected.deterministic_score,
            actual.evaluation.deterministic_score,
        ) {
            if let Some(regression) = material_regression(None, before, after, baseline.policy) {
                regressions.push(regression);
            }
        }
        let passed = new_blockers.is_empty()
            && missing_expected_blockers.is_empty()
            && newly_unmeasured_dimensions.is_empty()
            && regressions.is_empty()
            && !aggregate_became_unmeasured;
        cases.push(GameQualityCaseComparison {
            id: expected.id.clone(),
            new_blockers,
            missing_expected_blockers,
            newly_unmeasured_dimensions,
            regressions,
            aggregate_became_unmeasured,
            passed,
        });
    }
    let passed = cases.iter().all(|case| case.passed);
    let comparison = GameQualityComparison {
        schema: QUALITY_COMPARISON_SCHEMA.to_owned(),
        corpus_digest: baseline.corpus_digest.clone(),
        baseline_digest: object_digest(baseline)?,
        candidate_digest: object_digest(candidate)?,
        passed,
        cases,
    };
    Ok(comparison)
}

/// Deterministic CI lane available without a desktop renderer. Visual/runtime dimensions remain
/// explicitly `not_measured`; this function never promotes static structure into a quality score.
pub fn evaluate_static_corpus(
    corpus: &GameQualityCorpus,
    fixture_root: &Path,
) -> Result<GameQualityRun> {
    corpus.verify_at(fixture_root)?;
    let mut cases = Vec::with_capacity(corpus.cases.len());
    let is_v2 = corpus.schema == crate::game_quality_corpus::GAME_QUALITY_CORPUS_SCHEMA_V2;
    let corpus_dir = if is_v2 { "corpus-v2" } else { "corpus-v1" };

    for case in &corpus.cases {
        let authored_root = fixture_root.join(format!("{corpus_dir}/{}/authored", case.id));
        let blocker_codes = if is_v2 {
            let report = crate::godot::gates::check_project(&authored_root, false);
            report
                .blockers
                .into_iter()
                .map(|finding| finding.code)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            let manifest_path = authored_root.join(crate::GAME_MANIFEST_FILE);
            let manifest_text = std::fs::read_to_string(&manifest_path).map_err(|error| {
                baseline_error(
                    &format!("cannot read {}: {error}", manifest_path.display()),
                    "Restore the frozen corpus manifest.",
                )
            })?;
            let manifest = parse_manifest(&manifest_text)?;
            let mut scenes = Vec::new();
            for artifact in &case.authored_files {
                let Some(relative) = artifact.path.split_once("/authored/").map(|(_, path)| path)
                else {
                    continue;
                };
                if !relative.ends_with(".bscn.json") {
                    continue;
                }
                let path = authored_root.join(relative);
                let text = std::fs::read_to_string(&path).map_err(|error| {
                    baseline_error(
                        &format!("cannot read {}: {error}", path.display()),
                        "Restore the frozen corpus scene.",
                    )
                })?;
                scenes.push((relative.to_owned(), SceneDocument::parse(&text)?));
            }
            scenes.sort_by(|left, right| left.0.cmp(&right.0));
            game_inspector::inspect(&manifest, &scenes)
                .into_iter()
                .filter(|finding| finding.severity == SemanticSeverity::Blocker)
                .map(|finding| finding.code)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        };
        cases.push(GameQualityRunCase {
            id: case.id.clone(),
            blocker_codes,
            evaluation: GameQualityEvaluation::from_measurements(Vec::new())?,
        });
    }
    let run = GameQualityRun {
        schema: QUALITY_RUN_SCHEMA.to_owned(),
        corpus_schema: corpus.schema.clone(),
        corpus_digest: corpus_digest(corpus)?,
        rubric: QUALITY_RUBRIC.to_owned(),
        cases,
    };
    run.validate_against(corpus)?;
    Ok(run)
}

#[must_use]
fn material_regression(
    dimension: Option<QualityDimension>,
    baseline: u8,
    candidate: u8,
    policy: QualityRegressionPolicy,
) -> Option<QualityScoreRegression> {
    if candidate >= baseline {
        return None;
    }
    let absolute_drop = baseline - candidate;
    let denominator = u32::from(baseline.max(1));
    let basis_points = u32::from(absolute_drop).saturating_mul(10_000) / denominator;
    let regression_basis_points = u16::try_from(basis_points).unwrap_or(u16::MAX);
    (absolute_drop > policy.maximum_absolute_drop
        && regression_basis_points > policy.maximum_regression_basis_points)
        .then_some(QualityScoreRegression {
            dimension,
            baseline,
            candidate,
            absolute_drop,
            regression_basis_points,
        })
}

fn validate_policy(policy: QualityRegressionPolicy) -> Result<()> {
    if policy.maximum_absolute_drop > 100 || policy.maximum_regression_basis_points > 10_000 {
        Err(baseline_error(
            "quality regression policy is outside its score range",
            "Use an absolute drop from 0 through 100 and relative basis points from 0 through 10000.",
        ))
    } else {
        Ok(())
    }
}

fn validate_codes(codes: &[String]) -> Result<()> {
    let mut sorted = codes.to_vec();
    sorted.sort();
    sorted.dedup();
    if sorted != codes
        || codes.iter().any(|code| {
            code.len() != 10
                || !code.starts_with("BHP-GD-")
                || !code[7..].bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        Err(baseline_error(
            "quality blocker codes are invalid, duplicated or not sorted",
            "Store canonical unique BHP-GD-NNN codes so ordering cannot create a regression.",
        ))
    } else {
        Ok(())
    }
}

fn strictly_ordered_dimensions(dimensions: &[QualityDimension]) -> bool {
    dimensions.windows(2).all(|pair| pair[0] < pair[1])
}

fn require_id(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 80 {
        Err(baseline_error(
            "quality case id is empty or too long",
            "Use the stable corpus case id.",
        ))
    } else {
        Ok(())
    }
}

fn corpus_digest(corpus: &GameQualityCorpus) -> Result<String> {
    Ok(blake3::hash(corpus.dump()?.as_bytes()).to_hex().to_string())
}

fn object_digest<T: Serialize>(value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        baseline_error(
            &format!("cannot encode quality evidence for hashing: {error}"),
            "Report this as an engine bug.",
        )
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn dump<T: Serialize>(value: &T, label: &str) -> Result<String> {
    serde_json::to_string_pretty(value).map_err(|error| {
        baseline_error(
            &format!("cannot serialise {label}: {error}"),
            "Report this as an engine bug.",
        )
    })
}

fn baseline_error(message: &str, hint: &str) -> EngineError {
    EngineError::Schema(message.to_owned(), Some(hint.to_owned()))
}
