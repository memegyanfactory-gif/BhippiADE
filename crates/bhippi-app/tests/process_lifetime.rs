//! The one claim in ADR-0052 that cannot be tested from inside the app: when Bhippi is
//! **killed** rather than closed, everything it started still dies.
//!
//! A unit test cannot show this. Proving it means ending a process while it holds live
//! children, and the process holding the children would be the test runner. So this file
//! drives a stand-in: it re-runs this same test binary in a second process, tells that
//! process to be the app (install the guard, start a "Godot editor" that starts a "game",
//! report both pids), then terminates it the way a debugger's stop button does — no
//! shutdown handler, no destructors, no Rust code of ours running at all — and asks Windows
//! whether the two descendants are still there.
//!
//! Before ADR-0052 both survived, which is exactly what the owner reported: *even after
//! closing the app some apps are running in the bg*.
//!
//! Windows only: the guarantee is a job object, and off Windows there is nothing to assert.

#![cfg(windows)]
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Set on the stand-in so it knows it is being driven rather than merely enumerated.
const STANDIN: &str = "BHIPPI_PROCESS_GUARD_STANDIN";

/// The line the stand-in prints once both processes exist.
const MARKER: &str = "PIDS|";

/// How long a descendant is given to die after the app is killed. The kernel does it as the
/// handle closes; the margin is for a loaded machine.
const GRACE: Duration = Duration::from_secs(15);

/// Whether Windows still knows this pid.
fn alive(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .expect("tasklist runs");
    // A hit is a CSV row containing the pid in quotes; a miss is an INFO line or nothing.
    String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
}

fn kill(pid: u32) {
    let _ignored = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// The stand-in for Bhippi. Only does anything when [`STANDIN`] is set, so a plain
/// `cargo test` run — which does not pass `--ignored` — never reaches it either way.
///
/// It is deliberately shaped like the real app: the guard is installed before anything is
/// spawned, the child is a process this "app" launched, and the grandchild is a process the
/// *child* launched and whose pid the app never had — the Godot editor's Play.
#[test]
#[ignore = "driven as a second process by a_hard_kill_of_the_app_takes_every_descendant_with_it"]
// Never reaping the child is the point: a stand-in that tidied up on its way out would not
// be standing in for an app that is killed where it stands.
#[allow(clippy::zombie_processes)]
fn stand_in_app() {
    if std::env::var_os(STANDIN).is_none() {
        return;
    }

    bhippi_app::process_guard::install();
    assert!(
        bhippi_app::process_guard::is_installed(),
        "the stand-in app must hold the guarantee, or it is not standing in for anything"
    );

    let mut child = Command::new("powershell")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$game = Start-Process -FilePath 'cmd.exe' \
             -ArgumentList '/c','ping -n 900 127.0.0.1 > nul' \
             -WindowStyle Hidden -PassThru; \
             Write-Output $game.Id; Start-Sleep -Seconds 900",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the stand-in editor starts");

    let child_pid = child.id();
    let mut line = String::new();
    let read = child
        .stdout
        .take()
        .map(|pipe| BufReader::new(pipe).read_line(&mut line));
    assert!(
        matches!(read, Some(Ok(n)) if n > 0),
        "the stand-in editor must report the pid of the game it launched"
    );
    let grandchild_pid: u32 = line
        .trim()
        .parse()
        .unwrap_or_else(|error| panic!("it reports a pid, not {line:?}: {error}"));

    println!("{MARKER}{child_pid}|{grandchild_pid}");
    let _ignored = std::io::stdout().flush();

    // Now be an app: alive, holding both, with no intention of exiting. The parent kills
    // this process where it stands.
    std::thread::sleep(Duration::from_secs(900));
}

#[test]
fn a_hard_kill_of_the_app_takes_every_descendant_with_it() {
    let exe = std::env::current_exe().expect("this test binary has a path");
    let mut app = Command::new(exe)
        .args(["--exact", "stand_in_app", "--ignored", "--nocapture"])
        .env(STANDIN, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the stand-in app starts");

    let app_pid = app.id();
    let mut pids = None;
    if let Some(pipe) = app.stdout.take() {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            if let Some(rest) = line.trim().strip_prefix(MARKER) {
                let mut parts = rest.split('|');
                let child = parts.next().and_then(|raw| raw.parse::<u32>().ok());
                let grandchild = parts.next().and_then(|raw| raw.parse::<u32>().ok());
                if let (Some(child), Some(grandchild)) = (child, grandchild) {
                    pids = Some((child, grandchild));
                }
                break;
            }
        }
    }

    let Some((child_pid, grandchild_pid)) = pids else {
        kill(app_pid);
        panic!("the stand-in app never reported its two processes");
    };

    assert!(alive(child_pid), "the child is running before the kill");
    assert!(
        alive(grandchild_pid),
        "and so is the game it launched — the process that used to survive"
    );

    // The debugger's stop button: this process is terminated where it stands. No exit
    // handler, no destructor, no line of Bhippi's code runs after this.
    kill(app_pid);

    let started = Instant::now();
    while started.elapsed() < GRACE && (alive(child_pid) || alive(grandchild_pid)) {
        std::thread::sleep(Duration::from_millis(200));
    }

    let child_left = alive(child_pid);
    let grandchild_left = alive(grandchild_pid);
    // Leave nothing behind even when the assertion below is the thing that fails.
    kill(grandchild_pid);
    kill(child_pid);
    let _ignored = app.wait();

    assert!(
        !child_left,
        "the app was killed and its child kept running (pid {child_pid})"
    );
    assert!(
        !grandchild_left,
        "the app was killed and the game its editor launched kept running (pid \
         {grandchild_pid}) — this is the bug ADR-0052 fixes"
    );
}
