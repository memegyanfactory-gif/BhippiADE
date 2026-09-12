//! Driving Blender with Python, headless. **This module never spawns a process.**
//!
//! It is a [`CommandSpec`](crate::godot::command::CommandSpec) builder and a pair of pure
//! parsers, exactly like [`godot::detect`](crate::godot::detect) — running the command lives
//! in `bhippi_app::blender`, where process execution belongs.
//!
//! # Why this exists (ADR-0062)
//!
//! Bhippi's first Blender integration was `blender-mcp`: an MCP server that talks to an addon
//! running **inside an already-open Blender**. The owner's run reported it honestly —
//! *"Could not connect to Blender — make sure the Blender addon is running"* — and there was
//! nothing the agent could do about it, because the missing piece was a human opening Blender
//! and clicking Connect in a sidebar panel.
//!
//! Blender has driven itself from Python since long before any of that. `blender --background
//! --python script.py` runs a file with no window, no addon and no clicking, and `bpy` is the
//! same API the addon was forwarding calls to. So the agent writes Python and Bhippi runs it:
//! one hop instead of three, and nothing to set up.
//!
//! # What a caller must still do
//!
//! Nothing here validates the Python. A script is arbitrary code running with the user's own
//! permissions — the gate is that the user asked for Blender work, the same gate every other
//! write in a turn passes, and `bhippi_app` is where that is enforced. What this module does
//! guarantee is that the *command* is the right shape: background, no user startup file
//! interfering, and a timeout, so a script with an infinite loop in it is a failed step rather
//! than a wedged app.

use crate::error::{EngineError, Result};
use crate::godot::command::CommandSpec;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// The environment variable that overrides detection with an explicit path.
pub const BLENDER_PATH_ENV: &str = "BHIPPI_BLENDER";

/// The oldest Blender Bhippi will drive.
///
/// 3.0 is where `bpy` settled into the shape the agent is told to write against — the 2.8
/// series renamed enough of the API that a script written for 4.x is not merely wrong on it
/// but wrong in ways whose error messages do not point at the version.
pub const BLENDER_MINIMUM: (u32, u32) = (3, 0);

/// A `--version` probe answers in milliseconds; anything longer is a wedged binary.
pub const VERSION_TIMEOUT_SECS: u64 = 30;

/// How long one generated script may run before the runner kills it.
///
/// Generous, because a subdivision-heavy mesh or a glTF export of a real scene legitimately
/// takes tens of seconds on a cold start. Bounded, because the commonest failure mode of
/// generated Python is a loop that never ends, and an unbounded child would take the turn —
/// and the app's patience — with it.
pub const SCRIPT_TIMEOUT_SECS: u64 = 300;

/// How a candidate binary was found. Ordered by the priority detection walks them in.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BlenderSource {
    /// `BHIPPI_BLENDER` — an explicit override always wins.
    EnvVar,
    /// The path saved in Bhippi's own settings.
    Config,
    /// A standard install location for the platform.
    Installed,
    /// Found on `PATH`.
    Path,
}

/// One place a Blender binary might be, and how it was proposed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlenderCandidate {
    pub path: PathBuf,
    pub source: BlenderSource,
}

/// A parsed `blender --version` line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlenderVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl BlenderVersion {
    #[must_use]
    pub fn short(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    /// Whether Bhippi will drive this build.
    #[must_use]
    pub fn is_supported(&self) -> bool {
        (self.major, self.minor) >= BLENDER_MINIMUM
    }
}

/// Parse the first line of `blender --version` — `Blender 4.2.1` or `Blender 4.2.1 LTS`.
///
/// # Errors
/// Fails when the output carries no `Blender <major>.<minor>` line at all, which is what a
/// path pointing at something that is not Blender looks like.
pub fn parse_version(output: &str) -> Result<BlenderVersion> {
    for line in output.lines() {
        let Some(rest) = line.trim().strip_prefix("Blender ") else {
            continue;
        };
        // `4.2.1 LTS` and `4.2.1-beta` both answer the same question; take the leading
        // numeric run and ignore whatever the build appended to it.
        let numeric: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let mut parts = numeric.split('.').filter(|part| !part.is_empty());
        let major = parts.next().and_then(|p| p.parse().ok());
        let minor = parts.next().and_then(|p| p.parse().ok());
        if let (Some(major), Some(minor)) = (major, minor) {
            return Ok(BlenderVersion {
                major,
                minor,
                patch: parts.next().and_then(|p| p.parse().ok()).unwrap_or(0),
            });
        }
    }
    Err(EngineError::Build(
        format!(
            "that binary did not report a Blender version: {}",
            output.lines().next().unwrap_or("(no output)").trim()
        ),
        Some(format!(
            "Point Settings -> Blender (or {BLENDER_PATH_ENV}) at a real Blender executable."
        )),
    ))
}

/// Reject a build too old to run the API the agent is told to write against.
///
/// # Errors
/// Fails for anything below [`BLENDER_MINIMUM`].
pub fn require_supported(version: &BlenderVersion) -> Result<()> {
    if version.is_supported() {
        return Ok(());
    }
    let (major, minor) = BLENDER_MINIMUM;
    Err(EngineError::Build(
        format!(
            "Blender {} is older than the supported {major}.{minor}",
            version.short()
        ),
        Some(format!(
            "Install Blender {major}.{minor} or newer and point Settings -> Blender (or {BLENDER_PATH_ENV}) at it."
        )),
    ))
}

/// The `--version` probe for one candidate.
#[must_use]
pub fn version_command_for(path: &Path) -> CommandSpec {
    spec(path, vec!["--version".to_owned()], VERSION_TIMEOUT_SECS)
}

/// Every place to look for Blender, best first.
///
/// `config_path` is the user's explicit setting and `installed_roots` the platform's standard
/// install directories — passed in rather than read here so this stays pure and the app owns
/// every filesystem decision. Duplicates are dropped, keeping the highest-priority source for
/// a path proposed twice.
#[must_use]
pub fn candidate_paths(
    env_path: Option<&Path>,
    config_path: Option<&Path>,
    installed: &[PathBuf],
    on_path: Option<&Path>,
) -> Vec<BlenderCandidate> {
    let mut out: Vec<BlenderCandidate> = Vec::new();
    let mut push = |path: &Path, source: BlenderSource| {
        if out.iter().any(|held| held.path == path) {
            return;
        }
        out.push(BlenderCandidate {
            path: path.to_path_buf(),
            source,
        });
    };
    if let Some(path) = env_path {
        push(path, BlenderSource::EnvVar);
    }
    if let Some(path) = config_path {
        push(path, BlenderSource::Config);
    }
    for path in installed {
        push(path, BlenderSource::Installed);
    }
    if let Some(path) = on_path {
        push(path, BlenderSource::Path);
    }
    out
}

/// The executable's file name on this platform.
#[must_use]
pub const fn executable_name() -> &'static str {
    if cfg!(windows) {
        "blender.exe"
    } else {
        "blender"
    }
}

/// Run one Python file with no window and no user interface.
///
/// The flag order matters and is not stylistic:
///
/// | flag | why |
/// |---|---|
/// | `--background` | no window, no GPU, no event loop — the whole point |
/// | `<blend>` | an existing `.blend` must come **before** `--python`, or Blender loads the file after the script has already run and discards everything the script built |
/// | `--factory-startup` | omitted deliberately: it would also drop the user's enabled add-ons, and the glTF exporter the agent is told to use is one |
/// | `--python <file>` | the generated script |
/// | `--` | ends Blender's own flags, so nothing after is parsed as one |
///
/// A file rather than `--python-expr`: a generated script is multi-line Python, and passing
/// that through a command line is where quoting and newline handling go wrong on Windows in
/// ways that look like syntax errors in the model's code.
#[must_use]
pub fn script_command(blender: &Path, script: &Path, blend_file: Option<&Path>) -> CommandSpec {
    let mut args = vec!["--background".to_owned()];
    if let Some(blend) = blend_file {
        args.push(blend.to_string_lossy().into_owned());
    }
    args.push("--python".to_owned());
    args.push(script.to_string_lossy().into_owned());
    args.push("--".to_owned());
    spec(blender, args, SCRIPT_TIMEOUT_SECS)
}

/// A [`CommandSpec`] built here rather than in `godot::command`, whose constructor is private
/// to that module — and whose doc comment is a table of Godot's own flags. Every field is
/// public, so this needs no widening of that module's surface to drive a different binary.
fn spec(program: &Path, args: Vec<String>, timeout_secs: u64) -> CommandSpec {
    CommandSpec {
        program: program.to_path_buf(),
        args,
        cwd: None,
        env: Vec::new(),
        timeout_secs,
    }
}

/// What a finished script run is worth telling the model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptOutcome {
    pub ok: bool,
    /// The Python traceback's last line when there was one — the thing that actually says
    /// what went wrong, rather than the forty lines of `bpy` internals above it.
    pub error: Option<String>,
    /// What the script printed, trimmed of Blender's own boilerplate.
    pub output: String,
}

/// Lines Blender prints on every single run, which carry no information about the script and
/// would otherwise be most of what the model reads back.
const NOISE_PREFIXES: &[&str] = &[
    "Blender quit",
    "Color management:",
    "Read prefs:",
    "found bundled python:",
    "Writing userprefs:",
    "Warning: Falling back to the standard locale",
    "AL lib:",
];

/// Turn a finished run into the observation the model gets back.
///
/// Blender reports a Python failure on **stdout** and still exits 0 in some builds, so the
/// exit status alone is not the answer: a traceback in the output is a failure whatever the
/// process said on its way out.
#[must_use]
pub fn read_outcome(status_ok: bool, stdout: &str, stderr: &str) -> ScriptOutcome {
    let combined = if stderr.trim().is_empty() {
        stdout.to_owned()
    } else {
        format!("{stdout}\n{stderr}")
    };

    let traceback = combined.contains("Traceback (most recent call last)");
    // The last non-empty line of a traceback is the exception and its message; everything
    // above it is the frame stack, which is about Blender's internals, not the script.
    let error = traceback
        .then(|| {
            combined
                .lines()
                .map(str::trim_end)
                .rfind(|line| !line.trim().is_empty() && !line.trim_start().starts_with("File \""))
                .map(str::to_owned)
        })
        .flatten();

    let output = combined
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .filter(|line| {
            !NOISE_PREFIXES
                .iter()
                .any(|noise| line.trim_start().starts_with(noise))
        })
        .collect::<Vec<_>>()
        .join("\n");

    ScriptOutcome {
        ok: status_ok && !traceback,
        error,
        output,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_line_is_read_whatever_the_build_appended_to_it() {
        for (line, expected) in [
            ("Blender 4.2.1", (4, 2, 1)),
            ("Blender 4.2.1 LTS", (4, 2, 1)),
            ("Blender 3.6.0-beta", (3, 6, 0)),
            ("Blender 4.0", (4, 0, 0)),
        ] {
            let version = parse_version(line).unwrap_or_else(|e| panic!("{line}: {e}"));
            assert_eq!(
                (version.major, version.minor, version.patch),
                expected,
                "{line}"
            );
        }
    }

    #[test]
    fn the_version_survives_the_banner_blender_prints_above_it() {
        // A real `--version` is several lines; the one that matters is not always first.
        let real = "\tbuild date: 2024-07-16\nBlender 4.2.0 LTS\n\tbuild hash: a51f293548ad";
        assert_eq!(parse_version(real).expect("a version").minor, 2);
    }

    #[test]
    fn something_that_is_not_blender_is_refused_rather_than_guessed() {
        let error = parse_version("Python 3.11.9").expect_err("not Blender");
        assert!(format!("{error}").contains("did not report a Blender version"));
    }

    #[test]
    fn a_build_older_than_the_api_the_agent_writes_against_is_refused() {
        let old = BlenderVersion {
            major: 2,
            minor: 93,
            patch: 0,
        };
        assert!(!old.is_supported());
        assert!(require_supported(&old).is_err());
        let current = BlenderVersion {
            major: 4,
            minor: 2,
            patch: 1,
        };
        assert!(require_supported(&current).is_ok());
    }

    #[test]
    fn an_existing_blend_file_is_opened_before_the_script_runs() {
        // The whole reason the order is pinned: Blender loads a `.blend` named after
        // `--python` *after* the script, throwing away everything the script built.
        let spec = script_command(
            Path::new("blender.exe"),
            Path::new("build.py"),
            Some(Path::new("scene.blend")),
        );
        let blend = spec.args.iter().position(|a| a == "scene.blend");
        let python = spec.args.iter().position(|a| a == "--python");
        assert!(blend < python, "{:?}", spec.args);
        assert_eq!(spec.args[0], "--background", "never opens a window");
    }

    #[test]
    fn a_script_with_no_blend_file_starts_from_blenders_own_default_scene() {
        let spec = script_command(Path::new("blender"), Path::new("build.py"), None);
        assert_eq!(spec.args[0], "--background");
        assert_eq!(spec.args[1], "--python");
        assert!(spec.timeout_secs > 0, "a runaway script must be killable");
    }

    #[test]
    fn the_higher_priority_source_wins_a_path_proposed_twice() {
        let same = PathBuf::from("/opt/blender");
        let found = candidate_paths(
            Some(&same),
            Some(&same),
            std::slice::from_ref(&same),
            Some(&same),
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].source, BlenderSource::EnvVar);
    }

    #[test]
    fn candidates_are_walked_in_priority_order() {
        let found = candidate_paths(
            Some(Path::new("/env")),
            Some(Path::new("/config")),
            &[PathBuf::from("/installed")],
            Some(Path::new("/on-path")),
        );
        let sources: Vec<_> = found.iter().map(|c| c.source).collect();
        assert_eq!(
            sources,
            vec![
                BlenderSource::EnvVar,
                BlenderSource::Config,
                BlenderSource::Installed,
                BlenderSource::Path,
            ]
        );
    }

    #[test]
    fn a_traceback_is_a_failure_even_when_blender_exits_zero() {
        // Blender genuinely does this: the script raises, the message goes to stdout, and
        // the process still reports success. Trusting the exit code would report a prop that
        // was never built as built.
        let out = "Traceback (most recent call last):\n  File \"<string>\", line 3, in <module>\nAttributeError: 'NoneType' object has no attribute 'name'";
        let outcome = read_outcome(true, out, "");
        assert!(!outcome.ok, "a traceback is a failure");
        assert_eq!(
            outcome.error.as_deref(),
            Some("AttributeError: 'NoneType' object has no attribute 'name'"),
            "the exception, not the frame stack above it",
        );
    }

    #[test]
    fn a_clean_run_keeps_what_the_script_printed_and_drops_the_boilerplate() {
        let outcome = read_outcome(
            true,
            "Read prefs: C:\\Users\\a\\prefs.blend\nfound bundled python: /usr/share\nwrote assets/models/lamp.glb\n\nBlender quit",
            "",
        );
        assert!(outcome.ok);
        assert_eq!(outcome.output, "wrote assets/models/lamp.glb");
        assert!(outcome.error.is_none());
    }

    #[test]
    fn a_nonzero_exit_with_no_traceback_is_still_a_failure() {
        // A binary that will not start at all prints nothing useful and fails on status.
        let outcome = read_outcome(false, "", "blender: error while loading shared libraries");
        assert!(!outcome.ok);
        assert!(outcome.output.contains("shared libraries"));
    }
}
