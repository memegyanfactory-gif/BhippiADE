//! Inspector Agents: observe, diagnose, recommend — and never execute (ADR-0056).
//!
//! A normal agent is asked to *change* something. An inspector is asked to *understand*
//! something, and the difference is enforced rather than promised: nothing under this module
//! writes a file. It reads a project once into a [`snapshot::ProjectSnapshot`], nine
//! specialists reason over that one parse, and what comes back is a list of
//! [`finding::Finding`]s — each of which answers what, where, why, how sure, what happens if
//! it is ignored, and what to do about it, or is refused at construction.
//!
//! A repair is *described*, in the engine's own typed action vocabulary, and left inert. It
//! becomes bytes on disk only when a person has seen the preview and pressed Apply, and only
//! through `bhippi-app`'s existing `lower` → `apply_changeset` → check → journal path — the
//! same one the agent uses, with the same undo and the same script compile (INV-096).
//!
//! # What is deliberately absent
//!
//! * **No model call.** A whole-project scan costs zero tokens. The conversational layer
//!   (§20) sits *above* this, and answers from these findings rather than from the project.
//! * **No invented measurement.** [`agents::performance`] reports a frame time only when a
//!   run produced one, and otherwise says so (INV-098).
//! * **No engine of its own.** Every fact comes from `godot::tscn`, `godot::project` and
//!   `godot::gates` — the parsers the rest of the studio already trusts.

pub mod agents;
pub mod command;
pub mod context;
pub mod finding;
pub mod fix;
pub mod gate_bridge;
pub mod measurement;
pub mod memory;
pub mod nodes;
pub mod report;
pub mod snapshot;

use bhippi_types::{InspectorId, Severity, INSPECT_MAX_FINDINGS};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use std::time::Instant;

pub use command::{parse as parse_command, CommandContext, InspectCommand, Selection};
pub use context::{InspectContext, InspectorOutput};
pub use finding::{Evidence, Finding, FindingAction, Location};
pub use fix::{fix_token, ProposedFix};
pub use measurement::{PerformanceEvidence, HOW_TO_MEASURE};
pub use memory::{reconcile, Changes, FindingLedger, LedgerEntry, Reconciled};
pub use report::{
    Coverage, Dimension, HealthReport, InspectScope, InspectionReport, SeverityCounts,
    REPORT_SCHEMA,
};
pub use snapshot::ProjectSnapshot;

/// One row of the Inspector rail (§11).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct InspectorCard {
    pub id: InspectorId,
    pub label: String,
    /// One line: what this specialist looks at.
    pub blurb: String,
}

/// The rail, in order. The UI draws this list and invents no entry of its own.
#[must_use]
pub fn registry() -> Vec<InspectorCard> {
    InspectorId::ALL
        .into_iter()
        .map(|id| InspectorCard {
            id,
            label: id.label().to_owned(),
            blurb: blurb(id).to_owned(),
        })
        .collect()
}

const fn blurb(id: InspectorId) -> &'static str {
    match id {
        InspectorId::Scene => "Scenes, instances, lighting and what the level actually contains",
        InspectorId::Code => "GDScript: per-frame cost, dangling loads, code nothing runs",
        InspectorId::Gameplay => "Whether the game can be played through: triggers, groups, input",
        InspectorId::Asset => "Files on disk: size, references, duplicates",
        InspectorId::Performance => "Measured cost. Never estimated",
        InspectorId::Ui => "Control trees: contrast, padding, keyboard reach",
        InspectorId::Animation => "Players, trees and the transitions between states",
        InspectorId::Physics => "Colliders, layers and things that look solid and are not",
        InspectorId::Ai => "Navigation, agents and whether an NPC can move at all",
    }
}

/// Run an inspection over a Godot project.
///
/// Reads the project from disk and returns everything the requested inspectors found. Pure
/// apart from those reads: nothing is written, nothing is spawned, no model is called.
///
/// `now` is RFC 3339 and supplied by the caller, so a test can assert the report's timestamp
/// rather than tolerate it. CPU-bound and IO-bound both — a Tauri caller runs it inside
/// `spawn_blocking` (R6).
#[must_use]
pub fn run(
    root: &Path,
    command: &InspectCommand,
    performance: Option<&PerformanceEvidence>,
    now: &str,
) -> InspectionReport {
    let started = Instant::now();
    let snapshot = ProjectSnapshot::read(root);
    let context = InspectContext::new(&snapshot, &command.scope, performance);

    let mut findings = Vec::new();
    let mut coverage: Vec<(InspectorId, Coverage)> = Vec::new();

    // The gates first: they own the eighteen things that stop a build, and their findings
    // land in whichever inspector's list they belong to (§7).
    let gate_report = crate::godot::gates::check_project(root, false);
    for finding in gate_bridge::findings(&gate_report) {
        if command.runs(finding.inspector) {
            findings.push(finding);
        }
    }

    let mut run_one = |id: InspectorId, output: InspectorOutput| {
        findings.extend(output.findings);
        coverage.push((id, output.coverage));
    };

    if command.runs(InspectorId::Scene) {
        run_one(InspectorId::Scene, agents::scene::inspect(&context));
    }
    if command.runs(InspectorId::Code) {
        run_one(InspectorId::Code, agents::code::inspect(&context));
    }
    if command.runs(InspectorId::Gameplay) {
        run_one(InspectorId::Gameplay, agents::gameplay::inspect(&context));
    }
    if command.runs(InspectorId::Asset) {
        run_one(InspectorId::Asset, agents::assets::inspect(&context));
    }
    if command.runs(InspectorId::Performance) {
        run_one(
            InspectorId::Performance,
            agents::performance::inspect(&context),
        );
    }
    if command.runs(InspectorId::Ui) {
        run_one(InspectorId::Ui, agents::ui::inspect(&context));
    }
    if command.runs(InspectorId::Animation) {
        run_one(InspectorId::Animation, agents::animation::inspect(&context));
    }
    if command.runs(InspectorId::Physics) {
        run_one(InspectorId::Physics, agents::physics::inspect(&context));
    }
    if command.runs(InspectorId::Ai) {
        run_one(InspectorId::Ai, agents::ai::inspect(&context));
    }

    if let Some(floor) = command.min_severity {
        findings.retain(|finding| finding.severity <= floor);
    }

    // Stable order, so two scans of an unchanged project are byte-identical reports.
    findings.sort_by_key(Finding::sort_key);
    // An id is (inspector, code, address); two inspectors reaching the same conclusion about
    // the same thing is one finding, not two. `dedup_by` would only catch neighbours, and
    // the sort is by severity first, so the survivors are chosen explicitly.
    let mut seen = std::collections::BTreeSet::new();
    findings.retain(|finding| seen.insert(finding.id.clone()));

    let mut truncated = snapshot.truncated.clone();
    if findings.len() > INSPECT_MAX_FINDINGS {
        truncated.push(format!(
            "{} findings were not listed; the {INSPECT_MAX_FINDINGS} worst are",
            findings.len() - INSPECT_MAX_FINDINGS
        ));
        findings.truncate(INSPECT_MAX_FINDINGS);
    }

    let health = HealthReport::compute(&findings, &coverage);
    InspectionReport {
        schema: REPORT_SCHEMA.to_owned(),
        scope: command.scope.clone(),
        project: snapshot
            .name
            .clone()
            .unwrap_or_else(|| root.display().to_string()),
        started_at: now.to_owned(),
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        findings,
        health,
        truncated,
    }
}

/// The task text a *Send to agent* button hands to a normal Bhippi agent (§18).
///
/// It carries the evidence and the recommendation and stops there — it never tells the agent
/// *how* to make the change, because the inspector does not know the project's conventions
/// and the agent does.
#[must_use]
pub fn agent_task(finding: &Finding) -> String {
    let mut text = String::new();
    text.push_str(&format!("{}\n\n", finding.recommendation));
    text.push_str(&format!("Where: {}\n", finding.location.describe()));
    text.push_str(&format!("Why it matters: {}\n", finding.impact));
    text.push_str(&format!(
        "\nInspector evidence ({}, {}% confident):\n",
        finding.code, finding.confidence
    ));
    text.push_str(&format!("- {}\n", finding.cause));
    for evidence in &finding.evidence {
        text.push_str(&format!("- {} ({})\n", evidence.claim, evidence.source));
    }
    text.push_str(
        "\nCheck the evidence against the project before changing anything; the Inspector \
         reads files and does not run the game.\n",
    );
    text
}

/// Severity is ordered worst-first, so "at least this bad" is `<=`.
#[must_use]
pub const fn at_least(severity: Severity, floor: Severity) -> bool {
    (severity as u8) <= (floor as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::finding::Location;

    #[test]
    fn the_rail_lists_every_inspector_exactly_once_and_every_row_says_something() {
        let cards = registry();
        assert_eq!(cards.len(), InspectorId::ALL.len());
        for card in &cards {
            assert!(!card.blurb.trim().is_empty(), "{:?} has no blurb", card.id);
            assert_eq!(card.label, card.id.label());
        }
        let ids: Vec<InspectorId> = cards.iter().map(|card| card.id).collect();
        assert_eq!(ids, InspectorId::ALL.to_vec());
    }

    #[test]
    fn a_severity_floor_keeps_the_worse_ones_and_drops_the_rest() {
        assert!(at_least(Severity::Critical, Severity::High));
        assert!(at_least(Severity::High, Severity::High));
        assert!(!at_least(Severity::Medium, Severity::High));
    }

    #[test]
    fn the_agent_task_carries_the_evidence_and_never_dictates_the_edit() {
        let finding = Finding::draft(
            InspectorId::Performance,
            "BHP-INS-501",
            Severity::Medium,
            94,
            "City_Map lighting costs 9.4 ms",
            Location::scene("scenes/city.tscn"),
        )
        .cause("measured over 600 frames")
        .impact("the frame budget is spent before anything else draws")
        .recommend("Reduce the dynamic lighting cost in City_Map without changing how it looks.")
        .evidence("9.4 ms in the lighting pass", "playtest 2026-09-10")
        .build()
        .expect("the draft is complete");

        let task = agent_task(&finding);
        assert!(task.starts_with("Reduce the dynamic lighting cost"));
        assert!(task.contains("scenes/city.tscn"));
        assert!(task.contains("9.4 ms in the lighting pass"));
        assert!(task.contains("94% confident"));
        // It hands over evidence, not instructions for how to edit.
        assert!(!task.contains("set_property"));
    }

    /// The rule the whole subsystem rests on, checked the only way a rule like this can be:
    /// by reading the source. An inspector that can write will eventually write.
    #[test]
    fn inspection_never_writes() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/inspect");
        let mut offenders = Vec::new();
        walk(&root, &mut |path, source| {
            // Test modules are not shipped, and this test's own forbidden-word list lives in
            // one. Everything before the first `#[cfg(test)]` is the module as it runs.
            let shipped = source.split("#[cfg(test)]").next().unwrap_or(source);
            for (index, line) in shipped.lines().enumerate() {
                let trimmed = line.trim_start();
                // A doc comment may name the thing it promises not to do.
                if trimmed.starts_with("//") {
                    continue;
                }
                for forbidden in [
                    "fs::write",
                    "fs::create_dir",
                    "fs::remove_file",
                    "fs::remove_dir",
                    "fs::rename",
                    "fs::copy",
                    "File::create",
                    "OpenOptions",
                    "apply_changeset",
                    "Command::new",
                ] {
                    if trimmed.contains(forbidden) {
                        offenders.push(format!("{}:{} {forbidden}", path.display(), index + 1));
                    }
                }
            }
        });
        assert!(
            offenders.is_empty(),
            "the inspect module must never write or spawn: {offenders:?}"
        );
    }

    fn walk(directory: &std::path::Path, visit: &mut dyn FnMut(&std::path::Path, &str)) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, visit);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                if let Ok(source) = std::fs::read_to_string(&path) {
                    visit(&path, &source);
                }
            }
        }
    }
}
