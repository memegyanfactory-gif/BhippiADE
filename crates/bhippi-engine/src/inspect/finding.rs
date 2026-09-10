//! What an inspector is allowed to say, and the shape it has to say it in (ADR-0056 §3).
//!
//! A finding is not a sentence. It is six answers — *what*, *where*, *why*, *how sure*,
//! *what happens if this is ignored*, and *what to do* — plus the citations that back them.
//! [`FindingDraft::build`] refuses a draft missing any of the six, and refuses a confidence
//! below [`INSPECT_MIN_CONFIDENCE`], because a finding nobody can act on is worse than
//! silence: it costs the reader the same attention and returns nothing.
//!
//! The actions a finding offers (§16) are **computed from its location**, never authored.
//! A finding that names a script line offers *Open*; one that names a node offers *Locate*
//! and *Focus*. An inspector cannot accidentally offer *Fix* on a finding it has no fix for.

use crate::error::{EngineError, Result};
use bhippi_types::{FindingStatus, InspectorId, Severity, INSPECT_MAX_EVIDENCE};
use serde::{Deserialize, Serialize};
use specta::Type;

use super::fix::ProposedFix;

/// Where the finding is, in every addressing scheme that might apply to it.
///
/// Every field is optional and at least one must be set — a finding with no location is a
/// rumour. The combination decides which actions the drawer offers.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct Location {
    /// Project-relative scene path, forward slashes (`scenes/main.tscn`).
    #[serde(default)]
    pub scene: Option<String>,
    /// A node path inside `scene` (`Door/Area3D`). Meaningless without `scene`.
    #[serde(default)]
    pub node: Option<String>,
    /// Project-relative file path for a code or config finding.
    #[serde(default)]
    pub file: Option<String>,
    /// 1-based line inside `file`.
    #[serde(default)]
    pub line: Option<u32>,
    /// Project-relative asset path.
    #[serde(default)]
    pub asset: Option<String>,
    /// A named thing that is not a path: a group, an input action, a signal.
    #[serde(default)]
    pub symbol: Option<String>,
}

impl Location {
    #[must_use]
    pub fn scene(path: impl Into<String>) -> Self {
        Self {
            scene: Some(path.into()),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn node(scene: impl Into<String>, node: impl Into<String>) -> Self {
        Self {
            scene: Some(scene.into()),
            node: Some(node.into()),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn file(path: impl Into<String>) -> Self {
        Self {
            file: Some(path.into()),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn line(path: impl Into<String>, line: u32) -> Self {
        Self {
            file: Some(path.into()),
            line: Some(line),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn asset(path: impl Into<String>) -> Self {
        Self {
            asset: Some(path.into()),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn symbol(name: impl Into<String>) -> Self {
        Self {
            symbol: Some(name.into()),
            ..Self::default()
        }
    }

    /// Attach a symbol to an existing location.
    #[must_use]
    pub fn with_symbol(mut self, name: impl Into<String>) -> Self {
        self.symbol = Some(name.into());
        self
    }

    /// True when the location addresses nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scene.is_none() && self.file.is_none() && self.asset.is_none() && self.symbol.is_none()
    }

    /// The one line the drawer prints under the title.
    #[must_use]
    pub fn describe(&self) -> String {
        if let (Some(scene), Some(node)) = (&self.scene, &self.node) {
            return format!("{scene} · {node}");
        }
        if let Some(scene) = &self.scene {
            return scene.clone();
        }
        if let (Some(file), Some(line)) = (&self.file, self.line) {
            return format!("{file}:{line}");
        }
        if let Some(file) = &self.file {
            return file.clone();
        }
        if let Some(asset) = &self.asset {
            return asset.clone();
        }
        self.symbol.clone().unwrap_or_default()
    }

    /// The stable address a finding id is derived from.
    fn address(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}",
            self.scene.as_deref().unwrap_or(""),
            self.node.as_deref().unwrap_or(""),
            self.file.as_deref().unwrap_or(""),
            self.line.map(|line| line.to_string()).unwrap_or_default(),
            self.asset.as_deref().unwrap_or(""),
            self.symbol.as_deref().unwrap_or(""),
        )
    }
}

/// One citation: something the inspector actually read, and where it read it.
///
/// Evidence is what makes an answer checkable (§21). `source` is an address a person can
/// open; `claim` is the fact that address supports. Neither may be a paraphrase of the
/// finding's own title — that is a restatement, not evidence.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct Evidence {
    pub claim: String,
    /// `scenes/main.tscn#Door/Area3D`, `scripts/door.gd:42`, `project.godot#input`.
    pub source: String,
}

impl Evidence {
    #[must_use]
    pub fn new(claim: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            claim: claim.into(),
            source: source.into(),
        }
    }
}

/// What the drawer offers on a finding. Computed from the location, never authored (§16).
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum FindingAction {
    /// Open the file at the line.
    Open,
    /// Open the scene in the workspace.
    OpenScene,
    /// Focus the node in the open scene and frame the camera on it.
    Locate,
    /// Reveal the asset in the Assets dock.
    RevealAsset,
    /// Ask the Inspector about this finding, conversationally.
    Ask,
    /// Turn the finding into a normal agent task (§18).
    SendToAgent,
    /// Preview a typed fix. Never applies anything on its own.
    Fix,
    /// Stop reporting it.
    Ignore,
}

impl FindingAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::OpenScene => "open_scene",
            Self::Locate => "locate",
            Self::RevealAsset => "reveal_asset",
            Self::Ask => "ask",
            Self::SendToAgent => "send_to_agent",
            Self::Fix => "fix",
            Self::Ignore => "ignore",
        }
    }
}

/// One thing an inspector found, with everything a person needs to judge it.
///
/// `PartialEq` but not `Eq`: a proposed fix can carry a float property, and a float is not
/// a thing to compare for total equality.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Finding {
    /// Stable across scans: the same problem in the same place keeps the same id, so
    /// "ignored" and "resolved on 10 Sep" survive a rescan (§23).
    pub id: String,
    pub inspector: InspectorId,
    /// A stable `BHP-INS-nnn` code. The UI, the tests and a repair turn key on this.
    pub code: String,
    pub severity: Severity,
    /// 0–100, and never below [`INSPECT_MIN_CONFIDENCE`].
    pub confidence: u8,
    /// WHAT: one line, the problem, not the fix.
    pub title: String,
    /// WHERE.
    pub location: Location,
    /// The one line the drawer prints under the title. Computed here so the webview
    /// formats nothing (R3).
    pub where_label: String,
    /// WHY it is a problem: the mechanism, in the project's own terms.
    pub cause: String,
    /// WHAT HAPPENS if it is ignored.
    pub impact: String,
    /// WHAT TO DO, in words, whether or not a typed fix exists.
    pub recommendation: String,
    /// The citations behind the claim.
    pub evidence: Vec<Evidence>,
    /// A typed, previewable fix. `None` means the repair needs judgement.
    #[serde(default)]
    pub fix: Option<ProposedFix>,
    pub status: FindingStatus,
    /// Computed from `location` and `fix`.
    pub actions: Vec<FindingAction>,
}

impl Finding {
    /// Start a draft. Nothing is a finding until [`FindingDraft::build`] accepts it.
    #[must_use]
    pub fn draft(
        inspector: InspectorId,
        code: impl Into<String>,
        severity: Severity,
        confidence: u8,
        title: impl Into<String>,
        location: Location,
    ) -> FindingDraft {
        FindingDraft {
            inspector,
            code: code.into(),
            severity,
            confidence,
            title: title.into(),
            location,
            cause: String::new(),
            impact: String::new(),
            recommendation: String::new(),
            evidence: Vec::new(),
            fix: None,
        }
    }

    /// Sort key: worst first, then by inspector, then by code, then by address, so two
    /// scans of an unchanged project produce byte-identical reports.
    #[must_use]
    pub fn sort_key(&self) -> (Severity, InspectorId, String, String) {
        (
            self.severity,
            self.inspector,
            self.code.clone(),
            self.location.address(),
        )
    }
}

/// A finding under construction. The six answers are checked at [`FindingDraft::build`].
#[derive(Clone, Debug)]
pub struct FindingDraft {
    inspector: InspectorId,
    code: String,
    severity: Severity,
    confidence: u8,
    title: String,
    location: Location,
    cause: String,
    impact: String,
    recommendation: String,
    evidence: Vec<Evidence>,
    fix: Option<ProposedFix>,
}

impl FindingDraft {
    #[must_use]
    pub fn cause(mut self, text: impl Into<String>) -> Self {
        self.cause = text.into();
        self
    }

    #[must_use]
    pub fn impact(mut self, text: impl Into<String>) -> Self {
        self.impact = text.into();
        self
    }

    #[must_use]
    pub fn recommend(mut self, text: impl Into<String>) -> Self {
        self.recommendation = text.into();
        self
    }

    #[must_use]
    pub fn evidence(mut self, claim: impl Into<String>, source: impl Into<String>) -> Self {
        self.evidence.push(Evidence::new(claim, source));
        self
    }

    #[must_use]
    pub fn fix(mut self, fix: ProposedFix) -> Self {
        self.fix = Some(fix);
        self
    }

    /// Check the six answers and mint the finding.
    ///
    /// # Errors
    /// When any of *why*, *impact* or *recommendation* is blank, when the location
    /// addresses nothing, or when the confidence is below the reporting floor.
    pub fn build(self) -> Result<Finding> {
        let missing = |what: &str| -> EngineError {
            EngineError::Schema(
                format!("inspector finding {} has no {what}", self.code),
                Some(format!(
                    "Every finding answers what, where, why, how sure, what happens if \
                     ignored and what to do. This one is missing {what}."
                )),
            )
        };
        if self.title.trim().is_empty() {
            return Err(missing("title"));
        }
        if self.location.is_empty() {
            return Err(missing("location"));
        }
        if self.cause.trim().is_empty() {
            return Err(missing("cause"));
        }
        if self.impact.trim().is_empty() {
            return Err(missing("impact"));
        }
        if self.recommendation.trim().is_empty() {
            return Err(missing("recommendation"));
        }
        if self.confidence < bhippi_types::INSPECT_MIN_CONFIDENCE {
            return Err(EngineError::Schema(
                format!(
                    "inspector finding {} is only {}% confident",
                    self.code, self.confidence
                ),
                Some(format!(
                    "Below {}% an inspector has a suspicion, not a finding. Gather more \
                     evidence or say nothing.",
                    bhippi_types::INSPECT_MIN_CONFIDENCE
                )),
            ));
        }
        if self.confidence > bhippi_types::INSPECT_CONFIDENCE_CERTAIN {
            return Err(EngineError::Schema(
                format!(
                    "inspector finding {} claims {}%",
                    self.code, self.confidence
                ),
                Some("Confidence is a percentage; 100 is the ceiling.".to_owned()),
            ));
        }

        let mut evidence = self.evidence;
        evidence.truncate(INSPECT_MAX_EVIDENCE);
        let actions = actions_for(&self.location, self.fix.is_some());
        let id = finding_id(self.inspector, &self.code, &self.location);
        Ok(Finding {
            id,
            inspector: self.inspector,
            code: self.code,
            severity: self.severity,
            confidence: self.confidence,
            title: self.title,
            where_label: self.location.describe(),
            location: self.location,
            cause: self.cause,
            impact: self.impact,
            recommendation: self.recommendation,
            evidence,
            fix: self.fix,
            status: FindingStatus::Open,
            actions,
        })
    }
}

/// The stable identity of a problem: which inspector, which check, which address.
///
/// Deliberately **not** derived from the message text — a reworded title must not create a
/// second copy of a finding the user already ignored, and a fixed problem that comes back
/// must come back as the same row.
#[must_use]
pub fn finding_id(inspector: InspectorId, code: &str, location: &Location) -> String {
    let material = format!("{}|{code}|{}", inspector.as_str(), location.address());
    let digest = blake3::hash(material.as_bytes());
    format!("f_{}", &digest.to_hex()[..16])
}

/// Which actions this finding can honestly offer.
#[must_use]
pub fn actions_for(location: &Location, has_fix: bool) -> Vec<FindingAction> {
    let mut actions = Vec::new();
    if location.file.is_some() {
        actions.push(FindingAction::Open);
    }
    if location.scene.is_some() {
        actions.push(FindingAction::OpenScene);
        if location.node.is_some() {
            actions.push(FindingAction::Locate);
        }
    }
    if location.asset.is_some() {
        actions.push(FindingAction::RevealAsset);
    }
    actions.push(FindingAction::Ask);
    actions.push(FindingAction::SendToAgent);
    if has_fix {
        actions.push(FindingAction::Fix);
    }
    actions.push(FindingAction::Ignore);
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhippi_types::INSPECT_MIN_CONFIDENCE;

    fn complete() -> FindingDraft {
        Finding::draft(
            InspectorId::Gameplay,
            "BHP-INS-302",
            Severity::High,
            97,
            "Interaction trigger never fires",
            Location::node("scenes/main.tscn", "Door/Area3D"),
        )
        .cause("body_entered is not connected to anything and the script never connects it")
        .impact("The player cannot open the door")
        .recommend("Connect body_entered to Door._on_body_entered")
        .evidence(
            "Area3D has no [connection] block",
            "scenes/main.tscn#Door/Area3D",
        )
    }

    #[test]
    fn a_finding_missing_any_of_the_six_answers_is_refused() {
        assert!(complete().build().is_ok());

        let no_cause = complete().cause("");
        assert!(no_cause.build().is_err());

        let no_impact = complete().impact("   ");
        assert!(no_impact.build().is_err());

        let no_recommendation = complete().recommend("");
        assert!(no_recommendation.build().is_err());

        let nowhere = Finding::draft(
            InspectorId::Code,
            "BHP-INS-201",
            Severity::Low,
            90,
            "Something",
            Location::default(),
        )
        .cause("c")
        .impact("i")
        .recommend("r");
        assert!(nowhere.build().is_err());
    }

    #[test]
    fn a_guess_under_the_floor_is_not_reportable_and_neither_is_impossible_certainty() {
        let mut draft = complete();
        draft.confidence = INSPECT_MIN_CONFIDENCE - 1;
        assert!(draft.build().is_err());

        let mut floor = complete();
        floor.confidence = INSPECT_MIN_CONFIDENCE;
        assert!(floor.build().is_ok());

        let mut impossible = complete();
        impossible.confidence = 101;
        assert!(impossible.build().is_err());
    }

    #[test]
    fn the_id_is_the_address_and_survives_a_reworded_title() {
        let first = complete().build().expect("draft is complete");
        let mut reworded = complete();
        reworded.title = "The door will not open".to_owned();
        let second = reworded.build().expect("draft is complete");
        assert_eq!(first.id, second.id);

        let mut elsewhere = complete();
        elsewhere.location = Location::node("scenes/main.tscn", "Gate/Area3D");
        let other = elsewhere.build().expect("draft is complete");
        assert_ne!(first.id, other.id);
    }

    #[test]
    fn the_finding_carries_the_line_the_drawer_prints_so_the_webview_formats_nothing() {
        let finding = complete().build().expect("draft is complete");
        assert_eq!(finding.where_label, "scenes/main.tscn · Door/Area3D");
    }

    #[test]
    fn actions_come_from_the_location_and_fix_is_never_offered_without_one() {
        let finding = complete().build().expect("draft is complete");
        assert!(finding.actions.contains(&FindingAction::OpenScene));
        assert!(finding.actions.contains(&FindingAction::Locate));
        assert!(!finding.actions.contains(&FindingAction::Fix));
        assert!(!finding.actions.contains(&FindingAction::Open));

        let code = Finding::draft(
            InspectorId::Code,
            "BHP-INS-204",
            Severity::High,
            80,
            "Per-frame node lookup",
            Location::line("scripts/player.gd", 42),
        )
        .cause("c")
        .impact("i")
        .recommend("r")
        .build()
        .expect("draft is complete");
        assert!(code.actions.contains(&FindingAction::Open));
        assert!(!code.actions.contains(&FindingAction::Locate));
    }

    #[test]
    fn evidence_is_capped_so_a_citation_never_becomes_a_transcript() {
        let mut draft = complete();
        for index in 0..40 {
            draft = draft.evidence(format!("claim {index}"), "scenes/main.tscn");
        }
        let finding = draft.build().expect("draft is complete");
        assert_eq!(finding.evidence.len(), INSPECT_MAX_EVIDENCE);
    }
}
