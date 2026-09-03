//! Running Godot. The only place in Bhippi that spawns the engine as a process.
//!
//! `bhippi_engine::godot::command` decides *what* to run and stays a pure library; this
//! module decides *how*: an explicit argv with no shell anywhere near it, a scrubbed
//! environment (INV-003, mirroring `bhippi_providers::command`), stdout and stderr streamed
//! a line at a time so the Output Log fills while the game is still running, and a timeout
//! that actually kills the child rather than dropping the future and leaving it behind.
//!
//! Godot is not scrubbed quite as hard as a vendor CLI: it needs the platform's display
//! variables to open a window at all, and a headless run still reads `HOME`/`APPDATA` to
//! find its own editor settings and export templates. Everything outside that list — API
//! keys above all — is dropped.
//!
//! `CREATE_NO_WINDOW` is deliberately *not* set: the whole point of a windowed run is that
//! the user sees the game.

use crate::commands::AppError;
use bhippi_engine::godot::command::{version_command, CommandSpec};
use bhippi_engine::godot::detect::{
    candidate_paths, is_supported, pair_windows_binaries, parse_version, GodotInstall,
    GodotInstallSource,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::watch;

/// The environment a Godot child inherits. Everything here is needed for Godot to find its
/// own settings, its temp space or a display; nothing here can carry a credential.
const SAFE_ENV_KEYS: &[&str] = &[
    "APPDATA",
    "COMSPEC",
    "DBUS_SESSION_BUS_ADDRESS",
    "DISPLAY",
    "HOME",
    "HOMEDRIVE",
    "HOMEPATH",
    "LANG",
    "LC_ALL",
    "LOCALAPPDATA",
    "NUMBER_OF_PROCESSORS",
    "OS",
    "PATH",
    "PATHEXT",
    "PROCESSOR_ARCHITECTURE",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "SYSTEMDRIVE",
    "SYSTEMROOT",
    "TEMP",
    "TMP",
    "TMPDIR",
    "USERPROFILE",
    "WAYLAND_DISPLAY",
    "WINDIR",
    "XAUTHORITY",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_TYPE",
];

/// How long a kill is given to take effect before the runner stops waiting on the child.
const KILL_GRACE: Duration = Duration::from_secs(5);

/// Which stream a line came from.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GodotStream {
    Stdout,
    Stderr,
}

/// One line of engine output.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GodotOutputLine {
    pub stream: GodotStream,
    pub text: String,
}

/// How a run ended.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GodotExit {
    /// `None` when the process was killed rather than exiting on its own.
    pub code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
}

impl GodotExit {
    /// True when Godot finished on its own with status 0.
    #[must_use]
    pub fn is_success(self) -> bool {
        !self.timed_out && self.code == Some(0)
    }
}

/// A handle for stopping a running game.
///
/// A watch channel rather than a `Child`: the child is owned by the future doing the
/// streaming, and handing a second owner the ability to reap it is how you get a runner
/// that waits forever on a process someone else already collected.
#[derive(Clone, Debug)]
pub struct GodotProcessHandle {
    stop: watch::Sender<bool>,
}

impl GodotProcessHandle {
    /// Ask the running game to stop. Returns `false` when the run has already ended.
    pub fn kill(&self) -> bool {
        self.stop.send(true).is_ok()
    }

    #[must_use]
    pub fn is_stopped(&self) -> bool {
        *self.stop.borrow()
    }
}

/// The receiving half handed to [`run_spec_with_stop`].
#[derive(Clone, Debug)]
pub struct GodotStopSignal {
    stop: watch::Receiver<bool>,
}

/// Make a stop handle and the signal to pass into a run.
#[must_use]
pub fn stop_channel() -> (GodotProcessHandle, GodotStopSignal) {
    let (sender, receiver) = watch::channel(false);
    (
        GodotProcessHandle { stop: sender },
        GodotStopSignal { stop: receiver },
    )
}

/// Run one command, streaming its output.
pub async fn run_spec<F>(spec: &CommandSpec, on_line: F) -> Result<GodotExit, AppError>
where
    F: FnMut(GodotOutputLine) + Send,
{
    run_spec_with_stop(spec, None, on_line).await
}

/// Run one command, streaming its output, with an optional stop signal.
///
/// `timeout_secs == 0` means no timeout, which is what an interactive editor session needs;
/// the stop handle is then the only way it ends early.
pub async fn run_spec_with_stop<F>(
    spec: &CommandSpec,
    stop: Option<GodotStopSignal>,
    on_line: F,
) -> Result<GodotExit, AppError>
where
    F: FnMut(GodotOutputLine) + Send,
{
    run_spec_observed(spec, stop, |_pid| {}, on_line).await
}

/// [`run_spec_with_stop`], telling the caller the child's process id the moment it exists.
///
/// The `Child` itself stays owned by this future (see [`GodotProcessHandle`]); the pid is
/// what the embedded viewport (ADR-0045) needs to find the window the process opens, and a
/// pid cannot reap anything.
pub async fn run_spec_observed<S, F>(
    spec: &CommandSpec,
    stop: Option<GodotStopSignal>,
    on_spawn: S,
    mut on_line: F,
) -> Result<GodotExit, AppError>
where
    S: FnOnce(u32) + Send,
    F: FnMut(GodotOutputLine) + Send,
{
    let started = Instant::now();
    let mut command = tokio::process::Command::new(&spec.program);
    command.args(&spec.args);
    command.env_clear();
    for key in SAFE_ENV_KEYS {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    // The child belongs to this future. Without this a timeout that drops the future would
    // leave a headless Godot running for the rest of the session.
    command.kill_on_drop(true);

    let mut child = command.spawn().map_err(|error| AppError {
        message: format!("could not start {}: {error}", spec.program.display()),
        hint: Some(
            "Check the Godot path in Settings, or set BHIPPI_GODOT to the console build."
                .to_owned(),
        ),
    })?;

    if let Some(pid) = child.id() {
        on_spawn(pid);
    }

    let mut stdout = child.stdout.take().map(|pipe| BufReader::new(pipe).lines());
    let mut stderr = child.stderr.take().map(|pipe| BufReader::new(pipe).lines());
    let deadline =
        (spec.timeout_secs > 0).then(|| Instant::now() + Duration::from_secs(spec.timeout_secs));
    let mut stop = stop;

    let mut timed_out = false;
    let mut killed = false;
    let status = loop {
        // A closed pipe is not the end of the run: Godot can exit long after its last line.
        let stdout_line = async {
            match stdout.as_mut() {
                Some(lines) => lines.next_line().await,
                None => std::future::pending().await,
            }
        };
        let stderr_line = async {
            match stderr.as_mut() {
                Some(lines) => lines.next_line().await,
                None => std::future::pending().await,
            }
        };
        let stop_changed = async {
            match stop.as_mut() {
                Some(signal) => {
                    while signal.stop.changed().await.is_ok() {
                        if *signal.stop.borrow() {
                            return;
                        }
                    }
                    std::future::pending().await
                }
                None => std::future::pending().await,
            }
        };
        let timeout = async {
            match deadline {
                Some(at) => tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await,
                None => std::future::pending().await,
            }
        };

        tokio::select! {
            line = stdout_line => match line {
                Ok(Some(text)) => on_line(GodotOutputLine { stream: GodotStream::Stdout, text }),
                _ => stdout = None,
            },
            line = stderr_line => match line {
                Ok(Some(text)) => on_line(GodotOutputLine { stream: GodotStream::Stderr, text }),
                _ => stderr = None,
            },
            status = child.wait() => break status,
            () = stop_changed => {
                killed = true;
                let _ = child.start_kill();
                stop = None;
            }
            () = timeout => {
                timed_out = true;
                let _ = child.start_kill();
                break tokio::time::timeout(KILL_GRACE, child.wait())
                    .await
                    .unwrap_or_else(|_| Err(std::io::Error::other("Godot did not stop")));
            }
        }
    };

    // Whatever is left in the pipes was still real output; a run that fails on its last line
    // is precisely the run whose last line matters.
    if let Some(lines) = stdout.as_mut() {
        while let Ok(Some(text)) = lines.next_line().await {
            on_line(GodotOutputLine {
                stream: GodotStream::Stdout,
                text,
            });
        }
    }
    if let Some(lines) = stderr.as_mut() {
        while let Ok(Some(text)) = lines.next_line().await {
            on_line(GodotOutputLine {
                stream: GodotStream::Stderr,
                text,
            });
        }
    }

    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let code = match status {
        Ok(status) => status.code(),
        Err(error) if timed_out || killed => {
            tracing::debug!(%error, "Godot was killed before it reported a status");
            None
        }
        Err(error) => {
            return Err(AppError {
                message: format!("waiting for Godot failed: {error}"),
                hint: Some("Try the run again; the process was lost.".to_owned()),
            })
        }
    };
    Ok(GodotExit {
        code: if timed_out || killed { None } else { code },
        timed_out,
        duration_ms,
    })
}

/// Run one command and collect everything it printed. For short probes only.
pub async fn capture(spec: &CommandSpec) -> Result<(GodotExit, String), AppError> {
    let mut output = String::new();
    let exit = run_spec(spec, |line| {
        output.push_str(&line.text);
        output.push('\n');
    })
    .await?;
    Ok((exit, output))
}

/// The first supported Godot on this machine, or `None`.
///
/// Candidates are probed in priority order — `BHIPPI_GODOT`, the configured path, `PATH`,
/// then the platform's install directories — and each is asked its own version rather than
/// trusted for existing. On Windows the console build is what gets asked, because the
/// windowed one prints its version into a console that is not there.
pub async fn detect_godot(config_path: Option<&Path>) -> Option<GodotInstall> {
    for (candidate, source) in candidate_paths(config_path) {
        let (cli_exe, gui_exe) = pair_windows_binaries(&candidate);
        if !cli_exe.is_file() {
            continue;
        }
        let Ok((exit, output)) = capture(&version_command(&cli_exe)).await else {
            continue;
        };
        if !exit.is_success() {
            continue;
        }
        let Ok(version) = parse_version(&output) else {
            continue;
        };
        if !is_supported(&version) {
            tracing::info!(
                path = %cli_exe.display(),
                version = %version.raw,
                "skipping a Godot older than the supported minimum"
            );
            continue;
        }
        return Some(GodotInstall {
            cli_exe,
            gui_exe,
            version,
            source,
        });
    }
    None
}

/// Detection as a typed failure, for a command that cannot proceed without Godot.
pub async fn require_godot(config_path: Option<&Path>) -> Result<GodotInstall, AppError> {
    detect_godot(config_path).await.ok_or_else(|| {
        let offer = bhippi_engine::godot::detect::describe_install_offer();
        AppError {
            message: "no supported Godot was found on this machine".to_owned(),
            hint: Some(format!(
                "Install Godot {} and point Settings → Godot (or BHIPPI_GODOT) at it.",
                offer.version
            )),
        }
    })
}

/// The source a detected install came from, for the Settings panel's explanation.
#[must_use]
pub fn describe_source(source: GodotInstallSource) -> &'static str {
    match source {
        GodotInstallSource::EnvVar => "the BHIPPI_GODOT environment variable",
        GodotInstallSource::Config => "the path saved in Settings",
        GodotInstallSource::Path => "PATH",
        GodotInstallSource::CommonDir => "a standard install folder",
    }
}

#[cfg(test)]
mod tests {
    use super::{run_spec, run_spec_with_stop, stop_channel, GodotStream};
    use bhippi_engine::godot::command::CommandSpec;
    use std::path::PathBuf;

    /// A shell-free stand-in for Godot: whatever is on this platform that prints and exits.
    fn echo_spec(text: &str) -> CommandSpec {
        if cfg!(windows) {
            CommandSpec {
                program: PathBuf::from("cmd"),
                args: vec!["/c".to_owned(), format!("echo {text}")],
                cwd: None,
                env: Vec::new(),
                timeout_secs: 30,
            }
        } else {
            CommandSpec {
                program: PathBuf::from("/bin/sh"),
                args: vec!["-c".to_owned(), format!("echo {text}")],
                cwd: None,
                env: Vec::new(),
                timeout_secs: 30,
            }
        }
    }

    fn sleep_spec(seconds: u32, timeout_secs: u64) -> CommandSpec {
        if cfg!(windows) {
            CommandSpec {
                program: PathBuf::from("cmd"),
                args: vec![
                    "/c".to_owned(),
                    format!("ping -n {} 127.0.0.1 > nul", seconds + 1),
                ],
                cwd: None,
                env: Vec::new(),
                timeout_secs,
            }
        } else {
            CommandSpec {
                program: PathBuf::from("/bin/sh"),
                args: vec!["-c".to_owned(), format!("sleep {seconds}")],
                cwd: None,
                env: Vec::new(),
                timeout_secs,
            }
        }
    }

    #[tokio::test]
    async fn stdout_and_stderr_are_streamed_and_tagged_separately() {
        let mut lines = Vec::new();
        let exit = run_spec(&echo_spec("hello-godot"), |line| lines.push(line))
            .await
            .expect("the child runs");
        assert!(exit.is_success(), "{exit:?}");
        assert!(!exit.timed_out);
        assert!(lines
            .iter()
            .any(|line| line.stream == GodotStream::Stdout && line.text.contains("hello-godot")));

        let mut errors = Vec::new();
        let spec = if cfg!(windows) {
            CommandSpec {
                program: PathBuf::from("cmd"),
                args: vec!["/c".to_owned(), "echo oops 1>&2".to_owned()],
                cwd: None,
                env: Vec::new(),
                timeout_secs: 30,
            }
        } else {
            CommandSpec {
                program: PathBuf::from("/bin/sh"),
                args: vec!["-c".to_owned(), "echo oops 1>&2".to_owned()],
                cwd: None,
                env: Vec::new(),
                timeout_secs: 30,
            }
        };
        run_spec(&spec, |line| errors.push(line))
            .await
            .expect("the child runs");
        assert!(errors
            .iter()
            .any(|line| line.stream == GodotStream::Stderr && line.text.contains("oops")));
    }

    #[tokio::test]
    async fn a_child_that_overruns_its_budget_is_killed_and_reported() {
        let exit = run_spec(&sleep_spec(10, 1), |_| {})
            .await
            .expect("the runner returns");
        assert!(exit.timed_out, "{exit:?}");
        assert_eq!(exit.code, None);
        assert!(!exit.is_success());
        assert!(
            exit.duration_ms < 30_000,
            "the timeout must fire long before the child would have finished: {exit:?}"
        );
    }

    #[tokio::test]
    async fn a_running_game_can_be_stopped_by_its_handle() {
        let (handle, signal) = stop_channel();
        let stopper = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            assert!(handle.kill());
            assert!(handle.is_stopped());
        });
        let exit = run_spec_with_stop(&sleep_spec(10, 0), Some(signal), |_| {})
            .await
            .expect("the runner returns");
        stopper.await.expect("the stopper ran");
        assert!(!exit.timed_out, "it was stopped, not timed out: {exit:?}");
        assert_eq!(exit.code, None);
    }

    #[tokio::test]
    async fn a_program_that_is_not_there_is_a_typed_error_with_a_hint() {
        let spec = CommandSpec {
            program: PathBuf::from("definitely-not-godot-1a2b3c"),
            args: vec!["--version".to_owned()],
            cwd: None,
            env: Vec::new(),
            timeout_secs: 5,
        };
        let error = run_spec(&spec, |_| {}).await.expect_err("must fail");
        assert!(error.hint.is_some());
        assert!(error.message.contains("definitely-not-godot-1a2b3c"));
    }
}
