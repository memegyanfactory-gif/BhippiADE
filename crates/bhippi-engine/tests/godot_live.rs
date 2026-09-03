//! Live Godot tests. `#[ignore]` by default: they need a real Godot 4 on the machine.
//!
//! Everything else about the Godot support is proved against fixtures, which is what keeps
//! `cargo test` honest on a CI box with no engine installed. These are the claims fixtures
//! cannot make: that the flags in `command.rs` are the flags Godot 4.7 actually accepts,
//! that the scaffolded GDScript compiles, and that the probe writes telemetry a real
//! headless run produces.
//!
//! Run them with:
//!
//! ```text
//! set BHIPPI_GODOT=C:\...\Godot_v4.7.1-stable_win64_console.exe
//! cargo test -p bhippi-engine --test godot_live -- --ignored --nocapture
//! ```
//!
//! On Windows, point `BHIPPI_GODOT` at the **console** build: the plain `.exe` is a
//! GUI-subsystem binary whose stdout goes nowhere, so `--version` would come back empty.
//!
//! Process execution in a `#[cfg(test)]` integration test does not make `bhippi-engine`
//! impure — the library still only *describes* commands; this file runs them.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::godot::command::{
    check_script_command, editor_command, export_command, playtest_command, run_command,
    version_command, CommandSpec, RunOptions,
};
use bhippi_engine::godot::detect::{
    candidate_paths, export_templates_dir, export_templates_installed, is_supported,
    pair_windows_binaries, parse_version, GODOT_MINIMUM,
};
use bhippi_engine::godot::export_presets::{WEB_EXPORT_PATH, WEB_PRESET_NAME};
use bhippi_engine::godot::gates::check_project;
use bhippi_engine::godot::probe::{PlaytestInputs, PlaytestStep, TelemetryReport};
use bhippi_engine::godot::project::GodotProjectFile;
use bhippi_engine::godot::scaffold::{
    ensure_studio_addon, write_project, ProjectTemplate, STUDIO_ADDON_CFG_REL,
    STUDIO_ADDON_RES_PATH, STUDIO_ADDON_SCRIPT_REL,
};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Frames a live playtest runs for. Twenty samples at the probe's default interval.
const PLAYTEST_FRAMES: u32 = 120;
/// The frame the scripted jump is pressed on — late enough that the player has landed.
const JUMP_FRAME: u32 = 30;
/// The fewest telemetry lines a healthy 120-frame run must produce.
const MIN_TELEMETRY_LINES: usize = 10;
/// Frames a headless editor boot runs for before quitting. Long enough to get past the
/// filesystem scan, plugin initialisation and the editor layout restore.
const EDITOR_BOOT_FRAMES: u32 = 150;

/// The Godot to test against, or `None` with the reason printed.
fn godot() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("BHIPPI_GODOT") {
        let path = PathBuf::from(value);
        let (cli, _) = pair_windows_binaries(&path);
        if cli.is_file() {
            return Some(cli);
        }
        println!(
            "SKIP: BHIPPI_GODOT points at {}, which is not a file",
            cli.display()
        );
        return None;
    }
    for (candidate, _) in candidate_paths(None) {
        let (cli, _) = pair_windows_binaries(&candidate);
        if !cli.is_file() {
            continue;
        }
        if let Some(version) = probe_version(&cli) {
            if is_supported(&version) {
                return Some(cli);
            }
        }
    }
    let (major, minor) = GODOT_MINIMUM;
    println!(
        "SKIP: no Godot {major}.{minor}+ found. Set BHIPPI_GODOT to the console build to run these."
    );
    None
}

fn probe_version(path: &Path) -> Option<bhippi_engine::godot::detect::GodotVersion> {
    let spec = version_command(path);
    let output = run(&spec).ok()?;
    parse_version(&output.stdout).ok()
}

struct Output {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Output {
    fn all(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

/// Run a [`CommandSpec`] the way `bhippi_app::godot` would, minus the streaming.
fn run(spec: &CommandSpec) -> std::io::Result<Output> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    let output = command.output()?;
    Ok(Output {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

struct Project(PathBuf);

impl Project {
    fn scaffold(name: &str, template: ProjectTemplate) -> Self {
        let root = std::env::temp_dir().join(format!("bhippi-godot-live-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        write_project(&root, "Live Test", template, true).expect("scaffold");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "needs a real Godot 4 install; set BHIPPI_GODOT"]
fn the_installed_godot_reports_a_version_this_build_supports() {
    let Some(godot) = godot() else { return };
    let output = run(&version_command(&godot)).expect("godot --version runs");
    assert_eq!(output.code, Some(0), "stderr: {}", output.stderr);

    let version = parse_version(&output.stdout).expect("the version line parses");
    println!("godot --version -> {}", version.raw);
    assert!(
        is_supported(&version),
        "{} is older than the supported minimum {GODOT_MINIMUM:?}",
        version.short()
    );
    assert_eq!(version.major, 4);
}

#[test]
#[ignore = "needs a real Godot 4 install; set BHIPPI_GODOT"]
fn every_scaffolded_script_survives_check_only() {
    let Some(godot) = godot() else { return };
    for template in [
        ProjectTemplate::Empty3D,
        ProjectTemplate::ThirdPerson3D,
        ProjectTemplate::TopDown2D,
    ] {
        let project = Project::scaffold(&format!("check-{template:?}").to_lowercase(), template);
        assert!(
            check_project(project.path(), false).passes(),
            "the scaffold must pass its own gates first"
        );
        for script in [
            template.script_rel(),
            "bhippi/probe.gd",
            STUDIO_ADDON_SCRIPT_REL,
        ] {
            let spec = check_script_command(&godot, project.path(), script);
            let output = run(&spec).expect("godot --check-only runs");
            assert_eq!(
                output.code,
                Some(0),
                "{template:?} {script} failed --check-only:\n{}",
                output.all()
            );
        }
    }
}

/// The studio addon (ADR-0045) on a project that predates it: `ensure_studio_addon` puts it
/// back, Godot compiles the script, and a real editor boot loads the plugin without an error.
///
/// The editor is run `--headless --quit-after`, which is not a picture of the layout — no
/// fixture can prove *visually* that the docks are hidden. What it does prove is the part
/// that silently breaks: that Godot accepts the `plugin.cfg`, instantiates the
/// `EditorPlugin`, and that `EditorInterface.set_distraction_free_mode` is a real call on
/// this engine build rather than a method name that went away.
#[test]
#[ignore = "needs a real Godot 4 install; set BHIPPI_GODOT"]
fn the_studio_addon_installs_into_an_older_project_and_the_editor_loads_it() {
    let Some(godot) = godot() else { return };
    let project = Project::scaffold("studio-addon", ProjectTemplate::ThirdPerson3D);

    // Age the project back to before the addon existed, the way a project made by an older
    // Bhippi arrives at `godot_embed::launch`.
    std::fs::remove_dir_all(project.path().join("addons")).expect("remove addons");
    let project_file = project.path().join("project.godot");
    let mut aged =
        GodotProjectFile::parse(&std::fs::read_to_string(&project_file).expect("project reads"))
            .expect("project parses");
    assert!(aged.file.remove("editor_plugins", "enabled"));
    std::fs::write(&project_file, aged.to_text()).expect("aged project writes");

    assert!(
        ensure_studio_addon(project.path()).expect("the addon installs"),
        "an aged project must be brought up to date"
    );
    assert!(
        !ensure_studio_addon(project.path()).expect("the second call runs"),
        "and then left alone"
    );
    assert!(project.path().join(STUDIO_ADDON_CFG_REL).is_file());
    assert!(project.path().join(STUDIO_ADDON_SCRIPT_REL).is_file());
    assert!(
        check_project(project.path(), false).passes(),
        "the addon must not disturb the project's own gates"
    );

    // 1. The script compiles under this Godot.
    let spec = check_script_command(&godot, project.path(), STUDIO_ADDON_SCRIPT_REL);
    let output = run(&spec).expect("godot --check-only runs");
    assert_eq!(
        output.code,
        Some(0),
        "the studio addon failed --check-only:\n{}",
        output.all()
    );

    // 2. A real editor boot loads it: same argv as `godot_embed` spawns, plus the two flags
    //    that make it finish on its own.
    let mut boot = editor_command(&godot, project.path());
    boot.args.push("--headless".to_owned());
    boot.args.push("--quit-after".to_owned());
    boot.args.push(EDITOR_BOOT_FRAMES.to_string());
    println!("argv: {}", boot.display());
    let output = run(&boot).expect("the editor boots");
    let all = output.all();
    assert_eq!(output.code, Some(0), "{all}");
    assert!(
        !all.contains("SCRIPT ERROR") && !all.contains("Failed to load script"),
        "the editor must load the studio addon cleanly:\n{all}"
    );
    assert!(
        all.contains("Initializing plugins"),
        "the editor must have reached its plugin initialisation:\n{all}"
    );

    // The project file still lists it after a real editor has rewritten the project.
    let after = GodotProjectFile::parse(
        &std::fs::read_to_string(&project_file).expect("project reads back"),
    )
    .expect("project parses");
    assert!(
        after
            .editor_plugins()
            .iter()
            .any(|path| path == STUDIO_ADDON_RES_PATH),
        "Godot must keep the plugin enabled: {:?}",
        after.editor_plugins()
    );
}

#[test]
#[ignore = "needs a real Godot 4 install; set BHIPPI_GODOT"]
fn a_headless_run_of_a_scaffolded_project_exits_cleanly() {
    let Some(godot) = godot() else { return };
    let project = Project::scaffold("run", ProjectTemplate::ThirdPerson3D);
    let spec = run_command(
        &godot,
        project.path(),
        &RunOptions {
            headless: true,
            fixed_fps: Some(60),
            quit_after_frames: Some(30),
            user_args: Vec::new(),
        },
    );
    let output = run(&spec).expect("godot --headless runs");
    assert_eq!(output.code, Some(0), "{}", output.all());
    assert!(
        !output.all().contains("SCRIPT ERROR"),
        "a scaffolded project must run without script errors:\n{}",
        output.all()
    );
}

#[test]
#[ignore = "needs a real Godot 4 install; set BHIPPI_GODOT"]
fn a_scripted_playtest_writes_telemetry_and_the_jump_lifts_the_player() {
    let Some(godot) = godot() else { return };
    let project = Project::scaffold("playtest", ProjectTemplate::ThirdPerson3D);
    let inputs_path = project.path().join("playtest-inputs.json");
    let telemetry_path = project.path().join("playtest-telemetry.jsonl");

    // Press and release across several frames: the probe injects during `_process` while
    // the player reads the action in `_physics_process`, so a single-frame press could land
    // on the wrong side of the step and be missed.
    let mut steps = Vec::new();
    for frame in JUMP_FRAME..JUMP_FRAME + 4 {
        steps.push(PlaytestStep::action(frame, "jump", true));
        steps.push(PlaytestStep::action(frame, "jump", false));
    }
    let inputs = PlaytestInputs::new(steps);
    std::fs::write(&inputs_path, inputs.to_json().expect("inputs serialise")).expect("inputs");

    let spec = playtest_command(
        &godot,
        project.path(),
        &inputs_path,
        &telemetry_path,
        PLAYTEST_FRAMES,
    );
    println!("argv: {}", spec.display());
    let output = run(&spec).expect("the playtest runs");
    assert_eq!(output.code, Some(0), "{}", output.all());

    let text = std::fs::read_to_string(&telemetry_path).expect("the probe wrote telemetry");
    let report = TelemetryReport::from_jsonl(&text);
    println!(
        "telemetry: {} samples, done={}, frames={:?}, malformed={}",
        report.sample_count(),
        report.done,
        report.frames,
        report.malformed_lines
    );
    assert!(
        report.sample_count() >= MIN_TELEMETRY_LINES,
        "only {} samples in:\n{text}",
        report.sample_count()
    );
    assert!(report.done, "the probe must write its done line:\n{text}");
    assert_eq!(report.malformed_lines, 0);

    let tracked: Vec<&String> = report.last_positions.keys().collect();
    assert!(
        !tracked.is_empty(),
        "the player is in the bhippi_track group and must be sampled"
    );
    let player = tracked
        .iter()
        .find(|path| path.ends_with("Player"))
        .map(|path| (*path).clone())
        .expect("a tracked node named Player");

    let ys = report.axis_series(&player, 1);
    println!("player y: {ys:?}");
    assert!(ys.len() >= MIN_TELEMETRY_LINES);

    let jump_sample = usize::try_from(JUMP_FRAME).unwrap_or(0) / 6;
    let before = ys.get(jump_sample).copied().unwrap_or_default();
    let after = ys
        .iter()
        .skip(jump_sample)
        .copied()
        .fold(f64::MIN, f64::max);
    assert!(
        after > before + 0.05,
        "the jump should lift the player: before={before}, peak after={after}, series={ys:?}"
    );
    assert!(
        report.vars.contains_key("player_y"),
        "the player script publishes player_y through BhippiProbe.set_var"
    );
}

/// The web export. Skips when the export templates are not installed — they are a separate
/// several-hundred-megabyte download, and a missing one is not a failure of this code.
#[test]
#[ignore = "needs a real Godot 4 install and its export templates"]
fn the_web_preset_exports_a_playable_page() {
    let Some(godot) = godot() else { return };
    let Some(version) = probe_version(&godot) else {
        println!("SKIP: could not read the Godot version");
        return;
    };
    if !export_templates_installed(&version) {
        println!(
            "SKIP: no export templates in {}. Install them from Editor → Manage Export Templates.",
            export_templates_dir(&version)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "the per-user data directory".to_owned())
        );
        return;
    }

    let project = Project::scaffold("export", ProjectTemplate::ThirdPerson3D);
    let output_path = project.path().join(WEB_EXPORT_PATH);
    std::fs::create_dir_all(output_path.parent().unwrap_or(project.path())).expect("export dir");

    let spec = export_command(&godot, project.path(), WEB_PRESET_NAME, &output_path, true);
    println!("argv: {}", spec.display());
    let output = run(&spec).expect("the export runs");
    assert_eq!(output.code, Some(0), "{}", output.all());
    assert!(
        output_path.is_file(),
        "the export must write {}",
        output_path.display()
    );
    for sibling in ["index.pck", "index.wasm", "index.js"] {
        let path = output_path.with_file_name(sibling);
        assert!(path.is_file(), "the web export must write {sibling}");
    }
}
