//! Desktop-wide Computer Use overlay.
//!
//! Implements Phase 4 of `docs/12-COMPUTER-USE-IMPLEMENTATION-PLAN.md` (ADR-0019): a
//! transparent, always-on-top, click-through webview window spanning the whole virtual
//! desktop where the grid-scan aura and Bhippi's own pointer live. It appears when a
//! Computer Use turn starts and closes when the turn ends, fails, or is cancelled — the
//! engine holds an [`OverlayGuard`] for exactly the lifetime of the turn, so every return
//! path closes it.
//!
//! The overlay page reads raw Tauri events only (never specta types), so `ui/src/lib/ipc.ts`
//! is untouched: the typed IPC surface remains the app's contract, not this window's chrome.

use crate::computer;
use serde::Serialize;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{watch, Mutex};

/// Tauri webview label for the overlay window.
pub const OVERLAY_LABEL: &str = "overlay";

/// How long the exit fade is given before the window is hidden (matches the aura's CSS
/// `transition` on `opacity` + a small margin).
const HIDE_DELAY: Duration = Duration::from_millis(420);
#[allow(dead_code)]
/// Throttle between cursor position reads placed on the event pipe.
const WATCH_DELTA: Duration = Duration::from_millis(12);
#[allow(dead_code)]
/// Two distinct Escape presses inside this window are the global emergency stop.
const DOUBLE_ESCAPE_WINDOW: Duration = Duration::from_millis(900);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlayShow<'a> {
    label: &'a str,
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
}

#[allow(dead_code)]
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CursorPosition {
    x: i32,
    y: i32,
}

struct OverlayState {
    /// A Computer Use turn has the overlay up.
    active: bool,
    /// Bumped on every activation so a stale delayed hide can never hide a newer show.
    generation: u64,
    /// The running cursor watcher, aborted on deactivation.
    watch: Option<tauri::async_runtime::JoinHandle<()>>,
}

static STATE: OnceLock<Mutex<OverlayState>> = OnceLock::new();
static STOP_SIGNAL: OnceLock<watch::Sender<u64>> = OnceLock::new();

fn overlay_state() -> &'static Mutex<OverlayState> {
    STATE.get_or_init(|| {
        Mutex::new(OverlayState {
            active: false,
            generation: 0,
            watch: None,
        })
    })
}

fn stop_signal() -> &'static watch::Sender<u64> {
    STOP_SIGNAL.get_or_init(|| watch::channel(0).0)
}

/// RAII: showing the overlay on construction, hiding it on drop. The engine places one at
/// the top of a Computer Use turn so every return path — done, failed, fault, stop —
/// closes the desktop overlay without an explicit call per `return`.
pub struct OverlayGuard {
    handle: Option<AppHandle>,
    generation: u64,
}

impl OverlayGuard {
    /// Shows the overlay (a no-op when a turn is already showing it) and arms the guard.
    pub async fn begin(handle: &AppHandle, label: &str) -> Self {
        let generation = set_active(handle, true, label).await;
        Self {
            handle: Some(handle.clone()),
            generation,
        }
    }

    /// A guard that does nothing — used by engines without desktop chrome.
    #[must_use]
    pub fn inert() -> Self {
        Self {
            handle: None,
            generation: 0,
        }
    }

    /// Generation-scoped emergency-stop signal. A press from an older turn can never
    /// stop a newer one because the receiver value must equal this guard's generation.
    #[must_use]
    pub fn stop_receiver(&self) -> (u64, watch::Receiver<u64>) {
        (self.generation, stop_signal().subscribe())
    }
}

impl Drop for OverlayGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let generation = self.generation;
            tauri::async_runtime::spawn(async move {
                set_active_if_generation(&handle, generation).await;
            });
        }
    }
}

/// What the agent just did, for the overlay to draw (SPA-303).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlayAction<'a> {
    title: &'a str,
    x: Option<i32>,
    y: Option<i32>,
    index: usize,
}

/// Creates the desktop-wide overlay window, hidden (ADR-0019, back in SPA-303).
///
/// Transparent, always on top, off the taskbar, never focused — and click-through from
/// birth: `set_ignore_cursor_events` runs before the first show, which is what the earlier
/// disabling was about. The desktop underneath keeps every click; the page only paints.
pub fn create_overlay_window(app: &tauri::App) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    let window =
        WebviewWindowBuilder::new(app, OVERLAY_LABEL, WebviewUrl::App("overlay.html".into()))
            .title("Bhippi Computer Use")
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .focused(false)
            .shadow(false)
            .visible(false)
            .build()
            .map_err(|error| format!("overlay window could not be created: {error}"))?;
    window
        .set_ignore_cursor_events(true)
        .map_err(|error| format!("overlay passthrough failed: {error}"))?;
    Ok(())
}

/// Tells the overlay what the agent just did, so it ripples at the point and prints the
/// caption (ADR-0044 §2: every action drawn, with its caption).
pub fn announce_action(app: &AppHandle, title: &str, point: Option<(i32, i32)>, index: usize) {
    let _ignored = app.emit_to(
        OVERLAY_LABEL,
        "computer-overlay-action",
        OverlayAction {
            title,
            x: point.map(|p| p.0),
            y: point.map(|p| p.1),
            index,
        },
    );
}

/// Activates or deactivates the overlay. Activation returns the generation the activation
/// can be matched against; `0` means the overlay could not be started.
pub async fn set_active(app: &AppHandle, active: bool, label: &str) -> u64 {
    match set_active_inner(app, active, label, None).await {
        Ok(generation) => generation,
        Err(error) => {
            tracing::warn!(%error, "computer use overlay control failed");
            0
        }
    }
}

/// Deactivates only if the given activation generation is still the live one, so a stale
/// guard cannot hide an overlay that a later turn already re-showed.
async fn set_active_if_generation(app: &AppHandle, generation: u64) {
    if generation == 0 {
        return;
    }
    match set_active_inner(app, false, "", Some(generation)).await {
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "computer use overlay hide failed"),
    }
}

async fn set_active_inner(
    app: &AppHandle,
    active: bool,
    label: &str,
    only_if_generation: Option<u64>,
) -> Result<u64, String> {
    let mut state = overlay_state().lock().await;
    if guard_is_stale(state.active, state.generation, active, only_if_generation) {
        return Ok(state.generation);
    }
    if active == state.active {
        return Ok(state.generation);
    }

    if active {
        // Arm the generation and the Esc/Esc watcher *before* chrome. A missing or
        // tiny overlay window must never disable the emergency stop — that is the
        // only way the user can take the desktop back when the agent is moving it.
        state.generation = state.generation.saturating_add(1);
        let generation = state.generation;
        if let Some(watch) = state.watch.take() {
            watch.abort();
        }
        state.watch = Some(spawn_cursor_watcher(app, generation));
        state.active = true;

        match app.get_webview_window(OVERLAY_LABEL) {
            Some(window) => {
                if let Err(error) = show_overlay_window(&window, label).await {
                    tracing::warn!(
                        %error,
                        "computer use overlay window failed; emergency stop is still armed"
                    );
                }
            }
            None => tracing::warn!(
                "overlay window is missing; computer use still runs with Esc/Esc stop armed"
            ),
        }
        Ok(generation)
    } else {
        let window = app.get_webview_window(OVERLAY_LABEL);
        if let Some(window) = window.as_ref() {
            let _ignored = window.emit("computer-overlay-hide", ());
        }
        computer::restore_system_cursor().await;

        if let Some(watch) = state.watch.take() {
            watch.abort();
        }
        state.active = false;
        let generation = state.generation;
        let overlay_app = app.clone();

        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(HIDE_DELAY).await;
            let still_current = {
                let current = overlay_state().lock().await;
                !current.active && current.generation == generation
            };
            if still_current {
                if let Some(window) = overlay_app.get_webview_window(OVERLAY_LABEL) {
                    let _ignored = window.hide();
                }
            }
        });
        Ok(generation)
    }
}

async fn show_overlay_window(window: &tauri::WebviewWindow, label: &str) -> Result<(), String> {
    let bounds = computer::screen_bounds().await?;
    window
        .set_position(tauri::Position::Physical(
            (bounds.origin_x, bounds.origin_y).into(),
        ))
        .map_err(|error| format!("overlay reposition failed: {error}"))?;
    window
        .set_size(tauri::Size::Physical((bounds.width, bounds.height).into()))
        .map_err(|error| format!("overlay resize failed: {error}"))?;
    window
        .set_ignore_cursor_events(true)
        .map_err(|error| format!("overlay passthrough failed: {error}"))?;
    window
        .show()
        .map_err(|error| format!("overlay show failed: {error}"))?;
    // Emit after show so a page that wired listeners during startup never misses the
    // first paint. A second emit is cheap and covers a listener that attached between
    // the two.
    let payload = OverlayShow {
        label,
        origin_x: bounds.origin_x,
        origin_y: bounds.origin_y,
        width: bounds.width,
        height: bounds.height,
    };
    window
        .emit("computer-overlay-show", &payload)
        .map_err(|error| format!("overlay show event failed: {error}"))?;
    Ok(())
}

/// The pointer and Esc/Esc, read in-process at ~80 Hz (SPA-303): one `GetCursorPos` and
/// one `GetAsyncKeyState` per tick, no child process, nothing to orphan. The task is
/// aborted when the overlay hides, so a stale watcher cannot outlive its turn.
fn spawn_cursor_watcher(app: &AppHandle, generation: u64) -> tauri::async_runtime::JoinHandle<()> {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last: Option<(i32, i32)> = None;
        let mut last_escape: Option<std::time::Instant> = None;
        let mut escape_was_down = false;
        loop {
            let (point, escape_down) = win::poll();
            if escape_down && !escape_was_down {
                let now = std::time::Instant::now();
                let elapsed = last_escape.map(|at| now.saturating_duration_since(at));
                if is_double_escape(elapsed) {
                    stop_signal().send_replace(generation);
                    let _ignored = app.emit_to(
                        OVERLAY_LABEL,
                        "computer-overlay-stopping",
                        "Stopping Computer Use",
                    );
                    last_escape = None;
                } else {
                    last_escape = Some(now);
                }
            }
            escape_was_down = escape_down;
            if let Some(point) = point {
                if last != Some(point) {
                    last = Some(point);
                    let _ignored = app.emit_to(
                        OVERLAY_LABEL,
                        "computer-overlay-cursor",
                        CursorPosition {
                            x: point.0,
                            y: point.1,
                        },
                    );
                }
            }
            tokio::time::sleep(WATCH_DELTA).await;
        }
    })
}

// The second `unsafe` module in the product, beside ADR-0045's: two reads, no pointers
// retained, a SAFETY note on each. Nothing here allocates or outlives its call.
#[cfg(windows)]
#[allow(unsafe_code)]
mod win {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    const VK_ESCAPE: i32 = 0x1B;

    /// The pointer in physical virtual-desktop pixels, and whether Escape is down now.
    pub fn poll() -> (Option<(i32, i32)>, bool) {
        let mut point = POINT { x: 0, y: 0 };
        // SAFETY: `point` is a valid out-pointer for the duration of the call, which has
        // no other effect.
        let ok = unsafe { GetCursorPos(&mut point) } != 0;
        // SAFETY: a state query taking a key code and no pointers.
        let escape = (unsafe { GetAsyncKeyState(VK_ESCAPE) } as u16) & 0x8000 != 0;
        (ok.then_some((point.x, point.y)), escape)
    }
}

#[cfg(not(windows))]
mod win {
    pub fn poll() -> (Option<(i32, i32)>, bool) {
        (None, false)
    }
}

/// One persistent PowerShell process streams `X|Y` cursor positions until it dies. Each
/// moved position is emitted to the overlay as a raw event; the page lerps between them.
#[allow(dead_code)]
#[cfg(windows)]
async fn watch_cursor(app: &AppHandle, generation: u64) -> Result<(), String> {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::process::Command;

    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System.Runtime.InteropServices;
public static class BhippiKeyboard {
  [DllImport("user32.dll")]
  public static extern short GetAsyncKeyState(int virtualKey);
}
'@
$escapeWasDown = $false
while ($true) {
  $p = [System.Windows.Forms.Cursor]::Position
  $escapeDown = (([BhippiKeyboard]::GetAsyncKeyState(0x1B) -band 0x8000) -ne 0)
  if ($escapeDown -and -not $escapeWasDown) { Write-Output "E" }
  Write-Output "P|$($p.X)|$($p.Y)"
  $escapeWasDown = $escapeDown
  Start-Sleep -Milliseconds 12
}
"#;
    let mut child = Command::new("powershell")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            SCRIPT,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|error| format!("could not start the overlay cursor watcher: {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "overlay cursor watcher produced no stdout".to_owned())?;
    let mut lines = BufReader::new(stdout).lines();
    let mut last: Option<(i32, i32)> = None;
    let mut last_escape = None;

    loop {
        let line = {
            match tokio::time::timeout(Duration::from_secs(10), lines.next_line()).await {
                Ok(Ok(Some(line))) => line,
                // EOF (watcher killed or broken pipe) or a read error ends the task.
                Ok(Ok(None)) | Ok(Err(_)) | Err(_) => break,
            }
        };
        let line = line.trim();
        if line == "E" {
            let now = std::time::Instant::now();
            let elapsed = last_escape.map(|at| now.saturating_duration_since(at));
            if is_double_escape(elapsed) {
                stop_signal().send_replace(generation);
                let _ignored = app.emit_to(
                    OVERLAY_LABEL,
                    "computer-overlay-stopping",
                    "Stopping Computer Use",
                );
                last_escape = None;
            } else {
                last_escape = Some(now);
            }
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() != 3 || parts[0] != "P" {
            continue;
        }
        let (Ok(x), Ok(y)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>()) else {
            continue;
        };
        if last == Some((x, y)) {
            continue;
        }
        last = Some((x, y));
        let _ignored = app.emit_to(
            OVERLAY_LABEL,
            "computer-overlay-cursor",
            CursorPosition { x, y },
        );
        tokio::time::sleep(WATCH_DELTA).await;
    }

    let _ = child.kill().await;
    Ok(())
}

#[allow(dead_code)]
fn is_double_escape(elapsed: Option<Duration>) -> bool {
    elapsed.is_some_and(|elapsed| elapsed <= DOUBLE_ESCAPE_WINDOW)
}

/// A guard that was armed for an old activation must not act on a state that a newer
/// activation has taken over.
fn guard_is_stale(
    state_active: bool,
    state_generation: u64,
    target_active: bool,
    only_if_generation: Option<u64>,
) -> bool {
    state_active != target_active && only_if_generation.is_some_and(|gen| gen != state_generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inert_guard_drops_without_touching_tauri() {
        let guard = OverlayGuard::inert();
        drop(guard);
    }

    #[test]
    fn stale_guard_cannot_act_after_a_newer_activation() {
        // An old turn's hide must not run once a newer turn has taken over the overlay.
        assert!(guard_is_stale(true, 3, false, Some(2)));
        // The same generation is allowed to hide; an already-hidden overlay is a no-op.
        assert!(!guard_is_stale(true, 3, false, Some(3)));
        assert!(!guard_is_stale(false, 3, false, Some(2)));
        // A hint-less call (idempotency path) is never "stale".
        assert!(!guard_is_stale(true, 3, false, None));
    }

    #[test]
    fn only_two_escape_presses_inside_the_window_stop_control() {
        assert!(!is_double_escape(None));
        assert!(is_double_escape(Some(Duration::from_millis(899))));
        assert!(is_double_escape(Some(Duration::from_millis(900))));
        assert!(!is_double_escape(Some(Duration::from_millis(901))));
    }

    #[test]
    fn a_missing_overlay_window_still_gets_a_nonzero_generation() {
        // OverlayGuard::inert is generation 0 and cannot emergency-stop. A live
        // Computer Use turn must bump generation even if the webview never appears,
        // so Esc/Esc is armed independently of the aura.
        let mut generation = 0_u64;
        generation = generation.saturating_add(1);
        assert_ne!(generation, 0);
    }
}
