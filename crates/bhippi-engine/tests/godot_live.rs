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

use bhippi_engine::godot::action::{apply_changeset, lower};
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
use bhippi_engine::godot::hud::{
    self, HudBuildOptions, HUD_GAUGE_SCRIPT_REL, HUD_SCENE_REL, HUD_SCRIPT_REL,
};
use bhippi_engine::godot::live::{announce, focus, focus_scene, LiveEdit, LiveKind};
use bhippi_engine::godot::probe::{PlaytestInputs, PlaytestStep, TelemetryReport};
use bhippi_engine::godot::project::GodotProjectFile;
use bhippi_engine::godot::scaffold::{
    ensure_studio_addon, write_project, ProjectTemplate, MAIN_SCENE_REL, SKETCHFAB_ADDON_CFG_REL,
    SKETCHFAB_ADDON_RES_PATH, SKETCHFAB_ADDON_SCRIPT_REL, STUDIO_ADDON_CFG_REL,
    STUDIO_ADDON_RES_PATH, STUDIO_ADDON_SCRIPT_REL,
};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Frames a live playtest runs for. Twenty samples at the probe's default interval.
const PLAYTEST_FRAMES: u32 = 120;
/// The frame the scripted jump is pressed on — late enough that the player has landed.
const JUMP_FRAME: u32 = 30;
/// The fewest telemetry lines a healthy 120-frame run must produce.
const MIN_TELEMETRY_LINES: usize = 10;
/// Frames a headless editor boot runs for before quitting. Long enough to get past the
/// filesystem scan, plugin initialisation and the editor layout restore.
const EDITOR_BOOT_FRAMES: u32 = 150;
/// Frames a boot that has to see the live follower act runs for. The follower reads on a
/// wall-clock timer (`LIVE_POLL_MS`), and a headless editor burns frames far faster than real
/// time, so this is generous on purpose: too few frames is a flake, not a failure.
const EDITOR_LIVE_FRAMES: u32 = 3_000;
/// The longest the live test waits for the follower to say it is watching. A cold first boot
/// imports every asset before the plugin loads, and that is minutes-slow on a bad day.
const LIVE_SETTLE_SECS: u64 = 120;

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
    for (candidate, _) in candidate_paths(None, None) {
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

/// Block until `needle` shows up in the shared buffer, or `seconds` pass. `true` when it did.
fn wait_for(seen: &Arc<Mutex<String>>, needle: &str, seconds: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if seen
            .lock()
            .map(|held| held.contains(needle))
            .unwrap_or(false)
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
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
fn the_editor_addons_install_into_an_older_project_and_the_editor_loads_them() {
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
    // The Sketchfab strip arrives by the same route, with no migration of its own: a
    // project scaffolded before it existed gains it on the next workspace open (ADR-0055).
    assert!(project.path().join(SKETCHFAB_ADDON_CFG_REL).is_file());
    assert!(project.path().join(SKETCHFAB_ADDON_SCRIPT_REL).is_file());
    assert!(
        check_project(project.path(), false).passes(),
        "the addons must not disturb the project's own gates"
    );

    // 1. Both scripts compile under this Godot.
    for script in [STUDIO_ADDON_SCRIPT_REL, SKETCHFAB_ADDON_SCRIPT_REL] {
        let spec = check_script_command(&godot, project.path(), script);
        let output = run(&spec).expect("godot --check-only runs");
        assert_eq!(
            output.code,
            Some(0),
            "{script} failed --check-only:\n{}",
            output.all()
        );
    }

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
    for res_path in [STUDIO_ADDON_RES_PATH, SKETCHFAB_ADDON_RES_PATH] {
        assert!(
            after.editor_plugins().iter().any(|path| path == res_path),
            "Godot must keep {res_path} enabled: {:?}",
            after.editor_plugins()
        );
    }
}

#[test]
#[ignore = "needs a real Godot 4 install; set BHIPPI_GODOT"]
fn the_editor_follows_bhippis_live_signal_and_opens_the_scene_it_names() {
    // GAD-170/171, ADR-0050. Three claims no fixture can make: that a real editor loads the
    // follower without a script error; that an editor which opens with nothing on screen puts
    // the project's own scene there rather than showing a grey "no scene" hole; and that a
    // signal written *while the editor is already running* is acted on. The last one is why
    // the editor is spawned rather than run to completion — a signal that is already on disk
    // at boot is history, and the follower deliberately does not replay history.
    let Some(godot) = godot() else { return };
    let project = Project::scaffold("live-follow", ProjectTemplate::Empty3D);
    assert!(project.path().join(STUDIO_ADDON_SCRIPT_REL).is_file());

    // The addon compiles against *this* Godot, which is what pins every `EditorInterface`
    // method it calls — a name that does not exist is a parse error, not a silent no-op.
    let checked = run(&check_script_command(
        &godot,
        project.path(),
        STUDIO_ADDON_SCRIPT_REL,
    ))
    .expect("godot --check-only runs");
    assert_eq!(
        checked.code,
        Some(0),
        "the live follower failed --check-only:
{}",
        checked.all()
    );

    // A fresh Godot opens a project's own main scene by itself, which is not the case this
    // ticket is about. The case it is about is an editor that comes up with *nothing* on
    // screen — a project whose main scene is not set yet, or a session where the user closed
    // the last tab — and the studio viewport is then a grey "no scene" hole while the agent
    // builds a level in it. Unsetting the main scene reproduces that exactly, and leaves the
    // live signal as the only thing that can say where the work is.
    let project_file = project.path().join("project.godot");
    let mut settings =
        GodotProjectFile::parse(&std::fs::read_to_string(&project_file).expect("project reads"))
            .expect("project parses");
    assert!(
        settings.file.remove("application", "run/main_scene"),
        "the scaffold must have set a main scene for this to be worth unsetting"
    );
    std::fs::write(&project_file, settings.to_text()).expect("project writes");

    // One batch already applied before this editor session, in the shape
    // `godot_commands::apply_and_journal` announces it.
    let history = announce(
        project.path(),
        &LiveEdit {
            kind: LiveKind::Edit,
            actor: "agent".to_owned(),
            label: "add the sun".to_owned(),
            txn_id: "01LIVE".to_owned(),
            scene: focus_scene(&[MAIN_SCENE_REL.to_owned()]),
            changed_files: vec![MAIN_SCENE_REL.to_owned()],
            focus_nodes: vec!["Sun".to_owned()],
        },
    )
    .expect("the first signal is announced");
    assert_eq!(history.seq, 1);
    assert_eq!(history.scene.as_deref(), Some(MAIN_SCENE_REL));

    let mut spec = editor_command(&godot, project.path());
    spec.args.push("--headless".to_owned());
    spec.args.push("--quit-after".to_owned());
    spec.args.push(EDITOR_LIVE_FRAMES.to_string());
    println!("argv: {}", spec.display());
    let mut child = Command::new(&spec.program)
        .args(&spec.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the editor spawns");

    // Stdout is drained on its own thread, both because a full pipe would wedge the editor
    // and because the signal must not be written until the follower is actually watching.
    // Waiting on the follower's own line rather than on a clock is what stops this test being
    // a stopwatch race with a cold asset import.
    let seen = Arc::new(Mutex::new(String::new()));
    let pump = {
        let seen = Arc::clone(&seen);
        let stdout = child.stdout.take().expect("stdout is piped");
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                println!("{line}");
                if let Ok(mut held) = seen.lock() {
                    held.push_str(&line);
                    held.push('\n');
                }
            }
        })
    };
    // GAD-171 first, and it has to be first: the follower's opening decision is made on a
    // 1.5 s timer from plugin load, so a signal written before that would be what opened the
    // scene and this claim would prove nothing. Waiting on the follower's own line is what
    // orders the two halves of this test without a stopwatch.
    let opened = wait_for(&seen, "[Bhippi Studio] first scene:", LIVE_SETTLE_SECS);
    assert!(
        opened,
        "the follower never reached its first-scene decision within {LIVE_SETTLE_SECS}s:
{}",
        seen.lock().map(|held| held.clone()).unwrap_or_default()
    );

    let news = announce(
        project.path(),
        &LiveEdit {
            kind: LiveKind::Edit,
            actor: "agent".to_owned(),
            label: "light the room".to_owned(),
            txn_id: "02LIVE".to_owned(),
            scene: Some(MAIN_SCENE_REL.to_owned()),
            changed_files: vec![MAIN_SCENE_REL.to_owned()],
            focus_nodes: vec!["Sun".to_owned()],
        },
    )
    .expect("the second signal is announced");
    assert_eq!(news.seq, 2, "the counter continues across sessions");

    // GAD-170: and now the batch that landed *during* the session.
    let showed = wait_for(&seen, "[Bhippi Studio] showing", LIVE_SETTLE_SECS);
    assert!(
        showed,
        "the follower never acted on the signal within {LIVE_SETTLE_SECS}s:
{}",
        seen.lock().map(|held| held.clone()).unwrap_or_default()
    );

    // ADR-0050, the focus half: the agent has not written anything, it is *reading* a scene,
    // and the editor still has to go there. This is the case that fills the long middle of a
    // turn — minutes of the agent looking at a project while the viewport used to sit still.
    // A second scene, so the move is a real one rather than a no-op on the scene already up.
    let other_scene = project.path().join("scenes/hud.tscn");
    std::fs::write(
        &other_scene,
        "[gd_scene load_steps=1 format=3]

[node name=\"Hud\" type=\"Control\"]
",
    )
    .expect("a second scene is written");
    let looked = focus(project.path(), "scenes/hud.tscn", "Reading the scene")
        .expect("the focus is announced")
        .expect("a new scene is a real move");
    assert_eq!(looked.seq, 3, "a focus takes its turn in the same sequence");

    let followed = wait_for(&seen, "[Bhippi Studio] following", LIVE_SETTLE_SECS);
    assert!(
        followed,
        "the follower never acted on the focus within {LIVE_SETTLE_SECS}s:
{}",
        seen.lock().map(|held| held.clone()).unwrap_or_default()
    );

    let status = child.wait().expect("the editor exits");
    pump.join().expect("the output pump finishes");
    let all = seen.lock().map(|held| held.clone()).unwrap_or_default();
    assert_eq!(status.code(), Some(0), "{all}");
    assert!(
        !all.contains("SCRIPT ERROR") && !all.contains("Failed to load script"),
        "the editor must load the live follower cleanly:
{all}"
    );

    // It announced itself, seeded past the signal that was already on disk.
    assert!(
        all.contains("[Bhippi Studio] watching .bhippi/live/editor.json from sequence 1"),
        "the follower must say what it is watching, and from where:
{all}"
    );
    // GAD-171: an editor that came up with no scene open put one there — and the one it
    // chose is the scene Bhippi was last working in, which is the whole point of the ticket.
    assert!(
        all.contains(
            "[Bhippi Studio] first scene: opening res://scenes/main.tscn (nothing was open)"
        ),
        "a first-run editor must not be left on an empty screen:
{all}"
    );
    // GAD-170: the batch that landed *during* the session was shown, by its own label — and
    // the batch from before the session was not replayed as if it had just happened.
    assert!(
        all.contains("[Bhippi Studio] showing res://scenes/main.tscn — light the room"),
        "the follower must act on a signal that arrives while it is running:
{all}"
    );
    assert!(
        !all.contains("add the sun"),
        "a signal from before this session is history, not news:
{all}"
    );
    // ADR-0050: a read moved the editor, and said so in the words of the read rather than
    // claiming a change. "following", not "showing" — the distinction is the point: nothing
    // was written, so nothing was reloaded and no selection was disturbed.
    assert!(
        all.contains("[Bhippi Studio] following res://scenes/hud.tscn — Reading the scene"),
        "the editor must follow the agent while it reads, not only after it writes:
{all}"
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

/// Frames a HUD boot runs for. Enough to get through `_ready()`, the first layout pass and
/// a few `_process` ticks, which is where a bad node path or a missing method shows up.
const HUD_BOOT_FRAMES: u32 = 30;

/// Every HUD preset, built into a real project and compiled by a real Godot.
///
/// This is the claim no fixture can make. The scene is written by the same lowering the
/// studio uses, the script is the one the builder generates, and `--check-only` is the
/// compiler that decides whether the GDScript in `hud.rs` is GDScript at all. A typo in a
/// generated method name is invisible to `cargo test` and fatal in front of a player.
#[test]
#[ignore = "needs a real Godot 4 install; set BHIPPI_GODOT"]
fn every_hud_preset_compiles_in_a_real_godot() {
    let Some(godot) = godot() else { return };
    let project = Project::scaffold("hud-check", ProjectTemplate::ThirdPerson3D);

    for entry in hud::presets() {
        // for_project rather than new: after the first preset the project has a HUD, and a
        // rebuild that could not replace one would be a HUD you can never change.
        let built = hud::build(&HudBuildOptions::for_project(project.path(), entry.id))
            .unwrap_or_else(|error| panic!("{} does not build: {error}", entry.id));
        let changeset = lower(project.path(), &built.batch)
            .unwrap_or_else(|error| panic!("{} does not lower: {}", entry.id, error.error));
        apply_changeset(project.path(), &changeset).expect("the change set writes");

        assert!(
            project.path().join(HUD_SCENE_REL).is_file(),
            "{} wrote no scene",
            entry.id
        );

        let mut scripts = vec![HUD_SCRIPT_REL];
        if built.files.iter().any(|file| file == HUD_GAUGE_SCRIPT_REL) {
            scripts.push(HUD_GAUGE_SCRIPT_REL);
        }
        for script in scripts {
            let spec = check_script_command(&godot, project.path(), script);
            let output = run(&spec).expect("godot --check-only runs");
            assert_eq!(
                output.code,
                Some(0),
                "{} / {script} failed --check-only:\n{}",
                entry.id,
                output.all()
            );
        }
    }
}

/// Every skin, on the HUD that uses the most widget kinds. Skins only change numbers the
/// script bakes in, so one preset proves the generator; what this catches is a colour or a
/// size that stops being a valid literal.
#[test]
#[ignore = "needs a real Godot 4 install; set BHIPPI_GODOT"]
fn every_skin_compiles_in_a_real_godot() {
    let Some(godot) = godot() else { return };
    let project = Project::scaffold("hud-skins", ProjectTemplate::ThirdPerson3D);

    for skin in hud::skins() {
        let built = hud::build(
            &HudBuildOptions::for_project(project.path(), "preset.hud.lap_timer")
                .with_skin(skin.id),
        )
        .unwrap_or_else(|error| panic!("{} does not build: {error}", skin.id));
        let changeset = lower(project.path(), &built.batch)
            .unwrap_or_else(|error| panic!("{} does not lower: {}", skin.id, error.error));
        apply_changeset(project.path(), &changeset).expect("the change set writes");

        for script in [HUD_SCRIPT_REL, HUD_GAUGE_SCRIPT_REL] {
            let spec = check_script_command(&godot, project.path(), script);
            let output = run(&spec).expect("godot --check-only runs");
            assert_eq!(
                output.code,
                Some(0),
                "skin {} / {script} failed --check-only:\n{}",
                skin.id,
                output.all()
            );
        }
    }
}

/// The HUD instanced into the main scene and actually run.
///
/// `--check-only` proves the script parses; only a boot proves the *scene* is coherent —
/// that every node path the script reaches for exists, that the skin applies without a null
/// dereference, and that `_process` survives a frame. Godot prints script errors to stderr
/// and still exits 0, so the assertion is on the output, not only on the code.
#[test]
#[ignore = "needs a real Godot 4 install; set BHIPPI_GODOT"]
fn a_built_hud_boots_without_a_script_error() {
    let Some(godot) = godot() else { return };

    for id in [
        "preset.hud.ammo_health",
        "preset.hud.survival_meters",
        "preset.hud.explore_map",
        "preset.hud.lap_timer",
    ] {
        let name = id.replace("preset.hud.", "hud-boot-");
        let project = Project::scaffold(&name, ProjectTemplate::ThirdPerson3D);
        let built = hud::build(&HudBuildOptions::new(id)).expect("it builds");
        let changeset = lower(project.path(), &built.batch).expect("it lowers");
        apply_changeset(project.path(), &changeset).expect("it writes");

        assert!(
            check_project(project.path(), false).passes(),
            "{id}: a project carrying a HUD must still pass its own gates"
        );

        let run_options = RunOptions {
            headless: true,
            fixed_fps: Some(60),
            quit_after_frames: Some(HUD_BOOT_FRAMES),
            user_args: Vec::new(),
        };
        let output = run(&run_command(&godot, project.path(), &run_options))
            .expect("the headless run starts");
        assert_eq!(
            output.code,
            Some(0),
            "{id} did not exit cleanly:\n{}",
            output.all()
        );

        let noise = output.all();
        for symptom in [
            "SCRIPT ERROR",
            "Invalid access",
            "Invalid call",
            "Parser Error",
            "Node not found",
            "Cannot call method",
        ] {
            assert!(
                !noise.contains(symptom),
                "{id} logged {symptom} on boot:\n{noise}"
            );
        }
    }
}

/// The whole path, on this machine's real asset library: scan the vault, unpack a pack,
/// write the icons and their licence sidecars into a project, build a HUD that uses them,
/// and boot it in Godot.
///
/// Skips when there is no vault, because most machines have none. When there is one, this is
/// the only test that proves the pieces fit: that a Fab pack's PNGs survive the unpack, that
/// the sidecars satisfy the release gate, and that the generated script finds the textures
/// through `load()` rather than through an ext_resource nobody registered.
#[test]
#[ignore = "needs a real Godot 4 install and an asset library on this machine"]
fn the_real_asset_library_dresses_a_hud() {
    let Some(godot) = godot() else { return };
    let Some(vault) = bhippi_engine::fab::default_vault() else {
        println!("SKIP: no asset library on this machine");
        return;
    };

    let packs = bhippi_engine::fab::scan(&vault).expect("the vault reads");
    println!("{} packs in {}", packs.len(), vault.display());
    let Some(pack) = packs.iter().find(|pack| pack.supplies_icons) else {
        println!("SKIP: no pack in the vault carries 2D art");
        return;
    };
    println!("using {} by {}", pack.title, pack.seller);

    let project = Project::scaffold("hud-fab", ProjectTemplate::TopDown2D);
    let preset = hud::preset("preset.hud.lives_score").expect("the platformer HUD exists");
    let requests: Vec<bhippi_engine::fab::IconRequest> = hud::roles_for(preset)
        .into_iter()
        .map(|role| bhippi_engine::fab::IconRequest {
            role: role.id.to_owned(),
            keywords: role
                .keywords
                .iter()
                .map(|word| (*word).to_owned())
                .collect(),
        })
        .collect();

    let imported = bhippi_engine::fab::import_icons(
        pack,
        project.path(),
        hud::HUD_ICON_DIR,
        &requests,
        "Fab Standard License",
    )
    .expect("the import runs");
    println!(
        "imported {:?}, unmatched {:?}",
        imported
            .imported
            .iter()
            .map(|icon| icon.role.as_str())
            .collect::<Vec<_>>(),
        imported.unmatched
    );
    assert!(
        !imported.imported.is_empty(),
        "a 2D icon pack must answer at least one HUD role"
    );

    for icon in &imported.imported {
        let file = project.path().join(&icon.rel_path);
        assert!(file.is_file(), "{} was not written", icon.rel_path);
        let sidecar = project.path().join(format!("{}.meta.json", icon.rel_path));
        let text = std::fs::read_to_string(&sidecar).expect("every icon carries a sidecar");
        assert!(text.contains("Fab Standard License"), "{text}");
    }

    // The project's own art now answers the roles, without anything being passed in by hand.
    let options = HudBuildOptions::for_project(project.path(), preset.id);
    for icon in &imported.imported {
        assert_eq!(
            options.icons.get(&icon.role).map(String::as_str),
            Some(icon.res_path.as_str()),
            "{} did not resolve from the project",
            icon.role
        );
    }

    let built = hud::build(&options).expect("it builds");
    let changeset = lower(project.path(), &built.batch).expect("it lowers");
    apply_changeset(project.path(), &changeset).expect("it writes");

    // INV-074: an imported asset without a licence would block a release. These have one.
    let report = check_project(project.path(), true);
    assert!(
        report.passes(),
        "a HUD dressed from the library must still pass the release gates: {report:?}"
    );

    let run_options = RunOptions {
        headless: true,
        fixed_fps: Some(60),
        quit_after_frames: Some(HUD_BOOT_FRAMES),
        user_args: Vec::new(),
    };
    let output = run(&run_command(&godot, project.path(), &run_options)).expect("it runs");
    assert_eq!(output.code, Some(0), "{}", output.all());
    let noise = output.all();
    for symptom in [
        "SCRIPT ERROR",
        "Invalid call",
        "Node not found",
        "Failed loading resource",
    ] {
        assert!(!noise.contains(symptom), "boot logged {symptom}:\n{noise}");
    }
}
