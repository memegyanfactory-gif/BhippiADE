//! Pure contracts for safe `/gamedebug --fix` planning and verification.
//!
//! This module does not write a scene. It decides whether a report is fresh enough to justify
//! a repair, whether another attempt is allowed, and whether verified output must be rolled
//! back. The app layer still owns capability approval and the normal journalled transaction.

use crate::action::EngineActionBatch;
use crate::error::{EngineError, Result};
use crate::game_debug::{GameDebugFinding, GameDebugReport, REPORT_SCHEMA};
use crate::game_quality::{GameQualityEvaluation, QualityDimension, QualityMeasurementStatus};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

pub const REPAIR_PLAN_SCHEMA: &str = "bhippi-game-repair-plan@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RepairPolicy {
    /// Supplied by the versioned quality policy. A caller cannot accidentally get an
    /// unbounded loop by omitting it.
    pub max_attempts_per_finding: u8,
}

impl RepairPolicy {
    pub fn validate(self) -> Result<()> {
        if self.max_attempts_per_finding == 0 {
            return Err(repair_error(
                "repair policy max_attempts_per_finding must be at least 1",
                "Set a finite positive attempt cap in the versioned quality policy.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RepairProposal {
    pub finding_codes: Vec<String>,
    pub batch: EngineActionBatch,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RepairPlanItem {
    pub finding_codes: Vec<String>,
    pub patch_hash: String,
    pub batch: EngineActionBatch,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RepairPlan {
    pub schema: String,
    pub based_on_run_id: String,
    pub authored_hash: String,
    pub items: Vec<RepairPlanItem>,
}

impl RepairPlan {
    /// Build a canonical visible plan. Each finding belongs to exactly one coherent batch.
    /// The hash is over the actual typed batch, not model prose.
    pub fn build(
        report: &GameDebugReport,
        current_authored_hash: &str,
        proposals: Vec<RepairProposal>,
        attempts: &[RepairAttempt],
        policy: RepairPolicy,
    ) -> Result<Self> {
        policy.validate()?;
        ensure_report_fresh(report, current_authored_hash)?;
        if proposals.is_empty() {
            return Err(repair_error(
                "a repair plan must contain at least one coherent proposal",
                "Select one or more report findings and propose a labelled action batch.",
            ));
        }
        let available = report
            .findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect::<BTreeSet<_>>();
        let mut assigned = BTreeSet::new();
        let mut items = Vec::with_capacity(proposals.len());
        for mut proposal in proposals {
            proposal.finding_codes.sort();
            proposal.finding_codes.dedup();
            if proposal.finding_codes.is_empty() {
                return Err(repair_error(
                    "a repair proposal has no finding codes",
                    "Group each action batch under at least one code from the report.",
                ));
            }
            if proposal.batch.label.trim().is_empty() || proposal.batch.actions.is_empty() {
                return Err(repair_error(
                    "a repair proposal needs a label and at least one typed engine action",
                    "Create one labelled EngineActionBatch for the coherent repair.",
                ));
            }
            for code in &proposal.finding_codes {
                if !available.contains(code.as_str()) {
                    return Err(repair_error(
                        &format!("repair proposal cites unknown finding {code:?}"),
                        "Use an exact finding code from the fresh game-debug report.",
                    ));
                }
                if !assigned.insert(code.clone()) {
                    return Err(repair_error(
                        &format!("finding {code:?} is assigned to more than one repair batch"),
                        "Keep each finding in one coherent batch so approval and undo remain clear.",
                    ));
                }
            }
            let patch_hash = hash_batch(&proposal.batch)?;
            let guard = guard_candidate(&proposal.finding_codes, &patch_hash, attempts, policy);
            if let RepairGuardDecision::Stop { reason, codes } = guard {
                return Err(repair_error(
                    &format!("repair convergence guard stopped {codes:?}: {reason:?}"),
                    "Use the best verified state and return the unresolved evidence instead of retrying.",
                ));
            }
            items.push(RepairPlanItem {
                finding_codes: proposal.finding_codes,
                patch_hash,
                batch: proposal.batch,
            });
        }
        items.sort_by(|left, right| left.finding_codes.cmp(&right.finding_codes));
        Ok(Self {
            schema: REPAIR_PLAN_SCHEMA.to_owned(),
            based_on_run_id: report.run_id.clone(),
            authored_hash: current_authored_hash.to_owned(),
            items,
        })
    }
}

/// A completed attempt. `before_hash` and `after_hash` refer to authored state around the
/// approved transaction, while the follow-up diagnostic report itself must remain read-only.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RepairAttempt {
    pub attempt_id: String,
    pub finding_codes: Vec<String>,
    pub patch_hash: String,
    pub before_hash: String,
    pub after_hash: String,
    pub unresolved_finding_codes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RepairStopReason {
    AttemptCap,
    IdenticalPatch,
    NoProgress,
    Oscillation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum RepairGuardDecision {
    Allow,
    Stop {
        reason: RepairStopReason,
        codes: Vec<String>,
    },
}

#[must_use]
pub fn guard_candidate(
    finding_codes: &[String],
    patch_hash: &str,
    attempts: &[RepairAttempt],
    policy: RepairPolicy,
) -> RepairGuardDecision {
    let capped = finding_codes
        .iter()
        .filter(|code| {
            attempts
                .iter()
                .filter(|attempt| attempt.finding_codes.contains(code))
                .count()
                >= usize::from(policy.max_attempts_per_finding)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !capped.is_empty() {
        return RepairGuardDecision::Stop {
            reason: RepairStopReason::AttemptCap,
            codes: capped,
        };
    }
    let repeated = finding_codes
        .iter()
        .filter(|code| {
            attempts.iter().any(|attempt| {
                attempt.patch_hash == patch_hash && attempt.finding_codes.contains(code)
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    if !repeated.is_empty() {
        return RepairGuardDecision::Stop {
            reason: RepairStopReason::IdenticalPatch,
            codes: repeated,
        };
    }
    RepairGuardDecision::Allow
}

/// Assess a just-verified attempt. An authored state seen before means the loop is cycling;
/// an unchanged state means the patch made no progress. Reaching the cap only stops findings
/// that the new report still says are unresolved.
#[must_use]
pub fn assess_attempt(
    previous: &[RepairAttempt],
    current: &RepairAttempt,
    policy: RepairPolicy,
) -> RepairGuardDecision {
    if current.after_hash == current.before_hash {
        return RepairGuardDecision::Stop {
            reason: RepairStopReason::NoProgress,
            codes: current.unresolved_finding_codes.clone(),
        };
    }
    if previous.iter().any(|attempt| {
        attempt.before_hash == current.after_hash || attempt.after_hash == current.after_hash
    }) {
        return RepairGuardDecision::Stop {
            reason: RepairStopReason::Oscillation,
            codes: current.unresolved_finding_codes.clone(),
        };
    }
    let capped = current
        .unresolved_finding_codes
        .iter()
        .filter(|code| {
            previous
                .iter()
                .filter(|attempt| attempt.finding_codes.contains(code))
                .count()
                .saturating_add(1)
                >= usize::from(policy.max_attempts_per_finding)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !capped.is_empty() {
        return RepairGuardDecision::Stop {
            reason: RepairStopReason::AttemptCap,
            codes: capped,
        };
    }
    RepairGuardDecision::Allow
}

pub fn ensure_report_fresh(report: &GameDebugReport, current_authored_hash: &str) -> Result<()> {
    if report.schema != REPORT_SCHEMA {
        return Err(repair_error(
            &format!("unsupported game-debug report schema {:?}", report.schema),
            &format!("Run /gamedebug again to produce {REPORT_SCHEMA}."),
        ));
    }
    if !report.authored_tree_unchanged() {
        return Err(repair_error(
            "the diagnostic report mutated authored state while it was running",
            "Discard this report and investigate the diagnostic write before repairing anything.",
        ));
    }
    if report.authored_tree_after != current_authored_hash {
        return Err(repair_error(
            "the game-debug report is stale for the current authored tree",
            "Run /gamedebug again and plan repairs only from the new authored hash.",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FindingChangeKind {
    Resolved,
    New,
    Persisting,
    Regressed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct FindingChange {
    pub code: String,
    pub kind: FindingChangeKind,
    pub before_severity: Option<String>,
    pub after_severity: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DimensionChangeKind {
    Comparable,
    NewlyMeasured,
    NewlyUnmeasured,
    NotMeasured,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct DimensionChange {
    pub dimension: QualityDimension,
    pub kind: DimensionChangeKind,
    pub before_score: Option<u8>,
    pub after_score: Option<u8>,
    pub delta: Option<i16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum RepairVerificationDecision {
    Keep,
    RollBack {
        restore_authored_hash: String,
        blocker_codes: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RepairComparison {
    pub before_run_id: String,
    pub after_run_id: String,
    pub transaction_ids: Vec<String>,
    pub findings: Vec<FindingChange>,
    pub dimensions: Vec<DimensionChange>,
    pub decision: RepairVerificationDecision,
}

pub fn compare_before_after(
    before: &GameDebugReport,
    after: &GameDebugReport,
    transaction_ids: Vec<String>,
    before_quality: Option<&GameQualityEvaluation>,
    after_quality: Option<&GameQualityEvaluation>,
) -> Result<RepairComparison> {
    validate_report_pair(before, after)?;
    if transaction_ids.is_empty()
        || transaction_ids.iter().any(|id| id.trim().is_empty())
        || transaction_ids.iter().collect::<BTreeSet<_>>().len() != transaction_ids.len()
    {
        return Err(repair_error(
            "before/after comparison needs unique non-empty transaction ids",
            "Record each normal journalled repair transaction exactly once.",
        ));
    }

    let before_by_code = findings_by_code(&before.findings)?;
    let after_by_code = findings_by_code(&after.findings)?;
    let all_codes = before_by_code
        .keys()
        .chain(after_by_code.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut finding_changes = Vec::with_capacity(all_codes.len());
    let mut blocker_regressions = Vec::new();
    for code in all_codes {
        let before_finding = before_by_code.get(code).copied();
        let after_finding = after_by_code.get(code).copied();
        let kind = match (before_finding, after_finding) {
            (Some(_), None) => FindingChangeKind::Resolved,
            (None, Some(_)) => FindingChangeKind::New,
            (Some(left), Some(right))
                if severity_rank(&right.severity) > severity_rank(&left.severity) =>
            {
                FindingChangeKind::Regressed
            }
            (Some(_), Some(_)) => FindingChangeKind::Persisting,
            (None, None) => continue,
        };
        if after_finding.is_some_and(|finding| finding.severity == "blocker")
            && before_finding.is_none_or(|finding| finding.severity != "blocker")
        {
            blocker_regressions.push(code.to_owned());
        }
        finding_changes.push(FindingChange {
            code: code.to_owned(),
            kind,
            before_severity: before_finding.map(|finding| finding.severity.clone()),
            after_severity: after_finding.map(|finding| finding.severity.clone()),
        });
    }

    let dimensions = compare_dimensions(before_quality, after_quality)?;
    let decision = if blocker_regressions.is_empty() {
        RepairVerificationDecision::Keep
    } else {
        RepairVerificationDecision::RollBack {
            restore_authored_hash: before.authored_tree_after.clone(),
            blocker_codes: blocker_regressions,
        }
    };
    Ok(RepairComparison {
        before_run_id: before.run_id.clone(),
        after_run_id: after.run_id.clone(),
        transaction_ids,
        findings: finding_changes,
        dimensions,
        decision,
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct VerifiedRepairState {
    pub report_run_id: String,
    pub authored_hash: String,
    pub blocker_count: u32,
    pub finding_count: u32,
    pub deterministic_quality_score: Option<u8>,
}

/// Best means fewer blockers, then fewer total findings, then a higher complete deterministic
/// score. This is stable and never uses an optional model critique.
#[must_use]
pub fn best_verified_state(states: &[VerifiedRepairState]) -> Option<&VerifiedRepairState> {
    states
        .iter()
        .min_by(|left, right| compare_state(left, right))
}

fn compare_state(left: &VerifiedRepairState, right: &VerifiedRepairState) -> Ordering {
    left.blocker_count
        .cmp(&right.blocker_count)
        .then_with(|| left.finding_count.cmp(&right.finding_count))
        .then_with(|| {
            right
                .deterministic_quality_score
                .cmp(&left.deterministic_quality_score)
        })
        .then_with(|| left.report_run_id.cmp(&right.report_run_id))
}

fn compare_dimensions(
    before: Option<&GameQualityEvaluation>,
    after: Option<&GameQualityEvaluation>,
) -> Result<Vec<DimensionChange>> {
    match (before, after) {
        (Some(before), Some(after)) => {
            before.validate()?;
            after.validate()?;
            Ok(before
                .measurements
                .iter()
                .zip(&after.measurements)
                .map(|(left, right)| {
                    let (kind, delta) = match (left.status, right.status) {
                        (
                            QualityMeasurementStatus::Measured,
                            QualityMeasurementStatus::Measured,
                        ) => (
                            DimensionChangeKind::Comparable,
                            left.score
                                .zip(right.score)
                                .map(|(a, b)| i16::from(b) - i16::from(a)),
                        ),
                        (
                            QualityMeasurementStatus::NotMeasured,
                            QualityMeasurementStatus::Measured,
                        ) => (DimensionChangeKind::NewlyMeasured, None),
                        (
                            QualityMeasurementStatus::Measured,
                            QualityMeasurementStatus::NotMeasured,
                        ) => (DimensionChangeKind::NewlyUnmeasured, None),
                        (
                            QualityMeasurementStatus::NotMeasured,
                            QualityMeasurementStatus::NotMeasured,
                        ) => (DimensionChangeKind::NotMeasured, None),
                    };
                    DimensionChange {
                        dimension: left.dimension,
                        kind,
                        before_score: left.score,
                        after_score: right.score,
                        delta,
                    }
                })
                .collect())
        }
        _ => Ok(Vec::new()),
    }
}

fn validate_report_pair(before: &GameDebugReport, after: &GameDebugReport) -> Result<()> {
    if before.schema != REPORT_SCHEMA || after.schema != REPORT_SCHEMA {
        return Err(repair_error(
            "before/after reports use an unsupported schema",
            "Re-run both diagnostics with the current game-debug schema.",
        ));
    }
    if before.project != after.project || before.mode != after.mode {
        return Err(repair_error(
            "before/after reports are not for the same project and mode",
            "Compare two runs of the same diagnostic mode on the same project.",
        ));
    }
    if !before.authored_tree_unchanged() || !after.authored_tree_unchanged() {
        return Err(repair_error(
            "a diagnostic report changed authored state",
            "Discard the report; diagnostics must remain read-only around every repair.",
        ));
    }
    Ok(())
}

fn findings_by_code(findings: &[GameDebugFinding]) -> Result<BTreeMap<&str, &GameDebugFinding>> {
    let mut by_code = BTreeMap::new();
    for finding in findings {
        if by_code.insert(finding.code.as_str(), finding).is_some() {
            return Err(repair_error(
                &format!("report contains duplicate finding code {:?}", finding.code),
                "Keep finding codes unique within a diagnostic report.",
            ));
        }
    }
    Ok(by_code)
}

fn hash_batch(batch: &EngineActionBatch) -> Result<String> {
    let bytes = serde_json::to_vec(batch).map_err(|error| {
        repair_error(
            &format!("cannot hash repair action batch: {error}"),
            "Report this as an engine serialisation bug.",
        )
    })?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "blocker" => 3,
        "warning" => 2,
        _ => 1,
    }
}

fn repair_error(message: &str, hint: &str) -> EngineError {
    EngineError::Gate(message.to_owned(), Some(hint.to_owned()))
}
