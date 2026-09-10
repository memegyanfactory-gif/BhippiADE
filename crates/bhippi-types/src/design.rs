//! Numbers and vocabulary that shape the design intelligence layer (ADR-0046).
//!
//! Every budget here is a behaviour: the map that is always on, the pack Rust selects per
//! turn, the memory block, the mid-turn query rounds, and the caps on what the taste loop
//! may keep. They live here rather than inline so they are measured in one place and
//! changed in one place (R11).

use serde::{Deserialize, Serialize};
use specta::Type;

/// The always-on map of the design base: one line per module with its `when`. Measured at
/// ~770 tokens for 34 modules; past this it stops being a map.
pub const DESIGN_INDEX_TOKEN_BUDGET: u64 = 800;

/// The per-turn pack of retrieved sections. Comparable to the engine facts budget; the
/// rest of the base stays behind `design_query`.
pub const DESIGN_CONTEXT_TOKEN_BUDGET: u64 = 1_200;

/// Past six sections the model reads rules that contradict each other's emphasis.
pub const DESIGN_MAX_SECTIONS_PER_TURN: usize = 6;

/// Taste profile plus approved lessons, budgeted separately from the pack so a talkative
/// profile never crowds out the rule that would have prevented the mistake.
pub const DESIGN_MEMORY_TOKEN_BUDGET: u64 = 400;

/// Mid-turn `<design_query>` rounds. A design question should not need more.
pub const DESIGN_QUERY_MAX_ROUNDS: usize = 3;

/// One section, or eight search rows. A capped answer says so.
pub const DESIGN_QUERY_ANSWER_TOKEN_BUDGET: u64 = 900;

/// A search answer is a menu, not a dump.
pub const DESIGN_SEARCH_MAX_HITS: usize = 8;

/// More pins than this is a stylesheet, not a taste.
pub const TASTE_PROFILE_MAX_PINS: usize = 48;

/// The rendered taste block, inside the memory budget.
pub const TASTE_PROFILE_TOKEN_BUDGET: u64 = 300;

/// One event is an anecdote; a lesson needs at least this many episodes behind it.
pub const DESIGN_LESSON_MIN_EVIDENCE: usize = 2;

/// Past this, lessons need consolidation, not more rows.
pub const DESIGN_LESSONS_MAX_APPROVED: usize = 64;

/// The rendered lessons block, inside the memory budget.
pub const DESIGN_LESSON_TOKEN_BUDGET: u64 = 200;

/// Longest rule text a lesson may carry. A rule is one sentence.
pub const DESIGN_LESSON_MAX_RULE_BYTES: usize = 400;

/// What kind of visible thing a turn is making. Rust infers it from the workspace and the
/// batch; it decides which domains of the base are in play.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DesignSurface {
    /// A web page: a landing page, docs, a tool, the game's export shell or credits.
    WebPage,
    /// In-game UI: HUD, menus, dialogs, built from Godot `Control` nodes.
    GameUi,
    /// A 3D scene: layout, lighting, materials, camera, placed models.
    Scene3d,
    /// A 2D scene: sprites, tiles, parallax.
    Scene2d,
    /// The studio's own chrome, held to `docs/DESIGN-SYSTEM.md`.
    StudioChrome,
    /// Not known yet; only the foundations apply.
    #[default]
    Unknown,
}

impl DesignSurface {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WebPage => "web_page",
            Self::GameUi => "game_ui",
            Self::Scene3d => "scene_3d",
            Self::Scene2d => "scene_2d",
            Self::StudioChrome => "studio_chrome",
            Self::Unknown => "unknown",
        }
    }

    /// The knowledge-base domains this surface draws from, most specific first. The
    /// foundations and the process apply to everything and are not listed.
    #[must_use]
    pub const fn domains(self) -> &'static [&'static str] {
        match self {
            Self::WebPage => &["web", "art-direction"],
            Self::GameUi => &["game-ui", "art-direction", "audio"],
            Self::Scene3d => &["scene-3d", "art-direction", "audio"],
            Self::Scene2d => &["scene-2d", "game-ui", "art-direction", "audio"],
            Self::StudioChrome => &["web"],
            Self::Unknown => &[],
        }
    }
}
