//! Proves the PTY layer does the three things the batch runner could not.
//!
//! These drive `portable_pty` exactly the way `terminal.rs` does, without a Tauri app
//! around it — a `tauri::AppHandle` cannot be built in a test, but the part that was
//! broken is the process plumbing, and that is what these exercise: a real terminal, live
//! stdin, and output that arrives before the child exits.

// A failure to allocate a PTY or type into it is the test failing, and the panic
// message is the diagnosis. Matches the other integration tests in this crate.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// "Where is the cursor?" — the query a shell blocks on at startup.
const DSR_QUERY: &str = "\u{1b}[6n";
/// "Row 1, column 1." — the shape of answer a real emulator sends back.
const DSR_REPLY: &[u8] = b"\x1b[1;1R";

/// How long to wait for a shell to say something. Generous: a cold PowerShell on a
/// loaded CI box takes seconds to reach its first prompt.
const PATIENCE: Duration = Duration::from_secs(45);

fn size() -> PtySize {
    PtySize {
        cols: 100,
        rows: 30,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// A shell running in a real PTY, with a thread draining its output.
struct Harness {
    /// Shared because the reader thread also writes to it, to answer the shell's queries.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    output: mpsc::Receiver<String>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    /// Held for the lifetime of the harness: dropping the pair closes the PTY.
    _pair: portable_pty::PtyPair,
}

impl Harness {
    fn open() -> Self {
        let pty = NativePtySystem::default();
        let pair = pty.openpty(size()).expect("a pty must be allocatable");

        let mut command = if cfg!(windows) {
            let mut command = CommandBuilder::new("powershell.exe");
            command.args(["-NoLogo", "-NoProfile", "-NoExit"]);
            command
        } else {
            let mut command = CommandBuilder::new("/bin/sh");
            command.arg("-i");
            command
        };
        command.cwd(std::env::current_dir().expect("a working directory"));
        command.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(command)
            .expect("the shell must start");
        let mut reader = pair
            .master
            .try_clone_reader()
            .expect("the master must be readable");
        // `take_writer` hands out the single writer, so the test and the responder share
        // it rather than each asking for one.
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(
            pair.master
                .take_writer()
                .expect("the master must be writable"),
        ));

        let (sender, output) = mpsc::channel();
        let responder = Arc::clone(&writer);
        std::thread::spawn(move || {
            let mut buffer = vec![0_u8; 4096];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                // Stand in for the emulator on the one point that actually matters here:
                // a shell asks where the cursor is (CSI 6n) and BLOCKS until something
                // answers. Nothing did at first, and the shell hung with a blank screen —
                // the same failure the frontend hit when it attached its input handler
                // only after the PTY had opened, which is why that is now attached at
                // construction (`ui/src/lib/terminalStore.ts`).
                if text.contains(DSR_QUERY) {
                    if let Ok(mut writer) = responder.lock() {
                        let _ignored = writer.write_all(DSR_REPLY).and_then(|()| writer.flush());
                    }
                }
                if sender.send(text).is_err() {
                    break;
                }
            }
        });

        Self {
            writer,
            output,
            child,
            _pair: pair,
        }
    }

    fn type_line(&self, line: &str) {
        let mut writer = self.writer.lock().expect("the writer must be lockable");
        writer
            .write_all(line.as_bytes())
            .and_then(|()| writer.flush())
            .expect("typing into the terminal must work");
    }

    /// Collects output until `needle` shows up, or fails with what it did see.
    fn wait_for(&self, needle: &str) -> String {
        let deadline = Instant::now() + PATIENCE;
        let mut seen = String::new();
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(remaining) {
                Ok(chunk) => {
                    seen.push_str(&chunk);
                    if seen.contains(needle) {
                        return seen;
                    }
                }
                Err(_) => break,
            }
        }
        panic!("never saw {needle:?}. Output was:\n{seen}");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ignored = self.child.kill();
        let _ignored = self.child.wait();
    }
}

#[test]
fn a_shell_answers_typed_input_without_ever_exiting() {
    // This is the whole bug in one test. `run_cli_command` attaches Stdio::null(), so an
    // interactive program reads EOF and dies before drawing anything — the "opens and
    // gives me empty nothing" report. Here the shell answers and stays up.
    let mut harness = Harness::open();

    harness.type_line("echo bhippi-marker-one\r\n");
    let seen = harness.wait_for("bhippi-marker-one");
    assert!(seen.contains("bhippi-marker-one"));

    // Still alive: a second command lands on the same session.
    harness.type_line("echo bhippi-marker-two\r\n");
    harness.wait_for("bhippi-marker-two");

    assert!(
        harness.child.try_wait().ok().flatten().is_none(),
        "the shell must still be running after answering"
    );
}

#[test]
fn output_arrives_while_the_program_is_still_running() {
    // The batch runner resolved only on exit, so a long-running program showed nothing at
    // all until it was over. Here the first line is readable while it still runs.
    let harness = Harness::open();
    // Reach a prompt first, so the measurement below times the command and not the
    // shell's cold start.
    harness.type_line("echo bhippi-ready\r\n");
    harness.wait_for("bhippi-ready");

    let script = if cfg!(windows) {
        "Write-Output 'bhippi-first'; Start-Sleep -Seconds 4; Write-Output 'bhippi-last'\r\n"
    } else {
        "echo bhippi-first; sleep 4; echo bhippi-last\n"
    };
    let started = Instant::now();
    harness.type_line(script);
    harness.wait_for("bhippi-first");
    let first_at = started.elapsed();

    assert!(
        first_at < Duration::from_secs(4),
        "the first line arrived after {first_at:?} — output is still being held back \
         until the command finishes, which is the batch-runner behaviour this replaces"
    );
}

#[test]
fn the_child_believes_it_is_attached_to_a_terminal() {
    // A TUI checks this before it draws anything. Behind pipes the answer is "no", which
    // is why opencode rendered nothing.
    let harness = Harness::open();

    // A PTY echoes what is typed at it, so a probe that names its own marker verbatim
    // gets "found" in the echo of the command line before the answer has even run. The
    // marker is assembled at runtime instead: it appears in the output and nowhere in
    // the command being echoed.
    let probe = if cfg!(windows) {
        // PowerShell has no isatty; the equivalent question is whether a real console
        // buffer with a non-zero width is attached, which only a PTY provides.
        "Write-Output (\"tty\" + \"cols=\" + $Host.UI.RawUI.WindowSize.Width)\r\n"
    } else {
        "if [ -t 0 ]; then printf 'tty%s=%s\\n' cols yes; else printf 'tty%s=%s\\n' cols no; fi\n"
    };
    harness.type_line(probe);
    let seen = harness.wait_for("ttycols=");
    let answer: String = seen
        .rsplit("ttycols=")
        .next()
        .unwrap_or_default()
        .trim_start()
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();

    if cfg!(windows) {
        let cols: u16 = answer.parse().unwrap_or(0);
        assert!(cols > 0, "expected a real console width, saw {answer:?}");
    } else {
        assert_eq!(answer, "yes", "the shell must see a tty on stdin");
    }
}
