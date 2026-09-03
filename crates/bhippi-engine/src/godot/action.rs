//! Typed Godot edits, lowered to file changes that carry their own inverse.
//!
//! Every action names files and node paths, never handles. Lowering loads the current bytes
//! of each file it touches, applies the whole batch **in memory** through the `.tscn` and
//! `project.godot` models, and hands back both the new bytes and the ones that were there
//! before. That is what makes undo exact rather than approximate: putting a scene back is
//! writing `before`, not replaying an opposite edit and hoping it lands.
//!
//! Lowering is all-or-nothing. An action that cannot be applied stops the batch with the
//! failing index and a hint, and nothing is written — a half-applied batch is a project in a
//! state no one asked for and no one can name.

use super::export_presets::ExportPresets;
use super::project::{GodotProjectFile, DEFAULT_INPUT_DEADZONE};
use super::tscn::{node_path, Connection, TscnDocument, TscnNode, TscnValue};
use super::{check_node_name, join_node_path, rel_to_res, res_to_rel};
use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

/// The file `project.godot` always is, relative to the project root.
pub const PROJECT_FILE: &str = "project.godot";
/// The export presets file, relative to the project root.
pub const EXPORT_PRESETS_FILE: &str = "export_presets.cfg";
/// The extension a GDScript file must have.
pub const SCRIPT_EXTENSION: &str = "gd";
/// The largest script this will write. Beyond this something has gone wrong upstream.
pub const MAX_SCRIPT_BYTES: usize = 2 * 1024 * 1024;

/// One edit to a Godot project.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GodotAction {
    /// Create a new `.tscn` with a single typed root node.
    CreateScene {
        path: String,
        root_name: String,
        root_type: String,
    },
    AddNode {
        scene: String,
        /// `"."` is the scene root.
        parent: String,
        name: String,
        #[serde(rename = "type")]
        type_: String,
        #[serde(default)]
        properties: Vec<(String, TscnValue)>,
        #[serde(default)]
        groups: Vec<String>,
    },
    /// Remove a node and everything under it.
    RemoveNode {
        scene: String,
        path: String,
    },
    RenameNode {
        scene: String,
        path: String,
        name: String,
    },
    ReparentNode {
        scene: String,
        path: String,
        new_parent: String,
    },
    SetProperty {
        scene: String,
        path: String,
        property: String,
        value: TscnValue,
    },
    RemoveProperty {
        scene: String,
        path: String,
        property: String,
    },
    AddToGroup {
        scene: String,
        path: String,
        group: String,
    },
    AttachScript {
        scene: String,
        path: String,
        script_res_path: String,
    },
    /// Add a node that instances another scene.
    InstanceScene {
        scene: String,
        parent: String,
        name: String,
        scene_res_path: String,
    },
    ConnectSignal {
        scene: String,
        from: String,
        signal: String,
        to: String,
        method: String,
    },
    /// Write a GDScript file. The batch does **not** compile it; that is `--check-only`,
    /// which is a process and therefore the runner's job.
    WriteScript {
        path: String,
        source: String,
    },
    DeleteScript {
        path: String,
    },
    SetMainScene {
        res_path: String,
    },
    AddAutoload {
        name: String,
        res_path: String,
    },
    AddInputAction {
        name: String,
        keycodes: Vec<u32>,
        #[serde(default)]
        deadzone: Option<f64>,
    },
}

impl GodotAction {
    /// The `kind` discriminant serde writes for this action.
    ///
    /// Exhaustive on purpose: a variant added to [`GodotAction`] without a name here does not
    /// compile, which is what makes [`action_kinds`] a real inventory rather than a list
    /// somebody remembered to update.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CreateScene { .. } => "create_scene",
            Self::AddNode { .. } => "add_node",
            Self::RemoveNode { .. } => "remove_node",
            Self::RenameNode { .. } => "rename_node",
            Self::ReparentNode { .. } => "reparent_node",
            Self::SetProperty { .. } => "set_property",
            Self::RemoveProperty { .. } => "remove_property",
            Self::AddToGroup { .. } => "add_to_group",
            Self::AttachScript { .. } => "attach_script",
            Self::InstanceScene { .. } => "instance_scene",
            Self::ConnectSignal { .. } => "connect_signal",
            Self::WriteScript { .. } => "write_script",
            Self::DeleteScript { .. } => "delete_script",
            Self::SetMainScene { .. } => "set_main_scene",
            Self::AddAutoload { .. } => "add_autoload",
            Self::AddInputAction { .. } => "add_input_action",
        }
    }

    /// One value of every variant, used only to read the shape back out of serde.
    ///
    /// Nothing here is a *description* of the vocabulary — the field names the schema hint
    /// prints come from `serde_json` serialising these values, so a renamed field renames
    /// itself in the hint and in the prompt guard together. A variant missing from this list
    /// is caught by `every_variant_has_a_sample`.
    #[must_use]
    pub fn samples() -> Vec<Self> {
        vec![
            Self::CreateScene {
                path: "scenes/level_01.tscn".to_owned(),
                root_name: "Level".to_owned(),
                root_type: "Node3D".to_owned(),
            },
            Self::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "Coin".to_owned(),
                type_: "Area3D".to_owned(),
                properties: Vec::new(),
                groups: Vec::new(),
            },
            Self::RemoveNode {
                scene: "scenes/main.tscn".to_owned(),
                path: "Coin".to_owned(),
            },
            Self::RenameNode {
                scene: "scenes/main.tscn".to_owned(),
                path: "Coin".to_owned(),
                name: "Pickup".to_owned(),
            },
            Self::ReparentNode {
                scene: "scenes/main.tscn".to_owned(),
                path: "Coin".to_owned(),
                new_parent: "Level".to_owned(),
            },
            Self::SetProperty {
                scene: "scenes/main.tscn".to_owned(),
                path: "Player".to_owned(),
                property: "speed".to_owned(),
                value: TscnValue::Float(6.0),
            },
            Self::RemoveProperty {
                scene: "scenes/main.tscn".to_owned(),
                path: "Player".to_owned(),
                property: "speed".to_owned(),
            },
            Self::AddToGroup {
                scene: "scenes/main.tscn".to_owned(),
                path: "Coin".to_owned(),
                group: "pickup".to_owned(),
            },
            Self::AttachScript {
                scene: "scenes/main.tscn".to_owned(),
                path: "Coin".to_owned(),
                script_res_path: "res://scripts/coin.gd".to_owned(),
            },
            Self::InstanceScene {
                scene: "scenes/main.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "Coin".to_owned(),
                scene_res_path: "res://scenes/coin.tscn".to_owned(),
            },
            Self::ConnectSignal {
                scene: "scenes/main.tscn".to_owned(),
                from: "Coin".to_owned(),
                signal: "body_entered".to_owned(),
                to: ".".to_owned(),
                method: "_on_coin_body_entered".to_owned(),
            },
            Self::WriteScript {
                path: "scripts/coin.gd".to_owned(),
                source: "extends Area3D\n".to_owned(),
            },
            Self::DeleteScript {
                path: "scripts/coin.gd".to_owned(),
            },
            Self::SetMainScene {
                res_path: "res://scenes/main.tscn".to_owned(),
            },
            Self::AddAutoload {
                name: "Game".to_owned(),
                res_path: "res://scripts/game.gd".to_owned(),
            },
            Self::AddInputAction {
                name: "sprint".to_owned(),
                keycodes: vec![4_194_325],
                deadzone: None,
            },
        ]
    }

    /// The one-line label for the Activity Dock and the undo entry.
    #[must_use]
    pub fn to_label(&self) -> String {
        match self {
            Self::CreateScene { path, .. } => format!("Create scene {path}"),
            Self::AddNode { name, type_, .. } => format!("Add {type_} `{name}`"),
            Self::RemoveNode { path, .. } => format!("Remove `{path}`"),
            Self::RenameNode { path, name, .. } => format!("Rename `{path}` to `{name}`"),
            Self::ReparentNode {
                path, new_parent, ..
            } => format!("Move `{path}` under `{new_parent}`"),
            Self::SetProperty { path, property, .. } => format!("Set `{path}`.{property}"),
            Self::RemoveProperty { path, property, .. } => {
                format!("Clear `{path}`.{property}")
            }
            Self::AddToGroup { path, group, .. } => format!("Add `{path}` to group {group}"),
            Self::AttachScript {
                path,
                script_res_path,
                ..
            } => format!("Attach {script_res_path} to `{path}`"),
            Self::InstanceScene {
                name,
                scene_res_path,
                ..
            } => format!("Instance {scene_res_path} as `{name}`"),
            Self::ConnectSignal {
                signal, from, to, ..
            } => format!("Connect {signal} from `{from}` to `{to}`"),
            Self::WriteScript { path, .. } => format!("Write {path}"),
            Self::DeleteScript { path } => format!("Delete {path}"),
            Self::SetMainScene { res_path } => format!("Set main scene to {res_path}"),
            Self::AddAutoload { name, .. } => format!("Register autoload {name}"),
            Self::AddInputAction { name, .. } => format!("Add input action {name}"),
        }
    }

    /// The node this action is about, when it is about one.
    #[must_use]
    pub fn node_path(&self) -> Option<String> {
        match self {
            Self::RemoveNode { path, .. }
            | Self::RenameNode { path, .. }
            | Self::ReparentNode { path, .. }
            | Self::SetProperty { path, .. }
            | Self::RemoveProperty { path, .. }
            | Self::AddToGroup { path, .. }
            | Self::AttachScript { path, .. } => Some(path.clone()),
            Self::AddNode { parent, name, .. } | Self::InstanceScene { parent, name, .. } => {
                Some(join_node_path(parent, name))
            }
            Self::ConnectSignal { from, .. } => Some(from.clone()),
            _ => None,
        }
    }
}

/// Every action kind the batch vocabulary accepts, in declaration order.
///
/// Read off [`GodotAction::samples`] rather than written out again, so the prompt guard, the
/// capability map and the schema hints all count the same verbs.
#[must_use]
pub fn action_kinds() -> Vec<&'static str> {
    GodotAction::samples()
        .iter()
        .map(GodotAction::kind)
        .collect()
}

/// A one-line reminder of the fields one action kind takes: `add_node{groups,name,parent,…}`.
///
/// Generated from the enum itself — the names are the keys `serde_json` writes for a real
/// value of that variant, minus the `kind` tag — so there is no second catalogue that can
/// drift from the code the batch is actually parsed by. `None` for a kind that does not
/// exist, which is itself the useful answer when a model invents a verb.
#[must_use]
pub fn action_schema_hint(kind: &str) -> Option<String> {
    let sample = GodotAction::samples()
        .into_iter()
        .find(|action| action.kind() == kind)?;
    let value = serde_json::to_value(&sample).ok()?;
    let object = value.as_object()?;
    let fields = object
        .keys()
        .filter(|key| key.as_str() != "kind")
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(",");
    Some(format!("{kind}{{{fields}}}"))
}

/// An ordered batch that succeeds or fails as one.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GodotActionBatch {
    pub label: String,
    pub actions: Vec<GodotAction>,
}

impl GodotActionBatch {
    #[must_use]
    pub fn new(label: impl Into<String>, actions: Vec<GodotAction>) -> Self {
        Self {
            label: label.into(),
            actions,
        }
    }

    #[must_use]
    pub fn display_label(&self) -> String {
        if self.label.trim().is_empty() {
            format!("{} Godot actions", self.actions.len())
        } else {
            self.label.clone()
        }
    }
}

/// One file the batch changed, with everything needed to put it back.
///
/// `before: None` means the file did not exist, so undo deletes it. `after: None` means the
/// batch deleted it, so undo restores `before`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GodotFileChange {
    /// Project-relative, forward slashes.
    pub path: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
}

/// What one action did.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GodotActionOutcome {
    pub index: usize,
    pub ok: bool,
    pub message: String,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub node_path: Option<String>,
    /// True when the file this action wrote still has to survive `--check-only` before it
    /// can be trusted. Lowering never runs Godot, so it says so rather than implying it did.
    #[serde(default)]
    pub needs_check: bool,
}

/// The result of lowering a batch: nothing has been written yet.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GodotChangeSet {
    pub label: String,
    pub changes: Vec<GodotFileChange>,
    pub outcomes: Vec<GodotActionOutcome>,
}

impl GodotChangeSet {
    /// True when nothing actually differs — an idempotent batch.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// The scripts this batch wrote, which the caller should run `--check-only` over.
    #[must_use]
    pub fn scripts_needing_check(&self) -> Vec<String> {
        self.changes
            .iter()
            .filter(|change| {
                change.after.is_some() && change.path.ends_with(&format!(".{SCRIPT_EXTENSION}"))
            })
            .map(|change| change.path.clone())
            .collect()
    }
}

/// A failing action, with its position in the batch.
#[derive(Clone, Debug)]
pub struct GodotBatchError {
    pub index: usize,
    pub label: String,
    pub error: EngineError,
}

impl fmt::Display for GodotBatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "action {} ({}): {}",
            self.index, self.label, self.error
        )
    }
}

impl std::error::Error for GodotBatchError {}

impl From<GodotBatchError> for EngineError {
    fn from(value: GodotBatchError) -> Self {
        let hint = value.error.hint().map(str::to_owned);
        Self::Action(value.to_string(), hint)
    }
}

/// Swap every change's direction. Applying the result undoes the original exactly.
#[must_use]
pub fn invert(changeset: &GodotChangeSet) -> GodotChangeSet {
    GodotChangeSet {
        label: format!("Undo {}", changeset.label),
        changes: changeset
            .changes
            .iter()
            .map(|change| GodotFileChange {
                path: change.path.clone(),
                before: change.after.clone(),
                after: change.before.clone(),
            })
            .collect(),
        outcomes: Vec::new(),
    }
}

/// Write a change set to disk.
///
/// Each file is written to a sibling temp file and renamed over the target, so a crash
/// half-way leaves the old file intact rather than a truncated one. Deletions happen last
/// for the same reason.
pub fn apply_changeset(project_root: &Path, changeset: &GodotChangeSet) -> Result<()> {
    for change in &changeset.changes {
        let target = project_root.join(&change.path);
        match &change.after {
            Some(bytes) => write_atomic(&target, bytes)?,
            None => remove_if_present(&target)?,
        }
    }
    Ok(())
}

fn write_atomic(target: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| EngineError::Io {
            operation: "create directory",
            path: parent.display().to_string(),
            reason: error.to_string(),
            hint: Some("Check the project folder is writable.".to_owned()),
        })?;
    }
    let temp = temp_path(target);
    std::fs::write(&temp, bytes).map_err(|error| EngineError::Io {
        operation: "write",
        path: temp.display().to_string(),
        reason: error.to_string(),
        hint: Some("Check the project folder is writable.".to_owned()),
    })?;
    std::fs::rename(&temp, target).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        EngineError::Io {
            operation: "replace",
            path: target.display().to_string(),
            reason: error.to_string(),
            hint: Some("Close the file in the Godot editor and try again.".to_owned()),
        }
    })
}

fn temp_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "bhippi".to_owned());
    target.with_file_name(format!(".{name}.bhippi-tmp"))
}

fn remove_if_present(target: &Path) -> Result<()> {
    if !target.exists() {
        return Ok(());
    }
    std::fs::remove_file(target).map_err(|error| EngineError::Io {
        operation: "delete",
        path: target.display().to_string(),
        reason: error.to_string(),
        hint: Some("Close the file in the Godot editor and try again.".to_owned()),
    })
}

// ── lowering ─────────────────────────────────────────────────────────────────────────

/// Apply a batch in memory and return the file changes it implies.
///
/// Nothing is written: the caller reviews the change set, then calls [`apply_changeset`].
pub fn lower(
    project_root: &Path,
    batch: &GodotActionBatch,
) -> std::result::Result<GodotChangeSet, GodotBatchError> {
    let mut work = Lowering::new(project_root);
    let mut outcomes = Vec::new();
    for (index, action) in batch.actions.iter().enumerate() {
        let outcome = work.apply(index, action).map_err(|error| GodotBatchError {
            index,
            label: action.to_label(),
            error,
        })?;
        outcomes.push(outcome);
    }
    Ok(GodotChangeSet {
        label: batch.display_label(),
        changes: work.finish().map_err(|error| GodotBatchError {
            index: batch.actions.len().saturating_sub(1),
            label: batch.display_label(),
            error,
        })?,
        outcomes,
    })
}

struct Lowering<'a> {
    root: &'a Path,
    originals: BTreeMap<String, Option<Vec<u8>>>,
    scenes: BTreeMap<String, TscnDocument>,
    project: Option<GodotProjectFile>,
    presets: Option<ExportPresets>,
    files: BTreeMap<String, Option<Vec<u8>>>,
}

impl<'a> Lowering<'a> {
    fn new(root: &'a Path) -> Self {
        Self {
            root,
            originals: BTreeMap::new(),
            scenes: BTreeMap::new(),
            project: None,
            presets: None,
            files: BTreeMap::new(),
        }
    }

    /// The bytes on disk when the batch started, remembered for the inverse.
    fn original(&mut self, rel: &str) -> Option<Vec<u8>> {
        if let Some(cached) = self.originals.get(rel) {
            return cached.clone();
        }
        let bytes = std::fs::read(self.root.join(rel)).ok();
        self.originals.insert(rel.to_owned(), bytes.clone());
        bytes
    }

    fn scene_text(&mut self, rel: &str) -> Result<String> {
        let bytes = self.original(rel).ok_or_else(|| {
            EngineError::NotFound(
                format!("scene {rel}"),
                Some("Create the scene first, or check the path.".to_owned()),
            )
        })?;
        String::from_utf8(bytes).map_err(|_| {
            EngineError::Scene(
                format!("{rel} is not UTF-8 text"),
                Some(
                    "Text scenes (.tscn) are UTF-8; a binary .scn cannot be edited here."
                        .to_owned(),
                ),
            )
        })
    }

    fn scene(&mut self, reference: &str) -> Result<&mut TscnDocument> {
        let rel = res_to_rel(reference);
        if !self.scenes.contains_key(&rel) {
            let text = self.scene_text(&rel)?;
            let document = super::tscn::parse(&text)?;
            self.scenes.insert(rel.clone(), document);
        }
        self.scenes.get_mut(&rel).ok_or_else(|| {
            EngineError::Scene(
                format!("scene {rel} vanished mid-batch"),
                Some("Retry the batch.".to_owned()),
            )
        })
    }

    fn project(&mut self) -> Result<&mut GodotProjectFile> {
        if self.project.is_none() {
            let bytes = self.original(PROJECT_FILE).ok_or_else(|| {
                EngineError::NotFound(
                    PROJECT_FILE.to_owned(),
                    Some("This folder is not a Godot project; create one first.".to_owned()),
                )
            })?;
            let text = String::from_utf8(bytes).map_err(|_| {
                EngineError::Manifest(
                    "project.godot is not UTF-8 text".to_owned(),
                    Some("Restore it from version control.".to_owned()),
                )
            })?;
            self.project = Some(GodotProjectFile::parse(&text)?);
        }
        self.project.as_mut().ok_or_else(|| {
            EngineError::Manifest(
                "project.godot vanished mid-batch".to_owned(),
                Some("Retry the batch.".to_owned()),
            )
        })
    }

    /// Everything the batch changed, as ordered file changes. Files whose bytes came out
    /// identical are not reported at all: an edit that changed nothing is not a change.
    fn finish(mut self) -> Result<Vec<GodotFileChange>> {
        let mut pending: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        let scenes: Vec<(String, TscnDocument)> = self
            .scenes
            .iter()
            .map(|(rel, document)| (rel.clone(), document.clone()))
            .collect();
        for (rel, document) in scenes {
            pending.insert(rel, Some(document.to_text().into_bytes()));
        }
        if let Some(project) = &self.project {
            pending.insert(
                PROJECT_FILE.to_owned(),
                Some(project.to_text().into_bytes()),
            );
        }
        if let Some(presets) = &self.presets {
            pending.insert(
                EXPORT_PRESETS_FILE.to_owned(),
                Some(presets.to_text().into_bytes()),
            );
        }
        for (rel, bytes) in &self.files {
            pending.insert(rel.clone(), bytes.clone());
        }
        let mut changes = Vec::new();
        for (rel, after) in pending {
            let before = self.original(&rel);
            if before == after {
                continue;
            }
            changes.push(GodotFileChange {
                path: rel,
                before,
                after,
            });
        }
        Ok(changes)
    }

    fn apply(&mut self, index: usize, action: &GodotAction) -> Result<GodotActionOutcome> {
        let mut needs_check = false;
        match action {
            GodotAction::CreateScene {
                path,
                root_name,
                root_type,
            } => {
                check_node_name(root_name)?;
                check_type_name(root_type)?;
                let rel = res_to_rel(path);
                check_scene_path(&rel)?;
                if self.original(&rel).is_some() {
                    return Err(EngineError::Action(
                        format!("{rel} already exists"),
                        Some("Edit the existing scene, or choose another path.".to_owned()),
                    ));
                }
                let mut document = TscnDocument::new_scene(root_name, root_type);
                document.refresh_load_steps();
                self.scenes.insert(rel, document);
            }
            GodotAction::AddNode {
                scene,
                parent,
                name,
                type_,
                properties,
                groups,
            } => {
                check_node_name(name)?;
                check_type_name(type_)?;
                let document = self.scene(scene)?;
                require_node(document, parent)?;
                require_free(document, parent, name)?;
                let mut node = TscnNode::new(name, type_, Some(parent));
                node.groups = groups.clone();
                for (key, value) in properties {
                    node.set(key, value.clone());
                }
                insert_after_subtree(document, parent, node);
            }
            GodotAction::RemoveNode { scene, path } => {
                if path == "." {
                    return Err(EngineError::Action(
                        "the scene root cannot be removed".to_owned(),
                        Some(
                            "Delete the scene file instead, or replace the root's type.".to_owned(),
                        ),
                    ));
                }
                let document = self.scene(scene)?;
                require_node(document, path)?;
                let doomed = subtree_paths(document, path);
                document
                    .nodes
                    .retain(|node| !doomed.contains(&node_path(node)));
                document.connections.retain(|connection| {
                    !doomed.contains(&connection.from) && !doomed.contains(&connection.to)
                });
                document
                    .editables
                    .retain(|editable| !doomed.contains(editable));
                document.prune_ext_resources();
                document.refresh_load_steps();
            }
            GodotAction::RenameNode { scene, path, name } => {
                check_node_name(name)?;
                let document = self.scene(scene)?;
                require_node(document, path)?;
                let parent = super::parent_node_path(path).unwrap_or_else(|| ".".to_owned());
                if path != "." {
                    require_free(document, &parent, name)?;
                }
                let new_path = if path == "." {
                    ".".to_owned()
                } else {
                    join_node_path(&parent, name)
                };
                let Some(index) = node_index(document, path) else {
                    return Err(missing_node(path));
                };
                if let Some(node) = document.nodes.get_mut(index) {
                    node.name = name.clone();
                }
                rewrite_paths(document, path, &new_path);
            }
            GodotAction::ReparentNode {
                scene,
                path,
                new_parent,
            } => {
                if path == "." {
                    return Err(EngineError::Action(
                        "the scene root has no parent to change".to_owned(),
                        Some("Add a new root and move the old one under it instead.".to_owned()),
                    ));
                }
                let document = self.scene(scene)?;
                require_node(document, path)?;
                require_node(document, new_parent)?;
                if new_parent == path || new_parent.starts_with(&format!("{path}/")) {
                    return Err(EngineError::Action(
                        format!("`{new_parent}` is inside `{path}`"),
                        Some("A node cannot become its own descendant's child.".to_owned()),
                    ));
                }
                let name = super::node_path_name(path).to_owned();
                require_free(document, new_parent, &name)?;
                let new_path = join_node_path(new_parent, &name);
                let Some(index) = node_index(document, path) else {
                    return Err(missing_node(path));
                };
                if let Some(node) = document.nodes.get_mut(index) {
                    node.parent = Some(new_parent.clone());
                }
                rewrite_paths(document, path, &new_path);
            }
            GodotAction::SetProperty {
                scene,
                path,
                property,
                value,
            } => {
                check_property_name(property)?;
                let document = self.scene(scene)?;
                let Some(index) = node_index(document, path) else {
                    return Err(missing_node(path));
                };
                if let Some(node) = document.nodes.get_mut(index) {
                    node.set(property, value.clone());
                }
            }
            GodotAction::RemoveProperty {
                scene,
                path,
                property,
            } => {
                let document = self.scene(scene)?;
                let Some(index) = node_index(document, path) else {
                    return Err(missing_node(path));
                };
                let removed = document
                    .nodes
                    .get_mut(index)
                    .map(|node| node.remove(property))
                    .unwrap_or(false);
                if !removed {
                    return Err(EngineError::NotFound(
                        format!("`{path}` has no property `{property}`"),
                        Some(
                            "Read the node first; only properties the scene stores can be cleared."
                                .to_owned(),
                        ),
                    ));
                }
            }
            GodotAction::AddToGroup { scene, path, group } => {
                check_group_name(group)?;
                let document = self.scene(scene)?;
                let Some(index) = node_index(document, path) else {
                    return Err(missing_node(path));
                };
                if let Some(node) = document.nodes.get_mut(index) {
                    if !node.groups.contains(group) {
                        node.groups.push(group.clone());
                    }
                }
            }
            GodotAction::AttachScript {
                scene,
                path,
                script_res_path,
            } => {
                check_script_path(&res_to_rel(script_res_path))?;
                let document = self.scene(scene)?;
                let Some(index) = node_index(document, path) else {
                    return Err(missing_node(path));
                };
                let id = document.ensure_ext_resource("Script", script_res_path);
                if let Some(node) = document.nodes.get_mut(index) {
                    node.set("script", TscnValue::ExtResource(id));
                }
                document.refresh_load_steps();
            }
            GodotAction::InstanceScene {
                scene,
                parent,
                name,
                scene_res_path,
            } => {
                check_node_name(name)?;
                check_scene_path(&res_to_rel(scene_res_path))?;
                let document = self.scene(scene)?;
                require_node(document, parent)?;
                require_free(document, parent, name)?;
                let id = document.ensure_ext_resource("PackedScene", scene_res_path);
                let mut node = TscnNode::new(name, "Node", Some(parent));
                node.type_ = None;
                node.instance = Some(TscnValue::ExtResource(id));
                insert_after_subtree(document, parent, node);
                document.refresh_load_steps();
            }
            GodotAction::ConnectSignal {
                scene,
                from,
                signal,
                to,
                method,
            } => {
                check_identifier(signal, "signal")?;
                check_identifier(method, "method")?;
                let document = self.scene(scene)?;
                require_node(document, from)?;
                require_node(document, to)?;
                let connection = Connection {
                    signal: signal.clone(),
                    from: from.clone(),
                    to: to.clone(),
                    method: method.clone(),
                    flags: None,
                    binds: None,
                    unbinds: None,
                    order: Vec::new(),
                };
                if !document.connections.contains(&connection) {
                    document.connections.push(connection);
                }
            }
            GodotAction::WriteScript { path, source } => {
                let rel = res_to_rel(path);
                check_script_path(&rel)?;
                if source.trim().is_empty() {
                    return Err(EngineError::Action(
                        format!("{rel} would be empty"),
                        Some("A GDScript file needs at least an `extends` line.".to_owned()),
                    ));
                }
                if source.len() > MAX_SCRIPT_BYTES {
                    return Err(EngineError::Action(
                        format!("{rel} is {} bytes, past the cap", source.len()),
                        Some("Split the script; nothing this large is one file.".to_owned()),
                    ));
                }
                self.files.insert(rel, Some(source.clone().into_bytes()));
                needs_check = true;
            }
            GodotAction::DeleteScript { path } => {
                let rel = res_to_rel(path);
                check_script_path(&rel)?;
                if self.original(&rel).is_none() {
                    return Err(EngineError::NotFound(
                        rel,
                        Some("The script is already gone.".to_owned()),
                    ));
                }
                self.files.insert(rel, None);
            }
            GodotAction::SetMainScene { res_path } => {
                let rel = res_to_rel(res_path);
                check_scene_path(&rel)?;
                self.project()?.set_main_scene(&rel);
            }
            GodotAction::AddAutoload { name, res_path } => {
                check_identifier(name, "autoload name")?;
                let rel = res_to_rel(res_path);
                check_script_path(&rel)?;
                self.project()?.add_autoload(name, &rel_to_res(&rel), true);
            }
            GodotAction::AddInputAction {
                name,
                keycodes,
                deadzone,
            } => {
                check_identifier(name, "action name")?;
                if keycodes.is_empty() {
                    return Err(EngineError::Action(
                        format!("input action `{name}` has no keys"),
                        Some("Give the action at least one keycode.".to_owned()),
                    ));
                }
                let deadzone = deadzone.unwrap_or(DEFAULT_INPUT_DEADZONE);
                if !(0.0..=1.0).contains(&deadzone) {
                    return Err(EngineError::Action(
                        format!("deadzone {deadzone} is outside 0.0 – 1.0"),
                        Some("Godot's default is 0.5.".to_owned()),
                    ));
                }
                self.project()?.add_input_action(name, keycodes, deadzone);
            }
        }
        Ok(GodotActionOutcome {
            index,
            ok: true,
            message: action.to_label(),
            hint: None,
            node_path: action.node_path(),
            needs_check,
        })
    }
}

// ── tree helpers ─────────────────────────────────────────────────────────────────────

fn missing_node(path: &str) -> EngineError {
    EngineError::NotFound(
        format!("node `{path}`"),
        Some("Node paths are `.` for the root and `Parent/Child` below it.".to_owned()),
    )
}

fn node_index(document: &TscnDocument, path: &str) -> Option<usize> {
    document
        .nodes
        .iter()
        .position(|node| node_path(node) == path)
}

fn require_node(document: &TscnDocument, path: &str) -> Result<()> {
    if node_index(document, path).is_some() {
        Ok(())
    } else {
        Err(missing_node(path))
    }
}

/// Godot cannot hold two children of the same parent under one name — the second silently
/// becomes `Name2`, and every `NodePath` written against `Name` then points at the wrong one.
fn require_free(document: &TscnDocument, parent: &str, name: &str) -> Result<()> {
    let path = join_node_path(parent, name);
    if node_index(document, &path).is_some() {
        return Err(EngineError::Action(
            format!("`{parent}` already has a child named `{name}`"),
            Some("Pick another name, or edit the existing node.".to_owned()),
        ));
    }
    Ok(())
}

/// Every path in the subtree rooted at `path`, including `path` itself.
fn subtree_paths(document: &TscnDocument, path: &str) -> BTreeSet<String> {
    // `"."` is the whole scene: a child of the root is `"Player"`, not `"./Player"`, so the
    // prefix test below would match nothing and a new root child would land in front of the
    // existing ones.
    if path == "." {
        return document.nodes.iter().map(node_path).collect();
    }
    let prefix = format!("{path}/");
    document
        .nodes
        .iter()
        .map(node_path)
        .filter(|candidate| candidate == path || candidate.starts_with(&prefix))
        .collect()
}

/// Insert a node directly after its parent's existing subtree, which is where Godot writes a
/// new child: node order in the file is the order the scene tree is built in.
fn insert_after_subtree(document: &mut TscnDocument, parent: &str, node: TscnNode) {
    let subtree = subtree_paths(document, parent);
    let at = document
        .nodes
        .iter()
        .rposition(|existing| subtree.contains(&node_path(existing)))
        .map_or(document.nodes.len(), |position| position + 1);
    document.nodes.insert(at, node);
}

/// Re-point everything that named `old_path` (or something under it) at `new_path`.
///
/// A rename or a move that left connections and `editable` lines behind would produce a
/// scene Godot opens with "Node not found" errors, which is the failure mode that makes
/// people stop trusting an editor.
fn rewrite_paths(document: &mut TscnDocument, old_path: &str, new_path: &str) {
    if old_path == new_path {
        return;
    }
    let old_prefix = format!("{old_path}/");
    let rewrite = |value: &str| -> Option<String> {
        if value == old_path {
            Some(new_path.to_owned())
        } else {
            value
                .strip_prefix(&old_prefix)
                .map(|rest| format!("{new_path}/{rest}"))
        }
    };
    for node in &mut document.nodes {
        if let Some(parent) = &node.parent {
            if let Some(updated) = rewrite(parent) {
                node.parent = Some(updated);
            }
        }
    }
    for connection in &mut document.connections {
        if let Some(updated) = rewrite(&connection.from) {
            connection.from = updated;
        }
        if let Some(updated) = rewrite(&connection.to) {
            connection.to = updated;
        }
    }
    for editable in &mut document.editables {
        if let Some(updated) = rewrite(editable) {
            *editable = updated;
        }
    }
}

// ── name and path checks ─────────────────────────────────────────────────────────────

fn check_type_name(type_: &str) -> Result<()> {
    let valid = !type_.is_empty()
        && type_
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && type_
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    if valid {
        Ok(())
    } else {
        Err(EngineError::Action(
            format!("`{type_}` is not a Godot class name"),
            Some(
                "Use a class from Godot's hierarchy, such as Node3D or CharacterBody2D.".to_owned(),
            ),
        ))
    }
}

fn check_identifier(value: &str, what: &str) -> Result<()> {
    let valid = !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    if valid {
        Ok(())
    } else {
        Err(EngineError::Action(
            format!("`{value}` is not a valid {what}"),
            Some("Use letters, digits and underscores.".to_owned()),
        ))
    }
}

fn check_group_name(group: &str) -> Result<()> {
    if group.trim().is_empty() || group.contains('"') || group.contains('\n') {
        return Err(EngineError::Action(
            format!("`{group}` is not a usable group name"),
            Some("Group names are plain words, like `enemy` or `bhippi_track`.".to_owned()),
        ));
    }
    Ok(())
}

fn check_property_name(property: &str) -> Result<()> {
    let valid = !property.is_empty()
        && !property.starts_with(' ')
        && property.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '/' | '.' | ':')
        });
    if valid {
        Ok(())
    } else {
        Err(EngineError::Action(
            format!("`{property}` is not a property name a scene file can hold"),
            Some(
                "Properties look like `position`, `theme_override_constants/margin_left`."
                    .to_owned(),
            ),
        ))
    }
}

fn check_relative(rel: &str) -> Result<()> {
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.contains("..")
        || rel.contains(':')
        || rel.contains('\\')
    {
        return Err(EngineError::Action(
            format!("`{rel}` is not a path inside the project"),
            Some(
                "Use a project-relative path such as `scenes/main.tscn` or a res:// one."
                    .to_owned(),
            ),
        ));
    }
    Ok(())
}

fn check_scene_path(rel: &str) -> Result<()> {
    check_relative(rel)?;
    if !rel.ends_with(".tscn") {
        return Err(EngineError::Action(
            format!("`{rel}` is not a .tscn scene"),
            Some(
                "Bhippi edits Godot's text scene format; save binary .scn from the editor."
                    .to_owned(),
            ),
        ));
    }
    Ok(())
}

fn check_script_path(rel: &str) -> Result<()> {
    check_relative(rel)?;
    if !rel.ends_with(".gd") {
        return Err(EngineError::Action(
            format!("`{rel}` is not a .gd script"),
            Some("GDScript files end in .gd.".to_owned()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_changeset, invert, lower, GodotAction, GodotActionBatch, GodotChangeSet};
    use crate::godot::scene::GodotScene;
    use crate::godot::tscn::TscnValue;
    use std::path::{Path, PathBuf};

    const MAIN: &str = include_str!("../../../../tests/fixtures/godot/main.tscn");
    const PROJECT: &str = include_str!("../../../../tests/fixtures/godot/project.godot");

    /// A throwaway project on disk holding the two fixtures.
    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("bhippi-godot-action-{name}"));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("scenes")).expect("scenes dir");
            std::fs::create_dir_all(root.join("scripts")).expect("scripts dir");
            std::fs::write(root.join("scenes/main.tscn"), MAIN.replace("\r\n", "\n"))
                .expect("scene");
            std::fs::write(root.join("project.godot"), PROJECT.replace("\r\n", "\n"))
                .expect("project");
            std::fs::write(root.join("scripts/player.gd"), "extends Node\n").expect("script");
            Self { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        fn scene(&self) -> GodotScene {
            let text = std::fs::read_to_string(self.root.join("scenes/main.tscn"))
                .expect("scene readable");
            GodotScene::parse(&text).expect("scene parses")
        }

        fn run(&self, actions: Vec<GodotAction>) -> GodotChangeSet {
            let batch = GodotActionBatch::new("test", actions);
            let changeset = lower(&self.root, &batch).unwrap_or_else(|error| {
                panic!("lowering failed: {error}");
            });
            apply_changeset(&self.root, &changeset).expect("apply");
            changeset
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn adding_a_node_places_it_under_its_parent_and_nowhere_else() {
        let fixture = Fixture::new("add");
        fixture.run(vec![GodotAction::AddNode {
            scene: "scenes/main.tscn".to_owned(),
            parent: "Player".to_owned(),
            name: "Muzzle".to_owned(),
            type_: "Marker3D".to_owned(),
            properties: vec![("position".to_owned(), TscnValue::Vector3(0.0, 1.5, -0.5))],
            groups: vec!["muzzle".to_owned()],
        }]);

        let scene = fixture.scene();
        assert!(scene.contains("Player/Muzzle"));
        // Inserted after the Player subtree, so Floor and HUD keep their order.
        assert_eq!(
            scene.children("."),
            vec!["DirectionalLight3D", "Camera3D", "Player", "Floor", "HUD"]
        );
        let node = scene.node("Player/Muzzle").expect("the new node");
        assert_eq!(node.groups, vec!["muzzle"]);
        assert_eq!(
            node.properties
                .iter()
                .find(|(key, _)| key == "position")
                .map(|(_, value)| value.to_text()),
            Some("Vector3(0, 1.5, -0.5)".to_owned())
        );
    }

    #[test]
    fn removing_a_node_takes_its_children_connections_and_orphaned_resources() {
        let fixture = Fixture::new("remove");
        fixture.run(vec![GodotAction::RemoveNode {
            scene: "scenes/main.tscn".to_owned(),
            path: "HUD".to_owned(),
        }]);
        let scene = fixture.scene();
        assert!(!scene.contains("HUD"));
        assert!(!scene.contains("HUD/Control/Button"));
        assert!(
            scene.document.connections.is_empty(),
            "the connection pointed into HUD and had to go with it"
        );

        // The crate instance is the only user of the PackedScene resource.
        let before = fixture.scene().document.ext_resources.len();
        fixture.run(vec![GodotAction::RemoveNode {
            scene: "scenes/main.tscn".to_owned(),
            path: "Player/Crate".to_owned(),
        }]);
        let after = fixture.scene().document;
        assert_eq!(after.ext_resources.len(), before - 1);
        assert_eq!(after.header.load_steps, after.computed_load_steps());
        assert!(after.editables.is_empty(), "its editable line went too");
    }

    #[test]
    fn renaming_and_reparenting_rewrite_every_path_that_named_the_node() {
        let fixture = Fixture::new("move");
        fixture.run(vec![
            GodotAction::RenameNode {
                scene: "scenes/main.tscn".to_owned(),
                path: "HUD/Control".to_owned(),
                name: "Root".to_owned(),
            },
            GodotAction::ReparentNode {
                scene: "scenes/main.tscn".to_owned(),
                path: "Player/Crate".to_owned(),
                new_parent: "Floor".to_owned(),
            },
        ]);
        let scene = fixture.scene();
        assert!(scene.contains("HUD/Root/Button"));
        assert!(!scene.contains("HUD/Control"));
        assert_eq!(
            scene.document.connections.first().map(|c| c.from.clone()),
            Some("HUD/Root/Button".to_owned())
        );
        assert!(scene.contains("Floor/Crate"));
        assert!(!scene.contains("Player/Crate"));
        assert_eq!(scene.document.editables, vec!["Floor/Crate".to_owned()]);
    }

    #[test]
    fn scripts_signals_groups_and_project_settings_all_land_in_one_batch() {
        let fixture = Fixture::new("batch");
        let changeset = fixture.run(vec![
            GodotAction::WriteScript {
                path: "scripts/hud.gd".to_owned(),
                source: "extends Control\n\nfunc _ready() -> void:\n\tpass\n".to_owned(),
            },
            GodotAction::AttachScript {
                scene: "scenes/main.tscn".to_owned(),
                path: "HUD/Control".to_owned(),
                script_res_path: "res://scripts/hud.gd".to_owned(),
            },
            GodotAction::AddToGroup {
                scene: "scenes/main.tscn".to_owned(),
                path: "Floor".to_owned(),
                group: "bhippi_track".to_owned(),
            },
            GodotAction::ConnectSignal {
                scene: "scenes/main.tscn".to_owned(),
                from: "HUD/Control/Button".to_owned(),
                signal: "button_down".to_owned(),
                to: "HUD/Control".to_owned(),
                method: "_on_button_down".to_owned(),
            },
            GodotAction::SetMainScene {
                res_path: "res://scenes/main.tscn".to_owned(),
            },
            GodotAction::AddAutoload {
                name: "GameState".to_owned(),
                res_path: "res://scripts/hud.gd".to_owned(),
            },
            GodotAction::AddInputAction {
                name: "fire".to_owned(),
                keycodes: vec![70],
                deadzone: None,
            },
        ]);

        let mut touched: Vec<&str> = changeset
            .changes
            .iter()
            .map(|change| change.path.as_str())
            .collect();
        touched.sort_unstable();
        assert_eq!(
            touched,
            vec!["project.godot", "scenes/main.tscn", "scripts/hud.gd"]
        );
        assert_eq!(changeset.scripts_needing_check(), vec!["scripts/hud.gd"]);
        assert!(changeset.outcomes.iter().any(|outcome| outcome.needs_check));

        let scene = fixture.scene();
        assert_eq!(
            scene.node("HUD/Control").and_then(|node| node.script),
            Some("res://scripts/hud.gd".to_owned())
        );
        assert!(scene
            .find_in_group("bhippi_track")
            .contains(&"Floor".to_owned()));
        assert_eq!(scene.document.connections.len(), 2);

        let project = crate::godot::project::GodotProjectFile::parse(
            &std::fs::read_to_string(fixture.path().join("project.godot")).expect("project"),
        )
        .expect("project parses");
        assert!(project
            .autoloads()
            .iter()
            .any(|autoload| autoload.name == "GameState"));
        assert!(project.input_actions().contains(&"fire".to_owned()));
    }

    #[test]
    fn create_scene_writes_a_file_that_parses_back() {
        let fixture = Fixture::new("create");
        fixture.run(vec![
            GodotAction::CreateScene {
                path: "scenes/level_02.tscn".to_owned(),
                root_name: "Level02".to_owned(),
                root_type: "Node3D".to_owned(),
            },
            GodotAction::AddNode {
                scene: "scenes/level_02.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "Spawn".to_owned(),
                type_: "Marker3D".to_owned(),
                properties: Vec::new(),
                groups: Vec::new(),
            },
            GodotAction::InstanceScene {
                scene: "scenes/level_02.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "Crate".to_owned(),
                scene_res_path: "res://scenes/crate.tscn".to_owned(),
            },
        ]);
        let text = std::fs::read_to_string(fixture.path().join("scenes/level_02.tscn"))
            .expect("new scene on disk");
        let scene = GodotScene::parse(&text).expect("parses");
        assert_eq!(
            scene.root().map(|node| node.name.clone()),
            Some("Level02".to_owned())
        );
        assert_eq!(scene.children("."), vec!["Spawn", "Crate"]);
        assert_eq!(
            scene.instances(),
            vec![("Crate".to_owned(), "res://scenes/crate.tscn".to_owned())]
        );
        assert_eq!(
            scene.document.header.load_steps,
            scene.document.computed_load_steps()
        );
    }

    #[test]
    fn a_failing_action_stops_the_batch_and_writes_nothing() {
        let fixture = Fixture::new("allornothing");
        let before = std::fs::read(fixture.path().join("scenes/main.tscn")).expect("scene");
        let batch = GodotActionBatch::new(
            "half good",
            vec![
                GodotAction::AddToGroup {
                    scene: "scenes/main.tscn".to_owned(),
                    path: "Player".to_owned(),
                    group: "enemy".to_owned(),
                },
                GodotAction::SetProperty {
                    scene: "scenes/main.tscn".to_owned(),
                    path: "NoSuchNode".to_owned(),
                    property: "position".to_owned(),
                    value: TscnValue::Vector3(0.0, 0.0, 0.0),
                },
            ],
        );
        let error = lower(fixture.path(), &batch).expect_err("must fail");
        assert_eq!(error.index, 1);
        assert!(error.error.hint().is_some());
        assert_eq!(
            std::fs::read(fixture.path().join("scenes/main.tscn")).expect("scene"),
            before,
            "nothing may be written when any action fails"
        );
    }

    #[test]
    fn undoing_a_batch_restores_the_bytes_exactly() {
        let fixture = Fixture::new("undo");
        let before_scene = std::fs::read(fixture.path().join("scenes/main.tscn")).expect("scene");
        let before_project = std::fs::read(fixture.path().join("project.godot")).expect("project");

        let changeset = fixture.run(vec![
            GodotAction::RemoveNode {
                scene: "scenes/main.tscn".to_owned(),
                path: "Camera3D".to_owned(),
            },
            GodotAction::WriteScript {
                path: "scripts/new.gd".to_owned(),
                source: "extends Node\n".to_owned(),
            },
            GodotAction::AddInputAction {
                name: "crouch".to_owned(),
                keycodes: vec![67],
                deadzone: Some(0.25),
            },
        ]);
        assert!(fixture.path().join("scripts/new.gd").is_file());

        apply_changeset(fixture.path(), &invert(&changeset)).expect("undo applies");
        assert_eq!(
            std::fs::read(fixture.path().join("scenes/main.tscn")).expect("scene"),
            before_scene
        );
        assert_eq!(
            std::fs::read(fixture.path().join("project.godot")).expect("project"),
            before_project
        );
        assert!(
            !fixture.path().join("scripts/new.gd").exists(),
            "a file the batch created must be gone again"
        );
    }

    #[test]
    fn names_and_paths_godot_would_refuse_are_refused_here_with_a_hint() {
        let fixture = Fixture::new("refuse");
        let cases: Vec<GodotAction> = vec![
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "bad/name".to_owned(),
                type_: "Node3D".to_owned(),
                properties: Vec::new(),
                groups: Vec::new(),
            },
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "Player".to_owned(),
                type_: "Node3D".to_owned(),
                properties: Vec::new(),
                groups: Vec::new(),
            },
            GodotAction::AddNode {
                scene: "scenes/main.tscn".to_owned(),
                parent: ".".to_owned(),
                name: "Ok".to_owned(),
                type_: "not a class".to_owned(),
                properties: Vec::new(),
                groups: Vec::new(),
            },
            GodotAction::RemoveNode {
                scene: "scenes/main.tscn".to_owned(),
                path: ".".to_owned(),
            },
            GodotAction::ReparentNode {
                scene: "scenes/main.tscn".to_owned(),
                path: "Player".to_owned(),
                new_parent: "Player/MeshInstance3D".to_owned(),
            },
            GodotAction::WriteScript {
                path: "scripts/player.txt".to_owned(),
                source: "extends Node\n".to_owned(),
            },
            GodotAction::WriteScript {
                path: "scripts/empty.gd".to_owned(),
                source: "   \n".to_owned(),
            },
            GodotAction::WriteScript {
                path: "../outside.gd".to_owned(),
                source: "extends Node\n".to_owned(),
            },
            GodotAction::CreateScene {
                path: "scenes/main.tscn".to_owned(),
                root_name: "Main".to_owned(),
                root_type: "Node3D".to_owned(),
            },
            GodotAction::AddInputAction {
                name: "fire".to_owned(),
                keycodes: Vec::new(),
                deadzone: None,
            },
            GodotAction::AddInputAction {
                name: "fire".to_owned(),
                keycodes: vec![70],
                deadzone: Some(3.0),
            },
            GodotAction::RemoveProperty {
                scene: "scenes/main.tscn".to_owned(),
                path: "Player".to_owned(),
                property: "nonexistent".to_owned(),
            },
        ];
        for action in cases {
            let label = action.to_label();
            let batch = GodotActionBatch::new("refusals", vec![action]);
            let error = lower(fixture.path(), &batch)
                .err()
                .unwrap_or_else(|| panic!("{label} must be refused"));
            assert!(error.error.hint().is_some(), "{label} needs a hint");
        }
    }

    /// The samples list is the inventory every other consumer counts from, so it has to be
    /// complete. specta already knows every variant because `GodotAction` derives `Type`;
    /// asking it is the only check that cannot go stale when a verb is added.
    #[test]
    fn every_variant_has_a_sample() {
        // `definition` registers the named type into the collection and hands back a
        // reference to it, so the enum body is read from the collection rather than the
        // return value.
        let mut types = specta::Types::default();
        let _reference = <GodotAction as specta::Type>::definition(&mut types);
        let body = types
            .into_unsorted_iter()
            .find(|named| named.name().as_ref() == "GodotAction")
            .map(specta::datatype::NamedDataType::ty)
            .cloned()
            .expect("specta must register GodotAction");
        let specta::datatype::DataType::Enum(body) = body else {
            panic!("GodotAction must be an enum to specta");
        };
        let variants: std::collections::BTreeSet<String> = body
            .variants()
            .iter()
            .map(|(name, _)| name.to_string())
            .collect();
        let sampled: std::collections::BTreeSet<&str> = super::action_kinds().into_iter().collect();
        assert_eq!(
            variants.len(),
            sampled.len(),
            "GodotAction::samples() must carry every variant exactly once: specta says \
             {variants:?}, samples say {sampled:?}"
        );
        assert_eq!(
            sampled.len(),
            super::action_kinds().len(),
            "a kind is listed twice in GodotAction::samples()"
        );
    }

    /// The hint is read out of serde, so it names the wire fields and never the Rust ones
    /// (`type_` is `type` on the wire, and a `#[serde(default)]` field still appears).
    #[test]
    fn the_schema_hint_is_read_out_of_serde() {
        let hint = super::action_schema_hint("add_node").expect("add_node exists");
        assert!(hint.starts_with("add_node{"), "{hint}");
        assert!(hint.contains("type"), "{hint}");
        assert!(!hint.contains("type_"), "{hint}");
        assert!(hint.contains("groups"), "{hint}");
        assert!(!hint.contains("kind"), "the tag is not a field: {hint}");
        assert_eq!(super::action_schema_hint("teleport_node"), None);
        for kind in super::action_kinds() {
            assert!(
                super::action_schema_hint(kind).is_some(),
                "{kind} has no schema hint"
            );
        }
    }

    #[test]
    fn a_batch_that_changes_nothing_reports_no_changes() {
        let fixture = Fixture::new("noop");
        let batch = GodotActionBatch::new(
            "already true",
            vec![GodotAction::AddToGroup {
                scene: "scenes/main.tscn".to_owned(),
                path: "Player".to_owned(),
                group: "player".to_owned(),
            }],
        );
        let changeset = lower(fixture.path(), &batch).expect("lowers");
        assert!(changeset.is_empty());
        assert_eq!(changeset.outcomes.len(), 1);
    }
}
