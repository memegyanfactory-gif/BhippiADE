//! The HUD document (`bhippi-hud@1`, ENG-130/131/132/133).
//!
//! Until now a HUD "widget" was a scene entity carrying `UiDocument { layout: "health" }` —
//! a magic string with no fields behind it. That is why the owner could not change a
//! button's text: there was nowhere for the text to live. A health bar and a score label
//! were distinguishable only by their entity name.
//!
//! This is the file that fixes it. It is a real document with real widgets — text, buttons,
//! images, bars — each with a rectangle, a style, and where it makes sense a data binding
//! and a click action. It is deterministic sorted-key JSON, so it diffs cleanly, a model can
//! write it, and a person can open it in a text editor and change `"PAUSE"` to `"MENU"`.

use crate::document::SceneDocument;
use crate::error::{EngineError, Result};
use bhippi_types::AssetId;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const HUD_FORMAT: &str = "bhippi-hud@1";

/// A widget's id inside a HUD document. Stable across edits, like an entity id.
pub type WidgetId = String;

/// Which corner or edge a widget's offset is measured from.
///
/// Anchors rather than raw coordinates because a HUD has to survive a resize: "32px from the
/// top-right" stays right on every screen, where "at x=1888" is right only at 1920 wide.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    #[default]
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    /// Fill the parent, honouring `offset` as an inset on all sides.
    Stretch,
}

impl Anchor {
    /// The anchor's position in parent space as a 0..1 fraction.
    #[must_use]
    pub fn fraction(self) -> [f32; 2] {
        match self {
            Self::TopLeft | Self::Stretch => [0.0, 0.0],
            Self::TopCenter => [0.5, 0.0],
            Self::TopRight => [1.0, 0.0],
            Self::CenterLeft => [0.0, 0.5],
            Self::Center => [0.5, 0.5],
            Self::CenterRight => [1.0, 0.5],
            Self::BottomLeft => [0.0, 1.0],
            Self::BottomCenter => [0.5, 1.0],
            Self::BottomRight => [1.0, 1.0],
        }
    }
}

/// Where a widget sits and how big it is, in reference-resolution pixels.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Rect {
    pub anchor: Anchor,
    /// Offset from the anchor. Positive x is right, positive y is down.
    pub offset: [f32; 2],
    pub size: [f32; 2],
    /// Which point of the widget lands on the anchor (0,0 = its top-left, 1,1 = bottom-right).
    pub pivot: [f32; 2],
}

impl Default for Rect {
    fn default() -> Self {
        Self {
            anchor: Anchor::TopLeft,
            offset: [24.0, 24.0],
            size: [200.0, 40.0],
            pivot: [0.0, 0.0],
        }
    }
}

/// A widget's appearance. Everything optional falls back to the theme, so a widget written
/// with only `text` still renders.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct Style {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    /// The filled portion of a bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<f32>,
    /// Inner padding: `[horizontal, vertical]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<[f32; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    /// `asset:<ulid>` of a font, or a project-relative path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
}

/// What a button (or a key press) does. Deliberately a closed list: an action the runtime
/// cannot perform is a button that silently does nothing, which is worse than a rejected
/// document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum WidgetAction {
    PauseGame,
    ResumeGame,
    StopGame,
    QuitToMain,
    LoadLevel { level: String },
    SetVar { name: String, value: String },
    ToggleWidget { widget: WidgetId },
    CallScript { script: String, function: String },
}

/// The kinds of widget a HUD can contain.
///
/// Fixed, like the component registry, because the editor's Details panel, the runtime
/// renderer and the AI's schema excerpt all have to agree on the list.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    /// A container. Children position inside it.
    Panel,
    Text,
    Button,
    Image,
    ProgressBar,
    Crosshair,
    /// A row of repeated icons: lives, ammo.
    IconRow,
    Timer,
    Minimap,
    /// On-screen stick for touch builds.
    Joystick,
    /// Shows the key currently bound to an input action.
    KeyPrompt,
    List,
}

impl WidgetKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Panel => "panel",
            Self::Text => "text",
            Self::Button => "button",
            Self::Image => "image",
            Self::ProgressBar => "progress_bar",
            Self::Crosshair => "crosshair",
            Self::IconRow => "icon_row",
            Self::Timer => "timer",
            Self::Minimap => "minimap",
            Self::Joystick => "joystick",
            Self::KeyPrompt => "key_prompt",
            Self::List => "list",
        }
    }

    /// Every kind, in the order the Add-widget menu shows them.
    #[must_use]
    pub fn all() -> Vec<Self> {
        vec![
            Self::Panel,
            Self::Text,
            Self::Button,
            Self::Image,
            Self::ProgressBar,
            Self::Crosshair,
            Self::IconRow,
            Self::Timer,
            Self::Minimap,
            Self::Joystick,
            Self::KeyPrompt,
            Self::List,
        ]
    }

    /// Whether this kind can hold children.
    #[must_use]
    pub fn is_container(self) -> bool {
        matches!(self, Self::Panel | Self::List)
    }
}

/// One widget.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Widget {
    pub id: WidgetId,
    pub name: String,
    pub kind: WidgetKind,
    #[serde(default)]
    pub parent: Option<WidgetId>,
    /// Draw order among siblings; lower draws first.
    #[serde(default)]
    pub order: i32,
    #[serde(default = "yes")]
    pub visible: bool,
    /// Locked widgets cannot be selected or dragged on the canvas.
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub rect: Rect,
    #[serde(default)]
    pub style: Style,
    /// Runtime values this widget reads: `{"value": "player.health"}`.
    #[serde(default)]
    pub bind: BTreeMap<String, String>,
    /// Kind-specific fields, validated against [`widget_schema`].
    #[serde(default)]
    pub props: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_click: Option<WidgetAction>,
}

const fn yes() -> bool {
    true
}

impl Widget {
    /// A widget of `kind` with sensible defaults — what "Add → Button" produces.
    #[must_use]
    pub fn new(kind: WidgetKind, name: impl Into<String>) -> Self {
        let name = name.into();
        let mut props: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        let mut bind: BTreeMap<String, String> = BTreeMap::new();
        let mut rect = Rect::default();
        match kind {
            WidgetKind::Text => {
                props.insert("text".to_owned(), serde_json::json!(name.clone()));
            }
            WidgetKind::Button => {
                props.insert("text".to_owned(), serde_json::json!(name.to_uppercase()));
                rect.size = [140.0, 40.0];
            }
            WidgetKind::ProgressBar => {
                bind.insert("value".to_owned(), "player.health".to_owned());
                bind.insert("max".to_owned(), "player.health_max".to_owned());
                props.insert("show_text".to_owned(), serde_json::json!(true));
                rect.size = [260.0, 22.0];
            }
            WidgetKind::Timer => {
                bind.insert("value".to_owned(), "time.remaining".to_owned());
                props.insert("format".to_owned(), serde_json::json!("mm:ss"));
                rect.size = [120.0, 32.0];
            }
            WidgetKind::IconRow => {
                bind.insert("count".to_owned(), "player.lives".to_owned());
                props.insert("spacing".to_owned(), serde_json::json!(6.0));
            }
            WidgetKind::Crosshair => {
                rect.anchor = Anchor::Center;
                rect.pivot = [0.5, 0.5];
                rect.offset = [0.0, 0.0];
                rect.size = [24.0, 24.0];
            }
            _ => {}
        }
        Self {
            id: AssetId::new().to_string(),
            name,
            kind,
            parent: None,
            order: 0,
            visible: true,
            locked: false,
            rect,
            style: Style::default(),
            bind,
            props,
            on_click: None,
        }
    }
}

/// How the reference resolution maps onto the real screen.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ScaleMode {
    /// Scale to fit, preserving aspect — letterboxes rather than distorts.
    #[default]
    Fit,
    /// Scale to fill, cropping the overflow.
    Fill,
    /// Never scale; anchors still hold the layout together.
    Pixel,
}

/// The canvas the widgets are laid out on.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct Canvas {
    /// The resolution offsets and sizes are authored against.
    pub reference: [f32; 2],
    pub scale_mode: ScaleMode,
    /// Inset kept clear of screen edges, as a 0..0.5 fraction — TV overscan and phone notches.
    pub safe_area: f32,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            reference: [1920.0, 1080.0],
            scale_mode: ScaleMode::Fit,
            safe_area: 0.0,
        }
    }
}

/// One `assets/ui/*.hud.json` document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudDocument {
    pub format: String,
    pub id: AssetId,
    pub name: String,
    #[serde(default)]
    pub canvas: Canvas,
    #[serde(default)]
    pub widgets: Vec<Widget>,
}

impl HudDocument {
    #[must_use]
    pub fn empty(name: impl Into<String>) -> Self {
        Self {
            format: HUD_FORMAT.to_owned(),
            id: AssetId::new(),
            name: name.into(),
            canvas: Canvas::default(),
            widgets: Vec::new(),
        }
    }

    /// The HUD a new game ships with: a health bar, a score label and a pause button —
    /// three widgets that demonstrate binding, text and an action, and that a user can
    /// immediately edit.
    #[must_use]
    pub fn starter() -> Self {
        let mut doc = Self::empty("hud_main");

        let mut health = Widget::new(WidgetKind::ProgressBar, "HealthBar");
        health.rect.anchor = Anchor::TopLeft;
        health.rect.offset = [32.0, 32.0];
        health.style.bg = Some("#00000080".to_owned());
        health.style.fill = Some("#e0483c".to_owned());
        health.style.fg = Some("#ffffff".to_owned());
        health.style.radius = Some(6.0);
        health
            .props
            .insert("format".to_owned(), serde_json::json!("{value}/{max}"));

        let mut score = Widget::new(WidgetKind::Text, "ScoreLabel");
        score.rect.anchor = Anchor::TopCenter;
        score.rect.pivot = [0.5, 0.0];
        score.rect.offset = [0.0, 32.0];
        score.rect.size = [240.0, 36.0];
        score.style.font_size = Some(24.0);
        score.style.align = Some("center".to_owned());
        score
            .props
            .insert("text".to_owned(), serde_json::json!("Score: {score}"));
        score.bind.insert("score".to_owned(), "score".to_owned());
        score.order = 1;

        let mut pause = Widget::new(WidgetKind::Button, "PauseButton");
        pause.rect.anchor = Anchor::TopRight;
        pause.rect.pivot = [1.0, 0.0];
        pause.rect.offset = [-32.0, 32.0];
        pause.rect.size = [96.0, 34.0];
        pause.style.bg = Some("#151922cc".to_owned());
        pause.style.fg = Some("#e8eaf0".to_owned());
        pause.style.radius = Some(8.0);
        pause
            .props
            .insert("text".to_owned(), serde_json::json!("PAUSE"));
        pause.on_click = Some(WidgetAction::PauseGame);
        pause.order = 2;

        doc.widgets = vec![health, score, pause];
        doc
    }

    pub fn parse(text: &str) -> Result<Self> {
        let doc: Self = serde_json::from_str(text).map_err(|error| {
            EngineError::Asset(
                format!("invalid HUD document: {error}"),
                Some("HUDs are bhippi-hud@1 JSON; the Engine pane can re-create one.".to_owned()),
            )
        })?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn dump(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            EngineError::Asset(
                format!("cannot serialise HUD: {error}"),
                Some("Report this as an engine bug.".to_owned()),
            )
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != HUD_FORMAT {
            return Err(EngineError::Asset(
                format!("unsupported HUD format {:?}", self.format),
                Some(format!("Expected {HUD_FORMAT}.")),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(EngineError::Asset(
                "HUD name must not be empty".to_owned(),
                Some("Give the HUD a name.".to_owned()),
            ));
        }
        if self.canvas.reference[0] <= 0.0 || self.canvas.reference[1] <= 0.0 {
            return Err(EngineError::Asset(
                "canvas reference resolution must be positive".to_owned(),
                Some("Use something like [1920, 1080].".to_owned()),
            ));
        }
        if !(0.0..0.5).contains(&self.canvas.safe_area) {
            return Err(EngineError::Asset(
                format!("safe_area must be 0..0.5, got {}", self.canvas.safe_area),
                Some("A safe area of half the screen would leave nothing to draw in.".to_owned()),
            ));
        }

        let mut ids = BTreeSet::new();
        for widget in &self.widgets {
            if widget.id.trim().is_empty() {
                return Err(EngineError::Asset(
                    format!("widget {:?} has no id", widget.name),
                    Some("Every widget needs a stable id.".to_owned()),
                ));
            }
            if !ids.insert(widget.id.as_str()) {
                return Err(EngineError::Asset(
                    format!("duplicate widget id {}", widget.id),
                    Some("Widget ids must be unique inside a HUD.".to_owned()),
                ));
            }
            if widget.name.trim().is_empty() {
                return Err(EngineError::Asset(
                    format!("widget {} has an empty name", widget.id),
                    Some("Give every widget a name.".to_owned()),
                ));
            }
            validate_props(widget)?;
        }
        for widget in &self.widgets {
            if let Some(parent) = &widget.parent {
                let Some(found) = self.widget(parent) else {
                    return Err(EngineError::Asset(
                        format!("widget {} references missing parent {parent}", widget.name),
                        Some("Re-parent it, or remove the parent field.".to_owned()),
                    ));
                };
                if !found.kind.is_container() {
                    return Err(EngineError::Asset(
                        format!(
                            "widget {} is parented to a {}, which cannot hold children",
                            widget.name,
                            found.kind.as_str()
                        ),
                        Some("Only panel and list can contain other widgets.".to_owned()),
                    ));
                }
            }
        }
        self.detect_cycles()?;
        Ok(())
    }

    /// A parent chain that loops would hang the renderer, so it is refused at the door —
    /// the same rule scenes have.
    fn detect_cycles(&self) -> Result<()> {
        for widget in &self.widgets {
            let mut seen = BTreeSet::new();
            let mut current = Some(widget.id.clone());
            while let Some(step) = current {
                if !seen.insert(step.clone()) {
                    return Err(EngineError::Asset(
                        format!("widget {step} is in a parent cycle"),
                        Some("Undo the last re-parent.".to_owned()),
                    ));
                }
                current = self.widget(&step).and_then(|found| found.parent.clone());
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn widget(&self, id: &str) -> Option<&Widget> {
        self.widgets.iter().find(|widget| widget.id == id)
    }

    #[must_use]
    pub fn widget_mut(&mut self, id: &str) -> Option<&mut Widget> {
        self.widgets.iter_mut().find(|widget| widget.id == id)
    }

    /// Widgets with no parent, in draw order.
    #[must_use]
    pub fn roots(&self) -> Vec<&Widget> {
        let mut roots: Vec<&Widget> = self
            .widgets
            .iter()
            .filter(|widget| widget.parent.is_none())
            .collect();
        roots.sort_by_key(|widget| widget.order);
        roots
    }

    #[must_use]
    pub fn children_of(&self, id: &str) -> Vec<&Widget> {
        let mut children: Vec<&Widget> = self
            .widgets
            .iter()
            .filter(|widget| widget.parent.as_deref() == Some(id))
            .collect();
        children.sort_by_key(|widget| widget.order);
        children
    }

    /// Every binding path this HUD reads, deduplicated — what the runtime must supply.
    #[must_use]
    pub fn binding_paths(&self) -> Vec<String> {
        let mut paths: BTreeSet<&str> = BTreeSet::new();
        for widget in &self.widgets {
            for path in widget.bind.values() {
                paths.insert(path.as_str());
            }
        }
        paths.into_iter().map(str::to_owned).collect()
    }
}

/// Upgrade the retired HUD-as-scene shape into a real HUD document (ENG-139).
///
/// IDs come from the legacy scene/entity IDs, so running the upgrader twice over the same
/// input produces byte-identical JSON. The caller decides when to write the new file; this
/// pure conversion never alters or removes the legacy source.
pub fn upgrade_legacy_scene(scene: &SceneDocument) -> Result<HudDocument> {
    let mut hud = HudDocument {
        format: HUD_FORMAT.to_owned(),
        id: AssetId::from_ulid(scene.id.into_ulid()),
        name: if scene.name.trim().is_empty() {
            "hud_main".to_owned()
        } else {
            scene.name.clone()
        },
        canvas: Canvas::default(),
        widgets: Vec::new(),
    };

    for (order, entity) in scene.entities.iter().enumerate() {
        let Some(layout) = entity
            .components
            .get("UiDocument")
            .and_then(|value| value.get("layout"))
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let kind = match layout {
            "health" => WidgetKind::ProgressBar,
            "pause" | "menu" => WidgetKind::Button,
            "score" | "timer" | "text" => WidgetKind::Text,
            _ => WidgetKind::Panel,
        };
        let mut widget = Widget::new(kind, entity.name.clone());
        widget.id = entity.id.to_string();
        widget.order = i32::try_from(order).unwrap_or(i32::MAX);
        if let Some(pos) = entity
            .components
            .get("Transform")
            .and_then(|value| value.get("pos"))
            .and_then(serde_json::Value::as_array)
            .filter(|parts| parts.len() >= 2)
        {
            let x = pos[0].as_f64().unwrap_or(0.0) as f32;
            let y = pos[1].as_f64().unwrap_or(0.0) as f32;
            widget.rect.offset = [
                (x + 0.5) * hud.canvas.reference[0],
                (0.5 - y) * hud.canvas.reference[1],
            ];
        }
        match kind {
            WidgetKind::ProgressBar => {
                widget.rect.size = [320.0, 32.0];
                widget
                    .bind
                    .insert("value".to_owned(), "player.health".to_owned());
                widget.style.fill = Some("#e0483c".to_owned());
            }
            WidgetKind::Text => {
                widget.rect.size = [240.0, 40.0];
                widget.props.insert(
                    "text".to_owned(),
                    serde_json::json!(if layout == "score" {
                        "Score: {score}"
                    } else {
                        &entity.name
                    }),
                );
                if layout == "score" {
                    widget.bind.insert("score".to_owned(), "score".to_owned());
                }
            }
            WidgetKind::Button => {
                widget.rect.size = [120.0, 40.0];
                widget.props.insert(
                    "text".to_owned(),
                    serde_json::json!(entity.name.to_uppercase()),
                );
                widget.on_click = Some(WidgetAction::PauseGame);
            }
            _ => {}
        }
        hud.widgets.push(widget);
    }
    hud.validate()?;
    Ok(hud)
}

/// One field a widget kind understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PropSchema {
    pub name: &'static str,
    pub kind: PropKind,
    pub doc: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    Text,
    Number,
    Bool,
    /// An asset reference or project-relative path.
    Asset,
    Enum(&'static [&'static str]),
}

impl PropKind {
    fn describe(self) -> String {
        match self {
            Self::Text => "text".to_owned(),
            Self::Number => "number".to_owned(),
            Self::Bool => "bool".to_owned(),
            Self::Asset => "asset".to_owned(),
            Self::Enum(values) => format!("enum({})", values.join("|")),
        }
    }
}

/// The editable fields of one widget kind. The Details panel renders from this, and the AI's
/// schema excerpt is produced from it, so the two cannot drift apart.
#[must_use]
pub fn widget_schema(kind: WidgetKind) -> &'static [PropSchema] {
    match kind {
        WidgetKind::Panel => &[
            PropSchema {
                name: "layout",
                kind: PropKind::Enum(&["free", "row", "column"]),
                doc: "How children are arranged.",
            },
            PropSchema {
                name: "gap",
                kind: PropKind::Number,
                doc: "Spacing between children in row/column layout.",
            },
            PropSchema {
                name: "scroll",
                kind: PropKind::Bool,
                doc: "Clip and scroll overflowing children.",
            },
        ],
        WidgetKind::Text => &[
            PropSchema {
                name: "text",
                kind: PropKind::Text,
                doc: "The string to draw. {name} substitutes a binding.",
            },
            PropSchema {
                name: "wrap",
                kind: PropKind::Bool,
                doc: "Wrap at the widget width.",
            },
            PropSchema {
                name: "max_lines",
                kind: PropKind::Number,
                doc: "Truncate past this many lines.",
            },
            PropSchema {
                name: "uppercase",
                kind: PropKind::Bool,
                doc: "Render in capitals.",
            },
        ],
        WidgetKind::Button => &[
            PropSchema {
                name: "text",
                kind: PropKind::Text,
                doc: "The label.",
            },
            PropSchema {
                name: "icon",
                kind: PropKind::Asset,
                doc: "Optional icon drawn before the label.",
            },
            PropSchema {
                name: "disabled",
                kind: PropKind::Bool,
                doc: "Greyed out and unclickable.",
            },
            PropSchema {
                name: "tooltip",
                kind: PropKind::Text,
                doc: "Hover text.",
            },
        ],
        WidgetKind::Image => &[
            PropSchema {
                name: "source",
                kind: PropKind::Asset,
                doc: "The texture to draw.",
            },
            PropSchema {
                name: "fit",
                kind: PropKind::Enum(&["contain", "cover", "stretch"]),
                doc: "How it fills its rect.",
            },
            PropSchema {
                name: "tint",
                kind: PropKind::Text,
                doc: "Colour multiplied over the image.",
            },
        ],
        WidgetKind::ProgressBar => &[
            PropSchema {
                name: "direction",
                kind: PropKind::Enum(&["left_to_right", "right_to_left", "bottom_to_top"]),
                doc: "Fill direction.",
            },
            PropSchema {
                name: "show_text",
                kind: PropKind::Bool,
                doc: "Draw the numeric value over the bar.",
            },
            PropSchema {
                name: "format",
                kind: PropKind::Text,
                doc: "Text format, e.g. {value}/{max}.",
            },
        ],
        WidgetKind::Crosshair => &[
            PropSchema {
                name: "style",
                kind: PropKind::Enum(&["cross", "dot", "circle", "brackets"]),
                doc: "Crosshair shape.",
            },
            PropSchema {
                name: "thickness",
                kind: PropKind::Number,
                doc: "Line thickness.",
            },
        ],
        WidgetKind::IconRow => &[
            PropSchema {
                name: "source",
                kind: PropKind::Asset,
                doc: "The icon drawn once per count.",
            },
            PropSchema {
                name: "spacing",
                kind: PropKind::Number,
                doc: "Gap between icons.",
            },
            PropSchema {
                name: "max",
                kind: PropKind::Number,
                doc: "Stop drawing past this many.",
            },
        ],
        WidgetKind::Timer => &[
            PropSchema {
                name: "format",
                kind: PropKind::Enum(&["mm:ss", "ss", "hh:mm:ss"]),
                doc: "Time format.",
            },
            PropSchema {
                name: "count_down",
                kind: PropKind::Bool,
                doc: "Count down rather than up.",
            },
        ],
        WidgetKind::Minimap => &[
            PropSchema {
                name: "zoom",
                kind: PropKind::Number,
                doc: "World units across the map.",
            },
            PropSchema {
                name: "shape",
                kind: PropKind::Enum(&["square", "circle"]),
                doc: "Map shape.",
            },
        ],
        WidgetKind::Joystick => &[
            PropSchema {
                name: "side",
                kind: PropKind::Enum(&["left", "right"]),
                doc: "Which thumb it is under.",
            },
            PropSchema {
                name: "dead_zone",
                kind: PropKind::Number,
                doc: "Ignored travel from centre, 0..1.",
            },
        ],
        WidgetKind::KeyPrompt => &[PropSchema {
            name: "action",
            kind: PropKind::Text,
            doc: "The input action whose key is shown.",
        }],
        WidgetKind::List => &[
            PropSchema {
                name: "max_rows",
                kind: PropKind::Number,
                doc: "Rows drawn before overflow.",
            },
            PropSchema {
                name: "row_height",
                kind: PropKind::Number,
                doc: "Height of one row.",
            },
        ],
    }
}

/// A model-readable description of a widget kind, echoed beside a rejection (ENG-112's rule,
/// applied to HUDs).
#[must_use]
pub fn excerpt(kind: WidgetKind) -> String {
    let mut out = format!("{} — fields:\n", kind.as_str());
    for prop in widget_schema(kind) {
        out.push_str(&format!(
            "  {}: {}  — {}\n",
            prop.name,
            prop.kind.describe(),
            prop.doc
        ));
    }
    out.push_str("  (all widgets also have: rect, style, bind, visible, locked, order)\n");
    out
}

/// Parse a widget kind from its wire name.
#[must_use]
pub fn kind_from_str(name: &str) -> Option<WidgetKind> {
    WidgetKind::all()
        .into_iter()
        .find(|kind| kind.as_str() == name)
}

fn validate_props(widget: &Widget) -> Result<()> {
    let schema = widget_schema(widget.kind);
    for (name, value) in &widget.props {
        let Some(prop) = schema.iter().find(|prop| prop.name == name) else {
            return Err(EngineError::Asset(
                format!(
                    "{} has no property {name:?} on a {} widget",
                    widget.name,
                    widget.kind.as_str()
                ),
                Some(format!(
                    "Known: {}",
                    schema
                        .iter()
                        .map(|prop| prop.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            ));
        };
        let ok = match prop.kind {
            PropKind::Text | PropKind::Asset => value.is_string(),
            PropKind::Number => value.is_number(),
            PropKind::Bool => value.is_boolean(),
            PropKind::Enum(values) => value.as_str().is_some_and(|text| values.contains(&text)),
        };
        if !ok {
            return Err(EngineError::Asset(
                format!(
                    "{}.{name} must be {}, got {value}",
                    widget.name,
                    prop.kind.describe()
                ),
                Some(excerpt(widget.kind)),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        excerpt, kind_from_str, Anchor, HudDocument, Widget, WidgetAction, WidgetKind, HUD_FORMAT,
    };

    #[test]
    fn the_starter_hud_round_trips_and_is_editable() {
        let hud = HudDocument::starter();
        hud.validate().expect("valid");
        let text = hud.dump().expect("dump");
        assert_eq!(HudDocument::parse(&text).expect("parse"), hud);

        // The point of the whole format: the button's text is a field you can change.
        let pause = hud
            .widgets
            .iter()
            .find(|widget| widget.name == "PauseButton")
            .expect("pause button");
        assert_eq!(pause.props["text"], "PAUSE");
        assert_eq!(pause.on_click, Some(WidgetAction::PauseGame));
        assert_eq!(pause.rect.anchor, Anchor::TopRight);
    }

    #[test]
    fn a_widget_rejects_a_property_its_kind_does_not_have() {
        let mut hud = HudDocument::empty("hud");
        let mut text = Widget::new(WidgetKind::Text, "Label");
        text.props
            .insert("thickness".to_owned(), serde_json::json!(2.0));
        hud.widgets = vec![text];
        let error = hud.validate().expect_err("crosshair prop on a text widget");
        assert!(error.hint().is_some_and(|hint| hint.contains("uppercase")));
    }

    #[test]
    fn a_property_of_the_wrong_type_is_rejected_with_the_schema() {
        let mut hud = HudDocument::empty("hud");
        let mut bar = Widget::new(WidgetKind::ProgressBar, "Health");
        bar.props
            .insert("direction".to_owned(), serde_json::json!("sideways"));
        hud.widgets = vec![bar];
        let error = hud.validate().expect_err("bad enum");
        let hint = error.hint().expect("hint");
        assert!(
            hint.contains("left_to_right"),
            "the real values are offered"
        );
    }

    #[test]
    fn only_containers_may_have_children() {
        let mut hud = HudDocument::empty("hud");
        let panel = Widget::new(WidgetKind::Panel, "Root");
        let mut child = Widget::new(WidgetKind::Text, "Child");
        child.parent = Some(panel.id.clone());
        hud.widgets = vec![panel.clone(), child.clone()];
        hud.validate().expect("a panel may hold a child");

        let button = Widget::new(WidgetKind::Button, "Btn");
        let mut orphan = Widget::new(WidgetKind::Text, "Nope");
        orphan.parent = Some(button.id.clone());
        hud.widgets = vec![button, orphan];
        let error = hud.validate().expect_err("a button is not a container");
        assert!(error.hint().is_some_and(|hint| hint.contains("panel")));
    }

    #[test]
    fn a_parent_cycle_is_refused_rather_than_hanging_the_renderer() {
        let mut hud = HudDocument::empty("hud");
        let mut a = Widget::new(WidgetKind::Panel, "A");
        let mut b = Widget::new(WidgetKind::Panel, "B");
        a.parent = Some(b.id.clone());
        b.parent = Some(a.id.clone());
        hud.widgets = vec![a, b];
        assert!(hud.validate().is_err());
    }

    #[test]
    fn duplicate_widget_ids_and_missing_parents_are_rejected() {
        let mut hud = HudDocument::empty("hud");
        let one = Widget::new(WidgetKind::Text, "One");
        let mut two = Widget::new(WidgetKind::Text, "Two");
        two.id.clone_from(&one.id);
        hud.widgets = vec![one.clone(), two];
        assert!(hud.validate().is_err());

        let mut orphan = Widget::new(WidgetKind::Text, "Orphan");
        orphan.parent = Some("nope".to_owned());
        hud.widgets = vec![orphan];
        let error = hud.validate().expect_err("missing parent");
        assert!(error.hint().is_some());
    }

    #[test]
    fn roots_and_children_come_back_in_draw_order() {
        let hud = HudDocument::starter();
        let names: Vec<&str> = hud
            .roots()
            .iter()
            .map(|widget| widget.name.as_str())
            .collect();
        assert_eq!(names, vec!["HealthBar", "ScoreLabel", "PauseButton"]);
    }

    #[test]
    fn binding_paths_report_what_the_runtime_must_supply() {
        let paths = HudDocument::starter().binding_paths();
        assert!(paths.contains(&"player.health".to_owned()));
        assert!(paths.contains(&"player.health_max".to_owned()));
        assert!(paths.contains(&"score".to_owned()));
    }

    #[test]
    fn the_safe_area_cannot_swallow_the_screen() {
        let mut hud = HudDocument::empty("hud");
        hud.canvas.safe_area = 0.8;
        let error = hud.validate().expect_err("absurd safe area");
        assert!(error.hint().is_some());
    }

    #[test]
    fn every_kind_has_a_schema_and_a_stable_wire_name() {
        for kind in WidgetKind::all() {
            assert_eq!(kind_from_str(kind.as_str()), Some(kind));
            let text = excerpt(kind);
            assert!(text.starts_with(kind.as_str()));
            assert!(text.contains("rect, style, bind"));
        }
        assert!(kind_from_str("hologram").is_none());
    }

    #[test]
    fn a_future_format_marker_is_refused_with_the_expected_one() {
        let mut hud = HudDocument::starter();
        hud.format = "bhippi-hud@2".to_owned();
        let text = serde_json::to_string(&hud).expect("serialise");
        let error = HudDocument::parse(&text).expect_err("future format");
        assert!(error.hint().is_some_and(|hint| hint.contains(HUD_FORMAT)));
    }
}
