//! The lifetime of a Computer Use turn, and the emergency stop that ends one (ADR-0054).
//!
//! This used to be `overlay.rs`: a transparent, always-on-top, click-through webview spanning
//! the whole virtual desktop, on which the agent's actions were drawn (ADR-0019, ADR-0044
//! §2). ADR-0054 retired that window — the run is watched in the app now — and what is left
//! here is the part that was never about painting.
//!
//! Two things live in this module:
//!
//! 1. **[`ComputerTurnGuard`]**, held by the engine for exactly the length of a turn. It
//!    arms a generation on construction and disarms on drop, so every return path — done,
//!    failed, faulted, stopped — closes the turn without an explicit call per `return`.
//! 2. **Esc/Esc**, the emergency stop. Two Escape presses inside
//!    [`DOUBLE_ESCAPE_WINDOW`] send the guard's generation on a watch channel, and the loop
//!    in `chat.rs` stops between actions.
//!
//! The generation is what makes the stop safe under overlapping turns: a press belonging to
//! an older turn carries an older generation, and a receiver only acts on its own. That
//! ordering — arm the generation and the watcher *before* anything else — is why removing
//! the window changed nothing about the stop.

use std::sync::OnceLock;
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::{watch, Mutex};

/// Throttle between cursor/keyboard polls. ~80 Hz: fast enough that a deliberate double
/// Escape is never missed, slow enough to cost nothing.
const WATCH_DELTA: Duration = Duration::from_millis(12);

/// Two distinct Escape presses inside this window are the global emergency stop.
const DOUBLE_ESCAPE_WINDOW: Duration = Duration::from_millis(900);

struct GuardState {
    /// A Computer Use turn is running.
    active: bool,
    /// Bumped on every activation, so a stale disarm can never end a newer turn.
    generation: u64,
    /// The running Esc/Esc watcher, aborted on deactivation.
    watch: Option<tauri::async_runtime::JoinHandle<()>>,
}

static STATE: OnceLock<Mutex<GuardState>> = OnceLock::new();
static STOP_SIGNAL: OnceLock<watch::Sender<u64>> = OnceLock::new();

fn guard_state() -> &'static Mutex<GuardState> {
    STATE.get_or_init(|| {
        Mutex::new(GuardState {
            active: false,
            generation: 0,
            watch: None,
        })
    })
}

fn stop_signal() -> &'static watch::Sender<u64> {
    STOP_SIGNAL.get_or_init(|| watch::channel(0).0)
}

/// RAII over one Computer Use turn: arms the emergency stop on construction, disarms it on
/// drop.
pub struct ComputerTurnGuard {
    handle: Option<AppHandle>,
    generation: u64,
}

impl ComputerTurnGuard {
    /// Arms the turn and its emergency stop.
    ///
    /// The order is the whole safety argument and has not changed since ADR-0054: the
    /// generation and the Esc/Esc watcher first, chrome second. ADR-0057 hangs the edge glow
    /// off the end of that sequence, so a glow that will not open cannot delay — let alone
    /// prevent — the stop being armed.
    pub async fn begin(handle: &AppHandle) -> Self {
        let generation = set_active(true, None).await;
        let guard = Self {
            handle: Some(handle.clone()),
            generation,
        };
        if let Some(frame) = crate::computer_glow::desktop_frame().await {
            crate::computer_glow::show(handle, frame);
        }
        guard
    }

    /// A guard that does nothing — used by engines with no desktop to stop.
    #[must_use]
    pub fn inert() -> Self {
        Self {
            handle: None,
            generation: 0,
        }
    }

    /// Generation-scoped emergency-stop signal. A press from an older turn can never stop a
    /// newer one, because the received value must equal this guard's generation.
    #[must_use]
    pub fn stop_receiver(&self) -> (u64, watch::Receiver<u64>) {
        (self.generation, stop_signal().subscribe())
    }
}

impl Drop for ComputerTurnGuard {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        // The glow comes down here rather than at any of the turn's exit points, because
        // there are eight of those and `Drop` is the one thing all of them run. A border
        // left burning over somebody's desktop after the agent stopped is the worst failure
        // this feature has, so it is tied to the value whose destruction *is* the turn
        // ending — done, failed, faulted, stopped or panicked alike.
        crate::computer_glow::hide(&handle);
        let generation = self.generation;
        tauri::async_runtime::spawn(async move {
            set_active(false, Some(generation)).await;
        });
    }
}

/// Arm or disarm the turn. Returns the live generation; `0` means nothing was armed.
async fn set_active(active: bool, only_if_generation: Option<u64>) -> u64 {
    let mut state = guard_state().lock().await;
    if guard_is_stale(state.active, state.generation, active, only_if_generation) {
        return state.generation;
    }
    if active == state.active {
        return state.generation;
    }

    if active {
        state.generation = state.generation.saturating_add(1);
        let generation = state.generation;
        if let Some(watch) = state.watch.take() {
            watch.abort();
        }
        state.watch = Some(spawn_escape_watcher(generation));
        state.active = true;
        generation
    } else {
        if let Some(watch) = state.watch.take() {
            watch.abort();
        }
        state.active = false;
        state.generation
    }
}

/// Esc/Esc, read in-process at ~80 Hz: one `GetAsyncKeyState` per tick, no child process and
/// nothing to orphan. The task is aborted when the turn ends, so a stale watcher cannot
/// outlive the turn that armed it.
fn spawn_escape_watcher(generation: u64) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        let mut last_escape: Option<std::time::Instant> = None;
        let mut escape_was_down = false;
        loop {
            let escape_down = win::escape_is_down();
            if escape_down && !escape_was_down {
                let now = std::time::Instant::now();
                let elapsed = last_escape.map(|at| now.saturating_duration_since(at));
                if is_double_escape(elapsed) {
                    stop_signal().send_replace(generation);
                    last_escape = None;
                } else {
                    last_escape = Some(now);
                }
            }
            escape_was_down = escape_down;
            tokio::time::sleep(WATCH_DELTA).await;
        }
    })
}

// One of two `unsafe` modules in the product, beside ADR-0045's: a single state read, no
// pointers retained, a SAFETY note on it. Nothing here allocates or outlives its call.
#[cfg(windows)]
#[allow(unsafe_code)]
mod win {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    const VK_ESCAPE: i32 = 0x1B;

    /// Whether Escape is down right now.
    pub fn escape_is_down() -> bool {
        // SAFETY: a state query taking a key code and no pointers.
        (unsafe { GetAsyncKeyState(VK_ESCAPE) } as u16) & 0x8000 != 0
    }
}

#[cfg(not(windows))]
mod win {
    pub fn escape_is_down() -> bool {
        false
    }
}

fn is_double_escape(elapsed: Option<Duration>) -> bool {
    elapsed.is_some_and(|elapsed| elapsed <= DOUBLE_ESCAPE_WINDOW)
}

/// A guard armed for an old turn must not act on a state a newer turn has taken over.
fn guard_is_stale(
    state_active: bool,
    state_generation: u64,
    target_active: bool,
    only_if_generation: Option<u64>,
) -> bool {
    state_active != target_active
        && only_if_generation.is_some_and(|generation| generation != state_generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inert_guard_drops_without_touching_tauri() {
        let guard = ComputerTurnGuard::inert();
        drop(guard);
    }

    #[test]
    fn stale_guard_cannot_act_after_a_newer_turn() {
        // An old turn's disarm must not run once a newer turn has taken over.
        assert!(guard_is_stale(true, 3, false, Some(2)));
        // The same generation is allowed to disarm; an already-disarmed turn is a no-op.
        assert!(!guard_is_stale(true, 3, false, Some(3)));
        assert!(!guard_is_stale(false, 3, false, Some(2)));
        // A hint-less call (the idempotency path) is never "stale".
        assert!(!guard_is_stale(true, 3, false, None));
    }

    #[test]
    fn only_two_escape_presses_inside_the_window_stop_control() {
        assert!(!is_double_escape(None));
        assert!(is_double_escape(Some(Duration::from_millis(899))));
        assert!(is_double_escape(Some(Duration::from_millis(900))));
        assert!(!is_double_escape(Some(Duration::from_millis(901))));
    }

    /// ADR-0057's whole safety argument, read out of the source because it is an argument
    /// about *order* and there is no Tauri runtime in a unit test to observe it in.
    ///
    /// The stop must be armed before any chrome, and the chrome must come down on `Drop`
    /// rather than at a turn's exit points — there are eight of those and `Drop` is the one
    /// thing all of them run.
    #[test]
    fn the_stop_is_armed_before_the_glow_and_the_glow_comes_down_on_drop() {
        let source = include_str!("computer_guard.rs");
        let begin = source
            .split("pub async fn begin")
            .nth(1)
            .and_then(|rest| rest.split("\n    }").next())
            .expect("begin() is in this file");
        let armed_at = begin.find("set_active(true").expect("begin arms the turn");
        let glow_at = begin
            .find("computer_glow::show")
            .expect("begin shows the glow");
        assert!(
            armed_at < glow_at,
            "the emergency stop must be armed before any chrome is opened"
        );

        let drop_body = source
            .split("impl Drop for ComputerTurnGuard")
            .nth(1)
            .expect("the guard has a Drop impl");
        assert!(
            drop_body.contains("computer_glow::hide"),
            "every exit path takes the glow down, and Drop is the only thing all of them run"
        );
    }

    #[tokio::test]
    async fn arming_a_turn_takes_a_generation_and_disarming_gives_it_back() {
        // The property ADR-0054 leans on: the stop is armed by the generation alone, with no
        // window involved, so retiring the overlay could not disarm it.
        let first = set_active(true, None).await;
        assert_ne!(first, 0, "an armed turn must have a generation");
        assert_eq!(
            set_active(true, None).await,
            first,
            "arming twice is idempotent"
        );

        set_active(false, Some(first)).await;
        let second = set_active(true, None).await;
        assert!(second > first, "each turn takes a fresh generation");
        set_active(false, Some(second)).await;
    }
}
