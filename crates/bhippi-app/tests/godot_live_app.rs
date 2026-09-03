//! The Godot pane's own live loop, end to end, against a real Godot 4.
//!
//! `crates/bhippi-engine/tests/godot_live.rs` proves the *library* halves: that the flags
//! are the flags Godot accepts and that the scaffold compiles. This one proves the halves
//! `bhippi-app` added on top of them, in the order the pane actually performs them:
//!
//! 1. create a project, and have the gates accept it;
//! 2. check a script through the runner and read Godot's stderr back as `file:line`;
//! 3. playtest 120 frames headless and parse the telemetry into a report;
//! 4. export for the web;
//! 5. serve that export from the preview server and read `index.html` and the `.wasm` back
//!    over a real socket with the headers a Godot page needs.
//!
//! `#[ignore]` because it needs Godot **and** its export templates, which is a separate
//! multi-hundred-megabyte download. Run it with:
//!
//! ```text
//! set BHIPPI_GODOT=C:\...\Godot_v4.7.1-stable_win64_console.exe
//! cargo test -p bhippi-app --test godot_live_app -- --ignored --nocapture
//! ```
//!
//! On Windows point `BHIPPI_GODOT` at the **console** build: the plain `.exe` is a
//! GUI-subsystem binary whose stdout goes nowhere.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_app::godot::{capture, detect_godot};
use bhippi_app::godot_commands::{first_script_fault, parse_script_faults};
use bhippi_app::godot_preview;
use bhippi_engine::godot::action::{apply_changeset, lower, GodotAction, GodotActionBatch};
use bhippi_engine::godot::command::{check_script_command, export_command, playtest_command};
use bhippi_engine::godot::export_presets::{WEB_EXPORT_PATH, WEB_PRESET_NAME};
use bhippi_engine::godot::gates::check_project;
use bhippi_engine::godot::probe::{PlaytestInputs, PlaytestStep, TelemetryReport};
use bhippi_engine::godot::scaffold::{write_project, ProjectTemplate};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;

/// Frames the live playtest runs for: two seconds at the fixed 60 fps.
const PLAYTEST_FRAMES: u32 = 120;
/// The fewest telemetry lines a healthy run must produce.
const MIN_TELEMETRY_LINES: usize = 10;

fn workspace() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bhippi-godot-live-{}", std::process::id()));
    let _ignored = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp workspace");
    dir
}

/// One HTTP request against the preview server, as raw text.
fn request(port: u16, line: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .write_all(format!("{line}\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").as_bytes())
        .expect("write");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read");
    String::from_utf8_lossy(&response).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a real Godot 4 and its export templates; set BHIPPI_GODOT"]
async fn a_new_project_checks_playtests_exports_and_previews() {
    let Some(install) = detect_godot(None).await else {
        panic!("no Godot found — set BHIPPI_GODOT to the console build");
    };
    println!(
        "godot: {} ({})",
        install.cli().display(),
        install.version.raw
    );

    // ── 1. create ────────────────────────────────────────────────────────────────
    let workspace = workspace();
    let root = workspace.join("live-game");
    let written =
        write_project(&root, "Live Game", ProjectTemplate::ThirdPerson3D, false).expect("scaffold");
    println!("scaffolded {} files into {}", written.len(), root.display());

    let gates = check_project(&root, false);
    assert!(
        gates.passes(),
        "a freshly scaffolded project must pass its own gates: {:?}",
        gates.blockers
    );
    println!("gates: {} warnings, 0 blockers", gates.warnings.len());

    // ── 2. check a script, both ways round ───────────────────────────────────────
    let good = check_script_command(install.cli(), &root, "scripts/player.gd");
    let (exit, output) = capture(&good).await.expect("check runs");
    assert!(
        exit.is_success(),
        "the scaffolded script must compile: {output}"
    );
    assert!(
        parse_script_faults(&output).is_empty(),
        "a clean check must produce no faults: {output}"
    );

    // Now break it through the typed action path — the only writer the agent has — and
    // confirm the runner reports a located fault rather than a wall of text.
    let batch = GodotActionBatch::new(
        "break the player script",
        vec![GodotAction::WriteScript {
            path: "scripts/player.gd".to_owned(),
            source: "extends CharacterBody3D\n\nfunc _ready() -> void:\n\tvar x = = 1\n".to_owned(),
        }],
    );
    let changeset = lower(&root, &batch).expect("lowering a script write always succeeds");
    apply_changeset(&root, &changeset).expect("write");
    let (broken_exit, broken_output) = capture(&good).await.expect("check runs");
    assert!(!broken_exit.is_success(), "a broken script must fail");
    let fault = first_script_fault(&broken_output, "scripts/player.gd")
        .unwrap_or_else(|| panic!("no located fault in:\n{broken_output}"));
    println!("located fault: {}", fault.to_message());
    assert_eq!(fault.file, "scripts/player.gd");
    assert!(fault.line > 0, "the fault must name a line: {fault:?}");

    // Put it back the way the command does, with the inverse.
    apply_changeset(&root, &bhippi_engine::godot::action::invert(&changeset)).expect("revert");
    let (restored, _) = capture(&good).await.expect("check runs");
    assert!(
        restored.is_success(),
        "the inverse restores a compiling file"
    );

    // ── 3. playtest ──────────────────────────────────────────────────────────────
    let telemetry_dir = root.join(".bhippi").join("telemetry");
    std::fs::create_dir_all(&telemetry_dir).expect("telemetry dir");
    let inputs_path = telemetry_dir.join("live.inputs.json");
    let telemetry_path = telemetry_dir.join("live.jsonl");
    let inputs = PlaytestInputs::new(vec![
        PlaytestStep::action(10, "move_forward", true),
        PlaytestStep::action(40, "move_forward", false),
        PlaytestStep::action(50, "jump", true),
        PlaytestStep::action(54, "jump", false),
    ]);
    std::fs::write(&inputs_path, inputs.to_json().expect("valid inputs")).expect("write inputs");

    let spec = playtest_command(
        install.cli(),
        &root,
        &inputs_path,
        &telemetry_path,
        PLAYTEST_FRAMES,
    );
    let (playtest_exit, playtest_output) = capture(&spec).await.expect("playtest runs");
    println!("playtest exit: {playtest_exit:?}");
    assert!(
        !playtest_exit.timed_out,
        "a {PLAYTEST_FRAMES}-frame playtest must finish inside its budget: {playtest_output}"
    );
    let text = std::fs::read_to_string(&telemetry_path).expect("telemetry file");
    let lines = text.lines().filter(|line| !line.trim().is_empty()).count();
    let report = TelemetryReport::from_jsonl(&text);
    println!(
        "telemetry: {lines} lines, {} samples, done={}, frames={:?}, tracked={:?}",
        report.sample_count(),
        report.done,
        report.frames,
        report.last_positions.keys().collect::<Vec<_>>()
    );
    assert!(
        lines >= MIN_TELEMETRY_LINES,
        "expected at least {MIN_TELEMETRY_LINES} telemetry lines, got {lines}"
    );
    assert!(report.done, "the probe must write its final done line");
    assert_eq!(report.malformed_lines, 0, "telemetry must be clean JSONL");

    // ── 4. export web ────────────────────────────────────────────────────────────
    let output_path = root.join(WEB_EXPORT_PATH);
    std::fs::create_dir_all(output_path.parent().expect("parent")).expect("export dir");
    let export = export_command(install.cli(), &root, WEB_PRESET_NAME, &output_path, true);
    let (export_exit, export_output) = capture(&export).await.expect("export runs");
    println!("export exit: {export_exit:?}");
    assert!(
        output_path.is_file(),
        "the web export must land at {}: {export_output}",
        output_path.display()
    );

    // ── 5. preview ───────────────────────────────────────────────────────────────
    let server = godot_preview::start(&root).expect("the preview server starts");
    println!("preview: {}", server.url());
    let index = request(server.port(), "GET /index.html HTTP/1.1");
    assert!(index.starts_with("HTTP/1.1 200 OK"), "{index}");
    assert!(index.contains("Content-Type: text/html; charset=utf-8"));
    assert!(index.contains("Cross-Origin-Opener-Policy: same-origin"));
    assert!(index.contains("Cross-Origin-Embedder-Policy: require-corp"));
    assert!(index.contains("Cache-Control: no-store"));

    // The `.wasm` is what actually decides whether the page runs: the browser refuses to
    // stream-instantiate anything that is not `application/wasm`.
    let wasm_name = std::fs::read_dir(root.join("export").join("web"))
        .expect("export dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .find(|name| name.ends_with(".wasm"))
        .expect("the export ships a .wasm");
    let wasm = request(server.port(), &format!("GET /{wasm_name} HTTP/1.1"));
    println!("served {wasm_name}");
    assert!(wasm.starts_with("HTTP/1.1 200 OK"), "{wasm}");
    assert!(
        wasm.contains("Content-Type: application/wasm"),
        "the wasm needs its own type: {}",
        wasm.lines().take(8).collect::<Vec<_>>().join(" | ")
    );

    let escape = request(server.port(), "GET /../../project.godot HTTP/1.1");
    assert!(
        escape.starts_with("HTTP/1.1 403"),
        "the preview must not serve the project folder: {escape}"
    );

    server.stop();
    let _ignored = std::fs::remove_dir_all(&workspace);
}
