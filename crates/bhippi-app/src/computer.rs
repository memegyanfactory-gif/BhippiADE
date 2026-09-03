//! Computer Use: explicit-intent desktop perception and input automation.
//!
//! The provider decides *what* action to request. This module alone validates and executes
//! desktop input. On Windows the fixed-argv PowerShell bridge keeps unsafe code out of the Rust
//! workspace while `CREATE_NO_WINDOW` prevents a console from appearing.
//!
//! Every operation — reading the desktop bounds, capturing the screen, and sending input —
//! runs through the same shim in the same kind of process. That is deliberate. What actually
//! makes a click land on the thing the model aimed at is not absolute correctness of the
//! coordinate space but *agreement* between the screenshot the model looked at and the
//! pointer that moves afterwards. One shim, one DPI declaration, one set of metrics.

use base64::Engine as _;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Stdio;
#[cfg(windows)]
use std::time::Duration;

/// Generous enough for a long `type_text`, which is the only operation that is not instant.
#[cfg(windows)]
const POWERSHELL_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_TYPED_CHARS: usize = 20_000;
const MAX_SCROLL_DELTA: i32 = 12_000;
const MAX_CLICK_COUNT: u32 = 2;
/// A program name, a path or a URL — nothing longer is a real target.
const MAX_TARGET_CHARS: usize = 1_024;
/// The longest a single `wait` may hold the loop; the next screenshot is the point.
const MAX_WAIT_MS: u32 = 10_000;
/// How many windows `list_windows` names. Past this the model should ask by title.
const MAX_LISTED_WINDOWS: usize = 40;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ScreenBounds {
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenBounds {
    fn contains(self, x: i32, y: i32) -> bool {
        let right = i64::from(self.origin_x) + i64::from(self.width);
        let bottom = i64::from(self.origin_y) + i64::from(self.height);
        i64::from(x) >= i64::from(self.origin_x)
            && i64::from(x) < right
            && i64::from(y) >= i64::from(self.origin_y)
            && i64::from(y) < bottom
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ScreenCapture {
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    pub image_base64: String,
    pub captured_at: chrono::DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ComputerUseStatus {
    pub enabled: bool,
    pub full_access: bool,
    pub allowed_providers: Vec<String>,
    pub supported_providers: Vec<ProviderVisionCapability>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProviderVisionCapability {
    pub id: String,
    pub label: String,
    pub vision_supported: bool,
    pub computer_use_allowed: bool,
    pub note: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ComputerAction {
    Screenshot,
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseClick {
        button: String,
        count: u32,
        x: Option<i32>,
        y: Option<i32>,
    },
    MouseDrag {
        start_x: i32,
        start_y: i32,
        end_x: i32,
        end_y: i32,
    },
    MouseScroll {
        delta_x: i32,
        delta_y: i32,
    },
    TypeText {
        text: String,
    },
    KeyPress {
        key: String,
    },
    Hotkey {
        keys: Vec<String>,
    },
    GetScreenSize,
    GetCursorPosition,
    /// Opens a program, an `.exe`, a document, a folder or a URL the way Explorer would
    /// (SPA-302). The reach the owner asked for: "open anything".
    OpenApp {
        target: String,
    },
    OpenUrl {
        url: String,
    },
    /// Brings the first window whose title contains the text to the front.
    FocusWindow {
        title: String,
    },
    /// Names the open windows, so the model can pick one to focus rather than hunt.
    ListWindows,
    /// Pauses before the next screenshot, so an app can finish opening.
    Wait {
        ms: u32,
    },
}

impl ComputerAction {
    #[must_use]
    pub const fn requires_full_access(&self) -> bool {
        !matches!(
            self,
            Self::Screenshot
                | Self::GetScreenSize
                | Self::GetCursorPosition
                | Self::ListWindows
                | Self::Wait { .. }
        )
    }

    pub fn validate(&self, bounds: ScreenBounds) -> Result<(), String> {
        let coordinate = |x: i32, y: i32| {
            if bounds.contains(x, y) {
                Ok(())
            } else {
                Err(format!(
                    "Coordinate ({x}, {y}) is outside the desktop bounds ({}, {}) to ({}, {}).",
                    bounds.origin_x,
                    bounds.origin_y,
                    i64::from(bounds.origin_x) + i64::from(bounds.width) - 1,
                    i64::from(bounds.origin_y) + i64::from(bounds.height) - 1
                ))
            }
        };
        match self {
            Self::MouseMove { x, y } => coordinate(*x, *y),
            Self::MouseClick {
                button,
                count,
                x,
                y,
            } => {
                if !matches!(
                    button.to_ascii_lowercase().as_str(),
                    "left" | "right" | "middle"
                ) {
                    return Err("Mouse button must be left, right, or middle.".to_owned());
                }
                if !(1..=MAX_CLICK_COUNT).contains(count) {
                    return Err(format!(
                        "Click count must be between 1 and {MAX_CLICK_COUNT}."
                    ));
                }
                match (x, y) {
                    (Some(x), Some(y)) => coordinate(*x, *y),
                    (None, None) => Ok(()),
                    _ => Err(
                        "Mouse click coordinates must provide both x and y or neither.".to_owned(),
                    ),
                }
            }
            Self::MouseDrag {
                start_x,
                start_y,
                end_x,
                end_y,
            } => {
                coordinate(*start_x, *start_y)?;
                coordinate(*end_x, *end_y)
            }
            Self::MouseScroll { delta_x, delta_y } => {
                if delta_x.abs() > MAX_SCROLL_DELTA || delta_y.abs() > MAX_SCROLL_DELTA {
                    Err(format!("Scroll delta must not exceed {MAX_SCROLL_DELTA}."))
                } else if *delta_x == 0 && *delta_y == 0 {
                    Err("Scroll requires a non-zero horizontal or vertical delta.".to_owned())
                } else {
                    Ok(())
                }
            }
            Self::TypeText { text } => {
                if text.chars().count() > MAX_TYPED_CHARS {
                    Err(format!(
                        "Typed text exceeds the {MAX_TYPED_CHARS}-character limit."
                    ))
                } else {
                    Ok(())
                }
            }
            Self::KeyPress { key } => virtual_key(key)
                .map(|_| ())
                .ok_or_else(|| format!("Unsupported keyboard key: {key}")),
            Self::Hotkey { keys } => {
                if !(2..=4).contains(&keys.len()) {
                    return Err("A hotkey must contain between 2 and 4 keys.".to_owned());
                }
                if keys.iter().all(|key| virtual_key(key).is_some()) {
                    Ok(())
                } else {
                    Err("Hotkey contains an unsupported key.".to_owned())
                }
            }
            Self::Screenshot | Self::GetScreenSize | Self::GetCursorPosition => Ok(()),
            Self::OpenApp { target } => {
                let target = target.trim();
                if target.is_empty()
                    || target.chars().count() > MAX_TARGET_CHARS
                    || target.chars().any(char::is_control)
                {
                    Err("open_app needs a program name, a path or a URL, without control characters.".to_owned())
                } else {
                    Ok(())
                }
            }
            Self::OpenUrl { url } => {
                let url = url.trim();
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    Err("open_url needs an http(s) URL.".to_owned())
                } else if url.chars().count() > MAX_TARGET_CHARS
                    || url.chars().any(char::is_control)
                {
                    Err(
                        "open_url got a URL that is too long or carries control characters."
                            .to_owned(),
                    )
                } else {
                    Ok(())
                }
            }
            Self::FocusWindow { title } => {
                if title.trim().is_empty() {
                    Err("focus_window needs part of the window title.".to_owned())
                } else {
                    Ok(())
                }
            }
            Self::ListWindows => Ok(()),
            Self::Wait { ms } => {
                if *ms > MAX_WAIT_MS {
                    Err(format!("wait must not exceed {MAX_WAIT_MS} ms."))
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ComputerActionResult {
    pub success: bool,
    pub action: String,
    pub detail: String,
    pub screenshot: Option<ScreenCapture>,
    pub cursor: Option<(i32, i32)>,
    pub screen_size: Option<(u32, u32)>,
    pub screen_origin: Option<(i32, i32)>,
}

pub fn parse_action_json(json_str: &str) -> Option<ComputerAction> {
    let clean = json_str.trim();
    let clean = if clean.starts_with("```") {
        let trimmed = clean.strip_prefix("```").unwrap_or(clean);
        let trimmed = trimmed.strip_prefix("json").unwrap_or(trimmed);
        trimmed.strip_suffix("```").unwrap_or(trimmed).trim()
    } else {
        clean
    };
    if let Ok(action) = serde_json::from_str::<ComputerAction>(clean) {
        return Some(action);
    }
    normalize_action_object(clean).and_then(|value| serde_json::from_value(value).ok())
}

/// Accepts both strict and relaxed JSON emitted by vision CLIs. A relaxed body such as
/// `{action:mouse_move,x:960,y:540}` (unquoted keys and string values) is quoted into strict
/// JSON, then the `action` discriminator is renamed to the `type` this enum deserializes on.
fn normalize_action_object(json_str: &str) -> Option<serde_json::Value> {
    let mut value = serde_json::from_str::<serde_json::Value>(json_str)
        .ok()
        .or_else(|| {
            quote_relaxed_json(json_str).and_then(|strict| serde_json::from_str(&strict).ok())
        })?;
    if let Some(object) = value.as_object_mut() {
        if let Some(action) = object.remove("action") {
            object.insert("type".to_owned(), action);
        }
    }
    Some(value)
}

/// Quotes the unquoted keys and string values of a relaxed JSON object, copying quoted
/// strings and numbers, booleans and null through verbatim.
fn quote_relaxed_json(input: &str) -> Option<String> {
    let mut out = String::with_capacity(input.len() + 16);
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                out.push(c);
                i += 1;
                while i < chars.len() {
                    let e = chars[i];
                    out.push(e);
                    i += 1;
                    if e == '\\' {
                        if i < chars.len() {
                            out.push(chars[i]);
                            i += 1;
                        }
                    } else if e == '"' {
                        break;
                    }
                }
            }
            '{' | '}' | '[' | ']' | ',' | ':' => {
                out.push(c);
                i += 1;
            }
            c if c.is_whitespace() => {
                out.push(c);
                i += 1;
            }
            _ => {
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], ',' | '}' | ']' | ':')
                {
                    i += 1;
                }
                let token: String = chars[start..i].iter().collect();
                if token.is_empty() {
                    return None;
                }
                if token == "true"
                    || token == "false"
                    || token == "null"
                    || token.parse::<f64>().is_ok()
                {
                    out.push_str(&token);
                } else {
                    out.push('"');
                    out.push_str(&token.replace('\\', "\\\\").replace('"', "\\\""));
                    out.push('"');
                }
            }
        }
    }
    Some(out)
}

#[must_use]
pub fn provider_vision_matrix() -> Vec<ProviderVisionCapability> {
    [
        (
            "claude",
            "Claude Code",
            true,
            "Vision input · Read-only decision tools",
        ),
        (
            "codex",
            "Codex CLI",
            true,
            "Native image input · Read-only sandbox",
        ),
        (
            "grok",
            "Grok CLI",
            true,
            "Vision input · Read-only decision tools",
        ),
        ("opencode", "OpenCode", false, "Not authorised by ADR-0015"),
        (
            "ollama",
            "Ollama (Local)",
            false,
            "Not authorised by ADR-0015",
        ),
        ("demo", "Offline Demo", false, "No live desktop access"),
    ]
    .into_iter()
    .map(|(id, label, allowed, note)| ProviderVisionCapability {
        id: id.to_owned(),
        label: label.to_owned(),
        vision_supported: allowed,
        computer_use_allowed: allowed,
        note: note.to_owned(),
    })
    .collect()
}

#[must_use]
pub fn is_provider_authorized(provider_id: &str) -> bool {
    matches!(provider_id, "claude" | "codex" | "grok")
}

#[must_use]
pub fn is_vision_capable(provider_id: &str, model: Option<&str>) -> bool {
    if !is_provider_authorized(provider_id) {
        return false;
    }
    !model.is_some_and(|model| {
        let model = model.to_ascii_lowercase();
        model.starts_with("text-") || model.contains("text-only")
    })
}

/// Conservative intent gate: discussing the feature is not permission to use the desktop.
#[must_use]
pub fn explicitly_requests_computer_use(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("/computer") {
        let command_boundary =
            rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace);
        if command_boundary {
            return true;
        }
    }
    let development_discussion = [
        "feature",
        "implement",
        "implementation",
        "build",
        "code",
        "bug",
        "debug",
        "not working",
        "doesn't work",
        "does not work",
        "trying to add",
        "adding computer use",
        "computer use feature",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    if development_discussion {
        return false;
    }
    let direct_request = [
        "use computer",
        "use computer use",
        "use computer vision",
        "use the computer",
        "use my computer",
        "use my pc",
        "use the pc",
        "use pc",
        "access my computer",
        "access my pc",
        "access the computer",
        "access the pc",
        "control my computer",
        "control my pc",
        "control the pc",
        "control the computer",
        "control windows",
        "control desktop",
        "control the desktop",
        "operate my computer",
        "operate my pc",
        "operate the pc",
        "operate the computer",
        "take control of my computer",
        "take control of my pc",
        "take control of the pc",
        "take over my computer",
        "take over my pc",
        "take over the pc",
        "drive my computer",
        "drive my pc",
        "use the mouse",
        "use my mouse",
        "move the mouse",
        "move my mouse",
        "control the mouse",
        "click on my screen",
        "click on the screen",
        "click the mouse",
        "click on my desktop",
        "type on my computer",
        "type on my pc",
        "scroll on my screen",
        "scroll the screen",
        "on my desktop",
        "on my screen and click",
        "using computer use",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase));
    if direct_request {
        return true;
    }
    // Secondary heuristic: an explicit desktop-control verb joined to a desktop object,
    // e.g. "move the mouse to the center", "double-click that on my desktop".
    let action_verb = [
        "click",
        "clicking",
        "double-click",
        "right-click",
        "scroll",
        "drag",
        "type",
        "press",
        "move the mouse",
        "move my mouse",
        "use the mouse",
        "use my mouse",
        "control the mouse",
        "control the cursor",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    let desktop_object = [
        "screen",
        "desktop",
        "mouse",
        "cursor",
        "on my computer",
        "on my pc",
        "on the computer",
        "on the pc",
        "taskbar",
        "start menu",
        "file explorer",
        "notepad",
        "calculator",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    action_verb && desktop_object
}

/// The scope everything in this module operates in (GAD-012, INV-089).
///
/// Named rather than implied, because the two Computer Use scopes must not leak into each
/// other. This module is the **desktop** one: explicit, user-initiated, entered from
/// `/computer` or the composer toggle and never from a build run, a playtest or a plan
/// approval. The engine's scope is [`crate::computer_window::EngineCaptureScope`], whose only
/// constructor takes a window handle and which therefore has no arm that widens to this one.
#[must_use]
pub fn desktop_scope() -> crate::computer_window::CaptureScope {
    crate::computer_window::CaptureScope::Desktop
}

/// Captures the complete Windows virtual desktop, including monitors with negative origins.
///
/// The desktop-wide entry point. An engine observation must never reach it: it takes an
/// [`EngineCaptureScope`](crate::computer_window::EngineCaptureScope), which cannot name the
/// desktop, and `godot_observe` carries a test that its own source does not name this function.
pub async fn capture_screen() -> Result<ScreenCapture, String> {
    #[cfg(windows)]
    {
        let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$b = [System.Windows.Forms.SystemInformation]::VirtualScreen

$b64 = $null
try {
    $src = @'
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
public class BhippiCaptureHelper {
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenInputDesktop(uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenDesktop(string lpszDesktop, uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll")]
  public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SetThreadDesktop(IntPtr hDesktop);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr GetDC(IntPtr hWnd);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern int ReleaseDC(IntPtr hWnd, IntPtr hDC);
  [DllImport("gdi32.dll", SetLastError=true)]
  public static extern bool BitBlt(IntPtr hObject, int nXDest, int nYDest, int nWidth, int nHeight, IntPtr hObjectSource, int nXSrc, int nYSrc, uint dwRop);
  [DllImport("gdi32.dll", SetLastError=true)]
  public static extern IntPtr CreateCompatibleBitmap(IntPtr hDC, int nWidth, int nHeight);
  [DllImport("gdi32.dll", SetLastError=true)]
  public static extern IntPtr CreateCompatibleDC(IntPtr hDC);
  [DllImport("gdi32.dll", SetLastError=true)]
  public static extern bool DeleteDC(IntPtr hDC);
  [DllImport("gdi32.dll", SetLastError=true)]
  public static extern bool DeleteObject(IntPtr hObject);
  [DllImport("gdi32.dll", SetLastError=true)]
  public static extern IntPtr SelectObject(IntPtr hDC, IntPtr hObject);

  public static string Capture(int x, int y, int w, int h) {
    SetProcessDPIAware();
    string res = null;
    var t = new System.Threading.Thread(() => {
      IntPtr hDesk = OpenInputDesktop(0, false, 0x01FF);
      if (hDesk == IntPtr.Zero) hDesk = OpenDesktop("default", 0, false, 0x01FF);
      if (hDesk != IntPtr.Zero) SetThreadDesktop(hDesk);
      IntPtr hSrc = GetDC(IntPtr.Zero);
      if (hSrc == IntPtr.Zero) return;
      IntPtr hDest = CreateCompatibleDC(hSrc);
      IntPtr hBmp = CreateCompatibleBitmap(hSrc, w, h);
      IntPtr hOld = SelectObject(hDest, hBmp);
      bool ok = BitBlt(hDest, 0, 0, w, h, hSrc, x, y, 0x00CC0020);
      SelectObject(hDest, hOld);
      DeleteDC(hDest);
      ReleaseDC(IntPtr.Zero, hSrc);
      if (ok) {
        using (Bitmap bmp = Image.FromHbitmap(hBmp)) {
          using (var ms = new System.IO.MemoryStream()) {
            bmp.Save(ms, ImageFormat.Jpeg);
            res = Convert.ToBase64String(ms.ToArray());
          }
        }
      }
      DeleteObject(hBmp);
    });
    t.SetApartmentState(System.Threading.ApartmentState.STA);
    t.Start();
    t.Join();
    return res;
  }
}
'@
    Add-Type -TypeDefinition $src -ReferencedAssemblies System.Drawing -ErrorAction SilentlyContinue
    $b64 = [BhippiCaptureHelper]::Capture($b.Left, $b.Top, $b.Width, $b.Height)
} catch {}

if ([string]::IsNullOrEmpty($b64)) {
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen((New-Object System.Drawing.Point($b.Left, $b.Top)), [System.Drawing.Point]::Empty, $b.Size)
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Jpeg)
    $b64 = [Convert]::ToBase64String($ms.ToArray())
    $g.Dispose()
    $bmp.Dispose()
    $ms.Dispose()
}

Write-Output "$($b.Left)|$($b.Top)|$($b.Width)|$($b.Height)|$b64"
"#;
        let output = run_powershell_output(script).await?;
        let parts: Vec<&str> = output.trim().splitn(5, '|').collect();
        if parts.len() != 5 {
            return Err("Screen capture returned malformed coordinate metadata.".to_owned());
        }
        Ok(ScreenCapture {
            origin_x: parse_number(parts[0], "screen origin x")?,
            origin_y: parse_number(parts[1], "screen origin y")?,
            width: parse_number(parts[2], "screen width")?,
            height: parse_number(parts[3], "screen height")?,
            image_base64: parts[4].trim().to_owned(),
            captured_at: Utc::now(),
        })
    }
    #[cfg(not(windows))]
    {
        Err("Computer Use screen capture is currently available on Windows only.".to_owned())
    }
}

pub async fn save_capture(capture: &ScreenCapture, turn_id: &str) -> Result<PathBuf, String> {
    let safe_id: String = turn_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .take(80)
        .collect();
    let directory = std::env::temp_dir().join("bhippi-computer-use");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| format!("Could not prepare the screenshot directory: {error}"))?;
    let stem = if safe_id.is_empty() {
        "capture"
    } else {
        &safe_id
    };
    let path = directory.join(format!("{stem}.jpg"));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&capture.image_base64)
        .map_err(|error| format!("Could not decode the desktop screenshot: {error}"))?;
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| format!("Could not write the desktop screenshot: {error}"))?;
    Ok(path)
}

pub async fn remove_capture(path: &Path) {
    let _ignored = tokio::fs::remove_file(path).await;
}

pub async fn screen_bounds() -> Result<ScreenBounds, String> {
    #[cfg(windows)]
    {
        let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$b = [System.Windows.Forms.SystemInformation]::VirtualScreen
Write-Output "$($b.Left)|$($b.Top)|$($b.Width)|$($b.Height)"
"#;
        let output = run_powershell_output(script).await?;
        let parts: Vec<&str> = output.trim().split('|').collect();
        if parts.len() != 4 {
            return Err("Desktop bounds returned malformed coordinate metadata.".to_owned());
        }
        Ok(ScreenBounds {
            origin_x: parse_number(parts[0], "screen origin x")?,
            origin_y: parse_number(parts[1], "screen origin y")?,
            width: parse_number(parts[2], "screen width")?,
            height: parse_number(parts[3], "screen height")?,
        })
    }
    #[cfg(not(windows))]
    {
        Err("Computer Use is currently available on Windows only.".to_owned())
    }
}

pub async fn execute_action(action: ComputerAction) -> Result<ComputerActionResult, String> {
    let bounds = screen_bounds().await?;
    action.validate(bounds)?;
    match action {
        ComputerAction::Screenshot => {
            let capture = capture_screen().await?;
            Ok(result(
                "screenshot",
                format!("Captured desktop at {}x{}.", capture.width, capture.height),
            )
            .with_screenshot(capture))
        }
        ComputerAction::GetScreenSize => Ok(result(
            "get_screen_size",
            format!(
                "Desktop bounds: origin ({}, {}), size {}x{}.",
                bounds.origin_x, bounds.origin_y, bounds.width, bounds.height
            ),
        )
        .with_screen(bounds)),
        ComputerAction::GetCursorPosition => {
            let (x, y) = cursor_position().await?;
            Ok(result(
                "get_cursor_position",
                format!("Cursor position: ({x}, {y})."),
            )
            .with_cursor(x, y))
        }
        ComputerAction::MouseMove { x, y } => {
            set_cursor_position(x, y).await?;
            Ok(result("mouse_move", format!("Moved cursor to ({x}, {y}).")).with_cursor(x, y))
        }
        ComputerAction::MouseClick {
            button,
            count,
            x,
            y,
        } => {
            if let (Some(x), Some(y)) = (x, y) {
                set_cursor_position(x, y).await?;
            }
            mouse_click(&button, count).await?;
            let (cursor_x, cursor_y) = cursor_position().await?;
            Ok(result(
                "mouse_click",
                format!("Clicked {button} button {count} time(s) at ({cursor_x}, {cursor_y})."),
            )
            .with_cursor(cursor_x, cursor_y))
        }
        ComputerAction::MouseDrag {
            start_x,
            start_y,
            end_x,
            end_y,
        } => {
            mouse_drag(start_x, start_y, end_x, end_y).await?;
            Ok(result(
                "mouse_drag",
                format!("Dragged from ({start_x}, {start_y}) to ({end_x}, {end_y})."),
            )
            .with_cursor(end_x, end_y))
        }
        ComputerAction::MouseScroll { delta_x, delta_y } => {
            mouse_scroll(delta_x, delta_y).await?;
            Ok(result(
                "mouse_scroll",
                format!("Scrolled horizontally by {delta_x} and vertically by {delta_y}."),
            ))
        }
        ComputerAction::TypeText { text } => {
            type_text(&text).await?;
            Ok(result(
                "type_text",
                format!("Typed {} characters.", text.chars().count()),
            ))
        }
        ComputerAction::KeyPress { key } => {
            let code =
                virtual_key(&key).ok_or_else(|| format!("Unsupported keyboard key: {key}"))?;
            send_virtual_keys(&[code]).await?;
            Ok(result("key_press", format!("Pressed key: {key}.")))
        }
        ComputerAction::Hotkey { keys } => {
            let codes: Result<Vec<u8>, String> = keys
                .iter()
                .map(|key| {
                    virtual_key(key).ok_or_else(|| format!("Unsupported keyboard key: {key}"))
                })
                .collect();
            send_virtual_keys(&codes?).await?;
            Ok(result(
                "hotkey",
                format!("Pressed hotkey: {}.", keys.join("+")),
            ))
        }
        ComputerAction::OpenApp { target } => {
            open_target(&target).await?;
            Ok(result(
                "open_app",
                format!("Opened {target}. Give it a moment, then look for its window."),
            ))
        }
        ComputerAction::OpenUrl { url } => {
            open_target(&url).await?;
            Ok(result(
                "open_url",
                format!("Opened {url} in the default browser."),
            ))
        }
        ComputerAction::FocusWindow { title } => {
            let filter = crate::computer_window::WindowFilter {
                title_contains: Some(title.clone()),
                ..crate::computer_window::WindowFilter::default()
            };
            let window = crate::computer_window::find_window(filter)
                .await
                .map_err(|error| format!("{error} {}", error.hint()))?;
            crate::computer_window::focus_window(&window)
                .await
                .map_err(|error| format!("{error} {}", error.hint()))?;
            Ok(result(
                "focus_window",
                format!(
                    "Focused \"{}\" ({}x{} at {}, {}).",
                    window.title,
                    window.rect.width,
                    window.rect.height,
                    window.rect.x,
                    window.rect.y
                ),
            ))
        }
        ComputerAction::ListWindows => {
            let windows = crate::computer_window::find_windows(
                crate::computer_window::WindowFilter::default(),
            )
            .await
            .map_err(|error| format!("{error} {}", error.hint()))?;
            let mut lines: Vec<String> = windows
                .iter()
                .filter(|window| !window.title.trim().is_empty())
                .take(MAX_LISTED_WINDOWS)
                .map(|window| {
                    format!(
                        "\"{}\" [{}] {}x{} at ({}, {})",
                        window.title,
                        window.class_name,
                        window.rect.width,
                        window.rect.height,
                        window.rect.x,
                        window.rect.y
                    )
                })
                .collect();
            if lines.is_empty() {
                lines.push("no titled windows".to_owned());
            }
            Ok(result(
                "list_windows",
                format!("Open windows:\n{}", lines.join("\n")),
            ))
        }
        ComputerAction::Wait { ms } => {
            tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms))).await;
            Ok(result("wait", format!("Waited {ms} ms.")))
        }
    }
}

/// Opens a target the way Explorer's Run box would: a program on PATH, an `.exe`, a
/// document, a folder or a URL. Windows' own association lookup does the work (`start`
/// through `cmd`), and the target rides as one argument — never interpolated into a shell
/// line, which is what keeps `&` in a URL a character and not a command.
async fn open_target(target: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let target = target.trim().trim_matches('"').to_owned();
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let status = tokio::process::Command::new("cmd.exe")
            .args(["/c", "start", "", &target])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map_err(|error| format!("Could not start `{target}`: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "Windows could not open `{target}` (exit {status})."
            ))
        }
    }
    #[cfg(not(windows))]
    {
        let _unused = target;
        Err("Computer Use is currently available on Windows only.".to_owned())
    }
}

fn result(action: &str, detail: String) -> ComputerActionResult {
    ComputerActionResult {
        success: true,
        action: action.to_owned(),
        detail,
        screenshot: None,
        cursor: None,
        screen_size: None,
        screen_origin: None,
    }
}

trait ResultFields {
    fn with_cursor(self, x: i32, y: i32) -> Self;
    fn with_screen(self, bounds: ScreenBounds) -> Self;
    fn with_screenshot(self, capture: ScreenCapture) -> Self;
}

impl ResultFields for ComputerActionResult {
    fn with_cursor(mut self, x: i32, y: i32) -> Self {
        self.cursor = Some((x, y));
        self
    }

    fn with_screen(mut self, bounds: ScreenBounds) -> Self {
        self.screen_size = Some((bounds.width, bounds.height));
        self.screen_origin = Some((bounds.origin_x, bounds.origin_y));
        self
    }

    fn with_screenshot(mut self, capture: ScreenCapture) -> Self {
        self.screenshot = Some(capture);
        self
    }
}

#[cfg(windows)]
async fn cursor_position() -> Result<(i32, i32), String> {
    let output = run_powershell_output(
        r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$res = $null
try {
    $src = @'
using System;
using System.Runtime.InteropServices;
public class BhippiCursorRead {
  [StructLayout(LayoutKind.Sequential)]
  public struct POINT { public int X; public int Y; }
  [DllImport("user32.dll")]
  public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenInputDesktop(uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenDesktop(string lpszDesktop, uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SetThreadDesktop(IntPtr hDesktop);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool GetCursorPos(out POINT lpPoint);

  public static string Read() {
    SetProcessDPIAware();
    string ptStr = null;
    var t = new System.Threading.Thread(() => {
      IntPtr hDesk = OpenInputDesktop(0, false, 0x01FF);
      if (hDesk == IntPtr.Zero) hDesk = OpenDesktop("default", 0, false, 0x01FF);
      if (hDesk != IntPtr.Zero) SetThreadDesktop(hDesk);
      POINT pt;
      if (GetCursorPos(out pt)) {
        ptStr = pt.X + "|" + pt.Y;
      }
    });
    t.SetApartmentState(System.Threading.ApartmentState.STA);
    t.Start();
    t.Join();
    return ptStr;
  }
}
'@
    Add-Type -TypeDefinition $src -ErrorAction SilentlyContinue
    $res = [BhippiCursorRead]::Read()
} catch {}

if ([string]::IsNullOrEmpty($res)) {
    $p = [System.Windows.Forms.Cursor]::Position
    $res = "$($p.X)|$($p.Y)"
}
Write-Output $res
"#,
    )
    .await?;
    let parts: Vec<&str> = output.trim().split('|').collect();
    if parts.len() != 2 {
        return Err("Cursor position returned malformed coordinates.".to_owned());
    }
    Ok((
        parse_number(parts[0], "cursor x")?,
        parse_number(parts[1], "cursor y")?,
    ))
}

#[cfg(not(windows))]
async fn cursor_position() -> Result<(i32, i32), String> {
    Err("Computer Use is currently available on Windows only.".to_owned())
}

#[cfg(windows)]
async fn set_cursor_position(x: i32, y: i32) -> Result<(), String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$res = $null
try {{
    $src = @'
using System;
using System.Runtime.InteropServices;
public class BhippiCursorMove {{
  [StructLayout(LayoutKind.Sequential)]
  public struct POINT {{ public int X; public int Y; }}
  [DllImport("user32.dll")]
  public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenInputDesktop(uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenDesktop(string lpszDesktop, uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SetThreadDesktop(IntPtr hDesktop);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool GetCursorPos(out POINT lpPoint);

  public static string Move(int x, int y) {{
    SetProcessDPIAware();
    string ptStr = null;
    var t = new System.Threading.Thread(() => {{
      IntPtr hDesk = OpenInputDesktop(0, false, 0x01FF);
      if (hDesk == IntPtr.Zero) hDesk = OpenDesktop("default", 0, false, 0x01FF);
      if (hDesk != IntPtr.Zero) SetThreadDesktop(hDesk);

      SetCursorPos(x, y);
      System.Threading.Thread.Sleep(30);
      POINT pt;
      if (GetCursorPos(out pt)) {{
        ptStr = pt.X + "|" + pt.Y;
      }}
    }});
    t.SetApartmentState(System.Threading.ApartmentState.STA);
    t.Start();
    t.Join();
    return ptStr;
  }}
}}
'@
    Add-Type -TypeDefinition $src -ErrorAction SilentlyContinue
    $res = [BhippiCursorMove]::Move({x}, {y})
}} catch {{}}

if ([string]::IsNullOrEmpty($res)) {{
    [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point({x}, {y})
    Start-Sleep -Milliseconds 30
    $p = [System.Windows.Forms.Cursor]::Position
    $res = "$($p.X)|$($p.Y)"
}}
Write-Output $res
"#
    );
    let output = run_powershell_output(&script).await?;
    let parts: Vec<&str> = output.trim().split('|').collect();
    let observed: (i32, i32) = if parts.len() == 2 {
        (
            parse_number(parts[0], "cursor x")?,
            parse_number(parts[1], "cursor y")?,
        )
    } else {
        return Err("Cursor move returned malformed verification coordinates.".to_owned());
    };
    let diff_x = (observed.0 - x).abs();
    let diff_y = (observed.1 - y).abs();
    if diff_x <= 15 && diff_y <= 15 {
        Ok(())
    } else {
        Err(format!(
            "Windows reported cursor position ({}, {}) after a move to ({x}, {y}).",
            observed.0, observed.1
        ))
    }
}

#[cfg(not(windows))]
async fn set_cursor_position(_x: i32, _y: i32) -> Result<(), String> {
    Err("Computer Use is currently available on Windows only.".to_owned())
}

/// Safe no-op: Bhippi must never blank the Windows OS-wide system cursor, as any failure,
/// abnormal termination, or click capture leaves the entire operating system without a visible pointer.
pub async fn blank_system_cursor() -> Result<(), String> {
    Ok(())
}

/// Reloads the user's real cursor scheme (SPI_SETCURSORS), reversing any blanking. Safe to
/// call any time — an untouched scheme is simply re-applied.
pub async fn restore_system_cursor() {
    #[cfg(windows)]
    {
        let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$signature = @'
using System;
using System.Runtime.InteropServices;
public class BhippiCursorRestore {
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SystemParametersInfo(uint action, uint param, IntPtr data, uint flags);
}
'@
Add-Type -TypeDefinition $signature
# SPI_SETCURSORS (0x57) with SPIF_UPDATEINIFILE | SPIF_SENDCHANGE reloads the cursor scheme.
[BhippiCursorRestore]::SystemParametersInfo(0x57, 0, [IntPtr]::Zero, 0x03)
"#;
        let _ignored = run_powershell(script).await;
    }
}

#[cfg(windows)]
async fn mouse_click(button: &str, count: u32) -> Result<(), String> {
    let (down, up) = match button.to_ascii_lowercase().as_str() {
        "left" => (0x0002_u32, 0x0004_u32),
        "right" => (0x0008_u32, 0x0010_u32),
        "middle" => (0x0020_u32, 0x0040_u32),
        _ => return Err("Mouse button must be left, right, or middle.".to_owned()),
    };
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$signature = @'
using System;
using System.Runtime.InteropServices;
public class BhippiMouseClick {{
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenInputDesktop(uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenDesktop(string lpszDesktop, uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SetThreadDesktop(IntPtr hDesktop);
  [DllImport("user32.dll")]
  public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);

  public static void Click(uint down, uint up, int count) {{
    var t = new System.Threading.Thread(() => {{
      IntPtr hDesk = OpenInputDesktop(0, false, 0x01FF);
      if (hDesk == IntPtr.Zero) hDesk = OpenDesktop("default", 0, false, 0x01FF);
      if (hDesk != IntPtr.Zero) SetThreadDesktop(hDesk);
      for (int i = 0; i < count; i++) {{
        mouse_event(down, 0, 0, 0, UIntPtr.Zero);
        mouse_event(up, 0, 0, 0, UIntPtr.Zero);
        if (i < count - 1) System.Threading.Thread.Sleep(70);
      }}
    }});
    t.SetApartmentState(System.Threading.ApartmentState.STA);
    t.Start();
    t.Join();
  }}
}}
'@
Add-Type -TypeDefinition $signature -ErrorAction SilentlyContinue
[BhippiMouseClick]::Click({down}, {up}, {count})
"#
    );
    run_powershell(&script).await
}

#[cfg(not(windows))]
async fn mouse_click(_button: &str, _count: u32) -> Result<(), String> {
    Err("Computer Use is currently available on Windows only.".to_owned())
}

#[cfg(windows)]
async fn mouse_drag(start_x: i32, start_y: i32, end_x: i32, end_y: i32) -> Result<(), String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$signature = @'
using System;
using System.Runtime.InteropServices;
public class BhippiMouseDrag {{
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenInputDesktop(uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenDesktop(string lpszDesktop, uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SetThreadDesktop(IntPtr hDesktop);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")]
  public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);

  public static void Drag(int sx, int sy, int ex, int ey) {{
    var t = new System.Threading.Thread(() => {{
      IntPtr hDesk = OpenInputDesktop(0, false, 0x01FF);
      if (hDesk == IntPtr.Zero) hDesk = OpenDesktop("default", 0, false, 0x01FF);
      if (hDesk != IntPtr.Zero) SetThreadDesktop(hDesk);
      SetCursorPos(sx, sy);
      System.Threading.Thread.Sleep(60);
      mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
      System.Threading.Thread.Sleep(60);
      SetCursorPos(ex, ey);
      System.Threading.Thread.Sleep(80);
      mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
    }});
    t.SetApartmentState(System.Threading.ApartmentState.STA);
    t.Start();
    t.Join();
  }}
}}
'@
Add-Type -TypeDefinition $signature -ErrorAction SilentlyContinue
[BhippiMouseDrag]::Drag({start_x}, {start_y}, {end_x}, {end_y})
"#
    );
    run_powershell(&script).await?;
    let pos = cursor_position().await?;
    if (pos.0 - end_x).abs() <= 15 && (pos.1 - end_y).abs() <= 15 {
        Ok(())
    } else {
        Err("Cursor did not reach the requested drag endpoint.".to_owned())
    }
}

#[cfg(not(windows))]
async fn mouse_drag(_start_x: i32, _start_y: i32, _end_x: i32, _end_y: i32) -> Result<(), String> {
    Err("Computer Use is currently available on Windows only.".to_owned())
}

#[cfg(windows)]
async fn mouse_scroll(delta_x: i32, delta_y: i32) -> Result<(), String> {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$signature = @'
using System;
using System.Runtime.InteropServices;
public class BhippiMouseScroll {{
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenInputDesktop(uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenDesktop(string lpszDesktop, uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SetThreadDesktop(IntPtr hDesktop);
  [DllImport("user32.dll")]
  public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);

  public static void Scroll(int dx, int dy) {{
    var t = new System.Threading.Thread(() => {{
      IntPtr hDesk = OpenInputDesktop(0, false, 0x01FF);
      if (hDesk == IntPtr.Zero) hDesk = OpenDesktop("default", 0, false, 0x01FF);
      if (hDesk != IntPtr.Zero) SetThreadDesktop(hDesk);
      if (dy != 0) mouse_event(0x0800, 0, 0, (uint)dy, UIntPtr.Zero);
      if (dx != 0) mouse_event(0x1000, 0, 0, (uint)dx, UIntPtr.Zero);
    }});
    t.SetApartmentState(System.Threading.ApartmentState.STA);
    t.Start();
    t.Join();
  }}
}}
'@
Add-Type -TypeDefinition $signature -ErrorAction SilentlyContinue
[BhippiMouseScroll]::Scroll({delta_x}, {delta_y})
"#
    );
    run_powershell(&script).await
}

#[cfg(not(windows))]
async fn mouse_scroll(_delta_x: i32, _delta_y: i32) -> Result<(), String> {
    Err("Computer Use is currently available on Windows only.".to_owned())
}

#[cfg(windows)]
async fn type_text(text: &str) -> Result<(), String> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$signature = @'
using System;
using System.Runtime.InteropServices;
using System.Windows.Forms;
public class BhippiText {{
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenInputDesktop(uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenDesktop(string lpszDesktop, uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SetThreadDesktop(IntPtr hDesktop);

  public static void SendText(string text) {{
    var t = new System.Threading.Thread(() => {{
      IntPtr hDesk = OpenInputDesktop(0, false, 0x01FF);
      if (hDesk == IntPtr.Zero) hDesk = OpenDesktop("default", 0, false, 0x01FF);
      if (hDesk != IntPtr.Zero) SetThreadDesktop(hDesk);
      SendKeys.SendWait(text);
    }});
    t.SetApartmentState(System.Threading.ApartmentState.STA);
    t.Start();
    t.Join();
  }}
}}
'@
Add-Type -TypeDefinition $signature -ReferencedAssemblies System.Windows.Forms -ErrorAction SilentlyContinue
$raw = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded}'))
$escaped = [regex]::Replace($raw, '([+^%~(){{}}\[\]])', '{{$1}}')
$escaped = $escaped.Replace("`r`n", '{{ENTER}}').Replace("`n", '{{ENTER}}').Replace("`t", '{{TAB}}')
[BhippiText]::SendText($escaped)
"#
    );
    run_powershell(&script).await
}

#[cfg(not(windows))]
async fn type_text(_text: &str) -> Result<(), String> {
    Err("Computer Use is currently available on Windows only.".to_owned())
}

#[cfg(windows)]
async fn send_virtual_keys(codes: &[u8]) -> Result<(), String> {
    let codes = codes
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$signature = @'
using System;
using System.Runtime.InteropServices;
public class BhippiKeyboard {{
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenInputDesktop(uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr OpenDesktop(string lpszDesktop, uint dwFlags, bool fInherit, uint dwDesiredAccess);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern bool SetThreadDesktop(IntPtr hDesktop);
  [DllImport("user32.dll")]
  public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);

  public static void SendKeys(byte[] codes) {{
    var t = new System.Threading.Thread(() => {{
      IntPtr hDesk = OpenInputDesktop(0, false, 0x01FF);
      if (hDesk == IntPtr.Zero) hDesk = OpenDesktop("default", 0, false, 0x01FF);
      if (hDesk != IntPtr.Zero) SetThreadDesktop(hDesk);
      foreach (byte code in codes) {{
        keybd_event(code, 0, 0, UIntPtr.Zero);
      }}
      System.Threading.Thread.Sleep(40);
      Array.Reverse(codes);
      foreach (byte code in codes) {{
        keybd_event(code, 0, 2, UIntPtr.Zero);
      }}
    }});
    t.SetApartmentState(System.Threading.ApartmentState.STA);
    t.Start();
    t.Join();
  }}
}}
'@
Add-Type -TypeDefinition $signature -ErrorAction SilentlyContinue
[BhippiKeyboard]::SendKeys([byte[]]@({codes}))
"#
    );
    run_powershell(&script).await
}

#[cfg(not(windows))]
async fn send_virtual_keys(_codes: &[u8]) -> Result<(), String> {
    Err("Computer Use is currently available on Windows only.".to_owned())
}

/// Shared with `computer_window`, so a window-targeted key and a desktop-wide key are the same
/// key. Godot's `KEY_W` spellings reduce to these names once the prefix is stripped.
pub(crate) fn virtual_key(key: &str) -> Option<u8> {
    let lower = key.trim().to_ascii_lowercase();
    let named = match lower.as_str() {
        "backspace" => 0x08,
        "tab" => 0x09,
        "enter" | "return" => 0x0D,
        "shift" => 0x10,
        "ctrl" | "control" => 0x11,
        "alt" => 0x12,
        "escape" | "esc" => 0x1B,
        "space" => 0x20,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "insert" => 0x2D,
        "delete" | "del" => 0x2E,
        "win" | "windows" | "super" | "meta" => 0x5B,
        "f1" => 0x70,
        "f2" => 0x71,
        "f3" => 0x72,
        "f4" => 0x73,
        "f5" => 0x74,
        "f6" => 0x75,
        "f7" => 0x76,
        "f8" => 0x77,
        "f9" => 0x78,
        "f10" => 0x79,
        "f11" => 0x7A,
        "f12" => 0x7B,
        _ => 0,
    };
    if named != 0 {
        return Some(named);
    }
    let bytes = lower.as_bytes();
    if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
        Some(bytes[0].to_ascii_uppercase())
    } else {
        None
    }
}

#[cfg(windows)]
fn parse_number<T>(text: &str, name: &str) -> Result<T, String>
where
    T: std::str::FromStr,
{
    text.trim()
        .parse()
        .map_err(|_| format!("Could not parse {name} from the Windows response."))
}

#[cfg(windows)]
async fn run_powershell(script: &str) -> Result<(), String> {
    run_powershell_output(script).await.map(|_| ())
}

/// The one shim. `computer_window` runs its window-targeted scripts through this same fixed-argv,
/// `CREATE_NO_WINDOW` invocation so both surfaces fail and time out identically.
#[cfg(windows)]
pub(crate) async fn run_powershell_output(script: &str) -> Result<String, String> {
    let mut command = tokio::process::Command::new("powershell");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
    let output = tokio::time::timeout(POWERSHELL_TIMEOUT, command.output())
        .await
        .map_err(|_| "The Windows input bridge timed out after 10 seconds.".to_owned())?
        .map_err(|error| format!("Could not start the Windows input bridge: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if detail.is_empty() {
            format!("The Windows input bridge exited with {}.", output.status)
        } else {
            format!("The Windows input bridge failed: {detail}")
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> ScreenBounds {
        ScreenBounds {
            origin_x: -1280,
            origin_y: 0,
            width: 3200,
            height: 1080,
        }
    }

    #[test]
    fn authorization_is_exactly_the_adr_provider_set() {
        for provider in ["claude", "codex", "grok"] {
            assert!(is_provider_authorized(provider));
            assert!(is_vision_capable(provider, None));
        }
        for provider in ["opencode", "ollama", "demo", "kimi", "gemini"] {
            assert!(!is_provider_authorized(provider));
            assert!(!is_vision_capable(provider, Some("gpt-4o")));
        }
    }

    #[test]
    fn reach_actions_validate_and_only_the_observing_ones_skip_full_access() {
        assert!(ComputerAction::OpenApp {
            target: "notepad".to_owned()
        }
        .validate(bounds())
        .is_ok());
        assert!(ComputerAction::OpenApp {
            target: "  ".to_owned()
        }
        .validate(bounds())
        .is_err());
        assert!(ComputerAction::OpenUrl {
            url: "https://example.com/?a=1&b=2".to_owned()
        }
        .validate(bounds())
        .is_ok());
        assert!(ComputerAction::OpenUrl {
            url: "ftp://example.com".to_owned()
        }
        .validate(bounds())
        .is_err());
        assert!(ComputerAction::Wait { ms: 10_000 }
            .validate(bounds())
            .is_ok());
        assert!(ComputerAction::Wait { ms: 10_001 }
            .validate(bounds())
            .is_err());
        assert!(ComputerAction::FocusWindow {
            title: String::new()
        }
        .validate(bounds())
        .is_err());
        assert!(!ComputerAction::ListWindows.requires_full_access());
        assert!(!ComputerAction::Wait { ms: 1 }.requires_full_access());
        assert!(ComputerAction::OpenApp {
            target: "notepad".to_owned()
        }
        .requires_full_access());
        assert!(ComputerAction::FocusWindow {
            title: "Godot".to_owned()
        }
        .requires_full_access());
        assert_eq!(
            parse_action_json(r#"{"action":"open_app","target":"notepad"}"#),
            Some(ComputerAction::OpenApp {
                target: "notepad".to_owned()
            })
        );
        assert_eq!(
            parse_action_json(r#"{action:list_windows}"#),
            Some(ComputerAction::ListWindows)
        );
    }

    #[test]
    fn action_parser_accepts_action_and_type_discriminators() {
        let action = parse_action_json(
            r#"{"action":"mouse_click","button":"left","count":2,"x":-100,"y":200}"#,
        );
        assert_eq!(
            action,
            Some(ComputerAction::MouseClick {
                button: "left".to_owned(),
                count: 2,
                x: Some(-100),
                y: Some(200),
            })
        );
        assert_eq!(
            parse_action_json(r#"{"type":"hotkey","keys":["ctrl","c"]}"#),
            Some(ComputerAction::Hotkey {
                keys: vec!["ctrl".to_owned(), "c".to_owned()]
            })
        );
        assert_eq!(
            extract_actions(
                "before\n<computer_action>\n{action:mouse_move,x:960,y:540}\n</computer_action>\nafter"
            ),
            vec![ComputerAction::MouseMove { x: 960, y: 540 }]
        );
    }

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

    #[test]
    fn action_parser_accepts_relaxed_json_that_models_emit() {
        assert_eq!(
            parse_action_json(r#"{action:mouse_move,x:960,y:540}"#),
            Some(ComputerAction::MouseMove { x: 960, y: 540 })
        );
        assert_eq!(
            parse_action_json(r#"{type:hotkey, keys:[ctrl,c]}"#),
            Some(ComputerAction::Hotkey {
                keys: vec!["ctrl".to_owned(), "c".to_owned()]
            })
        );
        assert_eq!(
            parse_action_json(r#"{action:mouse_click,button:left,count:1,x:-100,y:200}"#),
            Some(ComputerAction::MouseClick {
                button: "left".to_owned(),
                count: 1,
                x: Some(-100),
                y: Some(200)
            })
        );
        assert_eq!(
            parse_action_json(r#"{"action":"mouse_move","x":600,"y":400}"#),
            Some(ComputerAction::MouseMove { x: 600, y: 400 })
        );
        assert_eq!(
            parse_action_json(r#"{this is not json at all"#),
            None,
            "garbage must never parse into an action"
        );
        assert_eq!(
            parse_action_json("{type:type_text,text:hello world}"),
            None,
            "unquoted strings containing whitespace are rejected, not executed"
        );
    }

    #[test]
    fn intent_gate_distinguishes_control_from_feature_discussion() {
        assert!(explicitly_requests_computer_use(
            "Please use my computer and click on my screen to open Notepad"
        ));
        assert!(explicitly_requests_computer_use(
            "Use computer use to control my PC"
        ));
        assert!(explicitly_requests_computer_use("use computer"));
        assert!(explicitly_requests_computer_use("please use the computer"));
        assert!(explicitly_requests_computer_use("use pc"));
        assert!(explicitly_requests_computer_use(
            "access my pc and use edge to open website youtube.com"
        ));
        assert!(explicitly_requests_computer_use(
            "/computer open Edge and go to youtube.com"
        ));
        assert!(explicitly_requests_computer_use(
            "/computer debug the browser window"
        ));
        assert!(!explicitly_requests_computer_use(
            "/computerize this workflow"
        ));
        assert!(!explicitly_requests_computer_use(
            "I am trying to add a feature called computer use; fix the code"
        ));
        assert!(!explicitly_requests_computer_use(
            "Explain how computer use works"
        ));
        assert!(explicitly_requests_computer_use(
            "move the mouse to the center of the screen"
        ));
        assert!(explicitly_requests_computer_use(
            "double-click the file on my desktop"
        ));
        assert!(!explicitly_requests_computer_use(
            "the mouse cursor on the screenshot is not moving in the app"
        ));
    }

    #[test]
    fn validation_blocks_offscreen_and_malformed_input() {
        assert!(ComputerAction::MouseMove { x: -100, y: 200 }
            .validate(bounds())
            .is_ok());
        assert!(ComputerAction::MouseMove { x: 3000, y: 200 }
            .validate(bounds())
            .is_err());
        assert!(ComputerAction::MouseClick {
            button: "side".to_owned(),
            count: 1,
            x: None,
            y: None,
        }
        .validate(bounds())
        .is_err());
        assert!(ComputerAction::Hotkey {
            keys: vec!["ctrl".to_owned()]
        }
        .validate(bounds())
        .is_err());
    }

    #[test]
    fn only_observation_actions_work_without_full_access() {
        assert!(!ComputerAction::Screenshot.requires_full_access());
        assert!(!ComputerAction::GetScreenSize.requires_full_access());
        assert!(ComputerAction::MouseScroll {
            delta_x: 0,
            delta_y: -120,
        }
        .requires_full_access());
        assert!(ComputerAction::TypeText {
            text: "hello".to_owned()
        }
        .requires_full_access());
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "moves the real Windows cursor and captures the live desktop"]
    async fn live_capture_and_reversible_cursor_move() {
        let desktop = screen_bounds()
            .await
            .unwrap_or_else(|error| panic!("desktop bounds must be available: {error}"));
        let before = cursor_position()
            .await
            .unwrap_or_else(|error| panic!("cursor position must be available: {error}"));
        let candidate_x = before.0.saturating_add(12);
        let candidate_y = before.1.saturating_add(12);
        let target = if desktop.contains(candidate_x, candidate_y) {
            (candidate_x, candidate_y)
        } else {
            (before.0.saturating_sub(12), before.1.saturating_sub(12))
        };

        execute_action(ComputerAction::MouseMove {
            x: target.0,
            y: target.1,
        })
        .await
        .unwrap_or_else(|error| panic!("cursor move must succeed: {error}"));
        let moved = cursor_position()
            .await
            .unwrap_or_else(|error| panic!("moved position must be readable: {error}"));
        execute_action(ComputerAction::MouseMove {
            x: before.0,
            y: before.1,
        })
        .await
        .unwrap_or_else(|error| panic!("cursor restore must succeed: {error}"));

        assert_eq!(moved, target);
        let capture = capture_screen()
            .await
            .unwrap_or_else(|error| panic!("live screenshot must succeed: {error}"));
        assert!(capture.width > 0);
        assert!(capture.height > 0);
        assert!(!capture.image_base64.is_empty());
    }
}
