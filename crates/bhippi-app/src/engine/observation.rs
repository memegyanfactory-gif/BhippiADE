//! Renderer observation bridge for the autonomous engine loop (ENG-186/187).
//!
//! Rendering and play simulation live in the webview by ADR-0028, while the model loop
//! lives in Rust. This is the narrow request/response seam: Rust emits a typed request, the
//! active Engine pane answers once, and a bounded one-shot returns the result. No frame
//! traffic crosses IPC.

use crate::commands::AppError;
use base64::Engine as _;
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

struct Pending {
    game_dir: PathBuf,
    tx: oneshot::Sender<Result<EngineObservationResult, AppError>>,
}

fn pending() -> &'static Mutex<HashMap<String, Pending>> {
    static PENDING: OnceLock<Mutex<HashMap<String, Pending>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

type ObservationReceiver = oneshot::Receiver<Result<EngineObservationResult, AppError>>;

fn insert(game_dir: &Path) -> Result<(String, ObservationReceiver), AppError> {
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
    let (request_id, rx) = insert(game_dir)?;
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
    let (request_id, rx) = insert(game_dir)?;
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

#[cfg(test)]
mod tests {
    use super::{
        decode_and_write, insert, pending, playtest_steps, png_dimensions, take, validate_camera,
        wait_with_timeout,
    };

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
        let (id, _rx) = insert(std::path::Path::new(".")).expect("insert request");
        let _first = take(&id).expect("first response owns the request");
        let second = match take(&id) {
            Ok(_) => panic!("duplicate response must be rejected"),
            Err(error) => error,
        };
        assert!(second.message.contains("no longer active"));
    }

    #[tokio::test]
    async fn timeout_removes_request_and_late_response_is_rejected() {
        let (id, rx) = insert(std::path::Path::new(".")).expect("insert request");
        let error = wait_with_timeout(&id, rx, std::time::Duration::from_millis(1))
            .await
            .expect_err("request must time out");
        assert!(error.message.contains("in time"));
        assert!(!pending().lock().expect("queue").contains_key(&id));
        assert!(take(&id).is_err());
    }
}
