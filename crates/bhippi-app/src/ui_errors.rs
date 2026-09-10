//! What the webview reports when it breaks.
//!
//! Every other failure in Bhippi leaves a record: a provider fault is a typed error, a
//! refused write is a `WARN` with the path, a Godot child's last line is in the Output log.
//! A JavaScript exception left nothing at all — it unmounted the page and was gone, so the
//! only evidence a person could offer was "it went blank", and the only honest answer was
//! that nobody could tell them why.
//!
//! This is the missing half of that story. The page catches its own crashes (a root error
//! boundary, plus `error` and `unhandledrejection` listeners for the ones that never reach
//! React) and sends them here, where they land in the same `~/.bhippi/logs` file as
//! everything else, through the same redactor. A blank window is now a log line with a
//! stack in it.
//!
//! It is deliberately one command with no state and no reply. Reporting a crash must not be
//! able to fail interestingly, or to become the second crash.

use serde::{Deserialize, Serialize};
use specta::Type;

/// How much of a stack is worth keeping.
///
/// Enough for the frames that matter, bounded so a runaway recursion cannot write a
/// megabyte into the log for every one of its thousands of frames.
const STACK_LIMIT: usize = 4_000;

/// The same, for the message and the component stack.
const TEXT_LIMIT: usize = 2_000;

/// Where the page was when it broke.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UiErrorKind {
    /// A React render threw and an error boundary caught it.
    Render,
    /// An exception nothing caught, from `window.onerror`.
    Uncaught,
    /// A rejected promise nobody handled.
    Rejection,
}

impl UiErrorKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Render => "a render threw",
            Self::Uncaught => "an uncaught exception",
            Self::Rejection => "an unhandled promise rejection",
        }
    }
}

/// One crash, as the page saw it.
#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct UiError {
    pub kind: UiErrorKind,
    pub message: String,
    /// The JavaScript stack, when the thrown value carried one.
    pub stack: Option<String>,
    /// React's component stack — which component was rendering — for a render error.
    pub component_stack: Option<String>,
    /// Where in the app it happened, as the boundary named itself ("the transcript").
    pub surface: Option<String>,
}

/// Trims to a limit on a character boundary, saying so when it cut.
fn clamp(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_owned();
    }
    let kept: String = trimmed.chars().take(limit).collect();
    format!("{kept}… (truncated)")
}

/// Record a crash the webview caught.
///
/// Infallible on purpose: the caller is a page that has already failed once, and a reporter
/// that can return an error is a reporter someone has to write a fallback for.
#[tauri::command]
#[specta::specta]
pub async fn report_ui_error(error: UiError) {
    let message = clamp(&error.message, TEXT_LIMIT);
    let stack = error
        .stack
        .as_deref()
        .map(|stack| clamp(stack, STACK_LIMIT));
    let component_stack = error
        .component_stack
        .as_deref()
        .map(|stack| clamp(stack, STACK_LIMIT));
    let surface = error.surface.as_deref().map(|name| clamp(name, 120));

    tracing::error!(
        kind = error.kind.label(),
        surface = surface.as_deref().unwrap_or("the app"),
        stack = stack.as_deref().unwrap_or(""),
        component_stack = component_stack.as_deref().unwrap_or(""),
        "the interface failed: {message}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_stack_is_cut_rather_than_written_whole() {
        let huge = "at frame\n".repeat(2_000);
        let kept = clamp(&huge, STACK_LIMIT);
        assert!(kept.chars().count() <= STACK_LIMIT + 16);
        assert!(kept.ends_with("(truncated)"), "and it says that it was cut");
    }

    /// A stack full of non-ASCII must not be sliced through a character.
    #[test]
    fn clamping_never_splits_a_character() {
        let text = "é".repeat(64);
        assert_eq!(
            clamp(&text, 10).chars().take(10).collect::<String>(),
            "é".repeat(10)
        );
    }

    #[test]
    fn a_short_message_is_left_exactly_as_it_came() {
        assert_eq!(
            clamp("  Cannot read properties of null  ", TEXT_LIMIT),
            "Cannot read properties of null"
        );
    }

    /// The label is what a person reads in the log; every kind must have one.
    #[test]
    fn every_kind_says_what_happened() {
        for kind in [
            UiErrorKind::Render,
            UiErrorKind::Uncaught,
            UiErrorKind::Rejection,
        ] {
            assert!(!kind.label().is_empty());
        }
    }
}
