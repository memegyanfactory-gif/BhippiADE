//! Evidence-backed game quality rubric.
//!
//! Scores cannot exist without evidence. Missing observations become `not_measured`, and
//! optional model critique is kept outside the deterministic score used by gates and CI.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const QUALITY_EVALUATION_SCHEMA: &str = "bhippi-game-quality@1";
pub const QUALITY_RUBRIC: &str = "bhippi-game-quality-rubric@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum QualityDimension {
    Bootability,
    GoalClarity,
    ControlCorrectness,
    ProgressionFinishability,
    FailureRecovery,
    RuntimeStability,
    VisualLegibility,
    HudFeedback,
    ContentCoherence,
    Performance,
}

impl QualityDimension {
    #[must_use]
    pub const fn all() -> [Self; 10] {
        [
            Self::Bootability,
            Self::GoalClarity,
            Self::ControlCorrectness,
            Self::ProgressionFinishability,
            Self::FailureRecovery,
            Self::RuntimeStability,
            Self::VisualLegibility,
            Self::HudFeedback,
            Self::ContentCoherence,
            Self::Performance,
        ]
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum QualityEvidenceKind {
    Finding,
    ScenarioAssertion,
    RuntimeMetric,
    Observation,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
pub struct QualityEvidence {
    pub kind: QualityEvidenceKind,
    pub address: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum QualityMeasurementStatus {
    Measured,
    NotMeasured,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct QualityMeasurement {
    pub dimension: QualityDimension,
    pub status: QualityMeasurementStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<QualityEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl QualityMeasurement {
    pub fn measured(
        dimension: QualityDimension,
        score: u8,
        confidence: f32,
        evidence: Vec<QualityEvidence>,
    ) -> Result<Self> {
        let measurement = Self {
            dimension,
            status: QualityMeasurementStatus::Measured,
            score: Some(score),
            confidence: Some(confidence),
            evidence,
            reason: None,
        };
        measurement.validate()?;
        Ok(measurement)
    }

    #[must_use]
    pub fn not_measured(dimension: QualityDimension, reason: impl Into<String>) -> Self {
        Self {
            dimension,
            status: QualityMeasurementStatus::NotMeasured,
            score: None,
            confidence: None,
            evidence: Vec::new(),
            reason: Some(reason.into()),
        }
    }

    fn validate(&self) -> Result<()> {
        match self.status {
            QualityMeasurementStatus::Measured => {
                let score = self.score.ok_or_else(|| {
                    quality_error(
                        "a measured quality dimension has no score",
                        "Supply a 0..=100 score derived from the cited evidence.",
                    )
                })?;
                if score > 100 {
                    return Err(quality_error(
                        &format!("quality score {score} is above 100"),
                        "Use a score from 0 through 100.",
                    ));
                }
                let confidence = self.confidence.ok_or_else(|| {
                    quality_error(
                        "a measured quality dimension has no confidence",
                        "Supply a finite confidence from 0 through 1.",
                    )
                })?;
                if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
                    return Err(quality_error(
                        &format!("quality confidence {confidence} is outside 0..=1"),
                        "Use a finite confidence from 0 through 1.",
                    ));
                }
                if self.evidence.is_empty() {
                    return Err(quality_error(
                        "a measured quality dimension has no evidence",
                        "Cite a finding, scenario assertion, runtime metric or captured observation.",
                    ));
                }
                if self.reason.is_some() {
                    return Err(quality_error(
                        "a measured quality dimension cannot also have a missing-evidence reason",
                        "Remove reason, or mark the dimension not_measured.",
                    ));
                }
                validate_evidence(&self.evidence)
            }
            QualityMeasurementStatus::NotMeasured => {
                if self.score.is_some() || self.confidence.is_some() || !self.evidence.is_empty() {
                    return Err(quality_error(
                        "a not_measured dimension cannot carry a score, confidence or evidence",
                        "Either provide complete evidence and mark it measured, or remove the guessed fields.",
                    ));
                }
                if self
                    .reason
                    .as_deref()
                    .is_none_or(|reason| reason.trim().is_empty())
                {
                    return Err(quality_error(
                        "a not_measured dimension needs a reason",
                        "State which observation or runtime evidence is missing.",
                    ));
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MultimodalCritique {
    pub model: String,
    pub model_version: String,
    pub summary: String,
    pub evidence: Vec<QualityEvidence>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GameQualityEvaluation {
    pub schema: String,
    pub rubric: String,
    pub measurements: Vec<QualityMeasurement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deterministic_score: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multimodal_critique: Option<MultimodalCritique>,
}

impl GameQualityEvaluation {
    /// Build canonical rubric output. Unobserved dimensions are inserted as `not_measured`,
    /// preventing a caller from silently shrinking the rubric to the checks it passed.
    pub fn from_measurements(measurements: Vec<QualityMeasurement>) -> Result<Self> {
        let mut by_dimension = BTreeMap::new();
        for mut measurement in measurements {
            measurement.evidence.sort();
            measurement.validate()?;
            let dimension = measurement.dimension;
            if by_dimension.insert(dimension, measurement).is_some() {
                return Err(quality_error(
                    &format!("duplicate quality dimension {dimension:?}"),
                    "Provide exactly one measurement per rubric dimension.",
                ));
            }
        }
        let measurements = QualityDimension::all()
            .into_iter()
            .map(|dimension| {
                by_dimension.remove(&dimension).unwrap_or_else(|| {
                    QualityMeasurement::not_measured(
                        dimension,
                        "No compatible deterministic or captured observation was supplied.",
                    )
                })
            })
            .collect::<Vec<_>>();
        let deterministic_score = complete_score(&measurements);
        Ok(Self {
            schema: QUALITY_EVALUATION_SCHEMA.to_owned(),
            rubric: QUALITY_RUBRIC.to_owned(),
            measurements,
            deterministic_score,
            multimodal_critique: None,
        })
    }

    pub fn parse(text: &str) -> Result<Self> {
        let evaluation: Self = serde_json::from_str(text).map_err(|error| {
            quality_error(
                &format!("invalid game quality evaluation: {error}"),
                &format!("Fix the JSON and keep schema {QUALITY_EVALUATION_SCHEMA}."),
            )
        })?;
        evaluation.validate()?;
        Ok(evaluation)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != QUALITY_EVALUATION_SCHEMA || self.rubric != QUALITY_RUBRIC {
            return Err(quality_error(
                &format!(
                    "unsupported quality schema {:?} or rubric {:?}",
                    self.schema, self.rubric
                ),
                &format!("Use schema {QUALITY_EVALUATION_SCHEMA} with rubric {QUALITY_RUBRIC}."),
            ));
        }
        if self.measurements.len() != QualityDimension::all().len() {
            return Err(quality_error(
                "a quality evaluation must contain every rubric dimension",
                "Build the evaluation with GameQualityEvaluation::from_measurements.",
            ));
        }
        let expected = QualityDimension::all();
        let mut seen = BTreeSet::new();
        for (index, measurement) in self.measurements.iter().enumerate() {
            measurement.validate()?;
            if measurement.dimension != expected[index] || !seen.insert(measurement.dimension) {
                return Err(quality_error(
                    "quality dimensions are missing, duplicated or out of canonical order",
                    "Emit each rubric dimension exactly once in rubric order.",
                ));
            }
            let mut sorted = measurement.evidence.clone();
            sorted.sort();
            if sorted != measurement.evidence {
                return Err(quality_error(
                    "quality evidence is not in canonical order",
                    "Sort evidence by kind, address and summary before serialising.",
                ));
            }
        }
        if self.deterministic_score != complete_score(&self.measurements) {
            return Err(quality_error(
                "deterministic_score does not match the complete evidence-backed rubric",
                "Recompute the score; leave it absent while any dimension is not_measured.",
            ));
        }
        if let Some(critique) = &self.multimodal_critique {
            if critique.model.trim().is_empty()
                || critique.model_version.trim().is_empty()
                || critique.summary.trim().is_empty()
                || critique.evidence.is_empty()
            {
                return Err(quality_error(
                    "multimodal critique is missing provenance, summary or evidence",
                    "Record model, model version, summary and captured evidence, or omit critique.",
                ));
            }
            validate_evidence(&critique.evidence)?;
        }
        Ok(())
    }

    pub fn dump(&self) -> Result<String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| {
            quality_error(
                &format!("cannot serialise game quality evaluation: {error}"),
                "Report this as an engine bug.",
            )
        })
    }
}

fn complete_score(measurements: &[QualityMeasurement]) -> Option<u8> {
    let scores = measurements
        .iter()
        .map(|measurement| measurement.score)
        .collect::<Option<Vec<_>>>()?;
    let total = scores.iter().map(|score| u32::from(*score)).sum::<u32>();
    let count = u32::try_from(scores.len()).ok()?;
    Some(((total + count / 2) / count) as u8)
}

fn validate_evidence(evidence: &[QualityEvidence]) -> Result<()> {
    for item in evidence {
        if item.address.trim().is_empty() || item.summary.trim().is_empty() {
            return Err(quality_error(
                "quality evidence needs an address and observed summary",
                "Point to a finding, scenario assertion, metric or captured artefact.",
            ));
        }
        if item
            .artifact_hash
            .as_deref()
            .is_some_and(|hash| hash.trim().is_empty())
        {
            return Err(quality_error(
                "quality evidence artifact_hash must not be empty",
                "Store the artefact hash, or omit artifact_hash when there is no artefact.",
            ));
        }
    }
    Ok(())
}

fn quality_error(message: &str, hint: &str) -> EngineError {
    EngineError::Schema(message.to_owned(), Some(hint.to_owned()))
}
