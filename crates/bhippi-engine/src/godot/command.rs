//! Godot CLI command **builders**. Pure: every function returns a [`CommandSpec`] and
//! nothing here spawns a process — `bhippi_app::godot` does that.
//!
//! # The flags, and what each one means
//!
//! | flag | meaning |
//! |---|---|
//! | `--version` | print `major.minor.patch.status.official.commit` and exit |
//! | `--headless` | run with the dummy display and audio drivers: no window, no GPU |
//! | `--path <dir>` | treat `<dir>` as the project directory (where `project.godot` is) |
//! | `--script <res://…>` | run a script instead of the project's main scene |
//! | `--check-only` | with `--script`, parse the script and exit; non-zero on a parse error |
//! | `--editor` | open the project in the editor rather than running it |
//! | `--fixed-fps <n>` | force a fixed delta and stop syncing to real time — deterministic |
//! | `--quit-after <n>` | quit after `n` iterations (frames); `0` disables |
//! | `--export-release <preset> <out>` | export with `<preset>` from `export_presets.cfg` |
//! | `--export-debug <preset> <out>` | the same, with the debug templates |
//! | `--` | end of Godot's own flags; everything after reaches `OS.get_cmdline_user_args()` |
//!
//! The `--` separator is why the probe can read `--bhippi-inputs=…`: Godot would otherwise
//! try to interpret those as its own options and refuse to start.
//!
//! These are checked against a real Godot 4.7 in `tests/godot_live.rs`, which is `#[ignore]`
//! so the suite still runs on a machine with no Godot installed.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// How long a command may run before the runner kills it. `0` means "no limit", which is
/// what an interactive editor session needs.
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;
/// A `--version` probe answers in milliseconds; anything longer is a wedged binary.
pub const VERSION_TIMEOUT_SECS: u64 = 20;
/// Parsing one script is fast, but a cold first run builds the import cache.
pub const CHECK_TIMEOUT_SECS: u64 = 120;
/// Exports compress the whole project and legitimately take minutes.
pub const EXPORT_TIMEOUT_SECS: u64 = 900;
/// The editor is interactive: it ends when the user closes it.
pub const EDITOR_TIMEOUT_SECS: u64 = 0;
/// The fixed frame rate every playtest runs at, so a recorded input file means the same
/// thing on every machine.
pub const PLAYTEST_FIXED_FPS: u32 = 60;
/// The longest playtest Bhippi will ask for: ten minutes of simulated time.
pub const MAX_PLAYTEST_FRAMES: u32 = 36_000;
/// Wall-clock headroom per playtest frame, in milliseconds. A headless frame at fixed fps
/// costs far less than this; the margin is what turns a hung game into a timeout.
pub const PLAYTEST_MS_PER_FRAME: u64 = 20;
/// The floor for a playtest timeout, however few frames were asked for — Godot's own
/// start-up (import scan, first-run cache) dominates a short run.
pub const PLAYTEST_MIN_TIMEOUT_SECS: u64 = 90;

/// The user-arg flag the probe reads its scripted input from.
pub const INPUTS_ARG: &str = "--bhippi-inputs=";
/// The user-arg flag the probe writes telemetry to.
pub const TELEMETRY_ARG: &str = "--bhippi-telemetry=";
/// The environment variable mirroring [`INPUTS_ARG`], for a game that prefers `OS.get_environment`.
pub const INPUTS_ENV: &str = "BHIPPI_INPUTS";
/// The environment variable mirroring [`TELEMETRY_ARG`].
pub const TELEMETRY_ENV: &str = "BHIPPI_TELEMETRY";

/// Everything needed to run one Godot command, and nothing about how to run it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Extra environment entries, on top of whatever scrubbed base the runner uses.
    pub env: Vec<(String, String)>,
    /// Seconds before the runner kills the child; `0` means no limit.
    pub timeout_secs: u64,
}

impl CommandSpec {
    fn new(program: &Path, args: Vec<String>, timeout_secs: u64) -> Self {
        Self {
            program: program.to_path_buf(),
            args,
            cwd: None,
            env: Vec::new(),
            timeout_secs,
        }
    }

    #[must_use]
    pub fn with_cwd(mut self, cwd: &Path) -> Self {
        self.cwd = Some(cwd.to_path_buf());
        self
    }

    #[must_use]
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_owned(), value.to_owned()));
        self
    }

    /// The argv as one line, for logs and for the Activity Dock. Not for a shell — the
    /// runner passes `args` through explicitly and never builds a command string.
    #[must_use]
    pub fn display(&self) -> String {
        let mut parts = vec![self.program.display().to_string()];
        parts.extend(self.args.iter().cloned());
        parts.join(" ")
    }
}

/// What a run should do.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RunOptions {
    pub headless: bool,
    pub fixed_fps: Option<u32>,
    pub quit_after_frames: Option<u32>,
    /// Passed after `--`, so they land in `OS.get_cmdline_user_args()`.
    #[serde(default)]
    pub user_args: Vec<String>,
}

fn path_arg(path: &Path) -> String {
    path.display().to_string()
}

/// `godot --version`
#[must_use]
pub fn version_command(godot: &Path) -> CommandSpec {
    CommandSpec::new(godot, vec!["--version".to_owned()], VERSION_TIMEOUT_SECS)
}

/// `godot --headless --path <root> --script res://<script> --check-only`
///
/// The script is normalised to a `res://` path: Godot resolves `--script` against the
/// project set by `--path`, and a bare relative path is resolved against the *working
/// directory* instead, which is how "the file is right there" turns into "file not found".
///
/// **Autoloads do not exist under `--check-only`.** Verified on 4.7.1: a script that names
/// a singleton directly (`BhippiProbe.set_var(…)`) fails to compile with "Identifier not
/// found", because autoloads are registered when the main loop starts and this mode never
/// starts one. Scripts that need a singleton must look it up —
/// `get_node_or_null("/root/BhippiProbe")` — which is what the scaffold's templates do.
#[must_use]
pub fn check_script_command(godot: &Path, project_root: &Path, script_rel: &str) -> CommandSpec {
    CommandSpec::new(
        godot,
        vec![
            "--headless".to_owned(),
            "--path".to_owned(),
            path_arg(project_root),
            "--script".to_owned(),
            super::rel_to_res(script_rel),
            "--check-only".to_owned(),
        ],
        CHECK_TIMEOUT_SECS,
    )
}

/// `godot --path <root> [--headless] [--fixed-fps n] [--quit-after n] [-- user args…]`
#[must_use]
pub fn run_command(godot: &Path, project_root: &Path, run: &RunOptions) -> CommandSpec {
    let mut args = vec!["--path".to_owned(), path_arg(project_root)];
    if run.headless {
        args.push("--headless".to_owned());
    }
    if let Some(fps) = run.fixed_fps {
        args.push("--fixed-fps".to_owned());
        args.push(fps.to_string());
    }
    if let Some(frames) = run.quit_after_frames {
        args.push("--quit-after".to_owned());
        args.push(frames.to_string());
    }
    if !run.user_args.is_empty() {
        args.push("--".to_owned());
        args.extend(run.user_args.iter().cloned());
    }
    CommandSpec::new(godot, args, DEFAULT_TIMEOUT_SECS)
}

/// `godot --path <root> --editor`
#[must_use]
pub fn editor_command(godot: &Path, project_root: &Path) -> CommandSpec {
    CommandSpec::new(
        godot,
        vec![
            "--path".to_owned(),
            path_arg(project_root),
            "--editor".to_owned(),
        ],
        EDITOR_TIMEOUT_SECS,
    )
}

/// `godot --headless --path <root> --export-release|--export-debug <preset> <output>`
#[must_use]
pub fn export_command(
    godot: &Path,
    project_root: &Path,
    preset: &str,
    output: &Path,
    release: bool,
) -> CommandSpec {
    let mode = if release {
        "--export-release"
    } else {
        "--export-debug"
    };
    CommandSpec::new(
        godot,
        vec![
            "--headless".to_owned(),
            "--path".to_owned(),
            path_arg(project_root),
            mode.to_owned(),
            preset.to_owned(),
            path_arg(output),
        ],
        EXPORT_TIMEOUT_SECS,
    )
}

/// A deterministic headless playtest: fixed 60 fps, a hard frame budget, the probe's input
/// and telemetry files passed both as user args and as environment variables.
///
/// `frames` is clamped to [`MAX_PLAYTEST_FRAMES`] — an agent that asks for a million frames
/// gets ten minutes of simulation and a runner that still returns.
#[must_use]
pub fn playtest_command(
    godot: &Path,
    project_root: &Path,
    inputs_file: &Path,
    telemetry_file: &Path,
    frames: u32,
) -> CommandSpec {
    let frames = frames.clamp(1, MAX_PLAYTEST_FRAMES);
    let inputs = path_arg(inputs_file);
    let telemetry = path_arg(telemetry_file);
    let mut spec = run_command(
        godot,
        project_root,
        &RunOptions {
            headless: true,
            fixed_fps: Some(PLAYTEST_FIXED_FPS),
            quit_after_frames: Some(frames),
            user_args: vec![
                format!("{INPUTS_ARG}{inputs}"),
                format!("{TELEMETRY_ARG}{telemetry}"),
            ],
        },
    );
    spec.env.push((INPUTS_ENV.to_owned(), inputs));
    spec.env.push((TELEMETRY_ENV.to_owned(), telemetry));
    spec.timeout_secs = playtest_timeout_secs(frames);
    spec
}

/// The wall-clock budget for a `frames`-long playtest.
#[must_use]
pub fn playtest_timeout_secs(frames: u32) -> u64 {
    let budget = u64::from(frames) * PLAYTEST_MS_PER_FRAME / 1_000;
    budget.max(PLAYTEST_MIN_TIMEOUT_SECS)
}

#[cfg(test)]
mod tests {
    use super::{
        check_script_command, editor_command, export_command, playtest_command,
        playtest_timeout_secs, run_command, version_command, RunOptions, MAX_PLAYTEST_FRAMES,
        PLAYTEST_FIXED_FPS, PLAYTEST_MIN_TIMEOUT_SECS,
    };
    use std::path::Path;

    fn godot() -> &'static Path {
        Path::new("/godot")
    }

    fn root() -> &'static Path {
        Path::new("/game")
    }

    #[test]
    fn version_and_editor_argv_are_exact() {
        assert_eq!(version_command(godot()).args, vec!["--version"]);
        assert_eq!(
            editor_command(godot(), root()).args,
            vec!["--path", "/game", "--editor"]
        );
    }

    #[test]
    fn check_only_normalises_the_script_to_a_res_path() {
        let spec = check_script_command(godot(), root(), "scripts/player.gd");
        assert_eq!(
            spec.args,
            vec![
                "--headless",
                "--path",
                "/game",
                "--script",
                "res://scripts/player.gd",
                "--check-only",
            ]
        );
        // An already-res:// path is left alone rather than doubled up.
        let already = check_script_command(godot(), root(), "res://scripts/player.gd");
        assert_eq!(already.args[4], "res://scripts/player.gd");
    }

    #[test]
    fn a_windowed_run_carries_nothing_it_was_not_asked_for() {
        let spec = run_command(godot(), root(), &RunOptions::default());
        assert_eq!(spec.args, vec!["--path", "/game"]);
        assert!(spec.env.is_empty());
    }

    #[test]
    fn user_args_land_after_the_double_dash() {
        let spec = run_command(
            godot(),
            root(),
            &RunOptions {
                headless: true,
                fixed_fps: Some(30),
                quit_after_frames: Some(120),
                user_args: vec!["--level=2".to_owned()],
            },
        );
        assert_eq!(
            spec.args,
            vec![
                "--path",
                "/game",
                "--headless",
                "--fixed-fps",
                "30",
                "--quit-after",
                "120",
                "--",
                "--level=2",
            ]
        );
    }

    #[test]
    fn export_argv_matches_godots_flag_order() {
        let release = export_command(
            godot(),
            root(),
            "Web",
            Path::new("/game/export/web/index.html"),
            true,
        );
        assert_eq!(
            release.args,
            vec![
                "--headless",
                "--path",
                "/game",
                "--export-release",
                "Web",
                "/game/export/web/index.html",
            ]
        );
        let debug = export_command(godot(), root(), "Web", Path::new("/out"), false);
        assert_eq!(debug.args[3], "--export-debug");
    }

    #[test]
    fn a_playtest_is_deterministic_and_tells_the_probe_where_to_write() {
        let spec = playtest_command(
            godot(),
            root(),
            Path::new("/tmp/in.json"),
            Path::new("/tmp/out.jsonl"),
            120,
        );
        assert_eq!(
            spec.args,
            vec![
                "--path",
                "/game",
                "--headless",
                "--fixed-fps",
                &PLAYTEST_FIXED_FPS.to_string(),
                "--quit-after",
                "120",
                "--",
                "--bhippi-inputs=/tmp/in.json",
                "--bhippi-telemetry=/tmp/out.jsonl",
            ]
        );
        assert_eq!(
            spec.env,
            vec![
                ("BHIPPI_INPUTS".to_owned(), "/tmp/in.json".to_owned()),
                ("BHIPPI_TELEMETRY".to_owned(), "/tmp/out.jsonl".to_owned()),
            ]
        );
        assert_eq!(spec.timeout_secs, PLAYTEST_MIN_TIMEOUT_SECS);
    }

    #[test]
    fn a_runaway_frame_count_is_clamped_rather_than_trusted() {
        let spec = playtest_command(
            godot(),
            root(),
            Path::new("/in"),
            Path::new("/out"),
            u32::MAX,
        );
        let quit_after = spec
            .args
            .iter()
            .position(|arg| arg == "--quit-after")
            .and_then(|at| spec.args.get(at + 1))
            .cloned()
            .unwrap_or_default();
        assert_eq!(quit_after, MAX_PLAYTEST_FRAMES.to_string());
        assert_eq!(
            playtest_timeout_secs(MAX_PLAYTEST_FRAMES),
            u64::from(MAX_PLAYTEST_FRAMES) * 20 / 1_000
        );
    }
}
