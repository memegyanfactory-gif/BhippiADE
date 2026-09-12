//! The edge glow shown while Bhippi is driving a surface (ADR-0057).
//!
//! ADR-0054 retired the desktop overlay because it was a *demo*: a scanning grid, a ripple at
//! every action point, a second labelled pointer chasing the real one, and a ticker of the
//! same rows the in-app panel was already drawing. This is the one thing that went with it
//! that should not have: a person whose machine is being driven by something other than their
//! hands deserves to be able to see that from across the room, without reading anything.
//!
//! So exactly one property of the old window comes back, and none of its content:
//!
//! * **It carries no information.** Present means *Bhippi is driving this surface*; absent
//!   means it is not. There is no text, no icon, no cursor, no progress. It cannot become a
//!   second place to read the run, which is the thing ADR-0054 was actually protecting.
//! * **It frames what is being driven** — today that is the virtual desktop, which is the
//!   only surface a `ComputerTurnGuard` is ever taken out for. See [`desktop_frame`] for why
//!   the game window is not resolved from in here.
//! * **It is never load-bearing.** Every function here returns `()`, logs its failures, and
//!   is called *after* the emergency stop is armed. A glow that will not open, will not
//!   position, or will not go click-through costs the user a border and nothing else.
//!
//! The window is sized in **physical** pixels from bounds that are already physical, so the
//! DPI and monitor-topology failure mode ADR-0054 worried about has no arithmetic to get
//! wrong: there is no logical/physical conversion anywhere in this file.

use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl};

/// The window's label. Matched by `capabilities/overlay.json`.
pub const GLOW_LABEL: &str = "computer-glow";

/// The page, copied verbatim out of `ui/public/` by Vite.
const GLOW_PAGE: &str = "computer-glow.html";

/// A frame smaller than this in either axis is not a surface anybody can watch being driven;
/// it is a bad rect, and a bad rect is not worth putting a window on the screen for.
const MIN_FRAME_PX: u32 = 64;

/// The rectangle the glow hugs, in physical screen pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GlowFrame {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl GlowFrame {
    /// The virtual desktop.
    #[must_use]
    pub const fn desktop(bounds: crate::computer::ScreenBounds) -> Self {
        Self {
            x: bounds.origin_x,
            y: bounds.origin_y,
            width: bounds.width,
            height: bounds.height,
        }
    }

    /// True when this is a rectangle worth drawing.
    #[must_use]
    pub const fn is_drawable(self) -> bool {
        self.width >= MIN_FRAME_PX && self.height >= MIN_FRAME_PX
    }
}

/// The rectangle a desktop-scoped turn is driving: every monitor, including any with a
/// negative origin.
///
/// Deliberately **only** the desktop. `ComputerScope::GameWindow` belongs to the engine's
/// observation path, which is bound to an [`EngineCaptureScope`](crate::computer_window::EngineCaptureScope)
/// precisely so it can never reach a desktop-wide call; resolving a game window's rect from
/// in here would need one of those desktop entry points and would be the leak that type
/// exists to prevent. Framing the game window is a separate change, on the day that path
/// grows a turn guard of its own (ADR-0057 §Consequences).
///
/// `None` is a turn with no glow, never a turn that fails: off Windows, or when the bounds
/// cannot be read, the run continues with its emergency stop armed and its panel drawing.
pub async fn desktop_frame() -> Option<GlowFrame> {
    match crate::computer::screen_bounds().await {
        Ok(bounds) => Some(GlowFrame::desktop(bounds)),
        Err(error) => {
            tracing::debug!(%error, "no desktop bounds; the run goes on without a glow");
            None
        }
    }
}

/// Put the glow on screen around `frame`. Idempotent: showing it again moves the existing
/// window rather than opening a second one.
pub fn show(app: &AppHandle, frame: GlowFrame) {
    if !frame.is_drawable() {
        tracing::debug!(
            width = frame.width,
            height = frame.height,
            "the surface being driven is too small to frame; no glow"
        );
        return;
    }

    if let Some(existing) = app.get_webview_window(GLOW_LABEL) {
        place(&existing, frame);
        if let Err(error) = existing.show() {
            tracing::debug!(%error, "the glow could not be re-shown");
        }
        return;
    }

    // `visible(false)` first: a window that appears at its default position and then jumps to
    // the screen edge is a flash of chrome in the corner of somebody's eye.
    let built =
        tauri::WebviewWindowBuilder::new(app, GLOW_LABEL, WebviewUrl::App(GLOW_PAGE.into()))
            .title("Bhippi is in control")
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .resizable(false)
            .minimizable(false)
            .maximizable(false)
            .closable(false)
            .focused(false)
            .visible(false)
            // A size is required at build time; the real one is set in physical pixels below.
            .inner_size(f64::from(MIN_FRAME_PX), f64::from(MIN_FRAME_PX))
            .build();

    let window = match built {
        Ok(window) => window,
        Err(error) => {
            tracing::warn!(%error, "the Computer Use glow could not open; the run continues");
            return;
        }
    };

    // Before it is ever visible: a window the user can click is a window that eats the input
    // the agent is trying to send. If this fails the window is closed rather than shown,
    // because a click-eating overlay is worse than no overlay at all.
    if let Err(error) = window.set_ignore_cursor_events(true) {
        tracing::warn!(%error, "the glow could not be made click-through; closing it");
        let _ = window.close();
        return;
    }

    place(&window, frame);
    if let Err(error) = window.show() {
        tracing::debug!(%error, "the glow could not be shown");
    }
}

/// Take the glow down. Safe to call when there is none.
pub fn hide(app: &AppHandle) {
    let Some(window) = app.get_webview_window(GLOW_LABEL) else {
        return;
    };
    // Closed rather than hidden: a turn is not a frequent enough event to keep a
    // desktop-sized webview resident between them, and a closed window cannot be left over
    // the user's screen by a later failure to hide it.
    if let Err(error) = window.close() {
        tracing::debug!(%error, "the glow could not be closed; hiding it instead");
        let _ = window.hide();
    }
}

/// Position and size in physical pixels — the units the bounds already came in.
fn place(window: &tauri::WebviewWindow, frame: GlowFrame) {
    if let Err(error) = window.set_position(PhysicalPosition::new(frame.x, frame.y)) {
        tracing::debug!(%error, "the glow could not be positioned");
    }
    if let Err(error) = window.set_size(PhysicalSize::new(frame.width, frame.height)) {
        tracing::debug!(%error, "the glow could not be sized");
        // A window at the wrong size is a border in the wrong place; make it small and
        // harmless rather than leaving it spanning something it is not describing.
        let _ = window.set_size(LogicalSize::new(
            f64::from(MIN_FRAME_PX),
            f64::from(MIN_FRAME_PX),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer::ScreenBounds;

    #[test]
    fn the_desktop_frame_is_the_virtual_screen_exactly() {
        let bounds = ScreenBounds {
            origin_x: -1920,
            origin_y: -200,
            width: 5120,
            height: 1440,
        };
        let frame = GlowFrame::desktop(bounds);
        assert_eq!(
            frame.x, -1920,
            "a monitor left of the primary keeps its origin"
        );
        assert_eq!(frame.y, -200);
        assert_eq!(frame.width, 5120);
        assert_eq!(frame.height, 1440);
        assert!(frame.is_drawable());
    }

    #[test]
    fn a_degenerate_rect_is_never_drawn() {
        let collapsed = GlowFrame {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
        assert!(!collapsed.is_drawable());

        let sliver = GlowFrame {
            x: 0,
            y: 0,
            width: 1920,
            height: MIN_FRAME_PX - 1,
        };
        assert!(
            !sliver.is_drawable(),
            "a minimised or collapsing window is not a surface to frame"
        );

        let real = GlowFrame {
            x: 0,
            y: 0,
            width: MIN_FRAME_PX,
            height: MIN_FRAME_PX,
        };
        assert!(real.is_drawable());
    }

    #[test]
    fn the_label_and_the_page_are_the_ones_the_rest_of_the_build_names() {
        // The capability file grants permissions to this label, and Vite copies this page.
        // Both are matched by name in `ui/tests/computer-glow.test.mjs`.
        assert_eq!(GLOW_LABEL, "computer-glow");
        assert_eq!(GLOW_PAGE, "computer-glow.html");
    }
}
