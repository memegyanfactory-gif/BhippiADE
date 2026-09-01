//! HUD edits (ENG-135/136/137).
//!
//! One vocabulary, used by both the Details panel and the AI, so "change the button's text"
//! is the same operation whoever asks for it. Every action is a whole-document transform
//! that validates before it returns, which is what lets the caller keep an undo stack of
//! snapshots: a HUD is a few kilobytes, so snapshotting is cheaper and far simpler than an
//! op/inverse pair, and it cannot drift out of sync with the document the way a hand-written
//! inverse can.

use crate::error::{EngineError, Result};
use crate::hud::{
    kind_from_str, Anchor, Canvas, HudDocument, ScaleMode, Widget, WidgetAction, WidgetId,
    WidgetKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

/// The editable surface of a HUD document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HudAction {
    /// Add a widget of `widget` kind, optionally inside a container.
    AddWidget {
        widget: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        parent: Option<WidgetId>,
    },
    RemoveWidget {
        id: WidgetId,
    },
    RenameWidget {
        id: WidgetId,
        name: String,
    },
    /// Set one kind-specific property — this is how a button's text changes.
    SetProp {
        id: WidgetId,
        prop: String,
        value: Value,
    },
    /// Merge style fields; unset ones keep their value.
    SetStyle {
        id: WidgetId,
        style: Value,
    },
    /// Merge rect fields; unset ones keep their value.
    SetRect {
        id: WidgetId,
        #[serde(default)]
        anchor: Option<String>,
        #[serde(default)]
        offset: Option<[f32; 2]>,
        #[serde(default)]
        size: Option<[f32; 2]>,
        #[serde(default)]
        pivot: Option<[f32; 2]>,
    },
    /// Point a binding slot at a runtime value, or clear it with an empty path.
    SetBind {
        id: WidgetId,
        slot: String,
        path: String,
    },
    /// Set (or clear) what a button does.
    SetAction {
        id: WidgetId,
        #[serde(default)]
        on_click: Option<WidgetAction>,
    },
    ReparentWidget {
        id: WidgetId,
        #[serde(default)]
        parent: Option<WidgetId>,
    },
    /// Move a widget within its siblings' draw order.
    ReorderWidget {
        id: WidgetId,
        order: i32,
    },
    SetVisible {
        id: WidgetId,
        visible: bool,
    },
    SetLocked {
        id: WidgetId,
        locked: bool,
    },
    SetCanvas {
        #[serde(default)]
        reference: Option<[f32; 2]>,
        #[serde(default)]
        scale_mode: Option<String>,
        #[serde(default)]
        safe_area: Option<f32>,
    },
}

impl HudAction {
    /// The one-line label for the undo affordance and the Activity Dock.
    #[must_use]
    pub fn to_label(&self) -> String {
        match self {
            Self::AddWidget { widget, .. } => format!("add {widget}"),
            Self::RemoveWidget { .. } => "remove widget".to_owned(),
            Self::RenameWidget { .. } => "rename widget".to_owned(),
            Self::SetProp { prop, .. } => format!("set {prop}"),
            Self::SetStyle { .. } => "restyle widget".to_owned(),
            Self::SetRect { .. } => "move/resize widget".to_owned(),
            Self::SetBind { slot, .. } => format!("bind {slot}"),
            Self::SetAction { .. } => "set click action".to_owned(),
            Self::ReparentWidget { .. } => "reparent widget".to_owned(),
            Self::ReorderWidget { .. } => "reorder widget".to_owned(),
            Self::SetVisible { visible, .. } => if *visible {
                "show widget"
            } else {
                "hide widget"
            }
            .to_owned(),
            Self::SetLocked { locked, .. } => if *locked {
                "lock widget"
            } else {
                "unlock widget"
            }
            .to_owned(),
            Self::SetCanvas { .. } => "edit canvas".to_owned(),
        }
    }

    /// Apply to `doc`, leaving it valid or leaving it untouched.
    ///
    /// The document is edited on a clone and only swapped in once it validates, so a
    /// rejected edit cannot leave a half-applied HUD behind.
    pub fn apply(&self, doc: &mut HudDocument) -> Result<WidgetId> {
        let mut next = doc.clone();
        let touched = self.apply_to(&mut next)?;
        next.validate()?;
        *doc = next;
        Ok(touched)
    }

    fn apply_to(&self, doc: &mut HudDocument) -> Result<WidgetId> {
        match self {
            Self::AddWidget {
                widget,
                name,
                parent,
            } => {
                let kind = kind_from_str(widget).ok_or_else(|| {
                    EngineError::Asset(
                        format!("unknown widget kind {widget:?}"),
                        Some(format!(
                            "Valid kinds: {}",
                            WidgetKind::all()
                                .into_iter()
                                .map(WidgetKind::as_str)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                    )
                })?;
                if let Some(parent) = parent {
                    if doc.widget(parent).is_none() {
                        return Err(missing(parent));
                    }
                }
                let label = name.clone().unwrap_or_else(|| default_name(kind, doc));
                let mut created = Widget::new(kind, label);
                created.parent.clone_from(parent);
                // New widgets go on top of their siblings, which is what "add" means
                // visually — otherwise a new button can appear behind a panel.
                created.order = doc
                    .widgets
                    .iter()
                    .filter(|widget| widget.parent == created.parent)
                    .map(|widget| widget.order)
                    .max()
                    .map_or(0, |top| top.saturating_add(1));
                let id = created.id.clone();
                doc.widgets.push(created);
                Ok(id)
            }
            Self::RemoveWidget { id } => {
                if doc.widget(id).is_none() {
                    return Err(missing(id));
                }
                // Removing a container takes its subtree, so no child is left orphaned
                // pointing at a parent that no longer exists.
                let mut doomed = vec![id.clone()];
                let mut index = 0;
                while index < doomed.len() {
                    let current = doomed[index].clone();
                    for child in doc.children_of(&current) {
                        doomed.push(child.id.clone());
                    }
                    index += 1;
                }
                doc.widgets.retain(|widget| !doomed.contains(&widget.id));
                Ok(id.clone())
            }
            Self::RenameWidget { id, name } => {
                if name.trim().is_empty() {
                    return Err(EngineError::Asset(
                        "widget name must not be empty".to_owned(),
                        Some("Give it a name.".to_owned()),
                    ));
                }
                let widget = doc.widget_mut(id).ok_or_else(|| missing(id))?;
                widget.name.clone_from(name);
                Ok(id.clone())
            }
            Self::SetProp { id, prop, value } => {
                let widget = doc.widget_mut(id).ok_or_else(|| missing(id))?;
                if value.is_null() {
                    widget.props.remove(prop);
                } else {
                    widget.props.insert(prop.clone(), value.clone());
                }
                Ok(id.clone())
            }
            Self::SetStyle { id, style } => {
                let widget = doc.widget_mut(id).ok_or_else(|| missing(id))?;
                let mut current = serde_json::to_value(&widget.style).map_err(serialise)?;
                merge(&mut current, style);
                widget.style = serde_json::from_value(current).map_err(|error| {
                    EngineError::Asset(
                        format!("that style is not valid: {error}"),
                        Some(
                            "Fields: bg, fg, fill, border_color, border_width, radius, padding, \
                             opacity, font, font_size, align."
                                .to_owned(),
                        ),
                    )
                })?;
                Ok(id.clone())
            }
            Self::SetRect {
                id,
                anchor,
                offset,
                size,
                pivot,
            } => {
                let parsed = match anchor {
                    Some(name) => Some(anchor_from_str(name)?),
                    None => None,
                };
                let widget = doc.widget_mut(id).ok_or_else(|| missing(id))?;
                if let Some(anchor) = parsed {
                    widget.rect.anchor = anchor;
                }
                if let Some(offset) = offset {
                    widget.rect.offset = *offset;
                }
                if let Some(size) = size {
                    if size[0] <= 0.0 || size[1] <= 0.0 {
                        return Err(EngineError::Asset(
                            "widget size must be positive".to_owned(),
                            Some("A zero-sized widget cannot be seen or clicked.".to_owned()),
                        ));
                    }
                    widget.rect.size = *size;
                }
                if let Some(pivot) = pivot {
                    widget.rect.pivot = *pivot;
                }
                Ok(id.clone())
            }
            Self::SetBind { id, slot, path } => {
                let widget = doc.widget_mut(id).ok_or_else(|| missing(id))?;
                if path.trim().is_empty() {
                    widget.bind.remove(slot);
                } else {
                    widget.bind.insert(slot.clone(), path.trim().to_owned());
                }
                Ok(id.clone())
            }
            Self::SetAction { id, on_click } => {
                let widget = doc.widget_mut(id).ok_or_else(|| missing(id))?;
                widget.on_click.clone_from(on_click);
                Ok(id.clone())
            }
            Self::ReparentWidget { id, parent } => {
                let world = doc
                    .widget(id)
                    .map(|widget| resolve_rect(doc, widget))
                    .ok_or_else(|| missing(id))?;
                if doc.widget(id).is_some_and(|widget| widget.locked) {
                    return Err(EngineError::Asset(
                        "a locked widget cannot be reparented".to_owned(),
                        Some("Unlock it first.".to_owned()),
                    ));
                }
                if let Some(parent) = parent {
                    if parent == id {
                        return Err(EngineError::Asset(
                            "a widget cannot be its own parent".to_owned(),
                            Some("Pick a different container.".to_owned()),
                        ));
                    }
                    if doc.widget(parent).is_none() {
                        return Err(missing(parent));
                    }
                    if !doc
                        .widget(parent)
                        .is_some_and(|widget| widget.kind.is_container())
                    {
                        return Err(EngineError::Asset(
                            "that widget cannot contain children".to_owned(),
                            Some("Drop onto a panel or another container.".to_owned()),
                        ));
                    }
                }
                let parent_rect = parent
                    .as_deref()
                    .and_then(|parent| doc.widget(parent))
                    .map(|widget| resolve_rect(doc, widget))
                    .unwrap_or_else(|| canvas_rect(doc));
                let widget = doc.widget_mut(id).ok_or_else(|| missing(id))?;
                widget.parent.clone_from(parent);
                // A tree operation must not make the widget jump on the canvas. Express the
                // same world rectangle in the new parent's top-left coordinate space.
                widget.rect.anchor = Anchor::TopLeft;
                widget.rect.pivot = [0.0, 0.0];
                widget.rect.offset = [world[0] - parent_rect[0], world[1] - parent_rect[1]];
                widget.rect.size = [world[2], world[3]];
                Ok(id.clone())
            }
            Self::ReorderWidget { id, order } => {
                let widget = doc.widget_mut(id).ok_or_else(|| missing(id))?;
                if widget.locked {
                    return Err(EngineError::Asset(
                        "a locked widget cannot be reordered".to_owned(),
                        Some("Unlock it first.".to_owned()),
                    ));
                }
                widget.order = *order;
                Ok(id.clone())
            }
            Self::SetVisible { id, visible } => {
                let widget = doc.widget_mut(id).ok_or_else(|| missing(id))?;
                widget.visible = *visible;
                Ok(id.clone())
            }
            Self::SetLocked { id, locked } => {
                let widget = doc.widget_mut(id).ok_or_else(|| missing(id))?;
                widget.locked = *locked;
                Ok(id.clone())
            }
            Self::SetCanvas {
                reference,
                scale_mode,
                safe_area,
            } => {
                let mut canvas = Canvas {
                    reference: reference.unwrap_or(doc.canvas.reference),
                    scale_mode: doc.canvas.scale_mode,
                    safe_area: safe_area.unwrap_or(doc.canvas.safe_area),
                };
                if let Some(mode) = scale_mode {
                    canvas.scale_mode = match mode.as_str() {
                        "fit" => ScaleMode::Fit,
                        "fill" => ScaleMode::Fill,
                        "pixel" => ScaleMode::Pixel,
                        other => {
                            return Err(EngineError::Asset(
                                format!("unknown scale mode {other:?}"),
                                Some("Use fit, fill or pixel.".to_owned()),
                            ))
                        }
                    };
                }
                doc.canvas = canvas;
                Ok(String::new())
            }
        }
    }
}

/// "Button 2" when a "Button" already exists — the same numbering the Outliner shows.
fn default_name(kind: WidgetKind, doc: &HudDocument) -> String {
    let base = match kind {
        WidgetKind::ProgressBar => "Bar".to_owned(),
        WidgetKind::IconRow => "Icons".to_owned(),
        WidgetKind::KeyPrompt => "Key".to_owned(),
        other => {
            let name = other.as_str();
            let mut chars = name.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => "Widget".to_owned(),
            }
        }
    };
    if !doc.widgets.iter().any(|widget| widget.name == base) {
        return base;
    }
    (2..)
        .map(|index| format!("{base} {index}"))
        .find(|candidate| !doc.widgets.iter().any(|widget| &widget.name == candidate))
        .unwrap_or(base)
}

fn anchor_from_str(name: &str) -> Result<Anchor> {
    let anchor = match name {
        "top_left" => Anchor::TopLeft,
        "top_center" => Anchor::TopCenter,
        "top_right" => Anchor::TopRight,
        "center_left" => Anchor::CenterLeft,
        "center" => Anchor::Center,
        "center_right" => Anchor::CenterRight,
        "bottom_left" => Anchor::BottomLeft,
        "bottom_center" => Anchor::BottomCenter,
        "bottom_right" => Anchor::BottomRight,
        "stretch" => Anchor::Stretch,
        other => {
            return Err(EngineError::Asset(
                format!("unknown anchor {other:?}"),
                Some(
                    "Use top_left, top_center, top_right, center_left, center, center_right, \
                     bottom_left, bottom_center, bottom_right or stretch."
                        .to_owned(),
                ),
            ))
        }
    };
    Ok(anchor)
}

fn missing(id: &str) -> EngineError {
    EngineError::Asset(
        format!("no widget {id} in this HUD"),
        Some("Refresh the widget list and retry.".to_owned()),
    )
}

fn serialise(error: serde_json::Error) -> EngineError {
    EngineError::Asset(
        format!("cannot read the widget style: {error}"),
        Some("Report this as an engine bug.".to_owned()),
    )
}

fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(base.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

/// A rect resolved into reference-resolution pixels: `[x, y, width, height]`, with the
/// origin at the top-left of the canvas.
///
/// Anchor maths lives here rather than in the webview (INV-073): the canvas editor, the
/// runtime renderer and any future exporter must place a widget in exactly the same place,
/// and three implementations of this would be three chances to disagree.
#[must_use]
pub fn resolve_rect(doc: &HudDocument, widget: &Widget) -> [f32; 4] {
    let parent = widget
        .parent
        .as_deref()
        .and_then(|id| doc.widget(id))
        .map(|found| resolve_rect(doc, found))
        .unwrap_or_else(|| canvas_rect(doc));

    if widget.rect.anchor == Anchor::Stretch {
        // Stretch reads `offset` as an inset and ignores `size`: the widget is the parent
        // minus a margin, which is what "fill this panel" means.
        return [
            parent[0] + widget.rect.offset[0],
            parent[1] + widget.rect.offset[1],
            (parent[2] - widget.rect.offset[0] * 2.0).max(0.0),
            (parent[3] - widget.rect.offset[1] * 2.0).max(0.0),
        ];
    }

    let fraction = widget.rect.anchor.fraction();
    let anchor_x = fraction[0].mul_add(parent[2], parent[0]);
    let anchor_y = fraction[1].mul_add(parent[3], parent[1]);
    [
        anchor_x + widget.rect.offset[0] - widget.rect.pivot[0] * widget.rect.size[0],
        anchor_y + widget.rect.offset[1] - widget.rect.pivot[1] * widget.rect.size[1],
        widget.rect.size[0],
        widget.rect.size[1],
    ]
}

fn canvas_rect(doc: &HudDocument) -> [f32; 4] {
    let inset = doc.canvas.safe_area;
    [
        doc.canvas.reference[0] * inset,
        doc.canvas.reference[1] * inset,
        doc.canvas.reference[0] * (1.0 - inset * 2.0),
        doc.canvas.reference[1] * (1.0 - inset * 2.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::{resolve_rect, HudAction};
    use crate::hud::{Anchor, HudDocument, WidgetAction, WidgetKind};

    fn starter() -> HudDocument {
        HudDocument::starter()
    }

    fn id_of(doc: &HudDocument, name: &str) -> String {
        doc.widgets
            .iter()
            .find(|widget| widget.name == name)
            .unwrap_or_else(|| panic!("no widget named {name}"))
            .id
            .clone()
    }

    /// The owner's headline case: change what the AI generated, by hand.
    #[test]
    fn a_buttons_text_and_position_can_be_changed() {
        let mut doc = starter();
        let pause = id_of(&doc, "PauseButton");

        HudAction::SetProp {
            id: pause.clone(),
            prop: "text".to_owned(),
            value: serde_json::json!("MENU"),
        }
        .apply(&mut doc)
        .expect("applies");
        HudAction::SetRect {
            id: pause.clone(),
            anchor: Some("top_left".to_owned()),
            offset: Some([16.0, 16.0]),
            size: None,
            pivot: Some([0.0, 0.0]),
        }
        .apply(&mut doc)
        .expect("applies");

        let widget = doc.widget(&pause).expect("still there");
        assert_eq!(widget.props["text"], "MENU");
        assert_eq!(widget.rect.anchor, Anchor::TopLeft);
        assert_eq!(widget.rect.offset, [16.0, 16.0]);
        // Its size was not named, so it keeps what it had.
        assert_eq!(widget.rect.size, [96.0, 34.0]);
    }

    #[test]
    fn restyling_merges_rather_than_replacing() {
        let mut doc = starter();
        let health = id_of(&doc, "HealthBar");
        HudAction::SetStyle {
            id: health.clone(),
            style: serde_json::json!({ "fill": "#22cc55" }),
        }
        .apply(&mut doc)
        .expect("applies");
        let widget = doc.widget(&health).expect("there");
        assert_eq!(widget.style.fill.as_deref(), Some("#22cc55"));
        assert_eq!(
            widget.style.bg.as_deref(),
            Some("#00000080"),
            "an unnamed style field is untouched"
        );
    }

    #[test]
    fn a_rejected_edit_leaves_the_document_exactly_as_it_was() {
        let mut doc = starter();
        let before = doc.clone();
        let health = id_of(&doc, "HealthBar");
        let error = HudAction::SetProp {
            id: health,
            prop: "thickness".to_owned(),
            value: serde_json::json!(3.0),
        }
        .apply(&mut doc)
        .expect_err("progress bars have no thickness");
        assert!(error.hint().is_some());
        assert_eq!(doc, before, "nothing was half-applied");
    }

    #[test]
    fn adding_a_widget_stacks_it_on_top_and_names_it_uniquely() {
        let mut doc = starter();
        let first = HudAction::AddWidget {
            widget: "button".to_owned(),
            name: None,
            parent: None,
        }
        .apply(&mut doc)
        .expect("applies");
        let second = HudAction::AddWidget {
            widget: "button".to_owned(),
            name: None,
            parent: None,
        }
        .apply(&mut doc)
        .expect("applies");

        assert_eq!(doc.widget(&first).expect("a").name, "Button");
        assert_eq!(doc.widget(&second).expect("b").name, "Button 2");
        assert!(
            doc.widget(&second).expect("b").order > doc.widget(&first).expect("a").order,
            "a new widget lands on top of its siblings"
        );
    }

    #[test]
    fn removing_a_container_takes_its_children_with_it() {
        let mut doc = HudDocument::empty("hud");
        let panel = HudAction::AddWidget {
            widget: "panel".to_owned(),
            name: Some("Root".to_owned()),
            parent: None,
        }
        .apply(&mut doc)
        .expect("panel");
        HudAction::AddWidget {
            widget: "text".to_owned(),
            name: Some("Child".to_owned()),
            parent: Some(panel.clone()),
        }
        .apply(&mut doc)
        .expect("child");
        assert_eq!(doc.widgets.len(), 2);

        HudAction::RemoveWidget { id: panel }
            .apply(&mut doc)
            .expect("removes");
        assert!(
            doc.widgets.is_empty(),
            "no orphan left pointing at a parent that is gone"
        );
    }

    #[test]
    fn a_click_action_can_be_set_and_cleared() {
        let mut doc = starter();
        let pause = id_of(&doc, "PauseButton");
        HudAction::SetAction {
            id: pause.clone(),
            on_click: Some(WidgetAction::LoadLevel {
                level: "assets/scenes/level_02.bscn.json".to_owned(),
            }),
        }
        .apply(&mut doc)
        .expect("applies");
        assert!(matches!(
            doc.widget(&pause).expect("there").on_click,
            Some(WidgetAction::LoadLevel { .. })
        ));

        HudAction::SetAction {
            id: pause.clone(),
            on_click: None,
        }
        .apply(&mut doc)
        .expect("applies");
        assert!(doc.widget(&pause).expect("there").on_click.is_none());
    }

    #[test]
    fn bindings_are_set_and_cleared_by_path() {
        let mut doc = starter();
        let health = id_of(&doc, "HealthBar");
        HudAction::SetBind {
            id: health.clone(),
            slot: "value".to_owned(),
            path: "player.shield".to_owned(),
        }
        .apply(&mut doc)
        .expect("applies");
        assert_eq!(
            doc.widget(&health).expect("there").bind["value"],
            "player.shield"
        );

        HudAction::SetBind {
            id: health.clone(),
            slot: "value".to_owned(),
            path: "  ".to_owned(),
        }
        .apply(&mut doc)
        .expect("applies");
        assert!(!doc
            .widget(&health)
            .expect("there")
            .bind
            .contains_key("value"));
    }

    #[test]
    fn a_widget_cannot_be_reparented_into_itself_or_a_non_container() {
        let mut doc = starter();
        let pause = id_of(&doc, "PauseButton");
        let score = id_of(&doc, "ScoreLabel");
        assert!(HudAction::ReparentWidget {
            id: pause.clone(),
            parent: Some(pause.clone()),
        }
        .apply(&mut doc)
        .is_err());
        assert!(HudAction::ReparentWidget {
            id: pause,
            parent: Some(score),
        }
        .apply(&mut doc)
        .is_err());
    }

    #[test]
    fn reparent_preserves_world_rect_and_locked_widgets_refuse_tree_edits() {
        let mut doc = starter();
        let pause = id_of(&doc, "PauseButton");
        let panel = HudAction::AddWidget {
            widget: "panel".to_owned(),
            name: Some("InventoryPanel".to_owned()),
            parent: None,
        }
        .apply(&mut doc)
        .expect("panel");
        HudAction::SetRect {
            id: panel.clone(),
            anchor: Some("top_left".to_owned()),
            offset: Some([300.0, 200.0]),
            size: Some([500.0, 400.0]),
            pivot: Some([0.0, 0.0]),
        }
        .apply(&mut doc)
        .expect("place panel");
        let before = resolve_rect(&doc, doc.widget(&pause).expect("pause"));
        HudAction::ReparentWidget {
            id: pause.clone(),
            parent: Some(panel),
        }
        .apply(&mut doc)
        .expect("reparent");
        let after = resolve_rect(&doc, doc.widget(&pause).expect("pause"));
        assert_eq!(before, after, "tree edits must not move canvas pixels");

        HudAction::SetLocked {
            id: pause.clone(),
            locked: true,
        }
        .apply(&mut doc)
        .expect("lock");
        assert!(HudAction::ReparentWidget {
            id: pause.clone(),
            parent: None,
        }
        .apply(&mut doc)
        .is_err());
        assert!(HudAction::ReorderWidget {
            id: pause,
            order: 99,
        }
        .apply(&mut doc)
        .is_err());
    }

    #[test]
    fn an_unknown_widget_kind_or_anchor_offers_the_real_list() {
        let mut doc = starter();
        let error = HudAction::AddWidget {
            widget: "hologram".to_owned(),
            name: None,
            parent: None,
        }
        .apply(&mut doc)
        .expect_err("unknown kind");
        assert!(error
            .hint()
            .is_some_and(|hint| hint.contains("progress_bar")));

        let health = id_of(&doc, "HealthBar");
        let error = HudAction::SetRect {
            id: health,
            anchor: Some("north".to_owned()),
            offset: None,
            size: None,
            pivot: None,
        }
        .apply(&mut doc)
        .expect_err("unknown anchor");
        assert!(error
            .hint()
            .is_some_and(|hint| hint.contains("bottom_right")));
    }

    #[test]
    fn a_zero_sized_widget_is_refused() {
        let mut doc = starter();
        let health = id_of(&doc, "HealthBar");
        let error = HudAction::SetRect {
            id: health,
            anchor: None,
            offset: None,
            size: Some([0.0, 20.0]),
            pivot: None,
        }
        .apply(&mut doc)
        .expect_err("zero width");
        assert!(error.hint().is_some());
    }

    /// The anchor maths the canvas editor and the runtime both depend on.
    #[test]
    fn rects_resolve_against_their_anchor_and_pivot() {
        let doc = starter();
        let health = doc
            .widgets
            .iter()
            .find(|widget| widget.name == "HealthBar")
            .expect("bar");
        // Top-left anchor, zero pivot: the offset is the position.
        assert_eq!(resolve_rect(&doc, health), [32.0, 32.0, 260.0, 22.0]);

        let pause = doc
            .widgets
            .iter()
            .find(|widget| widget.name == "PauseButton")
            .expect("button");
        // Top-right anchor with a right-edge pivot: 32px in from the right edge.
        let rect = resolve_rect(&doc, pause);
        assert!(
            (rect[0] - (1920.0 - 32.0 - 96.0)).abs() < 1e-3,
            "got {rect:?}"
        );
        assert_eq!(rect[1], 32.0);
    }

    #[test]
    fn a_child_resolves_inside_its_panel_not_the_screen() {
        let mut doc = HudDocument::empty("hud");
        let panel = HudAction::AddWidget {
            widget: "panel".to_owned(),
            name: Some("Box".to_owned()),
            parent: None,
        }
        .apply(&mut doc)
        .expect("panel");
        HudAction::SetRect {
            id: panel.clone(),
            anchor: Some("top_left".to_owned()),
            offset: Some([100.0, 50.0]),
            size: Some([400.0, 300.0]),
            pivot: None,
        }
        .apply(&mut doc)
        .expect("rect");

        let child = HudAction::AddWidget {
            widget: "text".to_owned(),
            name: Some("Inner".to_owned()),
            parent: Some(panel),
        }
        .apply(&mut doc)
        .expect("child");
        HudAction::SetRect {
            id: child.clone(),
            anchor: Some("center".to_owned()),
            offset: Some([0.0, 0.0]),
            size: Some([100.0, 20.0]),
            pivot: Some([0.5, 0.5]),
        }
        .apply(&mut doc)
        .expect("rect");

        let widget = doc.widget(&child).expect("there");
        let rect = resolve_rect(&doc, widget);
        // Centre of the panel (100+200, 50+150) minus half the widget.
        assert!((rect[0] - 250.0).abs() < 1e-3, "got {rect:?}");
        assert!((rect[1] - 190.0).abs() < 1e-3, "got {rect:?}");
    }

    #[test]
    fn stretch_reads_offset_as_an_inset() {
        let mut doc = HudDocument::empty("hud");
        let panel = HudAction::AddWidget {
            widget: "panel".to_owned(),
            name: Some("Full".to_owned()),
            parent: None,
        }
        .apply(&mut doc)
        .expect("panel");
        HudAction::SetRect {
            id: panel.clone(),
            anchor: Some("stretch".to_owned()),
            offset: Some([40.0, 20.0]),
            size: None,
            pivot: None,
        }
        .apply(&mut doc)
        .expect("rect");
        let rect = resolve_rect(&doc, doc.widget(&panel).expect("there"));
        assert_eq!(rect, [40.0, 20.0, 1920.0 - 80.0, 1080.0 - 40.0]);
    }

    #[test]
    fn the_canvas_safe_area_insets_every_root() {
        let mut doc = starter();
        HudAction::SetCanvas {
            reference: None,
            scale_mode: None,
            safe_area: Some(0.05),
        }
        .apply(&mut doc)
        .expect("applies");
        let health = doc
            .widgets
            .iter()
            .find(|widget| widget.name == "HealthBar")
            .expect("bar");
        let rect = resolve_rect(&doc, health);
        assert!(
            (rect[0] - (1920.0 * 0.05 + 32.0)).abs() < 1e-3,
            "got {rect:?}"
        );
    }

    #[test]
    fn every_action_has_a_readable_label() {
        assert_eq!(
            HudAction::AddWidget {
                widget: "button".to_owned(),
                name: None,
                parent: None
            }
            .to_label(),
            "add button"
        );
        assert_eq!(
            HudAction::SetProp {
                id: "x".to_owned(),
                prop: "text".to_owned(),
                value: serde_json::Value::Null
            }
            .to_label(),
            "set text"
        );
        assert_eq!(WidgetKind::Button.as_str(), "button");
    }
}
