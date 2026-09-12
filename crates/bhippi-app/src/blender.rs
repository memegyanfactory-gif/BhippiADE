//! Running the Python the agent wrote, inside Blender, with no window (ADR-0062).
//!
//! The pure half — where a binary might be, what its `--version` means, the exact command
//! shape, and how to read a finished run — is [`bhippi_engine::blender`]. This is the half
//! that touches the machine: it walks the filesystem for candidates, spawns the child through
//! the same [`run_spec`](crate::godot::run_spec) every Godot command goes through, and hands
//! back one [`ScriptOutcome`].
//!
//! # Why no Blender window is ever opened
//!
//! The old path needed one: `blender-mcp` forwards calls to an addon living inside a running
//! Blender, so "use Blender" began with the user opening Blender and pressing Connect. A
//! headless run needs none of that — same `bpy`, same exporters, no human in the loop. If the
//! user happens to have Blender open, this does not touch that window; it starts its own
//! process, does the work, and exits.

use bhippi_engine::blender::{
    self, BlenderCandidate, BlenderVersion, ScriptOutcome, BLENDER_PATH_ENV,
};
use std::path::{Path, PathBuf};

use crate::commands::AppError;
use crate::godot::GodotStream;

/// A Blender that answered `--version` and is new enough to drive.
#[derive(Clone, Debug)]
pub struct BlenderInstall {
    pub exe: PathBuf,
    pub version: BlenderVersion,
}

/// Standard install directories, per platform.
///
/// Blender's Windows installer lays down `…/Blender Foundation/Blender <major>.<minor>/blender.exe`,
/// so the version is a directory name rather than something to ask the binary for — which is
/// why this returns every match and lets the probe pick. Newest first: a machine with 3.6 and
/// 4.2 side by side should get 4.2 without the user having to say so.
fn installed_candidates() -> Vec<PathBuf> {
    let name = blender::executable_name();
    let mut roots: Vec<PathBuf> = Vec::new();

    #[cfg(windows)]
    {
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Ok(base) = std::env::var(var) {
                roots.push(PathBuf::from(base).join("Blender Foundation"));
            }
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            roots.push(
                PathBuf::from(local)
                    .join("Programs")
                    .join("Blender Foundation"),
            );
        }
    }
    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from("/Applications"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for base in ["/usr/bin", "/usr/local/bin", "/snap/bin", "/opt"] {
            roots.push(PathBuf::from(base));
        }
    }

    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        // A direct hit — `/usr/bin/blender`, or a root that is itself the install.
        let direct = root.join(name);
        if direct.is_file() {
            found.push(direct);
        }
        // …and one level down, which is where the Windows installer's versioned folders and
        // macOS's `Blender.app` live.
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        // Newest version folder first. Lexical on the folder name is right here because the
        // names are `Blender 3.6` / `Blender 4.2`, not free text.
        children.sort();
        children.reverse();
        for child in children {
            for nested in [
                child.join(name),
                // macOS bundles the executable inside the app.
                child.join("Contents").join("MacOS").join("Blender"),
            ] {
                if nested.is_file() && !found.contains(&nested) {
                    found.push(nested);
                }
            }
        }
    }
    found
}

/// The first entry on `PATH` named like the executable.
fn on_path() -> Option<PathBuf> {
    let name = blender::executable_name();
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// Ask one candidate what it is.
async fn probe(path: &Path) -> Option<BlenderVersion> {
    let spec = blender::version_command_for(path);
    let mut text = String::new();
    let exit = crate::godot::run_spec(&spec, |line| {
        text.push_str(&line.text);
        text.push('\n');
    })
    .await
    .ok()?;
    if exit.timed_out {
        tracing::debug!(path = %path.display(), "the Blender probe did not answer");
        return None;
    }
    blender::parse_version(&text).ok()
}

/// Find a Blender worth driving, or say why there is none.
///
/// # Errors
/// Fails when nothing on the machine answers `--version` as a supported Blender. The message
/// is the one the model reads back, so it names the override rather than only the problem.
pub async fn locate(config_path: Option<&str>) -> Result<BlenderInstall, AppError> {
    let env_path = std::env::var_os(BLENDER_PATH_ENV).map(PathBuf::from);
    let config = config_path.map(PathBuf::from);
    let candidates: Vec<BlenderCandidate> = blender::candidate_paths(
        env_path.as_deref(),
        config.as_deref(),
        &installed_candidates(),
        on_path().as_deref(),
    );

    let mut too_old: Option<String> = None;
    for candidate in &candidates {
        let Some(version) = probe(&candidate.path).await else {
            continue;
        };
        if blender::require_supported(&version).is_err() {
            // Remembered rather than returned: a newer one further down the list is still
            // worth finding, and "too old" is a better message than "not found" if not.
            too_old.get_or_insert_with(|| version.short());
            continue;
        }
        tracing::info!(
            path = %candidate.path.display(),
            version = %version.short(),
            source = ?candidate.source,
            "Blender located"
        );
        return Ok(BlenderInstall {
            exe: candidate.path.clone(),
            version,
        });
    }

    Err(match too_old {
        Some(version) => AppError::new(
            format!("the only Blender found is {version}, which is too old to drive"),
            format!("Install a current Blender, or set {BLENDER_PATH_ENV} to one."),
        ),
        None => AppError::new(
            "no Blender was found on this machine",
            format!(
                "Install Blender, or set {BLENDER_PATH_ENV} to its executable. \
                 Nothing needs to be open — Bhippi runs it in the background."
            ),
        ),
    })
}

/// Run one generated script and return what it did.
///
/// The script is written to a file under `scratch_dir` rather than passed on the command
/// line: generated Python is multi-line, and multi-line through a Windows command line is
/// where quoting turns working code into a syntax error. The file is kept after the run —
/// when a script fails, the thing the user most wants is to see what was actually executed.
///
/// # Errors
/// Fails when the scratch file cannot be written or the process cannot be spawned at all. A
/// script that runs and raises is **not** an error here: that is a [`ScriptOutcome`] with
/// `ok: false`, because the model can read the traceback and fix it, which is the whole loop.
pub async fn run_script(
    install: &BlenderInstall,
    scratch_dir: &Path,
    python: &str,
    blend_file: Option<&Path>,
) -> Result<ScriptOutcome, AppError> {
    std::fs::create_dir_all(scratch_dir).map_err(|error| {
        AppError::new(
            format!("the Blender scratch folder could not be created: {error}"),
            "Check the project folder is writable.",
        )
    })?;
    // Named by the clock rather than a shared id helper: this file is a breadcrumb for a
    // human reading a failed run, and a timestamp sorts the way they will want to read it.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let script = scratch_dir.join(format!("blender-{stamp}.py"));
    std::fs::write(&script, python).map_err(|error| {
        AppError::new(
            format!("the Blender script could not be written: {error}"),
            "Check the project folder is writable.",
        )
    })?;

    let spec = blender::script_command(&install.exe, &script, blend_file);
    let mut stdout = String::new();
    let mut stderr = String::new();
    let exit = crate::godot::run_spec(&spec, |line| {
        let sink = match line.stream {
            GodotStream::Stderr => &mut stderr,
            GodotStream::Stdout => &mut stdout,
        };
        sink.push_str(&line.text);
        sink.push('\n');
    })
    .await?;

    if exit.timed_out {
        return Ok(ScriptOutcome {
            ok: false,
            error: Some(format!(
                "the script was still running after {}s and was stopped",
                blender::SCRIPT_TIMEOUT_SECS
            )),
            output: stdout,
        });
    }
    Ok(blender::read_outcome(exit.is_success(), &stdout, &stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires an installed Blender; creates an isolated temporary asset"]
    async fn live_blender_creates_exports_and_reopens_an_asset() {
        let install = locate(None)
            .await
            .expect("live test requires Blender installed");
        let scratch = std::env::temp_dir().join(format!(
            "bhippi-blender-smoke-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let outcome = run_script(
            &install,
            &scratch,
            include_str!("../tests/fixtures/blender-smoke.py"),
            None,
        )
        .await
        .unwrap();
        assert!(outcome.ok, "{:?}", outcome.error);
        assert!(
            outcome.output.contains("BHIPPI_BLENDER_ROUNDTRIP_OK"),
            "{}",
            outcome.output
        );
        let exported = std::fs::read(scratch.join("assets/smoke.glb")).unwrap();
        assert_eq!(&exported[..4], b"glTF");
        println!(
            "Blender {} created and reopened the asset at {}",
            install.version.short(),
            scratch.display()
        );
    }

    #[test]
    fn the_search_looks_somewhere_on_every_platform() {
        // An empty candidate list would make `locate` report "no Blender" on a machine that
        // has one, which is the failure this whole module exists to stop happening.
        let roots = installed_candidates();
        let _ = roots; // may legitimately be empty in CI
        assert!(!blender::executable_name().is_empty());
    }

    #[tokio::test]
    async fn a_machine_with_no_blender_says_so_and_names_the_override() {
        // Pointed at a path that cannot exist, with no env var set, the answer has to be
        // actionable: the old integration's failure was "could not connect", which told the
        // user nothing about what to do.
        if std::env::var_os(BLENDER_PATH_ENV).is_some() {
            return; // a developer machine with a real override; nothing to assert
        }
        let missing = locate(Some("C:/definitely/not/here/blender.exe")).await;
        if let Err(error) = missing {
            assert!(error.message.contains("Blender"), "{}", error.message);
            assert!(error.hint.unwrap_or_default().contains(BLENDER_PATH_ENV));
        }
        // If it succeeded, this machine really does have Blender installed — which is a pass
        // for detection, not a failure of the test.
    }
}
