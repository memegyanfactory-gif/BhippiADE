//! Renderer observation bridge for the autonomous engine loop (ENG-186/187).
//!
//! Rendering and play simulation live in the webview by ADR-0028, while the model loop
//! lives in Rust. This is the narrow request/response seam: Rust emits a typed request, the
//! active Engine pane answers once, and a bounded one-shot returns the result. No frame
//! traffic crosses IPC.

use crate::commands::AppError;
use base64::Engine as _;
use bhippi_engine::game_test_plan::{GameTestBatchEvidence, GameTestPlan};
use bhippi_types::{
    TransactionId, ENGINE_OBSERVATION_TIMEOUT_SECS, ENGINE_PLAYTEST_MAX_FRAMES_PER_STEP,
    ENGINE_PLAYTEST_MAX_KEYS_PER_STEP, ENGINE_PLAYTEST_MAX_KEY_CODE_BYTES,
    ENGINE_PLAYTEST_MAX_STEPS, ENGINE_SCREENSHOT_MAX_BYTES, ENGINE_SCREENSHOT_MAX_DIMENSION,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri_specta::Event;
use tokio::sync::oneshot;

#[derive(Clone, Debug, Deserialize, Serialize, Type, Event)]
pub struct EngineScreenshotRequested {
    pub request_id: String,
    pub camera: String,
    pub annotate: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type, Event)]
pub struct EnginePlaytestRequested {
    pub request_id: String,
    pub steps_json: String,
    pub fixed_delta_seconds: f32,
    /// Rust-owned ceiling for the worker request; the outer one-shot uses the same bound.
    pub watchdog_millis: u64,
}

/// A validated authored scenario plan ready for execution in fresh disposable workers.
///
/// The plan crosses IPC as canonical JSON so the webview cannot reinterpret an invalid Rust
/// document. The authored hash is retained in the pending request and must match the submitted
/// batch evidence byte-for-byte.
#[derive(Clone, Debug, Deserialize, Serialize, Type, Event)]
pub struct EngineGameTestBatchRequested {
    pub request_id: String,
    pub plan_json: String,
    pub authored_tree_hash: String,
    pub fixed_delta_seconds: f32,
    /// Rust-owned ceiling for the whole batch request; every scenario still runs in its own
    /// disposable worker and reports its own sandbox budgets.
    pub watchdog_millis: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct EngineObservationResult {
    pub path: Option<String>,
    pub report: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PlaytestStep {
    #[serde(default)]
    keys: Vec<String>,
    frames: u32,
    #[serde(default)]
    note: Option<String>,
}

/// Validate and canonicalise the model's playtest plan before the webview sees it.
pub fn playtest_steps(payload: &str) -> Result<String, AppError> {
    let value: serde_json::Value = serde_json::from_str(payload).map_err(|error| AppError {
        message: format!("That playtest query is not valid JSON: {error}"),
        hint: Some(
            "Use {\"kind\":\"playtest\",\"steps\":[{\"keys\":[\"KeyW\"],\"frames\":60}]}"
                .to_owned(),
        ),
    })?;
    let raw = value.get("steps").cloned().ok_or_else(|| AppError {
        message: "A playtest query needs a steps array.".to_owned(),
        hint: Some("Each step names keyboard codes and a frame count.".to_owned()),
    })?;
    let steps: Vec<PlaytestStep> = serde_json::from_value(raw).map_err(|error| AppError {
        message: format!("Those playtest steps are invalid: {error}"),
        hint: Some(
            "Each step is {\"keys\":[\"KeyW\"],\"frames\":60,\"note\":\"walk forward\"}."
                .to_owned(),
        ),
    })?;
    if steps.is_empty() || steps.len() > ENGINE_PLAYTEST_MAX_STEPS {
        return Err(AppError {
            message: format!("A playtest needs 1 to {ENGINE_PLAYTEST_MAX_STEPS} input steps."),
            hint: Some("Split a longer test into several observations.".to_owned()),
        });
    }
    for (index, step) in steps.iter().enumerate() {
        if step.frames == 0 || step.frames > ENGINE_PLAYTEST_MAX_FRAMES_PER_STEP {
            return Err(AppError {
                message: format!(
                    "Playtest step {} must run for 1 to {ENGINE_PLAYTEST_MAX_FRAMES_PER_STEP} frames.",
                    index + 1
                ),
                hint: Some("At 60 Hz, 60 frames is one simulated second.".to_owned()),
            });
        }
        if step.keys.len() > ENGINE_PLAYTEST_MAX_KEYS_PER_STEP
            || step
                .keys
                .iter()
                .any(|key| key.len() > ENGINE_PLAYTEST_MAX_KEY_CODE_BYTES)
        {
            return Err(AppError {
                message: format!(
                    "Playtest step {} has too many or invalid key codes.",
                    index + 1
                ),
                hint: Some(
                    "Use browser KeyboardEvent.code names such as KeyW or Space.".to_owned(),
                ),
            });
        }
    }
    serde_json::to_string(&steps).map_err(|error| AppError {
        message: format!("Could not encode the validated playtest: {error}"),
        hint: Some("Retry the playtest query.".to_owned()),
    })
}

enum PendingKind {
    Screenshot,
    Playtest,
    GameTestBatch {
        plan: GameTestPlan,
        authored_tree_hash: String,
    },
}

struct Pending {
    game_dir: PathBuf,
    kind: PendingKind,
    tx: oneshot::Sender<Result<EngineObservationResult, AppError>>,
}

fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

type ObservationReceiver = oneshot::Receiver<Result<EngineObservationResult, AppError>>;

fn insert(game_dir: &Path, kind: PendingKind) -> Result<(String, ObservationReceiver), AppError> {
    let request_id = TransactionId::new().to_string();
    let (tx, rx) = oneshot::channel();
    let mut requests = pending().lock().map_err(|_| AppError {
        message: "The engine observation queue is unavailable.".to_owned(),
        hint: Some("Close and reopen the app, then retry the observation.".to_owned()),
    })?;
    requests.insert(
        request_id.clone(),
        Pending {
            game_dir: game_dir.to_path_buf(),
            kind,
            tx,
        },
    );
    Ok((request_id, rx))
}

async fn wait(
    request_id: &str,
    rx: ObservationReceiver,
) -> Result<EngineObservationResult, AppError> {
    wait_with_timeout(
        request_id,
        rx,
        std::time::Duration::from_secs(ENGINE_OBSERVATION_TIMEOUT_SECS),
    )
    .await
}

async fn wait_with_timeout(
    request_id: &str,
    rx: ObservationReceiver,
    timeout: std::time::Duration,
) -> Result<EngineObservationResult, AppError> {
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(AppError {
            message: "The Engine pane closed before it returned the observation.".to_owned(),
            hint: Some("Open the Engine pane and retry.".to_owned()),
        }),
        Err(_) => {
            if let Ok(mut requests) = pending().lock() {
                requests.remove(request_id);
            }
            Err(AppError {
                message: "The Engine pane did not answer the observation request in time."
                    .to_owned(),
                hint: Some("Keep the Engine pane open and visible, then retry.".to_owned()),
            })
        }
    }
}

pub async fn request_screenshot(
    app: &tauri::AppHandle,
    game_dir: &Path,
    camera: String,
    annotate: bool,
) -> Result<EngineObservationResult, AppError> {
    validate_camera(&camera)?;
    let (request_id, rx) = insert(game_dir, PendingKind::Screenshot)?;
    EngineScreenshotRequested {
        request_id: request_id.clone(),
        camera,
        annotate,
    }
    .emit(app)
    .map_err(|error| AppError {
        message: format!("Could not ask the viewport for a screenshot: {error}"),
        hint: Some("Open the Engine pane and retry.".to_owned()),
    })?;
    wait(&request_id, rx).await
}

fn validate_camera(camera: &str) -> Result<(), AppError> {
    let valid = matches!(camera, "editor" | "game")
        || camera
            .strip_prefix("entity:")
            .is_some_and(|id| !id.is_empty() && id.len() <= 128);
    if valid {
        Ok(())
    } else {
        Err(AppError {
            message: format!("Unknown screenshot camera `{camera}`."),
            hint: Some("Use editor, game, or entity:<camera entity id>.".to_owned()),
        })
    }
}

pub async fn request_playtest(
    app: &tauri::AppHandle,
    game_dir: &Path,
    steps_json: String,
) -> Result<EngineObservationResult, AppError> {
    let (request_id, rx) = insert(game_dir, PendingKind::Playtest)?;
    EnginePlaytestRequested {
        request_id: request_id.clone(),
        steps_json,
        fixed_delta_seconds: bhippi_types::ENGINE_PLAYTEST_FIXED_DELTA_SECONDS,
        watchdog_millis: ENGINE_OBSERVATION_TIMEOUT_SECS.saturating_mul(1_000),
    }
    .emit(app)
    .map_err(|error| AppError {
        message: format!("Could not ask the runtime for a playtest: {error}"),
        hint: Some("Open the Engine pane and retry.".to_owned()),
    })?;
    wait(&request_id, rx).await
}

/// Ask the Engine pane to execute an already validated authored scenario plan.
///
/// Validation and canonical serialisation happen again at this trust boundary so a caller cannot
/// mutate an earlier-validated value before it reaches the renderer. The exact plan and authored
/// hash are retained until the one-shot response arrives, then used to validate the evidence.
pub async fn request_game_test_batch(
    app: &tauri::AppHandle,
    game_dir: &Path,
    plan: GameTestPlan,
    authored_tree_hash: String,
) -> Result<EngineObservationResult, AppError> {
    let plan_json = plan.dump().map_err(super::engine_error)?;
    validate_authored_tree_hash(&authored_tree_hash)?;
    validate_current_authored_tree(game_dir, &authored_tree_hash)?;
    let (request_id, rx) = insert(
        game_dir,
        PendingKind::GameTestBatch {
            plan,
            authored_tree_hash: authored_tree_hash.clone(),
        },
    )?;
    let event = EngineGameTestBatchRequested {
        request_id: request_id.clone(),
        plan_json,
        authored_tree_hash,
        fixed_delta_seconds: bhippi_types::ENGINE_PLAYTEST_FIXED_DELTA_SECONDS,
        watchdog_millis: ENGINE_OBSERVATION_TIMEOUT_SECS.saturating_mul(1_000),
    };
    if let Err(error) = event.emit(app) {
        let _discarded = take(&request_id);
        return Err(AppError {
            message: format!("Could not ask the runtime for a game-test batch: {error}"),
            hint: Some("Open the Engine pane and retry.".to_owned()),
        });
    }
    wait(&request_id, rx).await
}

fn validate_authored_tree_hash(hash: &str) -> Result<(), AppError> {
    let valid = hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
    if valid {
        Ok(())
    } else {
        Err(AppError {
            message: "The game-test batch needs a canonical lowercase BLAKE3 authored-tree hash."
                .to_owned(),
            hint: Some("Hash the validated authored tree before starting any worker.".to_owned()),
        })
    }
}

fn validate_current_authored_tree(game_dir: &Path, expected_hash: &str) -> Result<(), AppError> {
    if bhippi_engine::game_debug::authored_tree_hash(game_dir) == expected_hash {
        Ok(())
    } else {
        Err(AppError {
            message: "The authored game changed while the game-test batch was running.".to_owned(),
            hint: Some(
                "Discard this stale evidence and rerun the plan from the current authored tree."
                    .to_owned(),
            ),
        })
    }
}

fn take(request_id: &str) -> Result<Pending, AppError> {
    pending()
        .lock()
        .map_err(|_| AppError::plain("The engine observation queue is unavailable."))?
        .remove(request_id)
        .ok_or_else(|| AppError {
            message: "That engine observation request is no longer active.".to_owned(),
            hint: Some("The request may have timed out; ask for a fresh observation.".to_owned()),
        })
}

#[tauri::command]
#[specta::specta]
pub async fn engine_submit_screenshot(
    request_id: String,
    image_base64: String,
    width: u32,
    height: u32,
) -> Result<(), AppError> {
    let request = take(&request_id)?;
    if !matches!(&request.kind, PendingKind::Screenshot) {
        return reject_wrong_response(request, "screenshot");
    }
    let estimated = image_base64.len().saturating_mul(3) / 4;
    let result = if estimated > ENGINE_SCREENSHOT_MAX_BYTES {
        Err(AppError {
            message: "The viewport screenshot is larger than the capture budget.".to_owned(),
            hint: Some("Lower screen percentage or use a smaller pane, then retry.".to_owned()),
        })
    } else {
        decode_and_write(&request.game_dir, &request_id, &image_base64, width, height)
    };
    let _ignored = request.tx.send(result);
    Ok(())
}

fn decode_and_write(
    game_dir: &Path,
    request_id: &str,
    image_base64: &str,
    width: u32,
    height: u32,
) -> Result<EngineObservationResult, AppError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image_base64.trim())
        .map_err(|error| AppError {
            message: format!("The viewport returned an invalid PNG payload: {error}"),
            hint: Some("Retry the screenshot from the Engine pane.".to_owned()),
        })?;
    let png_dimensions = png_dimensions(&bytes);
    if bytes.len() > ENGINE_SCREENSHOT_MAX_BYTES
        || png_dimensions != Some((width, height))
        || width == 0
        || height == 0
        || width > ENGINE_SCREENSHOT_MAX_DIMENSION
        || height > ENGINE_SCREENSHOT_MAX_DIMENSION
    {
        return Err(AppError {
            message: "The viewport capture is not a valid bounded PNG with matching dimensions."
                .to_owned(),
            hint: Some("Retry the screenshot from the Engine pane.".to_owned()),
        });
    }
    let dir = game_dir.join(".bhippi").join("engine").join("captures");
    std::fs::create_dir_all(&dir).map_err(|error| AppError {
        message: format!("Could not prepare the engine capture folder: {error}"),
        hint: Some("Check that the project folder is writable.".to_owned()),
    })?;
    let path = dir.join(format!("{request_id}.png"));
    std::fs::write(&path, bytes).map_err(|error| AppError {
        message: format!("Could not save the viewport screenshot: {error}"),
        hint: Some("Check that the project folder is writable.".to_owned()),
    })?;
    Ok(EngineObservationResult {
        path: Some(path.to_string_lossy().into_owned()),
        report: format!("Viewport captured at {width}×{height}."),
        width: Some(width),
        height: Some(height),
    })
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 45
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || &bytes[8..12] != 13_u32.to_be_bytes().as_slice()
        || &bytes[12..16] != b"IHDR"
        || &bytes[bytes.len() - 12..bytes.len() - 8] != 0_u32.to_be_bytes().as_slice()
        || &bytes[bytes.len() - 8..bytes.len() - 4] != b"IEND"
    {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    ))
}

#[tauri::command]
#[specta::specta]
pub async fn engine_submit_playtest(request_id: String, report: String) -> Result<(), AppError> {
    let request = take(&request_id)?;
    if !matches!(&request.kind, PendingKind::Playtest) {
        return reject_wrong_response(request, "playtest");
    }
    let result = if report.trim().is_empty() {
        Err(AppError {
            message: "The runtime returned an empty playtest report.".to_owned(),
            hint: Some("Run the scripted input sequence again.".to_owned()),
        })
    } else {
        Ok(EngineObservationResult {
            path: None,
            report,
            width: None,
            height: None,
        })
    };
    let _ignored = request.tx.send(result);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn engine_submit_game_test_batch(
    request_id: String,
    report: String,
) -> Result<(), AppError> {
    let request = take(&request_id)?;
    let PendingKind::GameTestBatch {
        plan,
        authored_tree_hash,
    } = &request.kind
    else {
        return reject_wrong_response(request, "game-test batch");
    };
    let result = validate_current_authored_tree(&request.game_dir, authored_tree_hash).and_then(
        |_| {
            GameTestBatchEvidence::parse(&report, plan)
            .map_err(super::engine_error)
            .and_then(|evidence| {
                if evidence.authored_tree_before != *authored_tree_hash
                    || evidence.authored_tree_after != *authored_tree_hash
                {
                    return Err(AppError {
                        message: "The game-test batch evidence does not match the authored tree that was requested."
                            .to_owned(),
                        hint: Some(
                            "Discard the batch and rerun every scenario from the current authored snapshot."
                                .to_owned(),
                        ),
                    });
                }
                evidence.dump(plan).map_err(super::engine_error)
            })
        },
    )
    .map(|canonical_report| EngineObservationResult {
        path: None,
        report: canonical_report,
        width: None,
        height: None,
    });
    let _ignored = request.tx.send(result);
    Ok(())
}

fn reject_wrong_response(request: Pending, submitted_kind: &str) -> Result<(), AppError> {
    let error = AppError {
        message: format!(
            "The Engine pane returned a {submitted_kind} response for a different request kind."
        ),
        hint: Some("Discard the stale response and retry the active engine request.".to_owned()),
    };
    let _ignored = request.tx.send(Err(error.clone()));
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_and_write, engine_submit_game_test_batch, insert, pending, playtest_steps,
        png_dimensions, take, validate_authored_tree_hash, validate_camera, wait_with_timeout,
        PendingKind,
    };
    use bhippi_engine::game_test_plan::{
        GameTestPlan, GAME_TEST_BATCH_FORMAT, GAME_TEST_PLAN_FORMAT,
    };

    fn smoke_batch_report(plan: &GameTestPlan, authored_hash: &str) -> String {
        let scenario = &plan.scenarios[0];
        let assertions = scenario
            .checkpoints
            .iter()
            .flat_map(|checkpoint| {
                checkpoint
                    .assertions
                    .iter()
                    .enumerate()
                    .map(move |(index, assertion)| {
                        serde_json::json!({
                            "checkpoint": checkpoint.name,
                            "assertion_index": index,
                            "passed": true,
                            "address": format!(
                                "runtime://checkpoint/{}/assertion/{index}",
                                checkpoint.name
                            ),
                            "observed": true,
                            "expected": assertion,
                        })
                    })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "format": GAME_TEST_BATCH_FORMAT,
            "plan_format": GAME_TEST_PLAN_FORMAT,
            "authored_tree_before": authored_hash,
            "authored_tree_after": authored_hash,
            "scenarios": [{
                "name": scenario.name,
                "initial_level": scenario.initial_level,
                "seed": scenario.seed,
                "worker_session_hash": format!("sha256:{}", "b".repeat(64)),
                "runtime": {
                    "protocol": "bhippi-runtime-protocol@1",
                    "execution": "application_module_worker",
                    "capabilities": [],
                    "budgets": {
                        "instructions_per_tick": 200000,
                        "instructions_total": 20000000,
                        "call_depth": 64,
                        "timers": 4096,
                        "heap_estimate_bytes": 67108864,
                        "wall_clock_millis": 300000,
                        "message_bytes": 1048576,
                        "messages_per_tick": 4096,
                        "spawned_entities": 4096,
                        "emitted_events": 16384,
                        "log_bytes": 1048576
                    },
                    "termination_reason": "completed",
                    "authored_hash_before": "fnv1a32:12345678",
                    "authored_hash_after": "fnv1a32:12345678",
                    "frames": 1,
                    "checkpoint_hashes": scenario
                        .checkpoints
                        .iter()
                        .enumerate()
                        .map(|(index, _)| format!("fnv1a32:{index:08x}"))
                        .collect::<Vec<_>>(),
                    "fault_count": 0,
                    "trace": {
                        "entries": [
                            {"kind":"capability","capability":"entity_read","decision":"denied"},
                            {"kind":"capability","capability":"entity_write_runtime","decision":"denied"},
                            {"kind":"capability","capability":"input_read","decision":"denied"},
                            {"kind":"capability","capability":"hud_action","decision":"denied"},
                            {"kind":"capability","capability":"level_travel","decision":"denied"},
                            {"kind":"capability","capability":"audio_event","decision":"denied"},
                            {"kind":"capability","capability":"deterministic_timer","decision":"denied"}
                        ],
                        "truncated": false,
                        "redactions": 0,
                        "usage": {
                            "instructions": 0,
                            "messages": 2,
                            "spawned_entities": 0,
                            "emitted_events": 0,
                            "log_bytes": 0,
                            "timers": 0,
                            "heap_estimate_bytes": 512,
                            "wall_clock_millis": 1
                        }
                    }
                },
                "assertions": assertions,
                "completed": true
            }]
        })
        .to_string()
    }

    #[test]
    fn playtest_steps_are_bounded_before_the_webview_sees_them() {
        let valid =
            playtest_steps(r#"{"kind":"playtest","steps":[{"keys":["KeyW"],"frames":60}]}"#)
                .expect("valid plan");
        assert!(valid.contains("KeyW"));

        let zero = playtest_steps(r#"{"kind":"playtest","steps":[{"keys":[],"frames":0}]}"#)
            .expect_err("zero frames must be rejected");
        assert!(zero.hint.is_some());

        let too_long = format!(
            r#"{{"kind":"playtest","steps":[{{"keys":[],"frames":{}}}]}}"#,
            bhippi_types::ENGINE_PLAYTEST_MAX_FRAMES_PER_STEP + 1
        );
        assert!(playtest_steps(&too_long).is_err());

        let too_many_steps = serde_json::json!({
            "steps": (0..=bhippi_types::ENGINE_PLAYTEST_MAX_STEPS)
                .map(|_| serde_json::json!({"keys": [], "frames": 1}))
                .collect::<Vec<_>>()
        });
        assert!(playtest_steps(&too_many_steps.to_string()).is_err());

        let long_key = "K".repeat(bhippi_types::ENGINE_PLAYTEST_MAX_KEY_CODE_BYTES + 1);
        let bad_key = serde_json::json!({"steps": [{"keys": [long_key], "frames": 1}]});
        assert!(playtest_steps(&bad_key.to_string()).is_err());
    }

    #[test]
    fn camera_contract_is_closed_and_png_dimensions_come_from_ihdr() {
        assert!(validate_camera("editor").is_ok());
        assert!(validate_camera("game").is_ok());
        assert!(validate_camera("entity:01JTEST").is_ok());
        assert!(validate_camera("desktop").is_err());
        assert!(validate_camera("entity:").is_err());

        let png = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
        )
        .expect("fixture png");
        assert_eq!(png_dimensions(&png), Some((1, 1)));
        assert_eq!(png_dimensions(b"not a png"), None);
    }

    #[test]
    fn screenshot_payload_checks_png_and_claimed_dimensions_before_writing() {
        let root = std::env::temp_dir().join(format!(
            "bhippi-capture-test-{}",
            bhippi_types::TransactionId::new()
        ));
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
        let valid = decode_and_write(&root, "valid", png, 1, 1).expect("valid capture");
        assert_eq!(valid.width, Some(1));
        assert!(valid
            .path
            .as_deref()
            .is_some_and(|path| path.ends_with("valid.png")));
        assert!(decode_and_write(&root, "mismatch", png, 2, 1).is_err());
        assert!(decode_and_write(&root, "bad", "not-base64", 1, 1).is_err());
        let _ignored = std::fs::remove_dir_all(root);
    }

    #[test]
    fn response_is_one_shot() {
        let (id, _rx) =
            insert(std::path::Path::new("."), PendingKind::Playtest).expect("insert request");
        let _first = take(&id).expect("first response owns the request");
        let second = match take(&id) {
            Ok(_) => panic!("duplicate response must be rejected"),
            Err(error) => error,
        };
        assert!(second.message.contains("no longer active"));
    }

    #[tokio::test]
    async fn timeout_removes_request_and_late_response_is_rejected() {
        let (id, rx) =
            insert(std::path::Path::new("."), PendingKind::Screenshot).expect("insert request");
        let error = wait_with_timeout(&id, rx, std::time::Duration::from_millis(1))
            .await
            .expect_err("request must time out");
        assert!(error.message.contains("in time"));
        assert!(!pending().lock().expect("queue").contains_key(&id));
        assert!(take(&id).is_err());
    }

    #[tokio::test]
    async fn game_test_batch_response_is_validated_against_pending_plan_and_hash() {
        let root = std::env::temp_dir().join(format!(
            "bhippi-game-test-batch-{}",
            bhippi_types::TransactionId::new()
        ));
        std::fs::create_dir_all(&root).expect("temporary project");
        let plan =
            GameTestPlan::mandatory_smoke("assets/scenes/main.bscn.json").expect("smoke plan");
        let authored_hash = bhippi_engine::game_debug::authored_tree_hash(&root);
        assert!(validate_authored_tree_hash(&authored_hash).is_ok());
        assert!(validate_authored_tree_hash(&authored_hash.to_ascii_uppercase()).is_err());
        let (id, rx) = insert(
            &root,
            PendingKind::GameTestBatch {
                plan: plan.clone(),
                authored_tree_hash: authored_hash.clone(),
            },
        )
        .expect("insert batch");
        engine_submit_game_test_batch(id, smoke_batch_report(&plan, &authored_hash))
            .await
            .expect("submission accepted");
        let result = rx.await.expect("response channel").expect("valid evidence");
        let parsed =
            bhippi_engine::game_test_plan::GameTestBatchEvidence::parse(&result.report, &plan)
                .expect("canonical response");
        assert_eq!(parsed.authored_tree_before, authored_hash);

        let (id, rx) = insert(
            &root,
            PendingKind::GameTestBatch {
                plan: plan.clone(),
                authored_tree_hash: authored_hash.clone(),
            },
        )
        .expect("insert batch");
        let mut wrong_plan: serde_json::Value =
            serde_json::from_str(&smoke_batch_report(&plan, &authored_hash)).expect("report");
        wrong_plan["scenarios"][0]["seed"] = serde_json::json!(99);
        engine_submit_game_test_batch(id, wrong_plan.to_string())
            .await
            .expect("submission delivered");
        let error = rx
            .await
            .expect("response channel")
            .expect_err("pending plan identity is authoritative");
        assert!(error.message.contains("planned identity"));

        let (id, rx) = insert(
            &root,
            PendingKind::GameTestBatch {
                plan: plan.clone(),
                authored_tree_hash: authored_hash.clone(),
            },
        )
        .expect("insert batch");
        engine_submit_game_test_batch(id, smoke_batch_report(&plan, &"a".repeat(64)))
            .await
            .expect("submission delivered");
        let error = rx
            .await
            .expect("response channel")
            .expect_err("worker cannot substitute a different authored tree");
        assert!(error.message.contains("does not match"));
        let _ignored = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn game_test_batch_rejects_authored_tree_changes_before_accepting_evidence() {
        let root = std::env::temp_dir().join(format!(
            "bhippi-game-test-freshness-{}",
            bhippi_types::TransactionId::new()
        ));
        std::fs::create_dir_all(&root).expect("temporary project");
        let plan =
            GameTestPlan::mandatory_smoke("assets/scenes/main.bscn.json").expect("smoke plan");
        let authored_hash = bhippi_engine::game_debug::authored_tree_hash(&root);
        let (id, rx) = insert(
            &root,
            PendingKind::GameTestBatch {
                plan: plan.clone(),
                authored_tree_hash: authored_hash.clone(),
            },
        )
        .expect("insert batch");
        std::fs::write(root.join(bhippi_engine::GAME_MANIFEST_FILE), "[game]\n")
            .expect("mutate authored tree");
        engine_submit_game_test_batch(id, smoke_batch_report(&plan, &authored_hash))
            .await
            .expect("submission delivered");
        let error = rx
            .await
            .expect("response channel")
            .expect_err("stale evidence must fail closed");
        assert!(error.message.contains("changed while"));
        let _ignored = std::fs::remove_dir_all(root);
    }
}
