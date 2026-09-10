//! A test states its preconditions with `unwrap`/`expect`: a panic here is a failing
//! test, not a crashed app. The workspace `deny` stands everywhere else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! The Inspector, end to end, over a real project on disk (ADR-0056, INS-001…005).
//!
//! The unit tests prove each check against a scene literal. This one proves the thing those
//! checks are part of: a project is written to a temp directory with four planted defects,
//! `inspect::run` walks it, and the report has to name all four, score them, and — the part
//! that matters most — leave every file on disk exactly as it found it.

use bhippi_engine::inspect::{self, InspectCommand, InspectScope};
use bhippi_types::{InspectorId, Severity};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const NOW: &str = "2026-09-10T12:00:00Z";

fn temp_project(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("bhippi_inspect_{name}_{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

/// A small but complete Godot project carrying four planted defects:
///
/// 1. `Trigger` has an overlap handler and no connection  → gameplay, with a fix.
/// 2. `scripts/door.gd` reads an input action nothing defines → gameplay, critical.
/// 3. `scripts/door.gd` walks the tree every frame → code.
/// 4. `Trigger` has no collision shape → physics.
fn planted_project() -> PathBuf {
    let root = temp_project("planted");
    write(
        &root,
        "project.godot",
        "config_version=5\n\n[application]\n\nconfig/name=\"Planted\"\nrun/main_scene=\"res://scenes/main.tscn\"\n\n[input]\n\njump={\n\"deadzone\": 0.5,\n\"events\": []\n}\n",
    );
    write(
        &root,
        "scenes/main.tscn",
        r#"[gd_scene load_steps=3 format=3]

[ext_resource type="Script" path="res://scripts/door.gd" id="1_door"]

[node name="Main" type="Node3D"]

[node name="Camera3D" type="Camera3D" parent="."]
current = true

[node name="Sun" type="DirectionalLight3D" parent="."]

[node name="Player" type="CharacterBody3D" parent="."]

[node name="PlayerShape" type="CollisionShape3D" parent="Player"]
shape = SubResource("CapsuleShape3D_1")

[node name="Door" type="StaticBody3D" parent="."]
script = ExtResource("1_door")

[node name="DoorShape" type="CollisionShape3D" parent="Door"]
shape = SubResource("BoxShape3D_1")

[node name="Trigger" type="Area3D" parent="Door"]
"#,
    );
    write(
        &root,
        "scripts/door.gd",
        "extends StaticBody3D\n\nfunc _on_trigger_body_entered(body):\n\tstart_interaction()\n\nfunc start_interaction():\n\tpass\n\nfunc _process(_delta):\n\tvar hud = get_node(\"/root/Main/HUD\")\n\nfunc _unhandled_input(_event):\n\tif Input.is_action_just_pressed(\"interact\"):\n\t\tstart_interaction()\n",
    );
    root
}

fn project_scan() -> InspectCommand {
    InspectCommand {
        scope: InspectScope::Project,
        inspectors: Vec::new(),
        min_severity: None,
    }
}

/// Every file under `root`, with its bytes — for proving nothing moved.
fn fingerprint(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    fn walk(directory: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(directory).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    walk(root, root, &mut out);
    out
}

#[test]
fn a_project_scan_names_every_planted_defect_and_says_which_inspector_found_it() {
    let root = planted_project();
    let report = inspect::run(&root, &project_scan(), None, NOW);

    assert_eq!(report.schema, inspect::REPORT_SCHEMA);
    assert_eq!(report.project, "Planted");
    assert_eq!(report.started_at, NOW);

    let codes: Vec<&str> = report
        .findings
        .iter()
        .map(|finding| finding.code.as_str())
        .collect();
    for expected in [
        inspect::agents::gameplay::CODE_TRIGGER_UNWIRED,
        inspect::agents::gameplay::CODE_UNDEFINED_INPUT,
        inspect::agents::code::CODE_TICK_LOOKUP,
        inspect::agents::physics::CODE_NO_COLLIDER,
    ] {
        assert!(
            codes.contains(&expected),
            "{expected} was not found; the report had {codes:?}"
        );
    }

    let trigger = report
        .findings
        .iter()
        .find(|finding| finding.code == inspect::agents::gameplay::CODE_TRIGGER_UNWIRED)
        .unwrap();
    assert_eq!(trigger.inspector, InspectorId::Gameplay);
    assert_eq!(trigger.location.scene.as_deref(), Some("scenes/main.tscn"));
    assert_eq!(trigger.location.node.as_deref(), Some("Door/Trigger"));
    assert!(trigger.fix.is_some(), "the unwired trigger offers a fix");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn inspecting_a_project_leaves_every_byte_of_it_alone() {
    let root = planted_project();
    let before = fingerprint(&root);

    let report = inspect::run(&root, &project_scan(), None, NOW);
    assert!(!report.findings.is_empty(), "the scan found something");
    // And previewing the fix — the whole point of the preview — writes nothing either.
    let fix = report
        .findings
        .iter()
        .find_map(|finding| finding.fix.as_ref())
        .expect("at least one finding proposes a fix");
    assert!(!fix.token.is_empty());
    assert!(!fix.files.is_empty());

    let after = fingerprint(&root);
    assert_eq!(before, after, "an inspection changed the project");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn two_scans_of_an_unchanged_project_produce_the_same_report() {
    let root = planted_project();
    let first = inspect::run(&root, &project_scan(), None, NOW);
    let second = inspect::run(&root, &project_scan(), None, NOW);

    let ids = |report: &inspect::InspectionReport| -> Vec<String> {
        report
            .findings
            .iter()
            .map(|finding| format!("{}:{}", finding.code, finding.id))
            .collect()
    };
    assert_eq!(ids(&first), ids(&second));
    assert_eq!(first.health.score, second.health.score);

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn with_no_measurement_the_health_score_says_it_is_incomplete_rather_than_guessing() {
    let root = planted_project();
    let report = inspect::run(&root, &project_scan(), None, NOW);

    assert!(!report.health.complete);
    assert_eq!(report.health.incomplete, vec![InspectorId::Performance]);
    assert_eq!(
        report.health.incomplete_line().as_deref(),
        Some("Project health incomplete. 1 inspector has not yet scanned.")
    );

    let performance = report
        .health
        .dimensions
        .iter()
        .find(|dimension| dimension.inspector == InspectorId::Performance)
        .unwrap();
    assert_eq!(performance.score, None);
    match &performance.coverage {
        inspect::Coverage::NotMeasured { how } => assert_eq!(how, inspect::HOW_TO_MEASURE),
        other => panic!("expected NotMeasured, got {other:?}"),
    }
    // The eight that did scan still produce a real number.
    assert!(report.health.score.is_some());

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_measurement_turns_the_performance_dimension_into_a_scored_one() {
    let root = planted_project();
    let evidence = inspect::PerformanceEvidence::new("headless playtest", NOW)
        .with_frames(600, 12_000)
        .in_scene("scenes/main.tscn");
    let report = inspect::run(&root, &project_scan(), Some(&evidence), NOW);

    assert!(report.health.complete, "every inspector has now answered");
    let measured = report
        .findings
        .iter()
        .find(|finding| finding.code == inspect::agents::performance::CODE_MEASURED)
        .expect("the measurement is reported");
    assert_eq!(measured.severity, Severity::Info);
    assert!(measured.title.contains("20 ms/frame"));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn narrowing_to_one_inspector_runs_only_that_one_and_leaves_the_rest_unscored() {
    let root = planted_project();
    let command = InspectCommand {
        scope: InspectScope::Project,
        inspectors: vec![InspectorId::Physics],
        min_severity: None,
    };
    let report = inspect::run(&root, &command, None, NOW);

    assert!(report
        .findings
        .iter()
        .all(|finding| finding.inspector == InspectorId::Physics));
    assert!(report.health.incomplete.contains(&InspectorId::Code));
    assert!(!report.health.incomplete.contains(&InspectorId::Physics));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_severity_floor_keeps_only_what_is_at_least_that_bad() {
    let root = planted_project();
    let command = InspectCommand {
        scope: InspectScope::Project,
        inspectors: Vec::new(),
        min_severity: Some(Severity::High),
    };
    let report = inspect::run(&root, &command, None, NOW);

    assert!(!report.findings.is_empty());
    for finding in &report.findings {
        assert!(
            matches!(finding.severity, Severity::Critical | Severity::High),
            "{} slipped through the High floor as {:?}",
            finding.code,
            finding.severity
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_scan_of_a_directory_that_is_not_a_project_reports_rather_than_panicking() {
    let root = temp_project("empty");
    let report = inspect::run(&root, &project_scan(), None, NOW);

    // No project.godot: the gates say so, and the scan still returns a report.
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code.starts_with("BHP-GD-")));
    assert_eq!(report.scope, InspectScope::Project);

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_ledger_turns_a_second_scan_into_a_list_of_what_changed() {
    let root = planted_project();
    let first = inspect::run(&root, &project_scan(), None, NOW);
    let opening = inspect::reconcile(
        &inspect::FindingLedger::default(),
        first.findings.clone(),
        NOW,
    );
    assert_eq!(opening.changes.new_ids.len(), first.findings.len());
    assert!(opening.changes.resolved.is_empty());

    // Wire the trigger up, exactly as the proposed fix would have.
    let scene = root.join("scenes/main.tscn");
    let text = std::fs::read_to_string(&scene).unwrap();
    std::fs::write(
        &scene,
        format!("{text}\n[connection signal=\"body_entered\" from=\"Door/Trigger\" to=\"Door\" method=\"_on_trigger_body_entered\"]\n"),
    )
    .unwrap();

    let second = inspect::run(&root, &project_scan(), None, "2026-09-11T12:00:00Z");
    let after = inspect::reconcile(
        &opening.ledger,
        second.findings.clone(),
        "2026-09-11T12:00:00Z",
    );

    assert!(
        after
            .changes
            .resolved
            .iter()
            .any(|entry| entry.code == inspect::agents::gameplay::CODE_TRIGGER_UNWIRED),
        "the fixed trigger is reported as resolved: {}",
        after.changes.summary()
    );
    assert!(after
        .changes
        .resolved
        .iter()
        .all(|entry| entry.resolved_at.as_deref() == Some("2026-09-11T12:00:00Z")));

    std::fs::remove_dir_all(&root).ok();
}

/// The most important test in the file.
///
/// A findings list that cries wolf gets skimmed, and a skimmed list is worth nothing. The
/// strongest available guard against that is Bhippi's own output: a project the studio just
/// scaffolded is, by definition, correct, so **every** finding a scan produces over one is a
/// false positive until somebody argues otherwise.
///
/// The allowance is deliberately narrow and named. A scaffold has no assets and no licence
/// sidecars, so the licence gate speaks; it is a template, so nothing has been wired up yet.
/// Nothing else may appear here without a decision.
#[test]
fn a_freshly_scaffolded_project_produces_no_critical_and_no_high_finding() {
    for template in [
        bhippi_engine::godot::scaffold::ProjectTemplate::Empty3D,
        bhippi_engine::godot::scaffold::ProjectTemplate::ThirdPerson3D,
        bhippi_engine::godot::scaffold::ProjectTemplate::TopDown2D,
    ] {
        let root = temp_project(&format!("scaffold_{template:?}").to_lowercase());
        std::fs::remove_dir_all(&root).ok();
        bhippi_engine::godot::scaffold::write_project(&root, "Fresh", template, true)
            .expect("the scaffold writes");

        let report = inspect::run(&root, &project_scan(), None, NOW);
        let loud: Vec<String> = report
            .findings
            .iter()
            .filter(|finding| matches!(finding.severity, Severity::Critical | Severity::High))
            .map(|finding| {
                format!(
                    "{} {} at {}",
                    finding.code, finding.title, finding.where_label
                )
            })
            .collect();
        assert!(
            loud.is_empty(),
            "{template:?} scaffold produced findings a user would have to dismiss: {loud:#?}"
        );

        std::fs::remove_dir_all(&root).ok();
    }
}
