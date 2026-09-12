//! A test states its preconditions with `unwrap`/`expect`: a panic here is a failing
//! test, not a crashed app. The workspace `deny` stands everywhere else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! The joystick, proved against the real Godot binary (`--ignored`, needs `BHIPPI_GODOT`).
//!
//! The other tests prove the batch lowers and the files parse *by Bhippi's own parser*, which
//! is exactly the kind of proof that can agree with itself and still be wrong. This one hands
//! the finished project to Godot 4.7.1 and asks it.
//!
//! Skips loudly when the binary is absent — a skipped test never reports as a pass.

use bhippi_engine::godot::action::{apply_changeset, lower, GodotAction, GodotActionBatch};
use bhippi_engine::godot::scaffold::{write_project, ProjectTemplate};
use bhippi_engine::godot::tscn::TscnValue;
use std::path::PathBuf;
use std::process::Command;

fn godot() -> Option<PathBuf> {
    if let Ok(from_env) = std::env::var("BHIPPI_GODOT") {
        let path = PathBuf::from(from_env);
        return path.exists().then_some(path);
    }
    let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../bhippi-app/resources/godot/Godot_v4.7.1-stable_win64_console.exe");
    bundled.exists().then_some(bundled)
}

#[test]
#[ignore = "needs a real Godot binary; run with --ignored"]
fn godot_itself_accepts_the_joystick_the_agent_builds() {
    let Some(godot) = godot() else {
        panic!("SKIPPED LOUDLY: no Godot binary. Set BHIPPI_GODOT or restore resources/godot/.");
    };

    let root = std::env::temp_dir().join(format!("bhippi-live-joystick-{}", ulid::Ulid::new()));
    let _ = std::fs::remove_dir_all(&root);
    write_project(&root, "LiveJoystick", ProjectTemplate::TopDown2D, true).expect("scaffold");

    let source = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/touch_joystick.gd"),
    )
    .expect("the joystick fixture is on disk");

    let batch = GodotActionBatch::new(
        "add a touch joystick",
        vec![
            GodotAction::WriteScript {
                path: "scripts/touch_joystick.gd".to_owned(),
                source,
            },
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "TouchControls".to_owned(),
                type_: "CanvasLayer".to_owned(),
                properties: Vec::new(),
                groups: Vec::new(),
            },
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: "TouchControls".to_owned(),
                name: "Joystick".to_owned(),
                type_: "Control".to_owned(),
                properties: vec![("offset_left".to_owned(), TscnValue::Float(80.0))],
                groups: Vec::new(),
            },
            GodotAction::AttachScript {
                scene: "scenes/main.tscn".to_owned(),
                path: "TouchControls/Joystick".to_owned(),
                script_res_path: "res://scripts/touch_joystick.gd".to_owned(),
            },
            // A collider with a real shape, through the verb that did not exist.
            GodotAction::AddSubResource {
                scene: "scenes/main.tscn".to_owned(),
                id: "RectangleShape2D_agentwall".to_owned(),
                type_: "RectangleShape2D".to_owned(),
                properties: vec![("size".to_owned(), TscnValue::Vector2(64.0, 64.0))],
            },
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "AgentWall".to_owned(),
                type_: "StaticBody2D".to_owned(),
                properties: Vec::new(),
                groups: Vec::new(),
            },
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: "AgentWall".to_owned(),
                name: "Shape".to_owned(),
                type_: "CollisionShape2D".to_owned(),
                properties: vec![(
                    "shape".to_owned(),
                    TscnValue::SubResource("RectangleShape2D_agentwall".to_owned()),
                )],
                groups: Vec::new(),
            },
        ],
    );
    let changeset = lower(&root, &batch).expect("the batch lowers");
    apply_changeset(&root, &changeset).expect("the batch applies");

    // 1. Godot parses the script.
    let check = Command::new(&godot)
        .args(["--headless", "--path"])
        .arg(&root)
        .args([
            "--check-only",
            "--script",
            "res://scripts/touch_joystick.gd",
        ])
        .output()
        .expect("Godot runs");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(
        check.status.success() && !output.contains("SCRIPT ERROR"),
        "Godot rejected the joystick script:\n{output}"
    );

    // 2. Godot opens the project and the scene without erroring on the sub-resource or the
    //    node tree. `--quit-after` runs a few frames of the real main loop, headless.
    let run = Command::new(&godot)
        .args(["--headless", "--path"])
        .arg(&root)
        .args(["--quit-after", "20"])
        .output()
        .expect("Godot runs");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    for complaint in [
        "SCRIPT ERROR",
        "ERROR: Condition",
        "Parse Error",
        "Failed loading",
    ] {
        assert!(
            !log.contains(complaint),
            "Godot reported `{complaint}` opening the built project:\n{log}"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}
