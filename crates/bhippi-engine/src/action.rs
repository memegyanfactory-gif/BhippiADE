use crate::document::{EditorMetadata, OrganizerFolder, SceneDocument};
use crate::error::{EngineError, Result};
use crate::scaffold::{template, templates};
use crate::transaction::{EntitySpec, Op};
use bhippi_types::EntityId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;
use std::collections::BTreeMap;

/// The high-level, tool-shaped vocabulary the AI (and @-commands) use to edit the scene.
/// These are not the storage ops — `into_ops` lowers each action into a transaction of
/// `Op`s, so the agent can never bypass INV-070 (single write path).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineAction {
    /// Place a palette template (see `templates()`). `at` is the spawn position (defaults
    /// to the template's own), `name` overrides the auto-name.
    Spawn {
        template: String,
        #[serde(default)]
        at: Option<[f32; 3]>,
        #[serde(default)]
        parent: Option<EntityId>,
        #[serde(default)]
        name: Option<String>,
    },
    Delete {
        entity: EntityId,
    },
    /// Set transform fields; unset fields keep their current values.
    SetTransform {
        entity: EntityId,
        #[serde(default)]
        pos: Option<[f32; 3]>,
        #[serde(default)]
        rot: Option<[f32; 4]>,
        #[serde(default)]
        scale: Option<[f32; 3]>,
    },
    AddComponent {
        entity: EntityId,
        component: String,
        value: Value,
    },
    /// Merge `value` onto the current component payload (`from` is captured here).
    PatchComponent {
        entity: EntityId,
        component: String,
        value: Value,
    },
    RemoveComponent {
        entity: EntityId,
        component: String,
    },
    Reparent {
        entity: EntityId,
        #[serde(default)]
        parent: Option<EntityId>,
    },
    Rename {
        entity: EntityId,
        name: String,
    },
    /// Create an Outliner-only organiser folder. It owns presentation, never transforms.
    CreateOrganizerFolder {
        name: String,
        #[serde(default)]
        parent: Option<String>,
    },
    RenameOrganizerFolder {
        folder: String,
        name: String,
    },
    MoveOrganizerFolder {
        folder: String,
        #[serde(default)]
        parent: Option<String>,
    },
    /// Delete a folder by flattening its child folders and entity assignments into its
    /// parent. There is intentionally no cascading/entity-delete mode.
    DeleteOrganizerFolder {
        folder: String,
    },
    MoveEntityToOrganizerFolder {
        entity: EntityId,
        #[serde(default)]
        folder: Option<String>,
    },
    Duplicate {
        entity: EntityId,
    },
    /// Apply a weather preset: writes `settings.weather` + `settings.ambient` and retunes
    /// every directional light's intensity, all inside one transaction.
    SetWeather {
        weather: String,
    },
    /// Move an entity by a delta rather than to an absolute position — what "nudge the
    /// crate two metres left" means without the model having to read the current transform.
    Translate {
        entity: EntityId,
        by: [f32; 3],
    },
    /// Point an entity's forward axis at another entity (or a world position). Computing
    /// the quaternion is the engine's job, not the model's.
    LookAt {
        entity: EntityId,
        #[serde(default)]
        target: Option<EntityId>,
        #[serde(default)]
        at: Option<[f32; 3]>,
    },
    /// Set one field inside a component payload by dotted path (`shape.cuboid`), so a
    /// single number can be changed without restating the whole component.
    SetComponentProperty {
        entity: EntityId,
        component: String,
        path: String,
        value: Value,
    },
    /// Replace an entity's tags.
    SetTags {
        entity: EntityId,
        tags: Vec<String>,
    },
    /// Show or hide an entity (writes the `Visibility` component).
    SetVisible {
        entity: EntityId,
        visible: bool,
    },
    /// Lock an entity against viewport selection and dragging.
    SetLocked {
        entity: EntityId,
        locked: bool,
    },
    /// Assign a mesh asset to an entity, adding `MeshRenderer` when it has none.
    SetMesh {
        entity: EntityId,
        mesh: String,
    },
    /// Assign a material asset to an entity's first material slot.
    SetMaterial {
        entity: EntityId,
        material: String,
    },
    /// Bind a gameplay script to an entity (`ScriptRef`).
    AttachScript {
        entity: EntityId,
        script: String,
        #[serde(default)]
        hooks: Option<Value>,
        #[serde(default)]
        config: Option<Value>,
    },
    /// Gather entities under a fresh empty parent placed at their centroid — the Outliner
    /// "group" every level designer expects.
    GroupEntities {
        entities: Vec<EntityId>,
        #[serde(default)]
        name: Option<String>,
    },
    /// Align entities on one axis. `mode` is min, center or max.
    AlignEntities {
        entities: Vec<EntityId>,
        axis: String,
        #[serde(default)]
        mode: Option<String>,
    },
    /// Space entities evenly along one axis. With `spacing`, they are placed `spacing`
    /// apart from the first; without it, evenly between the outermost two.
    DistributeEntities {
        entities: Vec<EntityId>,
        axis: String,
        #[serde(default)]
        spacing: Option<f32>,
    },
    /// Scatter `count` copies of a template inside a box, no closer than `min_distance`.
    ///
    /// Seeded, so the same request twice builds the same level. This exists because a model
    /// asked for forty positions invents forty bad ones, and nobody can reproduce or review
    /// the result.
    ScatterEntities {
        template: String,
        count: u32,
        min: [f32; 3],
        max: [f32; 3],
        #[serde(default)]
        min_distance: Option<f32>,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        parent: Option<EntityId>,
        #[serde(default)]
        name: Option<String>,
    },
    /// A regular grid of copies, centred on `origin`.
    PlaceGrid {
        template: String,
        origin: [f32; 3],
        columns: u32,
        rows: u32,
        spacing: [f32; 2],
        #[serde(default)]
        parent: Option<EntityId>,
        #[serde(default)]
        name: Option<String>,
    },
    /// Copies evenly spaced around a circle on the ground plane.
    PlaceRing {
        template: String,
        center: [f32; 3],
        radius: f32,
        count: u32,
        #[serde(default)]
        parent: Option<EntityId>,
        #[serde(default)]
        name: Option<String>,
        /// Turn each copy to face the centre — what you want for torches and pillars.
        #[serde(default)]
        face_center: bool,
    },
    /// Copies along the inside edge of a box: the wall line of a room.
    PlacePerimeter {
        template: String,
        min: [f32; 3],
        max: [f32; 3],
        spacing: f32,
        #[serde(default)]
        parent: Option<EntityId>,
        #[serde(default)]
        name: Option<String>,
    },
    /// Copies stacked straight up from a base point.
    PlaceStack {
        template: String,
        base: [f32; 3],
        count: u32,
        spacing: f32,
        #[serde(default)]
        parent: Option<EntityId>,
        #[serde(default)]
        name: Option<String>,
    },
    /// Build four cuboid walls around bounds, with deterministic door/window cut-outs.
    RoomFromBounds {
        template: String,
        min: [f32; 3],
        max: [f32; 3],
        height: f32,
        thickness: f32,
        #[serde(default)]
        openings: Vec<EngineWallOpening>,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        parent: Option<EntityId>,
        #[serde(default)]
        name: Option<String>,
    },
    /// Build an open-ended corridor between the nearest centre-line edges of two rooms.
    CorridorBetween {
        template: String,
        from_min: [f32; 3],
        from_max: [f32; 3],
        to_min: [f32; 3],
        to_max: [f32; 3],
        width: f32,
        height: f32,
        thickness: f32,
        #[serde(default)]
        seed: Option<u64>,
        #[serde(default)]
        parent: Option<EntityId>,
        #[serde(default)]
        name: Option<String>,
    },
    /// Set scene-level settings. Unset fields keep their current values, so the editor and
    /// the AI can nudge one field without restating the rest.
    SetSceneSettings {
        #[serde(default)]
        ambient: Option<[f32; 3]>,
        #[serde(default)]
        skybox: Option<String>,
        #[serde(default)]
        weather: Option<String>,
        #[serde(default)]
        hud: Option<String>,
        #[serde(default)]
        levels: Option<Vec<String>>,
    },
}

/// Serializable opening accepted by `room_from_bounds`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineWallOpening {
    /// north, south, east or west.
    pub wall: String,
    #[serde(default)]
    pub offset: f32,
    pub width: f32,
    pub height: f32,
}

/// A group of actions the caller means as **one** change: one transaction, one journal
/// row, one undo step (ENG-111). "Build me a warehouse" is a batch; reversing it must be a
/// single Ctrl+Z, not thirty.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineActionBatch {
    /// What the user (or the Undo affordance) should call this change.
    pub label: String,
    #[serde(default)]
    pub actions: Vec<EngineAction>,
}

/// Which action in a batch failed, and why. The batch applies nothing when this is
/// returned — a half-built level is worse than none, and the model can repair from the
/// index plus the hint.
#[derive(Clone, Debug)]
pub struct BatchError {
    pub index: usize,
    pub error: EngineError,
}

impl EngineActionBatch {
    /// Lower every action into one op list.
    ///
    /// Each action is lowered against a scratch copy that already has the previous actions
    /// applied, because action N+1 routinely depends on N — spawning a crate and then
    /// moving it only works if the second action can see the first. The ops carry explicit
    /// `from` values captured at their point in the sequence, so replaying the list against
    /// the real document reproduces exactly this order or fails loudly.
    pub fn lower(&self, doc: &SceneDocument) -> std::result::Result<Vec<Op>, BatchError> {
        let mut scratch = doc.clone();
        let mut ops = Vec::new();
        for (index, action) in self.actions.iter().enumerate() {
            let lowered = action
                .into_ops(&scratch)
                .map_err(|error| BatchError { index, error })?;
            let mut staged = crate::transaction::EngineTransaction {
                id: bhippi_types::TransactionId::new(),
                label: action.to_label(),
                actor: bhippi_types::EngineActor::Agent,
                ops: lowered.clone(),
                inverse: Vec::new(),
                touched: Vec::new(),
                scene: None,
            };
            staged
                .apply(&mut scratch)
                .map_err(|error| BatchError { index, error })?;
            ops.extend(lowered);
        }
        Ok(ops)
    }

    /// The batch's own label, falling back to a count when the caller gave none.
    #[must_use]
    pub fn display_label(&self) -> String {
        if self.label.trim().is_empty() {
            format!("{} engine actions", self.actions.len())
        } else {
            self.label.clone()
        }
    }
}

impl EngineAction {
    /// The component this action touches, if any — used to attach the right schema excerpt
    /// to a rejection.
    #[must_use]
    pub fn component_name(&self) -> Option<&str> {
        match self {
            Self::AddComponent { component, .. }
            | Self::PatchComponent { component, .. }
            | Self::RemoveComponent { component, .. }
            | Self::SetComponentProperty { component, .. } => Some(component),
            Self::SetTransform { .. } | Self::Translate { .. } | Self::LookAt { .. } => {
                Some("Transform")
            }
            Self::SetVisible { .. } | Self::SetLocked { .. } => Some("Visibility"),
            Self::SetMesh { .. } | Self::SetMaterial { .. } => Some("MeshRenderer"),
            Self::AttachScript { .. } => Some("ScriptRef"),
            _ => None,
        }
    }
}

impl EngineAction {
    /// Lower the action to a transaction-able `Op` list against the live document. Reads
    /// current values for `from` fields (set-transform / patch) so that stale AI edits are
    /// rejected by the transaction layer, not blindly merged (INV-070 concurrency).
    pub fn into_ops(&self, doc: &SceneDocument) -> Result<Vec<Op>> {
        match self {
            Self::Spawn {
                template: name,
                at,
                parent,
                name: override_name,
            } => {
                let spec = template(name).ok_or_else(|| {
                    EngineError::Action(
                        format!("unknown template {name:?}"),
                        Some(format!(
                            "Available templates: {}",
                            templates()
                                .iter()
                                .map(|spec| spec.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )),
                    )
                })?;
                if let Some(parent) = parent {
                    if doc.entity(*parent).is_none() {
                        return Err(EngineError::Action(
                            format!("parent {parent} is not in the scene"),
                            Some("Reparent onto an existing entity.".to_owned()),
                        ));
                    }
                }
                let mut components = spec.components.clone();
                if let (Some(at), Some(transform)) = (
                    at,
                    components.iter_mut().find(|(name, _)| name == "Transform"),
                ) {
                    if let Some(object) = transform.1.as_object_mut() {
                        object.insert("pos".to_owned(), json!(at));
                    }
                }
                let entity_name = override_name.clone().unwrap_or_else(|| default_name(name));
                Ok(vec![Op::Spawn {
                    entity: EntitySpec {
                        id: EntityId::new(),
                        name: entity_name,
                        parent: None,
                        tags: vec![],
                        components: components.into_iter().collect(),
                    },
                    parent: *parent,
                }])
            }
            Self::Delete { entity } => {
                if doc.entity(*entity).is_none() {
                    return Err(not_in_scene(*entity));
                }
                Ok(vec![Op::Delete { entity: *entity }])
            }
            Self::SetTransform {
                entity,
                pos,
                rot,
                scale,
            } => {
                let current = current_transform(doc, *entity)?;
                let mut to = current.clone();
                if let Some(object) = to.as_object_mut() {
                    if let Some(pos) = pos {
                        object.insert("pos".to_owned(), json!(pos));
                    }
                    if let Some(rot) = rot {
                        object.insert("rot".to_owned(), json!(rot));
                    }
                    if let Some(scale) = scale {
                        object.insert("scale".to_owned(), json!(scale));
                    }
                }
                Ok(vec![Op::SetTransform {
                    entity: *entity,
                    from: current,
                    to,
                }])
            }
            Self::AddComponent {
                entity,
                component,
                value,
            } => {
                crate::schema::validate_component(component, value)?;
                if doc.entity(*entity).is_none() {
                    return Err(not_in_scene(*entity));
                }
                Ok(vec![Op::AddComponent {
                    entity: *entity,
                    component: component.clone(),
                    value: value.clone(),
                }])
            }
            Self::PatchComponent {
                entity,
                component,
                value,
            } => {
                crate::schema::validate_component(component, value)?;
                let entity_out = doc.entity(*entity).ok_or_else(|| not_in_scene(*entity))?;
                let current = entity_out
                    .components
                    .get(component)
                    .cloned()
                    .ok_or_else(|| {
                        EngineError::Action(
                            format!("{entity} has no component {component}"),
                            Some("Add it first.".to_owned()),
                        )
                    })?;
                let merged = merge_payload(&current, value)?;
                Ok(vec![Op::PatchComponent {
                    entity: *entity,
                    component: component.clone(),
                    from: current,
                    to: merged,
                }])
            }
            Self::RemoveComponent { entity, component } => {
                if doc.entity(*entity).is_none() {
                    return Err(not_in_scene(*entity));
                }
                if component == "Transform" {
                    return Err(EngineError::Action(
                        "Transform cannot be removed".to_owned(),
                        Some("Transform is mandatory.".to_owned()),
                    ));
                }
                Ok(vec![Op::RemoveComponent {
                    entity: *entity,
                    component: component.clone(),
                    had: None,
                }])
            }
            Self::Reparent { entity, parent } => {
                let entity_out = doc.entity(*entity).ok_or_else(|| not_in_scene(*entity))?;
                if let Some(parent) = parent {
                    if doc.entity(*parent).is_none() {
                        return Err(not_in_scene(*parent));
                    }
                }
                Ok(vec![Op::SetParent {
                    entity: *entity,
                    from: entity_out.parent,
                    to: *parent,
                }])
            }
            Self::Rename { entity, name } => {
                let entity_out = doc.entity(*entity).ok_or_else(|| not_in_scene(*entity))?;
                if name.trim().is_empty() {
                    return Err(EngineError::Action(
                        "name must not be empty".to_owned(),
                        Some("Give the entity a name.".to_owned()),
                    ));
                }
                Ok(vec![Op::Rename {
                    entity: *entity,
                    from: entity_out.name.clone(),
                    to: name.clone(),
                }])
            }
            Self::CreateOrganizerFolder { name, parent } => {
                validate_folder_name(name)?;
                if let Some(parent) = parent {
                    folder(doc, parent)?;
                }
                let mut editor = doc.editor.clone();
                editor.folders.push(OrganizerFolder {
                    id: format!("folder_{}", ulid::Ulid::new()),
                    name: name.trim().to_owned(),
                    parent: parent.clone(),
                });
                editor_metadata_op(doc, editor)
            }
            Self::RenameOrganizerFolder { folder: id, name } => {
                validate_folder_name(name)?;
                folder(doc, id)?;
                let mut editor = doc.editor.clone();
                if let Some(folder) = editor.folders.iter_mut().find(|folder| folder.id == *id) {
                    folder.name = name.trim().to_owned();
                }
                editor_metadata_op(doc, editor)
            }
            Self::MoveOrganizerFolder { folder: id, parent } => {
                folder(doc, id)?;
                if parent.as_deref() == Some(id.as_str()) {
                    return Err(EngineError::Action(
                        "an organiser folder cannot contain itself".to_owned(),
                        Some("Choose another folder or the Outliner root.".to_owned()),
                    ));
                }
                if let Some(parent) = parent {
                    folder(doc, parent)?;
                }
                let mut editor = doc.editor.clone();
                if let Some(folder) = editor.folders.iter_mut().find(|folder| folder.id == *id) {
                    folder.parent = parent.clone();
                }
                editor_metadata_op(doc, editor)
            }
            Self::DeleteOrganizerFolder { folder: id } => {
                let deleted = folder(doc, id)?;
                let promoted_to = deleted.parent.clone();
                let mut editor = doc.editor.clone();
                editor.folders.retain(|folder| folder.id != *id);
                for folder in &mut editor.folders {
                    if folder.parent.as_deref() == Some(id.as_str()) {
                        folder.parent = promoted_to.clone();
                    }
                }
                for assigned in editor.entity_folders.values_mut() {
                    if assigned == id {
                        if let Some(parent) = promoted_to.as_ref() {
                            *assigned = parent.clone();
                        }
                    }
                }
                if promoted_to.is_none() {
                    editor.entity_folders.retain(|_, assigned| assigned != id);
                }
                editor_metadata_op(doc, editor)
            }
            Self::MoveEntityToOrganizerFolder {
                entity,
                folder: folder_id,
            } => {
                if doc.entity(*entity).is_none() {
                    return Err(not_in_scene(*entity));
                }
                if let Some(folder_id) = folder_id {
                    folder(doc, folder_id)?;
                }
                let mut editor = doc.editor.clone();
                match folder_id {
                    Some(folder_id) => {
                        editor.entity_folders.insert(*entity, folder_id.clone());
                    }
                    None => {
                        editor.entity_folders.remove(entity);
                    }
                }
                editor_metadata_op(doc, editor)
            }
            Self::Duplicate { entity } => {
                let source = doc.entity(*entity).ok_or_else(|| not_in_scene(*entity))?;
                Ok(vec![Op::Duplicate {
                    source: *entity,
                    new_entity: EntitySpec {
                        id: EntityId::new(),
                        name: format!("{} Copy", source.name),
                        parent: None,
                        tags: vec![],
                        components: Default::default(),
                    },
                }])
            }
            Self::SetWeather { weather } => {
                let preset = crate::weather::preset(weather).ok_or_else(|| {
                    EngineError::Action(
                        format!("unknown weather preset {weather:?}"),
                        Some(format!(
                            "Valid presets: {}",
                            crate::weather::WEATHER_IDS.join(", ")
                        )),
                    )
                })?;
                let mut settings = doc.settings.clone();
                settings.weather = Some(preset.id.clone());
                settings.ambient = preset.ambient;
                let mut ops = vec![Op::SetSettings {
                    from: Box::new(doc.settings.clone()),
                    to: Box::new(settings),
                }];
                // The sun follows the sky: every directional light takes the preset's
                // intensity. This used to happen in the webview (INV-073) and therefore
                // outside undo — now it rides in the same transaction.
                for entity in &doc.entities {
                    let Some(light) = entity.components.get("Light") else {
                        continue;
                    };
                    if light.get("kind").and_then(Value::as_str) != Some("directional") {
                        continue;
                    }
                    let mut lit = light.clone();
                    if let Some(object) = lit.as_object_mut() {
                        object.insert("intensity".to_owned(), json!(preset.sun));
                    }
                    if &lit == light {
                        continue;
                    }
                    ops.push(Op::PatchComponent {
                        entity: entity.id,
                        component: "Light".to_owned(),
                        from: light.clone(),
                        to: lit,
                    });
                }
                Ok(ops)
            }
            Self::Translate { entity, by } => {
                let current = current_transform(doc, *entity)?;
                let pos = current
                    .get("pos")
                    .and_then(Value::as_array)
                    .map(|values| read_vec3(values))
                    .unwrap_or([0.0, 0.0, 0.0]);
                let mut to = current.clone();
                if let Some(object) = to.as_object_mut() {
                    object.insert(
                        "pos".to_owned(),
                        json!([pos[0] + by[0], pos[1] + by[1], pos[2] + by[2]]),
                    );
                }
                Ok(vec![Op::SetTransform {
                    entity: *entity,
                    from: current,
                    to,
                }])
            }
            Self::LookAt { entity, target, at } => {
                let current = current_transform(doc, *entity)?;
                let from_pos = current
                    .get("pos")
                    .and_then(Value::as_array)
                    .map(|values| read_vec3(values))
                    .unwrap_or([0.0, 0.0, 0.0]);
                let target_pos = match (target, at) {
                    (Some(target), _) => {
                        let value = current_transform(doc, *target)?;
                        value
                            .get("pos")
                            .and_then(Value::as_array)
                            .map(|values| read_vec3(values))
                            .unwrap_or([0.0, 0.0, 0.0])
                    }
                    (None, Some(at)) => *at,
                    (None, None) => {
                        return Err(EngineError::Action(
                            "look_at needs a target entity or an at position".to_owned(),
                            Some("Pass a target entity id, or an at position.".to_owned()),
                        ))
                    }
                };
                let rotation = look_at_quat(from_pos, target_pos).ok_or_else(|| {
                    EngineError::Action(
                        "look_at target coincides with the entity".to_owned(),
                        Some("Move one of them first.".to_owned()),
                    )
                })?;
                let mut to = current.clone();
                if let Some(object) = to.as_object_mut() {
                    object.insert("rot".to_owned(), json!(rotation));
                }
                Ok(vec![Op::SetTransform {
                    entity: *entity,
                    from: current,
                    to,
                }])
            }
            Self::SetComponentProperty {
                entity,
                component,
                path,
                value,
            } => {
                let entity_out = doc.entity(*entity).ok_or_else(|| not_in_scene(*entity))?;
                let current = entity_out
                    .components
                    .get(component)
                    .cloned()
                    .ok_or_else(|| {
                        EngineError::Action(
                            format!("{entity} has no component {component}"),
                            Some("Add it first.".to_owned()),
                        )
                    })?;
                let patch = nest_by_path(path, value.clone())?;
                crate::schema::validate_component(component, &patch)?;
                let merged = merge_payload(&current, &patch)?;
                Ok(vec![Op::PatchComponent {
                    entity: *entity,
                    component: component.clone(),
                    from: current,
                    to: merged,
                }])
            }
            Self::SetTags { entity, tags } => {
                let entity_out = doc.entity(*entity).ok_or_else(|| not_in_scene(*entity))?;
                if tags.iter().any(|tag| tag.trim().is_empty()) {
                    return Err(EngineError::Action(
                        "tags must not be empty strings".to_owned(),
                        Some("Drop the blank tag.".to_owned()),
                    ));
                }
                Ok(vec![Op::SetTags {
                    entity: *entity,
                    from: entity_out.tags.clone(),
                    to: tags.clone(),
                }])
            }
            Self::SetVisible { entity, visible } => {
                visibility_ops(doc, *entity, Some(*visible), None)
            }
            Self::SetLocked { entity, locked } => visibility_ops(doc, *entity, None, Some(*locked)),
            Self::SetMesh { entity, mesh } => component_upsert(
                doc,
                *entity,
                "MeshRenderer",
                json!({ "mesh": mesh }),
                json!({ "mesh": mesh, "materials": [], "cast_shadows": true }),
            ),
            Self::SetMaterial { entity, material } => component_upsert(
                doc,
                *entity,
                "MeshRenderer",
                json!({ "materials": [material] }),
                json!({ "mesh": "", "materials": [material], "cast_shadows": true }),
            ),
            Self::AttachScript {
                entity,
                script,
                hooks,
                config,
            } => {
                let mut payload = serde_json::Map::new();
                payload.insert("script".to_owned(), json!(script));
                if let Some(hooks) = hooks {
                    payload.insert("hooks".to_owned(), hooks.clone());
                }
                if let Some(config) = config {
                    payload.insert("config".to_owned(), config.clone());
                }
                let value = Value::Object(payload);
                component_upsert(doc, *entity, "ScriptRef", value.clone(), value)
            }
            Self::GroupEntities { entities, name } => {
                let members = check_members(doc, entities)?;
                let centre = centroid(doc, &members);
                let group = EntityId::new();
                let mut ops = vec![Op::Spawn {
                    entity: EntitySpec {
                        id: group,
                        name: name.clone().unwrap_or_else(|| "Group".to_owned()),
                        parent: None,
                        tags: vec![],
                        components: [(
                            "Transform".to_owned(),
                            json!({ "pos": centre, "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
                        )]
                        .into_iter()
                        .collect(),
                    },
                    // The group is created under the shallowest shared parent, so grouping
                    // a set of siblings does not yank them out of their branch.
                    parent: shared_parent(doc, &members),
                }];
                for member in &members {
                    let current = doc.entity(*member).and_then(|entity| entity.parent);
                    ops.push(Op::SetParent {
                        entity: *member,
                        from: current,
                        to: Some(group),
                    });
                }
                Ok(ops)
            }
            Self::AlignEntities {
                entities,
                axis,
                mode,
            } => {
                let index = axis_index(axis)?;
                let members = check_members(doc, entities)?;
                if members.len() < 2 {
                    return Err(EngineError::Action(
                        "align needs at least two entities".to_owned(),
                        Some("Select more than one.".to_owned()),
                    ));
                }
                let values: Vec<f32> = members
                    .iter()
                    .map(|id| position_of(doc, *id)[index])
                    .collect();
                let mode = mode.as_deref().unwrap_or("center");
                let target = match mode {
                    "min" => values.iter().copied().fold(f32::INFINITY, f32::min),
                    "max" => values.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                    "center" => values.iter().sum::<f32>() / values.len() as f32,
                    other => {
                        return Err(EngineError::Action(
                            format!("unknown align mode {other:?}"),
                            Some("Use min, center or max.".to_owned()),
                        ))
                    }
                };
                let mut ops = Vec::new();
                for member in &members {
                    let mut pos = position_of(doc, *member);
                    if (pos[index] - target).abs() < f32::EPSILON {
                        continue;
                    }
                    pos[index] = target;
                    ops.push(set_position_op(doc, *member, pos)?);
                }
                Ok(ops)
            }
            Self::DistributeEntities {
                entities,
                axis,
                spacing,
            } => {
                let index = axis_index(axis)?;
                let members = check_members(doc, entities)?;
                if members.len() < 2 {
                    return Err(EngineError::Action(
                        "distribute needs at least two entities".to_owned(),
                        Some("Select more than one.".to_owned()),
                    ));
                }
                // Order by current position on the axis so the result matches what the user
                // sees, not the order the ids happened to arrive in.
                let mut ordered: Vec<(EntityId, f32)> = members
                    .iter()
                    .map(|id| (*id, position_of(doc, *id)[index]))
                    .collect();
                ordered.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                let start = ordered.first().map(|(_, value)| *value).unwrap_or(0.0);
                let step = match spacing {
                    Some(spacing) => *spacing,
                    None => {
                        let end = ordered.last().map(|(_, value)| *value).unwrap_or(start);
                        (end - start) / (ordered.len() as f32 - 1.0)
                    }
                };
                let mut ops = Vec::new();
                for (rank, (id, _)) in ordered.iter().enumerate() {
                    let mut pos = position_of(doc, *id);
                    let target = step.mul_add(rank as f32, start);
                    if (pos[index] - target).abs() < f32::EPSILON {
                        continue;
                    }
                    pos[index] = target;
                    ops.push(set_position_op(doc, *id, pos)?);
                }
                Ok(ops)
            }
            Self::ScatterEntities {
                template,
                count,
                min,
                max,
                min_distance,
                seed,
                parent,
                name,
            } => {
                let bounds = crate::procedural::Bounds::new(*min, *max)?;
                let points = crate::procedural::scatter(
                    bounds,
                    *count,
                    min_distance.unwrap_or(0.0),
                    seed.unwrap_or(0),
                )?;
                place(doc, template, &points, *parent, name.as_deref(), None)
            }
            Self::PlaceGrid {
                template,
                origin,
                columns,
                rows,
                spacing,
                parent,
                name,
            } => {
                let points = crate::procedural::grid(*origin, *columns, *rows, *spacing)?;
                place(doc, template, &points, *parent, name.as_deref(), None)
            }
            Self::PlaceRing {
                template,
                center,
                radius,
                count,
                parent,
                name,
                face_center,
            } => {
                let points = crate::procedural::ring(*center, *radius, *count)?;
                let facing = if *face_center { Some(*center) } else { None };
                place(doc, template, &points, *parent, name.as_deref(), facing)
            }
            Self::PlacePerimeter {
                template,
                min,
                max,
                spacing,
                parent,
                name,
            } => {
                let bounds = crate::procedural::Bounds::new(*min, *max)?;
                let points = crate::procedural::perimeter(bounds, *spacing)?;
                place(doc, template, &points, *parent, name.as_deref(), None)
            }
            Self::PlaceStack {
                template,
                base,
                count,
                spacing,
                parent,
                name,
            } => {
                let points = crate::procedural::stack(*base, *count, *spacing)?;
                place(doc, template, &points, *parent, name.as_deref(), None)
            }
            Self::RoomFromBounds {
                template,
                min,
                max,
                height,
                thickness,
                openings,
                seed,
                parent,
                name,
            } => {
                let bounds = crate::procedural::Bounds::new(*min, *max)?;
                let openings = openings
                    .iter()
                    .map(|opening| {
                        Ok(crate::procedural::WallOpening {
                            wall: parse_room_wall(&opening.wall)?,
                            offset: opening.offset,
                            width: opening.width,
                            height: opening.height,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let placements =
                    crate::procedural::room_from_bounds(bounds, *height, *thickness, &openings)?;
                place_architecture(
                    doc,
                    template,
                    &placements,
                    *parent,
                    name.as_deref().unwrap_or("Room Wall"),
                    seed.unwrap_or(0),
                )
            }
            Self::CorridorBetween {
                template,
                from_min,
                from_max,
                to_min,
                to_max,
                width,
                height,
                thickness,
                seed,
                parent,
                name,
            } => {
                let from = crate::procedural::Bounds::new(*from_min, *from_max)?;
                let to = crate::procedural::Bounds::new(*to_min, *to_max)?;
                let layout =
                    crate::procedural::corridor_between(from, to, *width, *height, *thickness)?;
                place_architecture(
                    doc,
                    template,
                    &layout.placements,
                    *parent,
                    name.as_deref().unwrap_or("Corridor Wall"),
                    seed.unwrap_or(0),
                )
            }
            Self::SetSceneSettings {
                ambient,
                skybox,
                weather,
                hud,
                levels,
            } => {
                let mut settings = doc.settings.clone();
                if let Some(ambient) = ambient {
                    settings.ambient = *ambient;
                }
                if let Some(skybox) = skybox {
                    settings.skybox = Some(skybox.clone()).filter(|value| !value.is_empty());
                }
                if let Some(weather) = weather {
                    if crate::weather::preset(weather).is_none() {
                        return Err(EngineError::Action(
                            format!("unknown weather preset {weather:?}"),
                            Some(format!(
                                "Valid presets: {}",
                                crate::weather::WEATHER_IDS.join(", ")
                            )),
                        ));
                    }
                    settings.weather = Some(weather.clone());
                }
                if let Some(hud) = hud {
                    settings.hud = Some(hud.clone()).filter(|value| !value.is_empty());
                }
                if let Some(levels) = levels {
                    settings.levels = levels.clone();
                }
                if settings == doc.settings {
                    return Err(EngineError::Action(
                        "the scene settings are already in that state".to_owned(),
                        Some("Change at least one field.".to_owned()),
                    ));
                }
                Ok(vec![Op::SetSettings {
                    from: Box::new(doc.settings.clone()),
                    to: Box::new(settings),
                }])
            }
        }
    }

    /// The one-line human/agent label for the ActivityDock step.
    #[must_use]
    pub fn to_label(&self) -> String {
        match self {
            Self::Spawn { template, .. } => format!("spawn {template}"),
            Self::Delete { .. } => "delete entity".to_owned(),
            Self::SetTransform { .. } => "move/rotate/scale entity".to_owned(),
            Self::AddComponent { component, .. } => format!("add {component}"),
            Self::PatchComponent { component, .. } => format!("edit {component}"),
            Self::RemoveComponent { component, .. } => format!("remove {component}"),
            Self::Reparent { .. } => "reparent entity".to_owned(),
            Self::Rename { .. } => "rename entity".to_owned(),
            Self::CreateOrganizerFolder { name, .. } => format!("create folder {name}"),
            Self::RenameOrganizerFolder { .. } => "rename organiser folder".to_owned(),
            Self::MoveOrganizerFolder { .. } => "move organiser folder".to_owned(),
            Self::DeleteOrganizerFolder { .. } => "flatten organiser folder".to_owned(),
            Self::MoveEntityToOrganizerFolder { .. } => "move entity to folder".to_owned(),
            Self::Duplicate { .. } => "duplicate entity".to_owned(),
            Self::SetWeather { weather } => format!("set weather {weather}"),
            Self::Translate { .. } => "nudge entity".to_owned(),
            Self::LookAt { .. } => "aim entity".to_owned(),
            Self::SetComponentProperty {
                component, path, ..
            } => format!("set {component}.{path}"),
            Self::SetTags { .. } => "set tags".to_owned(),
            Self::SetVisible { visible, .. } => {
                if *visible {
                    "show entity".to_owned()
                } else {
                    "hide entity".to_owned()
                }
            }
            Self::SetLocked { locked, .. } => {
                if *locked {
                    "lock entity".to_owned()
                } else {
                    "unlock entity".to_owned()
                }
            }
            Self::SetMesh { .. } => "assign mesh".to_owned(),
            Self::SetMaterial { .. } => "assign material".to_owned(),
            Self::AttachScript { script, .. } => format!("attach {script}"),
            Self::GroupEntities { entities, .. } => format!("group {} entities", entities.len()),
            Self::AlignEntities { axis, .. } => format!("align on {axis}"),
            Self::DistributeEntities { axis, .. } => format!("distribute on {axis}"),
            Self::ScatterEntities {
                count, template, ..
            } => format!("scatter {count} {template}"),
            Self::PlaceGrid {
                columns,
                rows,
                template,
                ..
            } => format!("place {columns}x{rows} {template} grid"),
            Self::PlaceRing {
                count, template, ..
            } => format!("ring of {count} {template}"),
            Self::PlacePerimeter { template, .. } => format!("{template} around the perimeter"),
            Self::PlaceStack {
                count, template, ..
            } => format!("stack {count} {template}"),
            Self::RoomFromBounds { .. } => "build room from bounds".to_owned(),
            Self::CorridorBetween { .. } => "build corridor between rooms".to_owned(),
            Self::SetSceneSettings { .. } => "edit scene settings".to_owned(),
        }
    }
}

fn validate_folder_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(EngineError::Action(
            "organiser folder name must not be empty".to_owned(),
            Some("Give the folder a name.".to_owned()),
        ));
    }
    Ok(())
}

fn folder<'a>(doc: &'a SceneDocument, id: &str) -> Result<&'a OrganizerFolder> {
    doc.editor
        .folders
        .iter()
        .find(|folder| folder.id == id)
        .ok_or_else(|| {
            EngineError::Action(
                format!("organiser folder {id:?} is not in the scene"),
                Some("Refresh the Outliner and retry.".to_owned()),
            )
        })
}

fn editor_metadata_op(doc: &SceneDocument, editor: EditorMetadata) -> Result<Vec<Op>> {
    if editor == doc.editor {
        return Err(EngineError::Action(
            "the Outliner is already arranged that way".to_owned(),
            Some("Choose a different folder or name.".to_owned()),
        ));
    }
    let mut candidate = doc.clone();
    candidate.editor = editor.clone();
    candidate.validate()?;
    Ok(vec![Op::SetEditorMetadata {
        from: Box::new(doc.editor.clone()),
        to: Box::new(editor),
    }])
}

fn parse_room_wall(value: &str) -> Result<crate::procedural::RoomWall> {
    match value.trim().to_ascii_lowercase().as_str() {
        "north" => Ok(crate::procedural::RoomWall::North),
        "south" => Ok(crate::procedural::RoomWall::South),
        "east" => Ok(crate::procedural::RoomWall::East),
        "west" => Ok(crate::procedural::RoomWall::West),
        other => Err(EngineError::Action(
            format!("unknown room wall {other:?}"),
            Some("Use north, south, east or west.".to_owned()),
        )),
    }
}

fn place_architecture(
    doc: &SceneDocument,
    template_name: &str,
    placements: &[crate::procedural::Placement],
    parent: Option<EntityId>,
    base_name: &str,
    seed: u64,
) -> Result<Vec<Op>> {
    let spec = template(template_name).ok_or_else(|| {
        EngineError::Action(
            format!("unknown template {template_name:?}"),
            Some(format!(
                "Available templates: {}",
                templates()
                    .iter()
                    .map(|spec| spec.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        )
    })?;
    if let Some(parent) = parent {
        if doc.entity(parent).is_none() {
            return Err(not_in_scene(parent));
        }
    }
    if placements.is_empty() {
        return Err(EngineError::Action(
            "architecture produced no placements".to_owned(),
            Some("Check the bounds and openings.".to_owned()),
        ));
    }

    placements
        .iter()
        .enumerate()
        .map(|(index, placement)| {
            let mut components: BTreeMap<String, Value> =
                spec.components.clone().into_iter().collect();
            let transform = components.entry("Transform".to_owned()).or_insert_with(|| {
                json!({ "pos": [0.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] })
            });
            if let Some(object) = transform.as_object_mut() {
                let half_yaw = placement.yaw / 2.0;
                object.insert("pos".to_owned(), json!(placement.position));
                object.insert(
                    "rot".to_owned(),
                    json!([0.0, half_yaw.sin(), 0.0, half_yaw.cos()]),
                );
                object.insert("scale".to_owned(), json!(placement.scale));
            }
            Ok(Op::Spawn {
                entity: EntitySpec {
                    id: deterministic_placement_id(
                        template_name,
                        base_name,
                        seed,
                        index,
                        placement,
                    ),
                    name: format!("{base_name} {:03}", index + 1),
                    parent: None,
                    tags: vec!["architecture".to_owned()],
                    components,
                },
                parent,
            })
        })
        .collect()
}

fn deterministic_placement_id(
    template_name: &str,
    base_name: &str,
    seed: u64,
    index: usize,
    placement: &crate::procedural::Placement,
) -> EntityId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"bhippi-engine-architecture-v1\0");
    hasher.update(template_name.as_bytes());
    hasher.update(&[0]);
    hasher.update(base_name.as_bytes());
    hasher.update(&seed.to_le_bytes());
    hasher.update(&(index as u64).to_le_bytes());
    for value in placement
        .position
        .into_iter()
        .chain([placement.yaw])
        .chain(placement.scale)
    {
        hasher.update(&value.to_bits().to_le_bytes());
    }
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    EntityId::from_ulid(ulid::Ulid::from(u128::from_be_bytes(bytes)))
}

/// Spawn one copy of `template` at every point, numbering the names.
///
/// One helper for all five patterns, so a scattered crate and a gridded pillar are built
/// exactly the same way and only the point list differs. `facing` turns each copy to look at
/// a target, which is what a ring of torches around a fire needs.
fn place(
    doc: &SceneDocument,
    template_name: &str,
    points: &[[f32; 3]],
    parent: Option<EntityId>,
    name: Option<&str>,
    facing: Option<[f32; 3]>,
) -> Result<Vec<Op>> {
    let spec = template(template_name).ok_or_else(|| {
        EngineError::Action(
            format!("unknown template {template_name:?}"),
            Some(format!(
                "Available templates: {}",
                templates()
                    .iter()
                    .map(|spec| spec.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        )
    })?;
    if let Some(parent) = parent {
        if doc.entity(parent).is_none() {
            return Err(not_in_scene(parent));
        }
    }
    if points.is_empty() {
        return Err(EngineError::Action(
            "that pattern produced no positions".to_owned(),
            Some("Widen the bounds, lower the count, or reduce min_distance.".to_owned()),
        ));
    }
    let base_name = name
        .map(str::to_owned)
        .unwrap_or_else(|| default_name(template_name));
    let mut ops = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        let mut components: BTreeMap<String, Value> = spec.components.clone().into_iter().collect();
        let transform = components
            .entry("Transform".to_owned())
            .or_insert_with(|| json!({ "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }));
        if let Some(object) = transform.as_object_mut() {
            object.insert("pos".to_owned(), json!(point));
            if let Some(target) = facing {
                if let Some(rotation) = look_at_quat(*point, target) {
                    object.insert("rot".to_owned(), json!(rotation));
                }
            }
        }
        ops.push(Op::Spawn {
            entity: EntitySpec {
                // 1-based and zero-padded so the Outliner sorts them the way they read.
                id: EntityId::new(),
                name: format!("{base_name} {:03}", index + 1),
                parent: None,
                tags: vec![],
                components,
            },
            parent,
        });
    }
    Ok(ops)
}

/// Read a JSON number triple, defaulting missing/!numeric entries to zero.
fn read_vec3(values: &[Value]) -> [f32; 3] {
    let at = |index: usize| values.get(index).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    [at(0), at(1), at(2)]
}

/// An entity's world position, or the origin when it somehow has no transform.
fn position_of(doc: &SceneDocument, entity: EntityId) -> [f32; 3] {
    doc.entity(entity)
        .and_then(|entity| entity.components.get("Transform"))
        .and_then(|transform| transform.get("pos"))
        .and_then(Value::as_array)
        .map(|values| read_vec3(values))
        .unwrap_or([0.0, 0.0, 0.0])
}

/// `x` / `y` / `z` to an index, with the valid set in the hint.
fn axis_index(axis: &str) -> Result<usize> {
    match axis.trim().to_ascii_lowercase().as_str() {
        "x" => Ok(0),
        "y" => Ok(1),
        "z" => Ok(2),
        other => Err(EngineError::Action(
            format!("unknown axis {other:?}"),
            Some("Use x, y or z.".to_owned()),
        )),
    }
}

/// A `SetTransform` op that only moves `entity` to `pos`, keeping rotation and scale.
fn set_position_op(doc: &SceneDocument, entity: EntityId, pos: [f32; 3]) -> Result<Op> {
    let current = current_transform(doc, entity)?;
    let mut to = current.clone();
    if let Some(object) = to.as_object_mut() {
        object.insert("pos".to_owned(), json!(pos));
    }
    Ok(Op::SetTransform {
        entity,
        from: current,
        to,
    })
}

/// Every member must be in the scene, and the list must not be empty or contain repeats —
/// a multi-entity action that silently skips half its input is worse than one that refuses.
fn check_members(doc: &SceneDocument, entities: &[EntityId]) -> Result<Vec<EntityId>> {
    if entities.is_empty() {
        return Err(EngineError::Action(
            "no entities were given".to_owned(),
            Some("Pass at least one entity id.".to_owned()),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(entities.len());
    for entity in entities {
        if doc.entity(*entity).is_none() {
            return Err(not_in_scene(*entity));
        }
        if seen.insert(*entity) {
            out.push(*entity);
        }
    }
    Ok(out)
}

/// The mean position of a set of entities.
fn centroid(doc: &SceneDocument, members: &[EntityId]) -> [f32; 3] {
    if members.is_empty() {
        return [0.0, 0.0, 0.0];
    }
    let mut sum = [0.0f32; 3];
    for member in members {
        let pos = position_of(doc, *member);
        for axis in 0..3 {
            sum[axis] += pos[axis];
        }
    }
    let count = members.len() as f32;
    [sum[0] / count, sum[1] / count, sum[2] / count]
}

/// The parent every member already shares, or `None` when they differ (the group then
/// lands at the scene root).
fn shared_parent(doc: &SceneDocument, members: &[EntityId]) -> Option<EntityId> {
    let mut parents = members
        .iter()
        .map(|id| doc.entity(*id).and_then(|entity| entity.parent));
    let first = parents.next().flatten()?;
    if parents.all(|parent| parent == Some(first)) {
        Some(first)
    } else {
        None
    }
}

/// Add a component when the entity has none, patch it when it has one. The two payloads
/// differ because a fresh component needs its full default shape while an existing one
/// should only take the fields being changed.
fn component_upsert(
    doc: &SceneDocument,
    entity: EntityId,
    component: &str,
    patch: Value,
    fresh: Value,
) -> Result<Vec<Op>> {
    let entity_out = doc.entity(entity).ok_or_else(|| not_in_scene(entity))?;
    match entity_out.components.get(component) {
        Some(current) => {
            crate::schema::validate_component(component, &patch)?;
            let merged = merge_payload(current, &patch)?;
            Ok(vec![Op::PatchComponent {
                entity,
                component: component.to_owned(),
                from: current.clone(),
                to: merged,
            }])
        }
        None => {
            crate::schema::validate_component(component, &fresh)?;
            Ok(vec![Op::AddComponent {
                entity,
                component: component.to_owned(),
                value: fresh,
            }])
        }
    }
}

/// Write `visible` / `locked` onto the `Visibility` component, creating it when absent.
/// Absent means visible and unlocked, so only the named half is written.
fn visibility_ops(
    doc: &SceneDocument,
    entity: EntityId,
    visible: Option<bool>,
    locked: Option<bool>,
) -> Result<Vec<Op>> {
    let mut patch = serde_json::Map::new();
    if let Some(visible) = visible {
        patch.insert("visible".to_owned(), json!(visible));
    }
    if let Some(locked) = locked {
        patch.insert("locked".to_owned(), json!(locked));
    }
    let fresh = json!({
        "visible": visible.unwrap_or(true),
        "locked": locked.unwrap_or(false),
    });
    component_upsert(doc, entity, "Visibility", Value::Object(patch), fresh)
}

/// Turn `a.b.c` + value into `{"a":{"b":{"c":value}}}` so a dotted property write becomes
/// an ordinary component patch the schema can still validate.
fn nest_by_path(path: &str, value: Value) -> Result<Value> {
    let parts: Vec<&str> = path
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return Err(EngineError::Action(
            "property path must not be empty".to_owned(),
            Some("Use a field name like \"intensity\", or \"shape.cuboid\".".to_owned()),
        ));
    }
    let mut nested = value;
    for part in parts.into_iter().rev() {
        nested = json!({ part: nested });
    }
    Ok(nested)
}

/// The quaternion that turns -Z (the engine's forward axis) from `from` towards `to`, with
/// +Y up. `None` when the two points coincide, which has no defined facing.
fn look_at_quat(from: [f32; 3], to: [f32; 3]) -> Option<[f32; 4]> {
    let forward = normalise([to[0] - from[0], to[1] - from[1], to[2] - from[2]])?;
    // Looking straight up or down leaves the yaw undefined against a +Y up vector, so fall
    // back to +Z as the reference — the same trick every look-at implementation needs.
    let world_up = if forward[1].abs() > 0.999 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    // Engine forward is -Z, so the basis is built from the *backward* vector.
    let backward = [-forward[0], -forward[1], -forward[2]];
    let right = normalise(cross(world_up, backward))?;
    let up = cross(backward, right);
    quat_from_basis(right, up, backward)
}

fn normalise(v: [f32; 3]) -> Option<[f32; 3]> {
    let length = v[0].mul_add(v[0], v[1].mul_add(v[1], v[2] * v[2])).sqrt();
    if length < 1e-6 {
        return None;
    }
    Some([v[0] / length, v[1] / length, v[2] / length])
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

/// Shepperd's method: pick the largest diagonal term so the square root never divides by
/// something near zero, which is where naive conversions produce NaN quaternions.
fn quat_from_basis(x: [f32; 3], y: [f32; 3], z: [f32; 3]) -> Option<[f32; 4]> {
    let trace = x[0] + y[1] + z[2];
    let quat = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [
            (y[2] - z[1]) / s,
            (z[0] - x[2]) / s,
            (x[1] - y[0]) / s,
            0.25 * s,
        ]
    } else if x[0] > y[1] && x[0] > z[2] {
        let s = (1.0 + x[0] - y[1] - z[2]).sqrt() * 2.0;
        [
            0.25 * s,
            (y[0] + x[1]) / s,
            (z[0] + x[2]) / s,
            (y[2] - z[1]) / s,
        ]
    } else if y[1] > z[2] {
        let s = (1.0 + y[1] - x[0] - z[2]).sqrt() * 2.0;
        [
            (y[0] + x[1]) / s,
            0.25 * s,
            (z[1] + y[2]) / s,
            (z[0] - x[2]) / s,
        ]
    } else {
        let s = (1.0 + z[2] - x[0] - y[1]).sqrt() * 2.0;
        [
            (z[0] + x[2]) / s,
            (z[1] + y[2]) / s,
            0.25 * s,
            (x[1] - y[0]) / s,
        ]
    };
    if quat.iter().any(|value| !value.is_finite()) {
        return None;
    }
    Some(quat)
}

fn current_transform(doc: &SceneDocument, entity: EntityId) -> Result<Value> {
    let entity_out = doc.entity(entity).ok_or_else(|| not_in_scene(entity))?;
    let current = entity_out
        .components
        .get("Transform")
        .cloned()
        .ok_or_else(|| {
            EngineError::Action(
                format!("{entity} has no Transform"),
                Some("Every entity has one; the scene may be corrupt.".to_owned()),
            )
        })?;
    Ok(current)
}

/// Deep-merge `patch` into `current` (objects merge; scalars/arrays replace).
fn merge_payload(current: &Value, patch: &Value) -> Result<Value> {
    match (current, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            let mut out = base.clone();
            for (key, value) in patch {
                match out.get(key) {
                    Some(existing) => out.insert(key.clone(), merge_payload(existing, value)?),
                    None => out.insert(key.clone(), value.clone()),
                };
            }
            Ok(Value::Object(out))
        }
        (_, other) => Ok(other.clone()),
    }
}

fn default_name(template: &str) -> String {
    let mut out = String::new();
    let mut capitalize = true;
    for character in template.chars() {
        if character == '_' || character == '-' {
            capitalize = true;
            if out.is_empty() {
                out.push(' ');
            }
            out.push(' ');
        } else {
            if capitalize {
                out.extend(character.to_uppercase());
                capitalize = false;
            } else {
                out.push(character);
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn not_in_scene(id: EntityId) -> EngineError {
    EngineError::Action(
        format!("entity {id} is not in the scene"),
        Some("Refresh the hierarchy and retry.".to_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::{EngineAction, EngineWallOpening};
    use crate::document::{Entity, SceneDocument};
    use crate::transaction::EngineTransaction;
    use bhippi_types::{EngineActor, EntityId, TransactionId};
    use serde_json::json;

    fn doc_with_entity() -> SceneDocument {
        let mut doc = SceneDocument::empty("level_01");
        doc.entities.push(Entity {
            id: EntityId::new(),
            name: "Crate".to_owned(),
            parent: None,
            tags: vec![],
            components: std::collections::BTreeMap::from([(
                "Transform".to_owned(),
                json!({ "pos": [0.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
            )]),
        });
        doc
    }

    fn transaction(ops: Vec<crate::transaction::Op>) -> EngineTransaction {
        EngineTransaction {
            id: TransactionId::new(),
            label: "ai action".to_owned(),
            actor: EngineActor::Agent,
            ops,
            inverse: vec![],
            touched: vec![],
            scene: None,
        }
    }

    #[test]
    fn spawn_action_lowers_to_a_valid_transaction() {
        let mut doc = doc_with_entity();
        let action = EngineAction::Spawn {
            template: "cube".to_owned(),
            at: Some([1.0, 0.0, 0.0]),
            parent: None,
            name: None,
        };
        let ops = action.into_ops(&doc).expect("lowers");
        let mut txn = transaction(ops);
        txn.apply(&mut doc).expect("applies");
        assert_eq!(doc.entity_count(), 2);
        let spawned = doc
            .entities
            .iter()
            .find(|entity| entity.name == "Cube")
            .expect("spawned named Cube");
        assert_eq!(
            spawned.components.get("Transform").expect("t")["pos"][0],
            1.0
        );
    }

    #[test]
    fn unknown_templates_are_rejected_with_the_palette() {
        let doc = doc_with_entity();
        let action = EngineAction::Spawn {
            template: "flying-saucer".to_owned(),
            at: None,
            parent: None,
            name: None,
        };
        let error = action.into_ops(&doc).expect_err("unknown template");
        assert!(error.hint().is_some());
    }

    #[test]
    fn patch_merges_onto_current_payload_and_rejects_stale() {
        let mut doc = doc_with_entity();
        let id = doc.entities[0].id;
        let action = EngineAction::PatchComponent {
            entity: id,
            component: "Transform".to_owned(),
            value: json!({ "pos": [3.0, 1.0, 0.0] }),
        };
        let ops = action.into_ops(&doc).expect("lowers");
        let mut txn = transaction(ops);
        txn.apply(&mut doc).expect("applies");
        let transform = doc
            .entity(id)
            .expect("c")
            .components
            .get("Transform")
            .expect("t");
        assert_eq!(transform["pos"][0], 3.0);
        assert_eq!(transform["pos"][1], 1.0);
        assert_eq!(transform["scale"][0], 1.0, "unset field kept");
    }

    #[test]
    fn rename_action_rejects_empty_names() {
        let doc = doc_with_entity();
        let action = EngineAction::Rename {
            entity: doc.entities[0].id,
            name: "   ".to_owned(),
        };
        assert!(action.into_ops(&doc).is_err());
    }

    #[test]
    fn weather_writes_settings_and_retunes_directional_lights_in_one_transaction() {
        let mut doc = doc_with_entity();
        let sun = EntityId::new();
        doc.entities.push(Entity {
            id: sun,
            name: "Sun".to_owned(),
            parent: None,
            tags: vec![],
            components: std::collections::BTreeMap::from([
                (
                    "Transform".to_owned(),
                    json!({ "pos": [0.0, 8.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
                ),
                (
                    "Light".to_owned(),
                    json!({ "kind": "directional", "color": [1.0, 1.0, 1.0], "intensity": 2.4 }),
                ),
            ]),
        });
        let before = doc.settings.clone();

        let ops = EngineAction::SetWeather {
            weather: "storm".to_owned(),
        }
        .into_ops(&doc)
        .expect("lowers");
        let mut txn = transaction(ops);
        txn.apply(&mut doc).expect("applies");

        assert_eq!(doc.settings.weather.as_deref(), Some("storm"));
        let storm = crate::weather::preset("storm").expect("preset");
        assert_eq!(doc.settings.ambient, storm.ambient);
        let intensity = doc.entity(sun).expect("sun").components["Light"]["intensity"]
            .as_f64()
            .expect("number");
        assert!((intensity - f64::from(storm.sun)).abs() < 1e-6);

        // One transaction — so one undo puts the sky and the sun back together.
        let mut stack = crate::transaction::UndoStack::new();
        stack.push(txn);
        stack.undo(&mut doc).expect("undo");
        assert_eq!(doc.settings, before);
        let restored = doc.entity(sun).expect("sun").components["Light"]["intensity"]
            .as_f64()
            .expect("number");
        assert!((restored - 2.4).abs() < 1e-6);
    }

    #[test]
    fn unknown_weather_is_rejected_with_the_preset_list() {
        let doc = doc_with_entity();
        let error = EngineAction::SetWeather {
            weather: "hurricane".to_owned(),
        }
        .into_ops(&doc)
        .expect_err("unknown preset");
        assert!(error.hint().is_some_and(|hint| hint.contains("overcast")));
    }

    #[test]
    fn scene_settings_merge_unset_fields_and_reject_a_no_op() {
        let mut doc = doc_with_entity();
        let ambient_before = doc.settings.ambient;
        let ops = EngineAction::SetSceneSettings {
            ambient: None,
            skybox: None,
            weather: Some("night".to_owned()),
            hud: Some("assets/scenes/hud.bscn.json".to_owned()),
            levels: None,
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(ops).apply(&mut doc).expect("applies");
        assert_eq!(doc.settings.weather.as_deref(), Some("night"));
        assert_eq!(
            doc.settings.hud.as_deref(),
            Some("assets/scenes/hud.bscn.json")
        );
        // ambient was not named, so this action must leave it exactly as it was — unlike
        // SetWeather, which deliberately writes the preset's ambient.
        assert_eq!(doc.settings.ambient, ambient_before);

        let error = EngineAction::SetSceneSettings {
            ambient: None,
            skybox: None,
            weather: Some("night".to_owned()),
            hud: Some("assets/scenes/hud.bscn.json".to_owned()),
            levels: None,
        }
        .into_ops(&doc)
        .expect_err("already in that state");
        assert!(error.hint().is_some());
    }

    #[test]
    fn a_stale_settings_write_is_rejected_not_merged() {
        let mut doc = doc_with_entity();
        let stale = EngineAction::SetWeather {
            weather: "rain".to_owned(),
        }
        .into_ops(&doc)
        .expect("lowers");
        // Someone else changes the settings first.
        let winner = EngineAction::SetWeather {
            weather: "snow".to_owned(),
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(winner).apply(&mut doc).expect("applies");

        let error = transaction(stale)
            .apply(&mut doc)
            .expect_err("stale settings must not merge");
        assert!(error.hint().is_some());
        assert_eq!(doc.settings.weather.as_deref(), Some("snow"));
    }

    /// A scene with `count` props laid out on X at the given positions.
    fn doc_with_props(positions: &[[f32; 3]]) -> (SceneDocument, Vec<EntityId>) {
        let mut doc = SceneDocument::empty("level_01");
        let mut ids = Vec::new();
        for (index, pos) in positions.iter().enumerate() {
            let id = EntityId::new();
            ids.push(id);
            doc.entities.push(Entity {
                id,
                name: format!("Prop{index}"),
                parent: None,
                tags: vec![],
                components: std::collections::BTreeMap::from([(
                    "Transform".to_owned(),
                    json!({ "pos": pos, "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
                )]),
            });
        }
        (doc, ids)
    }

    fn pos_of(doc: &SceneDocument, id: EntityId) -> [f32; 3] {
        let transform = &doc.entity(id).expect("entity").components["Transform"]["pos"];
        [
            transform[0].as_f64().unwrap_or(0.0) as f32,
            transform[1].as_f64().unwrap_or(0.0) as f32,
            transform[2].as_f64().unwrap_or(0.0) as f32,
        ]
    }

    #[test]
    fn translate_moves_by_a_delta_without_the_model_reading_the_transform() {
        let (mut doc, ids) = doc_with_props(&[[1.0, 2.0, 3.0]]);
        let ops = EngineAction::Translate {
            entity: ids[0],
            by: [-2.0, 0.0, 0.5],
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(ops).apply(&mut doc).expect("applies");
        assert_eq!(pos_of(&doc, ids[0]), [-1.0, 2.0, 3.5]);
    }

    #[test]
    fn set_component_property_writes_one_field_by_dotted_path() {
        let mut doc = doc_with_entity();
        let id = doc.entities[0].id;
        transaction(
            EngineAction::AddComponent {
                entity: id,
                component: "Light".to_owned(),
                value: json!({ "kind": "point", "intensity": 2.0, "range": 10.0 }),
            }
            .into_ops(&doc)
            .expect("lowers"),
        )
        .apply(&mut doc)
        .expect("applies");

        let ops = EngineAction::SetComponentProperty {
            entity: id,
            component: "Light".to_owned(),
            path: "intensity".to_owned(),
            value: json!(0.5),
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(ops).apply(&mut doc).expect("applies");

        let light = &doc.entity(id).expect("entity").components["Light"];
        assert_eq!(light["intensity"], 0.5);
        assert_eq!(light["range"], 10.0, "the other fields are untouched");
        assert_eq!(light["kind"], "point");
    }

    #[test]
    fn a_dotted_property_still_goes_through_schema_validation() {
        let mut doc = doc_with_entity();
        let id = doc.entities[0].id;
        transaction(
            EngineAction::AddComponent {
                entity: id,
                component: "RigidBody".to_owned(),
                value: json!({ "kind": "dynamic", "mass": 1.0, "lock_rotation": false }),
            }
            .into_ops(&doc)
            .expect("lowers"),
        )
        .apply(&mut doc)
        .expect("applies");

        let error = EngineAction::SetComponentProperty {
            entity: id,
            component: "RigidBody".to_owned(),
            path: "kind".to_owned(),
            value: json!("bouncy"),
        }
        .into_ops(&doc)
        .expect_err("an invalid enum must not reach the document");
        assert!(error.hint().is_some_and(|hint| hint.contains("kinematic")));
    }

    #[test]
    fn visibility_is_created_then_patched_and_undoes_cleanly() {
        let (mut doc, ids) = doc_with_props(&[[0.0, 0.0, 0.0]]);
        let mut stack = crate::transaction::UndoStack::new();

        let ops = EngineAction::SetVisible {
            entity: ids[0],
            visible: false,
        }
        .into_ops(&doc)
        .expect("lowers");
        let mut first = transaction(ops);
        first.apply(&mut doc).expect("applies");
        stack.push(first);
        assert_eq!(
            doc.entity(ids[0]).expect("e").components["Visibility"]["visible"],
            false
        );

        // A second write patches rather than replacing, so locking keeps it hidden.
        let ops = EngineAction::SetLocked {
            entity: ids[0],
            locked: true,
        }
        .into_ops(&doc)
        .expect("lowers");
        let mut second = transaction(ops);
        second.apply(&mut doc).expect("applies");
        stack.push(second);
        let visibility = &doc.entity(ids[0]).expect("e").components["Visibility"];
        assert_eq!(visibility["visible"], false);
        assert_eq!(visibility["locked"], true);

        stack.undo(&mut doc).expect("undo lock");
        assert_eq!(
            doc.entity(ids[0]).expect("e").components["Visibility"]["locked"],
            false
        );
        stack.undo(&mut doc).expect("undo hide");
        assert!(
            !doc.entity(ids[0]).expect("e").has_component("Visibility"),
            "undoing the first write removes the component it created"
        );
    }

    #[test]
    fn align_moves_only_the_named_axis() {
        let (mut doc, ids) = doc_with_props(&[[0.0, 1.0, 0.0], [4.0, 5.0, 2.0], [8.0, 9.0, -3.0]]);
        let ops = EngineAction::AlignEntities {
            entities: ids.clone(),
            axis: "y".to_owned(),
            mode: Some("min".to_owned()),
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(ops).apply(&mut doc).expect("applies");

        for id in &ids {
            assert_eq!(pos_of(&doc, *id)[1], 1.0, "aligned to the minimum");
        }
        assert_eq!(pos_of(&doc, ids[2])[0], 8.0, "x untouched");
        assert_eq!(pos_of(&doc, ids[2])[2], -3.0, "z untouched");
    }

    #[test]
    fn distribute_spaces_by_current_order_not_argument_order() {
        // Deliberately hand them over out of order: the result must follow the scene.
        let (mut doc, ids) = doc_with_props(&[[9.0, 0.0, 0.0], [0.0, 0.0, 0.0], [4.0, 0.0, 0.0]]);
        let ops = EngineAction::DistributeEntities {
            entities: vec![ids[0], ids[1], ids[2]],
            axis: "x".to_owned(),
            spacing: Some(3.0),
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(ops).apply(&mut doc).expect("applies");

        assert_eq!(pos_of(&doc, ids[1])[0], 0.0, "leftmost stays put");
        assert_eq!(pos_of(&doc, ids[2])[0], 3.0);
        assert_eq!(pos_of(&doc, ids[0])[0], 6.0);
    }

    #[test]
    fn distribute_without_spacing_spreads_evenly_between_the_outermost() {
        let (mut doc, ids) = doc_with_props(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [10.0, 0.0, 0.0]]);
        let ops = EngineAction::DistributeEntities {
            entities: ids.clone(),
            axis: "x".to_owned(),
            spacing: None,
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(ops).apply(&mut doc).expect("applies");
        assert_eq!(pos_of(&doc, ids[0])[0], 0.0);
        assert_eq!(pos_of(&doc, ids[1])[0], 5.0);
        assert_eq!(pos_of(&doc, ids[2])[0], 10.0);
    }

    #[test]
    fn group_parents_members_under_a_new_node_at_their_centroid() {
        let (mut doc, ids) = doc_with_props(&[[0.0, 0.0, 0.0], [4.0, 2.0, 0.0]]);
        let ops = EngineAction::GroupEntities {
            entities: ids.clone(),
            name: Some("Crates".to_owned()),
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(ops).apply(&mut doc).expect("applies");

        let group = doc
            .entities
            .iter()
            .find(|entity| entity.name == "Crates")
            .expect("group exists");
        assert_eq!(pos_of(&doc, group.id), [2.0, 1.0, 0.0]);
        for id in &ids {
            assert_eq!(doc.entity(*id).expect("member").parent, Some(group.id));
        }
        doc.validate().expect("grouping leaves a valid hierarchy");
    }

    #[test]
    fn multi_entity_actions_refuse_rather_than_silently_skip_a_missing_id() {
        let (doc, ids) = doc_with_props(&[[0.0, 0.0, 0.0]]);
        let error = EngineAction::AlignEntities {
            entities: vec![ids[0], EntityId::new()],
            axis: "x".to_owned(),
            mode: None,
        }
        .into_ops(&doc)
        .expect_err("a missing member is an error, not a skip");
        assert!(error.hint().is_some());
    }

    #[test]
    fn look_at_produces_a_unit_quaternion_that_faces_the_target() {
        let (mut doc, ids) = doc_with_props(&[[0.0, 0.0, 0.0], [0.0, 0.0, -10.0]]);
        let ops = EngineAction::LookAt {
            entity: ids[0],
            target: Some(ids[1]),
            at: None,
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(ops).apply(&mut doc).expect("applies");

        let rot = &doc.entity(ids[0]).expect("e").components["Transform"]["rot"];
        let q: Vec<f32> = (0..4)
            .map(|index| rot[index].as_f64().unwrap_or(0.0) as f32)
            .collect();
        let length = q.iter().map(|value| value * value).sum::<f32>().sqrt();
        assert!((length - 1.0).abs() < 1e-4, "unit quaternion, got {length}");
        // Already facing -Z, so this is (close to) the identity rotation.
        assert!(q[3].abs() > 0.999, "expected identity-ish, got {q:?}");
    }

    #[test]
    fn look_at_straight_up_does_not_produce_nan() {
        let (mut doc, ids) = doc_with_props(&[[0.0, 0.0, 0.0], [0.0, 10.0, 0.0]]);
        let ops = EngineAction::LookAt {
            entity: ids[0],
            target: Some(ids[1]),
            at: None,
        }
        .into_ops(&doc)
        .expect("the degenerate up-vector case must still resolve");
        transaction(ops).apply(&mut doc).expect("applies");
        let rot = &doc.entity(ids[0]).expect("e").components["Transform"]["rot"];
        for index in 0..4 {
            let value = rot[index].as_f64().expect("number");
            assert!(
                value.is_finite(),
                "quaternion component {index} is not finite"
            );
        }
    }

    #[test]
    fn look_at_at_the_same_point_is_refused_not_guessed() {
        let (doc, ids) = doc_with_props(&[[2.0, 2.0, 2.0]]);
        let error = EngineAction::LookAt {
            entity: ids[0],
            target: None,
            at: Some([2.0, 2.0, 2.0]),
        }
        .into_ops(&doc)
        .expect_err("no defined facing");
        assert!(error.hint().is_some());
    }

    #[test]
    fn set_mesh_adds_the_renderer_then_patches_it() {
        let (mut doc, ids) = doc_with_props(&[[0.0, 0.0, 0.0]]);
        let ops = EngineAction::SetMesh {
            entity: ids[0],
            mesh: "asset:01JD0000000000000000000000".to_owned(),
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(ops).apply(&mut doc).expect("applies");
        let renderer = &doc.entity(ids[0]).expect("e").components["MeshRenderer"];
        assert_eq!(renderer["mesh"], "asset:01JD0000000000000000000000");
        assert_eq!(renderer["cast_shadows"], true, "defaults filled on create");

        let ops = EngineAction::SetMaterial {
            entity: ids[0],
            material: "assets/materials/wood.mat.json".to_owned(),
        }
        .into_ops(&doc)
        .expect("lowers");
        transaction(ops).apply(&mut doc).expect("applies");
        let renderer = &doc.entity(ids[0]).expect("e").components["MeshRenderer"];
        assert_eq!(
            renderer["mesh"], "asset:01JD0000000000000000000000",
            "assigning a material keeps the mesh"
        );
        assert_eq!(renderer["materials"][0], "assets/materials/wood.mat.json");
    }

    #[test]
    fn set_tags_replaces_deduplicates_and_undoes() {
        let (mut doc, ids) = doc_with_props(&[[0.0, 0.0, 0.0]]);
        let mut stack = crate::transaction::UndoStack::new();
        let ops = EngineAction::SetTags {
            entity: ids[0],
            tags: vec!["gameplay".to_owned(), "prop".to_owned(), "prop".to_owned()],
        }
        .into_ops(&doc)
        .expect("lowers");
        let mut txn = transaction(ops);
        txn.apply(&mut doc).expect("applies");
        stack.push(txn);
        assert_eq!(
            doc.entity(ids[0]).expect("e").tags,
            vec!["gameplay".to_owned(), "prop".to_owned()]
        );

        stack.undo(&mut doc).expect("undo");
        assert!(doc.entity(ids[0]).expect("e").tags.is_empty());
    }

    #[test]
    fn organiser_folders_flatten_without_touching_hierarchy_or_transforms() {
        let (mut doc, ids) = doc_with_props(&[[0.0, 0.0, 0.0], [4.0, 2.0, 1.0]]);
        doc.entities[1].parent = Some(ids[0]);
        let authored = doc
            .entities
            .iter()
            .map(|entity| {
                (
                    entity.id,
                    entity.parent,
                    entity.components.get("Transform").cloned(),
                )
            })
            .collect::<Vec<_>>();

        let ops = EngineAction::CreateOrganizerFolder {
            name: "Environment".to_owned(),
            parent: None,
        }
        .into_ops(&doc)
        .expect("root folder lowers");
        transaction(ops)
            .apply(&mut doc)
            .expect("root folder applies");
        let root = doc.editor.folders[0].id.clone();

        let ops = EngineAction::CreateOrganizerFolder {
            name: "Props".to_owned(),
            parent: Some(root.clone()),
        }
        .into_ops(&doc)
        .expect("child folder lowers");
        transaction(ops)
            .apply(&mut doc)
            .expect("child folder applies");
        let props = doc.editor.folders[1].id.clone();

        let ops = EngineAction::MoveEntityToOrganizerFolder {
            entity: ids[1],
            folder: Some(props.clone()),
        }
        .into_ops(&doc)
        .expect("assignment lowers");
        transaction(ops)
            .apply(&mut doc)
            .expect("assignment applies");

        let ops = EngineAction::DeleteOrganizerFolder {
            folder: root.clone(),
        }
        .into_ops(&doc)
        .expect("flatten root lowers");
        transaction(ops)
            .apply(&mut doc)
            .expect("flatten root applies");
        assert_eq!(doc.editor.folders[0].parent, None, "child was promoted");
        assert_eq!(doc.editor.entity_folders.get(&ids[1]), Some(&props));

        let mut stack = crate::transaction::UndoStack::new();
        let ops = EngineAction::DeleteOrganizerFolder {
            folder: props.clone(),
        }
        .into_ops(&doc)
        .expect("flatten leaf lowers");
        let mut txn = transaction(ops);
        txn.apply(&mut doc).expect("flatten leaf applies");
        stack.push(txn);
        assert!(doc.editor.folders.is_empty());
        assert!(doc.editor.entity_folders.is_empty());
        assert_eq!(
            doc.entities
                .iter()
                .map(|entity| (
                    entity.id,
                    entity.parent,
                    entity.components.get("Transform").cloned(),
                ))
                .collect::<Vec<_>>(),
            authored,
            "folder arrangement is presentation-only"
        );

        stack.undo(&mut doc).expect("folder deletion undoes");
        assert_eq!(doc.editor.entity_folders.get(&ids[1]), Some(&props));
        assert_eq!(
            doc.entity(ids[1]).expect("entity remains").parent,
            Some(ids[0])
        );
    }

    #[test]
    fn organiser_folder_cycles_are_refused_before_the_transaction() {
        let mut doc = doc_with_entity();
        for (name, parent) in [("Root", None), ("Child", Some(0usize))] {
            let parent = parent.map(|index| doc.editor.folders[index].id.clone());
            let ops = EngineAction::CreateOrganizerFolder {
                name: name.to_owned(),
                parent,
            }
            .into_ops(&doc)
            .expect("folder lowers");
            transaction(ops).apply(&mut doc).expect("folder applies");
        }
        let root = doc.editor.folders[0].id.clone();
        let child = doc.editor.folders[1].id.clone();
        let error = EngineAction::MoveOrganizerFolder {
            folder: root,
            parent: Some(child),
        }
        .into_ops(&doc)
        .expect_err("cycle refused");
        assert!(error.hint().is_some());
    }

    #[test]
    fn palette_labels_are_readable() {
        assert_eq!(
            EngineAction::Delete {
                entity: EntityId::new()
            }
            .to_label(),
            "delete entity"
        );
        let spawn = EngineAction::Spawn {
            template: "trigger".to_owned(),
            at: None,
            parent: None,
            name: None,
        };
        assert!(spawn.to_label().contains("trigger"));
    }

    #[test]
    fn architectural_actions_lower_to_byte_identical_ops() {
        let doc = SceneDocument::empty("Architecture");
        let room = EngineAction::RoomFromBounds {
            template: "cube".to_owned(),
            min: [-5.0, 0.0, -4.0],
            max: [5.0, 0.0, 4.0],
            height: 3.0,
            thickness: 0.2,
            openings: vec![EngineWallOpening {
                wall: "north".to_owned(),
                offset: 0.0,
                width: 2.0,
                height: 2.2,
            }],
            seed: Some(42),
            parent: None,
            name: Some("Lab Wall".to_owned()),
        };
        let first = room.into_ops(&doc).expect("first lowering");
        let second = room.into_ops(&doc).expect("second lowering");
        assert_eq!(
            serde_json::to_vec(&first).expect("serialize"),
            serde_json::to_vec(&second).expect("serialize"),
            "same inputs must produce byte-identical operations"
        );

        let corridor = EngineAction::CorridorBetween {
            template: "cube".to_owned(),
            from_min: [-6.0, 0.0, -3.0],
            from_max: [-2.0, 0.0, 3.0],
            to_min: [4.0, 0.0, 1.0],
            to_max: [8.0, 0.0, 5.0],
            width: 2.0,
            height: 3.0,
            thickness: 0.2,
            seed: Some(9),
            parent: None,
            name: None,
        };
        assert_eq!(
            serde_json::to_vec(&corridor.into_ops(&doc).expect("first corridor"))
                .expect("serialize"),
            serde_json::to_vec(&corridor.into_ops(&doc).expect("second corridor"))
                .expect("serialize")
        );
    }

    #[test]
    fn architectural_actions_reject_invalid_geometry() {
        let doc = SceneDocument::empty("Architecture");
        let overlapping = EngineAction::CorridorBetween {
            template: "cube".to_owned(),
            from_min: [-2.0, 0.0, -2.0],
            from_max: [2.0, 0.0, 2.0],
            to_min: [1.0, 0.0, 1.0],
            to_max: [4.0, 0.0, 4.0],
            width: 2.0,
            height: 3.0,
            thickness: 0.2,
            seed: None,
            parent: None,
            name: None,
        };
        assert!(overlapping.into_ops(&doc).is_err());
    }
}
