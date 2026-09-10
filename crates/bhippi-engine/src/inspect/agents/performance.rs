//! The Performance Inspector: measured, or silent (ADR-0056 §8, INV-098).
//!
//! This is the shortest inspector in the set, and that is the design. Frame time, GPU time,
//! draw calls and VRAM are **measurements**. Bhippi can take them — the headless playtest
//! counts frames, and the Computer Use loop watches a real window — but until one of those
//! has run, the honest answer is *not measured yet*, with the button that would produce a
//! number beside it.
//!
//! The temptation this module exists to refuse is the plausible estimate: "four shadow-casting
//! lights, probably −8 to −14 fps". That sentence has never been true of a specific project,
//! and a user who acts on it and sees no change stops believing the panel that produced it.
//!
//! Static cost facts still get reported — they are just filed where they are provable. A
//! texture's size is the Asset Inspector's (`BHP-INS-402`), two meshes in the same place are
//! the Scene Inspector's (`BHP-INS-105`). Neither claims a frame-rate effect.

use bhippi_types::{InspectorId, Severity, INSPECT_CONFIDENCE_CERTAIN};

use super::collect;
use crate::inspect::context::{InspectContext, InspectorOutput};
use crate::inspect::finding::{Finding, Location};
use crate::inspect::measurement::HOW_TO_MEASURE;
use crate::inspect::report::Coverage;

/// A measurement from a run that happened.
pub const CODE_MEASURED: &str = "BHP-INS-501";

#[must_use]
pub fn inspect(context: &InspectContext<'_>) -> InspectorOutput {
    let Some(evidence) = context.performance else {
        return InspectorOutput {
            findings: Vec::new(),
            coverage: Coverage::NotMeasured {
                how: HOW_TO_MEASURE.to_owned(),
            },
        };
    };

    let mut findings = Vec::new();
    let scene = evidence
        .scene
        .clone()
        .or_else(|| context.snapshot.main_scene.clone())
        .unwrap_or_else(|| "project.godot".to_owned());

    let mut draft = Finding::draft(
        InspectorId::Performance,
        CODE_MEASURED,
        Severity::Info,
        INSPECT_CONFIDENCE_CERTAIN,
        evidence.headline(),
        Location::scene(&scene),
    )
    .cause(format!(
        "Measured by {} at {}. Every number here is arithmetic on that run; none of it is \
         estimated.",
        evidence.source, evidence.captured_at
    ))
    .impact("Nothing on its own — this is the baseline the next run is compared against.")
    .recommend("Run it again after a change that should have made the game faster, and compare.")
    .evidence(evidence.source.clone(), evidence.captured_at.clone());

    if let (Some(frames), Some(elapsed)) = (evidence.frames, evidence.elapsed_ms) {
        draft = draft.evidence(
            format!("{frames} frames in {elapsed} ms"),
            evidence.source.clone(),
        );
    }
    collect(&mut findings, draft);

    InspectorOutput {
        findings,
        coverage: Coverage::Scanned { items: 1 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::context::test_support::context_from;
    use crate::inspect::context::InspectContext;
    use crate::inspect::measurement::PerformanceEvidence;
    use crate::inspect::report::InspectScope;
    use crate::inspect::snapshot::ProjectSnapshot;

    #[test]
    fn with_no_measurement_it_reports_nothing_and_names_the_scan_that_would_help() {
        let snapshot = ProjectSnapshot::default();
        let output = inspect(&context_from(&snapshot));
        assert!(output.findings.is_empty());
        match output.coverage {
            Coverage::NotMeasured { how } => assert_eq!(how, HOW_TO_MEASURE),
            other => panic!("expected NotMeasured, got {other:?}"),
        }
    }

    #[test]
    fn with_a_measurement_it_reports_the_numbers_that_run_produced_and_no_others() {
        let snapshot = ProjectSnapshot {
            main_scene: Some("scenes/main.tscn".to_owned()),
            ..Default::default()
        };
        let evidence = PerformanceEvidence::new("headless playtest", "2026-09-10T09:00:00Z")
            .with_frames(600, 12_000);
        let scope = InspectScope::Project;
        let context = InspectContext::new(&snapshot, &scope, Some(&evidence));
        let output = inspect(&context);

        assert!(matches!(output.coverage, Coverage::Scanned { items: 1 }));
        let measured = output
            .findings
            .first()
            .expect("the measurement is reported");
        assert_eq!(measured.severity, Severity::Info);
        assert!(measured.title.contains("20 ms/frame"));
        assert!(measured.title.contains("50 fps"));
        // Nothing in the finding claims a cause or a saving.
        assert!(!measured.title.contains("bottleneck"));
        assert!(measured
            .evidence
            .iter()
            .any(|evidence| evidence.claim.contains("600 frames in 12000 ms")));
    }
}
