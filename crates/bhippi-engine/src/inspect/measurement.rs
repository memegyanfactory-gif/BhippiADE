//! Measured cost — and the deliberate absence of it (ADR-0056 §8, INV-098).
//!
//! Nothing in Bhippi may print a frame time it did not measure. Not "estimated", not
//! "typical for a scene this size", not a number a model produced: an inspector either has a
//! measurement from a run that actually happened, or it says *Not measured yet* and offers
//! the button that would produce one.
//!
//! The type enforces it. [`PerformanceEvidence`] cannot be constructed without naming its
//! `source` — the run it came from — and there is no `Default`, so "no evidence" is
//! `Option::None` at every call site rather than a zeroed struct that reads like a fast game.

use serde::{Deserialize, Serialize};
use specta::Type;

/// The one action that turns *Not measured yet* into a number.
pub const HOW_TO_MEASURE: &str = "Run Performance Scan";

/// A real measurement from a run that happened.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PerformanceEvidence {
    /// What produced it, in words a person can check: `"headless playtest, 600 frames"`.
    pub source: String,
    /// RFC 3339.
    pub captured_at: String,
    /// Frames the run rendered, when the runner counted them.
    #[serde(default)]
    pub frames: Option<u64>,
    /// Wall-clock milliseconds the counted frames took.
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
    /// The scene the run played, project-relative.
    #[serde(default)]
    pub scene: Option<String>,
}

impl PerformanceEvidence {
    #[must_use]
    pub fn new(source: impl Into<String>, captured_at: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            captured_at: captured_at.into(),
            frames: None,
            elapsed_ms: None,
            scene: None,
        }
    }

    #[must_use]
    pub fn with_frames(mut self, frames: u64, elapsed_ms: u64) -> Self {
        self.frames = Some(frames);
        self.elapsed_ms = Some(elapsed_ms);
        self
    }

    #[must_use]
    pub fn in_scene(mut self, scene: impl Into<String>) -> Self {
        self.scene = Some(scene.into());
        self
    }

    /// Milliseconds per frame, to one decimal, or `None` when the run did not produce both
    /// numbers. Arithmetic on measurements only — never a model's guess, never a constant.
    #[must_use]
    pub fn frame_ms(&self) -> Option<f64> {
        let frames = self.frames?;
        let elapsed = self.elapsed_ms?;
        if frames == 0 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        let value = elapsed as f64 / frames as f64;
        Some((value * 10.0).round() / 10.0)
    }

    /// Frames per second implied by the measured frame time.
    #[must_use]
    pub fn fps(&self) -> Option<f64> {
        let frame_ms = self.frame_ms()?;
        (frame_ms > 0.0).then(|| (1_000.0 / frame_ms * 10.0).round() / 10.0)
    }

    /// The one line the performance panel prints when a measurement exists.
    #[must_use]
    pub fn headline(&self) -> String {
        match (self.frames, self.frame_ms(), self.fps()) {
            (Some(frames), Some(frame_ms), Some(fps)) => {
                format!(
                    "{frames} frames · {frame_ms} ms/frame · {fps} fps ({})",
                    self.source
                )
            }
            _ => format!("Measured by {}, with no frame count", self.source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_time_is_division_and_nothing_else() {
        let evidence = PerformanceEvidence::new("headless playtest", "2026-09-10T00:00:00Z")
            .with_frames(600, 12_000);
        assert_eq!(evidence.frame_ms(), Some(20.0));
        assert_eq!(evidence.fps(), Some(50.0));
        assert!(evidence.headline().contains("600 frames"));
    }

    #[test]
    fn a_run_that_counted_nothing_reports_nothing_rather_than_zero() {
        let evidence = PerformanceEvidence::new("watch play", "2026-09-10T00:00:00Z");
        assert_eq!(evidence.frame_ms(), None);
        assert_eq!(evidence.fps(), None);
        assert!(evidence.headline().contains("no frame count"));

        let empty = PerformanceEvidence::new("playtest", "2026-09-10T00:00:00Z").with_frames(0, 10);
        assert_eq!(empty.frame_ms(), None);
    }
}
