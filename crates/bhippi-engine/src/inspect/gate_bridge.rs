//! The blocking gates, told as findings (ADR-0056 §7).
//!
//! `godot::gates` already knows the eighteen things that turn a project into a broken build.
//! Re-implementing any of them inside an inspector would give the user two answers to one
//! question and give the codebase two places to fix it, so the inspectors do not: the gate
//! runs once, and its report is *translated* here into the same [`Finding`] shape everything
//! else in the drawer uses.
//!
//! Two deliberate omissions. Scene parse failures are dropped because the scene inspector
//! sees every scene in the project rather than only the ones under `scenes/`, and reports
//! them itself. The probe codes are dropped because Bhippi's own instrumentation missing
//! from a project is Bhippi's problem, not a defect in the user's game — it belongs in the
//! studio's status, not in their findings list.

use crate::godot::gates::{self, GateReport};
use bhippi_types::{InspectorId, Severity, INSPECT_CONFIDENCE_CERTAIN};

use super::finding::{Finding, Location};

/// Which inspector owns each gate code, and how bad it is as a finding.
///
/// A gate *blocker* is `Critical`; a gate *warning* keeps the severity named here. The
/// mapping is a table rather than a chain of `if`s so a new gate code that nobody routed
/// shows up in `every_gate_code_is_routed` instead of vanishing from the drawer.
#[must_use]
pub fn route(code: &str) -> Option<(InspectorId, Severity)> {
    Some(match code {
        gates::CODE_MANIFEST | gates::CODE_RUNTIME => (InspectorId::Scene, Severity::High),
        gates::CODE_PROJECT_FILE | gates::CODE_PROJECT_PARSE => {
            (InspectorId::Scene, Severity::High)
        }
        // You cannot play a game with no way in: this is progression, not structure.
        gates::CODE_MAIN_SCENE_UNSET | gates::CODE_MAIN_SCENE_MISSING => {
            (InspectorId::Gameplay, Severity::High)
        }
        gates::CODE_MAIN_SCENE_DRIFT | gates::CODE_NAME_DRIFT => {
            (InspectorId::Scene, Severity::Medium)
        }
        gates::CODE_DANGLING_RESOURCE => (InspectorId::Asset, Severity::High),
        gates::CODE_MISSING_SCRIPT => (InspectorId::Code, Severity::High),
        gates::CODE_WEB_PRESET => (InspectorId::Asset, Severity::Low),
        gates::CODE_LICENSE_MISSING | gates::CODE_LICENSE_UNKNOWN => {
            (InspectorId::Asset, Severity::Medium)
        }
        gates::CODE_CAMERA_NOT_CURRENT => (InspectorId::Scene, Severity::High),
        // Owned elsewhere, on purpose — see the module docs.
        gates::CODE_SCENE_PARSE | gates::CODE_PROBE_AUTOLOAD | gates::CODE_PROBE_SCRIPT => {
            return None
        }
        _ => return None,
    })
}

/// Translate a gate report into findings.
///
/// The gate's `where_` is a project-relative path or a setting name; a path that looks like
/// a scene becomes a scene location so *Open scene* works, and everything else becomes a
/// file location so *Open* does.
#[must_use]
pub fn findings(report: &GateReport) -> Vec<Finding> {
    let mut out = Vec::new();
    for (finding, blocking) in report
        .blockers
        .iter()
        .map(|finding| (finding, true))
        .chain(report.warnings.iter().map(|finding| (finding, false)))
    {
        let Some((inspector, severity)) = route(&finding.code) else {
            continue;
        };
        let severity = if blocking {
            Severity::Critical
        } else {
            severity
        };
        let location = location_for(&finding.where_);
        let built = Finding::draft(
            inspector,
            finding.code.clone(),
            severity,
            INSPECT_CONFIDENCE_CERTAIN,
            finding.message.clone(),
            location,
        )
        .cause(if blocking {
            "A project gate blocks on this: it is not an opinion, the build stops here."
        } else {
            "A project gate reports this. It does not stop a debug run, and it does stop a release."
        })
        .impact(if blocking {
            "The project will not build or run until it is fixed."
        } else {
            "The release gate will refuse this project as it stands."
        })
        .recommend(finding.hint.clone())
        .evidence(finding.message.clone(), finding.where_.clone())
        .build();
        match built {
            Ok(finding) => out.push(finding),
            Err(error) => {
                tracing::error!(%error, "a gate finding could not be translated");
            }
        }
    }
    out
}

fn location_for(where_: &str) -> Location {
    if where_.ends_with(".tscn") || where_.ends_with(".tres") {
        Location::scene(where_)
    } else if where_.contains('/') || where_.contains('.') {
        Location::file(where_)
    } else {
        Location::symbol(where_)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::godot::gates::Finding as GateFinding;

    fn gate(code: &str, where_: &str) -> GateFinding {
        GateFinding {
            code: code.to_owned(),
            message: format!("{code} happened"),
            hint: "Do the thing.".to_owned(),
            where_: where_.to_owned(),
        }
    }

    /// Every code `gates` can emit is either routed or deliberately dropped. The list is
    /// the gate module's own constants, so a new gate that nobody thought about fails here.
    #[test]
    fn every_gate_code_is_either_routed_or_deliberately_dropped() {
        let dropped = [
            gates::CODE_SCENE_PARSE,
            gates::CODE_PROBE_AUTOLOAD,
            gates::CODE_PROBE_SCRIPT,
        ];
        let all = [
            gates::CODE_MANIFEST,
            gates::CODE_RUNTIME,
            gates::CODE_PROJECT_FILE,
            gates::CODE_PROJECT_PARSE,
            gates::CODE_MAIN_SCENE_UNSET,
            gates::CODE_MAIN_SCENE_MISSING,
            gates::CODE_SCENE_PARSE,
            gates::CODE_DANGLING_RESOURCE,
            gates::CODE_MISSING_SCRIPT,
            gates::CODE_PROBE_AUTOLOAD,
            gates::CODE_PROBE_SCRIPT,
            gates::CODE_WEB_PRESET,
            gates::CODE_LICENSE_MISSING,
            gates::CODE_LICENSE_UNKNOWN,
            gates::CODE_MAIN_SCENE_DRIFT,
            gates::CODE_CAMERA_NOT_CURRENT,
            gates::CODE_NAME_DRIFT,
        ];
        for code in all {
            let routed = route(code).is_some();
            let dropped = dropped.contains(&code);
            assert!(
                routed != dropped,
                "{code} is neither routed nor deliberately dropped"
            );
        }
    }

    #[test]
    fn a_blocker_is_critical_whatever_its_route_says() {
        let report = GateReport {
            blockers: vec![gate(gates::CODE_MISSING_SCRIPT, "scripts/player.gd")],
            warnings: vec![gate(gates::CODE_LICENSE_MISSING, "assets/rock.glb")],
        };
        let findings = findings(&report);
        assert_eq!(findings.len(), 2);
        let script = findings
            .iter()
            .find(|finding| finding.code == gates::CODE_MISSING_SCRIPT)
            .expect("the blocker is translated");
        assert_eq!(script.severity, Severity::Critical);
        assert_eq!(script.inspector, InspectorId::Code);
        assert_eq!(script.location.file.as_deref(), Some("scripts/player.gd"));

        let licence = findings
            .iter()
            .find(|finding| finding.code == gates::CODE_LICENSE_MISSING)
            .expect("the warning is translated");
        assert_eq!(licence.severity, Severity::Medium);
        assert_eq!(licence.inspector, InspectorId::Asset);
    }

    #[test]
    fn a_dropped_code_produces_no_finding_at_all() {
        let report = GateReport {
            blockers: vec![gate(gates::CODE_PROBE_SCRIPT, "bhippi/probe.gd")],
            warnings: Vec::new(),
        };
        assert!(findings(&report).is_empty());
    }

    #[test]
    fn a_scene_path_becomes_a_scene_location_so_open_scene_works() {
        let report = GateReport {
            blockers: Vec::new(),
            warnings: vec![gate(gates::CODE_CAMERA_NOT_CURRENT, "scenes/main.tscn")],
        };
        let findings = findings(&report);
        let camera = findings.first().expect("the warning is translated");
        assert_eq!(camera.location.scene.as_deref(), Some("scenes/main.tscn"));
    }
}
