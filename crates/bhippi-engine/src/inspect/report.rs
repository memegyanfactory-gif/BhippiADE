//! What one inspection produced, and the only place a health score is computed
//! (ADR-0056 §6, INV-097).
//!
//! Two rules shape everything here.
//!
//! **A score is arithmetic over findings, never a judgement.** A dimension starts at
//! [`INSPECT_HEALTH_MAX`] and every finding subtracts its severity's penalty. There is no
//! model in the loop and no hand-tuned "feels about right" — the same findings always give
//! the same number, which is what makes the number worth watching over time.
//!
//! **An inspector that did not look does not get a score.** Not zero, not a guess from its
//! neighbours: nothing, plus its name in [`HealthReport::incomplete`]. The performance
//! inspector is the one that lives here permanently until a real profile arrives, and that
//! is the point — a fabricated frame time is worse than an empty panel (§3).

use bhippi_types::{health_penalty, InspectorId, Severity, INSPECT_HEALTH_MAX};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::finding::Finding;

/// The schema every report carries, so a stored report from an older Bhippi is refused
/// rather than half-read.
pub const REPORT_SCHEMA: &str = "bhippi-inspect@1";

/// What one inspection was asked to look at.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InspectScope {
    /// Everything: every scene, script and asset under the project root.
    Project,
    /// One scene and what it instances.
    Level { scene: String },
    /// One thing the user has selected — a node, a script, an asset.
    Selection {
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        node: Option<String>,
        #[serde(default)]
        file: Option<String>,
        #[serde(default)]
        asset: Option<String>,
    },
    /// Only the files a caller says changed (§26). The caller owns "what changed" — this
    /// crate never runs git.
    Changes { files: Vec<String> },
}

impl InspectScope {
    /// The line the top bar prints: `Inspector | Current View: Level_Main`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Project => "Entire project".to_owned(),
            Self::Level { scene } => scene.clone(),
            Self::Selection {
                scene,
                node,
                file,
                asset,
            } => node
                .clone()
                .or_else(|| scene.clone())
                .or_else(|| file.clone())
                .or_else(|| asset.clone())
                .unwrap_or_else(|| "Selection".to_owned()),
            Self::Changes { files } => match files.len() {
                1 => "1 changed file".to_owned(),
                count => format!("{count} changed files"),
            },
        }
    }
}

/// How much of its subject an inspector actually saw.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Coverage {
    /// It was not asked to run.
    NotScanned,
    /// It ran and read `items` things.
    Scanned { items: u32 },
    /// It ran but the scan caps stopped it early.
    Partial { items: u32, reason: String },
    /// It cannot answer from files alone and no measurement exists. `how` is the one
    /// action that would produce one.
    NotMeasured { how: String },
}

impl Coverage {
    /// True when this inspector's answer is complete enough to score.
    #[must_use]
    pub const fn scores(&self) -> bool {
        matches!(self, Self::Scanned { .. } | Self::Partial { .. })
    }
}

/// One row of the health panel.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct Dimension {
    pub inspector: InspectorId,
    pub label: String,
    pub coverage: Coverage,
    pub findings: u32,
    /// `None` exactly when [`Coverage::scores`] is false.
    pub score: Option<u32>,
}

/// How many findings of each severity. Every count is a count, not an estimate.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct SeverityCounts {
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
    pub suggestion: u32,
    pub info: u32,
    pub total: u32,
}

impl SeverityCounts {
    #[must_use]
    pub fn of(findings: &[Finding]) -> Self {
        let mut counts = Self::default();
        for finding in findings {
            counts.total += 1;
            match finding.severity {
                Severity::Critical => counts.critical += 1,
                Severity::High => counts.high += 1,
                Severity::Medium => counts.medium += 1,
                Severity::Low => counts.low += 1,
                Severity::Suggestion => counts.suggestion += 1,
                Severity::Info => counts.info += 1,
            }
        }
        counts
    }
}

/// The project's health, computed from findings and nothing else.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct HealthReport {
    /// The mean of the dimensions that actually scanned. `None` when none did.
    pub score: Option<u32>,
    /// True when every inspector scanned. When false the UI must say so beside the score
    /// — a number over half the project is not the project's health.
    pub complete: bool,
    /// The inspectors with no answer, in rail order.
    pub incomplete: Vec<InspectorId>,
    /// The sentence the panel prints beside the score when the picture is not complete
    /// (§10). `None` when every inspector answered. Computed here so the webview does not
    /// have to decide when a score may be shown without a caveat (R3).
    pub incomplete_line: Option<String>,
    pub dimensions: Vec<Dimension>,
    pub counts: SeverityCounts,
}

impl HealthReport {
    /// Score a set of findings against a set of coverages.
    ///
    /// `coverage` is what each inspector reported about its own run; findings are matched
    /// to their inspector. An inspector present in `coverage` with no findings and a
    /// `Scanned` coverage scores [`INSPECT_HEALTH_MAX`] — that is a real answer, not a
    /// missing one.
    #[must_use]
    pub fn compute(findings: &[Finding], coverage: &[(InspectorId, Coverage)]) -> Self {
        let mut dimensions = Vec::with_capacity(InspectorId::ALL.len());
        let mut incomplete = Vec::new();
        let mut scored_total: u32 = 0;
        let mut scored_count: u32 = 0;

        for inspector in InspectorId::ALL {
            let coverage = coverage
                .iter()
                .find(|(id, _)| *id == inspector)
                .map(|(_, coverage)| coverage.clone())
                .unwrap_or(Coverage::NotScanned);
            let mine: Vec<&Finding> = findings
                .iter()
                .filter(|finding| finding.inspector == inspector)
                .collect();
            let score = if coverage.scores() {
                let penalty: u32 = mine
                    .iter()
                    .map(|finding| health_penalty(finding.severity))
                    .sum();
                let score = INSPECT_HEALTH_MAX.saturating_sub(penalty);
                scored_total += score;
                scored_count += 1;
                Some(score)
            } else {
                incomplete.push(inspector);
                None
            };
            dimensions.push(Dimension {
                inspector,
                label: inspector.label().to_owned(),
                coverage,
                findings: u32::try_from(mine.len()).unwrap_or(u32::MAX),
                score,
            });
        }

        // Integer mean, rounded half up. No float anywhere: a health score that drifts by
        // a rounding mode between two builds is not a score anybody can watch.
        let score =
            (scored_count > 0).then(|| (scored_total * 2 + scored_count) / (scored_count * 2));

        let incomplete_line = match incomplete.len() {
            0 => None,
            1 => Some("Project health incomplete. 1 inspector has not yet scanned.".to_owned()),
            count => Some(format!(
                "Project health incomplete. {count} inspectors have not yet scanned."
            )),
        };
        Self {
            score,
            complete: incomplete.is_empty(),
            incomplete,
            incomplete_line,
            dimensions,
            counts: SeverityCounts::of(findings),
        }
    }

    /// The sentence the panel prints when the picture is not complete (§10).
    #[must_use]
    pub fn incomplete_line(&self) -> Option<String> {
        self.incomplete_line.clone()
    }
}

/// One complete inspection.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct InspectionReport {
    pub schema: String,
    pub scope: InspectScope,
    /// The project root as the user sees it.
    pub project: String,
    /// RFC 3339, when the scan started.
    pub started_at: String,
    pub duration_ms: u64,
    /// Worst first, then stable. Two scans of an unchanged project produce the same order.
    pub findings: Vec<Finding>,
    pub health: HealthReport,
    /// What a cap cut off, in the inspector's own words. Empty when nothing was cut.
    pub truncated: Vec<String>,
}

impl InspectionReport {
    #[must_use]
    pub fn findings_for(&self, inspector: InspectorId) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|finding| finding.inspector == inspector)
            .collect()
    }

    /// One finding by id.
    #[must_use]
    pub fn finding(&self, id: &str) -> Option<&Finding> {
        self.findings.iter().find(|finding| finding.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::finding::Location;
    use bhippi_types::FindingStatus;

    fn finding(inspector: InspectorId, severity: Severity, code: &str) -> Finding {
        Finding {
            id: code.to_owned(),
            inspector,
            code: code.to_owned(),
            severity,
            confidence: 90,
            title: code.to_owned(),
            location: Location::file("scripts/x.gd"),
            cause: "c".to_owned(),
            impact: "i".to_owned(),
            recommendation: "r".to_owned(),
            where_label: "scripts/x.gd".to_owned(),
            evidence: Vec::new(),
            fix: None,
            status: FindingStatus::Open,
            actions: Vec::new(),
        }
    }

    fn all_scanned() -> Vec<(InspectorId, Coverage)> {
        InspectorId::ALL
            .into_iter()
            .map(|inspector| (inspector, Coverage::Scanned { items: 1 }))
            .collect()
    }

    #[test]
    fn a_clean_project_that_every_inspector_saw_scores_a_hundred_and_is_complete() {
        let health = HealthReport::compute(&[], &all_scanned());
        assert_eq!(health.score, Some(100));
        assert!(health.complete);
        assert!(health.incomplete_line().is_none());
        assert_eq!(health.counts.total, 0);
    }

    #[test]
    fn an_inspector_that_did_not_look_has_no_score_and_says_so() {
        let mut coverage = all_scanned();
        coverage.retain(|(id, _)| *id != InspectorId::Performance);
        coverage.push((
            InspectorId::Performance,
            Coverage::NotMeasured {
                how: "Run Performance Scan".to_owned(),
            },
        ));
        let health = HealthReport::compute(&[], &coverage);

        let performance = health
            .dimensions
            .iter()
            .find(|dimension| dimension.inspector == InspectorId::Performance)
            .expect("every inspector has a row");
        assert_eq!(performance.score, None);
        assert!(!health.complete);
        assert_eq!(health.incomplete, vec![InspectorId::Performance]);
        assert_eq!(
            health.incomplete_line().as_deref(),
            Some("Project health incomplete. 1 inspector has not yet scanned.")
        );
        // The eight that did look still score, and their number is real.
        assert_eq!(health.score, Some(100));
    }

    #[test]
    fn nothing_scanned_means_no_score_at_all_rather_than_zero() {
        let health = HealthReport::compute(&[], &[]);
        assert_eq!(health.score, None);
        assert_eq!(health.incomplete.len(), InspectorId::ALL.len());
    }

    #[test]
    fn severity_penalties_land_on_the_dimension_that_found_it() {
        let findings = vec![
            finding(InspectorId::Code, Severity::Critical, "BHP-INS-201"),
            finding(InspectorId::Code, Severity::High, "BHP-INS-202"),
        ];
        let health = HealthReport::compute(&findings, &all_scanned());
        let code = health
            .dimensions
            .iter()
            .find(|dimension| dimension.inspector == InspectorId::Code)
            .expect("every inspector has a row");
        // 100 − 40 − 20.
        assert_eq!(code.score, Some(40));
        assert_eq!(code.findings, 2);
        let scene = health
            .dimensions
            .iter()
            .find(|dimension| dimension.inspector == InspectorId::Scene)
            .expect("every inspector has a row");
        assert_eq!(scene.score, Some(100));
        assert_eq!(health.counts.critical, 1);
        assert_eq!(health.counts.high, 1);
        assert_eq!(health.counts.total, 2);
    }

    #[test]
    fn a_dimension_floors_at_zero_rather_than_wrapping() {
        let findings: Vec<Finding> = (0..20)
            .map(|index| {
                finding(
                    InspectorId::Scene,
                    Severity::Critical,
                    &format!("BHP-INS-1{index:02}"),
                )
            })
            .collect();
        let health = HealthReport::compute(&findings, &all_scanned());
        let scene = health
            .dimensions
            .iter()
            .find(|dimension| dimension.inspector == InspectorId::Scene)
            .expect("every inspector has a row");
        assert_eq!(scene.score, Some(0));
    }

    #[test]
    fn the_scope_label_is_what_the_top_bar_prints() {
        assert_eq!(InspectScope::Project.label(), "Entire project");
        assert_eq!(
            InspectScope::Level {
                scene: "scenes/main.tscn".to_owned()
            }
            .label(),
            "scenes/main.tscn"
        );
        assert_eq!(
            InspectScope::Changes {
                files: vec!["a".to_owned(), "b".to_owned()]
            }
            .label(),
            "2 changed files"
        );
    }
}
