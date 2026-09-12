//! Live proof that an OpenCode turn the size of a real one actually runs.
//!
//! The bug this exists for looked like an install problem and was not: `opencode run
//! [message..]` takes its message as a positional array, so Bhippi's engineered turn became
//! one 30-60 KB argv element, and Windows refuses a command line over 32,767 characters.
//! `CreateProcess` failed with os error 206 *before OpenCode started*, so the backend probed
//! healthy, answered a one-line question, and died on every turn the studio actually sends -
//! reported to the user as "provider OpenCode unavailable".
//!
//! `tests/streaming.rs` pins the wiring against a stub. This one spends real tokens on the
//! real CLI, because the thing worth knowing is that OpenCode itself reads a prompt this big
//! from stdin - a fact about the vendor, which no stub can establish.
//!
//! Run it deliberately:
//!
//!   cargo test -p bhippi-providers --test opencode_live -- --ignored --nocapture

#![cfg(windows)]

use bhippi_providers::model::{CompletionRequest, Delta};
use bhippi_providers::{CliProvider, Message, Provider};
use bhippi_types::TaskClass;
use futures_util::StreamExt;
use std::time::Duration;

/// Windows' documented ceiling for a whole command line.
const WINDOWS_COMMAND_LINE_CAP: usize = 32_767;

#[tokio::test]
#[ignore = "runs the real OpenCode CLI and spends its tokens"]
async fn a_turn_bigger_than_the_argv_cap_reaches_the_real_opencode() {
    let Some(spec) = bhippi_providers::spec("opencode") else {
        panic!("the catalogue must know OpenCode");
    };
    let Some(provider) = CliProvider::open(spec) else {
        eprintln!("SKIP: the opencode CLI is not installed on this machine.");
        return;
    };

    // Filler that reads like a turn rather than noise, with the instruction at the end so a
    // truncating path fails instead of passing on the prefix.
    let bulk = "Context line: the project is a small Godot platformer and every rule \
                already agreed still stands.\n"
        .repeat(340);
    let message = format!(
        "{bulk}\nIgnore all the context above. Reply with exactly this word and nothing \
         else: OVERSIZE_OK"
    );
    assert!(
        message.len() > WINDOWS_COMMAND_LINE_CAP,
        "the prompt must exceed the cap or this proves nothing: {} bytes",
        message.len()
    );
    eprintln!(
        "sending {} bytes ({}x the Windows argv cap)",
        message.len(),
        message.len() / WINDOWS_COMMAND_LINE_CAP
    );

    let mut request = CompletionRequest::new(
        TaskClass::Expander,
        "You follow the last instruction in the message exactly.",
        vec![Message::user(message)],
    );
    request.timeout = Duration::from_secs(180);

    let mut answer = String::new();
    match provider.complete(request).await {
        Ok(mut stream) => {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(Delta::Text { delta }) => answer.push_str(&delta),
                    Ok(Delta::Done { .. }) => break,
                    Ok(_) => {}
                    Err(error) => {
                        let error = error.to_string();
                        assert!(
                            !error.contains("too long for a Windows command line"),
                            "the turn is back in argv: {error}"
                        );
                        if error.contains("usage limit") || error.contains("credits") {
                            eprintln!("SKIP: the OpenCode account is exhausted ({error}).");
                            return;
                        }
                        panic!("the live turn failed: {error}");
                    }
                }
            }
        }
        Err(error) => {
            let error = error.to_string();
            assert!(
                !error.contains("too long for a Windows command line"),
                "the turn is back in argv: {error}"
            );
            panic!("OpenCode could not start: {error}");
        }
    }

    assert!(
        answer.contains("OVERSIZE_OK"),
        "the tail of the prompt must have reached the model; it answered: {answer:?}"
    );
    eprintln!("OK: OpenCode read the whole turn from stdin and answered {answer:?}");
}
