//! Window-targeted Computer Use: watching and playing a game in its own native window.
//!
//! `computer.rs` owns the desktop-wide surface. This module owns the narrower one: a single
//! top-level window — typically a Godot 4 game window, whose class is `Engine` and whose title
//! is the project name — that the model observes and drives without ever touching the rest of
//! the desktop. Every function here is scoped to one `WindowRef`; nothing falls back to a
//! desktop-wide action, and a coordinate outside the target window is refused, never clamped.
//!
//! It keeps `computer.rs`'s bargain: one PowerShell shim, one DPI declaration, one set of
//! metrics, so that the pixels the model looked at and the pointer that moves afterwards agree.
//! Two conventions make that agreement exact:
//!
//! * `WindowRef::rect` is the **client area** in screen pixels — the same pixels
//!   `capture_window` returns. `WindowRef::frame` carries the DWM extended frame bounds for
//!   callers that need the whole window instead. Translating a coordinate read off a capture
//!   therefore lands where the model aimed, with no title-bar offset to guess at.
//! * Coordinates in `WindowInput` are **logical** client coordinates. `dpi_scale` converts them
//!   to the physical pixels the rect is expressed in, so the same numbers work on a 100%, 150%
//!   or 200% display.
//!
//! Input is injected with `SendInput` using **scan codes**, not the `keybd_event` virtual-key
//! path `computer.rs` uses for the desktop. Games do not read the Win32 message queue: Godot,
//! like most engines, reads keyboard state through raw input, which reports the scan code and
//! ignores an injected virtual key with no scan code behind it. WASD sent as a bare virtual key
//! moves a text cursor and does not move a character.

use crate::commands::AppError;
use base64::Engine as _;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::time::Instant;
use tokio::sync::watch;
use tracing::Instrument as _;

/// PNG ceiling for one window capture, before base64. Captures are scaled down until they fit.
pub const WINDOW_CAPTURE_MAX_BYTES: usize = 1_500_000;
/// The capture stops shrinking here: below this a 1080p game window is unreadable anyway.
pub const WINDOW_CAPTURE_MIN_SCALE: f32 = 0.15;
/// Hard ceiling on one `WindowSession`, so a stuck observation loop cannot run forever.
pub const WINDOW_SESSION_MAX_STEPS: u32 = 120;
/// Wall-clock ceiling on one `WindowSession`.
pub const WINDOW_SESSION_MAX_MS: u64 = 180_000;
/// Enumeration never returns more than this many windows, however many the desktop has.
pub const WINDOW_ENUMERATION_LIMIT: usize = 256;
/// Longest a `Hold` may keep keys down in one step.
pub const WINDOW_HOLD_MAX_MS: u64 = 5_000;
/// Most keys a single `Hold` may press together.
pub const WINDOW_HOLD_MAX_KEYS: usize = 6;
/// Longest `Text` input accepted in one step.
pub const WINDOW_INPUT_MAX_TEXT_CHARS: usize = 2_000;
/// Windows reports DPI against this baseline; `dpi_scale` is `dpi / 96`.
const BASELINE_DPI: f32 = 96.0;

/// Result of every window-targeted operation.
pub type Result<T> = std::result::Result<T, WindowError>;

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

/// A typed failure with an actionable hint (R1). Nothing here degrades into a desktop-wide
/// action: an unsupported platform is an error, not a silent fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WindowError {
    /// The platform has no window bridge. Never a fallback to the desktop-wide path.
    Unsupported { operation: String },
    /// Enumeration ran but matched nothing.
    NotFound { filter: String },
    /// The window existed when it was found and is gone now.
    WindowClosed { hwnd: u64 },
    /// Windows refused to bring the window forward — usually a foreground lock.
    FocusRefused { hwnd: u64 },
    /// A coordinate fell outside the target window's client area.
    OutsideWindow { x: i32, y: i32, rect: WindowRect },
    /// A key name matched neither the Godot nor the crate spelling.
    UnknownKey {
        key: String,
        nearest: Option<String>,
    },
    /// The request itself is malformed — too much text, too long a hold, an empty capture.
    Invalid { message: String, hint: String },
    /// The session spent its step budget.
    StepLimit { limit: u32 },
    /// The session spent its wall-clock budget.
    TimeLimit { limit_ms: u64 },
    /// The emergency stop (or the caller's own stop signal) fired.
    Stopped,
    /// The PowerShell shim failed or answered with something unreadable.
    Bridge { detail: String },
}

impl WindowError {
    /// What to do about it. Every variant has an answer; that is the point of the type.
    #[must_use]
    pub fn hint(&self) -> String {
        match self {
            Self::Unsupported { .. } => {
                "Window capture and input are Windows-only. Run the game on Windows.".to_owned()
            }
            Self::NotFound { .. } => {
                "Start the game and widen the filter — a Godot window has class `Engine` and the project name as its title.".to_owned()
            }
            Self::WindowClosed { .. } => {
                "The game window closed. Find the window again before sending more input.".to_owned()
            }
            Self::FocusRefused { .. } => {
                "Click the game window once so Windows releases the foreground lock, then retry.".to_owned()
            }
            Self::OutsideWindow { rect, .. } => format!(
                "Aim inside the window: client coordinates run from (0, 0) to ({}, {}).",
                rect.width.saturating_sub(1),
                rect.height.saturating_sub(1)
            ),
            Self::UnknownKey { nearest, .. } => match nearest {
                Some(nearest) => format!("Did you mean `{nearest}`? Godot `KEY_W` spellings work too."),
                None => "Use a Godot name such as `KEY_W`, or a plain name such as `w`, `space` or `escape`.".to_owned(),
            },
            Self::Invalid { hint, .. } => hint.clone(),
            Self::StepLimit { .. } => {
                "Start a new session to keep watching; one session is deliberately bounded.".to_owned()
            }
            Self::TimeLimit { .. } => {
                "Start a new session to keep watching; one session is deliberately bounded.".to_owned()
            }
            Self::Stopped => "Ask again to resume; the stop signal ends the session.".to_owned(),
            Self::Bridge { .. } => {
                "Retry once. If it persists, check that PowerShell and .NET Desktop are available."
                    .to_owned()
            }
        }
    }
}

impl std::fmt::Display for WindowError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { operation } => {
                write!(formatter, "{operation} is not supported on this platform.")
            }
            Self::NotFound { filter } => {
                write!(formatter, "No visible window matched {filter}.")
            }
            Self::WindowClosed { hwnd } => {
                write!(formatter, "Window {hwnd} is no longer open.")
            }
            Self::FocusRefused { hwnd } => {
                write!(formatter, "Windows refused to focus window {hwnd}.")
            }
            Self::OutsideWindow { x, y, rect } => write!(
                formatter,
                "Client coordinate ({x}, {y}) is outside the window's {}x{} client area.",
                rect.width, rect.height
            ),
            Self::UnknownKey { key, .. } => write!(formatter, "Unsupported key name: {key}."),
            Self::Invalid { message, .. } => write!(formatter, "{message}"),
            Self::StepLimit { limit } => {
                write!(formatter, "This session already ran its {limit} steps.")
            }
            Self::TimeLimit { limit_ms } => write!(
                formatter,
                "This session already ran for its {limit_ms} ms budget."
            ),
            Self::Stopped => write!(formatter, "The session was stopped."),
            Self::Bridge { detail } => {
                write!(formatter, "The Windows window bridge failed: {detail}")
            }
        }
    }
}

impl std::error::Error for WindowError {}

impl From<WindowError> for AppError {
    fn from(error: WindowError) -> Self {
        Self {
            hint: Some(error.hint()),
            message: error.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------------------------

/// A rectangle in physical screen pixels.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl WindowRect {
    /// Screen-pixel containment, computed in `i64` so a rect at the right edge cannot overflow.
    #[must_use]
    pub fn contains(self, x: i32, y: i32) -> bool {
        let right = i64::from(self.x) + i64::from(self.width);
        let bottom = i64::from(self.y) + i64::from(self.height);
        i64::from(x) >= i64::from(self.x)
            && i64::from(x) < right
            && i64::from(y) >= i64::from(self.y)
            && i64::from(y) < bottom
    }
}

/// One top-level window, as the bridge saw it.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, Type)]
pub struct WindowRef {
    pub hwnd: u64,
    pub title: String,
    /// Window class. Godot's game window is `Engine`; the editor is `Engine` too, so the title
    /// is what tells them apart.
    pub class_name: String,
    pub process_id: u32,
    /// The client area in screen pixels — exactly what `capture_window` returns.
    pub rect: WindowRect,
    /// DWM extended frame bounds: the whole window, shadow excluded.
    pub frame: WindowRect,
    /// `dpi / 96`. Logical client coordinates are multiplied by this to reach physical pixels.
    pub dpi_scale: f32,
}

impl WindowRef {
    /// The client area in the logical coordinates `WindowInput` speaks.
    #[must_use]
    pub fn logical_size(&self) -> (u32, u32) {
        let scale = self.effective_scale();
        let width = (self.rect.width as f32 / scale).round().max(0.0);
        let height = (self.rect.height as f32 / scale).round().max(0.0);
        (width as u32, height as u32)
    }

    /// A zero or negative scale would divide the coordinate space by zero; treat it as 1:1.
    fn effective_scale(&self) -> f32 {
        if self.dpi_scale.is_finite() && self.dpi_scale > 0.0 {
            self.dpi_scale
        } else {
            1.0
        }
    }

    /// Logical client coordinates to physical screen coordinates. Refuses anything outside the
    /// client area rather than clamping it onto the window's edge.
    pub fn client_to_screen(&self, x: i32, y: i32) -> Result<(i32, i32)> {
        let scale = self.effective_scale();
        let physical_x = (x as f32 * scale).round() as i64;
        let physical_y = (y as f32 * scale).round() as i64;
        let outside = || WindowError::OutsideWindow {
            x,
            y,
            rect: WindowRect {
                x: 0,
                y: 0,
                width: self.logical_size().0,
                height: self.logical_size().1,
            },
        };
        if physical_x < 0
            || physical_y < 0
            || physical_x >= i64::from(self.rect.width)
            || physical_y >= i64::from(self.rect.height)
        {
            return Err(outside());
        }
        let screen_x = i64::from(self.rect.x) + physical_x;
        let screen_y = i64::from(self.rect.y) + physical_y;
        match (i32::try_from(screen_x), i32::try_from(screen_y)) {
            (Ok(screen_x), Ok(screen_y)) => Ok((screen_x, screen_y)),
            _ => Err(outside()),
        }
    }
}

/// What to look for. An absent field matches everything; matching is case-insensitive substring.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct WindowFilter {
    pub title_contains: Option<String>,
    pub process_id: Option<u32>,
    pub class_contains: Option<String>,
}

impl WindowFilter {
    #[must_use]
    pub fn matches(&self, window: &WindowRef) -> bool {
        let contains = |haystack: &str, needle: &Option<String>| match needle {
            Some(needle) => haystack
                .to_lowercase()
                .contains(&needle.trim().to_lowercase()),
            None => true,
        };
        contains(&window.title, &self.title_contains)
            && contains(&window.class_name, &self.class_contains)
            && self
                .process_id
                .is_none_or(|process_id| process_id == window.process_id)
    }

    /// Rendered for the `NotFound` error, so the model can see what it actually asked for.
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(title) = &self.title_contains {
            parts.push(format!("title containing `{title}`"));
        }
        if let Some(class) = &self.class_contains {
            parts.push(format!("class containing `{class}`"));
        }
        if let Some(process_id) = self.process_id {
            parts.push(format!("process {process_id}"));
        }
        if parts.is_empty() {
            "any visible window".to_owned()
        } else {
            parts.join(" and ")
        }
    }
}

/// How the window was captured. Recorded because the two paths behave differently: a
/// `PrintWindow` capture works on an occluded window, a screen copy does not.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WindowCaptureMethod {
    /// `PrintWindow` with `PW_RENDERFULLCONTENT`: works even when the window is behind another.
    PrintWindow,
    /// `CopyFromScreen` over the client rect after focusing: the fallback when `PrintWindow`
    /// returns an empty frame, which GPU-composited windows sometimes do.
    ScreenCopy,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CaptureOptions {
    /// Starting scale; the capture shrinks further if it does not fit `max_bytes`.
    pub scale: Option<f32>,
    pub max_bytes: usize,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            scale: None,
            max_bytes: WINDOW_CAPTURE_MAX_BYTES,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct WindowCapture {
    pub png_base64: String,
    pub width: u32,
    pub height: u32,
    /// Physical client pixels per captured pixel. 1.0 means the capture is unscaled.
    pub scale: f32,
    pub method: WindowCaptureMethod,
    pub window: WindowRef,
    pub captured_at: chrono::DateTime<Utc>,
}

impl WindowCapture {
    /// Maps a coordinate read off this image back to the logical client coordinates
    /// `WindowInput` takes. Without it a scaled-down capture would aim the pointer short.
    #[must_use]
    pub fn to_client(&self, x: i32, y: i32) -> (i32, i32) {
        let capture_scale = if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        };
        let dpi_scale = self.window.effective_scale();
        let divisor = capture_scale * dpi_scale;
        (
            (x as f32 / divisor).round() as i32,
            (y as f32 / divisor).round() as i32,
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WindowMouseButton {
    Left,
    Right,
    Middle,
}

impl WindowMouseButton {
    const fn code(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Middle => 2,
        }
    }
}

/// One input step aimed at the target window. Coordinates are logical client coordinates.
///
/// Struct variants, not newtypes: serde's internally tagged representation cannot carry a
/// newtype variant wrapping a string, and `ComputerAction` next door is shaped the same way.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WindowInput {
    KeyDown {
        key: KeyName,
    },
    KeyUp {
        key: KeyName,
    },
    KeyTap {
        key: KeyName,
    },
    Text {
        text: String,
    },
    MouseMove {
        x: i32,
        y: i32,
    },
    Click {
        x: i32,
        y: i32,
        button: WindowMouseButton,
    },
    /// Holds every key down for `frames_ms` — how a character walks in a game.
    Hold {
        keys: Vec<KeyName>,
        frames_ms: u64,
    },
}

impl WindowInput {
    /// Everything checkable without touching Windows: key names, sizes, and containment against
    /// the target window. Called before any bridge invocation, so a bad request costs nothing.
    pub fn validate(&self, window: &WindowRef) -> Result<()> {
        self.validate_shape()?;
        match self {
            Self::MouseMove { x, y } | Self::Click { x, y, .. } => {
                window.client_to_screen(*x, *y).map(|_| ())
            }
            _ => Ok(()),
        }
    }

    /// The half of [`validate`](Self::validate) that does not need a window: key names, text
    /// length, hold caps.
    ///
    /// A plan is checked before anything launches (GAD-097), and at that moment there is no
    /// window to measure a coordinate against — but a misspelled key or a ten-second hold is
    /// wrong whatever the window turns out to be, and finding it out before spawning Godot is
    /// the difference between a typed refusal and a game that opens only to be killed.
    pub fn validate_shape(&self) -> Result<()> {
        match self {
            Self::KeyDown { key } | Self::KeyUp { key } | Self::KeyTap { key } => {
                key.resolve().map(|_| ())
            }
            Self::Text { text } => {
                if text.is_empty() {
                    return Err(WindowError::Invalid {
                        message: "Text input needs at least one character.".to_owned(),
                        hint: "Send the characters you want typed into the window.".to_owned(),
                    });
                }
                if text.chars().count() > WINDOW_INPUT_MAX_TEXT_CHARS {
                    return Err(WindowError::Invalid {
                        message: format!(
                            "Text input exceeds the {WINDOW_INPUT_MAX_TEXT_CHARS}-character limit."
                        ),
                        hint: "Split the text across several steps.".to_owned(),
                    });
                }
                Ok(())
            }
            // Coordinates are the window's business; `validate` adds that check.
            Self::MouseMove { .. } | Self::Click { .. } => Ok(()),
            Self::Hold { keys, frames_ms } => {
                if keys.is_empty() || keys.len() > WINDOW_HOLD_MAX_KEYS {
                    return Err(WindowError::Invalid {
                        message: format!(
                            "A hold needs 1 to {WINDOW_HOLD_MAX_KEYS} keys, not {}.",
                            keys.len()
                        ),
                        hint: "Hold the movement keys the character needs, nothing more."
                            .to_owned(),
                    });
                }
                if *frames_ms == 0 || *frames_ms > WINDOW_HOLD_MAX_MS {
                    return Err(WindowError::Invalid {
                        message: format!(
                            "A hold must last 1 to {WINDOW_HOLD_MAX_MS} ms, not {frames_ms} ms."
                        ),
                        hint: "At 60 Hz, 1000 ms is about 60 frames of held input.".to_owned(),
                    });
                }
                for key in keys {
                    key.resolve()?;
                }
                Ok(())
            }
        }
    }

    /// Short label for tracing and for the step result.
    const fn label(&self) -> &'static str {
        match self {
            Self::KeyDown { .. } => "key_down",
            Self::KeyUp { .. } => "key_up",
            Self::KeyTap { .. } => "key_tap",
            Self::Text { .. } => "text",
            Self::MouseMove { .. } => "mouse_move",
            Self::Click { .. } => "click",
            Self::Hold { .. } => "hold",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Key names
// ---------------------------------------------------------------------------------------------

/// A key, spelled either the Godot way (`KEY_W`, `KEY_PAGEUP`) or the way `computer.rs` already
/// spells keys (`w`, `pageup`). Both resolve to the same virtual-key code.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(transparent)]
pub struct KeyName(String);

impl KeyName {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The Windows virtual-key code, or an error naming the closest spelling we do know.
    pub fn resolve(&self) -> Result<u8> {
        let trimmed = self.0.trim();
        let lower = trimmed.to_ascii_lowercase();
        let bare = lower.strip_prefix("key_").unwrap_or(&lower);
        if let Some(code) = godot_only_key(bare) {
            return Ok(code);
        }
        // `computer.rs` already knows the crate's own spellings, letters and digits; a Godot
        // name reduces to one of those once `KEY_` is stripped.
        if let Some(code) = crate::computer::virtual_key(bare) {
            return Ok(code);
        }
        Err(WindowError::UnknownKey {
            key: trimmed.to_owned(),
            nearest: nearest_key_name(bare),
        })
    }
}

impl From<&str> for KeyName {
    fn from(name: &str) -> Self {
        Self(name.to_owned())
    }
}

/// Keys Godot names that `computer.rs` does not: the keypad and the punctuation row.
fn godot_only_key(name: &str) -> Option<u8> {
    Some(match name {
        "kp_0" => 0x60,
        "kp_1" => 0x61,
        "kp_2" => 0x62,
        "kp_3" => 0x63,
        "kp_4" => 0x64,
        "kp_5" => 0x65,
        "kp_6" => 0x66,
        "kp_7" => 0x67,
        "kp_8" => 0x68,
        "kp_9" => 0x69,
        "kp_multiply" => 0x6A,
        "kp_add" => 0x6B,
        "kp_subtract" => 0x6D,
        "kp_period" => 0x6E,
        "kp_divide" => 0x6F,
        "kp_enter" => 0x0D,
        "capslock" => 0x14,
        "numlock" => 0x90,
        "scrolllock" => 0x91,
        "pause" => 0x13,
        "print" | "printscreen" => 0x2C,
        "menu" => 0x5D,
        "quoteleft" | "backtick" => 0xC0,
        "minus" => 0xBD,
        "equal" => 0xBB,
        "bracketleft" => 0xDB,
        "bracketright" => 0xDD,
        "backslash" => 0xDC,
        "semicolon" => 0xBA,
        "apostrophe" => 0xDE,
        "comma" => 0xBC,
        "period" => 0xBE,
        "slash" => 0xBF,
        _ => return None,
    })
}

/// Every spelling `resolve` accepts by name, for the "did you mean" hint.
const KNOWN_KEY_NAMES: &[&str] = &[
    "backspace",
    "tab",
    "enter",
    "return",
    "shift",
    "ctrl",
    "control",
    "alt",
    "escape",
    "esc",
    "space",
    "pageup",
    "pagedown",
    "end",
    "home",
    "left",
    "up",
    "right",
    "down",
    "insert",
    "delete",
    "win",
    "meta",
    "f1",
    "f2",
    "f3",
    "f4",
    "f5",
    "f6",
    "f7",
    "f8",
    "f9",
    "f10",
    "f11",
    "f12",
    "capslock",
    "numlock",
    "scrolllock",
    "pause",
    "printscreen",
    "menu",
    "quoteleft",
    "minus",
    "equal",
    "bracketleft",
    "bracketright",
    "backslash",
    "semicolon",
    "apostrophe",
    "comma",
    "period",
    "slash",
    "kp_0",
    "kp_enter",
    "kp_add",
    "kp_subtract",
];

/// Closest known spelling within a small edit distance, so a typo gets a name back instead of
/// a shrug. Single characters are excluded: every letter is within distance 1 of every other.
fn nearest_key_name(name: &str) -> Option<String> {
    if name.chars().count() < 2 {
        return None;
    }
    let mut best: Option<(usize, &str)> = None;
    for candidate in KNOWN_KEY_NAMES {
        let distance = edit_distance(name, candidate);
        if best.is_none_or(|(current, _)| distance < current) {
            best = Some((distance, candidate));
        }
    }
    best.filter(|(distance, _)| *distance <= 3)
        .map(|(_, candidate)| candidate.to_owned())
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0_usize; right.len() + 1];
    for (row, left_char) in left.iter().enumerate() {
        current[0] = row + 1;
        for (column, right_char) in right.iter().enumerate() {
            let substitution = usize::from(left_char != right_char);
            current[column + 1] = (previous[column] + substitution)
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

// ---------------------------------------------------------------------------------------------
// Enumeration
// ---------------------------------------------------------------------------------------------

/// One line of the bridge's JSON-lines output. Titles and class names travel base64-encoded:
/// PowerShell's stdout is read through the console code page, which mangles anything outside
/// it, and a game window titled with an em dash is not an error worth having.
#[derive(Debug, Deserialize)]
struct RawWindow {
    hwnd: i64,
    pid: u32,
    title_b64: String,
    class_b64: String,
    /// DWM extended frame bounds.
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    /// Client area in screen pixels.
    cx: i32,
    cy: i32,
    cw: i32,
    ch: i32,
    dpi: u32,
}

/// Parses the bridge's JSON lines. Lines that are not JSON objects are ignored — PowerShell
/// warnings share the pipe — but an object we cannot read is a bridge fault, not noise.
fn parse_window_lines(output: &str) -> Result<Vec<WindowRef>> {
    let mut windows = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let Ok(raw) = serde_json::from_str::<RawWindow>(line) else {
            continue;
        };
        if raw.cw <= 0 || raw.ch <= 0 {
            // A window with no client area cannot be watched or clicked.
            continue;
        }
        let hwnd = u64::try_from(raw.hwnd).map_err(|_| WindowError::Bridge {
            detail: format!("window handle {} is not a valid handle", raw.hwnd),
        })?;
        let dpi = if raw.dpi == 0 { 96 } else { raw.dpi };
        windows.push(WindowRef {
            hwnd,
            title: decode_base64_utf8(&raw.title_b64, "window title")?,
            class_name: decode_base64_utf8(&raw.class_b64, "window class")?,
            process_id: raw.pid,
            rect: WindowRect {
                x: raw.cx,
                y: raw.cy,
                width: raw.cw.unsigned_abs(),
                height: raw.ch.unsigned_abs(),
            },
            frame: WindowRect {
                x: raw.x,
                y: raw.y,
                width: raw.w.max(0).unsigned_abs(),
                height: raw.h.max(0).unsigned_abs(),
            },
            dpi_scale: dpi as f32 / BASELINE_DPI,
        });
        if windows.len() >= WINDOW_ENUMERATION_LIMIT {
            break;
        }
    }
    Ok(windows)
}

fn decode_base64_utf8(value: &str, field: &str) -> Result<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|error| WindowError::Bridge {
            detail: format!("{field} was not valid base64: {error}"),
        })?;
    String::from_utf8(bytes).map_err(|error| WindowError::Bridge {
        detail: format!("{field} was not valid UTF-8: {error}"),
    })
}

/// Every visible top-level window matching the filter, measured first-hand by Windows.
///
/// Minimised windows are listed — a game that is minimised still needs finding — but Windows
/// parks them at its `(-32000, -32000)` sentinel, so their rect is not somewhere to click.
/// [`focus_window`] restores them; re-find the window afterwards to pick up its real rect.
/// Cloaked windows (suspended UWP apps, hidden virtual-desktop windows) are left out entirely.
pub async fn find_windows(filter: WindowFilter) -> Result<Vec<WindowRef>> {
    let span = tracing::info_span!("computer_window.find", filter = %filter.describe());
    async move {
        #[cfg(windows)]
        {
            let output = run_bridge("[BhippiWindowBridge]::List()").await?;
            let windows: Vec<WindowRef> = parse_window_lines(&output)?
                .into_iter()
                .filter(|window| filter.matches(window))
                .collect();
            tracing::debug!(matched = windows.len(), "window enumeration finished");
            Ok(windows)
        }
        #[cfg(not(windows))]
        {
            let _unused = &filter;
            Err(WindowError::Unsupported {
                operation: "Window enumeration".to_owned(),
            })
        }
    }
    .instrument(span)
    .await
}

/// The single best match, or a typed `NotFound` naming what was searched for. A game is one
/// window; asking for "the" window is the common case and deserves not to be a `Vec` dance.
pub async fn find_window(filter: WindowFilter) -> Result<WindowRef> {
    let described = filter.describe();
    let mut windows = find_windows(filter).await?;
    if windows.is_empty() {
        return Err(WindowError::NotFound { filter: described });
    }
    // Largest client area wins: a game's splash or tooltip window is never the biggest one.
    windows.sort_by_key(|window| {
        std::cmp::Reverse(u64::from(window.rect.width) * u64::from(window.rect.height))
    });
    windows
        .into_iter()
        .next()
        .ok_or(WindowError::NotFound { filter: described })
}

/// Re-reads one window by handle. The rect is live data — a game window can be moved or resized
/// between two steps — so anything that acts on a window refreshes it first. A window that is
/// gone comes back as `WindowClosed`, which is what stops a session rather than letting input
/// land wherever the window used to be.
pub async fn refresh_window(window: &WindowRef) -> Result<WindowRef> {
    let hwnd = window.hwnd;
    let span = tracing::info_span!("computer_window.refresh", hwnd);
    async move {
        #[cfg(windows)]
        {
            let prelude = format!("$Hwnd = {}\n", hwnd_literal(hwnd)?);
            let output = run_bridge_with(&prelude, "[BhippiWindowBridge]::Describe($Hwnd)").await?;
            let mut found = parse_window_lines(&output)?;
            if found.is_empty() {
                return Err(bridge_status(&output, hwnd));
            }
            Ok(found.swap_remove(0))
        }
        #[cfg(not(windows))]
        {
            Err(WindowError::Unsupported {
                operation: "Window lookup".to_owned(),
            })
        }
    }
    .instrument(span)
    .await
}

// ---------------------------------------------------------------------------------------------
// Capture, focus, input
// ---------------------------------------------------------------------------------------------

/// One line of the capture bridge's output.
#[derive(Debug, Deserialize)]
struct RawCapture {
    method: String,
    width: u32,
    height: u32,
    scale: f32,
    png_b64: String,
}

fn parse_capture_line(output: &str, window: &WindowRef) -> Result<WindowCapture> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with('{'))
        .ok_or_else(|| bridge_status(output, window.hwnd))?;
    let raw: RawCapture = serde_json::from_str(line).map_err(|error| WindowError::Bridge {
        detail: format!("the capture payload was unreadable: {error}"),
    })?;
    if raw.png_b64.is_empty() || raw.width == 0 || raw.height == 0 {
        return Err(WindowError::Invalid {
            message: "The window produced an empty capture.".to_owned(),
            hint: "Make sure the window is not minimised, then capture again.".to_owned(),
        });
    }
    let method = match raw.method.as_str() {
        "print_window" => WindowCaptureMethod::PrintWindow,
        "screen_copy" => WindowCaptureMethod::ScreenCopy,
        other => {
            return Err(WindowError::Bridge {
                detail: format!("unknown capture method `{other}`"),
            })
        }
    };
    Ok(WindowCapture {
        png_base64: raw.png_b64,
        width: raw.width,
        height: raw.height,
        scale: raw.scale,
        method,
        window: window.clone(),
        captured_at: Utc::now(),
    })
}

/// Maps the bridge's short status words onto typed errors, so a closed window reads as
/// `WindowClosed` rather than as an unexplained parse failure.
fn bridge_status(output: &str, hwnd: u64) -> WindowError {
    let status = output.trim();
    if let Some(rest) = status.strip_prefix("ERR|") {
        let mut parts = rest.splitn(2, '|');
        let code = parts.next().unwrap_or_default();
        let detail = parts.next().unwrap_or_default().to_owned();
        return match code {
            "closed" => WindowError::WindowClosed { hwnd },
            "focus" => WindowError::FocusRefused { hwnd },
            "outside" => WindowError::Invalid {
                message: "The window moved and the target point is no longer inside it.".to_owned(),
                hint: "Find the window again to pick up its new position.".to_owned(),
            },
            "empty" => WindowError::Invalid {
                message: "The window has no client area to capture.".to_owned(),
                hint: "Restore the window from the taskbar and try again.".to_owned(),
            },
            _ => WindowError::Bridge { detail },
        };
    }
    WindowError::Bridge {
        detail: if status.is_empty() {
            "the bridge returned nothing".to_owned()
        } else {
            status.to_owned()
        },
    }
}

/// Captures the window's client area — and only that. Tries `PrintWindow` with
/// `PW_RENDERFULLCONTENT` first so an occluded window still yields a frame; falls back to a
/// focused screen copy when that comes back empty, and records which path ran.
pub async fn capture_window(window: &WindowRef, options: CaptureOptions) -> Result<WindowCapture> {
    let span = tracing::info_span!(
        "computer_window.capture",
        hwnd = window.hwnd,
        max_bytes = options.max_bytes
    );
    async move {
        if options.max_bytes == 0 {
            return Err(WindowError::Invalid {
                message: "A capture needs a non-zero byte budget.".to_owned(),
                hint: format!("Use CaptureOptions::default() for {WINDOW_CAPTURE_MAX_BYTES}."),
            });
        }
        #[cfg(windows)]
        {
            let scale = options.scale.unwrap_or(1.0);
            if !scale.is_finite() || scale <= 0.0 || scale > 1.0 {
                return Err(WindowError::Invalid {
                    message: format!("A capture scale of {scale} is out of range."),
                    hint: "Use a scale above 0 and at most 1.0, or leave it unset.".to_owned(),
                });
            }
            let max_bytes = options.max_bytes.min(WINDOW_CAPTURE_MAX_BYTES);
            let prelude = format!(
                "$Hwnd = {}\n$MaxBytes = {max_bytes}\n$Scale = {scale:.4}\n",
                hwnd_literal(window.hwnd)?
            );
            let output = run_bridge_with(
                &prelude,
                "[BhippiWindowBridge]::Capture($Hwnd, $MaxBytes, $Scale)",
            )
            .await?;
            let capture = parse_capture_line(&output, window)?;
            tracing::info!(
                width = capture.width,
                height = capture.height,
                method = ?capture.method,
                "window captured"
            );
            Ok(capture)
        }
        #[cfg(not(windows))]
        {
            let _unused = window;
            Err(WindowError::Unsupported {
                operation: "Window capture".to_owned(),
            })
        }
    }
    .instrument(span)
    .await
}

/// Brings the window forward and verifies with `GetForegroundWindow` that Windows agreed.
pub async fn focus_window(window: &WindowRef) -> Result<()> {
    let span = tracing::info_span!("computer_window.focus", hwnd = window.hwnd);
    async move {
        #[cfg(windows)]
        {
            let prelude = format!("$Hwnd = {}\n", hwnd_literal(window.hwnd)?);
            let output =
                run_bridge_with(&prelude, "[BhippiWindowBridge]::FocusOnly($Hwnd)").await?;
            if output.trim() == "OK" {
                Ok(())
            } else {
                Err(bridge_status(&output, window.hwnd))
            }
        }
        #[cfg(not(windows))]
        {
            let _unused = window;
            Err(WindowError::Unsupported {
                operation: "Window focus".to_owned(),
            })
        }
    }
    .instrument(span)
    .await
}

/// Focuses the window, then injects one input into it. Coordinates are translated here, in
/// Rust, against the window's own client rect; the bridge re-checks the live rect before it
/// acts, so a window that moved between the two refuses the click instead of hitting whatever
/// is now underneath.
pub async fn send_input_to_window(window: &WindowRef, input: WindowInput) -> Result<()> {
    let span = tracing::info_span!(
        "computer_window.input",
        hwnd = window.hwnd,
        input = input.label()
    );
    async move {
        input.validate(window)?;
        #[cfg(windows)]
        {
            let prelude = input_prelude(window, &input)?;
            let output = run_bridge_with(
                &prelude,
                "[BhippiWindowBridge]::Input($Hwnd, $Op, [int[]]$Vks, $HasPoint, $X, $Y, $Button, $HoldMs, $TextB64)",
            )
            .await?;
            if output.trim() == "OK" {
                tracing::debug!("window input delivered");
                Ok(())
            } else {
                Err(bridge_status(&output, window.hwnd))
            }
        }
        #[cfg(not(windows))]
        {
            Err(WindowError::Unsupported {
                operation: "Window input".to_owned(),
            })
        }
    }
    .instrument(span)
    .await
}

/// Asks the window to close the way its ✕ does, by posting `WM_CLOSE`.
///
/// This is not a nicety. Killing the process stops the game between two frames, so nothing in
/// it gets to run a shutdown path; a `WM_CLOSE` reaches Godot as `NOTIFICATION_WM_CLOSE_REQUEST`,
/// which is what makes the playtest probe write its final `{"done": true}` line. A telemetry
/// file without that line cannot say whether the samples simply stopped or the game did.
///
/// Posting is asynchronous: the window may still be open when this returns, and a game that
/// ignores the request stays open. The caller keeps its process handle as the backstop.
pub async fn request_window_close(window: &WindowRef) -> Result<()> {
    let span = tracing::info_span!("computer_window.close", hwnd = window.hwnd);
    async move {
        #[cfg(windows)]
        {
            let prelude = format!("$Hwnd = {}\n", hwnd_literal(window.hwnd)?);
            let output = run_bridge_with(&prelude, "[BhippiWindowBridge]::Close($Hwnd)").await?;
            if output.trim() == "OK" {
                Ok(())
            } else {
                Err(bridge_status(&output, window.hwnd))
            }
        }
        #[cfg(not(windows))]
        {
            let _unused = window;
            Err(WindowError::Unsupported {
                operation: "Window close".to_owned(),
            })
        }
    }
    .instrument(span)
    .await
}

/// Builds the numeric/base64 prelude the bridge script reads. Only integers, a fixed operation
/// word and base64 ever reach PowerShell, so no caller string is ever interpolated into code.
#[cfg(windows)]
fn input_prelude(window: &WindowRef, input: &WindowInput) -> Result<String> {
    let operation: &str;
    let mut keys: Vec<u8> = Vec::new();
    let mut point: Option<(i32, i32)> = None;
    let mut button = 0_u8;
    let mut hold_ms = 0_u64;
    let mut text_b64 = String::new();
    match input {
        WindowInput::KeyDown { key } => {
            operation = "key_down";
            keys.push(key.resolve()?);
        }
        WindowInput::KeyUp { key } => {
            operation = "key_up";
            keys.push(key.resolve()?);
        }
        WindowInput::KeyTap { key } => {
            operation = "key_tap";
            keys.push(key.resolve()?);
        }
        WindowInput::Text { text } => {
            operation = "text";
            text_b64 = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
        }
        WindowInput::MouseMove { x, y } => {
            operation = "mouse_move";
            point = Some(window.client_to_screen(*x, *y)?);
        }
        WindowInput::Click {
            x,
            y,
            button: requested,
        } => {
            operation = "click";
            point = Some(window.client_to_screen(*x, *y)?);
            button = requested.code();
        }
        WindowInput::Hold {
            keys: held,
            frames_ms,
        } => {
            operation = "hold";
            for key in held {
                keys.push(key.resolve()?);
            }
            hold_ms = *frames_ms;
        }
    }
    let codes = if keys.is_empty() {
        "@()".to_owned()
    } else {
        format!(
            "@({})",
            keys.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
        )
    };
    let (has_point, x, y) = match point {
        Some((x, y)) => (1, x, y),
        None => (0, 0, 0),
    };
    Ok(format!(
        "$Hwnd = {hwnd}\n$Op = '{operation}'\n$Vks = {codes}\n$HasPoint = {has_point}\n$X = {x}\n$Y = {y}\n$Button = {button}\n$HoldMs = {hold_ms}\n$TextB64 = '{text_b64}'\n",
        hwnd = hwnd_literal(window.hwnd)?
    ))
}

/// Window handles cross into PowerShell as a signed 64-bit literal, which is what `IntPtr`
/// takes. A handle that will not fit is a bridge fault, not something to truncate.
#[cfg(windows)]
fn hwnd_literal(hwnd: u64) -> Result<i64> {
    i64::try_from(hwnd).map_err(|_| WindowError::Bridge {
        detail: format!("window handle {hwnd} does not fit a native handle"),
    })
}

// ---------------------------------------------------------------------------------------------
// Scope — the boundary INV-089's last row is about
// ---------------------------------------------------------------------------------------------

/// Which pixels a Computer Use capture is allowed to cover (GAD-012, INV-089).
///
/// Bhippi has exactly two capture scopes and they must not leak into each other. `Desktop` is
/// the whole virtual screen and belongs to the explicit, user-initiated flow in
/// [`crate::computer`]; `Window` is one window Bhippi launched and is the only scope an engine
/// observation may ever have. The enum exists so the difference is a value the code carries
/// rather than a convention each author has to remember.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum CaptureScope {
    /// Every monitor. Reached only from `/computer` and the composer toggle — never from a
    /// build run, a playtest or a plan approval.
    Desktop,
    /// One window, addressed by handle.
    Window { window: WindowRef },
}

impl CaptureScope {
    #[must_use]
    pub const fn is_desktop(&self) -> bool {
        matches!(self, Self::Desktop)
    }

    /// The window this scope is bound to, or `None` for the desktop.
    #[must_use]
    pub const fn window(&self) -> Option<&WindowRef> {
        match self {
            Self::Desktop => None,
            Self::Window { window } => Some(window),
        }
    }

    /// For a log line or a fault message.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Desktop => "the whole desktop".to_owned(),
            Self::Window { window } => {
                format!("window {} (`{}`)", window.hwnd, window.title)
            }
        }
    }
}

/// A capture scope that **cannot** be [`CaptureScope::Desktop`].
///
/// INV-089's last row — "there is no code path from an engine observation to a desktop-wide
/// capture" — is easy to state and easy to violate by accident, because the desktop entry
/// points in [`crate::computer`] are one `use` away. This type is how the rule is enforced
/// instead of remembered: an engine observation takes an `EngineCaptureScope`, the only
/// constructor takes a [`WindowRef`], and every operation it offers is the window-targeted
/// one. There is no method here that widens, and [`Self::scope`] can only ever answer
/// `Window`.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineCaptureScope {
    window: WindowRef,
}

impl EngineCaptureScope {
    /// Bind the engine's eyes and hands to one window.
    #[must_use]
    pub const fn new(window: WindowRef) -> Self {
        Self { window }
    }

    #[must_use]
    pub const fn window(&self) -> &WindowRef {
        &self.window
    }

    /// Always `Window`. That is the whole point of the type.
    #[must_use]
    pub fn scope(&self) -> CaptureScope {
        CaptureScope::Window {
            window: self.window.clone(),
        }
    }

    /// Re-read the window by handle. A window that has gone comes back as `WindowClosed`,
    /// which is what ends an observation rather than letting input land on the desktop.
    pub async fn refresh(&mut self) -> Result<()> {
        self.window = refresh_window(&self.window).await?;
        Ok(())
    }

    pub async fn focus(&self) -> Result<()> {
        focus_window(&self.window).await
    }

    pub async fn capture(&self, options: CaptureOptions) -> Result<WindowCapture> {
        capture_window(&self.window, options).await
    }

    pub async fn send(&self, input: WindowInput) -> Result<()> {
        send_input_to_window(&self.window, input).await
    }

    pub async fn request_close(&self) -> Result<()> {
        request_window_close(&self.window).await
    }
}

// ---------------------------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct StepResult {
    pub capture: Option<WindowCapture>,
    pub elapsed_ms: u64,
    /// 1-based index of this step within the session.
    pub step: u32,
}

/// A bounded watch-and-play loop over one window.
///
/// The emergency stop that `computer.rs` honours lives in `overlay.rs` and is generation-scoped
/// to an `OverlayGuard` that only `chat.rs` constructs; reaching it from here would mean editing
/// `chat.rs`. So the session takes a stop receiver instead: whoever owns the guard maps its
/// signal into a `watch::Receiver<bool>` and hands it over with [`WindowSession::with_stop_signal`].
/// A session without one is still bounded by its step and time caps.
pub struct WindowSession {
    window: WindowRef,
    started_at: Instant,
    steps: u32,
    max_steps: u32,
    max_ms: u64,
    capture_options: CaptureOptions,
    stop: Option<watch::Receiver<bool>>,
}

impl WindowSession {
    #[must_use]
    pub fn start(window: WindowRef) -> Self {
        Self {
            window,
            started_at: Instant::now(),
            steps: 0,
            max_steps: WINDOW_SESSION_MAX_STEPS,
            max_ms: WINDOW_SESSION_MAX_MS,
            capture_options: CaptureOptions::default(),
            stop: None,
        }
    }

    /// Tighter caps than the module's ceilings; never looser.
    #[must_use]
    pub fn with_limits(mut self, max_steps: u32, max_ms: u64) -> Self {
        self.max_steps = max_steps.min(WINDOW_SESSION_MAX_STEPS);
        self.max_ms = max_ms.min(WINDOW_SESSION_MAX_MS);
        self
    }

    /// Capture settings for every step that asks for one. A loop that watches often wants a
    /// smaller frame than a one-off screenshot does.
    #[must_use]
    pub fn with_capture_options(mut self, options: CaptureOptions) -> Self {
        self.capture_options = options;
        self
    }

    /// A stop signal the caller already owns. `true` ends the session at the next step.
    #[must_use]
    pub fn with_stop_signal(mut self, stop: watch::Receiver<bool>) -> Self {
        self.stop = Some(stop);
        self
    }

    #[must_use]
    pub const fn window(&self) -> &WindowRef {
        &self.window
    }

    #[must_use]
    pub const fn steps_taken(&self) -> u32 {
        self.steps
    }

    #[must_use]
    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Budget and stop signal, checked before any Windows call so an exhausted session costs
    /// nothing and answers the same way on every platform.
    fn check_budget(&self) -> Result<()> {
        if self.stop.as_ref().is_some_and(|stop| *stop.borrow()) {
            return Err(WindowError::Stopped);
        }
        if self.steps >= self.max_steps {
            return Err(WindowError::StepLimit {
                limit: self.max_steps,
            });
        }
        let elapsed = self.elapsed_ms();
        if elapsed >= self.max_ms {
            return Err(WindowError::TimeLimit {
                limit_ms: self.max_ms,
            });
        }
        Ok(())
    }

    /// One observation step: optionally send input, optionally capture, always inside budget.
    /// The window is re-read by handle first, so input never lands on stale coordinates and a
    /// closed game is reported as `WindowClosed` instead of clicking through to the desktop.
    pub async fn step(&mut self, input: Option<WindowInput>, capture: bool) -> Result<StepResult> {
        self.check_budget()?;
        if let Some(input) = &input {
            // Validate against the last known rect before spending a round trip on refreshing.
            input.validate(&self.window)?;
        }
        // The attempt is spent whether or not it succeeds: a step that keeps failing must run
        // the budget down rather than retry against the same wall forever.
        self.steps = self.steps.saturating_add(1);
        self.window = refresh_window(&self.window).await?;
        if let Some(input) = input {
            input.validate(&self.window)?;
            send_input_to_window(&self.window, input).await?;
        }
        let capture = if capture {
            Some(capture_window(&self.window, self.capture_options).await?)
        } else {
            None
        };
        Ok(StepResult {
            capture,
            elapsed_ms: self.elapsed_ms(),
            step: self.steps,
        })
    }
}

// ---------------------------------------------------------------------------------------------
// The PowerShell bridge
// ---------------------------------------------------------------------------------------------

/// One C# class serves enumeration, capture, focus and input. Each PowerShell invocation
/// compiles it once and calls a single entry point, so the three operations cannot drift apart
/// in how they read a window's geometry or which desktop they attach to.
///
/// Only C# 5 features are used: Windows PowerShell 5.1's `Add-Type` compiles with the CodeDom
/// C# 5 compiler, so no string interpolation, no `out var`, no local functions.
#[cfg(windows)]
const WINDOW_BRIDGE_CSHARP: &str = r#"
using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public class BhippiWindowBridge {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
  [StructLayout(LayoutKind.Sequential)]
  public struct POINT { public int X; public int Y; }
  [StructLayout(LayoutKind.Sequential)]
  public struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)]
  public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit)]
  public struct INPUTUNION { [FieldOffset(0)] public MOUSEINPUT mi; [FieldOffset(0)] public KEYBDINPUT ki; }
  [StructLayout(LayoutKind.Sequential)]
  public struct INPUT { public uint type; public INPUTUNION u; }

  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
  [DllImport("user32.dll", SetLastError=true)] public static extern IntPtr OpenDesktop(string name, uint flags, bool inherit, uint access);
  [DllImport("user32.dll", SetLastError=true)] public static extern bool SetThreadDesktop(IntPtr desktop);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr param);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="GetWindowTextW")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="GetClassNameW")] public static extern int GetClassName(IntPtr hWnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll", EntryPoint="GetWindowThreadProcessId")] public static extern uint GetWindowThread(IntPtr hWnd, IntPtr zero);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT point);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr hWnd, int attribute, out RECT value, int size);
  [DllImport("dwmapi.dll", EntryPoint="DwmGetWindowAttribute")] public static extern int DwmGetIntAttribute(IntPtr hWnd, int attribute, out int value, int size);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int command);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint attach, uint attachTo, bool doAttach);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr hWnd, uint flags);
  [DllImport("user32.dll")] public static extern IntPtr SetFocus(IntPtr hWnd);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
  [DllImport("user32.dll", SetLastError=true)] public static extern uint SendInput(uint count, INPUT[] inputs, int size);
  [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint mapType);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern bool PostMessage(IntPtr hWnd, uint message, IntPtr wParam, IntPtr lParam);

  const uint WM_CLOSE = 0x0010;
  const int DWMWA_EXTENDED_FRAME_BOUNDS = 9;
  const int DWMWA_CLOAKED = 14;
  const int SW_RESTORE = 9;
  const uint GA_ROOT = 2;
  const uint PW_RENDERFULLCONTENT = 0x00000002;
  const uint INPUT_MOUSE = 0;
  const uint INPUT_KEYBOARD = 1;
  const uint KEYEVENTF_EXTENDEDKEY = 0x0001;
  const uint KEYEVENTF_KEYUP = 0x0002;
  const uint KEYEVENTF_UNICODE = 0x0004;
  const uint KEYEVENTF_SCANCODE = 0x0008;

  // Attaching to the input desktop is what lets this run from a background process, exactly as
  // the desktop-wide bridge in computer.rs does.
  static void AttachToInputDesktop() {
    IntPtr desktop = OpenInputDesktop(0, false, 0x01FF);
    if (desktop == IntPtr.Zero) desktop = OpenDesktop("default", 0, false, 0x01FF);
    if (desktop != IntPtr.Zero) SetThreadDesktop(desktop);
  }

  static string Base64(string value) {
    if (value == null) value = "";
    return Convert.ToBase64String(Encoding.UTF8.GetBytes(value));
  }

  static uint DpiOf(IntPtr hWnd) {
    try { uint dpi = GetDpiForWindow(hWnd); if (dpi > 0) return dpi; } catch (Exception) { }
    return 96;
  }

  static bool FrameBounds(IntPtr hWnd, out RECT bounds) {
    bounds = new RECT();
    if (DwmGetWindowAttribute(hWnd, DWMWA_EXTENDED_FRAME_BOUNDS, out bounds, Marshal.SizeOf(typeof(RECT))) == 0) return true;
    return GetWindowRect(hWnd, out bounds);
  }

  static bool ClientBounds(IntPtr hWnd, out RECT bounds) {
    bounds = new RECT();
    RECT client;
    if (!GetClientRect(hWnd, out client)) return false;
    POINT origin = new POINT();
    origin.X = 0; origin.Y = 0;
    if (!ClientToScreen(hWnd, ref origin)) return false;
    bounds.Left = origin.X;
    bounds.Top = origin.Y;
    bounds.Right = origin.X + (client.Right - client.Left);
    bounds.Bottom = origin.Y + (client.Bottom - client.Top);
    return true;
  }

  static bool Cloaked(IntPtr hWnd) {
    int cloaked = 0;
    try { if (DwmGetIntAttribute(hWnd, DWMWA_CLOAKED, out cloaked, 4) == 0) return cloaked != 0; } catch (Exception) { }
    return false;
  }

  // One JSON line describing one window, or null when it has nothing to watch. Enumeration and
  // the single-handle lookup share it so a window can never be described two different ways.
  static string Line(IntPtr hWnd) {
    RECT frame;
    if (!FrameBounds(hWnd, out frame)) return null;
    RECT client;
    if (!ClientBounds(hWnd, out client)) return null;
    int clientWidth = client.Right - client.Left;
    int clientHeight = client.Bottom - client.Top;
    if (clientWidth <= 0 || clientHeight <= 0) return null;
    StringBuilder title = new StringBuilder(512);
    GetWindowText(hWnd, title, title.Capacity);
    StringBuilder className = new StringBuilder(256);
    GetClassName(hWnd, className, className.Capacity);
    uint processId = 0;
    GetWindowThreadProcessId(hWnd, out processId);
    StringBuilder output = new StringBuilder();
    output.Append("{\"hwnd\":");
    output.Append(hWnd.ToInt64());
    output.Append(",\"pid\":");
    output.Append(processId);
    output.Append(",\"title_b64\":\"");
    output.Append(Base64(title.ToString()));
    output.Append("\",\"class_b64\":\"");
    output.Append(Base64(className.ToString()));
    output.Append("\",\"x\":");
    output.Append(frame.Left);
    output.Append(",\"y\":");
    output.Append(frame.Top);
    output.Append(",\"w\":");
    output.Append(frame.Right - frame.Left);
    output.Append(",\"h\":");
    output.Append(frame.Bottom - frame.Top);
    output.Append(",\"cx\":");
    output.Append(client.Left);
    output.Append(",\"cy\":");
    output.Append(client.Top);
    output.Append(",\"cw\":");
    output.Append(clientWidth);
    output.Append(",\"ch\":");
    output.Append(clientHeight);
    output.Append(",\"dpi\":");
    output.Append(DpiOf(hWnd));
    output.Append("}");
    return output.ToString();
  }

  public static string List() {
    string listed = null;
    System.Threading.Thread worker = new System.Threading.Thread(delegate() {
      AttachToInputDesktop();
      SetProcessDPIAware();
      StringBuilder output = new StringBuilder();
      EnumWindows(delegate(IntPtr hWnd, IntPtr param) {
        if (!IsWindowVisible(hWnd)) return true;
        if (Cloaked(hWnd)) return true;
        string line = Line(hWnd);
        if (line != null) { output.Append(line); output.Append("\n"); }
        return true;
      }, IntPtr.Zero);
      listed = output.ToString();
    });
    worker.SetApartmentState(System.Threading.ApartmentState.STA);
    worker.Start();
    worker.Join();
    return listed;
  }

  // Re-reading one window by handle, so a step in an observation loop does not pay for a full
  // desktop enumeration just to learn that the game window moved four pixels.
  public static string Describe(long handle) {
    string described = null;
    System.Threading.Thread worker = new System.Threading.Thread(delegate() {
      AttachToInputDesktop();
      SetProcessDPIAware();
      IntPtr hWnd = new IntPtr(handle);
      if (!IsWindow(hWnd) || !IsWindowVisible(hWnd)) { described = "ERR|closed|the window handle is no longer valid"; return; }
      string line = Line(hWnd);
      described = line == null ? "ERR|empty|the window has no client area" : line;
    });
    worker.SetApartmentState(System.Threading.ApartmentState.STA);
    worker.Start();
    worker.Join();
    return described;
  }

  // Windows only lets the foreground process hand focus away. Attaching to the foreground
  // thread's input queue is the documented way around that; if Windows still refuses, the
  // caller is told so rather than being left to type into whatever was focused instead.
  static bool Focus(IntPtr hWnd) {
    if (IsIconic(hWnd)) ShowWindow(hWnd, SW_RESTORE);
    // A game embedded in the studio viewport (ADR-0045) is a child of Bhippi's window. The
    // foreground window Windows reports is then that root, and the child only gets the
    // keyboard once its own thread is handed the focus.
    IntPtr root = GetAncestor(hWnd, GA_ROOT);
    if (root == IntPtr.Zero) root = hWnd;
    bool embedded = root != hWnd;
    for (int attempt = 0; attempt < 5; attempt++) {
      IntPtr foreground = GetForegroundWindow();
      if (foreground == root) return embedded ? FocusChild(hWnd) : true;
      uint foregroundThread = GetWindowThread(foreground, IntPtr.Zero);
      uint currentThread = GetCurrentThreadId();
      bool attached = false;
      if (foregroundThread != 0 && foregroundThread != currentThread) {
        attached = AttachThreadInput(foregroundThread, currentThread, true);
      }
      SetForegroundWindow(root);
      BringWindowToTop(root);
      if (attached) AttachThreadInput(foregroundThread, currentThread, false);
      System.Threading.Thread.Sleep(60);
      if (GetForegroundWindow() == root) return embedded ? FocusChild(hWnd) : true;
    }
    return GetForegroundWindow() == root;
  }

  // Keyboard focus is per thread: attach to the child's thread for the one call that moves it.
  static bool FocusChild(IntPtr hWnd) {
    uint childThread = GetWindowThread(hWnd, IntPtr.Zero);
    uint currentThread = GetCurrentThreadId();
    bool attached = childThread != 0 && childThread != currentThread && AttachThreadInput(currentThread, childThread, true);
    SetFocus(hWnd);
    if (attached) AttachThreadInput(currentThread, childThread, false);
    return true;
  }

  // Posted, not sent: SendMessage would block this thread until the game finished its own
  // shutdown, and a game that hangs on the way out would hang the bridge with it.
  public static string Close(long handle) {
    string result = null;
    System.Threading.Thread worker = new System.Threading.Thread(delegate() {
      AttachToInputDesktop();
      IntPtr hWnd = new IntPtr(handle);
      if (!IsWindow(hWnd)) { result = "ERR|closed|the window handle is no longer valid"; return; }
      result = PostMessage(hWnd, WM_CLOSE, IntPtr.Zero, IntPtr.Zero) ? "OK" : "ERR|bridge|the close request was refused";
    });
    worker.SetApartmentState(System.Threading.ApartmentState.STA);
    worker.Start();
    worker.Join();
    return result;
  }

  public static string FocusOnly(long handle) {
    string result = null;
    System.Threading.Thread worker = new System.Threading.Thread(delegate() {
      AttachToInputDesktop();
      SetProcessDPIAware();
      IntPtr hWnd = new IntPtr(handle);
      if (!IsWindow(hWnd)) { result = "ERR|closed|the window handle is no longer valid"; return; }
      result = Focus(hWnd) ? "OK" : "ERR|focus|windows kept the foreground window";
    });
    worker.SetApartmentState(System.Threading.ApartmentState.STA);
    worker.Start();
    worker.Join();
    return result;
  }

  // A window rendered by the GPU can answer PrintWindow with a fully black frame. Sampling a
  // sparse grid is enough to notice, and cheap enough to do on every capture.
  static bool Blank(Bitmap image) {
    int stepX = Math.Max(1, image.Width / 24);
    int stepY = Math.Max(1, image.Height / 24);
    for (int y = 0; y < image.Height; y += stepY) {
      for (int x = 0; x < image.Width; x += stepX) {
        Color pixel = image.GetPixel(x, y);
        if (pixel.A != 0 && (pixel.R > 8 || pixel.G > 8 || pixel.B > 8)) return false;
      }
    }
    return true;
  }

  static byte[] Encode(Bitmap source, double scale, out int width, out int height) {
    width = Math.Max(1, (int)Math.Round(source.Width * scale));
    height = Math.Max(1, (int)Math.Round(source.Height * scale));
    using (Bitmap target = new Bitmap(width, height, PixelFormat.Format32bppArgb)) {
      using (Graphics graphics = Graphics.FromImage(target)) {
        graphics.InterpolationMode = InterpolationMode.HighQualityBicubic;
        graphics.DrawImage(source, 0, 0, width, height);
      }
      using (MemoryStream stream = new MemoryStream()) {
        target.Save(stream, ImageFormat.Png);
        return stream.ToArray();
      }
    }
  }

  public static string Capture(long handle, int maxBytes, double scale) {
    string result = null;
    System.Threading.Thread worker = new System.Threading.Thread(delegate() {
      AttachToInputDesktop();
      SetProcessDPIAware();
      IntPtr hWnd = new IntPtr(handle);
      if (!IsWindow(hWnd)) { result = "ERR|closed|the window handle is no longer valid"; return; }
      RECT frame;
      RECT client;
      if (!FrameBounds(hWnd, out frame) || !ClientBounds(hWnd, out client)) { result = "ERR|empty|the window has no measurable bounds"; return; }
      int clientWidth = client.Right - client.Left;
      int clientHeight = client.Bottom - client.Top;
      if (clientWidth <= 0 || clientHeight <= 0) { result = "ERR|empty|the window has no client area"; return; }

      Bitmap picture = null;
      string method = "print_window";
      RECT window;
      if (GetWindowRect(hWnd, out window)) {
        int windowWidth = window.Right - window.Left;
        int windowHeight = window.Bottom - window.Top;
        if (windowWidth > 0 && windowHeight > 0) {
          using (Bitmap whole = new Bitmap(windowWidth, windowHeight, PixelFormat.Format32bppArgb)) {
            bool printed = false;
            using (Graphics graphics = Graphics.FromImage(whole)) {
              IntPtr hdc = graphics.GetHdc();
              printed = PrintWindow(hWnd, hdc, PW_RENDERFULLCONTENT);
              graphics.ReleaseHdc(hdc);
            }
            if (printed) {
              // The client area sits at this offset inside the window bitmap.
              Rectangle crop = new Rectangle(client.Left - window.Left, client.Top - window.Top, clientWidth, clientHeight);
              crop.Intersect(new Rectangle(0, 0, windowWidth, windowHeight));
              if (crop.Width > 0 && crop.Height > 0) {
                Bitmap candidate = new Bitmap(crop.Width, crop.Height, PixelFormat.Format32bppArgb);
                using (Graphics graphics = Graphics.FromImage(candidate)) {
                  graphics.DrawImage(whole, new Rectangle(0, 0, crop.Width, crop.Height), crop, GraphicsUnit.Pixel);
                }
                if (Blank(candidate)) { candidate.Dispose(); } else { picture = candidate; }
              }
            }
          }
        }
      }

      if (picture == null) {
        method = "screen_copy";
        if (!Focus(hWnd)) { result = "ERR|focus|windows kept the foreground window"; return; }
        System.Threading.Thread.Sleep(140);
        if (!ClientBounds(hWnd, out client)) { result = "ERR|empty|the window has no client area"; return; }
        clientWidth = client.Right - client.Left;
        clientHeight = client.Bottom - client.Top;
        if (clientWidth <= 0 || clientHeight <= 0) { result = "ERR|empty|the window has no client area"; return; }
        picture = new Bitmap(clientWidth, clientHeight, PixelFormat.Format32bppArgb);
        using (Graphics graphics = Graphics.FromImage(picture)) {
          graphics.CopyFromScreen(client.Left, client.Top, 0, 0, new Size(clientWidth, clientHeight), CopyPixelOperation.SourceCopy);
        }
      }

      double used = scale <= 0 ? 1.0 : scale;
      int width = 0;
      int height = 0;
      byte[] png = Encode(picture, used, out width, out height);
      int guard = 0;
      while (png.Length > maxBytes && used > 0.15 && guard < 6) {
        double factor = Math.Sqrt((double)maxBytes / (double)png.Length) * 0.9;
        if (factor > 0.95) factor = 0.95;
        used = used * factor;
        if (used < 0.15) used = 0.15;
        png = Encode(picture, used, out width, out height);
        guard++;
      }
      picture.Dispose();

      StringBuilder payload = new StringBuilder();
      payload.Append("{\"method\":\"");
      payload.Append(method);
      payload.Append("\",\"width\":");
      payload.Append(width);
      payload.Append(",\"height\":");
      payload.Append(height);
      payload.Append(",\"scale\":");
      payload.Append(used.ToString("0.0000", CultureInfo.InvariantCulture));
      payload.Append(",\"png_b64\":\"");
      payload.Append(Convert.ToBase64String(png));
      payload.Append("\"}");
      result = payload.ToString();
    });
    worker.SetApartmentState(System.Threading.ApartmentState.STA);
    worker.Start();
    worker.Join();
    return result;
  }

  // Arrows, navigation keys and the right-hand modifiers live on the extended scan-code page.
  static bool Extended(int key) {
    switch (key) {
      case 0x21: case 0x22: case 0x23: case 0x24:
      case 0x25: case 0x26: case 0x27: case 0x28:
      case 0x2C: case 0x2D: case 0x2E:
      case 0x5B: case 0x5C: case 0x5D:
      case 0x6F: case 0x90: case 0xA3: case 0xA5:
        return true;
    }
    return false;
  }

  // Scan codes, not virtual keys: a game reading raw input sees the scan code and ignores an
  // injected virtual key that has none behind it.
  static INPUT KeyEvent(int key, bool up) {
    INPUT input = new INPUT();
    input.type = INPUT_KEYBOARD;
    uint scan = MapVirtualKey((uint)key, 0);
    if (scan != 0) {
      input.u.ki.wVk = 0;
      input.u.ki.wScan = (ushort)scan;
      input.u.ki.dwFlags = KEYEVENTF_SCANCODE;
    } else {
      input.u.ki.wVk = (ushort)key;
      input.u.ki.wScan = 0;
      input.u.ki.dwFlags = 0;
    }
    if (Extended(key)) input.u.ki.dwFlags |= KEYEVENTF_EXTENDEDKEY;
    if (up) input.u.ki.dwFlags |= KEYEVENTF_KEYUP;
    return input;
  }

  static void SendOne(INPUT input) {
    INPUT[] batch = new INPUT[1];
    batch[0] = input;
    SendInput(1, batch, Marshal.SizeOf(typeof(INPUT)));
  }

  static void SendUnicode(string text) {
    for (int index = 0; index < text.Length; index++) {
      char character = text[index];
      if (character == '\r') continue;
      if (character == '\n') {
        SendOne(KeyEvent(0x0D, false));
        SendOne(KeyEvent(0x0D, true));
        continue;
      }
      if (character == '\t') {
        SendOne(KeyEvent(0x09, false));
        SendOne(KeyEvent(0x09, true));
        continue;
      }
      INPUT down = new INPUT();
      down.type = INPUT_KEYBOARD;
      down.u.ki.wVk = 0;
      down.u.ki.wScan = (ushort)character;
      down.u.ki.dwFlags = KEYEVENTF_UNICODE;
      INPUT up = down;
      up.u.ki.dwFlags = KEYEVENTF_UNICODE | KEYEVENTF_KEYUP;
      SendOne(down);
      SendOne(up);
      System.Threading.Thread.Sleep(2);
    }
  }

  static void MouseButton(int button, bool up) {
    uint flags;
    if (button == 1) { flags = up ? 0x0010u : 0x0008u; }
    else if (button == 2) { flags = up ? 0x0040u : 0x0020u; }
    else { flags = up ? 0x0004u : 0x0002u; }
    INPUT input = new INPUT();
    input.type = INPUT_MOUSE;
    input.u.mi.dwFlags = flags;
    SendOne(input);
  }

  public static string Input(long handle, string op, int[] keys, int hasPoint, int x, int y, int button, int holdMs, string textB64) {
    string result = null;
    System.Threading.Thread worker = new System.Threading.Thread(delegate() {
      AttachToInputDesktop();
      SetProcessDPIAware();
      IntPtr hWnd = new IntPtr(handle);
      if (!IsWindow(hWnd)) { result = "ERR|closed|the window handle is no longer valid"; return; }
      if (!Focus(hWnd)) { result = "ERR|focus|windows kept the foreground window"; return; }
      if (hasPoint != 0) {
        // Second gate: the window may have moved since Rust translated the coordinate.
        RECT client;
        if (!ClientBounds(hWnd, out client)) { result = "ERR|empty|the window has no client area"; return; }
        if (x < client.Left || x >= client.Right || y < client.Top || y >= client.Bottom) {
          result = "ERR|outside|the point is no longer inside the window";
          return;
        }
        SetCursorPos(x, y);
        System.Threading.Thread.Sleep(20);
      }
      // "mouse_move" is exactly the block above and nothing else, so it has no arm here.
      if (op == "key_down") {
        for (int index = 0; index < keys.Length; index++) SendOne(KeyEvent(keys[index], false));
      } else if (op == "key_up") {
        for (int index = 0; index < keys.Length; index++) SendOne(KeyEvent(keys[index], true));
      } else if (op == "key_tap") {
        for (int index = 0; index < keys.Length; index++) {
          SendOne(KeyEvent(keys[index], false));
          System.Threading.Thread.Sleep(30);
          SendOne(KeyEvent(keys[index], true));
        }
      } else if (op == "hold") {
        for (int index = 0; index < keys.Length; index++) SendOne(KeyEvent(keys[index], false));
        System.Threading.Thread.Sleep(holdMs);
        for (int index = keys.Length - 1; index >= 0; index--) SendOne(KeyEvent(keys[index], true));
      } else if (op == "text") {
        SendUnicode(Encoding.UTF8.GetString(Convert.FromBase64String(textB64)));
      } else if (op == "click") {
        MouseButton(button, false);
        System.Threading.Thread.Sleep(30);
        MouseButton(button, true);
      }
      result = "OK";
    });
    worker.SetApartmentState(System.Threading.ApartmentState.STA);
    worker.Start();
    worker.Join();
    return result;
  }
}
"#;

#[cfg(windows)]
async fn run_bridge(call: &str) -> Result<String> {
    run_bridge_with("", call).await
}

/// Wraps the shared C# class in the same fixed-argv, `CREATE_NO_WINDOW` PowerShell invocation
/// `computer.rs` uses. The prelude carries only Rust-generated integers, one operation word from
/// a fixed set, and base64 — no caller text is ever interpolated into executable script.
#[cfg(windows)]
async fn run_bridge_with(prelude: &str, call: &str) -> Result<String> {
    let script = format!(
        "$ErrorActionPreference = 'Stop'\nAdd-Type -AssemblyName System.Drawing\n{prelude}\n$src = @'\n{WINDOW_BRIDGE_CSHARP}\n'@\nAdd-Type -TypeDefinition $src -ReferencedAssemblies System.Drawing -ErrorAction Stop\n{call}\n"
    );
    crate::computer::run_powershell_output(&script)
        .await
        .map_err(|detail| WindowError::Bridge { detail })
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Two windows and a stray PowerShell warning, exactly as the bridge writes them.
    const ENUMERATION_FIXTURE: &str = concat!(
        "WARNING: something PowerShell wanted to say\n",
        // title "Bhippi Game", class "Engine"
        "{\"hwnd\":132456,\"pid\":4242,\"title_b64\":\"QmhpcHBpIEdhbWU=\",\"class_b64\":\"RW5naW5l\",",
        "\"x\":100,\"y\":80,\"w\":1288,\"h\":760,\"cx\":104,\"cy\":118,\"cw\":1280,\"ch\":720,\"dpi\":96}\n",
        // title "Notes", class "Chrome_WidgetWin_1", 150% display
        "{\"hwnd\":999,\"pid\":77,\"title_b64\":\"Tm90ZXM=\",\"class_b64\":\"Q2hyb21lX1dpZGdldFdpbl8x\",",
        "\"x\":0,\"y\":0,\"w\":800,\"h\":600,\"cx\":0,\"cy\":30,\"cw\":800,\"ch\":570,\"dpi\":144}\n",
        // zero-height client area: not watchable, not listed
        "{\"hwnd\":7,\"pid\":1,\"title_b64\":\"\",\"class_b64\":\"\",",
        "\"x\":0,\"y\":0,\"w\":0,\"h\":0,\"cx\":0,\"cy\":0,\"cw\":0,\"ch\":0,\"dpi\":96}\n",
    );

    fn godot_window() -> WindowRef {
        WindowRef {
            hwnd: 132_456,
            title: "Bhippi Game".to_owned(),
            class_name: "Engine".to_owned(),
            process_id: 4242,
            rect: WindowRect {
                x: 104,
                y: 118,
                width: 1280,
                height: 720,
            },
            frame: WindowRect {
                x: 100,
                y: 80,
                width: 1288,
                height: 760,
            },
            dpi_scale: 1.0,
        }
    }

    fn expect<T>(value: Result<T>, what: &str) -> T {
        match value {
            Ok(value) => value,
            Err(error) => panic!("{what} must succeed: {error}"),
        }
    }

    #[test]
    fn enumeration_lines_parse_into_window_refs() {
        let windows = expect(parse_window_lines(ENUMERATION_FIXTURE), "parsing");
        assert_eq!(windows.len(), 2, "the client-less window is dropped");

        let game = &windows[0];
        assert_eq!(game.hwnd, 132_456);
        assert_eq!(game.title, "Bhippi Game");
        assert_eq!(game.class_name, "Engine");
        assert_eq!(game.process_id, 4242);
        assert_eq!(game.rect.x, 104);
        assert_eq!(game.rect.y, 118);
        assert_eq!(game.rect.width, 1280);
        assert_eq!(game.rect.height, 720);
        assert_eq!(game.frame.width, 1288);
        assert!((game.dpi_scale - 1.0).abs() < f32::EPSILON);

        assert!((windows[1].dpi_scale - 1.5).abs() < f32::EPSILON);
        assert_eq!(windows[1].logical_size(), (533, 380));
    }

    #[test]
    fn enumeration_ignores_noise_but_reports_an_unreadable_record() {
        assert!(expect(parse_window_lines("not json at all\n\n"), "noise").is_empty());
        let broken = "{\"hwnd\":5,\"pid\":1,\"title_b64\":\"!!!!\",\"class_b64\":\"\",\"x\":0,\"y\":0,\"w\":10,\"h\":10,\"cx\":0,\"cy\":0,\"cw\":10,\"ch\":10,\"dpi\":96}";
        assert!(
            matches!(parse_window_lines(broken), Err(WindowError::Bridge { .. })),
            "a record we cannot decode is a bridge fault, not noise"
        );
    }

    #[test]
    fn filters_match_godot_class_and_title_together() {
        let windows = expect(parse_window_lines(ENUMERATION_FIXTURE), "parsing");
        let by_class = WindowFilter {
            class_contains: Some("engine".to_owned()),
            ..WindowFilter::default()
        };
        assert_eq!(windows.iter().filter(|w| by_class.matches(w)).count(), 1);

        let by_title = WindowFilter {
            title_contains: Some("bhippi".to_owned()),
            ..WindowFilter::default()
        };
        assert!(by_title.matches(&windows[0]));
        assert!(!by_title.matches(&windows[1]));

        let by_process = WindowFilter {
            process_id: Some(77),
            ..WindowFilter::default()
        };
        assert!(by_process.matches(&windows[1]));
        assert!(!by_process.matches(&windows[0]));

        assert!(
            WindowFilter::default().matches(&windows[0]),
            "empty matches all"
        );
    }

    #[test]
    fn client_coordinates_translate_through_the_dpi_scale() {
        let mut window = godot_window();

        assert_eq!(expect(window.client_to_screen(0, 0), "origin"), (104, 118));
        assert_eq!(
            expect(window.client_to_screen(640, 360), "centre"),
            (744, 478)
        );

        window.dpi_scale = 1.5;
        assert_eq!(
            expect(window.client_to_screen(400, 200), "150%"),
            (104 + 600, 118 + 300)
        );
        assert_eq!(window.logical_size(), (853, 480));

        window.dpi_scale = 2.0;
        assert_eq!(
            expect(window.client_to_screen(100, 50), "200%"),
            (104 + 200, 118 + 100)
        );
        assert_eq!(window.logical_size(), (640, 360));
    }

    #[test]
    fn coordinates_outside_the_client_area_are_refused_not_clamped() {
        let window = godot_window();
        for (x, y) in [(-1, 10), (10, -1), (1280, 10), (10, 720), (5000, 5000)] {
            assert!(
                matches!(
                    window.client_to_screen(x, y),
                    Err(WindowError::OutsideWindow { .. })
                ),
                "({x}, {y}) must be refused"
            );
        }
        // The refusal survives DPI: 700 logical is 1050 physical, past the 1280-wide window.
        let mut scaled = window.clone();
        scaled.dpi_scale = 2.0;
        assert!(matches!(
            scaled.client_to_screen(700, 10),
            Err(WindowError::OutsideWindow { .. })
        ));
        assert!(scaled.client_to_screen(600, 10).is_ok());

        // The screen-space rect agrees with the translation: the last pixel is inside, the
        // first pixel past the edge is not.
        assert!(window.rect.contains(104, 118));
        assert!(!window.rect.contains(103, 118));
        assert!(window.rect.contains(104 + 1279, 118 + 719));
        assert!(!window.rect.contains(104 + 1280, 118 + 719));
    }

    #[test]
    fn capture_coordinates_map_back_into_client_space() {
        let capture = WindowCapture {
            png_base64: "x".to_owned(),
            width: 640,
            height: 360,
            scale: 0.5,
            method: WindowCaptureMethod::PrintWindow,
            window: godot_window(),
            captured_at: Utc::now(),
        };
        assert_eq!(capture.to_client(320, 180), (640, 360));
        // A half-scale capture of a 200% window: image pixels are a quarter of logical ones.
        let mut scaled = capture.clone();
        scaled.window.dpi_scale = 2.0;
        assert_eq!(scaled.to_client(320, 180), (320, 180));
    }

    #[test]
    fn key_names_accept_godot_and_crate_spellings() {
        let pairs = [
            ("KEY_W", 0x57_u8),
            ("w", 0x57),
            ("KEY_SPACE", 0x20),
            ("space", 0x20),
            ("KEY_ESCAPE", 0x1B),
            ("esc", 0x1B),
            ("KEY_LEFT", 0x25),
            ("left", 0x25),
            ("KEY_ENTER", 0x0D),
            ("KEY_KP_ENTER", 0x0D),
            ("KEY_SHIFT", 0x10),
            ("KEY_CTRL", 0x11),
            ("KEY_PAGEUP", 0x21),
            ("KEY_F5", 0x74),
            ("KEY_0", 0x30),
            ("KEY_KP_4", 0x64),
            ("KEY_COMMA", 0xBC),
            ("  key_d  ", 0x44),
        ];
        for (name, code) in pairs {
            assert_eq!(
                expect(KeyName::new(name).resolve(), name),
                code,
                "{name} must resolve"
            );
        }
    }

    #[test]
    fn unknown_key_names_come_back_with_the_nearest_spelling() {
        let error = KeyName::new("KEY_ESCPE").resolve();
        match error {
            Err(WindowError::UnknownKey { key, nearest }) => {
                assert_eq!(key, "KEY_ESCPE");
                assert_eq!(nearest.as_deref(), Some("escape"));
            }
            other => panic!("a misspelt key must be a typed error: {other:?}"),
        }
        let hint = match KeyName::new("KEY_SPCE").resolve() {
            Err(error) => error.hint(),
            Ok(code) => panic!("KEY_SPCE must not resolve to {code}"),
        };
        assert!(hint.contains("space"), "hint should name the key: {hint}");
        assert!(matches!(
            KeyName::new("$").resolve(),
            Err(WindowError::UnknownKey { nearest: None, .. })
        ));
    }

    #[test]
    fn input_validation_refuses_oversized_and_offscreen_requests() {
        let window = godot_window();
        assert!(WindowInput::Text {
            text: "hello".to_owned()
        }
        .validate(&window)
        .is_ok());
        assert!(matches!(
            WindowInput::Text {
                text: "x".repeat(WINDOW_INPUT_MAX_TEXT_CHARS + 1)
            }
            .validate(&window),
            Err(WindowError::Invalid { .. })
        ));
        assert!(matches!(
            WindowInput::Text {
                text: String::new()
            }
            .validate(&window),
            Err(WindowError::Invalid { .. })
        ));
        assert!(matches!(
            WindowInput::Hold {
                keys: vec![KeyName::new("KEY_W")],
                frames_ms: WINDOW_HOLD_MAX_MS + 1
            }
            .validate(&window),
            Err(WindowError::Invalid { .. })
        ));
        assert!(matches!(
            WindowInput::Hold {
                keys: Vec::new(),
                frames_ms: 250
            }
            .validate(&window),
            Err(WindowError::Invalid { .. })
        ));
        assert!(WindowInput::Hold {
            keys: vec![KeyName::new("KEY_W"), KeyName::new("KEY_SHIFT")],
            frames_ms: 250
        }
        .validate(&window)
        .is_ok());
        assert!(matches!(
            WindowInput::Click {
                x: 4000,
                y: 10,
                button: WindowMouseButton::Left
            }
            .validate(&window),
            Err(WindowError::OutsideWindow { .. })
        ));
        assert!(matches!(
            WindowInput::KeyTap {
                key: KeyName::new("KEY_NOPE")
            }
            .validate(&window),
            Err(WindowError::UnknownKey { .. })
        ));
    }

    #[tokio::test]
    async fn session_refuses_to_exceed_its_step_cap() {
        let mut session =
            WindowSession::start(godot_window()).with_limits(0, WINDOW_SESSION_MAX_MS);
        match session.step(None, false).await {
            Err(WindowError::StepLimit { limit }) => assert_eq!(limit, 0),
            other => panic!("the step cap must stop the session: {other:?}"),
        }
        assert_eq!(session.steps_taken(), 0);
    }

    #[tokio::test]
    async fn session_refuses_to_exceed_its_time_cap() {
        let mut session = WindowSession::start(godot_window()).with_limits(10, 0);
        match session.step(None, false).await {
            Err(WindowError::TimeLimit { limit_ms }) => assert_eq!(limit_ms, 0),
            other => panic!("the time cap must stop the session: {other:?}"),
        }
    }

    #[tokio::test]
    async fn session_honours_the_stop_signal_before_touching_the_window() {
        let (sender, receiver) = watch::channel(false);
        let mut session = WindowSession::start(godot_window()).with_stop_signal(receiver);
        if sender.send(true).is_err() {
            panic!("the stop signal must be deliverable");
        }
        assert!(matches!(
            session.step(None, false).await,
            Err(WindowError::Stopped)
        ));
    }

    #[test]
    fn session_limits_can_only_tighten() {
        let session = WindowSession::start(godot_window())
            .with_limits(WINDOW_SESSION_MAX_STEPS + 500, WINDOW_SESSION_MAX_MS + 500);
        assert_eq!(session.max_steps, WINDOW_SESSION_MAX_STEPS);
        assert_eq!(session.max_ms, WINDOW_SESSION_MAX_MS);
    }

    #[test]
    fn every_error_carries_a_hint_and_a_message() {
        let errors = [
            WindowError::Unsupported {
                operation: "Window capture".to_owned(),
            },
            WindowError::NotFound {
                filter: "class containing `Engine`".to_owned(),
            },
            WindowError::WindowClosed { hwnd: 7 },
            WindowError::FocusRefused { hwnd: 7 },
            WindowError::OutsideWindow {
                x: 5,
                y: 5,
                rect: godot_window().rect,
            },
            WindowError::UnknownKey {
                key: "KEY_NOPE".to_owned(),
                nearest: None,
            },
            WindowError::StepLimit { limit: 3 },
            WindowError::TimeLimit { limit_ms: 3 },
            WindowError::Stopped,
            WindowError::Bridge {
                detail: "boom".to_owned(),
            },
        ];
        for error in errors {
            assert!(!error.to_string().is_empty());
            assert!(!error.hint().is_empty());
            let app: AppError = error.clone().into();
            assert_eq!(app.message, error.to_string());
            assert_eq!(app.hint, Some(error.hint()));
        }
    }

    #[test]
    fn bridge_status_words_become_typed_errors() {
        assert!(matches!(
            bridge_status("ERR|closed|gone", 9),
            WindowError::WindowClosed { hwnd: 9 }
        ));
        assert!(matches!(
            bridge_status("ERR|focus|refused", 9),
            WindowError::FocusRefused { hwnd: 9 }
        ));
        assert!(matches!(
            bridge_status("ERR|outside|moved", 9),
            WindowError::Invalid { .. }
        ));
        assert!(matches!(bridge_status("", 9), WindowError::Bridge { .. }));
    }

    #[test]
    fn capture_payloads_parse_and_empty_ones_are_refused() {
        let window = godot_window();
        let line = "{\"method\":\"print_window\",\"width\":640,\"height\":360,\"scale\":0.5,\"png_b64\":\"aGk=\"}";
        let capture = expect(parse_capture_line(line, &window), "capture parsing");
        assert_eq!(capture.width, 640);
        assert_eq!(capture.method, WindowCaptureMethod::PrintWindow);
        assert_eq!(capture.window.hwnd, window.hwnd);

        let empty =
            "{\"method\":\"screen_copy\",\"width\":0,\"height\":0,\"scale\":1.0,\"png_b64\":\"\"}";
        assert!(matches!(
            parse_capture_line(empty, &window),
            Err(WindowError::Invalid { .. })
        ));
        assert!(matches!(
            parse_capture_line("ERR|closed|gone", &window),
            Err(WindowError::WindowClosed { .. })
        ));
    }

    #[cfg(windows)]
    #[test]
    fn input_preludes_carry_only_numbers_and_base64() {
        let window = godot_window();
        let prelude = expect(
            input_prelude(
                &window,
                &WindowInput::Hold {
                    keys: vec![KeyName::new("KEY_W"), KeyName::new("KEY_SHIFT")],
                    frames_ms: 500,
                },
            ),
            "hold prelude",
        );
        assert!(prelude.contains("$Op = 'hold'"));
        assert!(prelude.contains("$Vks = @(87,16)"));
        assert!(prelude.contains("$HoldMs = 500"));
        assert!(prelude.contains("$HasPoint = 0"));

        let prelude = expect(
            input_prelude(
                &window,
                &WindowInput::Click {
                    x: 640,
                    y: 360,
                    button: WindowMouseButton::Right,
                },
            ),
            "click prelude",
        );
        assert!(prelude.contains("$HasPoint = 1"));
        assert!(prelude.contains("$X = 744"));
        assert!(prelude.contains("$Y = 478"));
        assert!(prelude.contains("$Button = 1"));
    }

    /// INV-089's last row, as a type-level property: nothing an engine observation holds can
    /// name the desktop, and the desktop scope is reached only from the module that owns it.
    #[test]
    fn an_engine_scope_can_only_ever_be_one_window() {
        let engine = EngineCaptureScope::new(godot_window());
        let scope = engine.scope();
        assert!(
            matches!(scope, CaptureScope::Window { .. }),
            "the engine path has no desktop arm to reach"
        );
        assert!(!scope.is_desktop());
        assert_eq!(
            scope.window().map(|window| window.hwnd),
            Some(godot_window().hwnd)
        );
        assert!(scope.describe().contains("Bhippi Game"));

        // The desktop scope exists, and only `computer.rs` — the explicit, user-initiated
        // flow — produces it.
        let desktop = crate::computer::desktop_scope();
        assert!(desktop.is_desktop());
        assert_eq!(desktop.window(), None);
        assert_ne!(desktop, scope);
    }

    /// A plan is validated before a window exists, so the shape check must stand alone —
    /// and must still be the same check `validate` performs once there is one.
    #[test]
    fn shape_validation_stands_alone_and_stays_part_of_the_windowed_check() {
        let window = godot_window();
        let bad_key = WindowInput::KeyTap {
            key: KeyName::new("KEY_NOT_A_KEY"),
        };
        assert!(bad_key.validate_shape().is_err());
        assert!(bad_key.validate(&window).is_err());

        let long_hold = WindowInput::Hold {
            keys: vec![KeyName::new("w")],
            frames_ms: WINDOW_HOLD_MAX_MS + 1,
        };
        assert!(long_hold.validate_shape().is_err());

        // A coordinate is the window's business and cannot be judged without one: the shape
        // check passes, and only `validate` refuses it.
        let outside = WindowInput::Click {
            x: 99_999,
            y: 10,
            button: WindowMouseButton::Left,
        };
        assert!(outside.validate_shape().is_ok());
        assert!(matches!(
            outside.validate(&window),
            Err(WindowError::OutsideWindow { .. })
        ));

        let inside = WindowInput::Click {
            x: 10,
            y: 10,
            button: WindowMouseButton::Left,
        };
        assert!(inside.validate_shape().is_ok());
        assert!(inside.validate(&window).is_ok());
    }

    /// Live tests touch the real desktop. They run only with `BHIPPI_LIVE_WINDOW=1`:
    /// `cargo test -p bhippi-app computer -- --ignored --nocapture`.
    fn live_enabled() -> bool {
        std::env::var("BHIPPI_LIVE_WINDOW").is_ok_and(|value| value == "1")
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "enumerates the real desktop's windows"]
    async fn live_enumeration_lists_visible_windows() {
        if !live_enabled() {
            println!("skipped: set BHIPPI_LIVE_WINDOW=1 to run");
            return;
        }
        let windows = expect(find_windows(WindowFilter::default()).await, "enumeration");
        println!("enumerated {} windows", windows.len());
        for window in windows.iter().take(15) {
            println!(
                "  hwnd={} pid={} class={} dpi={:.2} client={}x{} at ({}, {}) title={}",
                window.hwnd,
                window.process_id,
                window.class_name,
                window.dpi_scale,
                window.rect.width,
                window.rect.height,
                window.rect.x,
                window.rect.y,
                window.title
            );
        }
        assert!(!windows.is_empty(), "a live desktop has visible windows");
        assert!(windows.iter().all(|window| window.rect.width > 0));

        // The single-handle lookup an observation step uses must agree with enumeration.
        let Some(first) = windows.first() else {
            panic!("a live desktop has visible windows");
        };
        let refreshed = expect(refresh_window(first).await, "refresh by handle");
        println!(
            "refreshed hwnd={} title={}",
            refreshed.hwnd, refreshed.title
        );
        assert_eq!(refreshed.hwnd, first.hwnd);
        assert_eq!(refreshed.class_name, first.class_name);

        // A handle that was never a window is a typed closure, not a stray success.
        let mut ghost = first.clone();
        ghost.hwnd = 1;
        assert!(matches!(
            refresh_window(&ghost).await,
            Err(WindowError::WindowClosed { hwnd: 1 })
        ));
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "captures a real window from the live desktop"]
    async fn live_capture_of_a_real_window() {
        if !live_enabled() {
            println!("skipped: set BHIPPI_LIVE_WINDOW=1 to run");
            return;
        }
        let windows = expect(find_windows(WindowFilter::default()).await, "enumeration");
        // The desktop shell and the taskbar are windows too, and capturing them proves nothing
        // about a game: the interesting case is a real, GPU-composited application window.
        let target = windows
            .into_iter()
            .filter(|window| {
                !matches!(
                    window.class_name.as_str(),
                    "Progman" | "WorkerW" | "Shell_TrayWnd"
                ) && window.rect.x > -30_000
                    && !window.title.is_empty()
            })
            .max_by_key(|window| u64::from(window.rect.width) * u64::from(window.rect.height));
        let Some(target) = target else {
            panic!("a live desktop has at least one application window to capture");
        };
        println!(
            "capturing hwnd={} class={} title={}",
            target.hwnd, target.class_name, target.title
        );
        let capture = expect(
            capture_window(&target, CaptureOptions::default()).await,
            "capture",
        );
        println!(
            "captured {}x{} via {:?} at scale {:.3}, {} base64 chars",
            capture.width,
            capture.height,
            capture.method,
            capture.scale,
            capture.png_base64.len()
        );
        assert!(capture.width > 0 && capture.height > 0);
        assert!(!capture.png_base64.is_empty());
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&capture.png_base64)
            .unwrap_or_default();
        assert!(bytes.len() > 8, "the capture decodes to real bytes");
        assert_eq!(
            &bytes[..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
            "the capture is a PNG"
        );
        assert!(
            bytes.len() <= WINDOW_CAPTURE_MAX_BYTES,
            "the capture respects its byte budget"
        );

        // A tight budget must actually shrink the image rather than blow past the ceiling.
        let budget = 120_000;
        let small = expect(
            capture_window(
                &target,
                CaptureOptions {
                    scale: None,
                    max_bytes: budget,
                },
            )
            .await,
            "budgeted capture",
        );
        let small_bytes = base64::engine::general_purpose::STANDARD
            .decode(&small.png_base64)
            .unwrap_or_default();
        println!(
            "budgeted to {budget} bytes: {}x{} at scale {:.3}, {} bytes",
            small.width,
            small.height,
            small.scale,
            small_bytes.len()
        );
        assert!(small.width <= capture.width);
        assert!(small.scale <= capture.scale);
    }
}
