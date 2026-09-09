//! The HUD library: buildable HUD presets for the Godot path (GAD-160).
//!
//! Before this module a `preset.hud.*` id was a *card* — a title, a purpose sentence and a
//! list of Godot class names. Nothing built one, so an archetype naming
//! `hud: "preset.hud.lives_score"` produced a game with no HUD at all. Here a preset is a
//! **buildable spec**: slots, widgets, bindings and a skin, expanded by [`build`] into a
//! [`GodotActionBatch`] that writes a real `CanvasLayer` scene plus the GDScript that drives
//! it. The scene is a normal Godot scene a person can open and edit; the script only binds,
//! animates and skins, so nothing here is a second renderer (ADR-0043).
//!
//! The rules in `prompts/design/game-ui/hud.md` are enforced as **gates**, not advice:
//! [`HudPreset::validate`] refuses a preset that carries more than
//! [`MAX_PERSISTENT_WIDGETS`] always-on elements, puts anything persistent in the bottom
//! centre, puts anything but a reticle dead centre, or names a font size under
//! [`MIN_FONT_SIZE`]. A HUD that breaks the doctrine is a build failure, not a warning.
//!
//! Skins are the reason one preset is not one look: the same `ammo_health` spec renders as
//! `neon`, `military` or `paper` because every colour, radius and font size the script
//! applies comes from [`HudSkin`], and the generated script can swap one at runtime.

use super::action::{GodotAction, GodotActionBatch};
use super::rel_to_res;
use super::tscn::TscnValue;
use crate::error::{EngineError, Result};
use crate::intent::catalog::{PropertyKind, PropertySpec};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

/// Where the HUD scene lands in a project.
pub const HUD_SCENE_REL: &str = "scenes/hud.tscn";
/// The script the HUD root carries.
pub const HUD_SCRIPT_REL: &str = "scripts/hud.gd";
/// The `_draw`-based helper the ring, compass and minimap widgets share.
pub const HUD_GAUGE_SCRIPT_REL: &str = "scripts/hud_gauge.gd";
/// Where imported HUD icons live inside a project.
pub const HUD_ICON_DIR: &str = "assets/ui/icons";
/// The node name the HUD is instanced under in the main scene.
pub const HUD_NODE_NAME: &str = "HUD";
/// `CanvasLayer.layer` for the HUD: above the game, below a pause menu.
pub const HUD_CANVAS_LAYER: i64 = 100;

/// Section 1 of the doctrine: a HUD with more than five persistent elements has a budget
/// problem, not a layout problem.
pub const MAX_PERSISTENT_WIDGETS: usize = 5;
/// Section 3: nothing on a HUD is under 18 px at 1080p.
pub const MIN_FONT_SIZE: i64 = 18;
/// Section 2: the safe-area inset, as a fraction of the *shorter* screen edge.
pub const SAFE_AREA_FRACTION: f64 = 0.04;
/// The reference height every size in a skin is authored against.
pub const REFERENCE_HEIGHT: f64 = 1080.0;

// ---------------------------------------------------------------------------- slots

/// The nine anchor positions a widget may take. The doctrine assigns meaning to them:
/// top-left is the player's own state, top-right is session state, the bottom corners are
/// resources, and the centre belongs to the game.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum HudSlot {
    TopLeft,
    TopCentre,
    TopRight,
    MidLeft,
    Centre,
    MidRight,
    BottomLeft,
    BottomCentre,
    BottomRight,
}

impl HudSlot {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "top_left",
            Self::TopCentre => "top_centre",
            Self::TopRight => "top_right",
            Self::MidLeft => "mid_left",
            Self::Centre => "centre",
            Self::MidRight => "mid_right",
            Self::BottomLeft => "bottom_left",
            Self::BottomCentre => "bottom_centre",
            Self::BottomRight => "bottom_right",
        }
    }

    /// The node name of the container that owns this slot inside the HUD scene.
    #[must_use]
    pub const fn node_name(self) -> &'static str {
        match self {
            Self::TopLeft => "TopLeft",
            Self::TopCentre => "TopCentre",
            Self::TopRight => "TopRight",
            Self::MidLeft => "MidLeft",
            Self::Centre => "Centre",
            Self::MidRight => "MidRight",
            Self::BottomLeft => "BottomLeft",
            Self::BottomCentre => "BottomCentre",
            Self::BottomRight => "BottomRight",
        }
    }

    const fn unit_point(self) -> (f64, f64) {
        let x = match self {
            Self::TopLeft | Self::MidLeft | Self::BottomLeft => 0.0,
            Self::TopCentre | Self::Centre | Self::BottomCentre => 0.5,
            Self::TopRight | Self::MidRight | Self::BottomRight => 1.0,
        };
        let y = match self {
            Self::TopLeft | Self::TopCentre | Self::TopRight => 0.0,
            Self::MidLeft | Self::Centre | Self::MidRight => 0.5,
            Self::BottomLeft | Self::BottomCentre | Self::BottomRight => 1.0,
        };
        (x, y)
    }

    /// `anchor_left`, `anchor_top`, `anchor_right`, `anchor_bottom`. Every slot is anchored
    /// to a *point*, not a rect, so the container shrink-wraps its content and the corner
    /// stays put as the content changes width.
    #[must_use]
    pub const fn anchors(self) -> (f64, f64, f64, f64) {
        let (x, y) = self.unit_point();
        (x, y, x, y)
    }

    /// `grow_horizontal`, `grow_vertical` as Godot's `GROW_DIRECTION_*` ints: 0 begin,
    /// 1 end, 2 both. A right-anchored slot grows leftwards, so its content never leaves
    /// the screen as it gets wider.
    #[must_use]
    pub const fn grow(self) -> (i64, i64) {
        let (x, y) = self.unit_point();
        let horizontal = if x == 0.0 {
            1
        } else if x == 1.0 {
            0
        } else {
            2
        };
        let vertical = if y == 0.0 {
            1
        } else if y == 1.0 {
            0
        } else {
            2
        };
        (horizontal, vertical)
    }

    /// How the slot's stack aligns its children: 0 begin, 1 centre, 2 end. A top-right
    /// stack reads right-aligned or its numbers jitter as they change width.
    #[must_use]
    pub const fn alignment(self) -> i64 {
        let (x, _) = self.unit_point();
        if x == 0.0 {
            0
        } else if x == 1.0 {
            2
        } else {
            1
        }
    }

    /// The sign the safe-area inset takes on each axis: a top-left slot moves right and
    /// down, a bottom-right slot moves left and up.
    #[must_use]
    pub const fn inset_sign(self) -> (f64, f64) {
        let (x, y) = self.unit_point();
        let horizontal = if x == 0.0 {
            1.0
        } else if x == 1.0 {
            -1.0
        } else {
            0.0
        };
        let vertical = if y == 0.0 {
            1.0
        } else if y == 1.0 {
            -1.0
        } else {
            0.0
        };
        (horizontal, vertical)
    }

    /// Every slot, in reading order. The scene builds them all, so a later edit can move a
    /// widget between corners without adding a container by hand.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::TopLeft,
            Self::TopCentre,
            Self::TopRight,
            Self::MidLeft,
            Self::Centre,
            Self::MidRight,
            Self::BottomLeft,
            Self::BottomCentre,
            Self::BottomRight,
        ]
    }
}

// ------------------------------------------------------------------------ visibility

/// The doctrine's three budget groups. Only [`Persistent`](Self::Persistent) counts against
/// [`MAX_PERSISTENT_WIDGETS`].
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum HudVisibility {
    /// Needed every second. Always on screen.
    Persistent,
    /// Needed on change. Appears, holds, fades.
    OnChange,
    /// Needed on demand. Hidden until the player asks.
    OnDemand,
}

impl HudVisibility {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Persistent => "persistent",
            Self::OnChange => "on_change",
            Self::OnDemand => "on_demand",
        }
    }
}

// --------------------------------------------------------------------------- widgets

/// What a widget *is*. Each kind maps to a fixed recipe of Godot nodes in [`build`], so a
/// preset never describes a node tree and no tree is ever hand-written.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum HudWidgetKind {
    /// A meter: track, a lagging ghost segment and a fill, with the value beside it.
    Bar,
    /// Discrete pips — lives, hearts, shields. Reads at a glance where a number does not.
    Segments,
    /// A tabular number with a scale tick on change.
    Counter,
    /// A clock or countdown, tabular.
    Timer,
    /// A caption and a line of text: the objective, the position, a hint.
    Text,
    /// The one persistent centre element: a light shape over a dark outline.
    Reticle,
    /// A radial meter drawn with `draw_arc` — stamina, boost, detection.
    Ring,
    /// A north strip across the top: cheaper than a minimap and less of an interruption.
    Compass,
    /// A top-down chart of registered targets with a fixed categorical legend.
    Minimap,
    /// The on-change channel: a plate that appears, holds and fades.
    Toast,
    /// A row of ability or inventory slots with cooldown wipes and key glyphs.
    IconRow,
    /// A button. Puzzle games get a reset; nothing else on a HUD is clickable.
    Action,
}

impl HudWidgetKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Segments => "segments",
            Self::Counter => "counter",
            Self::Timer => "timer",
            Self::Text => "text",
            Self::Reticle => "reticle",
            Self::Ring => "ring",
            Self::Compass => "compass",
            Self::Minimap => "minimap",
            Self::Toast => "toast",
            Self::IconRow => "icon_row",
            Self::Action => "action",
        }
    }

    /// The Godot classes this kind builds. Every one is in
    /// [`crate::intent::catalog::GODOT_CLASSES`], which the tests assert.
    #[must_use]
    pub const fn godot_nodes(self) -> &'static [&'static str] {
        match self {
            Self::Bar => &[
                "PanelContainer",
                "VBoxContainer",
                "HBoxContainer",
                "TextureRect",
                "Label",
                "Control",
                "ColorRect",
            ],
            Self::Segments => &["PanelContainer", "HBoxContainer", "Label", "TextureRect"],
            Self::Counter | Self::Timer => {
                &["PanelContainer", "HBoxContainer", "TextureRect", "Label"]
            }
            Self::Text => &["PanelContainer", "VBoxContainer", "Label"],
            Self::Reticle => &["Control", "ColorRect", "Label"],
            Self::Ring | Self::Compass | Self::Minimap => &["PanelContainer", "Control", "Label"],
            Self::Toast => &[
                "VBoxContainer",
                "PanelContainer",
                "HBoxContainer",
                "TextureRect",
                "Label",
            ],
            Self::IconRow => &[
                "HBoxContainer",
                "PanelContainer",
                "TextureRect",
                "ColorRect",
                "Label",
            ],
            Self::Action => &["PanelContainer", "Button"],
        }
    }

    /// True when the kind draws itself in `_draw` and therefore carries the gauge script.
    #[must_use]
    pub const fn is_gauge(self) -> bool {
        matches!(self, Self::Ring | Self::Compass | Self::Minimap)
    }

    /// The widget's own rect, in reference pixels, when it is not content-sized.
    #[must_use]
    pub const fn min_size(self) -> Option<(f64, f64)> {
        match self {
            Self::Ring => Some((96.0, 96.0)),
            Self::Compass => Some((520.0, 40.0)),
            Self::Minimap => Some((200.0, 200.0)),
            Self::Reticle => Some((48.0, 48.0)),
            _ => None,
        }
    }
}

/// One element on a HUD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudWidget {
    /// The node name inside the slot container. Unique across the preset.
    pub name: &'static str,
    pub kind: HudWidgetKind,
    pub slot: HudSlot,
    pub visibility: HudVisibility,
    /// The game variable the script reads, e.g. `player.health`. Empty for a widget that
    /// carries no value of its own, such as a reticle.
    pub binding: &'static str,
    /// The variable holding the maximum, for bars, rings and segment rows.
    pub max_binding: &'static str,
    /// The caption beside the value. Empty draws no caption.
    pub caption: &'static str,
    /// The icon *role* — `heart`, `coin`, `ammo`. Resolved to a real texture at build time
    /// from the project's own assets first and the imported library second; a role with no
    /// texture falls back to a drawn glyph rather than a broken `TextureRect`.
    pub icon: &'static str,
    /// Below this fraction of the maximum the widget enters its low state: hue shift, a
    /// glyph, and a pulse on the element itself — never a red vignette alone.
    pub low_at: f64,
}

const fn widget(
    name: &'static str,
    kind: HudWidgetKind,
    slot: HudSlot,
    visibility: HudVisibility,
    binding: &'static str,
    caption: &'static str,
    icon: &'static str,
) -> HudWidget {
    HudWidget {
        name,
        kind,
        slot,
        visibility,
        binding,
        max_binding: "",
        caption,
        icon,
        low_at: 0.0,
    }
}

/// A meter: the one widget shape that needs every field, because a bar without a maximum,
/// a caption or a low threshold is not a meter.
#[allow(clippy::too_many_arguments)]
const fn meter(
    name: &'static str,
    kind: HudWidgetKind,
    slot: HudSlot,
    binding: &'static str,
    max_binding: &'static str,
    caption: &'static str,
    icon: &'static str,
    low_at: f64,
) -> HudWidget {
    HudWidget {
        name,
        kind,
        slot,
        visibility: HudVisibility::Persistent,
        binding,
        max_binding,
        caption,
        icon,
        low_at,
    }
}

// ----------------------------------------------------------------------------- skins

/// A HUD's whole look, in numbers the generated script applies.
///
/// One accent per skin, on its own ramp, over a neutral: the doctrine forbids a second.
/// Sizes are authored at [`REFERENCE_HEIGHT`] and scaled by the viewport at runtime, so a
/// 4K screen does not shrink the score to nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudSkin {
    pub id: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
    /// The plate behind text over the world. Alpha lives in the fourth channel.
    pub plate: [f64; 4],
    /// A meter's empty track: one step off the plate.
    pub track: [f64; 4],
    /// The one accent. Fills, active pips, the ring.
    pub accent: [f64; 4],
    /// The low state, reached by a hue shift rather than a new colour.
    pub warn: [f64; 4],
    pub text: [f64; 4],
    pub muted: [f64; 4],
    /// The outline under text and around the reticle.
    pub outline: [f64; 4],
    /// Plate corner radius in reference pixels. Zero reads as a pixel-art HUD.
    pub radius: f64,
    /// Plate border width. Zero for a soft skin, 2 for a technical one.
    pub border: f64,
    /// Persistent numbers, in reference pixels.
    pub value_size: i64,
    /// Captions and labels.
    pub label_size: i64,
    /// Padding inside a plate.
    pub padding: f64,
    /// Separation between stacked widgets.
    pub gap: f64,
    /// A meter's height.
    pub bar_height: f64,
    /// Captions in capitals: right for military and technical skins, wrong for a soft one.
    pub uppercase: bool,
}

/// Every skin Bhippi ships. A game picks one; a follow-up prompt can swap it without
/// rebuilding the HUD, because only the script reads these numbers.
#[must_use]
pub const fn skins() -> &'static [HudSkin] {
    &[
        HudSkin {
            id: "clean",
            title: "Clean",
            blurb: "Neutral plates, one cool accent. The default that never fights the art.",
            plate: [0.06, 0.07, 0.09, 0.55],
            track: [1.0, 1.0, 1.0, 0.14],
            accent: [0.35, 0.72, 1.0, 1.0],
            warn: [1.0, 0.45, 0.35, 1.0],
            text: [0.97, 0.98, 1.0, 1.0],
            muted: [0.73, 0.77, 0.83, 1.0],
            outline: [0.02, 0.02, 0.04, 0.85],
            radius: 10.0,
            border: 0.0,
            value_size: 28,
            label_size: 19,
            padding: 14.0,
            gap: 10.0,
            bar_height: 14.0,
            uppercase: false,
        },
        HudSkin {
            id: "neon",
            title: "Neon",
            blurb: "Dark glass and one electric accent, for synthwave runners and arcades.",
            plate: [0.04, 0.02, 0.09, 0.6],
            track: [0.55, 0.35, 1.0, 0.18],
            accent: [0.45, 1.0, 0.86, 1.0],
            warn: [1.0, 0.29, 0.62, 1.0],
            text: [0.96, 0.95, 1.0, 1.0],
            muted: [0.7, 0.66, 0.9, 1.0],
            outline: [0.09, 0.0, 0.18, 0.9],
            radius: 4.0,
            border: 2.0,
            value_size: 30,
            label_size: 19,
            padding: 14.0,
            gap: 12.0,
            bar_height: 12.0,
            uppercase: true,
        },
        HudSkin {
            id: "pixel",
            title: "Pixel",
            blurb: "Hard corners, no border radius, chunky numbers. For retro platformers.",
            plate: [0.05, 0.05, 0.08, 0.72],
            track: [0.16, 0.16, 0.22, 1.0],
            accent: [1.0, 0.82, 0.25, 1.0],
            warn: [0.95, 0.27, 0.27, 1.0],
            text: [1.0, 1.0, 1.0, 1.0],
            muted: [0.72, 0.72, 0.78, 1.0],
            outline: [0.0, 0.0, 0.0, 1.0],
            radius: 0.0,
            border: 3.0,
            value_size: 30,
            label_size: 20,
            padding: 12.0,
            gap: 10.0,
            bar_height: 16.0,
            uppercase: true,
        },
        HudSkin {
            id: "military",
            title: "Military",
            blurb: "Low-contrast olive plates, thin rules, phosphor green. For shooters.",
            plate: [0.07, 0.09, 0.07, 0.5],
            track: [0.55, 0.62, 0.5, 0.16],
            accent: [0.6, 0.92, 0.42, 1.0],
            warn: [0.98, 0.62, 0.16, 1.0],
            text: [0.9, 0.94, 0.87, 1.0],
            muted: [0.65, 0.71, 0.62, 1.0],
            outline: [0.02, 0.04, 0.02, 0.9],
            radius: 2.0,
            border: 1.0,
            value_size: 26,
            label_size: 18,
            padding: 12.0,
            gap: 9.0,
            bar_height: 10.0,
            uppercase: true,
        },
        HudSkin {
            id: "arcane",
            title: "Arcane",
            blurb: "Ink plates and a gold accent, for fantasy and dungeon crawlers.",
            plate: [0.08, 0.06, 0.05, 0.62],
            track: [0.75, 0.62, 0.4, 0.18],
            accent: [0.98, 0.79, 0.42, 1.0],
            warn: [0.86, 0.33, 0.3, 1.0],
            text: [0.98, 0.95, 0.89, 1.0],
            muted: [0.79, 0.72, 0.62, 1.0],
            outline: [0.05, 0.03, 0.02, 0.9],
            radius: 14.0,
            border: 1.0,
            value_size: 28,
            label_size: 20,
            padding: 16.0,
            gap: 11.0,
            bar_height: 14.0,
            uppercase: false,
        },
        HudSkin {
            id: "paper",
            title: "Paper",
            blurb: "A light HUD: warm paper plates and ink text, for puzzles and cosy games.",
            plate: [0.98, 0.96, 0.91, 0.86],
            track: [0.16, 0.14, 0.11, 0.14],
            accent: [0.16, 0.5, 0.42, 1.0],
            warn: [0.76, 0.28, 0.2, 1.0],
            text: [0.12, 0.11, 0.1, 1.0],
            muted: [0.38, 0.36, 0.33, 1.0],
            outline: [1.0, 1.0, 1.0, 0.85],
            radius: 12.0,
            border: 0.0,
            value_size: 28,
            label_size: 20,
            padding: 15.0,
            gap: 10.0,
            bar_height: 13.0,
            uppercase: false,
        },
        HudSkin {
            id: "noir",
            title: "Noir",
            blurb: "Near-black plates, white text, a single cold accent. Stealth and horror.",
            plate: [0.02, 0.02, 0.03, 0.66],
            track: [1.0, 1.0, 1.0, 0.1],
            accent: [0.86, 0.88, 0.92, 1.0],
            warn: [0.86, 0.15, 0.15, 1.0],
            text: [0.96, 0.96, 0.97, 1.0],
            muted: [0.6, 0.62, 0.66, 1.0],
            outline: [0.0, 0.0, 0.0, 0.95],
            radius: 3.0,
            border: 0.0,
            value_size: 26,
            label_size: 18,
            padding: 13.0,
            gap: 9.0,
            bar_height: 10.0,
            uppercase: true,
        },
        HudSkin {
            id: "candy",
            title: "Candy",
            blurb: "Rounded, high-saturation plates for casual and mobile games.",
            plate: [0.16, 0.09, 0.24, 0.6],
            track: [1.0, 1.0, 1.0, 0.2],
            accent: [1.0, 0.55, 0.75, 1.0],
            warn: [1.0, 0.78, 0.24, 1.0],
            text: [1.0, 0.99, 1.0, 1.0],
            muted: [0.85, 0.79, 0.9, 1.0],
            outline: [0.14, 0.04, 0.2, 0.9],
            radius: 20.0,
            border: 0.0,
            value_size: 30,
            label_size: 20,
            padding: 16.0,
            gap: 12.0,
            bar_height: 18.0,
            uppercase: false,
        },
    ]
}

/// The skin with this id.
#[must_use]
pub fn skin(id: &str) -> Option<&'static HudSkin> {
    skins().iter().find(|entry| entry.id == id)
}

/// The skin a preset uses when nothing asks for another.
pub const DEFAULT_SKIN: &str = "clean";

// --------------------------------------------------------------------------- presets

/// A buildable HUD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudPreset {
    /// The `preset.hud.*` id. The nine that already existed keep their ids, because the
    /// archetype packs name them and a rename would silently drop a pack's HUD.
    pub id: &'static str,
    pub title: &'static str,
    pub purpose: &'static str,
    /// The archetypes this HUD is a good default for. Empty means "any".
    pub archetypes: &'static [&'static str],
    /// The skin applied when the caller does not choose one.
    pub skin: &'static str,
    pub widgets: &'static [HudWidget],
    /// The knobs a follow-up prompt is likely to reach for.
    pub properties: &'static [PropertySpec],
}

impl HudPreset {
    /// Everything always on screen.
    pub fn persistent(&self) -> impl Iterator<Item = &HudWidget> {
        self.widgets
            .iter()
            .filter(|entry| entry.visibility == HudVisibility::Persistent)
    }

    /// The Godot classes this preset builds, deduplicated and sorted. This is what the
    /// preset card in the catalogue reports, and it is derived rather than authored, so a
    /// card can never claim a node the builder does not emit.
    #[must_use]
    pub fn godot_nodes(&self) -> Vec<&'static str> {
        let mut nodes: BTreeSet<&'static str> = ["CanvasLayer", "Control"].into_iter().collect();
        for entry in self.widgets {
            nodes.extend(entry.kind.godot_nodes().iter().copied());
        }
        nodes.into_iter().collect()
    }

    /// The doctrine, as blocking gates.
    ///
    /// Every rule here is one a HUD breaks by accident and nobody notices until the game is
    /// in front of a player: a sixth always-on element, a label parked over the centre of
    /// the screen, an 11 px caption. Refusing at build time is the whole point — a warning
    /// on a generated HUD is a warning nobody reads.
    pub fn validate(&self) -> Result<()> {
        let gate = |message: String, hint: &str| {
            EngineError::Gate(format!("{}: {message}", self.id), Some(hint.to_owned()))
        };

        if self.widgets.is_empty() {
            return Err(gate(
                "a HUD preset with no widgets builds an empty CanvasLayer".to_owned(),
                "Give the preset at least one widget, or do not offer it.",
            ));
        }

        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for entry in self.widgets {
            if !seen.insert(entry.name) {
                return Err(gate(
                    format!("two widgets are both called {}", entry.name),
                    "Widget names become node names, and a Godot node path must be unique.",
                ));
            }
            if !super::is_valid_node_name(entry.name) {
                return Err(gate(
                    format!("{} is not a legal Godot node name", entry.name),
                    "Node names may not contain . : @ / or a quote.",
                ));
            }
        }

        let persistent = self.persistent().count();
        if persistent > MAX_PERSISTENT_WIDGETS {
            return Err(gate(
                format!(
                    "{persistent} persistent widgets, over the budget of {MAX_PERSISTENT_WIDGETS}"
                ),
                "Move the ones the player needs only on change to on_change, and the ones \
                 they need on demand behind a button.",
            ));
        }
        if persistent == 0 {
            return Err(gate(
                "no persistent widget: nothing is ever on screen".to_owned(),
                "At least one element earns its place every second, or the HUD is a toast queue.",
            ));
        }

        for entry in self.widgets {
            if entry.slot == HudSlot::BottomCentre && entry.visibility == HudVisibility::Persistent
            {
                return Err(gate(
                    format!("{} sits persistently in the bottom centre", entry.name),
                    "Nothing persistent lives in the bottom centre; it is where the player \
                     looks when they are moving.",
                ));
            }
            if entry.slot == HudSlot::Centre && entry.kind != HudWidgetKind::Reticle {
                return Err(gate(
                    format!(
                        "{} is a {} in the centre slot",
                        entry.name,
                        entry.kind.as_str()
                    ),
                    "The centre belongs to the game. Only a reticle may sit in it.",
                ));
            }
            if entry.kind == HudWidgetKind::Reticle && entry.slot != HudSlot::Centre {
                return Err(gate(
                    format!("the reticle {} is not in the centre slot", entry.name),
                    "A reticle that is not centred is a decoration.",
                ));
            }
            let needs_max = matches!(
                entry.kind,
                HudWidgetKind::Bar | HudWidgetKind::Ring | HudWidgetKind::Segments
            );
            if needs_max && entry.max_binding.is_empty() {
                return Err(gate(
                    format!(
                        "{} is a {} with no maximum to fill against",
                        entry.name,
                        entry.kind.as_str()
                    ),
                    "A meter without a maximum cannot be drawn; give it a max_binding.",
                ));
            }
            let needs_value = !matches!(
                entry.kind,
                HudWidgetKind::Reticle | HudWidgetKind::IconRow | HudWidgetKind::Action
            );
            if needs_value && entry.binding.is_empty() {
                return Err(gate(
                    format!("{} reads no game variable", entry.name),
                    "Give the widget a binding, or it displays its placeholder forever.",
                ));
            }
        }

        let reticles = self
            .widgets
            .iter()
            .filter(|entry| entry.kind == HudWidgetKind::Reticle)
            .count();
        if reticles > 1 {
            return Err(gate(
                format!("{reticles} reticles"),
                "There is one centre of the screen.",
            ));
        }

        Ok(())
    }

    /// The same doctrine applied to a chosen skin. Kept apart from [`Self::validate`]
    /// because a preset is fixed and a skin is a choice the caller makes.
    pub fn validate_skin(skin: &HudSkin) -> Result<()> {
        if skin.value_size < MIN_FONT_SIZE || skin.label_size < MIN_FONT_SIZE {
            return Err(EngineError::Gate(
                format!(
                    "skin {} sets a font size under {MIN_FONT_SIZE} px ({} value, {} label)",
                    skin.id, skin.value_size, skin.label_size
                ),
                Some(
                    "A HUD is read at two metres on a TV. Nothing on it goes under 18 px at 1080p."
                        .to_owned(),
                ),
            ));
        }
        if skin.plate[3] <= 0.0 {
            return Err(EngineError::Gate(
                format!("skin {} has a fully transparent plate", skin.id),
                Some(
                    "Bare text over a moving scene fails the contrast floor somewhere in \
                     every level. Give the plate an alpha."
                        .to_owned(),
                ),
            ));
        }
        Ok(())
    }
}

const fn number(name: &'static str, default: &'static str) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyKind::Number,
        default,
        unit: None,
        min: None,
        max: None,
    }
}

const fn boolean(name: &'static str, default: &'static str) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyKind::Bool,
        default,
        unit: None,
        min: None,
        max: None,
    }
}

const fn text_prop(name: &'static str, default: &'static str) -> PropertySpec {
    PropertySpec {
        name,
        kind: PropertyKind::Text,
        default,
        unit: None,
        min: None,
        max: None,
    }
}

/// Every HUD Bhippi can build.
///
/// The first nine ids are the ones the archetype packs already name; they were cards and
/// are now specs. The last five exist because the nine covered no boss fight, no stealth
/// meter, no rhythm game, no map, and had no answer for a designer who wants the HUD to get
/// out of the way.
static PRESETS: &[HudPreset] = &[
    HudPreset {
        id: "preset.hud.health_score",
        title: "Health and score",
        purpose: "A health meter top-left, the score top-right and an ability row in the \
                      bottom-left corner.",
        archetypes: &["top_down_action"],
        skin: "clean",
        widgets: &[
            meter(
                "Health",
                HudWidgetKind::Bar,
                HudSlot::TopLeft,
                "player.health",
                "player.max_health",
                "Health",
                "heart",
                0.3,
            ),
            widget(
                "Score",
                HudWidgetKind::Counter,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "game.score",
                "Score",
                "star",
            ),
            widget(
                "Abilities",
                HudWidgetKind::IconRow,
                HudSlot::BottomLeft,
                HudVisibility::Persistent,
                "player.abilities",
                "",
                "",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("max_health", "100"),
            boolean("show_score", "true"),
            number("ability_slots", "4"),
        ],
    },
    HudPreset {
        id: "preset.hud.lives_score",
        title: "Lives and score",
        purpose: "Lives as pips, coins and the level timer top-right, pickups as toasts.",
        archetypes: &["platformer_2d", "platformer_3d"],
        skin: "pixel",
        widgets: &[
            meter(
                "Lives",
                HudWidgetKind::Segments,
                HudSlot::TopLeft,
                "player.lives",
                "player.max_lives",
                "",
                "heart",
                0.34,
            ),
            widget(
                "Coins",
                HudWidgetKind::Counter,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "game.coins",
                "",
                "coin",
            ),
            widget(
                "Clock",
                HudWidgetKind::Timer,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "game.time",
                "",
                "clock",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("lives", "3"),
            boolean("show_timer", "true"),
            number("coin_target", "0"),
        ],
    },
    HudPreset {
        id: "preset.hud.lap_timer",
        title: "Lap and timer",
        purpose: "Position and lap top-left, the split clock top-right, speed as a dial \
                      bottom-right.",
        archetypes: &["racing_kart"],
        skin: "neon",
        widgets: &[
            widget(
                "Position",
                HudWidgetKind::Text,
                HudSlot::TopLeft,
                HudVisibility::Persistent,
                "race.position",
                "Position",
                "",
            ),
            widget(
                "Lap",
                HudWidgetKind::Counter,
                HudSlot::TopLeft,
                HudVisibility::Persistent,
                "race.lap",
                "Lap",
                "",
            ),
            widget(
                "Split",
                HudWidgetKind::Timer,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "race.time",
                "",
                "clock",
            ),
            meter(
                "Speed",
                HudWidgetKind::Ring,
                HudSlot::BottomRight,
                "vehicle.speed",
                "vehicle.max_speed",
                "km/h",
                "",
                0.0,
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomLeft,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("lap_count", "3"),
            boolean("show_position", "true"),
            number("max_speed", "180"),
        ],
    },
    HudPreset {
        id: "preset.hud.wave_counter",
        title: "Wave and resources",
        purpose: "Base health top-left, gold and the wave number top-right, the build bar \
                      bottom-left.",
        archetypes: &["tower_defense"],
        skin: "arcane",
        widgets: &[
            meter(
                "BaseHealth",
                HudWidgetKind::Bar,
                HudSlot::TopLeft,
                "base.health",
                "base.max_health",
                "Base",
                "shield",
                0.35,
            ),
            widget(
                "Gold",
                HudWidgetKind::Counter,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "game.gold",
                "",
                "coin",
            ),
            widget(
                "Wave",
                HudWidgetKind::Counter,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "game.wave",
                "Wave",
                "",
            ),
            widget(
                "BuildBar",
                HudWidgetKind::IconRow,
                HudSlot::BottomLeft,
                HudVisibility::Persistent,
                "game.towers",
                "",
                "",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("wave_count", "10"),
            boolean("show_gold", "true"),
            number("tower_slots", "5"),
        ],
    },
    HudPreset {
        id: "preset.hud.collectible_counter",
        title: "Collectibles and objective",
        purpose: "The objective top-left, a collected-of-target readout top-right and a \
                      compass strip across the top.",
        archetypes: &["exploration"],
        skin: "paper",
        widgets: &[
            widget(
                "Objective",
                HudWidgetKind::Text,
                HudSlot::TopLeft,
                HudVisibility::Persistent,
                "game.objective",
                "Objective",
                "",
            ),
            meter(
                "Collected",
                HudWidgetKind::Segments,
                HudSlot::TopRight,
                "game.collected",
                "game.collect_target",
                "",
                "gem",
                0.0,
            ),
            widget(
                "Compass",
                HudWidgetKind::Compass,
                HudSlot::TopCentre,
                HudVisibility::Persistent,
                "player.heading",
                "",
                "",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("collect_target", "10"),
            boolean("show_hint", "true"),
            boolean("show_compass", "true"),
        ],
    },
    HudPreset {
        id: "preset.hud.ammo_health",
        title: "Ammo and health",
        purpose: "A reticle in the centre, health bottom-left, ammo bottom-right and the \
                      frag tally top-right.",
        archetypes: &["fps_arena"],
        skin: "military",
        widgets: &[
            widget(
                "Reticle",
                HudWidgetKind::Reticle,
                HudSlot::Centre,
                HudVisibility::Persistent,
                "",
                "",
                "",
            ),
            meter(
                "Health",
                HudWidgetKind::Bar,
                HudSlot::BottomLeft,
                "player.health",
                "player.max_health",
                "",
                "heart",
                0.28,
            ),
            widget(
                "Ammo",
                HudWidgetKind::Counter,
                HudSlot::BottomRight,
                HudVisibility::Persistent,
                "weapon.ammo",
                "",
                "ammo",
            ),
            widget(
                "Frags",
                HudWidgetKind::Counter,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "game.frags",
                "Frags",
                "",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::TopCentre,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("ammo_capacity", "30"),
            number("max_health", "100"),
            boolean("show_frags", "true"),
        ],
    },
    HudPreset {
        id: "preset.hud.distance_score",
        title: "Distance and multiplier",
        purpose: "Distance across the top centre, the multiplier and the best run \
                      top-right.",
        archetypes: &["endless_runner"],
        skin: "neon",
        widgets: &[
            widget(
                "Distance",
                HudWidgetKind::Counter,
                HudSlot::TopCentre,
                HudVisibility::Persistent,
                "run.distance",
                "",
                "",
            ),
            widget(
                "Multiplier",
                HudWidgetKind::Counter,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "run.multiplier",
                "Multi",
                "",
            ),
            widget(
                "Best",
                HudWidgetKind::Text,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "run.best",
                "Best",
                "",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            boolean("show_best", "true"),
            number("score_target", "0"),
            text_prop("distance_unit", "m"),
        ],
    },
    HudPreset {
        id: "preset.hud.survival_meters",
        title: "Survival meters",
        purpose: "Health, hunger and stamina stacked top-left, the clock top-right, a \
                      compass across the top. Exactly at the persistent budget.",
        archetypes: &["survival"],
        skin: "arcane",
        widgets: &[
            meter(
                "Health",
                HudWidgetKind::Bar,
                HudSlot::TopLeft,
                "player.health",
                "player.max_health",
                "Health",
                "heart",
                0.3,
            ),
            meter(
                "Hunger",
                HudWidgetKind::Bar,
                HudSlot::TopLeft,
                "player.hunger",
                "player.max_hunger",
                "Hunger",
                "food",
                0.25,
            ),
            meter(
                "Stamina",
                HudWidgetKind::Bar,
                HudSlot::TopLeft,
                "player.stamina",
                "player.max_stamina",
                "Stamina",
                "bolt",
                0.2,
            ),
            widget(
                "Clock",
                HudWidgetKind::Timer,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "world.time",
                "",
                "clock",
            ),
            widget(
                "Compass",
                HudWidgetKind::Compass,
                HudSlot::TopCentre,
                HudVisibility::Persistent,
                "player.heading",
                "",
                "",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("max_health", "100"),
            boolean("show_clock", "true"),
            boolean("show_compass", "true"),
        ],
    },
    HudPreset {
        id: "preset.hud.move_counter",
        title: "Moves and par",
        purpose: "Moves taken against par top-left, a reset button top-right. Nothing else \
                      earns the screen in a puzzle.",
        archetypes: &["puzzle_physics"],
        skin: "paper",
        widgets: &[
            widget(
                "Moves",
                HudWidgetKind::Counter,
                HudSlot::TopLeft,
                HudVisibility::Persistent,
                "puzzle.moves",
                "Moves",
                "",
            ),
            widget(
                "Par",
                HudWidgetKind::Text,
                HudSlot::TopLeft,
                HudVisibility::Persistent,
                "puzzle.par",
                "Par",
                "",
            ),
            widget(
                "Reset",
                HudWidgetKind::Action,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "",
                "Reset",
                "",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("par_moves", "12"),
            boolean("show_reset", "true"),
            boolean("show_par", "true"),
        ],
    },
    HudPreset {
        id: "preset.hud.boss_fight",
        title: "Boss fight",
        purpose: "A named boss meter across the top centre with phase pips, the player's \
                      own health bottom-left.",
        archetypes: &["top_down_action", "platformer_3d", "fps_arena"],
        skin: "noir",
        widgets: &[
            meter(
                "BossHealth",
                HudWidgetKind::Bar,
                HudSlot::TopCentre,
                "boss.health",
                "boss.max_health",
                "",
                "",
                0.25,
            ),
            widget(
                "BossName",
                HudWidgetKind::Text,
                HudSlot::TopCentre,
                HudVisibility::Persistent,
                "boss.name",
                "",
                "",
            ),
            meter(
                "Health",
                HudWidgetKind::Bar,
                HudSlot::BottomLeft,
                "player.health",
                "player.max_health",
                "",
                "heart",
                0.3,
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("phases", "3"),
            boolean("show_name", "true"),
            number("max_health", "100"),
        ],
    },
    HudPreset {
        id: "preset.hud.stealth_awareness",
        title: "Stealth awareness",
        purpose: "A detection ring top-centre, a noise meter bottom-left and the current \
                      objective top-left.",
        archetypes: &["top_down_action", "exploration"],
        skin: "noir",
        widgets: &[
            meter(
                "Awareness",
                HudWidgetKind::Ring,
                HudSlot::TopCentre,
                "stealth.awareness",
                "stealth.max_awareness",
                "",
                "eye",
                0.0,
            ),
            meter(
                "Noise",
                HudWidgetKind::Bar,
                HudSlot::BottomLeft,
                "stealth.noise",
                "stealth.max_noise",
                "Noise",
                "",
                0.0,
            ),
            widget(
                "Objective",
                HudWidgetKind::Text,
                HudSlot::TopLeft,
                HudVisibility::Persistent,
                "game.objective",
                "Objective",
                "",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("detection_seconds", "3"),
            boolean("show_noise", "true"),
            boolean("show_objective", "true"),
        ],
    },
    HudPreset {
        id: "preset.hud.combo_rhythm",
        title: "Combo and accuracy",
        purpose: "A combo counter and an accuracy meter top-right, the beat line across \
                      the top centre.",
        archetypes: &["top_down_action", "endless_runner"],
        skin: "candy",
        widgets: &[
            widget(
                "Combo",
                HudWidgetKind::Counter,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "rhythm.combo",
                "Combo",
                "",
            ),
            meter(
                "Accuracy",
                HudWidgetKind::Bar,
                HudSlot::TopRight,
                "rhythm.accuracy",
                "rhythm.max_accuracy",
                "Accuracy",
                "",
                0.5,
            ),
            widget(
                "BeatLine",
                HudWidgetKind::Compass,
                HudSlot::TopCentre,
                HudVisibility::Persistent,
                "rhythm.phase",
                "",
                "",
            ),
            widget(
                "Judgement",
                HudWidgetKind::Toast,
                HudSlot::BottomCentre,
                HudVisibility::OnChange,
                "rhythm.judgement",
                "",
                "",
            ),
        ],
        properties: &[
            number("combo_target", "50"),
            boolean("show_accuracy", "true"),
            number("beats_per_minute", "120"),
        ],
    },
    HudPreset {
        id: "preset.hud.explore_map",
        title: "Map and objective",
        purpose: "A north-up minimap top-right with a fixed legend, the objective \
                      top-left.",
        archetypes: &["exploration", "survival"],
        skin: "clean",
        widgets: &[
            widget(
                "Minimap",
                HudWidgetKind::Minimap,
                HudSlot::TopRight,
                HudVisibility::Persistent,
                "map.targets",
                "",
                "",
            ),
            widget(
                "Objective",
                HudWidgetKind::Text,
                HudSlot::TopLeft,
                HudVisibility::Persistent,
                "game.objective",
                "Objective",
                "",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            number("map_range", "60"),
            boolean("north_up", "true"),
            boolean("show_objective", "true"),
        ],
    },
    HudPreset {
        id: "preset.hud.minimal",
        title: "Minimal",
        purpose: "One persistent line and a toast channel. The HUD for a game whose \
                      subject is the world, not the numbers.",
        archetypes: &[],
        skin: "clean",
        widgets: &[
            widget(
                "Objective",
                HudWidgetKind::Text,
                HudSlot::TopLeft,
                HudVisibility::Persistent,
                "game.objective",
                "",
                "",
            ),
            widget(
                "Notices",
                HudWidgetKind::Toast,
                HudSlot::BottomRight,
                HudVisibility::OnChange,
                "game.notice",
                "",
                "",
            ),
        ],
        properties: &[
            boolean("show_objective", "true"),
            number("toast_seconds", "3"),
        ],
    },
];

/// Every HUD Bhippi can build, in the order the picker shows them.
#[must_use]
pub const fn presets() -> &'static [HudPreset] {
    PRESETS
}

/// The preset with this id.
#[must_use]
pub fn preset(id: &str) -> Option<&'static HudPreset> {
    presets().iter().find(|entry| entry.id == id)
}

/// The presets a given archetype is a good fit for, best first. An archetype with no
/// declared HUD still gets the whole library rather than nothing.
#[must_use]
pub fn presets_for(archetype: &str) -> Vec<&'static HudPreset> {
    let mut matched: Vec<&'static HudPreset> = presets()
        .iter()
        .filter(|entry| entry.archetypes.contains(&archetype))
        .collect();
    matched.extend(
        presets()
            .iter()
            .filter(|entry| !entry.archetypes.contains(&archetype)),
    );
    matched
}

// ------------------------------------------------------------------------ icon roles

/// One icon a HUD can ask for, plus the words that find it in a pack whose file names
/// nobody standardised.
///
/// Keywords are ordered: the first that matches wins, so `heart` beats `life` and a pack
/// that ships both a heart and a life-bar sprite yields the heart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IconRole {
    pub id: &'static str,
    pub title: &'static str,
    pub keywords: &'static [&'static str],
}

/// Every role the presets name.
#[must_use]
pub const fn icon_roles() -> &'static [IconRole] {
    &[
        IconRole {
            id: "heart",
            title: "Health",
            keywords: &["heart", "health", "life"],
        },
        IconRole {
            id: "coin",
            title: "Currency",
            keywords: &["coin", "gold", "money", "credit"],
        },
        IconRole {
            id: "star",
            title: "Score",
            keywords: &["star", "trophy", "score"],
        },
        IconRole {
            id: "gem",
            title: "Collectible",
            keywords: &["gem", "crystal", "diamond", "jewel"],
        },
        IconRole {
            id: "shield",
            title: "Defence",
            keywords: &["shield", "armor", "armour", "guard"],
        },
        IconRole {
            id: "clock",
            title: "Time",
            keywords: &["clock", "timer", "hourglass", "calendar"],
        },
        IconRole {
            id: "ammo",
            title: "Ammunition",
            keywords: &["ammo", "bullet", "magazine", "cartridge"],
        },
        IconRole {
            id: "bolt",
            title: "Energy",
            keywords: &["lightning", "bolt", "energy", "power"],
        },
        IconRole {
            id: "food",
            title: "Hunger",
            keywords: &["food", "meat", "apple", "bread", "hunger"],
        },
        IconRole {
            id: "eye",
            title: "Awareness",
            keywords: &["eye", "vision", "detect", "sight"],
        },
    ]
}

/// The role with this id.
#[must_use]
pub fn icon_role(id: &str) -> Option<&'static IconRole> {
    icon_roles().iter().find(|role| role.id == id)
}

/// The roles one preset asks for, in the order its widgets name them.
#[must_use]
pub fn roles_for(preset: &HudPreset) -> Vec<&'static IconRole> {
    let mut roles: Vec<&'static IconRole> = Vec::new();
    for entry in preset.widgets {
        if entry.icon.is_empty() {
            continue;
        }
        if let Some(role) = icon_role(entry.icon) {
            if !roles.iter().any(|existing| existing.id == role.id) {
                roles.push(role);
            }
        }
    }
    roles
}

/// The icons a HUD build should use, answered by the project itself.
///
/// A game that has been dressed once keeps its art: whatever sits in the project's own icon
/// folder wins, and an external library is only worth consulting for the roles left over.
#[must_use]
pub fn icons_from_project(
    project_root: &std::path::Path,
    preset: &HudPreset,
) -> BTreeMap<String, String> {
    let roles: Vec<&str> = roles_for(preset).iter().map(|role| role.id).collect();
    crate::fab::project_icons(project_root, HUD_ICON_DIR, &roles)
}

// ----------------------------------------------------------------------------- views

/// One widget as the picker draws it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudWidgetView {
    pub name: String,
    pub kind: HudWidgetKind,
    pub slot: HudSlot,
    pub visibility: HudVisibility,
    pub binding: String,
    pub max_binding: String,
    pub caption: String,
    pub icon: String,
    pub low_at: f64,
}

/// One preset as the picker draws it.
///
/// A view rather than the table itself: the tables are `&'static str` all the way down,
/// which is right for a `const` and wrong for a wire type.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudPresetView {
    pub id: String,
    pub title: String,
    pub purpose: String,
    pub archetypes: Vec<String>,
    /// The preset's own default skin.
    pub skin: String,
    pub widgets: Vec<HudWidgetView>,
    /// How many of those are always on screen, against [`MAX_PERSISTENT_WIDGETS`]. The
    /// number the picker shows, because a HUD's budget is the first thing about it.
    pub persistent: u32,
    /// Icon roles this preset would use if the project had art for them.
    pub icon_roles: Vec<String>,
    pub godot_nodes: Vec<String>,
}

/// One skin as the picker draws it. Colours cross as `#rrggbbaa` so the swatches need no
/// conversion on the other side.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudSkinView {
    pub id: String,
    pub title: String,
    pub blurb: String,
    pub plate: String,
    pub track: String,
    pub accent: String,
    pub warn: String,
    pub text: String,
    pub muted: String,
    pub outline: String,
    pub radius: f64,
    pub border: f64,
    pub value_size: i32,
    pub label_size: i32,
    pub uppercase: bool,
}

/// One icon role as the picker draws it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct IconRoleView {
    pub id: String,
    pub title: String,
    pub keywords: Vec<String>,
}

/// Everything the picker needs in one call.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudLibraryView {
    pub presets: Vec<HudPresetView>,
    pub skins: Vec<HudSkinView>,
    pub roles: Vec<IconRoleView>,
    pub default_skin: String,
    pub max_persistent: u32,
    pub min_font_size: i32,
    pub safe_area_fraction: f64,
}

fn hex(value: [f64; 4]) -> String {
    let channel = |amount: f64| (amount.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        channel(value[0]),
        channel(value[1]),
        channel(value[2]),
        channel(value[3])
    )
}

impl HudSkinView {
    #[must_use]
    pub fn of(skin: &HudSkin) -> Self {
        Self {
            id: skin.id.to_owned(),
            title: skin.title.to_owned(),
            blurb: skin.blurb.to_owned(),
            plate: hex(skin.plate),
            track: hex(skin.track),
            accent: hex(skin.accent),
            warn: hex(skin.warn),
            text: hex(skin.text),
            muted: hex(skin.muted),
            outline: hex(skin.outline),
            radius: skin.radius,
            border: skin.border,
            value_size: i32::try_from(skin.value_size).unwrap_or(i32::MAX),
            label_size: i32::try_from(skin.label_size).unwrap_or(i32::MAX),
            uppercase: skin.uppercase,
        }
    }
}

impl HudPresetView {
    #[must_use]
    pub fn of(preset: &HudPreset) -> Self {
        Self {
            id: preset.id.to_owned(),
            title: preset.title.to_owned(),
            purpose: preset.purpose.to_owned(),
            archetypes: preset
                .archetypes
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            skin: preset.skin.to_owned(),
            widgets: preset
                .widgets
                .iter()
                .map(|entry| HudWidgetView {
                    name: entry.name.to_owned(),
                    kind: entry.kind,
                    slot: entry.slot,
                    visibility: entry.visibility,
                    binding: entry.binding.to_owned(),
                    max_binding: entry.max_binding.to_owned(),
                    caption: entry.caption.to_owned(),
                    icon: entry.icon.to_owned(),
                    low_at: entry.low_at,
                })
                .collect(),
            persistent: u32::try_from(preset.persistent().count()).unwrap_or(u32::MAX),
            icon_roles: roles_for(preset)
                .into_iter()
                .map(|role| role.id.to_owned())
                .collect(),
            godot_nodes: preset
                .godot_nodes()
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }
}

/// The whole library, ready for the picker.
#[must_use]
pub fn library() -> HudLibraryView {
    HudLibraryView {
        presets: presets().iter().map(HudPresetView::of).collect(),
        skins: skins().iter().map(HudSkinView::of).collect(),
        roles: icon_roles()
            .iter()
            .map(|role| IconRoleView {
                id: role.id.to_owned(),
                title: role.title.to_owned(),
                keywords: role
                    .keywords
                    .iter()
                    .map(|word| (*word).to_owned())
                    .collect(),
            })
            .collect(),
        default_skin: DEFAULT_SKIN.to_owned(),
        max_persistent: u32::try_from(MAX_PERSISTENT_WIDGETS).unwrap_or(u32::MAX),
        min_font_size: i32::try_from(MIN_FONT_SIZE).unwrap_or(i32::MAX),
        safe_area_fraction: SAFE_AREA_FRACTION,
    }
}

/// The same library, ordered for one archetype: the HUDs that fit the game first.
#[must_use]
pub fn library_for(archetype: &str) -> HudLibraryView {
    let mut view = library();
    if !archetype.is_empty() {
        let order: Vec<String> = presets_for(archetype)
            .into_iter()
            .map(|entry| entry.id.to_owned())
            .collect();
        view.presets.sort_by_key(|entry| {
            order
                .iter()
                .position(|id| *id == entry.id)
                .unwrap_or(usize::MAX)
        });
    }
    view
}

/// What a project currently has.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudProjectState {
    /// True when the project carries a generated HUD scene.
    pub installed: bool,
    /// The preset id the installed HUD was built from, when the script still says so.
    pub preset: Option<String>,
    /// The skin id it was built with.
    pub skin: Option<String>,
    /// Icon roles this project already has art for.
    pub icons: BTreeMap<String, String>,
    /// True when the main scene carries a HUD instance.
    pub attached: bool,
}

/// Read what a project's HUD is, from the project.
///
/// The generated script names its own preset and skin in its header, which makes the
/// installed HUD self-describing: nothing has to be recorded in a manifest, and a project
/// edited outside Bhippi still answers honestly.
#[must_use]
pub fn project_state(project_root: &std::path::Path) -> HudProjectState {
    let scene = project_root.join(HUD_SCENE_REL);
    let installed = scene.is_file();
    let header = std::fs::read_to_string(project_root.join(HUD_SCRIPT_REL))
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("## Bhippi HUD"))
                .map(str::to_owned)
        });
    let (preset, skin) = header.map_or((None, None), |line| {
        let preset = line
            .split("preset ")
            .nth(1)
            .and_then(|rest| rest.split(',').next())
            .map(|value| value.trim().to_owned());
        let skin = line
            .split("skin ")
            .nth(1)
            .map(|value| value.trim().trim_end_matches('.').to_owned());
        (preset, skin)
    });

    let roles: Vec<&str> = icon_roles().iter().map(|role| role.id).collect();
    let attached = std::fs::read_to_string(project_root.join(super::scaffold::MAIN_SCENE_REL))
        .ok()
        .is_some_and(|text| main_scene_has_hud(&text));

    HudProjectState {
        installed,
        preset,
        skin,
        icons: crate::fab::project_icons(project_root, HUD_ICON_DIR, &roles),
        attached,
    }
}

// --------------------------------------------------------------------------- building

/// Everything the caller chooses about one HUD build.
#[derive(Clone, Debug)]
pub struct HudBuildOptions {
    /// A `preset.hud.*` id.
    pub preset: String,
    /// A skin id. Empty takes the preset's own default.
    pub skin: String,
    /// Where the HUD scene goes, project-relative.
    pub scene_rel: String,
    /// Where the driving script goes, project-relative.
    pub script_rel: String,
    /// The scene the HUD is instanced into, project-relative. `None` leaves it standalone,
    /// which is what a preview does.
    pub attach_to: Option<String>,
    /// Icon role to `res://` texture path. A role missing from this map draws a glyph.
    pub icons: BTreeMap<String, String>,
    /// The project already has a HUD scene at [`Self::scene_rel`], so the batch must delete
    /// it before writing the new one. Swapping preset or skin is the common case, and a
    /// build that could only ever run once would be useless.
    pub replace_scene: bool,
    /// The attach target already carries a `HUD` node, so the batch must detach it before
    /// instancing the new one.
    pub detach_existing: bool,
}

impl HudBuildOptions {
    /// The conventional build for a project that has no HUD yet: the standard paths, the
    /// preset's own skin, attached to the main scene.
    ///
    /// Prefer [`Self::for_project`] when the project is on disk — the two `replace` flags
    /// are facts about that project, and guessing them wrong is the difference between a
    /// rebuild and a refusal.
    #[must_use]
    pub fn new(preset: impl Into<String>) -> Self {
        Self {
            preset: preset.into(),
            skin: String::new(),
            scene_rel: HUD_SCENE_REL.to_owned(),
            script_rel: HUD_SCRIPT_REL.to_owned(),
            attach_to: Some(super::scaffold::MAIN_SCENE_REL.to_owned()),
            icons: BTreeMap::new(),
            replace_scene: false,
            detach_existing: false,
        }
    }

    /// The build for a real project: the icons it already carries, and the truth about
    /// whether it has a HUD to replace.
    ///
    /// The builder itself stays pure — it never looks at a disk — so the facts a rebuild
    /// depends on are gathered here, once, where they can be seen.
    #[must_use]
    pub fn for_project(project_root: &std::path::Path, preset_id: &str) -> Self {
        let mut options = Self::new(preset_id);
        options.replace_scene = project_root.join(&options.scene_rel).is_file();
        if let Some(entry) = preset(preset_id) {
            options.icons = icons_from_project(project_root, entry);
        }
        options.detach_existing = options
            .attach_to
            .as_ref()
            .and_then(|target| std::fs::read_to_string(project_root.join(target)).ok())
            .is_some_and(|text| main_scene_has_hud(&text));
        options
    }

    #[must_use]
    pub fn with_skin(mut self, skin: impl Into<String>) -> Self {
        self.skin = skin.into();
        self
    }

    #[must_use]
    pub fn with_icons(mut self, icons: BTreeMap<String, String>) -> Self {
        self.icons = icons;
        self
    }

    #[must_use]
    pub fn standalone(mut self) -> Self {
        self.attach_to = None;
        self.detach_existing = false;
        self
    }
}

/// True when a `.tscn` already carries a node named [`HUD_NODE_NAME`] under its root.
fn main_scene_has_hud(text: &str) -> bool {
    super::tscn::parse(text)
        .ok()
        .is_some_and(|document| document.node(HUD_NODE_NAME).is_some())
}

// The .tscn property helpers. Every value the builder writes goes through one of these, so
// a typo produces a compile error rather than a Godot property Godot silently ignores.
fn pf(name: &str, value: f64) -> (String, TscnValue) {
    (name.to_owned(), TscnValue::Float(value))
}

fn pi(name: &str, value: i64) -> (String, TscnValue) {
    (name.to_owned(), TscnValue::Int(value))
}

fn ps(name: &str, value: &str) -> (String, TscnValue) {
    (name.to_owned(), TscnValue::Str(value.to_owned()))
}

fn pv2(name: &str, x: f64, y: f64) -> (String, TscnValue) {
    (name.to_owned(), TscnValue::Vector2(x, y))
}

/// `MOUSE_FILTER_IGNORE`. Everything on a HUD except the one Button lets clicks through to
/// the game underneath.
const MOUSE_IGNORE: i64 = 2;
/// `SIZE_EXPAND_FILL`, for the spacer that pushes a bar's value to the right.
const SIZE_EXPAND_FILL: i64 = 3;
/// The reference width of a meter.
const METER_WIDTH: f64 = 220.0;
/// The reference size of a widget icon.
const ICON_SIZE: f64 = 26.0;
/// Pips emitted into the scene. The script hides the spare ones and clones more when a game
/// hands it a larger maximum, so the scene stays readable in the editor either way.
const DEFAULT_PIPS: usize = 5;
/// Ability or tower slots emitted into the scene.
const DEFAULT_SLOTS: usize = 4;

/// Accumulates the batch so a recipe reads as a node tree rather than as vector pushes.
struct Builder {
    scene: String,
    actions: Vec<GodotAction>,
}

impl Builder {
    fn node(
        &mut self,
        parent: &str,
        name: &str,
        type_: &str,
        properties: Vec<(String, TscnValue)>,
    ) -> String {
        self.actions.push(GodotAction::AddNode {
            scene: self.scene.clone(),
            parent: parent.to_owned(),
            name: name.to_owned(),
            type_: type_.to_owned(),
            properties,
            groups: Vec::new(),
        });
        if parent == "." {
            name.to_owned()
        } else {
            format!("{parent}/{name}")
        }
    }
}

/// A container that neither eats clicks nor draws anything of its own.
fn box_props(separation: f64, alignment: i64) -> Vec<(String, TscnValue)> {
    vec![
        pi("mouse_filter", MOUSE_IGNORE),
        pi(
            "theme_override_constants/separation",
            separation.round() as i64,
        ),
        pi("alignment", alignment),
    ]
}

/// The result of one HUD build: the batch plus what the caller needs to report.
#[derive(Clone, Debug)]
pub struct HudBuild {
    pub batch: GodotActionBatch,
    /// The preset that was built.
    pub preset: &'static HudPreset,
    /// The skin that was applied.
    pub skin: &'static HudSkin,
    /// Icon roles the preset asked for that no texture answered. These fall back to a
    /// caption, which is why a missing icon is reported rather than fatal.
    pub unresolved_icons: Vec<String>,
    /// Project-relative files the batch writes.
    pub files: Vec<String>,
}

/// Expand a preset into the actions that build it.
///
/// Nothing here touches the filesystem: the batch is data, applied by the same runner every
/// other Godot edit goes through, so a HUD build is journaled and undoable like any other.
pub fn build(options: &HudBuildOptions) -> Result<HudBuild> {
    let preset = preset(&options.preset).ok_or_else(|| {
        EngineError::NotFound(
            format!("no HUD preset {}", options.preset),
            Some(format!(
                "Known presets: {}.",
                presets()
                    .iter()
                    .map(|entry| entry.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        )
    })?;
    preset.validate()?;

    let skin_id = if options.skin.is_empty() {
        preset.skin
    } else {
        options.skin.as_str()
    };
    let skin = skin(skin_id).ok_or_else(|| {
        EngineError::NotFound(
            format!("no HUD skin {skin_id}"),
            Some(format!(
                "Known skins: {}.",
                skins()
                    .iter()
                    .map(|entry| entry.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        )
    })?;
    HudPreset::validate_skin(skin)?;

    let mut unresolved: Vec<String> = Vec::new();
    for entry in preset.widgets {
        if !entry.icon.is_empty()
            && !options.icons.contains_key(entry.icon)
            && !unresolved.contains(&entry.icon.to_owned())
        {
            unresolved.push(entry.icon.to_owned());
        }
    }

    let mut builder = Builder {
        scene: options.scene_rel.clone(),
        actions: Vec::new(),
    };

    // A rebuild replaces rather than merges: the scene is generated, so the old one carries
    // nothing worth keeping, and a merge would leave last preset's widgets behind.
    if options.detach_existing {
        if let Some(target) = &options.attach_to {
            builder.actions.push(GodotAction::RemoveNode {
                scene: target.clone(),
                path: HUD_NODE_NAME.to_owned(),
            });
        }
    }
    if options.replace_scene {
        builder.actions.push(GodotAction::DeleteScene {
            path: options.scene_rel.clone(),
        });
    }

    builder.actions.push(GodotAction::CreateScene {
        path: options.scene_rel.clone(),
        root_name: HUD_NODE_NAME.to_owned(),
        root_type: "CanvasLayer".to_owned(),
    });
    builder.actions.push(GodotAction::SetProperty {
        scene: options.scene_rel.clone(),
        path: ".".to_owned(),
        property: "layer".to_owned(),
        value: TscnValue::Int(HUD_CANVAS_LAYER),
    });

    // The one full-rect Control every slot hangs from. Anchored to the whole viewport and
    // transparent to the mouse, so the HUD never intercepts a click meant for the game.
    let root = builder.node(
        ".",
        "Root",
        "Control",
        vec![
            pf("anchor_right", 1.0),
            pf("anchor_bottom", 1.0),
            pi("mouse_filter", MOUSE_IGNORE),
        ],
    );

    // Every slot exists whether or not this preset uses it: moving a widget between corners
    // is then a reparent, not a new container.
    let mut slot_paths: BTreeMap<HudSlot, String> = BTreeMap::new();
    let inset = (SAFE_AREA_FRACTION * f64::from(super::scaffold::VIEWPORT_HEIGHT as i32)).round();
    for &slot in HudSlot::all() {
        let (al, at, ar, ab) = slot.anchors();
        let (grow_h, grow_v) = slot.grow();
        let (sign_x, sign_y) = slot.inset_sign();
        let dx = sign_x * inset;
        let dy = sign_y * inset;
        let path = builder.node(
            &root,
            slot.node_name(),
            "VBoxContainer",
            vec![
                pf("anchor_left", al),
                pf("anchor_top", at),
                pf("anchor_right", ar),
                pf("anchor_bottom", ab),
                pf("offset_left", dx),
                pf("offset_top", dy),
                pf("offset_right", dx),
                pf("offset_bottom", dy),
                pi("grow_horizontal", grow_h),
                pi("grow_vertical", grow_v),
                pi("mouse_filter", MOUSE_IGNORE),
                pi(
                    "theme_override_constants/separation",
                    skin.gap.round() as i64,
                ),
                pi("alignment", if slot.alignment() == 2 { 2 } else { 0 }),
            ],
        );
        slot_paths.insert(slot, path);
    }

    for entry in preset.widgets {
        let parent = slot_paths
            .get(&entry.slot)
            .cloned()
            .unwrap_or_else(|| root.clone());
        emit_widget(&mut builder, &parent, entry, skin, options);
    }

    let mut actions = builder.actions;

    // The gauge script only ships when something draws with it.
    let needs_gauge = preset.widgets.iter().any(|entry| entry.kind.is_gauge());
    let mut files = vec![options.scene_rel.clone(), options.script_rel.clone()];
    if needs_gauge {
        actions.push(GodotAction::WriteScript {
            path: HUD_GAUGE_SCRIPT_REL.to_owned(),
            source: gauge_script(),
        });
        files.push(HUD_GAUGE_SCRIPT_REL.to_owned());
    }

    actions.push(GodotAction::WriteScript {
        path: options.script_rel.clone(),
        source: hud_script(preset, skin, options),
    });
    actions.push(GodotAction::AttachScript {
        scene: options.scene_rel.clone(),
        path: ".".to_owned(),
        script_res_path: rel_to_res(&options.script_rel),
    });

    if needs_gauge {
        for entry in preset.widgets.iter().filter(|entry| entry.kind.is_gauge()) {
            let slot = slot_paths
                .get(&entry.slot)
                .cloned()
                .unwrap_or_else(|| root.clone());
            actions.push(GodotAction::AttachScript {
                scene: options.scene_rel.clone(),
                path: format!("{slot}/{}/Gauge", entry.name),
                script_res_path: rel_to_res(HUD_GAUGE_SCRIPT_REL),
            });
        }
    }

    if let Some(target) = &options.attach_to {
        actions.push(GodotAction::InstanceScene {
            scene: target.clone(),
            parent: ".".to_owned(),
            name: HUD_NODE_NAME.to_owned(),
            scene_res_path: rel_to_res(&options.scene_rel),
        });
    }

    Ok(HudBuild {
        batch: GodotActionBatch::new(format!("HUD: {} ({})", preset.title, skin.title), actions),
        preset,
        skin,
        unresolved_icons: unresolved,
        files,
    })
}

/// One widget's node recipe. The kinds are deliberately fixed: a preset chooses a kind, and
/// the tree that kind builds is the same in every game, so a HUD from one preset is legible
/// to anyone who has read another.
fn emit_widget(
    builder: &mut Builder,
    parent: &str,
    entry: &HudWidget,
    skin: &HudSkin,
    options: &HudBuildOptions,
) {
    let has_icon = !entry.icon.is_empty() && options.icons.contains_key(entry.icon);
    let caption = if skin.uppercase {
        entry.caption.to_uppercase()
    } else {
        entry.caption.to_owned()
    };
    let align = entry.slot.alignment();

    match entry.kind {
        HudWidgetKind::Bar => {
            let plate = builder.node(
                parent,
                entry.name,
                "PanelContainer",
                vec![pi("mouse_filter", MOUSE_IGNORE)],
            );
            let body = builder.node(&plate, "Body", "VBoxContainer", box_props(4.0, 0));
            let head = builder.node(&body, "Head", "HBoxContainer", box_props(8.0, 0));
            if has_icon {
                builder.node(
                    &head,
                    "Icon",
                    "TextureRect",
                    vec![
                        pv2("custom_minimum_size", ICON_SIZE, ICON_SIZE),
                        pi("expand_mode", 1),
                        pi("stretch_mode", 5),
                        pi("mouse_filter", MOUSE_IGNORE),
                    ],
                );
            }
            if !caption.is_empty() {
                builder.node(
                    &head,
                    "Caption",
                    "Label",
                    vec![ps("text", &caption), pi("mouse_filter", MOUSE_IGNORE)],
                );
            }
            builder.node(
                &head,
                "Spacer",
                "Control",
                vec![
                    pi("size_flags_horizontal", SIZE_EXPAND_FILL),
                    pi("mouse_filter", MOUSE_IGNORE),
                ],
            );
            builder.node(
                &head,
                "Value",
                "Label",
                vec![ps("text", "0"), pi("mouse_filter", MOUSE_IGNORE)],
            );
            let meter_node = builder.node(
                &body,
                "Meter",
                "Control",
                vec![
                    pv2("custom_minimum_size", METER_WIDTH, skin.bar_height),
                    pi("mouse_filter", MOUSE_IGNORE),
                ],
            );
            // Track fills the meter; ghost and fill are anchored to the left edge and sized
            // by the script, so the ghost can lag behind the fill and read as damage taken.
            builder.node(
                &meter_node,
                "Track",
                "ColorRect",
                vec![
                    pf("anchor_right", 1.0),
                    pf("anchor_bottom", 1.0),
                    pi("mouse_filter", MOUSE_IGNORE),
                ],
            );
            builder.node(
                &meter_node,
                "Ghost",
                "ColorRect",
                vec![pf("anchor_bottom", 1.0), pi("mouse_filter", MOUSE_IGNORE)],
            );
            builder.node(
                &meter_node,
                "Fill",
                "ColorRect",
                vec![pf("anchor_bottom", 1.0), pi("mouse_filter", MOUSE_IGNORE)],
            );
        }
        HudWidgetKind::Segments => {
            let plate = builder.node(
                parent,
                entry.name,
                "PanelContainer",
                vec![pi("mouse_filter", MOUSE_IGNORE)],
            );
            let body = builder.node(&plate, "Body", "HBoxContainer", box_props(8.0, 0));
            if !caption.is_empty() {
                builder.node(
                    &body,
                    "Caption",
                    "Label",
                    vec![ps("text", &caption), pi("mouse_filter", MOUSE_IGNORE)],
                );
            }
            let pips = builder.node(&body, "Pips", "HBoxContainer", box_props(5.0, 0));
            let pip_type = if has_icon { "TextureRect" } else { "ColorRect" };
            for index in 1..=DEFAULT_PIPS {
                let mut props = vec![
                    pv2("custom_minimum_size", ICON_SIZE, ICON_SIZE),
                    pi("mouse_filter", MOUSE_IGNORE),
                ];
                if has_icon {
                    props.push(pi("expand_mode", 1));
                    props.push(pi("stretch_mode", 5));
                }
                builder.node(&pips, &format!("Pip{index}"), pip_type, props);
            }
        }
        HudWidgetKind::Counter | HudWidgetKind::Timer => {
            let plate = builder.node(
                parent,
                entry.name,
                "PanelContainer",
                vec![pi("mouse_filter", MOUSE_IGNORE)],
            );
            let body = builder.node(&plate, "Body", "HBoxContainer", box_props(8.0, align));
            if has_icon {
                builder.node(
                    &body,
                    "Icon",
                    "TextureRect",
                    vec![
                        pv2("custom_minimum_size", ICON_SIZE, ICON_SIZE),
                        pi("expand_mode", 1),
                        pi("stretch_mode", 5),
                        pi("mouse_filter", MOUSE_IGNORE),
                    ],
                );
            }
            if !caption.is_empty() {
                builder.node(
                    &body,
                    "Caption",
                    "Label",
                    vec![ps("text", &caption), pi("mouse_filter", MOUSE_IGNORE)],
                );
            }
            let placeholder = if entry.kind == HudWidgetKind::Timer {
                "0:00"
            } else {
                "0"
            };
            builder.node(
                &body,
                "Value",
                "Label",
                vec![ps("text", placeholder), pi("mouse_filter", MOUSE_IGNORE)],
            );
        }
        HudWidgetKind::Text => {
            let plate = builder.node(
                parent,
                entry.name,
                "PanelContainer",
                vec![pi("mouse_filter", MOUSE_IGNORE)],
            );
            let body = builder.node(&plate, "Body", "VBoxContainer", box_props(2.0, align));
            if !caption.is_empty() {
                builder.node(
                    &body,
                    "Caption",
                    "Label",
                    vec![ps("text", &caption), pi("mouse_filter", MOUSE_IGNORE)],
                );
            }
            builder.node(
                &body,
                "Value",
                "Label",
                vec![ps("text", "—"), pi("mouse_filter", MOUSE_IGNORE)],
            );
        }
        HudWidgetKind::Reticle => {
            let size = entry.kind.min_size().unwrap_or((48.0, 48.0));
            let node = builder.node(
                parent,
                entry.name,
                "Control",
                vec![
                    pv2("custom_minimum_size", size.0, size.1),
                    pi("mouse_filter", MOUSE_IGNORE),
                ],
            );
            // Outline first so the light shape draws over it: two colours, never one.
            builder.node(
                &node,
                "Outline",
                "ColorRect",
                vec![pi("mouse_filter", MOUSE_IGNORE)],
            );
            builder.node(
                &node,
                "Dot",
                "ColorRect",
                vec![pi("mouse_filter", MOUSE_IGNORE)],
            );
            builder.node(
                &node,
                "Prompt",
                "Label",
                vec![
                    ps("text", ""),
                    pi("horizontal_alignment", 1),
                    pi("mouse_filter", MOUSE_IGNORE),
                ],
            );
        }
        HudWidgetKind::Ring | HudWidgetKind::Compass | HudWidgetKind::Minimap => {
            let size = entry.kind.min_size().unwrap_or((120.0, 120.0));
            let plate = builder.node(
                parent,
                entry.name,
                "PanelContainer",
                vec![pi("mouse_filter", MOUSE_IGNORE)],
            );
            let gauge = builder.node(
                &plate,
                "Gauge",
                "Control",
                vec![
                    pv2("custom_minimum_size", size.0, size.1),
                    pi("mouse_filter", MOUSE_IGNORE),
                ],
            );
            builder.node(
                &gauge,
                "Value",
                "Label",
                vec![
                    ps("text", ""),
                    pf("anchor_right", 1.0),
                    pf("anchor_bottom", 1.0),
                    pi("horizontal_alignment", 1),
                    pi("vertical_alignment", 1),
                    pi("mouse_filter", MOUSE_IGNORE),
                ],
            );
        }
        HudWidgetKind::Toast => {
            let stack = builder.node(
                parent,
                entry.name,
                "VBoxContainer",
                box_props(6.0, if align == 2 { 2 } else { align }),
            );
            let template = builder.node(
                &stack,
                "Template",
                "PanelContainer",
                vec![
                    ("visible".to_owned(), TscnValue::Bool(false)),
                    pi("mouse_filter", MOUSE_IGNORE),
                ],
            );
            let body = builder.node(&template, "Body", "HBoxContainer", box_props(8.0, 0));
            builder.node(
                &body,
                "Text",
                "Label",
                vec![ps("text", ""), pi("mouse_filter", MOUSE_IGNORE)],
            );
        }
        HudWidgetKind::IconRow => {
            let row = builder.node(parent, entry.name, "HBoxContainer", box_props(8.0, align));
            for index in 1..=DEFAULT_SLOTS {
                let plate = builder.node(
                    &row,
                    &format!("Slot{index}"),
                    "PanelContainer",
                    vec![pi("mouse_filter", MOUSE_IGNORE)],
                );
                let inner = builder.node(
                    &plate,
                    "Inner",
                    "Control",
                    vec![
                        pv2("custom_minimum_size", 52.0, 52.0),
                        pi("mouse_filter", MOUSE_IGNORE),
                    ],
                );
                builder.node(
                    &inner,
                    "Icon",
                    "TextureRect",
                    vec![
                        pf("anchor_right", 1.0),
                        pf("anchor_bottom", 1.0),
                        pi("expand_mode", 1),
                        pi("stretch_mode", 5),
                        pi("mouse_filter", MOUSE_IGNORE),
                    ],
                );
                // A cooldown wipes down over the slot rather than dimming it: a dimmed icon
                // and a disabled icon look the same, a wipe reads as time.
                builder.node(
                    &inner,
                    "Cooldown",
                    "ColorRect",
                    vec![pf("anchor_right", 1.0), pi("mouse_filter", MOUSE_IGNORE)],
                );
                builder.node(
                    &inner,
                    "Key",
                    "Label",
                    vec![
                        ps("text", &index.to_string()),
                        pf("anchor_left", 1.0),
                        pf("anchor_top", 1.0),
                        pf("anchor_right", 1.0),
                        pf("anchor_bottom", 1.0),
                        pi("grow_horizontal", 0),
                        pi("grow_vertical", 0),
                        pi("mouse_filter", MOUSE_IGNORE),
                    ],
                );
            }
        }
        HudWidgetKind::Action => {
            let plate = builder.node(parent, entry.name, "PanelContainer", Vec::new());
            let label = if caption.is_empty() {
                entry.name.to_owned()
            } else {
                caption.clone()
            };
            builder.node(&plate, "Button", "Button", vec![ps("text", &label)]);
        }
    }
}

// ------------------------------------------------------------------------- gdscript

fn gd_color(value: [f64; 4]) -> String {
    format!(
        "Color({:.4}, {:.4}, {:.4}, {:.4})",
        value[0], value[1], value[2], value[3]
    )
}

fn gd_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The path a widget's own node sits at inside the HUD scene.
fn widget_path(entry: &HudWidget) -> String {
    format!("Root/{}/{}", entry.slot.node_name(), entry.name)
}

/// The script the HUD root carries.
///
/// It is generated rather than templated because the data it drives — which widgets exist,
/// where they are and what they read — *is* the preset. The behaviour below is the same in
/// every game: the ghost drain, the tick, the low state and the toast lifecycle, written
/// once and given different rows to act on.
fn hud_script(preset: &HudPreset, skin: &HudSkin, options: &HudBuildOptions) -> String {
    let mut rows = String::new();
    for entry in preset.widgets {
        let icon = options.icons.get(entry.icon).cloned().unwrap_or_default();
        rows.push_str(&format!(
            "\t{{ \"name\": {}, \"path\": {}, \"kind\": {}, \"slot\": {}, \"visibility\": {}, \
             \"binding\": {}, \"max_binding\": {}, \"low_at\": {:.3}, \"icon\": {} }},\n",
            gd_string(entry.name),
            gd_string(&widget_path(entry)),
            gd_string(entry.kind.as_str()),
            gd_string(entry.slot.as_str()),
            gd_string(entry.visibility.as_str()),
            gd_string(entry.binding),
            gd_string(entry.max_binding),
            entry.low_at,
            gd_string(&icon),
        ));
    }

    let slot_rows = HudSlot::all()
        .iter()
        .map(|slot| {
            let (sx, sy) = slot.inset_sign();
            format!(
                "\t{{ \"path\": {}, \"x\": {:.1}, \"y\": {:.1} }},",
                gd_string(&format!("Root/{}", slot.node_name())),
                sx,
                sy
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r##"extends CanvasLayer
## Bhippi HUD — preset {preset_id}, skin {skin_id}.
##
## The scene beside this file holds the structure; this script holds the behaviour. Every
## colour, size and inset below comes from the skin dictionary, so set_skin() restyles a
## live HUD without touching a node. Every value arrives through set_value(), so the game
## never reaches into the HUD's node tree and the HUD never reaches into the game.
##
## Generated by Bhippi. Edit the preset, not this file — a rebuild overwrites it.

const REFERENCE_HEIGHT := {reference_height:.1}
const SAFE_AREA_FRACTION := {safe_fraction:.4}
## How long the ghost segment takes to catch up with a drop, in seconds.
const GHOST_SETTLE := 0.45
## The counter tick.
const TICK_TIME := 0.12
## A toast appears, holds for this long, then fades.
const TOAST_HOLD := 2.6
const TOAST_FADE := 0.45
## The low-state pulse period.
const PULSE_PERIOD := 1.2
## The meter width and pip size the skin scales from.
const METER_WIDTH := 220.0
const PIP_SIZE := 26.0
const SLOT_SIZE := 52.0

const WIDGETS := [
{rows}]

const SLOTS := [
{slot_rows}
]

var skin := {{
	"plate": {plate},
	"track": {track},
	"accent": {accent},
	"warn": {warn},
	"text": {text},
	"muted": {muted},
	"outline": {outline},
	"radius": {radius:.1},
	"border": {border:.1},
	"value_size": {value_size},
	"label_size": {label_size},
	"padding": {padding:.1},
	"gap": {gap:.1},
	"bar_height": {bar_height:.1},
	"uppercase": {uppercase},
}}

var _values := {{}}
var _maxima := {{}}
var _ghosts := {{}}
var _low := {{}}
var _scale := 1.0
var _pulse := 0.0


func _ready() -> void:
	var viewport := get_viewport()
	if viewport != null and not viewport.size_changed.is_connected(_relayout):
		viewport.size_changed.connect(_relayout)
	for row in WIDGETS:
		var maximum: String = row["max_binding"]
		if maximum != "":
			_maxima[maximum] = 1.0
	_relayout()
	apply_skin()
	for row in WIDGETS:
		_refresh(row)


# ------------------------------------------------------------- the public surface

## Push one game value in. Everything the HUD shows arrives this way.
func set_value(binding: String, value: Variant) -> void:
	var previous: Variant = _values.get(binding, null)
	_values[binding] = value
	for row in WIDGETS:
		if row["binding"] == binding:
			_refresh(row, previous)


## Set what a meter fills against. Call this before the first set_value() or the bar reads
## as full.
func set_max(binding: String, value: float) -> void:
	_maxima[binding] = maxf(value, 0.0001)
	for row in WIDGETS:
		if row["max_binding"] == binding:
			_refresh(row)


func get_value(binding: String) -> Variant:
	return _values.get(binding, null)


## The on-change channel. Everything the player needs to notice but not to watch goes here
## rather than onto a persistent element.
func notify(message: String, icon: Texture2D = null) -> void:
	for row in WIDGETS:
		if row["kind"] != "toast":
			continue
		var stack := get_node_or_null(NodePath(row["path"]))
		if stack == null:
			continue
		var template := stack.get_node_or_null("Template")
		if template == null:
			continue
		var toast := template.duplicate() as Control
		if toast == null:
			continue
		toast.visible = true
		toast.modulate = Color(1.0, 1.0, 1.0, 0.0)
		stack.add_child(toast)
		_style_plate(toast)
		var label := toast.get_node_or_null("Body/Text") as Label
		if label != null:
			_style_label(label, int(skin["value_size"]), skin["text"])
			label.text = message
		var image := toast.get_node_or_null("Body/Icon") as TextureRect
		if image != null and icon != null:
			image.texture = icon
		var tween := create_tween()
		tween.tween_property(toast, "modulate:a", 1.0, TOAST_FADE)
		tween.tween_interval(TOAST_HOLD)
		tween.tween_property(toast, "modulate:a", 0.0, TOAST_FADE)
		tween.tween_callback(toast.queue_free)
		return


## Swap the whole look without rebuilding a node. Pass any subset of the skin keys.
func set_skin(values: Dictionary) -> void:
	for key in values:
		skin[key] = values[key]
	_relayout()
	apply_skin()
	for row in WIDGETS:
		_refresh(row)


## Point a reticle at what it is over: "rest", "target" or "interact", plus the verb the
## player would press. State reads as shape, never as colour alone.
func set_reticle_state(state: String, prompt: String = "") -> void:
	for row in WIDGETS:
		if row["kind"] != "reticle":
			continue
		var node := get_node_or_null(NodePath(row["path"])) as Control
		if node == null:
			continue
		var inner := Vector2(6.0, 6.0) * _scale
		if state == "target":
			inner = Vector2(22.0, 3.0) * _scale
		elif state == "interact":
			inner = Vector2(4.0, 22.0) * _scale
		var dot := node.get_node_or_null("Dot") as Control
		if dot != null:
			dot.size = inner
			dot.position = (node.size - inner) * 0.5
		var outline := node.get_node_or_null("Outline") as Control
		if outline != null:
			var pad := 2.0 * _scale
			outline.size = inner + Vector2(pad, pad) * 2.0
			outline.position = (node.size - outline.size) * 0.5
		var label := node.get_node_or_null("Prompt") as Label
		if label != null:
			label.text = prompt
			label.visible = prompt != ""
			label.position = Vector2(0.0, node.size.y * 0.5 + 12.0 * _scale)
			label.size = Vector2(node.size.x, label.size.y)


## Hand the minimap what it should draw: an array of {{ "offset": Vector2, "role": String }}
## in metres relative to the player. Roles come from a fixed legend, so a colour never
## changes meaning between levels.
func set_map_targets(targets: Array) -> void:
	for row in WIDGETS:
		if row["kind"] != "minimap":
			continue
		var gauge := get_node_or_null(NodePath(String(row["path"]) + "/Gauge"))
		if gauge != null and gauge.has_method("set_blips"):
			gauge.call("set_blips", targets)


# ---------------------------------------------------------------- layout and skin

func _relayout() -> void:
	var viewport := get_viewport()
	var screen := Vector2(1280.0, 720.0)
	if viewport != null:
		screen = viewport.get_visible_rect().size
	# Sizes are authored at 1080p and scaled by the viewport, so 4K does not shrink the
	# score to nothing and a phone does not swallow it.
	_scale = clampf(screen.y / REFERENCE_HEIGHT, 0.6, 2.4)
	var inset := roundf(minf(screen.x, screen.y) * SAFE_AREA_FRACTION)
	var separation := int(roundf(float(skin["gap"]) * _scale))
	for entry in SLOTS:
		var node := get_node_or_null(NodePath(entry["path"])) as Control
		if node == null:
			continue
		var dx: float = float(entry["x"]) * inset
		var dy: float = float(entry["y"]) * inset
		node.offset_left = dx
		node.offset_right = dx
		node.offset_top = dy
		node.offset_bottom = dy
		node.add_theme_constant_override("separation", separation)


func apply_skin() -> void:
	for row in WIDGETS:
		var node := get_node_or_null(NodePath(row["path"])) as Control
		if node == null:
			continue
		var kind := String(row["kind"])
		var icon := String(row["icon"])
		match kind:
			"bar":
				_style_plate(node)
				_style_label(node.get_node_or_null("Body/Head/Caption") as Label,
					int(skin["label_size"]), skin["muted"])
				_style_label(node.get_node_or_null("Body/Head/Value") as Label,
					int(skin["value_size"]), skin["text"])
				_load_icon_into(node.get_node_or_null("Body/Head"), icon)
				var meter := node.get_node_or_null("Body/Meter") as Control
				if meter != null:
					meter.custom_minimum_size = Vector2(METER_WIDTH, float(skin["bar_height"])) * _scale
					_paint(meter.get_node_or_null("Track") as ColorRect, skin["track"])
					_paint(meter.get_node_or_null("Ghost") as ColorRect, skin["warn"])
					_paint(meter.get_node_or_null("Fill") as ColorRect, skin["accent"])
			"segments":
				_style_plate(node)
				_style_label(node.get_node_or_null("Body/Caption") as Label,
					int(skin["label_size"]), skin["muted"])
				_load_icon_into(node.get_node_or_null("Body/Pips"), icon)
			"counter", "timer":
				_style_plate(node)
				_style_label(node.get_node_or_null("Body/Caption") as Label,
					int(skin["label_size"]), skin["muted"])
				_style_label(node.get_node_or_null("Body/Value") as Label,
					int(skin["value_size"]), skin["text"])
				_load_icon_into(node.get_node_or_null("Body"), icon)
			"text":
				_style_plate(node)
				_style_label(node.get_node_or_null("Body/Caption") as Label,
					int(skin["label_size"]), skin["muted"])
				_style_label(node.get_node_or_null("Body/Value") as Label,
					int(skin["value_size"]), skin["text"])
			"reticle":
				_paint(node.get_node_or_null("Outline") as ColorRect, skin["outline"])
				_paint(node.get_node_or_null("Dot") as ColorRect, skin["text"])
				_style_label(node.get_node_or_null("Prompt") as Label,
					int(skin["label_size"]), skin["text"])
				set_reticle_state("rest")
			"ring", "compass", "minimap":
				_style_plate(node)
				var gauge := node.get_node_or_null("Gauge") as Control
				if gauge != null:
					gauge.custom_minimum_size = _gauge_size(kind) * _scale
					if gauge.has_method("configure"):
						gauge.call("configure", kind, skin)
				_style_label(node.get_node_or_null("Gauge/Value") as Label,
					int(skin["value_size"]), skin["text"])
			"toast":
				var template := node.get_node_or_null("Template") as Control
				if template != null:
					_style_plate(template)
					_style_label(template.get_node_or_null("Body/Text") as Label,
						int(skin["value_size"]), skin["text"])
			"icon_row":
				for child in node.get_children():
					_style_plate(child as Control)
					var inner := child.get_node_or_null("Inner") as Control
					if inner == null:
						continue
					inner.custom_minimum_size = Vector2(SLOT_SIZE, SLOT_SIZE) * _scale
					_paint(inner.get_node_or_null("Cooldown") as ColorRect, Color(0.0, 0.0, 0.0, 0.55))
					_style_label(inner.get_node_or_null("Key") as Label,
						int(skin["label_size"]), skin["muted"])
			"action":
				_style_plate(node)
				var button := node.get_node_or_null("Button") as Button
				if button != null:
					button.add_theme_font_size_override("font_size",
						int(roundf(float(skin["value_size"]) * _scale)))
					button.add_theme_color_override("font_color", skin["text"])


func _gauge_size(kind: String) -> Vector2:
	if kind == "compass":
		return Vector2(520.0, 40.0)
	if kind == "minimap":
		return Vector2(200.0, 200.0)
	return Vector2(96.0, 96.0)


## A plate: a box at the skin's alpha behind every readout. Bare text over a scene fails the
## contrast floor somewhere in every level.
func _style_plate(node: Control) -> void:
	if node == null:
		return
	var box := StyleBoxFlat.new()
	box.bg_color = skin["plate"]
	var radius := int(roundf(float(skin["radius"]) * _scale))
	box.corner_radius_top_left = radius
	box.corner_radius_top_right = radius
	box.corner_radius_bottom_left = radius
	box.corner_radius_bottom_right = radius
	var pad := int(roundf(float(skin["padding"]) * _scale))
	box.content_margin_left = pad
	box.content_margin_right = pad
	box.content_margin_top = int(roundf(float(pad) * 0.7))
	box.content_margin_bottom = int(roundf(float(pad) * 0.7))
	var border := int(roundf(float(skin["border"]) * _scale))
	if border > 0:
		box.border_width_left = border
		box.border_width_right = border
		box.border_width_top = border
		box.border_width_bottom = border
		box.border_color = skin["outline"]
	node.add_theme_stylebox_override("panel", box)


func _style_label(label: Label, points: int, colour: Color) -> void:
	if label == null:
		return
	label.add_theme_font_size_override("font_size", maxi(int(roundf(float(points) * _scale)), 12))
	label.add_theme_color_override("font_color", colour)
	# The outline is the second half of the contrast answer: the plate handles the block,
	# the outline handles the one glyph that overhangs it.
	label.add_theme_constant_override("outline_size", maxi(int(roundf(2.0 * _scale)), 1))
	label.add_theme_color_override("font_outline_color", skin["outline"])


func _paint(rect: ColorRect, colour: Color) -> void:
	if rect != null:
		rect.color = colour


## A role with no texture is not an error: the caption already carries the meaning, and a
## TextureRect with nothing in it is worse than no icon at all.
func _load_icon_into(parent: Node, path: String) -> void:
	if parent == null or path == "" or not ResourceLoader.exists(path):
		return
	var texture := load(path) as Texture2D
	if texture == null:
		return
	for child in parent.get_children():
		var rect := child as TextureRect
		if rect != null:
			rect.texture = texture


# --------------------------------------------------------------------- the widgets

func _refresh(row: Dictionary, previous: Variant = null) -> void:
	var node := get_node_or_null(NodePath(row["path"])) as Control
	if node == null:
		return
	var value: Variant = _values.get(row["binding"], null)
	var kind := String(row["kind"])
	if kind == "bar":
		_refresh_bar(node, row, value)
	elif kind == "segments":
		_refresh_segments(node, row, value)
	elif kind == "counter":
		_refresh_counter(node, value, previous)
	elif kind == "timer":
		_refresh_timer(node, value)
	elif kind == "text":
		var label := node.get_node_or_null("Body/Value") as Label
		if label != null:
			label.text = "—" if value == null else str(value)
	elif kind == "ring" or kind == "compass":
		_refresh_gauge(node, row, value)


func _ratio(row: Dictionary, value: Variant) -> float:
	if value == null:
		return 0.0
	var maximum := float(_maxima.get(row["max_binding"], 1.0))
	if maximum <= 0.0:
		return 0.0
	return clampf(float(value) / maximum, 0.0, 1.0)


func _refresh_bar(node: Control, row: Dictionary, value: Variant) -> void:
	var meter := node.get_node_or_null("Body/Meter") as Control
	if meter == null:
		return
	var ratio := _ratio(row, value)
	var width := maxf(meter.size.x, meter.custom_minimum_size.x)
	var key := String(row["name"])
	var fill := meter.get_node_or_null("Fill") as Control
	if fill != null:
		fill.offset_right = width * ratio
	var ghost := meter.get_node_or_null("Ghost") as Control
	var was := float(_ghosts.get(key, ratio))
	if ghost != null:
		if ratio >= was:
			# A gain has nothing to show: the ghost snaps forward with the fill.
			_ghosts[key] = ratio
			ghost.offset_right = width * ratio
		else:
			# A loss leaves the ghost behind and drains it, so the player reads how much
			# was taken rather than only what is left.
			ghost.offset_right = width * was
			var tween := create_tween()
			tween.tween_method(_set_ghost.bind(key, ghost, width), was, ratio, GHOST_SETTLE)
	var label := node.get_node_or_null("Body/Head/Value") as Label
	if label != null:
		var maximum := float(_maxima.get(row["max_binding"], 1.0))
		var current := 0.0 if value == null else float(value)
		label.text = "%d/%d" % [int(roundf(current)), int(roundf(maximum))]
	_apply_low_state(key, fill, ratio, float(row["low_at"]))


func _set_ghost(amount: float, key: String, ghost: Control, width: float) -> void:
	_ghosts[key] = amount
	if is_instance_valid(ghost):
		ghost.offset_right = width * amount


## Low is signalled three ways — the fill shifts hue, the value reads in the warning colour
## and the element itself pulses. Never a red vignette alone, and never colour on its own.
func _apply_low_state(key: String, fill: Control, ratio: float, low_at: float) -> void:
	var low := low_at > 0.0 and ratio <= low_at
	_low[key] = low
	var rect := fill as ColorRect
	if rect != null:
		rect.color = skin["warn"] if low else skin["accent"]


func _refresh_segments(node: Control, row: Dictionary, value: Variant) -> void:
	var pips := node.get_node_or_null("Body/Pips") as Control
	if pips == null:
		return
	var maximum := int(roundf(float(_maxima.get(row["max_binding"], 1.0))))
	var filled := 0 if value == null else int(roundf(float(value)))
	var children := pips.get_children()
	# The scene ships a handful of pips so it reads in the editor; a game with a larger
	# maximum clones the first rather than leaving the row short.
	while children.size() < maximum and children.size() > 0:
		var extra := children[0].duplicate()
		pips.add_child(extra)
		children = pips.get_children()
	for index in children.size():
		var pip := children[index] as Control
		if pip == null:
			continue
		pip.visible = index < maximum
		pip.custom_minimum_size = Vector2(PIP_SIZE, PIP_SIZE) * _scale
		var on := index < filled
		var rect := pip as ColorRect
		if rect != null:
			rect.color = skin["accent"] if on else skin["track"]
		else:
			pip.modulate = Color(1.0, 1.0, 1.0, 1.0) if on else Color(1.0, 1.0, 1.0, 0.25)


func _refresh_counter(node: Control, value: Variant, previous: Variant) -> void:
	var label := node.get_node_or_null("Body/Value") as Label
	if label == null:
		return
	label.text = "0" if value == null else str(value)
	if previous == null or previous == value:
		return
	# A short scale tick, not an odometer roll: the player should notice the change without
	# reading the digits twice.
	label.pivot_offset = label.size * 0.5
	var tween := create_tween()
	tween.tween_property(label, "scale", Vector2(1.14, 1.14), TICK_TIME * 0.4)
	tween.tween_property(label, "scale", Vector2.ONE, TICK_TIME * 0.6)


func _refresh_timer(node: Control, value: Variant) -> void:
	var label := node.get_node_or_null("Body/Value") as Label
	if label == null:
		return
	var seconds := 0.0 if value == null else float(value)
	var whole := int(seconds)
	label.text = "%d:%02d" % [whole / 60, whole % 60]


func _refresh_gauge(node: Control, row: Dictionary, value: Variant) -> void:
	var gauge := node.get_node_or_null("Gauge") as Control
	if gauge == null:
		return
	if String(row["kind"]) == "compass":
		if gauge.has_method("set_heading"):
			gauge.call("set_heading", 0.0 if value == null else float(value))
		return
	if gauge.has_method("set_ratio"):
		gauge.call("set_ratio", _ratio(row, value))
	var label := gauge.get_node_or_null("Value") as Label
	if label != null:
		label.text = "0" if value == null else str(int(roundf(float(value))))


func _process(delta: float) -> void:
	_pulse = fmod(_pulse + delta, PULSE_PERIOD)
	var wave := 0.82 + 0.18 * sin(_pulse / PULSE_PERIOD * TAU)
	for row in WIDGETS:
		if String(row["kind"]) != "bar":
			continue
		if not bool(_low.get(row["name"], false)):
			continue
		var node := get_node_or_null(NodePath(row["path"])) as Control
		if node != null:
			# The element pulses, not the screen.
			node.modulate.a = wave
"##,
        preset_id = preset.id,
        skin_id = skin.id,
        reference_height = REFERENCE_HEIGHT,
        safe_fraction = SAFE_AREA_FRACTION,
        rows = rows,
        slot_rows = slot_rows,
        plate = gd_color(skin.plate),
        track = gd_color(skin.track),
        accent = gd_color(skin.accent),
        warn = gd_color(skin.warn),
        text = gd_color(skin.text),
        muted = gd_color(skin.muted),
        outline = gd_color(skin.outline),
        radius = skin.radius,
        border = skin.border,
        value_size = skin.value_size,
        label_size = skin.label_size,
        padding = skin.padding,
        gap = skin.gap,
        bar_height = skin.bar_height,
        uppercase = skin.uppercase,
    )
}

/// The ring, compass and minimap are one script: all three project a value onto a shape, and
/// all three are drawn rather than assembled, because a radial meter built out of nodes is a
/// pile of rotated rectangles nobody can edit.
fn gauge_script() -> String {
    r##"extends Control
## Bhippi HUD gauge — a ring, a compass strip or a minimap.
##
## Configured by hud.gd, which passes the skin in. Nothing here reads the game directly.
## Generated by Bhippi; a rebuild overwrites it.

var mode := "ring"
var ratio := 0.0
var heading := 0.0
var map_range := 60.0
var north_up := true
var blips: Array = []

var track_colour := Color(1.0, 1.0, 1.0, 0.14)
var accent := Color(0.35, 0.72, 1.0, 1.0)
var warn := Color(1.0, 0.45, 0.35, 1.0)
var text_colour := Color(1.0, 1.0, 1.0, 1.0)
var muted_colour := Color(0.73, 0.77, 0.83, 1.0)
var thickness := 9.0

## The legend is fixed. A colour that means "enemy" in one level means "enemy" in all of them.
const LEGEND := {
	"player": Color(1.0, 1.0, 1.0, 1.0),
	"enemy": Color(0.95, 0.32, 0.32, 1.0),
	"objective": Color(1.0, 0.82, 0.25, 1.0),
	"pickup": Color(0.45, 0.85, 1.0, 1.0),
}

const CARDINALS := ["N", "E", "S", "W"]
## One tick every fifteen degrees: enough to read motion, few enough to read at all.
const TICKS := 24


func configure(kind: String, values: Dictionary) -> void:
	mode = kind
	track_colour = values.get("track", track_colour)
	accent = values.get("accent", accent)
	warn = values.get("warn", warn)
	text_colour = values.get("text", text_colour)
	muted_colour = values.get("muted", muted_colour)
	thickness = maxf(float(values.get("bar_height", 9.0)), 4.0)
	queue_redraw()


func set_ratio(value: float) -> void:
	ratio = clampf(value, 0.0, 1.0)
	queue_redraw()


func set_heading(value: float) -> void:
	heading = value
	queue_redraw()


func set_blips(values: Array) -> void:
	blips = values
	queue_redraw()


func _draw() -> void:
	if mode == "compass":
		_draw_compass()
	elif mode == "minimap":
		_draw_minimap()
	else:
		_draw_ring()


func _draw_ring() -> void:
	var centre := size * 0.5
	var radius := minf(size.x, size.y) * 0.5 - thickness
	if radius <= 0.0:
		return
	draw_arc(centre, radius, 0.0, TAU, 64, track_colour, thickness, true)
	if ratio <= 0.0:
		return
	# Filling from twelve o'clock clockwise is the one convention every player already has.
	var start := -PI * 0.5
	var colour := warn if ratio >= 0.85 else accent
	draw_arc(centre, radius, start, start + TAU * ratio, 64, colour, thickness, true)


func _draw_compass() -> void:
	var font := ThemeDB.fallback_font
	if font == null:
		return
	var mid := size.y * 0.5
	draw_line(Vector2(0.0, mid), Vector2(size.x, mid), track_colour, 2.0, true)
	var per_cardinal := TICKS / 4
	for index in TICKS:
		var bearing := float(index) * (360.0 / float(TICKS))
		var offset := fposmod(bearing - heading + 180.0, 360.0) - 180.0
		var x := size.x * 0.5 + offset / 180.0 * size.x
		if x < 0.0 or x > size.x:
			continue
		var cardinal := index % per_cardinal == 0
		var height := size.y * (0.42 if cardinal else 0.24)
		draw_line(Vector2(x, mid - height), Vector2(x, mid + height), muted_colour, 2.0, true)
		if cardinal:
			var letter: String = CARDINALS[index / per_cardinal]
			draw_string(font, Vector2(x - 6.0, mid - height - 4.0), letter,
				HORIZONTAL_ALIGNMENT_LEFT, -1, 16, text_colour)
	draw_line(Vector2(size.x * 0.5, 0.0), Vector2(size.x * 0.5, size.y), accent, 2.0, true)


func _draw_minimap() -> void:
	var centre := size * 0.5
	var radius := minf(size.x, size.y) * 0.5 - 2.0
	if radius <= 0.0:
		return
	# The border is a plate, not a frame: it separates the chart from the world behind it
	# without drawing a second box around a box.
	draw_circle(centre, radius, track_colour)
	for entry in blips:
		if not (entry is Dictionary):
			continue
		var row: Dictionary = entry
		var offset: Vector2 = row.get("offset", Vector2.ZERO)
		if not north_up:
			offset = offset.rotated(deg_to_rad(heading))
		var scaled := offset / maxf(map_range, 0.001) * radius
		if scaled.length() > radius:
			scaled = scaled.normalized() * radius
		var role: String = row.get("role", "pickup")
		draw_circle(centre + scaled, 4.0, LEGEND.get(role, muted_colour))
	draw_circle(centre, 4.0, LEGEND["player"])
"##
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::catalog;

    fn any_preset() -> &'static HudPreset {
        preset("preset.hud.ammo_health").expect("the fps HUD exists")
    }

    #[test]
    fn every_preset_obeys_the_doctrine() {
        for entry in presets() {
            entry
                .validate()
                .unwrap_or_else(|error| panic!("{}: {error}", entry.id));
        }
    }

    #[test]
    fn every_skin_is_readable_at_distance() {
        for entry in skins() {
            HudPreset::validate_skin(entry).unwrap_or_else(|error| panic!("{}: {error}", entry.id));
            assert!(
                entry.value_size >= MIN_FONT_SIZE,
                "{} sets a value size under the floor",
                entry.id
            );
        }
    }

    #[test]
    fn preset_ids_are_unique_and_prefixed() {
        let mut seen = BTreeSet::new();
        for entry in presets() {
            assert!(
                entry.id.starts_with("preset.hud."),
                "{} is not a preset.hud.* id",
                entry.id
            );
            assert!(seen.insert(entry.id), "{} is listed twice", entry.id);
        }
    }

    #[test]
    fn skin_ids_are_unique_and_the_default_exists() {
        let mut seen = BTreeSet::new();
        for entry in skins() {
            assert!(seen.insert(entry.id), "{} is listed twice", entry.id);
        }
        assert!(skin(DEFAULT_SKIN).is_some(), "the default skin must exist");
        for entry in presets() {
            assert!(
                skin(entry.skin).is_some(),
                "{} names unknown skin {}",
                entry.id,
                entry.skin
            );
        }
    }

    /// The one rule that keeps the two tables from drifting: a HUD preset the intent
    /// compiler can name must be a HUD preset the builder can build, and the card must
    /// promise exactly the nodes the builder emits.
    #[test]
    fn the_catalogue_and_the_builder_agree() {
        let cards: Vec<_> = catalog::presets()
            .iter()
            .filter(|card| card.id.starts_with("preset.hud."))
            .collect();
        assert_eq!(
            cards.len(),
            presets().len(),
            "the catalogue lists {} HUD cards and the library builds {}",
            cards.len(),
            presets().len()
        );
        for entry in presets() {
            let card = cards
                .iter()
                .find(|card| card.id == entry.id)
                .unwrap_or_else(|| panic!("{} has no catalogue card", entry.id));
            assert_eq!(
                card.godot_nodes.to_vec(),
                entry.godot_nodes(),
                "{} promises nodes the builder does not emit",
                entry.id
            );
            assert_eq!(card.title, entry.title, "{} titles differ", entry.id);
        }
    }

    #[test]
    fn every_node_the_builder_emits_is_a_catalogued_class() {
        for entry in presets() {
            for node in entry.godot_nodes() {
                assert!(
                    catalog::is_godot_class(node),
                    "{} builds {node}, which is not in GODOT_CLASSES",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn a_sixth_persistent_widget_is_refused() {
        const TOO_MANY: HudPreset = HudPreset {
            id: "preset.hud.overloaded",
            title: "Overloaded",
            purpose: "Six things the player must watch at once.",
            archetypes: &[],
            skin: "clean",
            widgets: &[
                widget(
                    "A",
                    HudWidgetKind::Text,
                    HudSlot::TopLeft,
                    HudVisibility::Persistent,
                    "a",
                    "",
                    "",
                ),
                widget(
                    "B",
                    HudWidgetKind::Text,
                    HudSlot::TopLeft,
                    HudVisibility::Persistent,
                    "b",
                    "",
                    "",
                ),
                widget(
                    "C",
                    HudWidgetKind::Text,
                    HudSlot::TopLeft,
                    HudVisibility::Persistent,
                    "c",
                    "",
                    "",
                ),
                widget(
                    "D",
                    HudWidgetKind::Text,
                    HudSlot::TopRight,
                    HudVisibility::Persistent,
                    "d",
                    "",
                    "",
                ),
                widget(
                    "E",
                    HudWidgetKind::Text,
                    HudSlot::TopRight,
                    HudVisibility::Persistent,
                    "e",
                    "",
                    "",
                ),
                widget(
                    "F",
                    HudWidgetKind::Text,
                    HudSlot::TopRight,
                    HudVisibility::Persistent,
                    "f",
                    "",
                    "",
                ),
            ],
            properties: &[],
        };
        let error = TOO_MANY.validate().expect_err("six is over the budget");
        assert!(error.to_string().contains("budget"), "{error}");
    }

    #[test]
    fn the_centre_of_the_screen_belongs_to_the_game() {
        const CENTRE_LABEL: HudPreset = HudPreset {
            id: "preset.hud.centred",
            title: "Centred",
            purpose: "A score parked over the middle of the screen.",
            archetypes: &[],
            skin: "clean",
            widgets: &[widget(
                "Score",
                HudWidgetKind::Counter,
                HudSlot::Centre,
                HudVisibility::Persistent,
                "game.score",
                "",
                "",
            )],
            properties: &[],
        };
        let error = CENTRE_LABEL
            .validate()
            .expect_err("only a reticle may sit in the centre");
        assert!(error.to_string().contains("centre"), "{error}");
    }

    #[test]
    fn nothing_persistent_sits_in_the_bottom_centre() {
        const BOTTOM: HudPreset = HudPreset {
            id: "preset.hud.bottom",
            title: "Bottom",
            purpose: "A meter under the player's feet.",
            archetypes: &[],
            skin: "clean",
            widgets: &[widget(
                "Hint",
                HudWidgetKind::Text,
                HudSlot::BottomCentre,
                HudVisibility::Persistent,
                "game.hint",
                "",
                "",
            )],
            properties: &[],
        };
        let error = BOTTOM
            .validate()
            .expect_err("the bottom centre stays clear");
        assert!(error.to_string().contains("bottom centre"), "{error}");
    }

    #[test]
    fn a_skin_under_the_font_floor_is_refused() {
        let mut tiny = *skin("clean").expect("clean exists");
        tiny.label_size = 11;
        let error = HudPreset::validate_skin(&tiny).expect_err("11 px is unreadable");
        assert!(error.to_string().contains("18"), "{error}");
    }

    #[test]
    fn a_meter_without_a_maximum_is_refused() {
        const NO_MAX: HudPreset = HudPreset {
            id: "preset.hud.no_max",
            title: "No maximum",
            purpose: "A bar with nothing to fill against.",
            archetypes: &[],
            skin: "clean",
            widgets: &[widget(
                "Health",
                HudWidgetKind::Bar,
                HudSlot::TopLeft,
                HudVisibility::Persistent,
                "player.health",
                "",
                "",
            )],
            properties: &[],
        };
        let error = NO_MAX.validate().expect_err("a bar needs a maximum");
        assert!(error.to_string().contains("maximum"), "{error}");
    }

    #[test]
    fn slots_anchor_to_a_point_and_grow_inwards() {
        assert_eq!(HudSlot::TopLeft.anchors(), (0.0, 0.0, 0.0, 0.0));
        assert_eq!(HudSlot::BottomRight.anchors(), (1.0, 1.0, 1.0, 1.0));
        // Right-anchored content grows leftwards, or a widening number leaves the screen.
        assert_eq!(HudSlot::TopRight.grow(), (0, 1));
        assert_eq!(HudSlot::BottomLeft.grow(), (1, 0));
        assert_eq!(HudSlot::Centre.grow(), (2, 2));
        assert_eq!(HudSlot::TopRight.inset_sign(), (-1.0, 1.0));
        assert_eq!(HudSlot::TopCentre.inset_sign(), (0.0, 1.0));
    }

    #[test]
    fn a_build_creates_the_scene_the_script_and_the_instance() {
        let build = build(&HudBuildOptions::new("preset.hud.lives_score"))
            .expect("the platformer HUD builds");
        let kinds: Vec<&str> = build.batch.actions.iter().map(GodotAction::kind).collect();
        assert_eq!(kinds.first(), Some(&"create_scene"));
        assert!(kinds.contains(&"write_script"), "{kinds:?}");
        assert!(kinds.contains(&"attach_script"), "{kinds:?}");
        assert!(kinds.contains(&"instance_scene"), "{kinds:?}");
        assert_eq!(build.skin.id, "pixel", "the preset picks its own skin");
    }

    #[test]
    fn every_slot_container_exists_even_when_empty() {
        let build =
            build(&HudBuildOptions::new("preset.hud.minimal")).expect("the minimal HUD builds");
        for slot in HudSlot::all() {
            let name = slot.node_name();
            assert!(
                build.batch.actions.iter().any(|action| matches!(
                    action,
                    GodotAction::AddNode { name: node, .. } if node == name
                )),
                "the {name} slot is missing, so a widget cannot be moved into it"
            );
        }
    }

    #[test]
    fn a_standalone_build_is_not_instanced_anywhere() {
        let build = build(&HudBuildOptions::new("preset.hud.minimal").standalone())
            .expect("a preview builds");
        assert!(
            !build
                .batch
                .actions
                .iter()
                .any(|action| action.kind() == "instance_scene"),
            "a preview must not touch the main scene"
        );
    }

    #[test]
    fn the_gauge_script_ships_only_when_something_draws() {
        let with_gauge =
            build(&HudBuildOptions::new("preset.hud.explore_map")).expect("the map HUD builds");
        assert!(
            with_gauge.files.contains(&HUD_GAUGE_SCRIPT_REL.to_owned()),
            "a minimap needs the gauge script"
        );
        let without =
            build(&HudBuildOptions::new("preset.hud.minimal")).expect("the minimal HUD builds");
        assert!(
            !without.files.contains(&HUD_GAUGE_SCRIPT_REL.to_owned()),
            "a HUD that draws nothing should not carry a drawing script"
        );
    }

    #[test]
    fn a_skin_override_reaches_the_generated_script() {
        let build = build(&HudBuildOptions::new("preset.hud.ammo_health").with_skin("candy"))
            .expect("the fps HUD builds in candy");
        assert_eq!(build.skin.id, "candy");
        let source = build
            .batch
            .actions
            .iter()
            .find_map(|action| match action {
                GodotAction::WriteScript { path, source } if path == HUD_SCRIPT_REL => Some(source),
                _ => None,
            })
            .expect("the HUD script is written");
        assert!(source.contains("skin candy"), "the header names the skin");
        assert!(
            source.contains("1.0000, 0.5500, 0.7500"),
            "the candy accent reaches the script"
        );
    }

    #[test]
    fn an_unknown_preset_or_skin_is_named_in_the_error() {
        let error = build(&HudBuildOptions::new("preset.hud.nope")).expect_err("no such preset");
        assert!(error.to_string().contains("preset.hud.nope"), "{error}");
        let error = build(&HudBuildOptions::new("preset.hud.minimal").with_skin("chrome"))
            .expect_err("no such skin");
        assert!(error.to_string().contains("chrome"), "{error}");
    }

    #[test]
    fn a_missing_icon_is_reported_rather_than_fatal() {
        let build =
            build(&HudBuildOptions::new("preset.hud.lives_score")).expect("it builds without art");
        assert!(
            build.unresolved_icons.contains(&"heart".to_owned()),
            "an unresolved role is reported: {:?}",
            build.unresolved_icons
        );
        // ...and no TextureRect is emitted for it, because an empty one draws nothing.
        assert!(
            !build.batch.actions.iter().any(|action| matches!(
                action,
                GodotAction::AddNode { type_, .. } if type_ == "TextureRect"
            )),
            "a role with no texture must not leave an empty TextureRect behind"
        );
    }

    #[test]
    fn a_resolved_icon_reaches_the_scene_and_the_script() {
        let mut icons = BTreeMap::new();
        icons.insert(
            "heart".to_owned(),
            "res://assets/ui/icons/heart.png".to_owned(),
        );
        icons.insert(
            "coin".to_owned(),
            "res://assets/ui/icons/coin.png".to_owned(),
        );
        icons.insert(
            "clock".to_owned(),
            "res://assets/ui/icons/clock.png".to_owned(),
        );
        let build = build(&HudBuildOptions::new("preset.hud.lives_score").with_icons(icons))
            .expect("it builds with art");
        assert!(
            build.unresolved_icons.is_empty(),
            "{:?}",
            build.unresolved_icons
        );
        assert!(
            build.batch.actions.iter().any(|action| matches!(
                action,
                GodotAction::AddNode { type_, .. } if type_ == "TextureRect"
            )),
            "a resolved role becomes a TextureRect"
        );
        let source = build
            .batch
            .actions
            .iter()
            .find_map(|action| match action {
                GodotAction::WriteScript { path, source } if path == HUD_SCRIPT_REL => Some(source),
                _ => None,
            })
            .expect("the HUD script is written");
        assert!(
            source.contains("res://assets/ui/icons/heart.png"),
            "the script loads the texture at runtime rather than baking an ext_resource"
        );
    }

    #[test]
    fn the_generated_script_carries_the_whole_public_surface() {
        let build = build(&HudBuildOptions::new(any_preset().id)).expect("it builds");
        let source = build
            .batch
            .actions
            .iter()
            .find_map(|action| match action {
                GodotAction::WriteScript { path, source } if path == HUD_SCRIPT_REL => Some(source),
                _ => None,
            })
            .expect("the HUD script is written");
        for method in [
            "func set_value(",
            "func set_max(",
            "func get_value(",
            "func notify(",
            "func set_skin(",
            "func set_reticle_state(",
            "func set_map_targets(",
        ] {
            assert!(source.contains(method), "the script is missing {method}");
        }
        assert!(
            source.contains("SAFE_AREA_FRACTION := 0.0400"),
            "the safe-area inset is baked in"
        );
    }

    #[test]
    fn an_archetype_gets_its_own_hud_first() {
        let ordered = presets_for("fps_arena");
        assert_eq!(ordered.len(), presets().len(), "nothing is dropped");
        assert_eq!(
            ordered[0].id, "preset.hud.ammo_health",
            "the shooter HUD leads for a shooter"
        );
    }

    /// A HUD you cannot change is not a HUD preset, it is a one-shot. Swapping preset or
    /// skin has to delete the generated scene and detach the old instance first.
    #[test]
    fn a_rebuild_replaces_the_old_hud_rather_than_refusing() {
        let fresh = build(&HudBuildOptions::new("preset.hud.minimal")).expect("first build");
        let kinds: Vec<&str> = fresh.batch.actions.iter().map(GodotAction::kind).collect();
        assert!(
            !kinds.contains(&"delete_scene") && !kinds.contains(&"remove_node"),
            "a project with no HUD has nothing to delete: {kinds:?}"
        );

        let mut options = HudBuildOptions::new("preset.hud.ammo_health");
        options.replace_scene = true;
        options.detach_existing = true;
        let again = build(&options).expect("second build");
        let kinds: Vec<&str> = again.batch.actions.iter().map(GodotAction::kind).collect();
        assert_eq!(
            kinds.first(),
            Some(&"remove_node"),
            "the old instance is detached before the scene it points at goes"
        );
        assert_eq!(
            kinds.get(1),
            Some(&"delete_scene"),
            "and the old scene goes before the new one is created: {kinds:?}"
        );
        assert_eq!(kinds.get(2), Some(&"create_scene"));
    }

    #[test]
    fn for_project_reads_the_replace_flags_off_disk() {
        let dir = std::env::temp_dir().join(format!("bhippi-hud-rebuild-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("scenes")).expect("scenes dir");

        let clean = HudBuildOptions::for_project(&dir, "preset.hud.minimal");
        assert!(
            !clean.replace_scene,
            "an empty project has no HUD to replace"
        );
        assert!(!clean.detach_existing);

        std::fs::write(dir.join(HUD_SCENE_REL), "[gd_scene format=3]\n").expect("hud scene");
        std::fs::write(
            dir.join(super::super::scaffold::MAIN_SCENE_REL),
            "[gd_scene format=3]\n\n[node name=\"Main\" type=\"Node3D\"]\n\n\
             [node name=\"HUD\" type=\"CanvasLayer\" parent=\".\"]\n",
        )
        .expect("main scene");

        let dirty = HudBuildOptions::for_project(&dir, "preset.hud.minimal");
        assert!(
            dirty.replace_scene,
            "the HUD scene is there and must go first"
        );
        assert!(
            dirty.detach_existing,
            "and so is the instance in the main scene"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_icon_a_preset_asks_for_is_a_known_role() {
        for entry in presets() {
            for widget in entry.widgets {
                if widget.icon.is_empty() {
                    continue;
                }
                assert!(
                    icon_role(widget.icon).is_some(),
                    "{} asks for icon role {}, which has no keywords to find it by",
                    entry.id,
                    widget.icon
                );
            }
        }
    }

    #[test]
    fn icon_roles_are_unique_and_carry_keywords() {
        let mut seen = BTreeSet::new();
        for role in icon_roles() {
            assert!(seen.insert(role.id), "{} is listed twice", role.id);
            assert!(
                !role.keywords.is_empty(),
                "{} has nothing to match on",
                role.id
            );
            // The first keyword is the word packs actually use, which is not always the
            // role's own name — energy meters are shipped as "lightning" far more often
            // than as "bolt". The role's name must still be reachable.
            assert!(
                role.keywords.contains(&role.id),
                "{} cannot be found by its own name",
                role.id
            );
        }
    }

    #[test]
    fn a_project_that_already_has_art_keeps_it() {
        let dir = std::env::temp_dir().join(format!("bhippi-hud-icons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(HUD_ICON_DIR)).expect("icon dir");
        std::fs::write(dir.join(HUD_ICON_DIR).join("heart.png"), b"x").expect("icon");

        let entry = preset("preset.hud.lives_score").expect("the platformer HUD exists");
        let icons = icons_from_project(&dir, entry);
        assert_eq!(
            icons.get("heart").map(String::as_str),
            Some("res://assets/ui/icons/heart.png")
        );

        let built = build(&HudBuildOptions::new(entry.id).with_icons(icons)).expect("it builds");
        assert!(
            !built.unresolved_icons.contains(&"heart".to_owned()),
            "the project's own heart answers the role"
        );
        assert!(
            built.unresolved_icons.contains(&"coin".to_owned()),
            "and the roles it does not cover are still reported"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
