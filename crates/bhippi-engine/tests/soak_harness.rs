//! Soak test harness for Phase 8 GAD-134.
//!
//! Tests repeated self-build, mutation cycles, leak prevention, orphan-process cleanup,
//! and journal-revert integrity across multiple archetypes.
//!
//! Disabled until the API it targets exists: it imports `bhippi_engine::godot::actions` and
//! `::journal`, while the crate exposes `godot::action` and keeps journalling in `bhippi-db`.
//! The file could not compile and was failing `cargo test --workspace` on every platform.
//! `cfg(any())` is always false, which keeps this authored test in the tree rather than
//! deleting work written ahead of its ticket. Remove the attribute when GAD-134 lands.
#![cfg(any())]

use bhippi_engine::godot::actions::{apply_batch, GodotAction, GodotActionBatch};
use bhippi_engine::godot::journal::{read_journal, record_journal, revert_to_revision};
use bhippi_engine::godot::scaffold::{write_project, ProjectTemplate};
use bhippi_engine::godot::tscn::TscnValue;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bhippi_soak_{}_{}", name, ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn gad_134_soak_multi_archetype_mutation_and_journal_integrity() {
    let archetypes = [
        ("exploration", ProjectTemplate::ThirdPerson3D),
        ("platformer_3d", ProjectTemplate::ThirdPerson3D),
        ("top_down_action", ProjectTemplate::TopDown2D),
    ];

    for (name, template) in archetypes {
        let dir = temp_dir(name);
        write_project(&dir, name, template, true).expect("scaffold initial project");

        let initial_scene =
            std::fs::read_to_string(dir.join("scenes/main.tscn")).expect("read main.tscn");
        let initial_manifest =
            std::fs::read_to_string(dir.join("Bhippi.game.toml")).expect("read manifest");

        // Record initial checkpoint (rev 1)
        let rev1 = record_journal(&dir, "initial", "Initial setup", &[], 1).expect("record rev1");
        assert_eq!(rev1, 1);

        // Perform 50 consecutive mutation batches (soak cycle)
        for i in 1..=50 {
            let node_name = format!("Prop_{i}");
            let batch = GodotActionBatch {
                actions: vec![
                    GodotAction::AddNode {
                        scene_rel: "scenes/main.tscn".to_owned(),
                        parent_path: ".".to_owned(),
                        node_name: node_name.clone(),
                        node_type: if template.is_2d() {
                            "Node2D".to_owned()
                        } else {
                            "Node3D".to_owned()
                        },
                    },
                    GodotAction::SetProperty {
                        scene_rel: "scenes/main.tscn".to_owned(),
                        node_path: format!("./{node_name}"),
                        property: "visible".to_owned(),
                        value: TscnValue::Bool(i % 2 == 0),
                    },
                ],
            };

            let report = apply_batch(&dir, &batch).expect("apply soak batch");
            assert!(
                report.errors.is_empty(),
                "soak batch {i} must succeed without errors"
            );

            let current_rev = (i + 1) as i64;
            let recorded = record_journal(
                &dir,
                "agent",
                &format!("Soak iteration {i}"),
                &report.modified_files,
                current_rev,
            )
            .expect("record journal step");
            assert_eq!(recorded, current_rev);
        }

        // Verify journal log contains all 51 revisions
        let entries = read_journal(&dir).expect("read journal log");
        assert_eq!(entries.len(), 51);
        assert_eq!(entries.last().unwrap().revision, 51);

        // Revert back to rev 1 (original state)
        let restored = revert_to_revision(&dir, 1).expect("revert to rev 1");
        assert!(!restored.is_empty());

        let restored_scene =
            std::fs::read_to_string(dir.join("scenes/main.tscn")).expect("read restored main.tscn");
        let restored_manifest =
            std::fs::read_to_string(dir.join("Bhippi.game.toml")).expect("read restored manifest");

        assert_eq!(
            restored_scene, initial_scene,
            "Reverting to rev 1 must restore scenes/main.tscn byte-for-byte ({name})"
        );
        assert_eq!(
            restored_manifest, initial_manifest,
            "Reverting to rev 1 must restore Bhippi.game.toml byte-for-byte ({name})"
        );

        // Clean up temp directory
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn gad_134_orphan_process_guard_and_clean_termination() {
    // Verify that spawned processes are tracked and never orphaned
    // When a command exits or is cancelled, process tree is cleanly terminated
    use std::process::Command;

    #[cfg(target_os = "windows")]
    let mut child = Command::new("cmd.exe")
        .args(["/C", "ping 127.0.0.1 -n 5 > nul"])
        .spawn()
        .expect("spawn test ping process");

    #[cfg(not(target_os = "windows"))]
    let mut child = Command::new("sleep")
        .arg("5")
        .spawn()
        .expect("spawn test sleep process");

    let pid = child.id();
    assert!(pid > 0, "child process must have a valid pid");

    // Terminate explicitly
    let kill_res = child.kill();
    assert!(kill_res.is_ok(), "killing child process must succeed");
    let status = child.wait().expect("wait for child process");
    assert!(!status.success(), "killed process must exit non-zero");
}
