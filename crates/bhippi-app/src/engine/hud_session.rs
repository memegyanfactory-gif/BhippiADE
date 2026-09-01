//! Open HUD documents (ENG-134…137).
//!
//! The same shape as `EngineSessions`, for the other document family: one open document per
//! HUD file, one undo stack, one dirty flag. Both the Details panel and the AI dispatch
//! [`HudAction`]s through here, so there is a single write path for HUDs exactly as there is
//! for scenes.
//!
//! Undo is a snapshot stack rather than ops-with-inverses. A HUD is a few kilobytes and
//! `HudAction::apply` already produces a whole validated document, so snapshots are both
//! cheaper to implement and impossible to desynchronise from the document — where a
//! hand-written inverse for thirteen action kinds would be thirteen chances to get it wrong.

use crate::commands::AppError;
use bhippi_engine::hud::{HudDocument, Widget, WidgetKind};
use bhippi_engine::hud_action::{resolve_rect, HudAction};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::path::Path;

/// How many edits back a HUD can be undone. Small documents, so this is generous.
const HUD_UNDO_CAP: usize = 200;

/// One widget as the editor renders it: identity, tree position, and the rectangle already
/// resolved into canvas pixels so the webview does no anchor maths (INV-073).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudWidgetView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub parent: Option<String>,
    pub order: i32,
    pub visible: bool,
    pub locked: bool,
    pub is_container: bool,
    /// `[x, y, width, height]` in reference-resolution pixels, origin top-left.
    pub rect: [f32; 4],
    /// Depth in the widget tree, so the Outliner can indent without walking it.
    pub depth: u32,
}

/// The state the HUD editor renders.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudState {
    pub path: String,
    pub name: String,
    pub reference: [f32; 2],
    pub scale_mode: String,
    pub safe_area: f32,
    pub dirty: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub revision: u32,
    pub selection: Option<String>,
    /// Widgets in draw order, parents before children.
    pub widgets: Vec<HudWidgetView>,
    /// The canonical `bhippi-hud@1` document, for the Details panel to read fields from.
    pub document_json: String,
}

struct OpenHud {
    path: String,
    doc: HudDocument,
    undo: Vec<HudDocument>,
    redo: Vec<HudDocument>,
    labels: Vec<String>,
    dirty: bool,
    revision: u32,
    selection: Option<String>,
}

impl OpenHud {
    fn load(game_dir: &Path, rel: &str) -> Result<Self, AppError> {
        let path = game_dir.join(rel);
        let doc = match std::fs::read_to_string(&path) {
            Ok(text) => HudDocument::parse(&text).map_err(super::engine_error)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                migrate_legacy_hud(game_dir, rel)?.unwrap_or_else(HudDocument::starter)
            }
            Err(error) => {
                return Err(AppError {
                    message: format!("Could not read {rel}: {error}"),
                    hint: Some("Check that the HUD file is readable.".to_owned()),
                })
            }
        };
        Ok(Self {
            path: rel.to_owned(),
            doc,
            undo: Vec::new(),
            redo: Vec::new(),
            labels: Vec::new(),
            dirty: false,
            revision: 0,
            selection: None,
        })
    }

    fn state(&self) -> HudState {
        HudState {
            path: self.path.clone(),
            name: self.doc.name.clone(),
            reference: self.doc.canvas.reference,
            scale_mode: match self.doc.canvas.scale_mode {
                bhippi_engine::hud::ScaleMode::Fit => "fit",
                bhippi_engine::hud::ScaleMode::Fill => "fill",
                bhippi_engine::hud::ScaleMode::Pixel => "pixel",
            }
            .to_owned(),
            safe_area: self.doc.canvas.safe_area,
            dirty: self.dirty,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            undo_label: self.labels.last().cloned(),
            revision: self.revision,
            selection: self.selection.clone(),
            widgets: self.widget_views(),
            document_json: self.doc.dump().unwrap_or_default(),
        }
    }

    /// Flatten the widget tree depth-first in draw order, resolving each rect.
    fn widget_views(&self) -> Vec<HudWidgetView> {
        let mut out = Vec::with_capacity(self.doc.widgets.len());
        let mut stack: Vec<(&Widget, u32)> = self
            .doc
            .roots()
            .into_iter()
            .rev()
            .map(|widget| (widget, 0))
            .collect();
        while let Some((widget, depth)) = stack.pop() {
            out.push(HudWidgetView {
                id: widget.id.clone(),
                name: widget.name.clone(),
                kind: widget.kind.as_str().to_owned(),
                parent: widget.parent.clone(),
                order: widget.order,
                visible: widget.visible,
                locked: widget.locked,
                is_container: widget.kind.is_container(),
                rect: resolve_rect(&self.doc, widget),
                depth,
            });
            for child in self.doc.children_of(&widget.id).into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }
        out
    }

    fn apply(&mut self, action: &HudAction) -> Result<String, AppError> {
        let before = self.doc.clone();
        let touched = action.apply(&mut self.doc).map_err(super::engine_error)?;
        self.undo.push(before);
        self.labels.push(action.to_label());
        if self.undo.len() > HUD_UNDO_CAP {
            self.undo.remove(0);
            self.labels.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
        if !touched.is_empty() {
            self.selection = Some(touched.clone());
        }
        Ok(touched)
    }

    fn write(&mut self, game_dir: &Path) -> Result<(), AppError> {
        let path = game_dir.join(&self.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| AppError {
                message: format!("Could not create the HUD folder: {error}"),
                hint: Some("Check the project is writable.".to_owned()),
            })?;
        }
        let text = self.doc.dump().map_err(super::engine_error)?;
        std::fs::write(&path, text).map_err(|error| AppError {
            message: format!("Could not save {}: {error}", self.path),
            hint: Some("Check the file is writable.".to_owned()),
        })?;
        self.dirty = false;
        Ok(())
    }
}

/// Write the deterministic `hud.bscn.json` → `hud_main.hud.json` migration once. The old
/// scene stays in place as a recoverable source; the manifest is repointed to the new HUD.
fn migrate_legacy_hud(game_dir: &Path, rel: &str) -> Result<Option<HudDocument>, AppError> {
    if rel != super::DEFAULT_HUD_PATH {
        return Ok(None);
    }
    let manifest = bhippi_engine::manifest::load_manifest(game_dir).map_err(super::engine_error)?;
    let manifest_legacy = manifest
        .as_ref()
        .and_then(|value| value.game.hud_scene.as_deref())
        .filter(|path| path.ends_with(".bscn.json"));
    let conventional = "assets/scenes/hud.bscn.json";
    let legacy_rel = manifest_legacy
        .filter(|path| game_dir.join(path).is_file())
        .or_else(|| {
            game_dir
                .join(conventional)
                .is_file()
                .then_some(conventional)
        });
    let Some(legacy_rel) = legacy_rel else {
        return Ok(None);
    };
    let legacy_text =
        std::fs::read_to_string(game_dir.join(legacy_rel)).map_err(|error| AppError {
            message: format!("Could not read the legacy HUD {legacy_rel}: {error}"),
            hint: Some("The legacy file was left untouched.".to_owned()),
        })?;
    let legacy = bhippi_engine::document::SceneDocument::parse_lenient(&legacy_text)
        .map_err(super::engine_error)?;
    let upgraded =
        bhippi_engine::hud::upgrade_legacy_scene(&legacy).map_err(super::engine_error)?;
    let target = game_dir.join(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| AppError {
            message: format!("Could not create the HUD folder: {error}"),
            hint: Some("The legacy file was left untouched.".to_owned()),
        })?;
    }
    std::fs::write(&target, upgraded.dump().map_err(super::engine_error)?).map_err(|error| {
        AppError {
            message: format!("Could not write the upgraded HUD: {error}"),
            hint: Some("The legacy file was left untouched.".to_owned()),
        }
    })?;
    if let Some(mut manifest) = manifest {
        manifest.game.hud_scene = Some(rel.to_owned());
        std::fs::write(
            bhippi_engine::manifest::manifest_path(game_dir),
            bhippi_engine::scaffold::format_manifest(&manifest),
        )
        .map_err(|error| AppError {
            message: format!("The HUD upgraded, but the manifest could not be updated: {error}"),
            hint: Some(format!("Set game.hud_scene to {rel}.")),
        })?;
    }
    Ok(Some(upgraded))
}

/// Every HUD the editor currently holds open.
#[derive(Default)]
pub struct HudSessions {
    open: BTreeMap<(String, String), OpenHud>,
}

impl HudSessions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn key(game_dir: &Path, rel: &str) -> (String, String) {
        (
            game_dir.to_string_lossy().replace('\\', "/"),
            rel.replace('\\', "/"),
        )
    }

    fn entry(&mut self, game_dir: &Path, rel: &str) -> Result<&mut OpenHud, AppError> {
        let key = Self::key(game_dir, rel);
        if !self.open.contains_key(&key) {
            self.open.insert(key.clone(), OpenHud::load(game_dir, rel)?);
        }
        self.open.get_mut(&key).ok_or_else(|| AppError {
            message: format!("{rel} is not open."),
            hint: Some("Open the HUD first.".to_owned()),
        })
    }

    pub fn open(&mut self, game_dir: &Path, rel: &str) -> Result<HudState, AppError> {
        Ok(self.entry(game_dir, rel)?.state())
    }

    /// Re-read from disk, discarding unsaved edits.
    pub fn reload(&mut self, game_dir: &Path, rel: &str) -> Result<HudState, AppError> {
        self.open.remove(&Self::key(game_dir, rel));
        self.open(game_dir, rel)
    }

    pub fn apply(
        &mut self,
        game_dir: &Path,
        rel: &str,
        action: &HudAction,
    ) -> Result<HudState, AppError> {
        let hud = self.entry(game_dir, rel)?;
        hud.apply(action)?;
        Ok(hud.state())
    }

    /// Apply several edits as one undo step — the Details panel commits a whole form this way.
    pub fn apply_many(
        &mut self,
        game_dir: &Path,
        rel: &str,
        actions: &[HudAction],
        label: &str,
    ) -> Result<HudState, AppError> {
        if actions.is_empty() {
            return Err(AppError {
                message: "No HUD actions were given.".to_owned(),
                hint: Some("Pass at least one edit.".to_owned()),
            });
        }
        let hud = self.entry(game_dir, rel)?;
        let before = hud.doc.clone();
        // All-or-nothing, like a scene batch: a half-applied form is not a state anyone
        // asked for.
        for action in actions {
            if let Err(error) = action.apply(&mut hud.doc) {
                hud.doc = before;
                return Err(super::engine_error(error));
            }
        }
        hud.undo.push(before);
        hud.labels.push(label.to_owned());
        if hud.undo.len() > HUD_UNDO_CAP {
            hud.undo.remove(0);
            hud.labels.remove(0);
        }
        hud.redo.clear();
        hud.dirty = true;
        hud.revision = hud.revision.wrapping_add(1);
        Ok(hud.state())
    }

    pub fn undo(&mut self, game_dir: &Path, rel: &str) -> Result<HudState, AppError> {
        let hud = self.entry(game_dir, rel)?;
        let Some(previous) = hud.undo.pop() else {
            return Ok(hud.state());
        };
        hud.labels.pop();
        hud.redo.push(std::mem::replace(&mut hud.doc, previous));
        hud.dirty = true;
        hud.revision = hud.revision.wrapping_add(1);
        Ok(hud.state())
    }

    pub fn redo(&mut self, game_dir: &Path, rel: &str) -> Result<HudState, AppError> {
        let hud = self.entry(game_dir, rel)?;
        let Some(next) = hud.redo.pop() else {
            return Ok(hud.state());
        };
        hud.undo.push(std::mem::replace(&mut hud.doc, next));
        hud.labels.push("redo".to_owned());
        hud.dirty = true;
        hud.revision = hud.revision.wrapping_add(1);
        Ok(hud.state())
    }

    pub fn save(&mut self, game_dir: &Path, rel: &str) -> Result<HudState, AppError> {
        let hud = self.entry(game_dir, rel)?;
        hud.write(game_dir)?;
        Ok(hud.state())
    }

    pub fn select(
        &mut self,
        game_dir: &Path,
        rel: &str,
        widget: Option<String>,
    ) -> Result<HudState, AppError> {
        let hud = self.entry(game_dir, rel)?;
        hud.selection = widget.filter(|id| hud.doc.widget(id).is_some());
        Ok(hud.state())
    }

    /// The live document, for the play composer and the gates.
    #[must_use]
    pub fn document(&self, game_dir: &Path, rel: &str) -> Option<&HudDocument> {
        self.open.get(&Self::key(game_dir, rel)).map(|hud| &hud.doc)
    }
}

/// One widget kind offered by the Add menu, with the fields the Details panel renders.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudWidgetKindView {
    pub kind: String,
    pub label: String,
    pub is_container: bool,
    pub props: Vec<HudPropView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HudPropView {
    pub name: String,
    /// `text` · `number` · `bool` · `asset` · `enum`
    pub kind: String,
    /// Present for `enum`.
    pub options: Vec<String>,
    pub doc: String,
}

/// The widget catalog, straight from the engine registry — the Details panel and the Add
/// menu render from this rather than keeping their own copy of the field list.
#[must_use]
pub fn widget_catalog() -> Vec<HudWidgetKindView> {
    WidgetKind::all()
        .into_iter()
        .map(|kind| HudWidgetKindView {
            kind: kind.as_str().to_owned(),
            label: pretty(kind.as_str()),
            is_container: kind.is_container(),
            props: bhippi_engine::hud::widget_schema(kind)
                .iter()
                .map(|prop| {
                    let (kind_name, options) = match prop.kind {
                        bhippi_engine::hud::PropKind::Text => ("text", Vec::new()),
                        bhippi_engine::hud::PropKind::Number => ("number", Vec::new()),
                        bhippi_engine::hud::PropKind::Bool => ("bool", Vec::new()),
                        bhippi_engine::hud::PropKind::Asset => ("asset", Vec::new()),
                        bhippi_engine::hud::PropKind::Enum(values) => (
                            "enum",
                            values.iter().map(|value| (*value).to_owned()).collect(),
                        ),
                    };
                    HudPropView {
                        name: prop.name.to_owned(),
                        kind: kind_name.to_owned(),
                        options,
                        doc: prop.doc.to_owned(),
                    }
                })
                .collect(),
        })
        .collect()
}

fn pretty(slug: &str) -> String {
    slug.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{widget_catalog, HudSessions};
    use bhippi_engine::hud_action::HudAction;
    use bhippi_types::AssetId;

    fn temp(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bhippi-hud-{label}-{}", AssetId::new()));
        std::fs::create_dir_all(dir.join("assets/ui")).expect("dir");
        dir
    }

    const HUD: &str = "assets/ui/hud_main.hud.json";

    #[test]
    fn opening_a_hud_that_does_not_exist_yet_gives_the_starter_layout() {
        let dir = temp("fresh");
        let mut sessions = HudSessions::new();
        let state = sessions.open(&dir, HUD).expect("open");
        assert_eq!(state.widgets.len(), 3);
        assert!(!state.dirty, "just opening does not dirty it");
        // The rects arrive already resolved, so the webview does no anchor maths.
        let pause = state
            .widgets
            .iter()
            .find(|widget| widget.name == "PauseButton")
            .expect("button");
        assert!(
            pause.rect[0] > 1700.0,
            "top-right anchored: {:?}",
            pause.rect
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_legacy_hud_scene_upgrades_once_and_byte_identically() {
        let dir = temp("legacy");
        let legacy_path = dir.join("assets/scenes/hud.bscn.json");
        std::fs::create_dir_all(legacy_path.parent().expect("parent")).expect("legacy dir");
        let legacy = bhippi_engine::scaffold::hud_scene()
            .dump()
            .expect("legacy scene");
        std::fs::write(&legacy_path, &legacy).expect("write legacy");

        let mut first_sessions = HudSessions::new();
        let first = first_sessions.open(&dir, HUD).expect("upgrade");
        assert!(first
            .widgets
            .iter()
            .any(|widget| widget.name == "HealthBar"));
        assert!(first
            .widgets
            .iter()
            .any(|widget| widget.name == "ScoreLabel"));
        let target = dir.join(HUD);
        let first_bytes = std::fs::read(&target).expect("upgraded file");
        assert!(
            legacy_path.is_file(),
            "the recoverable legacy source remains"
        );

        std::fs::remove_file(&target).expect("repeat conversion");
        let mut second_sessions = HudSessions::new();
        second_sessions.open(&dir, HUD).expect("upgrade again");
        let second_bytes = std::fs::read(&target).expect("second upgraded file");
        assert_eq!(
            first_bytes, second_bytes,
            "same source means byte-identical HUD"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn editing_then_undoing_returns_the_previous_text() {
        let dir = temp("undo");
        let mut sessions = HudSessions::new();
        let state = sessions.open(&dir, HUD).expect("open");
        let pause = state
            .widgets
            .iter()
            .find(|widget| widget.name == "PauseButton")
            .expect("button")
            .id
            .clone();

        let after = sessions
            .apply(
                &dir,
                HUD,
                &HudAction::SetProp {
                    id: pause.clone(),
                    prop: "text".to_owned(),
                    value: serde_json::json!("MENU"),
                },
            )
            .expect("applies");
        assert!(after.dirty);
        assert!(after.can_undo);
        assert_eq!(after.undo_label.as_deref(), Some("set text"));
        assert!(after.document_json.contains("MENU"));
        assert_eq!(after.selection.as_deref(), Some(pause.as_str()));

        let undone = sessions.undo(&dir, HUD).expect("undo");
        assert!(undone.document_json.contains("PAUSE"));
        assert!(undone.can_redo);

        let redone = sessions.redo(&dir, HUD).expect("redo");
        assert!(redone.document_json.contains("MENU"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_form_commit_is_one_undo_step_and_is_all_or_nothing() {
        let dir = temp("form");
        let mut sessions = HudSessions::new();
        let state = sessions.open(&dir, HUD).expect("open");
        let bar = state
            .widgets
            .iter()
            .find(|widget| widget.name == "HealthBar")
            .expect("bar")
            .id
            .clone();

        let after = sessions
            .apply_many(
                &dir,
                HUD,
                &[
                    HudAction::RenameWidget {
                        id: bar.clone(),
                        name: "Shield".to_owned(),
                    },
                    HudAction::SetStyle {
                        id: bar.clone(),
                        style: serde_json::json!({ "fill": "#3aa0ff" }),
                    },
                ],
                "edit health bar",
            )
            .expect("applies");
        assert_eq!(after.undo_label.as_deref(), Some("edit health bar"));
        assert!(after.document_json.contains("Shield"));

        // One undo takes the whole form back.
        let undone = sessions.undo(&dir, HUD).expect("undo");
        assert!(undone.document_json.contains("HealthBar"));
        assert!(!undone.can_undo, "the form was a single entry");

        // A form containing a bad edit applies none of it.
        let before = sessions.open(&dir, HUD).expect("open").document_json;
        let error = sessions
            .apply_many(
                &dir,
                HUD,
                &[
                    HudAction::RenameWidget {
                        id: bar.clone(),
                        name: "Armour".to_owned(),
                    },
                    HudAction::SetProp {
                        id: bar,
                        prop: "thickness".to_owned(),
                        value: serde_json::json!(2),
                    },
                ],
                "bad form",
            )
            .expect_err("the second edit is invalid");
        assert!(error.hint.is_some());
        assert_eq!(
            sessions.open(&dir, HUD).expect("open").document_json,
            before,
            "the valid first edit was rolled back too"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keyboard_style_reparent_and_reorder_are_one_undo_step() {
        let dir = temp("tree-move");
        let mut sessions = HudSessions::new();
        let initial = sessions.open(&dir, HUD).expect("open");
        let pause = initial
            .widgets
            .iter()
            .find(|widget| widget.name == "PauseButton")
            .expect("button")
            .id
            .clone();
        let panel_state = sessions
            .apply(
                &dir,
                HUD,
                &HudAction::AddWidget {
                    widget: "panel".to_owned(),
                    name: Some("MenuPanel".to_owned()),
                    parent: None,
                },
            )
            .expect("panel");
        let panel = panel_state
            .widgets
            .iter()
            .find(|widget| widget.name == "MenuPanel")
            .expect("panel view")
            .id
            .clone();
        let moved = sessions
            .apply_many(
                &dir,
                HUD,
                &[
                    HudAction::ReparentWidget {
                        id: pause.clone(),
                        parent: Some(panel),
                    },
                    HudAction::ReorderWidget {
                        id: pause.clone(),
                        order: 7,
                    },
                ],
                "indent PauseButton",
            )
            .expect("move");
        let moved_button = moved
            .widgets
            .iter()
            .find(|widget| widget.id == pause)
            .expect("moved button");
        assert_eq!(moved_button.order, 7);
        assert!(moved_button.parent.is_some());

        let undone = sessions.undo(&dir, HUD).expect("undo move");
        let restored = undone
            .widgets
            .iter()
            .find(|widget| widget.id == pause)
            .expect("restored button");
        assert!(restored.parent.is_none());
        assert_ne!(restored.order, 7);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_writes_a_document_the_parser_accepts_and_reload_discards_edits() {
        let dir = temp("save");
        let mut sessions = HudSessions::new();
        let state = sessions.open(&dir, HUD).expect("open");
        let score = state
            .widgets
            .iter()
            .find(|widget| widget.name == "ScoreLabel")
            .expect("label")
            .id
            .clone();
        sessions
            .apply(
                &dir,
                HUD,
                &HudAction::SetProp {
                    id: score,
                    prop: "text".to_owned(),
                    value: serde_json::json!("Points: {score}"),
                },
            )
            .expect("applies");
        let saved = sessions.save(&dir, HUD).expect("save");
        assert!(!saved.dirty);

        let text = std::fs::read_to_string(dir.join(HUD)).expect("read");
        let parsed = bhippi_engine::hud::HudDocument::parse(&text).expect("strict parse");
        assert!(parsed.widgets.iter().any(|widget| widget
            .props
            .get("text")
            .is_some_and(|value| value == "Points: {score}")));

        // An unsaved edit is dropped by reload.
        let bar = saved
            .widgets
            .iter()
            .find(|widget| widget.name == "HealthBar")
            .expect("bar")
            .id
            .clone();
        sessions
            .apply(
                &dir,
                HUD,
                &HudAction::RenameWidget {
                    id: bar,
                    name: "Temporary".to_owned(),
                },
            )
            .expect("applies");
        let reloaded = sessions.reload(&dir, HUD).expect("reload");
        assert!(!reloaded.document_json.contains("Temporary"));
        assert!(!reloaded.dirty);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_widget_catalog_matches_the_engine_registry() {
        let catalog = widget_catalog();
        assert_eq!(catalog.len(), 12);
        let button = catalog
            .iter()
            .find(|entry| entry.kind == "button")
            .expect("button");
        assert_eq!(button.label, "Button");
        assert!(!button.is_container);
        assert!(button.props.iter().any(|prop| prop.name == "text"));

        let bar = catalog
            .iter()
            .find(|entry| entry.kind == "progress_bar")
            .expect("bar");
        assert_eq!(bar.label, "Progress Bar");
        let direction = bar
            .props
            .iter()
            .find(|prop| prop.name == "direction")
            .expect("direction");
        assert_eq!(direction.kind, "enum");
        assert!(direction.options.contains(&"left_to_right".to_owned()));

        assert!(
            catalog
                .iter()
                .find(|entry| entry.kind == "panel")
                .expect("panel")
                .is_container
        );
    }

    #[test]
    fn widgets_come_back_parents_before_children_with_a_depth() {
        let dir = temp("tree");
        let mut sessions = HudSessions::new();
        sessions.open(&dir, HUD).expect("open");
        let panel = sessions
            .apply(
                &dir,
                HUD,
                &HudAction::AddWidget {
                    widget: "panel".to_owned(),
                    name: Some("Box".to_owned()),
                    parent: None,
                },
            )
            .expect("panel")
            .selection
            .expect("selected");
        let state = sessions
            .apply(
                &dir,
                HUD,
                &HudAction::AddWidget {
                    widget: "text".to_owned(),
                    name: Some("Inner".to_owned()),
                    parent: Some(panel.clone()),
                },
            )
            .expect("child");

        let panel_at = state
            .widgets
            .iter()
            .position(|widget| widget.id == panel)
            .expect("panel listed");
        let child_at = state
            .widgets
            .iter()
            .position(|widget| widget.name == "Inner")
            .expect("child listed");
        assert!(panel_at < child_at, "parents come first");
        assert_eq!(state.widgets[child_at].depth, 1);
        assert_eq!(state.widgets[panel_at].depth, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
