//! The Computer Use playtest loop: Bhippi launching the game, watching it and playing it
//! (ADR-0044, `docs/16` GAD-095…099, INV-089).
//!
//! `godot_playtest` next door answers *"did the score variable reach ten?"* — a headless,
//! deterministic replay whose whole evidence is a telemetry file. It cannot answer *"is the
//! player stuck inside the wall?"* or *"does the HUD cover the health bar?"*, because those
//! are visible in a picture and nowhere else. This module is the other half: a real Godot
//! window opens, Bhippi focuses it, presses keys into it, and photographs it, while the probe
//! keeps writing telemetry underneath.
//!
//! Three rules shape it.
//!
//! **One window, never the desktop.** Every capture and every keystroke goes through an
//! [`EngineCaptureScope`], whose only constructor takes a [`WindowRef`] and which has no arm
//! that widens to the desktop-wide path in [`crate::computer`] (INV-089's last row). A window
//! that vanishes ends the run with a typed reason; it never falls back to a screen grab.
//!
//! **Evidence is always a pair.** A frame with no telemetry sample behind it, or a telemetry
//! assertion with no frame, is *partial* evidence and says so in [`VisualEvidence::faults`]
//! (ADR-0044 §3). [`VisualPlaytestResult::evidence`] is what pairs them, and it is a pure
//! function over the captures and the report so it can be tested without a game.
//!
//! **The game is asked to close, not killed.** The probe writes its final `{"done": true}`
//! line from `NOTIFICATION_WM_CLOSE_REQUEST`; a terminated process never gets there, and a
//! telemetry file with no `done` line cannot say whether the samples stopped or the game did.
//! So the loop posts `WM_CLOSE` first and keeps the process handle only as the backstop.

use crate::commands::AppError;
use crate::computer_window::{
    find_windows, CaptureOptions, EngineCaptureScope, KeyName, WindowCaptureMethod, WindowError,
    WindowFilter, WindowInput, WindowRef, WINDOW_CAPTURE_MAX_BYTES, WINDOW_HOLD_MAX_MS,
};
use crate::godot::{
    run_spec_with_stop, GodotExit, GodotOutputLine, GodotProcessHandle, GodotStopSignal,
};
use bhippi_engine::godot::command::{run_command, RunOptions, TELEMETRY_ARG, TELEMETRY_ENV};
use bhippi_engine::godot::probe::{TelemetryEvent, TelemetryLine, TelemetryReport, TrackedNode};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::watch;

// ── limits ───────────────────────────────────────────────────────────────────────────

/// The most input steps one visual playtest may carry. A turn that needs more than this is
/// several turns, and a cap the model cannot argue with is the point.
pub const VISUAL_PLAYTEST_MAX_STEPS: usize = 64;
/// The wall-clock ceiling on one visual playtest, whatever the plan asks for.
pub const VISUAL_PLAYTEST_MAX_MS: u64 = 120_000;
/// PNG ceiling per frame, before base64. The same budget a one-off window capture gets.
pub const VISUAL_CAPTURE_MAX_BYTES: usize = WINDOW_CAPTURE_MAX_BYTES;
/// The most frames one visual playtest returns. Every frame is a full PNG headed for a vision
/// model's context, so this is a cost ceiling as much as a memory one.
pub const VISUAL_MAX_CAPTURES: usize = 24;
/// How long to wait for the game window to appear before giving up.
pub const WINDOW_WAIT_MS: u64 = 15_000;
/// The gap between two window polls. Each poll is a PowerShell round trip, so this is a floor
/// on the cadence rather than the cadence itself.
pub const WINDOW_POLL_MS: u64 = 200;
/// The longest a step may dwell after its input before the frame is taken.
pub const VISUAL_STEP_MAX_HOLD_MS: u64 = WINDOW_HOLD_MAX_MS;
/// How long a note on a step may be. It rides into a model's context with the frame.
pub const VISUAL_NOTE_MAX_CHARS: usize = 200;
/// How long the game is given to shut itself down after `WM_CLOSE` before it is killed.
pub const VISUAL_SHUTDOWN_GRACE_MS: u64 = 8_000;
/// How far a frame and a telemetry sample may be apart and still be called the same moment.
///
/// The probe samples every six frames — 100 ms at the fixed 60 fps — so a correctly aligned
/// frame lands within 50 ms of a sample. A quarter of a second is that, doubled, and no more:
/// past it the two are describing different moments and the frame is partial evidence.
pub const EVIDENCE_PAIR_TOLERANCE_MS: i64 = 250;
/// Two Escape presses inside this window are the emergency stop, exactly as `computer.rs`
/// spells it.
const ESCAPE_DOUBLE_WINDOW: Duration = Duration::from_millis(900);
/// Trailing engine output lines a result carries back.
const LOG_TAIL: usize = 40;
/// The most faults [`VisualPlaytestResult::evidence`] lists. A wall of them is not evidence.
const MAX_EVIDENCE_FAULTS: usize = 8;

// ── the plan ─────────────────────────────────────────────────────────────────────────

/// One thing to do to the running game, and what to record afterwards.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct VisualStep {
    /// What to send. `None` waits and looks — which is how you photograph an idle animation
    /// or a cutscene without touching it.
    pub input: Option<WindowInput>,
    /// How long to dwell after the input before the frame is taken. A `Hold` already lasts
    /// its own `frames_ms`; this is the settle time on top, for a jump arc to finish.
    pub hold_ms: Option<u64>,
    /// What this step is for, in the model's own words. It rides into the evidence.
    pub note: Option<String>,
}

impl VisualStep {
    /// A held movement key — how a character walks.
    #[must_use]
    pub fn hold(key: &str, frames_ms: u64, note: &str) -> Self {
        Self {
            input: Some(WindowInput::Hold {
                keys: vec![KeyName::new(key)],
                frames_ms,
            }),
            hold_ms: None,
            note: Some(note.to_owned()),
        }
    }

    /// A tap, plus the dwell that lets whatever it started finish.
    #[must_use]
    pub fn tap(key: &str, settle_ms: u64, note: &str) -> Self {
        Self {
            input: Some(WindowInput::KeyTap {
                key: KeyName::new(key),
            }),
            hold_ms: Some(settle_ms),
            note: Some(note.to_owned()),
        }
    }
}

/// A whole visual playtest, as it crosses IPC.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct VisualPlaytestPlan {
    pub steps: Vec<VisualStep>,
    /// Photograph after every step. The opening frame is taken either way.
    pub capture_every_step: bool,
    /// The plan's own wall-clock budget, clamped to [`VISUAL_PLAYTEST_MAX_MS`].
    pub max_ms: u64,
    /// Pass the probe a telemetry file, so every frame has a sample behind it.
    pub telemetry: bool,
}

impl VisualPlaytestPlan {
    /// The plan the pane's **Watch play** button runs: walk forward two seconds, jump, strafe
    /// left one second — four frames.
    ///
    /// It lives in Rust rather than the webview for the reason `default_playtest_inputs` does:
    /// it is a *test*, and evidence a UI could rewrite is not evidence.
    #[must_use]
    pub fn watch_play() -> Self {
        Self {
            steps: vec![
                VisualStep::hold("KEY_W", 2_000, "walk forward"),
                VisualStep::tap("KEY_SPACE", 700, "jump"),
                VisualStep::hold("KEY_A", 1_000, "strafe left"),
            ],
            capture_every_step: true,
            max_ms: 30_000,
            telemetry: true,
        }
    }

    /// How many frames this plan will produce if nothing goes wrong.
    #[must_use]
    pub fn planned_captures(&self) -> usize {
        1 + if self.capture_every_step {
            self.steps.len()
        } else {
            0
        }
    }

    /// Everything checkable before Godot is spawned.
    ///
    /// All of it runs first, on purpose: a plan that will be refused should cost nothing, and
    /// a game window that opens and is killed a second later is the worst way to learn that a
    /// key name was misspelled.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.steps.is_empty() {
            return Err(AppError {
                message: "A visual playtest needs at least one step.".to_owned(),
                hint: Some(
                    "A step with no input still watches: use it to photograph the game idle."
                        .to_owned(),
                ),
            });
        }
        if self.steps.len() > VISUAL_PLAYTEST_MAX_STEPS {
            return Err(AppError {
                message: format!(
                    "{} steps is past the {VISUAL_PLAYTEST_MAX_STEPS}-step cap.",
                    self.steps.len()
                ),
                hint: Some("Split the run into several shorter visual playtests.".to_owned()),
            });
        }
        if self.max_ms == 0 || self.max_ms > VISUAL_PLAYTEST_MAX_MS {
            return Err(AppError {
                message: format!(
                    "A visual playtest must run for 1 to {VISUAL_PLAYTEST_MAX_MS} ms, not {}.",
                    self.max_ms
                ),
                hint: Some(
                    "Two minutes is the ceiling; a useful one is under thirty seconds.".to_owned(),
                ),
            });
        }
        let planned = self.planned_captures();
        if planned > VISUAL_MAX_CAPTURES {
            return Err(AppError {
                message: format!(
                    "This plan would take {planned} frames, past the {VISUAL_MAX_CAPTURES}-frame cap."
                ),
                hint: Some(
                    "Turn capture_every_step off, or use fewer steps: every frame is a PNG a \
                     vision model has to read."
                        .to_owned(),
                ),
            });
        }
        let mut dwell = 0_u64;
        for (index, step) in self.steps.iter().enumerate() {
            if let Some(input) = &step.input {
                input.validate_shape().map_err(|error| {
                    let error: AppError = error.into();
                    AppError {
                        message: format!("Step {}: {}", index + 1, error.message),
                        hint: error.hint,
                    }
                })?;
                if let WindowInput::Hold { frames_ms, .. } = input {
                    dwell = dwell.saturating_add(*frames_ms);
                }
            }
            if let Some(hold_ms) = step.hold_ms {
                if hold_ms == 0 || hold_ms > VISUAL_STEP_MAX_HOLD_MS {
                    return Err(AppError {
                        message: format!(
                            "Step {}: a dwell must last 1 to {VISUAL_STEP_MAX_HOLD_MS} ms, not {hold_ms} ms.",
                            index + 1
                        ),
                        hint: Some("Leave hold_ms unset to photograph the step immediately."
                            .to_owned()),
                    });
                }
                dwell = dwell.saturating_add(hold_ms);
            }
            if step
                .note
                .as_ref()
                .is_some_and(|note| note.chars().count() > VISUAL_NOTE_MAX_CHARS)
            {
                return Err(AppError {
                    message: format!(
                        "Step {}: the note is longer than {VISUAL_NOTE_MAX_CHARS} characters.",
                        index + 1
                    ),
                    hint: Some("A note names what the step is for, in a phrase.".to_owned()),
                });
            }
        }
        if dwell >= self.max_ms {
            return Err(AppError {
                message: format!(
                    "These steps hold input for {dwell} ms, which does not fit the {} ms budget.",
                    self.max_ms
                ),
                hint: Some(
                    "Raise max_ms, or shorten the holds: the budget must cover the dwells plus \
                     the frames."
                        .to_owned(),
                ),
            });
        }
        Ok(())
    }
}

// ── the result ───────────────────────────────────────────────────────────────────────

/// One frame, as it crosses IPC.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct VisualCapture {
    /// `None` is the opening frame, before any input; `Some(i)` is after step `i` (0-based).
    pub step_index: Option<u32>,
    pub png_base64: String,
    pub width: u32,
    pub height: u32,
    /// Which path produced the pixels. Recorded rather than hidden: a Godot window rendered
    /// by the GPU can answer `PrintWindow` with a blank frame, and a screen copy needs the
    /// window in front — so knowing which one ran is knowing what the frame can be trusted for.
    pub method: WindowCaptureMethod,
    /// Milliseconds from the moment Bhippi spawned Godot.
    pub at_ms: u64,
    /// Godot's own `Time.get_ticks_msec()` when this frame was taken, read from the last
    /// complete telemetry line on disk. `None` when telemetry is off or nothing was written
    /// yet; see [`VisualPlaytestResult::evidence`] for what the pairing does without it.
    pub godot_time_ms: Option<i64>,
    /// The step's note, carried so the evidence needs nothing but the captures.
    pub note: Option<String>,
}

/// Why the loop stopped.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum VisualStopReason {
    /// Every step ran.
    Completed,
    /// The plan's `max_ms` came first.
    TimeLimit,
    /// The window closed — the player quit, or the game crashed.
    WindowGone,
    /// Esc pressed twice.
    EmergencyStop,
    /// An input or a capture failed; `stopped_detail` says which.
    StepFailed,
}

impl VisualStopReason {
    /// Whether the run did everything it was asked to. Anything else means the evidence is
    /// about a shorter game than the plan describes.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        matches!(self, Self::Completed)
    }

    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Completed => "every step ran",
            Self::TimeLimit => "the run hit its time budget",
            Self::WindowGone => "the game window closed mid-run",
            Self::EmergencyStop => "Esc/Esc stopped the run",
            Self::StepFailed => "a step could not be delivered",
        }
    }
}

/// Everything one visual playtest produced.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct VisualPlaytestResult {
    pub captures: Vec<VisualCapture>,
    /// `None` when the plan asked for no telemetry, or the probe wrote nothing.
    pub telemetry: Option<TelemetryReport>,
    /// The window as it was last read. Its `process_id` is the game Bhippi launched.
    pub window: WindowRef,
    /// How long the window took to appear, from launch.
    pub window_ready_ms: u64,
    /// `None` when the run could not be waited on.
    pub exit: Option<GodotExit>,
    pub stopped_reason: VisualStopReason,
    pub stopped_detail: Option<String>,
    pub elapsed_ms: u64,
    pub log_tail: Vec<String>,
    pub telemetry_path: Option<String>,
    /// The paired, bounded summary a model reads alongside the PNGs (GAD-097). Computed here
    /// rather than in the webview: pairing is a rule, and rules live in Rust (INV-073).
    pub evidence: VisualEvidence,
}

impl VisualPlaytestResult {
    /// Pair every frame with the telemetry sample nearest it in time (GAD-097).
    ///
    /// **How the two clocks are aligned.** A frame's `at_ms` counts from the moment Bhippi
    /// spawned Godot; a telemetry sample's `time` is `Time.get_ticks_msec()`, which counts
    /// from the engine's own initialisation a little *after* that spawn. The two run at the
    /// same rate with a small unknown offset, which is why every frame also records
    /// `godot_time_ms` — the `time` of the last complete telemetry line on disk when the frame
    /// was taken. When that reading is present the pairing needs no offset at all. When it is
    /// absent the frame falls back to `at_ms` corrected by the offset the first sample and the
    /// moment the window appeared imply, which is coarser by roughly one window poll.
    ///
    /// Either way the tolerance is [`EVIDENCE_PAIR_TOLERANCE_MS`]: past it there is no pair,
    /// and a frame without one is recorded as partial evidence rather than quietly paired with
    /// whatever sample happened to be closest.
    #[must_use]
    pub fn evidence(&self) -> VisualEvidence {
        pair_evidence(
            &self.captures,
            self.telemetry.as_ref(),
            self.fallback_offset_ms(),
            &self.log_tail,
            self.stopped_reason,
            self.stopped_detail.clone(),
            self.exit,
            self.elapsed_ms,
        )
    }

    /// Godot's clock minus Bhippi's, as the window's appearance and the first sample imply it.
    fn fallback_offset_ms(&self) -> i64 {
        let Some(first) = self
            .telemetry
            .as_ref()
            .and_then(|report| report.samples.first())
        else {
            return 0;
        };
        first.time - i64::try_from(self.window_ready_ms).unwrap_or(i64::MAX)
    }
}

/// True when this result has pixels a model has to *look* at.
///
/// The hook GAD-098 asks for. `chat.rs` already owns the handoff itself and it is not
/// duplicated here: `ChatEngine::run_turn`'s computer-use branch tests the picked provider
/// with `computer::is_vision_capable`, and when it cannot see, `ChatEngine::pick_computer_provider`
/// hands the session to the first enabled, installed, authorised vision CLI in the ADR-0015
/// order (claude → codex → grok), never silently: the swap is written into a thinking line and
/// into the transcript, and when none is available the turn says the visual half was not
/// measured (ADR-0018 §5, reaffirmed by ADR-0044 §5).
///
/// A chat bridge that runs a visual playtest routes it down that same path rather than growing
/// a second one; this function is the whole of the decision it has to make.
///
/// A result with no frames is not a failure — the telemetry half still stands — but it needs
/// no vision provider, and asking for one would spend a handoff on nothing to see.
#[must_use]
pub fn requires_vision(result: &VisualPlaytestResult) -> bool {
    !result.captures.is_empty()
}

// ── evidence (GAD-097) ───────────────────────────────────────────────────────────────

/// The telemetry half of one pair, trimmed to what a model can use.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EvidenceSample {
    pub frame: u32,
    /// `Time.get_ticks_msec()` at this sample.
    pub time_ms: i64,
    /// How far this sample was from the frame. Never more than [`EVIDENCE_PAIR_TOLERANCE_MS`].
    pub skew_ms: i64,
    /// Where every node in the `bhippi_track` group was.
    pub tracked: Vec<TrackedNode>,
    /// The watched variables as of this sample.
    pub vars: BTreeMap<String, serde_json::Value>,
    /// Events since the previously paired frame — what happened *between* the two pictures.
    pub events: Vec<TelemetryEvent>,
}

/// One frame and what the game thought was true at that moment.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EvidenceFrame {
    pub step_index: Option<u32>,
    pub at_ms: u64,
    pub note: Option<String>,
    /// `None` means this frame is **half a pair** and may not promote anything to passed.
    pub telemetry_sample: Option<EvidenceSample>,
}

/// Where the run ended up, whatever the frames showed.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct EvidenceFinalState {
    /// True only when the probe wrote its `done` line — the one proof the game shut down
    /// rather than being killed mid-frame.
    pub done: bool,
    pub sample_count: u32,
    pub last_positions: BTreeMap<String, Vec<f64>>,
    pub vars: BTreeMap<String, serde_json::Value>,
    pub event_names: Vec<String>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stopped_reason: Option<VisualStopReason>,
    pub elapsed_ms: u64,
}

/// The model-facing summary of one visual playtest.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct VisualEvidence {
    pub frames: Vec<EvidenceFrame>,
    pub final_state: EvidenceFinalState,
    /// Everything that makes this evidence less than whole, in plain words. ADR-0044 §3: half
    /// a pair may never promote a quality dimension to *passed*, and this is where the halves
    /// are named so a build run can refuse rather than guess.
    pub faults: Vec<String>,
}

impl VisualEvidence {
    /// True when every frame found a sample and the game shut down cleanly.
    #[must_use]
    pub fn is_whole(&self) -> bool {
        self.faults.is_empty()
    }

    /// How many frames have a sample behind them.
    #[must_use]
    pub fn paired_frames(&self) -> usize {
        self.frames
            .iter()
            .filter(|frame| frame.telemetry_sample.is_some())
            .count()
    }
}

/// The sample nearest `target_ms`, or `None` when the nearest is further than `tolerance_ms`.
///
/// Nearest rather than "the last one before": a frame taken 40 ms after a sample belongs to
/// that sample, and a frame taken 40 ms before the next one belongs to the next.
#[must_use]
pub fn nearest_sample(
    samples: &[TelemetryLine],
    target_ms: i64,
    tolerance_ms: i64,
) -> Option<(usize, &TelemetryLine, i64)> {
    samples
        .iter()
        .enumerate()
        .map(|(index, sample)| (index, sample, sample.time - target_ms))
        .min_by_key(|(_, _, skew)| skew.abs())
        .filter(|(_, _, skew)| skew.abs() <= tolerance_ms)
}

/// Pair frames with samples. Pure, so the alignment can be tested without a game.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn pair_evidence(
    captures: &[VisualCapture],
    telemetry: Option<&TelemetryReport>,
    fallback_offset_ms: i64,
    log_tail: &[String],
    stopped_reason: VisualStopReason,
    stopped_detail: Option<String>,
    exit: Option<GodotExit>,
    elapsed_ms: u64,
) -> VisualEvidence {
    let samples: &[TelemetryLine] = telemetry.map_or(&[], |report| report.samples.as_slice());
    let mut frames: Vec<EvidenceFrame> = Vec::with_capacity(captures.len());
    let mut unpaired = 0_usize;
    // Where the previous pair landed, so "events since last" means what it says.
    let mut previous_index: Option<usize> = None;

    for capture in captures {
        let target = capture.godot_time_ms.unwrap_or_else(|| {
            i64::try_from(capture.at_ms).unwrap_or(i64::MAX) + fallback_offset_ms
        });
        let paired = nearest_sample(samples, target, EVIDENCE_PAIR_TOLERANCE_MS);
        let sample = paired.map(|(index, sample, skew)| {
            let from = previous_index.map_or(0, |previous| previous + 1);
            let events: Vec<TelemetryEvent> = samples
                .get(from..=index)
                .unwrap_or_default()
                .iter()
                .flat_map(|line| line.events.iter().cloned())
                .collect();
            previous_index = Some(index);
            EvidenceSample {
                frame: sample.frame,
                time_ms: sample.time,
                skew_ms: skew,
                tracked: sample.tracked.clone(),
                vars: sample.vars.clone(),
                events,
            }
        });
        if sample.is_none() {
            unpaired += 1;
        }
        frames.push(EvidenceFrame {
            step_index: capture.step_index,
            at_ms: capture.at_ms,
            note: capture.note.clone(),
            telemetry_sample: sample,
        });
    }

    let mut faults: Vec<String> = Vec::new();
    if captures.is_empty() {
        faults.push(
            "No frame was captured: a telemetry assertion here is half a pair (ADR-0044 §3)."
                .to_owned(),
        );
    }
    match telemetry {
        None => faults.push(
            "No telemetry was collected: a visual claim here is half a pair (ADR-0044 §3)."
                .to_owned(),
        ),
        Some(report) => {
            if report.samples.is_empty() {
                faults.push(
                    "The telemetry file is empty — check that the BhippiProbe autoload is \
                     registered and that a node is in the `bhippi_track` group."
                        .to_owned(),
                );
            }
            if !report.done {
                faults.push(
                    "The telemetry has no `done` line: the game did not shut down cleanly, so \
                     the last samples may be missing."
                        .to_owned(),
                );
            }
            if report.malformed_lines > 0 {
                faults.push(format!(
                    "{} telemetry line(s) were not JSON — the game printed into the telemetry file.",
                    report.malformed_lines
                ));
            }
            if report.truncated {
                faults.push("The telemetry was truncated at its line cap.".to_owned());
            }
        }
    }
    if unpaired > 0 {
        faults.push(format!(
            "{unpaired} of {} frame(s) found no telemetry sample within \
             {EVIDENCE_PAIR_TOLERANCE_MS} ms: those frames are partial evidence.",
            captures.len()
        ));
    }
    if !stopped_reason.is_complete() {
        faults.push(match stopped_detail {
            Some(detail) => format!("{}: {detail}", stopped_reason.describe()),
            None => format!("{}.", stopped_reason.describe()),
        });
    }
    for line in log_tail.iter().filter(|line| is_engine_error(line)).take(3) {
        faults.push(format!("Godot printed: {}", line.trim()));
    }
    faults.truncate(MAX_EVIDENCE_FAULTS);

    VisualEvidence {
        frames,
        final_state: EvidenceFinalState {
            done: telemetry.is_some_and(|report| report.done),
            sample_count: u32::try_from(samples.len()).unwrap_or(u32::MAX),
            last_positions: telemetry
                .map(|report| report.last_positions.clone())
                .unwrap_or_default(),
            vars: telemetry
                .map(|report| report.vars.clone())
                .unwrap_or_default(),
            event_names: telemetry
                .map(TelemetryReport::event_names)
                .unwrap_or_default(),
            exit_code: exit.and_then(|exit| exit.code),
            timed_out: exit.is_some_and(|exit| exit.timed_out),
            stopped_reason: Some(stopped_reason),
            elapsed_ms,
        },
        faults,
    }
}

/// Whether a Godot output line is one the evidence should carry.
fn is_engine_error(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("ERROR:")
        || trimmed.starts_with("SCRIPT ERROR:")
        || trimmed.starts_with("USER ERROR:")
        || trimmed.starts_with("USER SCRIPT ERROR:")
}

// ── the emergency stop ───────────────────────────────────────────────────────────────

/// The armed Esc/Esc watcher. Aborting the task drops the child, which is `kill_on_drop`, so
/// nothing outlives the playtest that armed it.
pub struct EscapeWatcher {
    task: Option<tokio::task::JoinHandle<()>>,
    /// Kept alive on platforms with no watcher so the receiver's channel stays open.
    _sender: Option<watch::Sender<bool>>,
}

impl Drop for EscapeWatcher {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// Arm the emergency stop for the life of one visual playtest (INV-089).
///
/// Armed *before* anything is launched and dropped when the loop returns, so the stop exists
/// for exactly as long as there is something to stop. It is deliberately its own watcher
/// rather than the desktop overlay's: the overlay belongs to a Computer Use turn, an engine
/// playtest is not one, and a stop that depends on chrome existing is a stop that can be
/// disarmed by a window failing to open.
#[must_use]
pub fn arm_emergency_stop() -> (watch::Receiver<bool>, EscapeWatcher) {
    let (sender, receiver) = watch::channel(false);
    #[cfg(windows)]
    {
        let task = tokio::spawn(async move {
            if let Err(error) = watch_escape(sender).await {
                tracing::debug!(%error, "the Esc/Esc watcher stopped early");
            }
        });
        (
            receiver,
            EscapeWatcher {
                task: Some(task),
                _sender: None,
            },
        )
    }
    #[cfg(not(windows))]
    {
        (
            receiver,
            EscapeWatcher {
                task: None,
                _sender: Some(sender),
            },
        )
    }
}

/// Poll `GetAsyncKeyState` for Escape and report the second press inside the window.
///
/// One PowerShell process for the life of the playtest, `CREATE_NO_WINDOW`, killed on drop —
/// the same shape as every other bridge in the Computer Use code. It reports *transitions*,
/// so a key held down is one press.
#[cfg(windows)]
async fn watch_escape(sender: watch::Sender<bool>) -> std::io::Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    const SCRIPT: &str = concat!(
        "$ErrorActionPreference = 'Stop'\n",
        "Add-Type -Namespace BhippiEsc -Name Keys -MemberDefinition '",
        "[DllImport(\"user32.dll\")] public static extern short GetAsyncKeyState(int k);'\n",
        "$was = $false\n",
        "while ($true) {\n",
        "  $down = (([BhippiEsc.Keys]::GetAsyncKeyState(0x1B) -band 0x8000) -ne 0)\n",
        "  if ($down -and -not $was) { Write-Output 'E' }\n",
        "  $was = $down\n",
        "  Start-Sleep -Milliseconds 30\n",
        "}\n",
    );

    let mut child = tokio::process::Command::new("powershell")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            SCRIPT,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .creation_flags(0x0800_0000)
        .spawn()?;
    let Some(stdout) = child.stdout.take() else {
        return Ok(());
    };
    let mut lines = BufReader::new(stdout).lines();
    let mut last: Option<Instant> = None;
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim() != "E" {
            continue;
        }
        let now = Instant::now();
        if last.is_some_and(|at| now.saturating_duration_since(at) <= ESCAPE_DOUBLE_WINDOW) {
            tracing::info!("Esc/Esc stopped the visual playtest");
            let _ignored = sender.send(true);
            break;
        }
        last = Some(now);
    }
    let _ignored = child.kill().await;
    Ok(())
}

// ── the loop ─────────────────────────────────────────────────────────────────────────

/// Everything the runner needs that is not the plan.
pub struct VisualLaunch<'a> {
    /// The project root — a path a caller has already resolved and registered.
    pub root: &'a Path,
    /// The **GUI** Godot binary. A windowed run through the console build flashes a console.
    pub gui_exe: &'a Path,
    /// The game's own name, which is what Godot puts in the window title.
    pub game_name: &'a str,
    /// The stop channel whoever owns the process slot is holding.
    pub stop: (GodotProcessHandle, GodotStopSignal),
    /// Where engine output goes. `None` in a test that has no pump.
    pub lines: Option<tokio::sync::mpsc::UnboundedSender<GodotOutputLine>>,
    /// Called once with the game window the moment it exists, before it is focused. The
    /// studio hands the window to the embedded viewport here (ADR-0045) so Watch play happens
    /// inside Bhippi's window like every other run. An error stops the playtest: a game
    /// window that cannot be embedded must not stay open on its own.
    pub on_window: Option<Box<WindowHook>>,
}

/// See [`VisualLaunch::on_window`].
pub type WindowHook = dyn Fn(&WindowRef) -> Result<(), AppError> + Send + Sync;

/// Launch the game windowed, watch it, play it, and bring back the pair.
///
/// The Tauri command is a thin wrapper over this: it resolves the project, claims the process
/// slot and announces the run. Everything that makes the loop what it is lives here, so the
/// live test can drive it without a Tauri runtime.
pub async fn run_visual_playtest(
    launch: VisualLaunch<'_>,
    plan: &VisualPlaytestPlan,
) -> Result<VisualPlaytestResult, AppError> {
    plan.validate()?;

    // Armed before anything is launched (INV-089): a game that opens and immediately grabs
    // the keyboard must still be stoppable.
    let (stop_rx, _escape) = arm_emergency_stop();

    let telemetry_path = if plan.telemetry {
        Some(prepare_telemetry(launch.root)?)
    } else {
        None
    };
    let filter = game_window_filter(launch.game_name);
    // Whatever already matched before the launch is not the window Bhippi started — the Godot
    // editor's own window has class `Engine` too, and so does a game the user ran by hand.
    let before: BTreeSet<u64> = find_windows(filter.clone())
        .await
        .map(|windows| windows.into_iter().map(|window| window.hwnd).collect())
        .unwrap_or_default();

    let spec = {
        let mut spec = run_command(
            launch.gui_exe,
            launch.root,
            &RunOptions {
                headless: false,
                user_args: telemetry_path
                    .as_ref()
                    .map(|path| vec![format!("{TELEMETRY_ARG}{}", path.display())])
                    .unwrap_or_default(),
                ..RunOptions::default()
            },
        );
        if let Some(path) = &telemetry_path {
            spec.env
                .push((TELEMETRY_ENV.to_owned(), path.display().to_string()));
        }
        // The loop owns the stop; the runner's own timeout is only the backstop for a loop
        // that never gets to ask.
        spec.timeout_secs = plan.max_ms / 1_000 + VISUAL_SHUTDOWN_GRACE_MS / 1_000 + 60;
        spec
    };

    let started = Instant::now();
    let (killer, signal) = launch.stop;
    let sink = launch.lines;
    let run = tokio::spawn(async move {
        let mut tail: Vec<String> = Vec::new();
        let result = run_spec_with_stop(&spec, Some(signal), |line| {
            if tail.len() >= LOG_TAIL {
                tail.remove(0);
            }
            tail.push(line.text.clone());
            if let Some(sender) = sink.as_ref() {
                let _ignored = sender.send(line);
            }
        })
        .await;
        drop(sink);
        (result, tail)
    });

    // From here on every early return has to stop the game, so the failures share one tail.
    let outcome = observe(
        &before,
        &filter,
        plan,
        telemetry_path.as_deref(),
        started,
        &stop_rx,
        launch.on_window.as_deref(),
    )
    .await;

    let (scope, captures, stopped_reason, stopped_detail, window_ready_ms) = match outcome {
        Ok(observed) => observed,
        Err(error) => {
            killer.kill();
            let _ignored = run.await;
            return Err(error);
        }
    };

    // Ask, then insist. `WM_CLOSE` is what makes the probe write its `done` line; the kill is
    // only for a game that ignores it.
    if let Err(error) = scope.request_close().await {
        tracing::debug!(%error, "the game window would not take a close request");
    }
    let closed = tokio::time::timeout(Duration::from_millis(VISUAL_SHUTDOWN_GRACE_MS), run).await;
    let (exit, log_tail) = match closed {
        Ok(Ok((result, tail))) => (result.ok(), tail),
        Ok(Err(_)) => (None, Vec::new()),
        Err(_) => {
            // It did not go on its own: this is `godot_stop`'s kill, by the same handle the
            // pane's Stop button holds.
            killer.kill();
            (None, Vec::new())
        }
    };

    let telemetry = telemetry_path.as_ref().map(|path| {
        TelemetryReport::from_jsonl(&std::fs::read_to_string(path).unwrap_or_default())
    });

    let mut result = VisualPlaytestResult {
        captures,
        telemetry,
        window: scope.window().clone(),
        window_ready_ms,
        exit,
        stopped_reason,
        stopped_detail,
        elapsed_ms: elapsed_ms(started),
        log_tail,
        telemetry_path: telemetry_path
            .as_ref()
            .map(|path| path.display().to_string()),
        evidence: VisualEvidence::default(),
    };
    result.evidence = result.evidence();
    Ok(result)
}

/// What the observation half produced: the scope it was bound to, the frames, and why it ended.
type Observed = (
    EngineCaptureScope,
    Vec<VisualCapture>,
    VisualStopReason,
    Option<String>,
    u64,
);

/// Find the window, hand it over, focus it, then run the plan against it.
#[allow(clippy::too_many_arguments)]
async fn observe(
    before: &BTreeSet<u64>,
    filter: &WindowFilter,
    plan: &VisualPlaytestPlan,
    telemetry_path: Option<&Path>,
    started: Instant,
    stop: &watch::Receiver<bool>,
    on_window: Option<&WindowHook>,
) -> Result<Observed, AppError> {
    let window = wait_for_window(before, filter, started, stop).await?;
    let window_ready_ms = elapsed_ms(started);
    if let Some(hook) = on_window {
        hook(&window)?;
    }
    let mut scope = EngineCaptureScope::new(window);
    tracing::info!(
        hwnd = scope.window().hwnd,
        pid = scope.window().process_id,
        title = %scope.window().title,
        window_ready_ms,
        "the game window is up"
    );
    // Focus first: a screen copy needs the window in front, and input is refused when it is not.
    scope.focus().await?;

    let mut captures: Vec<VisualCapture> = Vec::with_capacity(plan.planned_captures());
    let options = CaptureOptions {
        scale: None,
        max_bytes: VISUAL_CAPTURE_MAX_BYTES,
    };
    // The opening frame: what the game looks like before anybody touches it.
    match take_frame(&scope, options, None, None, telemetry_path, started).await {
        Ok(frame) => captures.push(frame),
        Err(error) => {
            return Ok((
                scope,
                captures,
                stop_reason_for(&error),
                Some(error.to_string()),
                window_ready_ms,
            ))
        }
    }

    for (index, step) in plan.steps.iter().enumerate() {
        let step_index = u32::try_from(index).unwrap_or(u32::MAX);
        if *stop.borrow() {
            return Ok((
                scope,
                captures,
                VisualStopReason::EmergencyStop,
                None,
                window_ready_ms,
            ));
        }
        if elapsed_ms(started) >= plan.max_ms {
            return Ok((
                scope,
                captures,
                VisualStopReason::TimeLimit,
                Some(format!(
                    "stopped after step {index} of {}",
                    plan.steps.len()
                )),
                window_ready_ms,
            ));
        }
        // The rect is live data: a game window can be moved between two steps, and one that
        // has gone must end the run rather than let the next keystroke land elsewhere.
        if let Err(error) = scope.refresh().await {
            return Ok((
                scope,
                captures,
                stop_reason_for(&error),
                Some(error.to_string()),
                window_ready_ms,
            ));
        }
        if let Some(input) = step.input.clone() {
            if let Err(error) = scope.send(input).await {
                return Ok((
                    scope,
                    captures,
                    stop_reason_for(&error),
                    Some(format!("step {}: {error}", index + 1)),
                    window_ready_ms,
                ));
            }
        }
        if let Some(hold_ms) = step.hold_ms {
            tokio::time::sleep(Duration::from_millis(hold_ms)).await;
        }
        if plan.capture_every_step && captures.len() < VISUAL_MAX_CAPTURES {
            match take_frame(
                &scope,
                options,
                Some(step_index),
                step.note.clone(),
                telemetry_path,
                started,
            )
            .await
            {
                Ok(frame) => captures.push(frame),
                Err(error) => {
                    return Ok((
                        scope,
                        captures,
                        stop_reason_for(&error),
                        Some(format!("step {}: {error}", index + 1)),
                        window_ready_ms,
                    ))
                }
            }
        }
    }
    Ok((
        scope,
        captures,
        VisualStopReason::Completed,
        None,
        window_ready_ms,
    ))
}

/// One frame, plus the game's own clock reading at the moment it was taken.
async fn take_frame(
    scope: &EngineCaptureScope,
    options: CaptureOptions,
    step_index: Option<u32>,
    note: Option<String>,
    telemetry_path: Option<&Path>,
    started: Instant,
) -> Result<VisualCapture, WindowError> {
    let capture = scope.capture(options).await?;
    // Read the clock *after* the capture returns: the bridge grabs the frame at the end of
    // its own round trip, so that is the moment the pixels are from.
    let at_ms = elapsed_ms(started);
    let godot_time_ms = telemetry_path.and_then(telemetry_tail_time);
    Ok(VisualCapture {
        step_index,
        png_base64: capture.png_base64,
        width: capture.width,
        height: capture.height,
        method: capture.method,
        at_ms,
        godot_time_ms,
        note,
    })
}

/// Which stop reason a window failure means.
fn stop_reason_for(error: &WindowError) -> VisualStopReason {
    match error {
        WindowError::WindowClosed { .. } | WindowError::NotFound { .. } => {
            VisualStopReason::WindowGone
        }
        WindowError::Stopped => VisualStopReason::EmergencyStop,
        _ => VisualStopReason::StepFailed,
    }
}

/// Poll until the game window Bhippi just launched appears.
///
/// The runner does not hand back the child's process id — `run_spec_with_stop` owns the
/// `Child` so a second owner cannot reap it — so the window is identified by class and title
/// instead, with the pre-launch snapshot doing the work the pid would: a window that was
/// already there when Bhippi launched the game is not the window Bhippi launched, and the
/// Godot editor's own window (class `Engine`, title containing the project name) is exactly
/// the one that would otherwise be picked up.
async fn wait_for_window(
    before: &BTreeSet<u64>,
    filter: &WindowFilter,
    started: Instant,
    stop: &watch::Receiver<bool>,
) -> Result<WindowRef, AppError> {
    let deadline = Instant::now() + Duration::from_millis(WINDOW_WAIT_MS);
    let mut last_error: Option<WindowError> = None;
    loop {
        if *stop.borrow() {
            return Err(AppError {
                message: "The visual playtest was stopped before the game window appeared."
                    .to_owned(),
                hint: Some("Press Watch play again to try once more.".to_owned()),
            });
        }
        match find_windows(filter.clone()).await {
            Ok(found) => {
                if let Some(window) = pick_game_window(found, before) {
                    return Ok(window);
                }
            }
            Err(error) => {
                // A bridge hiccup is worth one more poll; the deadline is what ends this.
                tracing::debug!(%error, "window enumeration failed while waiting for the game");
                last_error = Some(error);
            }
        }
        if Instant::now() >= deadline {
            let waited = elapsed_ms(started);
            return Err(AppError {
                message: match last_error {
                    Some(error) => format!(
                        "The game window did not appear within {WINDOW_WAIT_MS} ms: {error}"
                    ),
                    None => format!(
                        "The game window did not appear within {WINDOW_WAIT_MS} ms (waited \
                         {waited} ms from launch)."
                    ),
                },
                hint: Some(
                    "Check the Output log: a project that fails to start prints why. A first \
                     run also imports every asset, which can take longer than this."
                        .to_owned(),
                ),
            });
        }
        tokio::time::sleep(Duration::from_millis(WINDOW_POLL_MS)).await;
    }
}

/// The best candidate among what enumeration found: a window that was not there before, is not
/// the editor, and — among those — is the biggest.
#[must_use]
fn pick_game_window(found: Vec<WindowRef>, before: &BTreeSet<u64>) -> Option<WindowRef> {
    let mut fresh: Vec<WindowRef> = found
        .into_iter()
        .filter(|window| !before.contains(&window.hwnd))
        .filter(|window| !is_editor_window(window))
        .collect();
    // Largest client area wins: a splash or a tooltip is never the biggest window a game has.
    fresh.sort_by_key(|window| {
        std::cmp::Reverse(u64::from(window.rect.width) * u64::from(window.rect.height))
    });
    fresh.into_iter().next()
}

/// Godot's editor has the same window class as the game it runs, and its title carries the
/// project name too. What tells them apart is the editor's own suffix.
#[must_use]
fn is_editor_window(window: &WindowRef) -> bool {
    let title = window.title.to_lowercase();
    title.contains("godot engine") || title.ends_with("- godot")
}

/// Class and title: the two things a Godot game window is known by.
#[must_use]
pub fn game_window_filter(game_name: &str) -> WindowFilter {
    let title = game_name.trim();
    WindowFilter {
        // Godot 4 names its window class `Engine`, for the game and the editor alike.
        class_contains: Some("Engine".to_owned()),
        title_contains: (!title.is_empty()).then(|| title.to_owned()),
        process_id: None,
    }
}

/// `.bhippi/telemetry/<ulid>.jsonl`, created and ready to be appended to.
fn prepare_telemetry(root: &Path) -> Result<PathBuf, AppError> {
    let directory = root.join(".bhippi").join("telemetry");
    std::fs::create_dir_all(&directory).map_err(|error| AppError {
        message: format!("Could not create the telemetry folder: {error}"),
        hint: Some("Check the project folder is writable.".to_owned()),
    })?;
    Ok(directory.join(format!("{}.jsonl", ulid::Ulid::new())))
}

/// The `time` of the last **complete** telemetry line on disk.
///
/// The probe flushes every sample, so the file is readable while the game runs. A half-written
/// last line simply fails to parse and the line before it answers instead, which is why this
/// reads back rather than taking the final line on faith.
#[must_use]
fn telemetry_tail_time(path: &Path) -> Option<i64> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().rev().find_map(|line| {
        let line = line.trim();
        if !line.starts_with('{') {
            return None;
        }
        serde_json::from_str::<serde_json::Value>(line)
            .ok()?
            .get("time")
            .and_then(serde_json::Value::as_i64)
    })
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer_window::{CaptureScope, WindowMouseButton, WindowRect};

    fn window() -> WindowRef {
        WindowRef {
            hwnd: 4_242,
            title: "Live Game".to_owned(),
            class_name: "Engine".to_owned(),
            process_id: 909,
            rect: WindowRect {
                x: 0,
                y: 0,
                width: 1_152,
                height: 648,
            },
            frame: WindowRect {
                x: 0,
                y: 0,
                width: 1_160,
                height: 688,
            },
            dpi_scale: 1.0,
        }
    }

    fn frame(step_index: Option<u32>, at_ms: u64, godot_time_ms: Option<i64>) -> VisualCapture {
        VisualCapture {
            step_index,
            png_base64: "iVBORw0KGgo=".to_owned(),
            width: 1_152,
            height: 648,
            method: WindowCaptureMethod::PrintWindow,
            at_ms,
            godot_time_ms,
            note: step_index.map(|index| format!("step {index}")),
        }
    }

    fn sample(frame: u32, time: i64, x: f64, events: &[&str]) -> TelemetryLine {
        TelemetryLine {
            frame,
            time,
            fps: 60.0,
            scene: "Main".to_owned(),
            node_count: 9,
            tracked: vec![TrackedNode {
                path: "/root/Main/Player".to_owned(),
                pos: Some(vec![x, 1.0, 0.0]),
                vel: None,
            }],
            vars: BTreeMap::new(),
            events: events
                .iter()
                .map(|name| TelemetryEvent {
                    frame,
                    name: (*name).to_owned(),
                    data: None,
                })
                .collect(),
        }
    }

    /// A synthetic run: samples every 100 ms, exactly as the probe writes them at 60 fps.
    fn report(done: bool) -> TelemetryReport {
        let mut report = TelemetryReport {
            done,
            ..TelemetryReport::default()
        };
        for step in 0..20_u32 {
            let time = i64::from(step) * 100;
            let names: &[&str] = if step == 7 { &["jumped"] } else { &[] };
            report
                .samples
                .push(sample(step * 6, time, f64::from(step) * 0.5, names));
        }
        report
            .last_positions
            .insert("/root/Main/Player".to_owned(), vec![9.5, 1.0, 0.0]);
        report.events.push(TelemetryEvent {
            frame: 42,
            name: "jumped".to_owned(),
            data: None,
        });
        report
    }

    // ── plan validation ──────────────────────────────────────────────────────────

    #[test]
    fn the_default_watch_play_plan_is_four_frames_and_valid() {
        let plan = VisualPlaytestPlan::watch_play();
        assert!(plan.validate().is_ok(), "{:?}", plan.validate());
        assert_eq!(plan.planned_captures(), 4);
        assert_eq!(plan.steps.len(), 3);
        assert!(plan.telemetry, "a frame with no sample is half a pair");
        assert!(plan.max_ms <= VISUAL_PLAYTEST_MAX_MS);
    }

    #[test]
    fn a_plan_past_any_cap_is_refused_before_anything_launches() {
        let base = VisualPlaytestPlan::watch_play();
        let refusals: Vec<VisualPlaytestPlan> = vec![
            // no steps at all
            VisualPlaytestPlan {
                steps: Vec::new(),
                ..base.clone()
            },
            // past the step cap
            VisualPlaytestPlan {
                steps: (0..=VISUAL_PLAYTEST_MAX_STEPS)
                    .map(|_| VisualStep {
                        input: None,
                        hold_ms: None,
                        note: None,
                    })
                    .collect(),
                capture_every_step: false,
                ..base.clone()
            },
            // past the time cap
            VisualPlaytestPlan {
                max_ms: VISUAL_PLAYTEST_MAX_MS + 1,
                ..base.clone()
            },
            VisualPlaytestPlan {
                max_ms: 0,
                ..base.clone()
            },
            // past the frame cap: every step photographed
            VisualPlaytestPlan {
                steps: (0..VISUAL_MAX_CAPTURES)
                    .map(|_| VisualStep {
                        input: None,
                        hold_ms: None,
                        note: None,
                    })
                    .collect(),
                capture_every_step: true,
                ..base.clone()
            },
            // a key that resolves to nothing
            VisualPlaytestPlan {
                steps: vec![VisualStep::hold("KEY_NOT_A_KEY", 100, "nope")],
                ..base.clone()
            },
            // a hold past the module's ceiling
            VisualPlaytestPlan {
                steps: vec![VisualStep::hold(
                    "KEY_W",
                    WINDOW_HOLD_MAX_MS + 1,
                    "too long",
                )],
                ..base.clone()
            },
            // a dwell past the step ceiling
            VisualPlaytestPlan {
                steps: vec![VisualStep::tap(
                    "KEY_SPACE",
                    VISUAL_STEP_MAX_HOLD_MS + 1,
                    "wait",
                )],
                ..base.clone()
            },
            // a note nobody would read
            VisualPlaytestPlan {
                steps: vec![VisualStep {
                    input: None,
                    hold_ms: None,
                    note: Some("n".repeat(VISUAL_NOTE_MAX_CHARS + 1)),
                }],
                ..base.clone()
            },
            // holds that cannot fit the budget
            VisualPlaytestPlan {
                steps: vec![
                    VisualStep::hold("KEY_W", 5_000, "walk"),
                    VisualStep::hold("KEY_A", 5_000, "strafe"),
                ],
                max_ms: 4_000,
                ..base.clone()
            },
        ];
        for plan in refusals {
            let error = match plan.validate() {
                Ok(()) => panic!("this plan must be refused: {plan:?}"),
                Err(error) => error,
            };
            assert!(error.hint.is_some(), "{} needs a hint", error.message);
        }
        // The frame cap is exactly at the boundary, not one past it.
        let exact = VisualPlaytestPlan {
            steps: (0..VISUAL_MAX_CAPTURES - 1)
                .map(|_| VisualStep {
                    input: None,
                    hold_ms: None,
                    note: None,
                })
                .collect(),
            capture_every_step: true,
            ..base
        };
        assert_eq!(exact.planned_captures(), VISUAL_MAX_CAPTURES);
        assert!(exact.validate().is_ok());
    }

    #[test]
    fn a_coordinate_step_is_shape_checked_without_a_window() {
        let plan = VisualPlaytestPlan {
            steps: vec![VisualStep {
                input: Some(WindowInput::Click {
                    x: 40,
                    y: 40,
                    button: WindowMouseButton::Left,
                }),
                hold_ms: Some(100),
                note: Some("click the start button".to_owned()),
            }],
            ..VisualPlaytestPlan::watch_play()
        };
        assert!(plan.validate().is_ok());
    }

    // ── evidence pairing (GAD-097) ───────────────────────────────────────────────

    #[test]
    fn every_frame_is_paired_with_the_sample_nearest_it_in_time() {
        let telemetry = report(true);
        let captures = vec![
            frame(None, 40, Some(0)),
            frame(Some(0), 540, Some(500)),
            frame(Some(1), 1_240, Some(1_200)),
        ];
        let evidence = pair_evidence(
            &captures,
            Some(&telemetry),
            0,
            &[],
            VisualStopReason::Completed,
            None,
            Some(GodotExit {
                code: Some(0),
                timed_out: false,
                duration_ms: 4_000,
            }),
            4_000,
        );
        assert_eq!(evidence.frames.len(), 3);
        assert_eq!(evidence.paired_frames(), 3);
        let times: Vec<i64> = evidence
            .frames
            .iter()
            .filter_map(|frame| frame.telemetry_sample.as_ref().map(|s| s.time_ms))
            .collect();
        assert_eq!(times, vec![0, 500, 1_200]);
        assert!(evidence
            .frames
            .iter()
            .filter_map(|frame| frame.telemetry_sample.as_ref())
            .all(|sample| sample.skew_ms.abs() <= EVIDENCE_PAIR_TOLERANCE_MS));
        // Events are attributed to the frame they happened before, not to every frame.
        let jumped: Vec<usize> = evidence
            .frames
            .iter()
            .enumerate()
            .filter(|(_, frame)| {
                frame
                    .telemetry_sample
                    .as_ref()
                    .is_some_and(|sample| sample.events.iter().any(|e| e.name == "jumped"))
            })
            .map(|(index, _)| index)
            .collect();
        assert_eq!(jumped, vec![2], "the jump happened between frames 1 and 2");
        assert!(evidence.is_whole(), "faults: {:?}", evidence.faults);
        assert!(evidence.final_state.done);
        assert_eq!(evidence.final_state.sample_count, 20);
        assert_eq!(evidence.final_state.exit_code, Some(0));
    }

    #[test]
    fn the_fallback_offset_aligns_a_frame_that_never_read_the_games_clock() {
        let telemetry = report(true);
        // No `godot_time_ms`: the frame only knows Bhippi's clock. The offset says Godot's
        // clock was 700 ms behind it, which is engine start-up.
        let captures = vec![frame(Some(0), 1_900, None)];
        let evidence = pair_evidence(
            &captures,
            Some(&telemetry),
            -700,
            &[],
            VisualStopReason::Completed,
            None,
            None,
            2_000,
        );
        let paired = evidence.frames[0]
            .telemetry_sample
            .as_ref()
            .map(|sample| sample.time_ms);
        assert_eq!(
            paired,
            Some(1_200),
            "1900 - 700 lands on the 1200 ms sample"
        );
    }

    #[test]
    fn a_frame_outside_the_tolerance_is_half_a_pair_and_says_so() {
        let telemetry = report(true);
        // 400 ms past the last sample: further than the tolerance from anything.
        let captures = vec![frame(None, 0, Some(0)), frame(Some(0), 2_300, Some(2_300))];
        let evidence = pair_evidence(
            &captures,
            Some(&telemetry),
            0,
            &[],
            VisualStopReason::Completed,
            None,
            None,
            2_400,
        );
        assert_eq!(evidence.paired_frames(), 1);
        assert!(evidence.frames[1].telemetry_sample.is_none());
        assert!(!evidence.is_whole());
        assert!(
            evidence
                .faults
                .iter()
                .any(|fault| fault.contains("1 of 2 frame(s) found no telemetry sample")),
            "{:?}",
            evidence.faults
        );
        // Just inside the tolerance still pairs: the boundary is inclusive.
        let edge = vec![frame(Some(0), 0, Some(1_900 - EVIDENCE_PAIR_TOLERANCE_MS))];
        let evidence = pair_evidence(
            &edge,
            Some(&telemetry),
            0,
            &[],
            VisualStopReason::Completed,
            None,
            None,
            0,
        );
        assert_eq!(evidence.paired_frames(), 1);
    }

    #[test]
    fn half_a_pair_in_either_direction_is_a_fault() {
        // Frames, no telemetry.
        let visual_only = pair_evidence(
            &[frame(None, 0, None)],
            None,
            0,
            &[],
            VisualStopReason::Completed,
            None,
            None,
            100,
        );
        assert!(visual_only
            .faults
            .iter()
            .any(|fault| fault.contains("No telemetry was collected")));

        // Telemetry, no frames.
        let telemetry_only = pair_evidence(
            &[],
            Some(&report(true)),
            0,
            &[],
            VisualStopReason::Completed,
            None,
            None,
            100,
        );
        assert!(telemetry_only
            .faults
            .iter()
            .any(|fault| fault.contains("No frame was captured")));
        assert!(telemetry_only.frames.is_empty());

        // A killed game: no `done` line, and the reason said out loud.
        let killed = pair_evidence(
            &[frame(None, 0, Some(0))],
            Some(&report(false)),
            0,
            &["SCRIPT ERROR: Parse Error: something".to_owned()],
            VisualStopReason::WindowGone,
            Some("the window closed".to_owned()),
            None,
            900,
        );
        assert!(killed
            .faults
            .iter()
            .any(|fault| fault.contains("no `done` line")));
        assert!(killed
            .faults
            .iter()
            .any(|fault| fault.contains("the game window closed mid-run")));
        assert!(killed
            .faults
            .iter()
            .any(|fault| fault.contains("SCRIPT ERROR")));
        assert!(killed.faults.len() <= MAX_EVIDENCE_FAULTS);
    }

    #[test]
    fn nearest_sample_prefers_the_closer_side_and_respects_the_tolerance() {
        let samples = [sample(0, 0, 0.0, &[]), sample(6, 100, 1.0, &[])];
        let (index, _, skew) = nearest_sample(&samples, 60, 250).expect("a pair inside tolerance");
        assert_eq!(index, 1, "60 ms is nearer 100 than 0");
        assert_eq!(skew, 40);
        assert!(nearest_sample(&samples, 400, 250).is_none());
        assert!(nearest_sample(&[], 0, 250).is_none());
    }

    // ── vision handoff (GAD-098) ─────────────────────────────────────────────────

    #[test]
    fn only_a_result_with_frames_needs_a_vision_provider() {
        let mut result = VisualPlaytestResult {
            captures: vec![frame(None, 0, Some(0))],
            telemetry: Some(report(true)),
            window: window(),
            window_ready_ms: 900,
            exit: None,
            stopped_reason: VisualStopReason::Completed,
            stopped_detail: None,
            elapsed_ms: 5_000,
            log_tail: Vec::new(),
            telemetry_path: None,
            evidence: VisualEvidence::default(),
        };
        assert!(requires_vision(&result));
        result.captures.clear();
        assert!(
            !requires_vision(&result),
            "a telemetry-only result must not spend a vision handoff on nothing to see"
        );
        // The method and the stored field agree; one is just the other, recomputed.
        result.captures.push(frame(None, 0, Some(0)));
        result.evidence = result.evidence();
        assert_eq!(result.evidence, result.evidence());
        // window_ready_ms 900 against a first sample at 0 means Godot's clock ran 900 ms
        // behind Bhippi's, which is the start-up the fallback offset exists to absorb.
        assert_eq!(result.fallback_offset_ms(), -900);
    }

    // ── scope (GAD-012 / INV-089) ────────────────────────────────────────────────

    #[test]
    fn the_playtest_loop_can_only_ever_capture_one_window() {
        let scope = EngineCaptureScope::new(window());
        assert!(matches!(scope.scope(), CaptureScope::Window { .. }));
        assert!(!scope.scope().is_desktop());
        assert_eq!(scope.window().hwnd, window().hwnd);
    }

    /// GAD-099's architecture grep, as a test rather than a CI incantation: this module must
    /// not name the desktop-wide entry points at all. The needles are split so the assertion
    /// does not match itself.
    #[test]
    fn no_engine_path_names_the_desktop_wide_capture() {
        let source = include_str!("godot_observe.rs");
        for needle in [
            concat!("capture_", "screen"),
            concat!("screen_", "bounds"),
            concat!("execute_", "action"),
            concat!("Computer", "Action"),
        ] {
            assert!(
                !source.contains(needle),
                "the engine observation path must never reach `{needle}`"
            );
        }
        // And it does reach the one that is bounded to a window.
        assert!(source.contains("EngineCaptureScope"));
    }

    // ── window identification ────────────────────────────────────────────────────

    #[test]
    fn the_window_picked_is_the_new_one_that_is_not_the_editor() {
        let filter = game_window_filter("Live Game");
        assert_eq!(filter.class_contains.as_deref(), Some("Engine"));
        assert_eq!(filter.title_contains.as_deref(), Some("Live Game"));
        assert!(filter.process_id.is_none());

        let editor = WindowRef {
            hwnd: 11,
            title: "Live Game - Main.tscn - Godot Engine".to_owned(),
            ..window()
        };
        let stale = WindowRef {
            hwnd: 12,
            ..window()
        };
        let game = WindowRef {
            hwnd: 13,
            ..window()
        };
        assert!(filter.matches(&editor) && filter.matches(&stale) && filter.matches(&game));

        let before: BTreeSet<u64> = [11_u64, 12].into_iter().collect();
        let picked = pick_game_window(vec![editor.clone(), stale, game.clone()], &before);
        assert_eq!(picked.map(|window| window.hwnd), Some(game.hwnd));

        // The editor is refused even when it is the only new window: it is not the game.
        assert!(pick_game_window(vec![editor], &BTreeSet::new()).is_none());
        // Two fresh candidates: the bigger one is the game, not the splash.
        let splash = WindowRef {
            hwnd: 14,
            rect: WindowRect {
                x: 0,
                y: 0,
                width: 200,
                height: 100,
            },
            ..window()
        };
        let picked = pick_game_window(vec![splash, game.clone()], &BTreeSet::new());
        assert_eq!(picked.map(|window| window.hwnd), Some(game.hwnd));
    }

    #[test]
    fn a_game_with_no_name_still_gets_a_filter_that_matches_a_godot_window() {
        let filter = game_window_filter("   ");
        assert!(filter.title_contains.is_none());
        assert!(filter.matches(&window()));
    }

    // ── the live telemetry tail ──────────────────────────────────────────────────

    #[test]
    fn the_tail_reader_skips_a_half_written_last_line() {
        let path =
            std::env::temp_dir().join(format!("bhippi-visual-tail-{}.jsonl", ulid::Ulid::new()));
        std::fs::write(
            &path,
            "{\"frame\":0,\"time\":16,\"tracked\":[]}\n\
             {\"frame\":6,\"time\":116,\"tracked\":[]}\n\
             {\"frame\":12,\"time\":2",
        )
        .expect("write the fixture");
        assert_eq!(telemetry_tail_time(&path), Some(116));

        std::fs::write(&path, "not json at all\n").expect("write the fixture");
        assert_eq!(telemetry_tail_time(&path), None);
        assert_eq!(telemetry_tail_time(Path::new("no-such-file.jsonl")), None);
        let _ignored = std::fs::remove_file(&path);
    }

    #[test]
    fn stop_reasons_map_a_closed_window_to_a_reason_and_not_to_a_retry() {
        assert_eq!(
            stop_reason_for(&WindowError::WindowClosed { hwnd: 1 }),
            VisualStopReason::WindowGone
        );
        assert_eq!(
            stop_reason_for(&WindowError::NotFound {
                filter: "anything".to_owned()
            }),
            VisualStopReason::WindowGone
        );
        assert_eq!(
            stop_reason_for(&WindowError::Stopped),
            VisualStopReason::EmergencyStop
        );
        assert_eq!(
            stop_reason_for(&WindowError::FocusRefused { hwnd: 1 }),
            VisualStopReason::StepFailed
        );
        assert!(!VisualStopReason::TimeLimit.is_complete());
        assert!(VisualStopReason::Completed.is_complete());
    }

    #[tokio::test]
    async fn the_emergency_stop_is_armed_disarmed_and_starts_unpressed() {
        let (stop, watcher) = arm_emergency_stop();
        assert!(!*stop.borrow(), "nothing is stopped before Esc is pressed");
        drop(watcher);
        // The receiver survives its watcher: a loop that is already returning still reads a
        // sane value rather than panicking on a closed channel.
        assert!(!*stop.borrow());
    }
}
