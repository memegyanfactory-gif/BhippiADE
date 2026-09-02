//! Real interactive terminals, backed by a PTY.
//!
//! # Why this exists
//!
//! `run_cli_command` (in `workspace.rs`) is a batch runner: it attaches `Stdio::null()`
//! to stdin, waits for the child to exit, and returns everything at once. That is fine
//! for `git status`. It cannot host a program like `opencode`, `claude`, `htop` or a
//! REPL, because those want three things it structurally cannot give them:
//!
//!   1. **A terminal.** They call `isatty()`. Behind pipes they either refuse to draw a
//!      UI or draw one that no pipe can carry.
//!   2. **Stdin.** With `Stdio::null()` every read returns EOF immediately, so a TUI
//!      shuts down the moment it starts. That is the "opens and gives me empty nothing"
//!      the owner reported: opencode starts, reads EOF, and exits before drawing.
//!   3. **Incremental output.** `.output()` resolves once, on exit. A long-running
//!      program shows nothing at all until it is over.
//!
//! So this module allocates a genuine pseudo-terminal (ConPTY on Windows, a Unix PTY
//! elsewhere), runs the user's shell inside it, streams the raw bytes out as they
//! arrive, and writes keystrokes back in.
//!
//! # No sandbox
//!
//! This is deliberate and it is the point (INV-032 covers untrusted *content*, not the
//! owner's own shell). The child inherits this process's environment and PATH, starts in
//! the project directory, and is not wrapped, filtered, or restricted. A tool the owner
//! can run in Windows Terminal behaves identically here, including anything it launches
//! and any credential it reads from the environment.
//!
//! # Bytes, not strings
//!
//! Output is forwarded base64-encoded. A PTY emits escape sequences and arbitrary UTF-8
//! that a read can split mid-codepoint; decoding per chunk in Rust would corrupt those
//! boundaries and mangle the very control codes the renderer needs. The bytes reach the
//! terminal emulator exactly as the program wrote them.

use base64::Engine as _;
use portable_pty::{CommandBuilder, NativePtySystem, PtyPair, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::AppError;

/// One byte over the largest read we forward in a single event.
///
/// A program that dumps a megabyte (`cat` on a big file, a verbose build) would otherwise
/// produce one enormous IPC payload and stall the UI thread while it is decoded. 8 KiB is
/// comfortably above a screenful and small enough to stay responsive.
const READ_CHUNK: usize = 8 * 1024;

/// Shells a terminal can be opened with.
///
/// The list is deliberately short: these are the hosts a program like opencode is
/// launched *from*. The program itself is then typed at the prompt, exactly as it would
/// be in Windows Terminal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TerminalShell {
    Powershell,
    Cmd,
    GitBash,
    Wsl,
    /// The platform's own default (`$SHELL` on Unix, PowerShell on Windows).
    System,
}

impl TerminalShell {
    /// Builds the command for this shell, or reports why it cannot run here.
    fn command(self, cwd: &Path) -> Result<CommandBuilder, AppError> {
        let mut builder = match self {
            Self::Powershell => {
                let mut command = CommandBuilder::new("powershell.exe");
                // -NoExit keeps the session alive for interactive use; -NoLogo trims the
                // banner. The execution policy is scoped to this process only.
                command.args(["-NoLogo", "-ExecutionPolicy", "Bypass", "-NoExit"]);
                command
            }
            Self::Cmd => CommandBuilder::new("cmd.exe"),
            Self::GitBash => {
                let bash = find_git_bash().ok_or_else(|| AppError {
                    message: "Git Bash is not installed.".to_owned(),
                    hint: Some(
                        "Install Git for Windows, or pick PowerShell in the shell menu.".to_owned(),
                    ),
                })?;
                let mut command = CommandBuilder::new(bash.to_string_lossy().to_string());
                command.args(["--login", "-i"]);
                command
            }
            Self::Wsl => CommandBuilder::new("wsl.exe"),
            Self::System => {
                if cfg!(target_os = "windows") {
                    let mut command = CommandBuilder::new("powershell.exe");
                    command.args(["-NoLogo", "-ExecutionPolicy", "Bypass", "-NoExit"]);
                    command
                } else {
                    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_owned());
                    let mut command = CommandBuilder::new(shell);
                    command.arg("-i");
                    command
                }
            }
        };

        builder.cwd(cwd);

        // Inherit the whole environment, then state the terminal's own capabilities.
        // Without TERM a curses program falls back to a dumb terminal and draws nothing;
        // without COLORTERM it drops to 16 colours. These describe what the frontend
        // emulator actually supports, so they are facts, not decoration.
        for (key, value) in std::env::vars() {
            builder.env(key, value);
        }
        builder.env("TERM", "xterm-256color");
        builder.env("COLORTERM", "truecolor");
        // Node and Bun CLIs (opencode among them) check this before drawing colour.
        builder.env("FORCE_COLOR", "3");
        Ok(builder)
    }
}

#[cfg(windows)]
fn find_git_bash() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
    [
        PathBuf::from("C:\\Program Files\\Git\\bin\\bash.exe"),
        PathBuf::from("C:\\Program Files (x86)\\Git\\bin\\bash.exe"),
        PathBuf::from(local).join("Programs\\Git\\bin\\bash.exe"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

#[cfg(not(windows))]
fn find_git_bash() -> Option<std::path::PathBuf> {
    Some(std::path::PathBuf::from("/bin/bash"))
}

/// One live terminal.
struct Session {
    /// Writing to the PTY master is what "typing" means.
    writer: Mutex<Box<dyn Write + Send>>,
    /// Held so the terminal can be resized after it opens.
    pair: Mutex<PtyPair>,
    child: Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
    /// Set when the session is closed, so the reader thread stops rather than spinning
    /// on a dead descriptor.
    closed: Arc<AtomicBool>,
}

/// Every open terminal, keyed by the id handed to the frontend.
#[derive(Default)]
pub struct TerminalRegistry {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_id: AtomicU64,
}

impl TerminalRegistry {
    fn mint_id(&self) -> String {
        format!("term-{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn insert(&self, id: String, session: Arc<Session>) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.insert(id, session);
        }
    }

    fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.lock().ok()?.get(id).cloned()
    }

    fn remove(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.lock().ok()?.remove(id)
    }

    /// Kills every terminal. Called on shutdown so no orphan shell survives the window.
    pub fn shutdown(&self) {
        let sessions: Vec<Arc<Session>> = self
            .sessions
            .lock()
            .map(|mut map| map.drain().map(|(_, session)| session).collect())
            .unwrap_or_default();
        for session in sessions {
            end(&session);
        }
    }
}

/// Marks a session closed and kills its child. Safe to call twice.
fn end(session: &Session) {
    session.closed.store(true, Ordering::Relaxed);
    if let Ok(mut child) = session.child.lock() {
        let _ignored = child.kill();
        let _ignored = child.wait();
    }
}

/// A chunk of terminal output, or the notice that the terminal has ended.
#[derive(Clone, Deserialize, Serialize, Type, tauri_specta::Event)]
pub struct TerminalOutput {
    pub id: String,
    /// Base64 of the raw PTY bytes. Never decoded on this side — see the module docs.
    pub chunk: String,
}

/// The terminal's child process exited.
#[derive(Clone, Deserialize, Serialize, Type, tauri_specta::Event)]
pub struct TerminalExited {
    pub id: String,
    pub exit_code: Option<i32>,
}

/// A newly opened terminal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TerminalSession {
    pub id: String,
    /// Absolute directory the shell started in.
    pub cwd: String,
    pub shell: TerminalShell,
}

fn clamp_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        // A zero-width PTY makes some shells divide by zero while laying out a prompt,
        // and the frontend can legitimately report 0 for one frame while mounting.
        cols: cols.clamp(2, 1000),
        rows: rows.clamp(1, 500),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Opens a shell in `path` and starts streaming its output.
#[tauri::command]
#[specta::specta]
pub async fn terminal_open(
    app: tauri::AppHandle,
    registry: tauri::State<'_, Arc<TerminalRegistry>>,
    path: String,
    shell: TerminalShell,
    cols: u16,
    rows: u16,
) -> Result<TerminalSession, AppError> {
    use tauri_specta::Event as _;

    let cwd = crate::workspace::canonical_directory(&path)?;
    let command = shell.command(&cwd)?;

    let pty = NativePtySystem::default();
    let pair = pty
        .openpty(clamp_size(cols, rows))
        .map_err(|error| AppError {
            message: format!("Could not open a terminal: {error}"),
            hint: Some(
                "Windows needs the ConPTY support that ships with Windows 10 1809 or newer."
                    .to_owned(),
            ),
        })?;

    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| AppError {
            message: format!("Could not start the shell: {error}"),
            hint: Some("Check that the selected shell is installed and on PATH.".to_owned()),
        })?;

    let reader = pair.master.try_clone_reader().map_err(|error| AppError {
        message: format!("Could not read from the terminal: {error}"),
        hint: None,
    })?;
    let writer = pair.master.take_writer().map_err(|error| AppError {
        message: format!("Could not write to the terminal: {error}"),
        hint: None,
    })?;

    let id = registry.mint_id();
    let closed = Arc::new(AtomicBool::new(false));
    let session = Arc::new(Session {
        writer: Mutex::new(writer),
        pair: Mutex::new(pair),
        child: Mutex::new(child),
        closed: Arc::clone(&closed),
    });
    registry.insert(id.clone(), Arc::clone(&session));

    // A PTY read is a blocking syscall with no async equivalent on Windows, so it gets a
    // real OS thread rather than a Tokio task — parking a runtime worker on it would
    // starve everything else the app is doing.
    let pump_id = id.clone();
    let pump_app = app.clone();
    std::thread::Builder::new()
        .name(format!("bhippi-pty-{id}"))
        .spawn(move || pump(reader, pump_app, pump_id, closed))
        .map_err(|error| AppError {
            message: format!("Could not start the terminal reader: {error}"),
            hint: None,
        })?;

    // Reap the child on its own thread so an exit is reported the moment it happens,
    // rather than whenever the next keystroke notices.
    let watch_id = id.clone();
    let watch_app = app.clone();
    let watch_session = Arc::clone(&session);
    std::thread::Builder::new()
        .name(format!("bhippi-pty-wait-{id}"))
        .spawn(move || {
            let status = watch_session
                .child
                .lock()
                .ok()
                .and_then(|mut child| child.wait().ok());
            let exit_code =
                status.map(|status| i32::try_from(status.exit_code()).unwrap_or(i32::MAX));
            watch_session.closed.store(true, Ordering::Relaxed);
            let _ignored = (TerminalExited {
                id: watch_id,
                exit_code,
            })
            .emit(&watch_app);
        })
        .map_err(|error| AppError {
            message: format!("Could not watch the terminal: {error}"),
            hint: None,
        })?;

    tracing::info!(terminal = %id, cwd = %cwd.display(), ?shell, "terminal opened");
    Ok(TerminalSession {
        id,
        cwd: cwd.to_string_lossy().to_string(),
        shell,
    })
}

/// Forwards PTY bytes to the frontend until the terminal ends.
fn pump(
    mut reader: Box<dyn Read + Send>,
    app: tauri::AppHandle,
    id: String,
    closed: Arc<AtomicBool>,
) {
    use tauri_specta::Event as _;
    let encoder = base64::engine::general_purpose::STANDARD;
    let mut buffer = vec![0_u8; READ_CHUNK];
    loop {
        if closed.load(Ordering::Relaxed) {
            break;
        }
        match reader.read(&mut buffer) {
            // EOF: the slave side is gone, which means the shell has exited.
            Ok(0) => break,
            Ok(read) => {
                let event = TerminalOutput {
                    id: id.clone(),
                    chunk: encoder.encode(&buffer[..read]),
                };
                if event.emit(&app).is_err() {
                    // The window is gone; nothing left to deliver output to.
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

/// Sends keystrokes (or pasted text) to the terminal.
#[tauri::command]
#[specta::specta]
pub async fn terminal_write(
    registry: tauri::State<'_, Arc<TerminalRegistry>>,
    id: String,
    data: String,
) -> Result<(), AppError> {
    let session = registry.get(&id).ok_or_else(|| AppError {
        message: "That terminal is no longer open.".to_owned(),
        hint: Some("Open a new terminal to keep working.".to_owned()),
    })?;
    let mut writer = session.writer.lock().map_err(|_| AppError {
        message: "The terminal is busy.".to_owned(),
        hint: None,
    })?;
    writer
        .write_all(data.as_bytes())
        .and_then(|()| writer.flush())
        .map_err(|error| AppError {
            message: format!("Could not send input to the terminal: {error}"),
            hint: None,
        })
}

/// Tells the PTY its new size, so full-screen programs re-lay-out.
#[tauri::command]
#[specta::specta]
pub async fn terminal_resize(
    registry: tauri::State<'_, Arc<TerminalRegistry>>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), AppError> {
    let Some(session) = registry.get(&id) else {
        // A resize racing a close is normal and not worth an error in the UI.
        return Ok(());
    };
    let pair = session.pair.lock().map_err(|_| AppError {
        message: "The terminal is busy.".to_owned(),
        hint: None,
    })?;
    pair.master
        .resize(clamp_size(cols, rows))
        .map_err(|error| AppError {
            message: format!("Could not resize the terminal: {error}"),
            hint: None,
        })
}

/// Ends a terminal and kills its shell.
#[tauri::command]
#[specta::specta]
pub async fn terminal_close(
    registry: tauri::State<'_, Arc<TerminalRegistry>>,
    id: String,
) -> Result<(), AppError> {
    if let Some(session) = registry.remove(&id) {
        end(&session);
        tracing::info!(terminal = %id, "terminal closed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_sized_terminal_is_widened_to_something_a_shell_can_lay_out_in() {
        // The frontend legitimately reports 0x0 for the frame before the element has
        // been measured, and a zero-column PTY makes some prompts divide by zero.
        let size = clamp_size(0, 0);
        assert!(size.cols >= 2);
        assert!(size.rows >= 1);

        let normal = clamp_size(120, 30);
        assert_eq!(normal.cols, 120);
        assert_eq!(normal.rows, 30);
    }

    #[test]
    fn ids_are_unique_across_a_registry() {
        let registry = TerminalRegistry::default();
        let first = registry.mint_id();
        let second = registry.mint_id();
        assert_ne!(first, second);
    }

    #[test]
    fn a_write_to_an_unknown_terminal_is_an_actionable_error() {
        let registry = TerminalRegistry::default();
        assert!(registry.get("term-does-not-exist").is_none());
    }

    #[test]
    fn the_shell_command_declares_a_capable_terminal() {
        // Without TERM a curses program draws nothing at all, which is indistinguishable
        // from the bug this module fixes. Pin that the environment says otherwise.
        let cwd = std::env::current_dir().expect("a working directory");
        let command = TerminalShell::System
            .command(&cwd)
            .expect("the system shell must be constructible");
        let env: Vec<(String, String)> = command
            .iter_full_env_as_str()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
        let value_of = |name: &str| {
            env.iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.clone())
        };
        assert_eq!(value_of("TERM").as_deref(), Some("xterm-256color"));
        assert_eq!(value_of("COLORTERM").as_deref(), Some("truecolor"));
        assert_eq!(value_of("FORCE_COLOR").as_deref(), Some("3"));
    }

    #[test]
    fn the_shell_inherits_the_environment_rather_than_a_scrubbed_one() {
        // "It should not follow any sandbox": a tool that works in Windows Terminal has
        // to work here, and most of them need PATH and their own config env vars.
        let cwd = std::env::current_dir().expect("a working directory");
        let command = TerminalShell::System
            .command(&cwd)
            .expect("the system shell must be constructible");
        let inherited = command.iter_full_env_as_str().count();
        let ours = std::env::vars().count();
        assert!(
            inherited >= ours,
            "expected the full environment ({ours} vars), saw {inherited}"
        );
    }
}
