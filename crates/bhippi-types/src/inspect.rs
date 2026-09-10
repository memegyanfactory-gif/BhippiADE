//! Numbers and vocabulary for the Inspector Agents layer (ADR-0056).
//!
//! An inspector *observes*, *diagnoses* and *recommends*. It never executes. Everything in
//! this file is either a word the four stages share or a number that shapes what an
//! inspector will say — the scan caps, the confidence floor below which a guess is not
//! reported at all, and the severity weights the project-health score is computed from.
//! They live here rather than inline so a score can never be two different formulas in two
//! different screens (R11).

use serde::{Deserialize, Serialize};
use specta::Type;

// ── scan caps ────────────────────────────────────────────────────────────────────────

/// The most findings one report carries. A project with more than this does not need a
/// longer list; it needs the top of the one it has. A capped report says it was capped.
pub const INSPECT_MAX_FINDINGS: usize = 400;

/// The most scenes one scan parses. Above this the scan reports partial coverage rather
/// than blocking the UI for a minute.
pub const INSPECT_MAX_SCENES: usize = 600;

/// The most scripts one scan reads.
pub const INSPECT_MAX_SCRIPTS: usize = 1_500;

/// The largest script the code inspector reads in full. A generated file above this is
/// noted, not parsed line by line.
pub const INSPECT_MAX_SCRIPT_BYTES: u64 = 512 * 1024;

/// The largest file a scan hashes for duplicate detection. Bigger files are left
/// unhashed rather than partially hashed: half a hash finds duplicates that are not.
pub const INSPECT_MAX_HASH_BYTES: u64 = 16 * 1024 * 1024;

/// The most evidence lines one finding carries. Evidence is a citation, not a transcript.
pub const INSPECT_MAX_EVIDENCE: usize = 8;

/// The most typed actions one proposed fix may contain. A fix bigger than this is a task
/// for an agent (§18), not a one-click repair.
pub const INSPECT_MAX_FIX_ACTIONS: usize = 12;

// ── confidence ───────────────────────────────────────────────────────────────────────

/// Below this, an inspector has a suspicion rather than a finding, and says nothing. The
/// floor exists so "97% confident" stays meaningful: everything reported is above it.
pub const INSPECT_MIN_CONFIDENCE: u8 = 50;

/// The confidence a purely structural fact carries — the file is not on disk, the node
/// type is what it is. There is nothing to be uncertain about.
pub const INSPECT_CONFIDENCE_CERTAIN: u8 = 100;

/// The confidence a heuristic over authored structure carries: the shape is a strong
/// indicator but a project may have a reason.
pub const INSPECT_CONFIDENCE_HEURISTIC: u8 = 75;

// ── health scoring ───────────────────────────────────────────────────────────────────

/// A dimension with nothing wrong scores this.
pub const INSPECT_HEALTH_MAX: u32 = 100;

/// What one finding of each severity costs its dimension's score. Subtractive and
/// saturating: a dimension floors at zero rather than going negative.
#[must_use]
pub const fn health_penalty(severity: Severity) -> u32 {
    match severity {
        Severity::Critical => 40,
        Severity::High => 20,
        Severity::Medium => 8,
        Severity::Low => 3,
        Severity::Suggestion => 1,
        Severity::Info => 0,
    }
}

// ── static thresholds the asset and animation inspectors measure against ──────────────

/// A texture file above this is worth a look on its own; it is a fact about the file, not
/// a profiler measurement.
pub const INSPECT_TEXTURE_LARGE_BYTES: u64 = 8 * 1024 * 1024;

/// A single mesh/scene asset file above this is worth a look.
pub const INSPECT_MESH_LARGE_BYTES: u64 = 32 * 1024 * 1024;

/// A texture whose longest side exceeds this is oversized for anything but a skybox.
pub const INSPECT_TEXTURE_LARGE_PIXELS: u32 = 4_096;

/// An `AnimationPlayer` blend time under this pops. Godot's own default for a new
/// transition is 0.0, which is exactly the case worth reporting.
pub const INSPECT_BLEND_TIME_FLOOR: f64 = 0.15;

/// A `Control` whose contrast ratio against its background is under this fails the
/// accessibility floor the studio holds itself to (INV-034).
pub const INSPECT_CONTRAST_FLOOR: f64 = 4.5;

// ── vocabulary ───────────────────────────────────────────────────────────────────────

/// Which specialist found it. The rail is this list, in this order.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum InspectorId {
    /// Scenes, nodes, transforms, lighting, cameras, instancing.
    Scene,
    /// Scripts: the project's own source, its symbols and its wiring.
    Code,
    /// Reachability: can the game actually be played through?
    Gameplay,
    /// Files on disk: size, references, duplication, licence, LODs.
    Asset,
    /// Measured cost. Never estimated — see [`InspectorId::Performance`] callers.
    Performance,
    /// `Control` trees: HUD, menus, spacing, contrast, reachability.
    Ui,
    /// `AnimationPlayer`, `AnimationTree`, state machines, blend times.
    Animation,
    /// Collision shapes, bodies, layers and masks.
    Physics,
    /// Navigation, agents and the decision graphs behind them.
    Ai,
}

impl InspectorId {
    /// Every inspector, in rail order.
    pub const ALL: [Self; 9] = [
        Self::Scene,
        Self::Code,
        Self::Gameplay,
        Self::Asset,
        Self::Performance,
        Self::Ui,
        Self::Animation,
        Self::Physics,
        Self::Ai,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scene => "scene",
            Self::Code => "code",
            Self::Gameplay => "gameplay",
            Self::Asset => "asset",
            Self::Performance => "performance",
            Self::Ui => "ui",
            Self::Animation => "animation",
            Self::Physics => "physics",
            Self::Ai => "ai",
        }
    }

    /// The name the rail shows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Scene => "Scene",
            Self::Code => "Code",
            Self::Gameplay => "Gameplay",
            Self::Asset => "Assets",
            Self::Performance => "Performance",
            Self::Ui => "UI",
            Self::Animation => "Animation",
            Self::Physics => "Physics",
            Self::Ai => "AI",
        }
    }

    /// Parsed from a `/inspect <word>` argument or a rail id.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        let word = word.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|inspector| {
                inspector.as_str() == word
                    || inspector.label().eq_ignore_ascii_case(&word)
                    // The two plurals a person types.
                    || (matches!(inspector, Self::Asset) && word == "assets")
                    || (matches!(inspector, Self::Scene) && word == "scenes")
            })
            .or(match word.as_str() {
                "npc" | "ai/npc" => Some(Self::Ai),
                "perf" => Some(Self::Performance),
                "script" | "scripts" => Some(Self::Code),
                _ => None,
            })
    }
}

/// How bad it is. Ordered worst-first so a sort by severity is a sort by this.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The game is broken or will not build.
    Critical,
    /// A player will hit this.
    High,
    /// Worth fixing before release.
    Medium,
    /// Worth fixing eventually.
    Low,
    /// An improvement, not a defect.
    Suggestion,
    /// A fact worth surfacing that is not a problem at all.
    Info,
}

impl Severity {
    pub const ALL: [Self; 6] = [
        Self::Critical,
        Self::High,
        Self::Medium,
        Self::Low,
        Self::Suggestion,
        Self::Info,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Suggestion => "suggestion",
            Self::Info => "info",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
            Self::Suggestion => "Suggestion",
            Self::Info => "Info",
        }
    }
}

/// How dangerous applying a proposed fix is. It decides nothing on its own — every fix
/// waits for the same explicit approval (INV-096) — but the user deserves to know.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum FixRisk {
    /// One property or one connection, reversible by the engine's own undo.
    Low,
    /// Several nodes or a script body.
    Medium,
    /// Touches project settings, deletes something, or spans files.
    High,
}

impl FixRisk {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Where a finding stands. `Resolved` is written by the reconcile step when a later scan
/// no longer sees it; `Ignored` only ever by the user.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    #[default]
    Open,
    Resolved,
    Ignored,
}

impl FindingStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::Ignored => "ignored",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_orders_worst_first_so_a_plain_sort_is_a_triage() {
        let mut severities = vec![Severity::Low, Severity::Critical, Severity::Medium];
        severities.sort();
        assert_eq!(
            severities,
            vec![Severity::Critical, Severity::Medium, Severity::Low]
        );
    }

    #[test]
    fn info_costs_a_dimension_nothing_and_critical_costs_the_most() {
        assert_eq!(health_penalty(Severity::Info), 0);
        let worst = health_penalty(Severity::Critical);
        for severity in Severity::ALL {
            assert!(health_penalty(severity) <= worst);
        }
    }

    #[test]
    fn every_inspector_round_trips_through_its_own_word_and_its_own_label() {
        for inspector in InspectorId::ALL {
            assert_eq!(InspectorId::parse(inspector.as_str()), Some(inspector));
            assert_eq!(InspectorId::parse(inspector.label()), Some(inspector));
        }
        assert_eq!(InspectorId::parse("assets"), Some(InspectorId::Asset));
        assert_eq!(InspectorId::parse("perf"), Some(InspectorId::Performance));
        assert_eq!(InspectorId::parse("npc"), Some(InspectorId::Ai));
        assert_eq!(InspectorId::parse("nonsense"), None);
    }

    /// The three confidences have to stay ordered: a "certain" finding that scored below a
    /// "heuristic" one would make the number in the drawer meaningless. Read through
    /// locals so the comparison is a real one at run time rather than a constant clippy
    /// folds away.
    #[test]
    fn the_confidence_floor_sits_under_both_named_confidences() {
        let floor: u8 = INSPECT_MIN_CONFIDENCE;
        let heuristic: u8 = INSPECT_CONFIDENCE_HEURISTIC;
        let certain: u8 = INSPECT_CONFIDENCE_CERTAIN;
        assert!(floor < heuristic, "{floor} is not under {heuristic}");
        assert!(heuristic < certain, "{heuristic} is not under {certain}");
        assert_eq!(certain, 100);
    }
}
