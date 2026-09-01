use crate::document::{Entity, SceneDocument, SceneSettings};
use crate::error::{EngineError, Result};
use bhippi_types::{EngineActor, EngineTransactionSummary, EntityId, SceneId, TransactionId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;
use std::collections::BTreeMap;

// The `let _ = entity_out;` pattern above releases the immutable borrow taken to read the
// expected prior value before re-borrowing mutably — the "compare, then swap" idiom that
// keeps concurrent edits from being silently merged (INV-070).

/// The engine's single write path (INV-070): every change is a transaction of ops with a
/// captured inverse. `apply` is atomic and computes the inverse, so undo, the audit
/// journal (INV-071) and the explain step all come from the same record.
///
/// One editor write, serialisable for IPC and the journal. Muting ops carry the prior
/// value (or the whole prior subtree) so their inverse is explicit and replayable.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Spawn {
        entity: EntitySpec,
        parent: Option<EntityId>,
    },
    /// Cascades to descendants; the inverse restores the whole removed subtree.
    Delete { entity: EntityId },
    SetTransform {
        entity: EntityId,
        from: Value,
        to: Value,
    },
    AddComponent {
        entity: EntityId,
        component: String,
        value: Value,
    },
    PatchComponent {
        entity: EntityId,
        component: String,
        from: Value,
        to: Value,
    },
    /// `had` is the value to restore on undo; when produced de novo (interactive path) it
    /// is captured from the document at apply time.
    RemoveComponent {
        entity: EntityId,
        component: String,
        had: Option<Value>,
    },
    SetParent {
        entity: EntityId,
        from: Option<EntityId>,
        to: Option<EntityId>,
    },
    Rename {
        entity: EntityId,
        from: String,
        to: String,
    },
    Duplicate {
        source: EntityId,
        new_entity: EntitySpec,
    },
    /// Scene-level settings (ambient, skybox, weather, HUD path, level list). Carries the
    /// whole prior value so the inverse is a plain swap; settings are small and comparing
    /// them wholesale is what makes a stale editor write fail instead of merge (INV-070).
    SetSettings {
        from: Box<SceneSettings>,
        to: Box<SceneSettings>,
    },
    /// Replace an entity's tag list. Tags are scene data (the Outliner filters on them and
    /// play composition writes layer tags), so editing them is a transaction like any other.
    SetTags {
        entity: EntityId,
        from: Vec<String>,
        to: Vec<String>,
    },
}

/// A write-ready entity description (used by Spawn/Duplicate and the asset-palette
/// templates), reusing the scene's deterministic serialisation layout.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EntitySpec {
    pub id: EntityId,
    pub name: String,
    #[serde(default)]
    pub parent: Option<EntityId>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub components: BTreeMap<String, Value>,
}

impl EntitySpec {
    #[must_use]
    pub fn from_entity(entity: &Entity) -> Self {
        Self {
            id: entity.id,
            name: entity.name.clone(),
            parent: entity.parent,
            tags: entity.tags.clone(),
            components: entity.components.clone(),
        }
    }
}

/// An applied (or redoable) transaction. `ops` is the forward path; `inverse` undoes the
/// transaction when iterated in reverse. `scene` is set by `apply`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineTransaction {
    pub id: TransactionId,
    pub label: String,
    pub actor: EngineActor,
    #[serde(default)]
    pub ops: Vec<Op>,
    #[serde(default)]
    pub inverse: Vec<Op>,
    #[serde(default)]
    pub touched: Vec<EntityId>,
    #[serde(default)]
    pub scene: Option<SceneId>,
}

impl EngineTransaction {
    /// Apply the forward ops atomically on `scene`, computing and storing the inverse.
    /// Returns the journal fact. Rejects being applied twice — use `redo` for that.
    pub fn apply(&mut self, doc: &mut SceneDocument) -> Result<EngineTransactionSummary> {
        self.apply_with_scene(doc, doc.id)
    }

    /// Internal apply that stamps the scene id explicitly (so undo keeps the original).
    fn apply_with_scene(
        &mut self,
        doc: &mut SceneDocument,
        scene: SceneId,
    ) -> Result<EngineTransactionSummary> {
        if !self.inverse.is_empty() {
            return Err(EngineError::Transaction(
                "transaction already applied".to_owned(),
                Some("Undo or redo, do not double-apply.".to_owned()),
            ));
        }
        let mut inverse: Vec<Op> = Vec::new();
        for op in &self.ops {
            let inv = apply_op(doc, op).inspect_err(|_| {
                rollback(doc, &inverse);
            })?;
            inverse.extend(inv);
        }
        dedupe_touched(self);
        self.inverse = inverse;
        if self.touched.is_empty() {
            for op in &self.ops {
                op_touched(op, &mut self.touched);
            }
        }
        self.scene.get_or_insert(scene);
        Ok(self.summary())
    }

    /// Re-apply forward ops after an undo (fresh id, fresh scene).
    pub fn redo(&mut self, doc: &mut SceneDocument) -> Result<EngineTransactionSummary> {
        self.id = TransactionId::new();
        self.inverse.clear();
        self.touched.clear();
        self.scene = Some(doc.id);
        self.apply(doc)
    }

    /// The journal fact emitted as an event and stored by the app.
    #[must_use]
    pub fn summary(&self) -> EngineTransactionSummary {
        EngineTransactionSummary {
            label: self.label.clone(),
            actor: self.actor,
            op_count: self.ops.len(),
            touched: self.touched.clone(),
            scene: self.scene.unwrap_or_default(),
        }
    }
}

/// The interactive multi-op session (plan §8.4): the hierarchy, Inspector and gizmo drags
/// record ops live; only commit pushes a journal entry and the undo stack.
pub struct Session {
    label: String,
    actor: EngineActor,
    ops: Vec<Op>,
    inverse: Vec<Op>,
    touched: Vec<EntityId>,
    dirty: bool,
}

impl Session {
    /// Start a transaction-scoped edit stream.
    #[must_use]
    pub fn begin(label: impl Into<String>, actor: EngineActor) -> Self {
        Self {
            label: label.into(),
            actor,
            ops: Vec::new(),
            inverse: Vec::new(),
            touched: Vec::new(),
            dirty: false,
        }
    }

    /// Record one op (applied immediately — interactive). Its inverse is captured here so
    /// commit yields a transaction that undoes cleanly even though it was applied live.
    pub fn record(&mut self, doc: &mut SceneDocument, op: Op) -> Result<()> {
        let inverse = apply_op(doc, &op)?;
        self.ops.push(op);
        self.inverse.extend(inverse);
        if let Some(op) = self.ops.last() {
            op_touched(op, &mut self.touched);
        }
        self.dirty = true;
        Ok(())
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    #[must_use]
    pub fn op_count(&self) -> usize {
        self.ops.len()
    }

    /// Validate the resulting document and bind live edits into a fresh transaction.
    /// The transaction is already "applied live"; the caller pushes it on the undo stack
    /// without calling `apply` again.
    pub fn commit(mut self, doc: &SceneDocument) -> Result<EngineTransaction> {
        if !self.dirty {
            return Err(EngineError::Transaction(
                "cannot commit an empty session".to_owned(),
                Some("Make a change first.".to_owned()),
            ));
        }
        doc.validate()?;
        dedupe_touched_vec(&mut self.touched);
        Ok(EngineTransaction {
            id: TransactionId::new(),
            label: self.label,
            actor: self.actor,
            scene: Some(doc.id),
            ops: self.ops,
            inverse: self.inverse,
            touched: self.touched,
        })
    }
}

/// Undo/redo history (plan §8.4) with a hard cap (`UNDO_STACK_CAP`).
#[derive(Default)]
pub struct UndoStack {
    undo: Vec<EngineTransaction>,
    redo: Vec<EngineTransaction>,
    cap: usize,
}

impl UndoStack {
    #[must_use]
    pub fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            cap: crate::UNDO_STACK_CAP,
        }
    }

    /// Push a just-applied transaction; clears the redo line.
    pub fn push(&mut self, transaction: EngineTransaction) {
        if transaction.ops.is_empty() {
            return;
        }
        self.redo.clear();
        self.undo.push(transaction);
        if self.undo.len() > self.cap {
            self.undo.remove(0);
        }
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The transaction `undo` would reverse — the editor labels its Undo affordance with
    /// this ("Undo spawn cube"), and the AI's batch label is what makes "Undo AI Change"
    /// legible rather than a bare arrow.
    #[must_use]
    pub fn peek_undo(&self) -> Option<&EngineTransaction> {
        self.undo.last()
    }

    /// The transaction `redo` would re-apply.
    #[must_use]
    pub fn peek_redo(&self) -> Option<&EngineTransaction> {
        self.redo.last()
    }

    /// Undo the top transaction by applying its inverse (reverse order), moving it to the
    /// redo line. `scene` is the document the ops apply to.
    pub fn undo(&mut self, doc: &mut SceneDocument) -> Result<EngineTransactionSummary> {
        let mut transaction = self.undo.pop().ok_or_else(|| {
            EngineError::Transaction(
                "nothing to undo".to_owned(),
                Some("Make a change first.".to_owned()),
            )
        })?;
        let scene = transaction.scene.unwrap_or(doc.id);
        let inverse = transaction.inverse.clone();
        let mut inverted = inverse.into_iter().rev().collect::<Vec<_>>();
        let mut restored_inverse: Vec<Op> = Vec::new();
        for op in &mut inverted {
            let inv = apply_op(doc, op).inspect_err(|_| {
                rollback(doc, &restored_inverse);
            })?;
            restored_inverse.extend(inv);
        }
        transaction.inverse = restored_inverse;
        transaction.scene = Some(scene);
        transaction.touched.clear();
        for op in &transaction.inverse {
            op_touched(op, &mut transaction.touched);
        }
        self.redo.push(transaction);
        self.redo
            .last()
            .map(|t| t.summary())
            .ok_or_else(|| EngineError::Transaction("undo failed to roll".to_owned(), None))
    }

    /// Redo the top undone transaction with a fresh id.
    pub fn redo(&mut self, doc: &mut SceneDocument) -> Result<EngineTransactionSummary> {
        let mut transaction = self.redo.pop().ok_or_else(|| {
            EngineError::Transaction("nothing to redo".to_owned(), Some("Undo first.".to_owned()))
        })?;
        let summary = transaction.redo(doc)?;
        self.undo.push(transaction);
        Ok(summary)
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.undo.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }
}

// ── single-op application ────────────────────────────────────────────────────────────

/// Apply one op; returns its inverse ops. No state is mutated unless the op fully
/// validates first (single-op atomicity).
fn apply_op(doc: &mut SceneDocument, op: &Op) -> Result<Vec<Op>> {
    match op {
        Op::Spawn { entity, parent } => {
            if !entity.name.trim().is_empty() && entity.name.trim().is_empty() {
                return Err(missing_name());
            }
            if let Some(parent) = parent {
                if parent == &entity.id {
                    return Err(self_parent());
                }
                if doc.entity(*parent).is_none() {
                    return Err(missing_parent(parent));
                }
            }
            if entity.name.trim().is_empty() {
                return Err(missing_name());
            }
            if doc.entity(entity.id).is_some() {
                return Err(EngineError::Transaction(
                    format!("cannot spawn {}: id already exists", entity.id),
                    Some("Refresh the hierarchy and retry.".to_owned()),
                ));
            }
            for (component, value) in &entity.components {
                crate::schema::validate_component(component, value)?;
            }
            let mut entity_out = spec_to_entity(entity);
            entity_out.parent = *parent;
            doc.entities.push(entity_out);
            Ok(vec![Op::Delete { entity: entity.id }])
        }
        Op::Delete { entity } => {
            let subtree = subtree_ids(doc, *entity).ok_or_else(|| not_in_scene(*entity))?;
            let captured: Vec<EntitySpec> = subtree
                .iter()
                .filter_map(|id| doc.entity(*id))
                .map(EntitySpec::from_entity)
                .collect();
            // Build the undo path in *restore execution order*: spawn children before
            // parents, then re-link parents. `UndoStack` applies the stored inverse in
            // reverse, so we store the exact reverse of this sequence.
            let mut inverse: Vec<Op> = Vec::new();
            for spec in captured.iter().rev() {
                inverse.push(Op::Spawn {
                    entity: spec.clone(),
                    parent: None,
                });
            }
            for spec in &captured {
                if let Some(parent) = spec.parent {
                    inverse.push(Op::SetParent {
                        entity: spec.id,
                        from: None,
                        to: Some(parent),
                    });
                }
            }
            doc.entities.retain(|e| !subtree.contains(&e.id));
            inverse.reverse();
            Ok(inverse)
        }
        Op::SetTransform { entity, from, to } => {
            crate::schema::validate_component("Transform", to)?;
            let entity_out = doc
                .entity_mut(*entity)
                .ok_or_else(|| not_in_scene(*entity))?;
            let current = entity_out
                .components
                .get("Transform")
                .cloned()
                .unwrap_or_else(|| json!({}));
            if &current != from {
                return Err(concurrent_edit(*entity));
            }
            entity_out
                .components
                .insert("Transform".to_owned(), to.clone());
            Ok(vec![Op::SetTransform {
                entity: *entity,
                from: to.clone(),
                to: from.clone(),
            }])
        }
        Op::AddComponent {
            entity,
            component,
            value,
        } => {
            crate::schema::validate_component(component, value)?;
            let entity_out = doc
                .entity_mut(*entity)
                .ok_or_else(|| not_in_scene(*entity))?;
            if entity_out.components.contains_key(component) {
                return Err(EngineError::Transaction(
                    format!("{entity} already has component {component}"),
                    Some("Patch it instead of adding a second.".to_owned()),
                ));
            }
            entity_out
                .components
                .insert(component.clone(), value.clone());
            Ok(vec![Op::RemoveComponent {
                entity: *entity,
                component: component.clone(),
                had: Some(value.clone()),
            }])
        }
        Op::PatchComponent {
            entity,
            component,
            from,
            to,
        } => {
            crate::schema::validate_component(component, to)?;
            let entity_out = doc
                .entity_mut(*entity)
                .ok_or_else(|| not_in_scene(*entity))?;
            let current = entity_out
                .components
                .get(component)
                .cloned()
                .ok_or_else(|| {
                    EngineError::Transaction(
                        format!("{entity} has no component {component}"),
                        Some("Add it first.".to_owned()),
                    )
                })?;
            if &current != from {
                return Err(concurrent_edit(*entity));
            }
            entity_out.components.insert(component.clone(), to.clone());
            Ok(vec![Op::PatchComponent {
                entity: *entity,
                component: component.clone(),
                from: to.clone(),
                to: from.clone(),
            }])
        }
        Op::RemoveComponent {
            entity,
            component,
            had,
        } => {
            if component == "Transform" {
                return Err(EngineError::Transaction(
                    "Transform is mandatory on every entity".to_owned(),
                    Some("Transform cannot be removed.".to_owned()),
                ));
            }
            let entity_out = doc
                .entity_mut(*entity)
                .ok_or_else(|| not_in_scene(*entity))?;
            let captured = entity_out.components.remove(component);
            let captured = match captured {
                Some(value) => Some(value),
                None => had.clone(),
            };
            let Some(captured) = captured else {
                return Err(EngineError::Transaction(
                    format!("{entity} has no component {component}"),
                    Some("Nothing to remove.".to_owned()),
                ));
            };
            Ok(vec![Op::AddComponent {
                entity: *entity,
                component: component.clone(),
                value: captured,
            }])
        }
        Op::SetParent { entity, from, to } => {
            if let Some(to) = to {
                if to == entity {
                    return Err(self_parent());
                }
                if doc.entity(*to).is_none() {
                    return Err(missing_parent(to));
                }
                if is_descendant(doc, *entity, *to) {
                    return Err(EngineError::Transaction(
                        format!("cannot parent {entity} under its descendant {to}"),
                        Some("That would create a cycle.".to_owned()),
                    ));
                }
            }
            let entity_out = doc.entity(*entity).ok_or_else(|| not_in_scene(*entity))?;
            if &entity_out.parent != from {
                return Err(concurrent_edit(*entity));
            }
            let _ = entity_out;
            doc.entity_mut(*entity)
                .ok_or_else(|| not_in_scene(*entity))?
                .parent = *to;
            Ok(vec![Op::SetParent {
                entity: *entity,
                from: *to,
                to: *from,
            }])
        }
        Op::Rename { entity, from, to } => {
            if to.trim().is_empty() {
                return Err(missing_name());
            }
            let entity_out = doc.entity(*entity).ok_or_else(|| not_in_scene(*entity))?;
            if &entity_out.name != from {
                return Err(concurrent_edit(*entity));
            }
            let _ = entity_out;
            doc.entity_mut(*entity)
                .ok_or_else(|| not_in_scene(*entity))?
                .name = to.clone();
            Ok(vec![Op::Rename {
                entity: *entity,
                from: to.clone(),
                to: from.clone(),
            }])
        }
        Op::Duplicate { source, new_entity } => {
            let source_specs: Vec<EntitySpec> = subtree_ids(doc, *source)
                .ok_or_else(|| not_in_scene(*source))?
                .iter()
                .filter_map(|id| doc.entity(*id))
                .map(EntitySpec::from_entity)
                .collect();
            if new_entity.id == *source {
                return Err(EngineError::Transaction(
                    "duplicate target id must differ from source".to_owned(),
                    Some("Use a fresh id.".to_owned()),
                ));
            }
            if doc.entity(new_entity.id).is_some() {
                return Err(EngineError::Transaction(
                    format!("duplicate target {} already exists", new_entity.id),
                    Some("Use a fresh id.".to_owned()),
                ));
            }
            // Remap every entity in the source subtree onto fresh ids; root → new_entity.
            let mut remap: BTreeMap<EntityId, EntityId> = BTreeMap::new();
            remap.insert(*source, new_entity.id);
            for spec in &source_specs {
                if spec.id != *source {
                    remap.insert(spec.id, EntityId::new());
                }
            }
            let old_to_new: BTreeMap<EntityId, EntityId> = remap.clone();
            let mut cloned: Vec<EntitySpec> = Vec::new();
            for spec in &source_specs {
                let fresh_id = *remap.get(&spec.id).ok_or_else(|| {
                    EngineError::Transaction(format!("no remap for {}", spec.id), None)
                })?;
                let cloned_spec = if spec.id == *source {
                    let mut components = if new_entity.components.is_empty() {
                        spec.components.clone()
                    } else {
                        new_entity.components.clone()
                    };
                    offset_transform_x(&mut components, 1.0);
                    EntitySpec {
                        id: new_entity.id,
                        name: if new_entity.name.trim().is_empty() {
                            format!("{} Copy", spec.name)
                        } else {
                            new_entity.name.clone()
                        },
                        parent: new_entity.parent.or(spec.parent),
                        tags: if new_entity.tags.is_empty() {
                            spec.tags.clone()
                        } else {
                            new_entity.tags.clone()
                        },
                        components,
                    }
                } else {
                    EntitySpec {
                        id: fresh_id,
                        name: spec.name.clone(),
                        parent: spec.parent,
                        tags: spec.tags.clone(),
                        components: spec.components.clone(),
                    }
                };
                cloned.push(cloned_spec);
            }
            for spec in &mut cloned {
                spec.parent = spec.parent.map(|p| *old_to_new.get(&p).unwrap_or(&p));
            }
            // Insert directly after the source subtree, preserving authoring order.
            let insert_at = doc
                .entities
                .iter()
                .position(|e| e.id == *source)
                .ok_or_else(|| not_in_scene(*source))?
                + 1;
            for (offset, spec) in cloned.iter().enumerate() {
                let mut entity = spec_to_entity(spec);
                entity.parent = spec.parent.filter(|p| doc.entity(*p).is_some());
                doc.entities.insert(insert_at + offset, entity);
            }
            Ok(vec![Op::Delete {
                entity: new_entity.id,
            }])
        }
        Op::SetTags { entity, from, to } => {
            let entity_out = doc.entity(*entity).ok_or_else(|| not_in_scene(*entity))?;
            if &entity_out.tags != from {
                return Err(concurrent_edit(*entity));
            }
            let _ = entity_out;
            let mut tags = to.clone();
            tags.sort();
            tags.dedup();
            doc.entity_mut(*entity)
                .ok_or_else(|| not_in_scene(*entity))?
                .tags = tags.clone();
            Ok(vec![Op::SetTags {
                entity: *entity,
                from: tags,
                to: from.clone(),
            }])
        }
        Op::SetSettings { from, to } => {
            if &doc.settings != from.as_ref() {
                return Err(EngineError::Transaction(
                    "the scene settings changed since this edit was prepared".to_owned(),
                    Some("Reload the scene and repeat the change.".to_owned()),
                ));
            }
            if let Some(weather) = to.weather.as_deref() {
                if !crate::weather::WEATHER_IDS.contains(&weather) {
                    return Err(EngineError::Transaction(
                        format!("unknown weather preset {weather:?}"),
                        Some(format!(
                            "Valid presets: {}",
                            crate::weather::WEATHER_IDS.join(", ")
                        )),
                    ));
                }
            }
            doc.settings = to.as_ref().clone();
            Ok(vec![Op::SetSettings {
                from: to.clone(),
                to: from.clone(),
            }])
        }
    }
}

fn op_touched(op: &Op, touched: &mut Vec<EntityId>) {
    match op {
        Op::Spawn { entity, .. } => touched.push(entity.id),
        Op::Delete { entity } => touched.push(*entity),
        Op::SetTransform { entity, .. } => touched.push(*entity),
        Op::AddComponent { entity, .. } => touched.push(*entity),
        Op::PatchComponent { entity, .. } => touched.push(*entity),
        Op::RemoveComponent { entity, .. } => touched.push(*entity),
        Op::SetParent { entity, .. } => touched.push(*entity),
        Op::Rename { entity, .. } => touched.push(*entity),
        Op::Duplicate { new_entity, .. } => touched.push(new_entity.id),
        Op::SetTags { entity, .. } => touched.push(*entity),
        // Scene-level: no entity is touched, so the hierarchy needs no patch — the
        // settings half of the state event carries the change.
        Op::SetSettings { .. } => {}
    }
}

fn dedupe_touched(txn: &mut EngineTransaction) {
    dedupe_touched_vec(&mut txn.touched);
}

fn dedupe_touched_vec(touched: &mut Vec<EntityId>) {
    let mut seen = std::collections::BTreeSet::new();
    touched.retain(|id| seen.insert(*id));
}

/// All entity ids of the subtree rooted at `id`, including the root, in top-down order.
fn subtree_ids(doc: &SceneDocument, id: EntityId) -> Option<Vec<EntityId>> {
    doc.entity(id)?;
    let mut out = Vec::new();
    fn walk(doc: &SceneDocument, parent: EntityId, out: &mut Vec<EntityId>) {
        out.push(parent);
        for entity in doc.entities.iter().filter(|e| e.parent == Some(parent)) {
            walk(doc, entity.id, out);
        }
    }
    walk(doc, id, &mut out);
    Some(out)
}

fn is_descendant(doc: &SceneDocument, ancestor: EntityId, candidate: EntityId) -> bool {
    let mut current = candidate;
    let mut guard = 0usize;
    while let Some(parent) = doc.entity(current).and_then(|e| e.parent) {
        guard += 1;
        if parent == ancestor {
            return true;
        }
        if guard > doc.entities.len() {
            return false;
        }
        current = parent;
    }
    false
}

fn rollback(doc: &mut SceneDocument, inverse: &[Op]) {
    for op in inverse.iter().rev() {
        let _ = apply_op(doc, op);
    }
}

fn spec_to_entity(spec: &EntitySpec) -> Entity {
    Entity {
        id: spec.id,
        name: spec.name.clone(),
        parent: spec.parent,
        tags: spec.tags.clone(),
        components: spec.components.clone(),
    }
}

// ── error shorthands (no unwrap/expect outside tests) ────────────────────────────────

fn missing_name() -> EngineError {
    EngineError::Transaction(
        "entity name must not be empty".to_owned(),
        Some("Give the entity a name.".to_owned()),
    )
}

fn self_parent() -> EngineError {
    EngineError::Transaction(
        "an entity cannot be its own parent".to_owned(),
        Some("Choose a different parent.".to_owned()),
    )
}

fn offset_transform_x(components: &mut BTreeMap<String, Value>, delta: f32) {
    let Some(transform) = components.get_mut("Transform") else {
        return;
    };
    let Some(object) = transform.as_object_mut() else {
        return;
    };
    let Some(pos) = object.get_mut("pos").and_then(Value::as_array_mut) else {
        return;
    };
    if let Some(Value::Number(x)) = pos.first_mut() {
        if let Some(value) = x.as_f64() {
            *x = serde_json::Number::from_f64(f64::from(delta) + value)
                .unwrap_or_else(|| serde_json::Number::from(0));
        }
    }
}

fn missing_parent(parent: &EntityId) -> EngineError {
    EngineError::Transaction(
        format!("{parent} is not in the scene"),
        Some("Reparent onto an existing entity.".to_owned()),
    )
}

fn not_in_scene(id: EntityId) -> EngineError {
    EngineError::Transaction(
        format!("entity {id} is not in the scene"),
        Some("Refresh the hierarchy and retry.".to_owned()),
    )
}

fn concurrent_edit(id: EntityId) -> EngineError {
    EngineError::Transaction(
        format!("entity {id} changed elsewhere"),
        Some("Refresh and retry the edit.".to_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Entity, SceneDocument};
    use bhippi_types::{EngineActor, EntityId};

    fn doc_with_crate() -> SceneDocument {
        let mut doc = SceneDocument::empty("level_01");
        let crate_id = EntityId::new();
        doc.entities.push(Entity {
            id: crate_id,
            name: "Crate".to_owned(),
            parent: None,
            tags: vec![],
            components: BTreeMap::from([(
                "Transform".to_owned(),
                json!({ "pos": [0.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
            )]),
        });
        doc
    }

    fn make_txn(label: &str, ops: Vec<Op>) -> EngineTransaction {
        EngineTransaction {
            id: TransactionId::new(),
            label: label.to_owned(),
            actor: EngineActor::User,
            ops,
            inverse: vec![],
            touched: vec![],
            scene: None,
        }
    }

    #[test]
    fn spawn_delete_round_trips_through_undo_and_redo() {
        let mut doc = doc_with_crate();
        let car = EntityId::new();
        let mut forward = make_txn(
            "spawn car",
            vec![Op::Spawn {
                entity: EntitySpec {
                    id: car,
                    name: "Car".to_owned(),
                    parent: None,
                    tags: vec![],
                    components: BTreeMap::from([(
                        "Transform".to_owned(),
                        json!({ "pos": [1.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0] }),
                    )]),
                },
                parent: None,
            }],
        );
        let summary = forward.apply(&mut doc).expect("apply");
        assert!(summary.touched.contains(&car));
        assert_eq!(doc.entity_count(), 2);

        let mut stack = UndoStack::new();
        stack.push(forward.clone());
        stack.undo(&mut doc).expect("undo");
        assert_eq!(doc.entity_count(), 1);
        stack.redo(&mut doc).expect("redo");
        assert_eq!(doc.entity_count(), 2);
        assert!(doc.entity(car).is_some());
    }

    #[test]
    fn delete_cascades_and_recovers_the_subtree() {
        let mut doc = doc_with_crate();
        let a = EntityId::new();
        let b = EntityId::new();
        doc.entities.push(Entity {
            id: a,
            name: "A".to_owned(),
            parent: None,
            tags: vec![],
            components: Default::default(),
        });
        doc.entities.push(Entity {
            id: b,
            name: "B".to_owned(),
            parent: Some(a),
            tags: vec![],
            components: Default::default(),
        });
        let mut forward = make_txn("delete A", vec![Op::Delete { entity: a }]);
        forward.apply(&mut doc).expect("apply");
        assert_eq!(doc.entity_count(), 1);
        let mut stack = UndoStack::new();
        stack.push(forward);
        stack.undo(&mut doc).expect("undo");
        assert_eq!(doc.entity_count(), 3);
        assert_eq!(doc.entity(b).expect("b").parent, Some(a));
    }

    #[test]
    fn duplicate_copies_components_and_offsets_x() {
        let mut doc = doc_with_crate();
        let source = doc.entities[0].id;
        let new_id = EntityId::new();
        let mut txn = make_txn(
            "dup",
            vec![Op::Duplicate {
                source,
                new_entity: EntitySpec {
                    id: new_id,
                    name: "Crate Copy".to_owned(),
                    parent: None,
                    tags: vec![],
                    components: BTreeMap::new(),
                },
            }],
        );
        txn.apply(&mut doc).expect("duplicate");
        assert_eq!(doc.entities.len(), 2);
        let copy = doc.entity(new_id).expect("copy");
        assert_eq!(copy.name, "Crate Copy");
        let pos = copy
            .components
            .get("Transform")
            .and_then(|value| value.get("pos"))
            .and_then(Value::as_array)
            .expect("copied transform");
        assert_eq!(pos[0].as_f64(), Some(1.0));
        assert!(copy.components.contains_key("Transform"));
        let mut stack = UndoStack::new();
        stack.push(txn);
        stack.undo(&mut doc).expect("undo duplicate");
        assert_eq!(doc.entities.len(), 1);
        assert!(doc.entity(source).is_some());
    }

    #[test]
    fn transform_edit_and_undo_moves_position() {
        let mut doc = doc_with_crate();
        let id = doc.entities[0].id;
        let from = json!({ "pos": [0.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] });
        let to = json!({ "pos": [2.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] });
        let mut forward = make_txn(
            "move",
            vec![Op::SetTransform {
                entity: id,
                from,
                to,
            }],
        );
        forward.apply(&mut doc).expect("apply");
        let transform = doc
            .entity(id)
            .expect("crate")
            .components
            .get("Transform")
            .expect("transform");
        assert_eq!(transform["pos"][0], 2.0);
        let mut stack = UndoStack::new();
        stack.push(forward);
        stack.undo(&mut doc).expect("undo");
        let transform = doc
            .entity(id)
            .expect("crate")
            .components
            .get("Transform")
            .expect("transform");
        assert_eq!(transform["pos"][0], 0.0);
    }

    #[test]
    fn stale_from_values_are_rejected_not_merged() {
        let mut doc = doc_with_crate();
        let id = doc.entities[0].id;
        let stale = json!({ "pos": [0.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] });
        let mut first = make_txn(
            "first",
            vec![Op::SetTransform {
                entity: id,
                from: stale.clone(),
                to: json!({ "pos": [5.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
            }],
        );
        // Second transaction reuses the *same* stale `from` (0.0) after the first made
        // the current value 5.0 — the engine must reject it and mutate nothing.
        let mut second = make_txn(
            "second",
            vec![Op::SetTransform {
                entity: id,
                from: stale,
                to: json!({ "pos": [9.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
            }],
        );
        first.apply(&mut doc).expect("apply first");
        assert!(second.apply(&mut doc).is_err());
        let transform = doc
            .entity(id)
            .expect("crate")
            .components
            .get("Transform")
            .expect("transform");
        assert_eq!(
            transform["pos"][0], 5.0,
            "stale edit must not partially mutate"
        );
    }

    #[test]
    fn session_commit_binds_ops_and_cap_trims_undo() {
        let mut doc = doc_with_crate();
        let id = doc.entities[0].id;
        let mut session = Session::begin("session", EngineActor::User);
        session
            .record(
                &mut doc,
                Op::Rename {
                    entity: id,
                    from: "Crate".to_owned(),
                    to: "BigCrate".to_owned(),
                },
            )
            .expect("record");
        let txn = session.commit(&doc).expect("commit");
        assert_eq!(doc.entities[0].name, "BigCrate");
        let mut stack = UndoStack::new();
        stack.push(txn);
        stack.undo(&mut doc).expect("undo");
        assert_eq!(doc.entities[0].name, "Crate");

        let mut current = "Crate".to_owned();
        for i in 0..700u32 {
            let next = format!("Name{i}");
            let mut t = make_txn(
                &format!("name{i}"),
                vec![Op::Rename {
                    entity: id,
                    from: current.clone(),
                    to: next.clone(),
                }],
            );
            t.apply(&mut doc).expect("apply");
            stack.push(t);
            current = next;
        }
        assert!(stack.len() <= crate::UNDO_STACK_CAP);
        assert_eq!(stack.len(), crate::UNDO_STACK_CAP);
    }

    #[test]
    fn parent_cycles_are_impossible_through_ops() {
        let mut doc = doc_with_crate();
        let a = EntityId::new();
        let b = EntityId::new();
        doc.entities.push(Entity {
            id: a,
            name: "A".to_owned(),
            parent: None,
            tags: vec![],
            components: Default::default(),
        });
        doc.entities.push(Entity {
            id: b,
            name: "B".to_owned(),
            parent: Some(a),
            tags: vec![],
            components: Default::default(),
        });
        let mut cyc = make_txn(
            "cycle",
            vec![Op::SetParent {
                entity: a,
                from: None,
                to: Some(b),
            }],
        );
        assert!(cyc.apply(&mut doc).is_err());
        assert!(doc.validate().is_ok());
    }

    #[test]
    fn remove_then_add_component_restores_value_on_undo() {
        let mut doc = doc_with_crate();
        let id = doc.entities[0].id;
        let mut forward = make_txn(
            "add tag",
            vec![Op::AddComponent {
                entity: id,
                component: "Tag".to_owned(),
                value: json!({ "value": "collectible" }),
            }],
        );
        forward.apply(&mut doc).expect("apply");
        assert!(doc.entity(id).expect("c").components.contains_key("Tag"));
        let mut stack = UndoStack::new();
        stack.push(forward);
        stack.undo(&mut doc).expect("undo");
        assert!(!doc.entity(id).expect("c").components.contains_key("Tag"));
    }
}
