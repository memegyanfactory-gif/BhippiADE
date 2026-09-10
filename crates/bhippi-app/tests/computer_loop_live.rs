//! Live, end-to-end proofs of the Computer Use loop.
//!
//! These run the exact seams a real turn uses - capture -> screenshot file -> computer-mode
//! request -> provider -> action envelope -> interpretation -> validation -> execution ->
//! cursor check - against the real live desktop. All are `#[ignore]`d and gated to Windows
//! because two of them move the real cursor.
//!
//! Run them deliberately with:
//!
//!   cargo test -p bhippi-app --test computer_loop_live -- --ignored --nocapture
//!
//! Three tests:
//! - `synthetic_vision_agent_completes_the_loop` is deterministic and needs no model: a
//!   stand-in agent always returns a valid mouse_move to the desktop centre. It proves the
//!   whole machine-side loop (capture, observation, parse, validation, execution, pointer
//!   landing) with the real pointer and a real screenshot.
//! - `real_vision_cli_answers_with_an_executable_action` drives an installed vendor CLI and
//!   puts its reply through `computer_loop::interpret_reply`, the same interpreter the live
//!   turn uses - so it fails for the reasons a real turn would, and prints the correction the
//!   model would have been sent (ADR-0048). The provider comes from `BHIPPI_LIVE_PROVIDER`
//!   (claude | codex | grok), default claude. A vendor account that is simply exhausted
//!   (limit / payment) skips itself; any other failure is a real regression and panics.
//! - `settling_is_bounded_whatever_the_screen_is_doing` reads the screen twice and moves
//!   nothing, checking that the settle ceiling is far above the cost of a capture pair.

#![cfg(windows)]

use bhippi_app::computer::{
    capture_screen, execute_action, parse_action_json, remove_capture, save_capture, screen_bounds,
    ComputerAction, ScreenBounds,
};
use bhippi_providers::model::CostClass;
use bhippi_providers::{spec, Capabilities, CliProvider, CompletionRequest, DeltaStream};
use bhippi_providers::{Delta, Message, Provider};
use bhippi_types::{Health, TaskClass};
use futures_util::StreamExt;
use std::time::Duration;

const COMPUTER_SYSTEM: &str = include_str!("../../../prompts/chat-computer-use.md");

const MOVE_ONLY_TASK: &str = "Using Computer Use, move the mouse cursor to the geometric \
                              centre of the screen. Do not click, right-click, double-click, \
                              scroll, type, press keys, drag, or open anything. Only move the \
                              pointer, then finish with a short plain summary of where you \
                              moved it.";

/// The pre-ADR-0048 observation shape, kept deliberately.
///
/// The live loop now builds its observation with `computer_loop::observation`. This one
/// stays as the *older* wording, so the CLI test proves a model can follow the protocol from
/// the prompt alone rather than from a block that happens to match this month's phrasing.
fn observation(capture: &bhippi_app::computer::ScreenCapture, path: &std::path::Path) -> String {
    format!(
        "Initial desktop observation.\nCurrent desktop screenshot: {}\nVirtual desktop origin: ({}, {})\nVirtual desktop size: {}x{}\nInspect this exact current image before choosing one next action. Return no action block when the user's task is complete.",
        path.display(),
        capture.origin_x,
        capture.origin_y,
        capture.width,
        capture.height,
    )
}

/// A deliberately naive tag scrape, used only by the synthetic agent below.
///
/// The real path is `computer_loop::interpret_reply`, which is what the CLI test uses. This
/// stays simple so the synthetic test exercises execution rather than interpretation.
fn extract_actions(text: &str) -> Vec<ComputerAction> {
    let mut results = Vec::new();
    let mut cursor = 0;
    while let Some(start_tag) = text[cursor..].find("<computer_action>") {
        let content_start = cursor + start_tag + "<computer_action>".len();
        if let Some(end_tag) = text[content_start..].find("</computer_action>") {
            let json_str = text[content_start..content_start + end_tag].trim();
            if let Some(action) = parse_action_json(json_str) {
                results.push(action);
            }
            cursor = content_start + end_tag + "</computer_action>".len();
        } else {
            break;
        }
    }
    results
}

async fn read_cursor() -> (i32, i32) {
    let result = execute_action(ComputerAction::GetCursorPosition)
        .await
        .unwrap_or_else(|error| panic!("cursor must be readable on the live desktop: {error}"));
    result
        .cursor
        .unwrap_or_else(|| panic!("cursor result must carry the position"))
}

fn vendor_exhausted(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "exhausted",
        "session limit",
        "usage limit",
        "payment",
        "402",
        "credits",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Wraps a stream future in the same generous timeout the engine applies.
async fn timeout_call<T>(
    future: impl std::future::Future<Output = Result<T, bhippi_types::BhippiError>>,
) -> Result<T, String> {
    match tokio::time::timeout(Duration::from_secs(300), future).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("timed out after 300s".to_owned()),
    }
}

/// A deterministic stand-in for a vision CLI: it always answers with a single reversible
/// `mouse_move` to the requested desktop centre. Proves the full machine-side loop without
/// spending a vendor token.
struct SyntheticVisionAgent {
    bounds: ScreenBounds,
    caps: Capabilities,
}

#[async_trait::async_trait]
impl Provider for SyntheticVisionAgent {
    fn id(&self) -> &str {
        "synthetic-vision"
    }

    fn caps(&self) -> &Capabilities {
        &self.caps
    }

    async fn complete(&self, _req: CompletionRequest) -> bhippi_types::Result<DeltaStream> {
        let centre_x = i64::from(self.bounds.origin_x) + i64::from(self.bounds.width / 2);
        let centre_y = i64::from(self.bounds.origin_y) + i64::from(self.bounds.height / 2);
        let payload = format!(
            "I can see the desktop.\n<computer_action>\n{{\"type\":\"mouse_move\",\"x\":{centre_x},\"y\":{centre_y}}}\n</computer_action>\nMoved the pointer to the centre of the screen."
        );
        let stream = futures_util::stream::iter(vec![
            Ok(Delta::Text { delta: payload }),
            Ok(Delta::Done {
                stop_reason: bhippi_providers::StopReason::Completed,
            }),
        ])
        .boxed();
        Ok(stream)
    }

    async fn health(&self) -> Health {
        Health::Healthy { latency_ms: 0 }
    }
}

#[tokio::test]
#[ignore = "moves the real cursor on the live desktop"]
async fn synthetic_vision_agent_completes_the_loop() {
    let bounds = screen_bounds()
        .await
        .unwrap_or_else(|error| panic!("desktop bounds must be readable: {error}"));
    let before = read_cursor().await;

    let capture = capture_screen()
        .await
        .unwrap_or_else(|error| panic!("live screenshot must succeed: {error}"));
    let capture_path = save_capture(&capture, "live-loop")
        .await
        .unwrap_or_else(|error| panic!("screenshot must be written: {error}"));

    let system =
        format!("{COMPUTER_SYSTEM}\n\nMouse and keyboard input are authorised for this turn.");
    let mut request = CompletionRequest::new(
        TaskClass::Expander,
        system,
        vec![
            Message::user(MOVE_ONLY_TASK.to_owned()),
            Message::user(observation(&capture, &capture_path)),
        ],
    );
    request.max_tokens = 512;
    request.timeout = Duration::from_secs(60);
    let request = request
        .for_computer_use()
        .with_images(vec![capture_path.to_string_lossy().into_owned()]);

    let provider = SyntheticVisionAgent {
        bounds,
        caps: Capabilities {
            context_window: 64_000,
            vision: true,
            tools: false,
            streaming: true,
            tokens_per_second: None,
            cost_class: CostClass::FreeLocal,
        },
    };

    let raw_text = match timeout_call(provider.complete(request.clone())).await {
        Ok(stream) => {
            let mut out = String::new();
            let mut stream = stream;
            while let Some(item) = stream.next().await {
                if let Ok(Delta::Text { delta }) = item {
                    out.push_str(&delta);
                }
            }
            out
        }
        Err(error) => panic!("synthetic agent must answer: {error}"),
    };

    let mut actions = extract_actions(&raw_text);
    assert_eq!(actions.len(), 1, "expected one action, got:\n{raw_text}");
    let action = actions.remove(0);
    let ComputerAction::MouseMove { x, y } = action else {
        panic!("synthetic agent must emit a mouse_move, got {action:?}");
    };
    action
        .validate(bounds)
        .unwrap_or_else(|error| panic!("centre must be on-screen: {error}"));

    let result = execute_action(action.clone())
        .await
        .unwrap_or_else(|error| panic!("mouse move must execute: {error}"));
    let landed = result
        .cursor
        .unwrap_or_else(|| panic!("execute result must carry the cursor position"));
    assert_eq!(
        landed,
        (x, y),
        "pointer must land exactly where the action asked"
    );

    execute_action(ComputerAction::MouseMove {
        x: before.0,
        y: before.1,
    })
    .await
    .unwrap_or_else(|error| panic!("pointer restore must succeed: {error}"));
    remove_capture(&capture_path).await;
}

#[tokio::test]
#[ignore = "moves the real cursor and spends tokens with a live vision CLI"]
async fn real_vision_cli_answers_with_an_executable_action() {
    let provider_id = std::env::var("BHIPPI_LIVE_PROVIDER").unwrap_or_else(|_| "claude".to_owned());
    let Some(provider_spec) = spec(&provider_id) else {
        eprintln!("SKIP: unknown provider {provider_id} for live computer test.");
        return;
    };
    let Some(provider) = CliProvider::open(provider_spec) else {
        eprintln!("SKIP: {provider_id} CLI is not installed on this machine.");
        return;
    };

    assert!(
        bhippi_app::computer::explicitly_requests_computer_use(MOVE_ONLY_TASK),
        "the intent gate must recognise this as a Computer Use request"
    );

    let bounds = screen_bounds()
        .await
        .unwrap_or_else(|error| panic!("desktop bounds must be readable: {error}"));
    let before = read_cursor().await;

    let capture = capture_screen()
        .await
        .unwrap_or_else(|error| panic!("live screenshot must succeed: {error}"));
    let capture_path = save_capture(&capture, "live-cli")
        .await
        .unwrap_or_else(|error| panic!("screenshot must be written: {error}"));

    let system =
        format!("{COMPUTER_SYSTEM}\n\nMouse and keyboard input are authorised for this turn.");
    let mut request = CompletionRequest::new(
        TaskClass::Expander,
        system,
        vec![
            Message::user(MOVE_ONLY_TASK.to_owned()),
            Message::user(observation(&capture, &capture_path)),
        ],
    )
    .for_computer_use()
    .with_images(vec![capture_path.to_string_lossy().into_owned()])
    .with_model(None);
    request.max_tokens = 2048;
    request.timeout = Duration::from_secs(180);

    let raw_text = match timeout_call(provider.complete(request.clone())).await {
        Ok(stream) => {
            let mut out = String::new();
            let mut stream = stream;
            let mut first_error = None;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(Delta::Text { delta }) => out.push_str(&delta),
                    Ok(Delta::Thinking { delta }) => eprintln!("THINKING: {delta}"),
                    Ok(Delta::Step { verb, title, .. }) => {
                        eprintln!("STEP: {verb} {title}");
                    }
                    Ok(Delta::Done { stop_reason }) => {
                        if stop_reason == bhippi_providers::StopReason::Cancelled {
                            eprintln!("WARN: provider reports the stream was cancelled");
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        first_error = Some(error.to_string());
                        break;
                    }
                }
            }
            if let Some(error) = first_error {
                if vendor_exhausted(&error) {
                    remove_capture(&capture_path).await;
                    eprintln!(
                        "SKIP: {provider_id} account is exhausted ({error}); \
                         a healthy account proves the loop."
                    );
                    return;
                }
                panic!("provider stream failed: {error}");
            }
            out
        }
        Err(error) => {
            remove_capture(&capture_path).await;
            if vendor_exhausted(&error) {
                eprintln!(
                    "SKIP: {provider_id} account is exhausted ({error}); \
                     a healthy account proves the loop."
                );
                return;
            }
            panic!("{provider_id} CLI could not answer in time: {error}");
        }
    };

    // ADR-0048: the reply goes through the same interpreter the live loop uses, so this test
    // fails for the same reasons a real turn would — and its failure message is the exact
    // correction the model would have been sent.
    let proposed = match bhippi_app::computer_loop::interpret_reply(
        &raw_text,
        bhippi_types::ComputerScope::Desktop,
    ) {
        bhippi_app::computer_loop::ReplyVerdict::Act { proposed, .. } => *proposed,
        bhippi_app::computer_loop::ReplyVerdict::Repair(kind) => {
            remove_capture(&capture_path).await;
            panic!(
                "{provider_id} did not follow the protocol: {}\nit would have been told:\n{}\n\nreply:\n{raw_text}",
                kind.summary(),
                kind.message(bhippi_types::ComputerScope::Desktop)
            );
        }
        bhippi_app::computer_loop::ReplyVerdict::Complete { summary } => {
            remove_capture(&capture_path).await;
            panic!("{provider_id} claimed completion without acting: {summary}");
        }
    };
    // The reason is required by the prompt and optional in the parser, so a CLI that omits
    // it is reported rather than failed — making it fatal would break an older backend over
    // a caption.
    match proposed.reason.as_deref() {
        Some(reason) => eprintln!("{provider_id} gave a reason: {reason}"),
        None => eprintln!("NOTE: {provider_id} sent no reason; the overlay caption reads thinner"),
    }
    let action = proposed.action;
    let ComputerAction::MouseMove { x, y } = action else {
        remove_capture(&capture_path).await;
        panic!(
            "model returned {action:?} despite an explicit move-only instruction; refusing to execute it live"
        );
    };
    action
        .validate(bounds)
        .unwrap_or_else(|error| panic!("provider action must be on-screen: {error}"));

    let result = execute_action(action.clone())
        .await
        .unwrap_or_else(|error| panic!("mouse move must execute: {error}"));
    let landed = result
        .cursor
        .unwrap_or_else(|| panic!("execute result must carry the cursor position"));
    assert_eq!(landed, (x, y), "pointer must land where the action asked");

    execute_action(ComputerAction::MouseMove {
        x: before.0,
        y: before.1,
    })
    .await
    .unwrap_or_else(|error| panic!("pointer restore must succeed: {error}"));
    remove_capture(&capture_path).await;

    eprintln!("OK: {provider_id} returned a valid {action:?}; pointer verified at {landed:?}.");
}

/// The settle detector, against a screen that is actually moving (ADR-0048 section 5).
///
/// The unit tests prove the hash discriminates; only a live screen proves the loop around it
/// terminates. Both endings are correct - a still screen settles on the first comparison, a
/// moving one (a clock, a caret, an animation) runs to the ceiling - so the assertion is on
/// the bound, which is the property that matters: a turn must not hang waiting for a screen
/// to hold still. Nothing is executed here; it reads the screen and moves nothing.
#[tokio::test]
#[ignore = "captures the live desktop twice"]
async fn settling_is_bounded_whatever_the_screen_is_doing() {
    let started = std::time::Instant::now();
    let first = capture_screen()
        .await
        .unwrap_or_else(|error| panic!("live screenshot must succeed: {error}"));
    let second = capture_screen()
        .await
        .unwrap_or_else(|error| panic!("live screenshot must succeed: {error}"));
    let still = bhippi_app::computer_loop::frame_hash(&first)
        == bhippi_app::computer_loop::frame_hash(&second);
    eprintln!(
        "two consecutive captures {} in {:?}",
        if still {
            "matched - the screen is still"
        } else {
            "differed - something on screen is animating"
        },
        started.elapsed()
    );
    assert!(
        started.elapsed()
            < Duration::from_millis(bhippi_types::COMPUTER_SETTLE_TIMEOUT_MS.saturating_mul(4)),
        "capturing twice must cost far less than the settle budget, or the ceiling is wrong"
    );
}
