//! The contract this adapter exists for: a CLI answer must reach the screen while the
//! vendor is still writing it.
//!
//! The old adapter called `Command::output()`, which cannot produce anything until the
//! child exits. Every test it had still passed, because the *content* was correct — it
//! just arrived all at once, at the end, which is what "Claude takes too much time"
//! actually was. Correct-but-late is invisible to a test that only checks final text, so
//! these check the timing instead.
//!
//! A stub CLI stands in for a vendor: it prints one event, stalls, then prints the rest.
//! If the adapter buffers, the first delta cannot arrive before the stall is over.
//!
//! The last test in this file checks the other direction — that the prompt leaves through
//! stdin and never through argv, which is what stops a `--`-shaped line inside a prompt
//! from reaching Claude Code as a flag.

#![cfg(windows)]

use bhippi_providers::model::{CompletionRequest, Delta};
use bhippi_providers::{CliProvider, Message, Provider};
use bhippi_types::TaskClass;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, MutexGuard};

// Unique binary names prevent native-binary discovery from selecting a real provider.
fn fixture_spec(id: &str) -> Option<&'static bhippi_providers::catalog::ProviderSpec> {
    let mut spec = *bhippi_providers::spec(id)?;
    spec.binary = Some(match id {
        "claude" => "bhippi-fixture-claude",
        "codex" => "bhippi-fixture-codex",
        "opencode" => "bhippi-fixture-opencode",
        _ => return None,
    });
    Some(Box::leak(Box::new(spec)))
}

/// How long the stub waits between its first line and the rest.
const STALL: Duration = Duration::from_millis(1500);

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "bhippi-stream-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    assert!(std::fs::create_dir_all(&dir).is_ok(), "scratch dir");
    dir
}

/// Writes a PowerShell stub that prints Claude-shaped `stream-json` with a stall in it.
fn write_stub(dir: &Path) -> PathBuf {
    let shim = dir.join("bhippi-fixture-claude.ps1");
    let millis = STALL.as_millis();
    let delta = |text: &str| {
        format!(
            "{{\"type\":\"stream_event\",\"event\":{{\"type\":\"content_block_delta\",\
             \"delta\":{{\"type\":\"text_delta\",\"text\":\"{text}\"}}}}}}"
        )
    };
    let script = format!(
        "param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Rest)\n\
         Write-Output '{}'\n\
         Start-Sleep -Milliseconds {millis}\n\
         Write-Output '{}'\n\
         Write-Output '{{\"is_error\":false,\"subtype\":\"success\",\"type\":\"result\",\
         \"result\":\"first second\",\"usage\":{{\"input_tokens\":9,\"output_tokens\":2}}}}'\n",
        delta("first "),
        delta("second"),
    );
    assert!(std::fs::write(&shim, script).is_ok(), "stub written");
    shim
}

/// Serialises the tests in this file.
///
/// `PATH` is process-wide, and both tests below install a stub launcher by prepending to
/// it. Run concurrently — which is what the test harness does by default — one test's
/// restore erases the other's prepend, and whichever loses the race resolves the *real*
/// `claude` instead of its stub. That produced a genuinely flaky failure, which is worse
/// than no test at all, so the two hold this lock for their whole duration.
/// An async mutex, not a std one: the guard is held across the awaits that run the stub,
/// and a blocking guard held over an await can deadlock the runtime it is parked on.
async fn path_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().await
}

/// Points the vendor's install roots at an empty directory, returning what was there.
///
/// Resolution deliberately prefers Claude Code's *native* binary over npm's shims, and
/// on any machine that has the CLI that binary really exists — so a stub on PATH alone is
/// bypassed and these tests would spawn the real vendor. Emptying `APPDATA` and
/// `USERPROFILE` is what makes "no vendor is installed here" true.
///
/// `HOME` is pointed at a stable directory rather than the scratch one on purpose: the
/// adapter derives its agent workspace from it and caches that for the life of the
/// process, and the scratch directory is deleted when the test ends.
fn hide_native_installs(empty: &Path) -> Vec<(&'static str, Option<std::ffi::OsString>)> {
    let saved = ["APPDATA", "USERPROFILE", "HOME"]
        .into_iter()
        .map(|key| (key, std::env::var_os(key)))
        .collect();
    let stable_home = std::env::temp_dir().join("bhippi-stream-home");
    let _ignored = std::fs::create_dir_all(&stable_home);
    std::env::set_var("APPDATA", empty);
    std::env::set_var("USERPROFILE", empty);
    std::env::set_var("HOME", &stable_home);
    saved
}

fn restore_env(saved: Vec<(&'static str, Option<std::ffi::OsString>)>) {
    for (key, value) in saved {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

/// Puts `dir` at the front of PATH for this process, returning the original.
fn prepend_path(dir: &Path) -> Option<std::ffi::OsString> {
    let original = std::env::var_os("PATH");
    let mut dirs = vec![dir.to_path_buf()];
    if let Some(path) = &original {
        dirs.extend(std::env::split_paths(path));
    }
    if let Ok(joined) = std::env::join_paths(dirs) {
        std::env::set_var("PATH", joined);
    }
    original
}

fn request() -> CompletionRequest {
    CompletionRequest::new(
        TaskClass::Expander,
        "system",
        vec![Message::user("hello".to_owned())],
    )
}

/// The regression pin: the first delta must land while the stub is still asleep.
#[tokio::test]
async fn the_first_delta_arrives_before_the_process_exits() {
    let _serialised = path_lock().await;
    let dir = scratch("early");
    write_stub(&dir);
    let original = prepend_path(&dir);
    let roots = hide_native_installs(&dir);

    let Some(spec) = fixture_spec("claude") else {
        panic!("the catalogue must know Claude Code");
    };
    let Some(provider) = CliProvider::open(spec) else {
        panic!("the stub must resolve as a launcher");
    };

    let started = Instant::now();
    let mut stream = match provider.complete(request()).await {
        Ok(stream) => stream,
        Err(error) => panic!("the stub must start: {error}"),
    };

    let mut first_text_at = None;
    let mut last_event_at = started.elapsed();
    let mut text = String::new();
    let mut usage_seen = false;
    while let Some(item) = stream.next().await {
        last_event_at = started.elapsed();
        match item {
            Ok(Delta::Text { delta }) => {
                first_text_at.get_or_insert(last_event_at);
                text.push_str(&delta);
            }
            Ok(Delta::Usage { .. }) => usage_seen = true,
            Ok(Delta::Done { .. }) => break,
            Ok(_) => {}
            Err(error) => panic!("the stub must not fail: {error}"),
        }
    }

    if let Some(path) = original {
        std::env::set_var("PATH", path);
    }
    restore_env(roots);
    let _ignored = std::fs::remove_dir_all(&dir);

    let Some(first) = first_text_at else {
        panic!("no text ever arrived");
    };
    // The claim is that the adapter streams rather than buffering, and the stall in the stub
    // is what makes that observable: a streaming adapter delivers the first delta, then waits
    // out the stall, so the end of the stream is roughly a stall later than that first delta.
    // A buffering one hands everything over at once and the two are simultaneous.
    //
    // This used to assert `first < STALL`, measured from before the process was spawned — so
    // it also timed PowerShell's startup. On a loaded runner that alone can exceed the stall,
    // and it failed on Windows CI while the adapter was streaming perfectly.
    let spread = last_event_at.saturating_sub(first);
    assert!(
        spread >= STALL / 2,
        "the whole stream landed within {spread:?} of its first delta and the stub stalls for \
         {STALL:?} — the adapter is buffering the process instead of streaming it"
    );
    // The answer must also still be right, and said exactly once even though the stub
    // printed it twice over (as partials, then again in `result`).
    assert_eq!(text, "first second");
    assert!(
        usage_seen,
        "the vendor's token counts must reach the ledger"
    );
}

/// A vendor that reports its own failure and still exits 0 must surface as a failure,
/// not as a successful empty answer. This is the "the CLI answered with nothing" bug.
#[tokio::test]
async fn an_in_band_failure_on_a_clean_exit_reaches_the_caller() {
    let _serialised = path_lock().await;
    let dir = scratch("inband");
    let shim = dir.join("bhippi-fixture-claude.ps1");
    assert!(std::fs::write(
        &shim,
        "param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Rest)\n\
         [Console]::In.ReadToEnd() | Out-Null\n\
         Write-Output '{\"is_error\":true,\"subtype\":\"error_during_execution\",\
         \"type\":\"result\",\"result\":\"Claude usage limit reached. Resets at 4pm.\"}'\n\
         exit 0\n",
    )
    .is_ok());
    let original = prepend_path(&dir);
    let roots = hide_native_installs(&dir);

    let Some(spec) = fixture_spec("claude") else {
        panic!("the catalogue must know Claude Code");
    };
    let Some(provider) = CliProvider::open(spec) else {
        panic!("the stub must resolve as a launcher");
    };

    let mut failure = None;
    if let Ok(mut stream) = provider.complete(request()).await {
        while let Some(item) = stream.next().await {
            if let Err(error) = item {
                failure = Some(error.to_string());
                break;
            }
        }
    }

    if let Some(path) = original {
        std::env::set_var("PATH", path);
    }
    restore_env(roots);
    let _ignored = std::fs::remove_dir_all(&dir);

    let Some(failure) = failure else {
        panic!("an is_error result must reach the caller as a failure");
    };
    assert!(failure.contains("usage limit reached"), "{failure}");
    assert!(
        !failure.contains("answered with nothing"),
        "the vendor explained itself; that explanation must not be replaced with a \
         guess: {failure}"
    );
}

/// The regression pin for the turn that died on `unknown option '--→ · ##'`.
///
/// The engineered prompt is tens of kilobytes, and it contains lines that begin with `--`
/// and words in quotes — it is a document, not a word. Handed to Claude Code as an argv
/// element it survived Rust's own spawn and was then re-split by npm's Windows launcher,
/// so a line from the middle of the prompt arrived at the CLI as a flag and the turn
/// failed instantly with what looked like an out-of-date CLI.
///
/// The stub records both channels: everything it was given in argv, and everything it
/// read from stdin. The prompt must be wholly in the second and nowhere in the first.
#[tokio::test]
async fn the_prompt_reaches_the_cli_on_stdin_and_never_through_argv() {
    const SYSTEM: &str = "You are the engine.\n--strict-mcp-config is not yours to send.\n\
                          Use the \"quoted\" verb, never a bare one.";
    const MESSAGE: &str = "→ · ## build a platformer\n--add-dir C:\\Games\n\"go\"";

    let _serialised = path_lock().await;
    let dir = scratch("stdin");
    let stdin_capture = dir.join("stdin.txt");
    let argv_capture = dir.join("argv.txt");
    // The reader is built by hand rather than using `[Console]::In`, which decodes a
    // redirected pipe with the console's OEM code page and would mangle the non-ASCII
    // characters this test cares about. The vendor reads stdin as UTF-8; so does this.
    let script = format!(
        "param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Rest)\n\
         $reader = [System.IO.StreamReader]::new([Console]::OpenStandardInput(), \
         [System.Text.UTF8Encoding]::new($false))\n\
         [System.IO.File]::WriteAllText('{stdin}', $reader.ReadToEnd())\n\
         [System.IO.File]::WriteAllLines('{argv}', [string[]]$Rest)\n\
         Write-Output '{{\"is_error\":false,\"subtype\":\"success\",\"type\":\"result\",\
         \"result\":\"ok\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}'\n",
        stdin = stdin_capture.display(),
        argv = argv_capture.display(),
    );
    assert!(std::fs::write(dir.join("bhippi-fixture-claude.ps1"), script).is_ok());
    let original = prepend_path(&dir);
    let roots = hide_native_installs(&dir);

    let Some(spec) = fixture_spec("claude") else {
        panic!("the catalogue must know Claude Code");
    };
    let Some(provider) = CliProvider::open(spec) else {
        panic!("the stub must resolve as a launcher");
    };

    let request = CompletionRequest::new(
        TaskClass::Expander,
        SYSTEM,
        vec![Message::user(MESSAGE.to_owned())],
    )
    .with_model(Some("haiku".to_owned()));
    let mut answer = String::new();
    match provider.complete(request).await {
        Ok(mut stream) => {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(Delta::Text { delta }) => answer.push_str(&delta),
                    Ok(Delta::Done { .. }) => break,
                    Ok(_) => {}
                    Err(error) => panic!("the stub must not fail: {error}"),
                }
            }
        }
        Err(error) => panic!("the stub must start: {error}"),
    }

    let seen_stdin = std::fs::read_to_string(&stdin_capture).unwrap_or_default();
    let seen_argv = std::fs::read_to_string(&argv_capture).unwrap_or_default();

    if let Some(path) = original {
        std::env::set_var("PATH", path);
    }
    restore_env(roots);
    let _ignored = std::fs::remove_dir_all(&dir);

    assert_eq!(answer, "ok");
    // Line endings are the console's business; the text is ours.
    let seen_stdin = seen_stdin.replace("\r\n", "\n");
    assert!(
        seen_stdin.contains(SYSTEM),
        "the system prompt never reached stdin: {seen_stdin:?}"
    );
    assert!(
        seen_stdin.contains(&MESSAGE.replace("\r\n", "\n")),
        "the message never reached stdin: {seen_stdin:?}"
    );
    for fragment in [
        "--strict-mcp-config is not yours",
        "--add-dir C:\\Games",
        "quoted",
    ] {
        assert!(
            !seen_argv.contains(fragment),
            "{fragment:?} reached the CLI as an argument, which is the bug: {seen_argv:?}"
        );
    }
    // The recipe still has to arrive, model flag included — an empty argv would make the
    // negative assertions above pass for the wrong reason.
    //
    // Only what a PowerShell shim actually forwards is asserted here: this stub is
    // reached through one, and its parameter binder silently eats `-p`, `--output-format`
    // and `--verbose` on the way. That is the same class of damage this whole change is
    // about, and the reason the native binary is now preferred; the exact argv is pinned
    // where no shell can touch it, in `cli::tests::claudes_prompt_never_appears_in_argv`.
    let argv: Vec<&str> = seen_argv.lines().collect();
    assert!(argv.contains(&"--strict-mcp-config"), "{argv:?}");
    assert!(argv.contains(&"--include-partial-messages"), "{argv:?}");
    assert!(
        argv.windows(2).any(|pair| pair == ["--model", "haiku"]),
        "{argv:?}"
    );
}

/// The regression pin for "provider OpenCode unavailable" on every real turn.
///
/// `opencode run [message..]` takes its message as a **positional array**, so the whole
/// engineered turn became one argv element. Windows caps an entire command line at 32,767
/// characters and a turn is 30-60 KB, so `CreateProcess` refused it with os error 206
/// *before OpenCode started* - which is why the backend probed healthy, answered a one-line
/// question, and then failed on everything the studio actually sends.
///
/// So the size is the test. The prompt here is deliberately larger than the cap: on the old
/// recipe this cannot spawn at all, and no amount of stub cooperation would let it. It also
/// checks the same thing the Claude pin checks - the text is wholly on stdin and nowhere in
/// argv - because a `--`-shaped line inside a prompt is a flag if it ever reaches argv.
#[tokio::test]
async fn an_opencode_turn_larger_than_the_windows_argv_cap_still_runs() {
    /// Windows' documented ceiling for a whole command line.
    const WINDOWS_COMMAND_LINE_CAP: usize = 32_767;

    let _serialised = path_lock().await;
    let dir = scratch("opencode-oversize");
    let stdin_capture = dir.join("stdin.txt");
    let argv_capture = dir.join("argv.txt");

    // OpenCode-shaped `--format json` events: one text part, then a step_finish carrying
    // the token counts the usage ledger reads.
    let script = format!(
        "param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Rest)\n\
         $reader = [System.IO.StreamReader]::new([Console]::OpenStandardInput(), \
         [System.Text.UTF8Encoding]::new($false))\n\
         [System.IO.File]::WriteAllText('{stdin}', $reader.ReadToEnd())\n\
         [System.IO.File]::WriteAllLines('{argv}', [string[]]$Rest)\n\
         Write-Output '{{\"type\":\"text\",\"part\":{{\"type\":\"text\",\"text\":\"ok\"}}}}'\n\
         Write-Output '{{\"type\":\"step_finish\",\"part\":{{\"type\":\"step-finish\",\
         \"tokens\":{{\"input\":9,\"output\":1}}}}}}'\n",
        stdin = stdin_capture.display(),
        argv = argv_capture.display(),
    );
    assert!(std::fs::write(dir.join("bhippi-fixture-opencode.ps1"), script).is_ok());
    let original = prepend_path(&dir);
    let roots = hide_native_installs(&dir);

    let Some(spec) = fixture_spec("opencode") else {
        panic!("the catalogue must know OpenCode");
    };
    assert!(
        spec.prompt_via_stdin,
        "this test is meaningless if the recipe puts the turn back in argv"
    );
    let Some(provider) = CliProvider::open(spec) else {
        panic!("the stub must resolve as a launcher");
    };

    // A turn the old recipe could not have spawned. The marker is at the far end so a
    // truncating path fails rather than passing on the prefix.
    let bulk = "Build the level. Keep every rule already agreed. ".repeat(900);
    let message = format!("{bulk}\n--end-of-turn marker: OVERSIZE_TAIL");
    assert!(
        message.len() > WINDOWS_COMMAND_LINE_CAP,
        "the prompt must exceed the cap or this proves nothing: {} bytes",
        message.len()
    );

    let request = CompletionRequest::new(
        TaskClass::Expander,
        "system",
        vec![Message::user(message.clone())],
    )
    .with_model(Some("opencode/big-pickle".to_owned()));

    let mut answer = String::new();
    let outcome = provider.complete(request).await;
    let mut failure = None;
    match outcome {
        Ok(mut stream) => {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(Delta::Text { delta }) => answer.push_str(&delta),
                    Ok(Delta::Done { .. }) => break,
                    Ok(_) => {}
                    Err(error) => {
                        failure = Some(error.to_string());
                        break;
                    }
                }
            }
        }
        Err(error) => failure = Some(error.to_string()),
    }

    let seen_stdin = std::fs::read_to_string(&stdin_capture).unwrap_or_default();
    let seen_argv = std::fs::read_to_string(&argv_capture).unwrap_or_default();

    if let Some(path) = original {
        std::env::set_var("PATH", path);
    }
    restore_env(roots);
    let _ignored = std::fs::remove_dir_all(&dir);

    if let Some(error) = failure {
        assert!(
            !error.contains("too long for a Windows command line"),
            "the turn is back in argv: {error}"
        );
        panic!("the stub must not fail: {error}");
    }
    assert_eq!(answer, "ok");

    let seen_stdin = seen_stdin.replace("\r\n", "\n");
    assert!(
        seen_stdin.contains("OVERSIZE_TAIL"),
        "the whole prompt must reach stdin, tail included ({} bytes seen)",
        seen_stdin.len()
    );
    assert!(
        !seen_argv.contains("OVERSIZE_TAIL") && !seen_argv.contains("Build the level"),
        "no part of the turn may travel in argv: {seen_argv:?}"
    );
    // What argv *should* carry: the subcommand, the recipe's flags, and the model.
    assert!(seen_argv.contains("run"), "{seen_argv:?}");
    assert!(seen_argv.contains("--thinking"), "{seen_argv:?}");
    assert!(seen_argv.contains("opencode/big-pickle"), "{seen_argv:?}");
    assert!(
        seen_argv.len() < WINDOWS_COMMAND_LINE_CAP,
        "argv must stay far below the cap: {} bytes",
        seen_argv.len()
    );
}

#[tokio::test]
async fn a_codex_turn_larger_than_the_windows_argv_cap_still_runs() {
    /// Windows' documented ceiling for a whole command line.
    const WINDOWS_COMMAND_LINE_CAP: usize = 32_767;

    let _serialised = path_lock().await;
    let dir = scratch("codex-oversize");
    let stdin_capture = dir.join("stdin.txt");
    let argv_capture = dir.join("argv.txt");

    // OpenCode-shaped `--format json` events: one text part, then a step_finish carrying
    // the token counts the usage ledger reads.
    let script = format!(
        "param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Rest)\n\
         $reader = [System.IO.StreamReader]::new([Console]::OpenStandardInput(), \
         [System.Text.UTF8Encoding]::new($false))\n\
         [System.IO.File]::WriteAllText('{stdin}', $reader.ReadToEnd())\n\
         [System.IO.File]::WriteAllLines('{argv}', [string[]]$Rest)\n\
         Write-Output '{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"ok\"}}}}'\n",
        stdin = stdin_capture.display(),
        argv = argv_capture.display(),
    );
    assert!(std::fs::write(dir.join("bhippi-fixture-codex.ps1"), script).is_ok());
    let original = prepend_path(&dir);
    let roots = hide_native_installs(&dir);

    let Some(spec) = fixture_spec("codex") else {
        panic!("the catalogue must know OpenCode");
    };
    assert!(
        spec.prompt_via_stdin,
        "this test is meaningless if the recipe puts the turn back in argv"
    );
    let Some(provider) = CliProvider::open(spec) else {
        panic!("the stub must resolve as a launcher");
    };

    // A turn the old recipe could not have spawned. The marker is at the far end so a
    // truncating path fails rather than passing on the prefix.
    let bulk = "Build the level. Keep every rule already agreed. ".repeat(900);
    let message = format!("{bulk}\n--end-of-turn marker: OVERSIZE_TAIL");
    assert!(
        message.len() > WINDOWS_COMMAND_LINE_CAP,
        "the prompt must exceed the cap or this proves nothing: {} bytes",
        message.len()
    );

    let request = CompletionRequest::new(
        TaskClass::Expander,
        "system",
        vec![Message::user(message.clone())],
    )
    .with_model(Some("gpt-5.4".to_owned()));

    let mut answer = String::new();
    let outcome = provider.complete(request).await;
    let mut failure = None;
    match outcome {
        Ok(mut stream) => {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(Delta::Text { delta }) => answer.push_str(&delta),
                    Ok(Delta::Done { .. }) => break,
                    Ok(_) => {}
                    Err(error) => {
                        failure = Some(error.to_string());
                        break;
                    }
                }
            }
        }
        Err(error) => failure = Some(error.to_string()),
    }

    let seen_stdin = std::fs::read_to_string(&stdin_capture).unwrap_or_default();
    let seen_argv = std::fs::read_to_string(&argv_capture).unwrap_or_default();

    if let Some(path) = original {
        std::env::set_var("PATH", path);
    }
    restore_env(roots);
    let _ignored = std::fs::remove_dir_all(&dir);

    if let Some(error) = failure {
        assert!(
            !error.contains("too long for a Windows command line"),
            "the turn is back in argv: {error}"
        );
        panic!("the stub must not fail: {error}");
    }
    assert_eq!(answer, "ok");

    let seen_stdin = seen_stdin.replace("\r\n", "\n");
    assert!(
        seen_stdin.contains("OVERSIZE_TAIL"),
        "the whole prompt must reach stdin, tail included ({} bytes seen)",
        seen_stdin.len()
    );
    assert!(
        !seen_argv.contains("OVERSIZE_TAIL") && !seen_argv.contains("Build the level"),
        "no part of the turn may travel in argv: {seen_argv:?}"
    );
    // What argv *should* carry: the subcommand, the recipe's flags, and the model.
    assert!(seen_argv.contains("exec"), "{seen_argv:?}");
    assert!(seen_argv.contains("--json"), "{seen_argv:?}");
    assert!(seen_argv.contains("gpt-5.4"), "{seen_argv:?}");
    assert!(
        seen_argv.len() < WINDOWS_COMMAND_LINE_CAP,
        "argv must stay far below the cap: {} bytes",
        seen_argv.len()
    );
}

#[tokio::test]
async fn partial_output_followed_by_a_failed_exit_is_not_success() {
    let _serialised = path_lock().await;
    let dir = scratch("partial-exit");
    let shim = dir.join("bhippi-fixture-claude.ps1");
    assert!(std::fs::write(
        &shim,
        "param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Rest)\n\
         [Console]::In.ReadToEnd() | Out-Null\n\
         Write-Output '{\"type\":\"stream_event\",\"event\":{\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"partial\"}}}'\n\
         [Console]::Error.WriteLine('vendor connection lost')\n\
         exit 1\n",
    )
    .is_ok());
    let original = prepend_path(&dir);
    let roots = hide_native_installs(&dir);

    let Some(spec) = fixture_spec("claude") else {
        panic!("the catalogue must know Claude Code");
    };
    let Some(provider) = CliProvider::open(spec) else {
        panic!("the stub must resolve as a launcher");
    };

    let mut failure = None;
    let mut answer = String::new();
    if let Ok(mut stream) = provider.complete(request()).await {
        while let Some(item) = stream.next().await {
            match item {
                Ok(Delta::Text { delta }) => answer.push_str(&delta),
                Err(error) => {
                    failure = Some(error.to_string());
                    break;
                }
                Ok(_) => {}
            }
        }
    }

    if let Some(path) = original {
        std::env::set_var("PATH", path);
    }
    restore_env(roots);
    let _ignored = std::fs::remove_dir_all(&dir);

    assert_eq!(
        answer, "partial",
        "partial output must remain available before failure"
    );
    let Some(failure) = failure else {
        panic!("an is_error result must reach the caller as a failure");
    };
    assert!(failure.contains("vendor connection lost"), "{failure}");
    assert!(
        !failure.contains("answered with nothing"),
        "the vendor explained itself; that explanation must not be replaced with a \
         guess: {failure}"
    );
}
