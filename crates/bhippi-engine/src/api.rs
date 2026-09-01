//! The engine query API (plan SEC 7.4, ADR-0027).
//!
//! A pure, deterministic, read-only facade over [`SceneDocument`] and [`AssetIndex`] that
//! answers the Scenic lookup questions the AI and the inspector share — one uniform
//! surface instead of ad-hoc component-JSON parsing. Every projection is deterministic:
//! entities come back in authoring order, assets in `BTreeMap` key order, JSON maps as
//! `BTreeMap`. Nothing here mutates the scene or touches the database.
//!
//! Each query supports a **compact** form (identity / order / scalar facts) and a **deep**
//! expansion (full component payloads and resolved asset records), selected on the
//! [`SceneQueries`] facade with [`SceneQueries::compact`] / [`SceneQueries::deep`].

use crate::asset::{AssetIndex, AssetKind, AssetRecord};
use crate::document::{Entity, SceneDocument, SceneKind, SceneSettings};
use crate::query::HierarchyEntry;
use bhippi_types::{AssetId, EntityId};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

/// How much of a scene a query should return. `Compact` is a small, stable identity/order
/// view; `Deep` adds full component payloads and resolved asset records.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Expansion {
    Compact,
    Deep,
}

impl Expansion {
    #[must_use]
    pub fn is_deep(self) -> bool {
        matches!(self, Self::Deep)
    }
}

/// Deterministic read-only scene query surface (ADR-0027). Borrows the scene and an
/// optional asset index; the asset-backed queries return identity-level facts even when no
/// index is supplied (they simply have no `record` to resolve).
#[derive(Clone, Debug)]
pub struct SceneQueries<'a> {
    scene: &'a SceneDocument,
    assets: Option<&'a AssetIndex>,
    expansion: Expansion,
}

impl<'a> SceneQueries<'a> {
    /// Query a scene with no asset index and default `compact` expansion.
    #[must_use]
    pub fn new(scene: &'a SceneDocument) -> Self {
        Self {
            scene,
            assets: None,
            expansion: Expansion::Compact,
        }
    }

    /// Query a scene together with its asset index (for resolved asset records).
    #[must_use]
    pub fn with_assets(scene: &'a SceneDocument, assets: &'a AssetIndex) -> Self {
        Self {
            scene,
            assets: Some(assets),
            expansion: Expansion::Compact,
        }
    }

    /// A copy of this facade at `compact` expansion. Deterministic regardless of which mode
    /// the caller uses; swap freely between the two on the same borrow.
    #[must_use]
    pub fn compact(&self) -> Self {
        Self {
            expansion: Expansion::Compact,
            ..*self
        }
    }

    /// A copy of this facade at `deep` expansion.
    #[must_use]
    pub fn deep(&self) -> Self {
        Self {
            expansion: Expansion::Deep,
            ..*self
        }
    }

    #[must_use]
    pub fn scene(&self) -> &'a SceneDocument {
        self.scene
    }

    #[must_use]
    pub fn assets(&self) -> Option<&'a AssetIndex> {
        self.assets
    }

    fn record(&self, id: AssetId) -> Option<AssetRecord> {
        self.assets.and_then(|index| index.get(id)).cloned()
    }

    /// Project an entity into an [`EntityView`] (compact: identity + component *names*;
    /// deep: + full component payloads).
    #[must_use]
    pub fn get_entity(&self, id: EntityId) -> Option<EntityView> {
        let entity = self.scene.entity(id)?;
        Some(self.entity_view(entity))
    }

    fn entity_view(&self, entity: &'a Entity) -> EntityView {
        let mut component_names = entity.components.keys().cloned().collect::<Vec<_>>();
        component_names.sort();
        let components = if self.expansion.is_deep() {
            Some(entity.components.clone())
        } else {
            None
        };
        EntityView {
            id: entity.id,
            name: entity.name.clone(),
            parent: entity.parent,
            tags: entity.tags.clone(),
            stable_path: self
                .scene
                .stable_path(entity.id)
                .unwrap_or_else(|| format!("scene:/<missing>#{}", entity.id)),
            component_names,
            components,
        }
    }

    fn entity_ref(&self, entity: &'a Entity) -> EntityRef {
        EntityRef {
            id: entity.id,
            name: entity.name.clone(),
            parent: entity.parent,
            stable_path: self
                .scene
                .stable_path(entity.id)
                .unwrap_or_else(|| format!("scene:/<missing>#{}", entity.id)),
        }
    }

    /// `scene.get(id)` — a deterministic scene summary. Compact: identity + counts + kind;
    /// deep: + settings and the full parent-first hierarchy.
    #[must_use]
    pub fn get(&self) -> SceneView {
        let roots = self.scene.roots();
        let hierarchy = if self.expansion.is_deep() {
            Some(crate::query::hierarchy(self.scene))
        } else {
            None
        };
        SceneView {
            id: self.scene.id,
            name: self.scene.name.clone(),
            kind: self.scene.settings.kind,
            entity_count: self.scene.entity_count(),
            root_count: roots.len(),
            settings: if self.expansion.is_deep() {
                Some(self.scene.settings.clone())
            } else {
                None
            },
            hierarchy,
        }
    }

    /// `scene.find_entities(query)` — entities matching the filter, in authoring order.
    #[must_use]
    pub fn find_entities(&self, query: &EntityQuery) -> Vec<EntityRef> {
        self.scene
            .entities
            .iter()
            .filter(|entity| query.matches(entity))
            .map(|entity| self.entity_ref(entity))
            .collect()
    }

    /// `scene.get_components(entity_id)` — component names (compact) or name→payload
    /// (deep) for one entity.
    #[must_use]
    pub fn get_components(&self, id: EntityId) -> Option<ComponentsView> {
        let entity = self.scene.entity(id)?;
        let mut names = entity.components.keys().cloned().collect::<Vec<_>>();
        names.sort();
        Some(ComponentsView {
            entity: entity.id,
            names,
            payloads: if self.expansion.is_deep() {
                Some(entity.components.clone())
            } else {
                None
            },
        })
    }

    /// `scene.get_children(entity_id)` — immediate children. Compact: ids; deep: + the
    /// child [`EntityRef`]s.
    #[must_use]
    pub fn get_children(&self, id: EntityId) -> Option<ChildrenView> {
        self.scene.entity(id)?;
        let ids = self.scene.children_of(id);
        let entries = if self.expansion.is_deep() {
            Some(
                ids.iter()
                    .filter_map(|child| self.scene.entity(*child))
                    .map(|entity| self.entity_ref(entity))
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        Some(ChildrenView {
            entity: id,
            ids,
            entries,
        })
    }

    /// `scene.get_parent(entity_id)` — the entity's immediate parent. Compact: the
    /// parent's identity; deep: + the parent's component payloads.
    #[must_use]
    pub fn get_parent(&self, id: EntityId) -> Option<ParentView> {
        let entity = self.scene.entity(id)?;
        let parent_id = entity.parent;
        let parent = parent_id
            .and_then(|pid| self.scene.entity(pid))
            .map(|p| self.entity_ref(p));
        let parent_components = if self.expansion.is_deep() {
            parent_id
                .and_then(|pid| self.scene.entity(pid))
                .map(|p| p.components.clone())
        } else {
            None
        };
        Some(ParentView {
            entity: id,
            parent,
            parent_components,
        })
    }

    /// `scene.get_scripts(entity_id)` — the entity's `ScriptRef` binding. Compact: the
    /// script asset reference; deep: + `hooks` and `config`.
    #[must_use]
    pub fn get_scripts(&self, id: EntityId) -> Option<ScriptsView> {
        let entity = self.scene.entity(id)?;
        let script = entity_sorted_str_field(entity, "ScriptRef", "script");
        let hooks = if self.expansion.is_deep() {
            entity.component_field("ScriptRef", "hooks").cloned()
        } else {
            None
        };
        let config = if self.expansion.is_deep() {
            entity.component_field("ScriptRef", "config").cloned()
        } else {
            None
        };
        Some(ScriptsView {
            entity: id,
            script,
            hooks,
            config,
        })
    }

    /// `scene.get_asset_users(asset_id)` — the entities whose components reference the
    /// asset, with the referencing component names (deep: + payloads).
    #[must_use]
    pub fn get_asset_users(&self, asset: AssetId) -> AssetUsersView {
        let mut users = Vec::new();
        for entity in &self.scene.entities {
            let references = entity_asset_refs(&entity.components, asset);
            if !references.is_empty() {
                let mut components = references.keys().cloned().collect::<Vec<_>>();
                components.sort();
                let payloads = if self.expansion.is_deep() {
                    Some(
                        references
                            .into_iter()
                            .collect::<BTreeMap<String, serde_json::Value>>(),
                    )
                } else {
                    None
                };
                users.push(AssetUser {
                    entity: entity.id,
                    name: entity.name.clone(),
                    stable_path: self.entity_ref(entity).stable_path,
                    components,
                    payloads,
                });
            }
        }
        AssetUsersView {
            asset,
            record: self.record(asset),
            users,
        }
    }

    /// `scene.get_asset_dependencies(asset_id)` — the *other* assets referenced by the same
    /// entities that reference this asset (what ships alongside it in the scene), in
    /// `AssetId` key order. See ADR-0027 for why the scene graph is the dependency source.
    #[must_use]
    pub fn get_asset_dependencies(&self, asset: AssetId) -> AssetDependenciesView {
        let mut users = BTreeSet::new();
        for entity in &self.scene.entities {
            if entity_asset_refs(&entity.components, asset).is_empty() {
                continue;
            }
            for other in entity_all_asset_refs(&entity.components).values() {
                if *other != asset {
                    users.insert(*other);
                }
            }
        }
        let dependencies = users
            .into_iter()
            .map(|other| AssetDependency {
                asset: other,
                record: self.record(other),
            })
            .collect();
        AssetDependenciesView {
            asset,
            record: self.record(asset),
            dependencies,
        }
    }

    /// `scene.get_material_graph(material_id)` — the material asset, the entities that
    /// reference it, and the texture assets that ship alongside it.
    #[must_use]
    pub fn get_material_graph(&self, material: AssetId) -> MaterialGraphView {
        let users = self.get_asset_users(material).users;
        let mut textures = BTreeSet::new();
        for entity in &self.scene.entities {
            if entity_asset_refs(&entity.components, material).is_empty() {
                continue;
            }
            for asset in entity_all_asset_refs(&entity.components).values() {
                if let Some(record) = self.assets.and_then(|index| index.get(*asset)) {
                    if record.kind == AssetKind::Texture {
                        textures.insert(*asset);
                    }
                }
            }
        }
        MaterialGraphView {
            material,
            record: self.record(material),
            users,
            textures: textures.into_iter().collect(),
        }
    }

    /// `scene.get_shader(shader_id)` — the shader asset and the mesh users that reference it.
    #[must_use]
    pub fn get_shader(&self, shader: AssetId) -> ShaderView {
        ShaderView {
            shader,
            record: self.record(shader),
            users: self.get_asset_users(shader).users,
        }
    }

    /// `scene.get_animation_graph(entity_id)` — the entity's animation clip (and the mesh it
    /// drives), plus any other assets it co-references.
    #[must_use]
    pub fn get_animation_graph(&self, id: EntityId) -> Option<AnimationGraphView> {
        let entity = self.scene.entity(id)?;
        let clip = entity
            .component_field("AnimationPlayer", "clip")
            .and_then(serde_json::Value::as_str)
            .and_then(strip_asset_ref);
        let mesh = entity
            .component_field("MeshRenderer", "mesh")
            .or_else(|| entity.component_field("SkinnedMeshRenderer", "mesh"))
            .and_then(serde_json::Value::as_str)
            .and_then(strip_asset_ref);
        let mut co_referenced = BTreeSet::new();
        for asset in entity_all_asset_refs(&entity.components).values() {
            if Some(*asset) != clip {
                co_referenced.insert(*asset);
            }
        }
        Some(AnimationGraphView {
            entity: id,
            clip,
            clip_record: clip.and_then(|asset| self.record(asset)),
            mesh,
            co_referenced: co_referenced.into_iter().collect(),
        })
    }

    /// `scene.get_physics(entity_id)` — the body/collider/character-controller projection.
    /// Compact: the scalar facts; deep: + the full `RigidBody`/`Collider`/
    /// `CharacterController` payloads under `extras`.
    #[must_use]
    pub fn get_physics(&self, id: EntityId) -> Option<PhysicsView> {
        let entity = self.scene.entity(id)?;
        let mut body_kind = None;
        let mut mass = None;
        let mut lock_rotation = None;
        let mut collider_shape = None;
        let mut sensor = None;
        let mut has_character_controller = false;
        let mut has_physics = false;
        let mut extras = BTreeMap::new();

        if let Some(rigid) = entity.components.get("RigidBody") {
            has_physics = true;
            body_kind = rigid
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            mass = rigid.get("mass").and_then(serde_json::Value::as_f64);
            lock_rotation = rigid
                .get("lock_rotation")
                .and_then(serde_json::Value::as_bool);
            if self.expansion.is_deep() {
                extras.insert("RigidBody".to_owned(), rigid.clone());
            }
        }
        if let Some(collider) = entity.components.get("Collider") {
            has_physics = true;
            collider_shape = collider
                .get("shape")
                .and_then(|shape| serde_json::to_string(shape).ok());
            sensor = collider.get("sensor").and_then(serde_json::Value::as_bool);
            if self.expansion.is_deep() {
                extras.insert("Collider".to_owned(), collider.clone());
            }
        }
        if entity.components.contains_key("CharacterController") {
            has_physics = true;
            has_character_controller = true;
            if self.expansion.is_deep() {
                if let Some(cc) = entity.components.get("CharacterController") {
                    extras.insert("CharacterController".to_owned(), cc.clone());
                }
            }
        }
        if !has_physics {
            return None;
        }
        Some(PhysicsView {
            entity: id,
            body_kind,
            mass,
            lock_rotation,
            collider_shape,
            sensor,
            has_character_controller,
            extras: if self.expansion.is_deep() {
                Some(extras)
            } else {
                None
            },
        })
    }
}

// ── filters ────────────────────────────────────────────────────────────────────────────

/// A deterministic filter for [`SceneQueries::find_entities`]. `None` fields are not
/// tested; when every provided field must match. `roots_only` restricts to root entities.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct EntityQuery {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub has_component: Option<String>,
    #[serde(default)]
    pub parent: Option<EntityId>,
    #[serde(default)]
    pub roots_only: bool,
}

impl EntityQuery {
    #[must_use]
    pub fn matches(&self, entity: &Entity) -> bool {
        if let Some(name) = &self.name {
            if entity.name != *name {
                return false;
            }
        }
        if let Some(tag) = &self.tag {
            if !entity.tags.iter().any(|existing| existing == tag) {
                return false;
            }
        }
        if let Some(component) = &self.has_component {
            if !entity.components.contains_key(component) {
                return false;
            }
        }
        if let Some(parent) = self.parent {
            if entity.parent != Some(parent) {
                return false;
            }
        }
        if self.roots_only && entity.parent.is_some() {
            return false;
        }
        true
    }
}

// ── views ──────────────────────────────────────────────────────────────────────────────

/// `scene.get(id)` summary (ADR-0027). See [`SceneQueries::get`].
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SceneView {
    pub id: bhippi_types::SceneId,
    pub name: String,
    pub kind: SceneKind,
    pub entity_count: usize,
    pub root_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<SceneSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hierarchy: Option<Vec<HierarchyEntry>>,
}

/// A single entity projection (compact + deep, see [`SceneQueries::get_entity`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EntityView {
    pub id: EntityId,
    pub name: String,
    pub parent: Option<EntityId>,
    pub tags: Vec<String>,
    pub stable_path: String,
    pub component_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<BTreeMap<String, serde_json::Value>>,
}

/// Identity-level entity reference (used by find/children/material/etc. queries).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EntityRef {
    pub id: EntityId,
    pub name: String,
    pub parent: Option<EntityId>,
    pub stable_path: String,
}

/// Component projection for one entity (see [`SceneQueries::get_components`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ComponentsView {
    pub entity: EntityId,
    pub names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payloads: Option<BTreeMap<String, serde_json::Value>>,
}

/// Immediate children of an entity (see [`SceneQueries::get_children`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ChildrenView {
    pub entity: EntityId,
    pub ids: Vec<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<EntityRef>>,
}

/// Parent of an entity (see [`SceneQueries::get_parent`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ParentView {
    pub entity: EntityId,
    pub parent: Option<EntityRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_components: Option<BTreeMap<String, serde_json::Value>>,
}

/// The `ScriptRef` binding of an entity (see [`SceneQueries::get_scripts`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ScriptsView {
    pub entity: EntityId,
    pub script: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

/// One entity that references an asset (see [`SceneQueries::get_asset_users`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetUser {
    pub entity: EntityId,
    pub name: String,
    pub stable_path: String,
    pub components: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payloads: Option<BTreeMap<String, serde_json::Value>>,
}

/// Entities that reference an asset.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetUsersView {
    pub asset: AssetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<AssetRecord>,
    pub users: Vec<AssetUser>,
}

/// One co-shipping dependency (see [`SceneQueries::get_asset_dependencies`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetDependency {
    pub asset: AssetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<AssetRecord>,
}

/// Other assets referenced by the entities that reference this asset.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetDependenciesView {
    pub asset: AssetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<AssetRecord>,
    pub dependencies: Vec<AssetDependency>,
}

/// Material users + co-shipped textures (see [`SceneQueries::get_material_graph`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialGraphView {
    pub material: AssetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<AssetRecord>,
    pub users: Vec<AssetUser>,
    pub textures: Vec<AssetId>,
}

/// Shader users (see [`SceneQueries::get_shader`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ShaderView {
    pub shader: AssetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<AssetRecord>,
    pub users: Vec<AssetUser>,
}

/// One entity's animation graph (see [`SceneQueries::get_animation_graph`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AnimationGraphView {
    pub entity: EntityId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<AssetId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_record: Option<AssetRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<AssetId>,
    pub co_referenced: Vec<AssetId>,
}

/// One entity's physics projection (see [`SceneQueries::get_physics`]; mirrors ADR-0026).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PhysicsView {
    pub entity: EntityId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mass: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_rotation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collider_shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensor: Option<bool>,
    #[serde(default)]
    pub has_character_controller: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extras: Option<BTreeMap<String, serde_json::Value>>,
}

// ── helpers ────────────────────────────────────────────────────────────────────────────

impl Entity {
    fn component_field(&self, component: &str, field: &str) -> Option<&serde_json::Value> {
        self.components
            .get(component)
            .and_then(|payload| payload.get(field))
    }
}

fn entity_sorted_str_field(entity: &Entity, component: &str, field: &str) -> Option<String> {
    entity
        .component_field(component, field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

fn strip_asset_ref(text: &str) -> Option<AssetId> {
    text.strip_prefix("asset:").and_then(|id| id.parse().ok())
}

/// Which components reference `target`, with the (single) referencing payload each.
fn entity_asset_refs(
    components: &BTreeMap<String, serde_json::Value>,
    target: AssetId,
) -> BTreeMap<String, serde_json::Value> {
    let mut out = BTreeMap::new();
    for (name, payload) in components {
        if value_contains_asset(payload, target) {
            out.insert(name.clone(), payload.clone());
        }
    }
    out
}

/// All distinct asset references across an entity's components, keyed by component name
/// (deterministic, one entry per component that references assets at all).
fn entity_all_asset_refs(
    components: &BTreeMap<String, serde_json::Value>,
) -> BTreeMap<String, AssetId> {
    let mut out = BTreeMap::new();
    for (name, payload) in components {
        if let Some(asset) = first_asset_ref(payload) {
            out.insert(name.clone(), asset);
        }
    }
    out
}

fn first_asset_ref(value: &serde_json::Value) -> Option<AssetId> {
    match value {
        serde_json::Value::String(text) => strip_asset_ref(text),
        serde_json::Value::Array(items) => items.iter().find_map(first_asset_ref),
        serde_json::Value::Object(map) => map.values().find_map(first_asset_ref),
        _ => None,
    }
}

fn value_contains_asset(value: &serde_json::Value, target: AssetId) -> bool {
    match value {
        serde_json::Value::String(text) => strip_asset_ref(text) == Some(target),
        serde_json::Value::Array(items) => {
            items.iter().any(|item| value_contains_asset(item, target))
        }
        serde_json::Value::Object(map) => map
            .values()
            .any(|value| value_contains_asset(value, target)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetIndex, AssetKind, LicenseState};

    #[test]
    fn scene_summary_is_deterministic_across_modes() {
        let doc = sample_scene();
        let compact = SceneQueries::new(&doc).get();
        let deep_q = SceneQueries::new(&doc).deep();
        let deep = deep_q.get();
        assert_eq!(compact.entity_count, 4);
        assert_eq!(compact.root_count, 1);
        assert_eq!(compact.name, "level_01");
        assert!(compact.hierarchy.is_none());
        assert!(deep.hierarchy.is_some());
        assert_eq!(deep.hierarchy.unwrap().len(), 4);
    }

    #[test]
    fn get_entity_compact_is_identity_deep_adds_components() {
        let doc = sample_scene();
        let player = doc.entities.iter().find(|e| e.name == "Player").unwrap().id;
        let compact = SceneQueries::new(&doc).get_entity(player).unwrap();
        assert!(compact.components.is_none());
        assert!(compact.component_names.contains(&"Transform".to_owned()));
        assert!(compact.stable_path.contains("Player"));

        let deep_q = SceneQueries::new(&doc).deep();
        let deep = deep_q.get_entity(player).unwrap();
        assert!(deep.components.is_some());
        assert!(deep.components.as_ref().unwrap().contains_key("Transform"));
    }

    #[test]
    fn find_entities_filters_are_false_default_and_deterministic() {
        let doc = sample_scene();
        let q = SceneQueries::new(&doc);

        let by_name = q.find_entities(&EntityQuery {
            name: Some("Crate".to_owned()),
            ..Default::default()
        });
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0].name, "Crate");

        let by_tag = q.find_entities(&EntityQuery {
            tag: Some("gameplay".to_owned()),
            ..Default::default()
        });
        assert_eq!(by_tag.len(), 1);
        assert_eq!(by_tag[0].name, "Player");

        let empty = q.find_entities(&EntityQuery::default());
        assert_eq!(empty.len(), 4);
    }

    #[test]
    fn children_and_parent_resolve_and_missing_returns_none() {
        let doc = sample_scene();
        let q = SceneQueries::new(&doc);
        let environment = doc
            .entities
            .iter()
            .find(|e| e.name == "Environment")
            .unwrap()
            .id;
        let crate_entity = doc.entities.iter().find(|e| e.name == "Crate").unwrap().id;

        let children = q.get_children(environment).unwrap();
        assert_eq!(children.ids.len(), 3);

        let deep_q = q.deep();
        let children_deep = deep_q.get_children(environment).unwrap();
        let entries = children_deep.entries.as_deref().unwrap_or_default();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Sun", "Player", "Crate"]);

        let parent = q.get_parent(crate_entity).unwrap();
        assert_eq!(parent.parent.unwrap().name, "Environment");

        let missing = EntityId::new();
        assert!(q.get_entity(missing).is_none());
        assert!(q.get_children(missing).is_none());
        assert!(q.get_parent(missing).is_none());
    }

    #[test]
    fn asset_users_and_dependencies_are_grounded_in_the_scene() {
        let (doc, ids) = sample_scene_with_assets();
        let q = SceneQueries::with_assets(&doc, &ids.index);

        let users = q.get_asset_users(ids.material);
        assert_eq!(users.record.as_ref().unwrap().kind, AssetKind::Material);
        assert!(!users.users.is_empty(), "the crate uses the material");

        let deps = q.get_asset_dependencies(ids.material);
        assert!(
            deps.dependencies.iter().any(|d| d.asset == ids.texture),
            "material co-ships its albedo texture"
        );
    }

    #[test]
    fn material_graph_and_shader_resolve_users() {
        let (doc, ids) = sample_scene_with_assets();
        let q = SceneQueries::with_assets(&doc, &ids.index);

        let graph = q.get_material_graph(ids.material);
        assert_eq!(graph.material, ids.material);
        assert!(graph.textures.contains(&ids.texture));

        let shader = q.get_shader(ids.shader);
        assert_eq!(shader.shader, ids.shader);
        assert!(!shader.users.is_empty());
    }

    #[test]
    fn animation_graph_reports_clip_and_mesh() {
        let (doc, ids) = sample_scene_with_assets();
        let q = SceneQueries::with_assets(&doc, &ids.index);
        let animator = doc.entities.iter().find(|e| e.name == "Idle").unwrap().id;

        let graph = q.get_animation_graph(animator).unwrap();
        assert_eq!(graph.clip, Some(ids.animation));
        assert_eq!(
            graph.clip_record.as_ref().unwrap().kind,
            AssetKind::Animation
        );
    }

    #[test]
    fn physics_mirrors_the_world_brain_projection() {
        let doc = sample_scene();
        let q = SceneQueries::new(&doc);
        let player = doc.entities.iter().find(|e| e.name == "Player").unwrap().id;

        let compact = q.get_physics(player).unwrap();
        assert_eq!(compact.body_kind.as_deref(), Some("kinematic"));
        assert!(compact.has_character_controller);
        assert!(compact.extras.is_none());

        let deep_q = q.deep();
        let deep = deep_q.get_physics(player).unwrap();
        assert!(deep.extras.is_some());
        assert!(deep.extras.as_ref().unwrap().contains_key("RigidBody"));

        let plain = doc.entities.iter().find(|e| e.name == "Sun").unwrap().id;
        assert!(q.get_physics(plain).is_none(), "sun has no physics");
    }

    #[test]
    fn deep_expansion_round_trips_through_serde_with_omitted_fields() {
        let doc = sample_scene();
        let q = SceneQueries::new(&doc).get();
        let compact_json = serde_json::to_value(&q).unwrap();
        assert!(compact_json.get("hierarchy").is_none());
        assert!(compact_json.get("settings").is_none());
    }

    struct SceneAssets {
        index: AssetIndex,
        material: AssetId,
        texture: AssetId,
        shader: AssetId,
        animation: AssetId,
    }

    fn sample_scene_with_assets() -> (SceneDocument, SceneAssets) {
        let mut index = AssetIndex::default();
        let mut make = |kind: AssetKind| {
            let record = AssetRecord {
                id: AssetId::new(),
                path_rel: format!("assets/{kind}.bin"),
                kind,
                hash: String::new(),
                license: LicenseState::Unknown,
                size_bytes: 0,
                used_by_scenes: Vec::new(),
            };
            let id = record.id;
            index.assets.insert(id, record);
            id
        };
        let material = make(AssetKind::Material);
        let texture = make(AssetKind::Texture);
        let shader = make(AssetKind::Shader);
        let animation = make(AssetKind::Animation);

        let doc = scene_for_assets(material, texture, shader, animation);

        (
            doc,
            SceneAssets {
                index,
                material,
                texture,
                shader,
                animation,
            },
        )
    }

    fn scene_for_assets(
        material: AssetId,
        texture: AssetId,
        shader: AssetId,
        animation: AssetId,
    ) -> SceneDocument {
        let mut doc = SceneDocument::empty("level_01");
        let floor = EntityId::new();
        let crate_entity = EntityId::new();
        let idle = EntityId::new();
        doc.entities = vec![
            crate::document::Entity {
                id: floor,
                name: "Floor".to_owned(),
                parent: None,
                tags: vec![],
                components: BTreeMap::from([
                    (
                        "MeshRenderer".to_owned(),
                        serde_json::json!({ "mesh": "asset:x", "materials": [format!("asset:{material}")] }),
                    ),
                    (
                        "ShaderRef".to_owned(),
                        serde_json::json!({ "shader": format!("asset:{shader}") }),
                    ),
                ]),
            },
            crate::document::Entity {
                id: crate_entity,
                name: "Crate".to_owned(),
                parent: None,
                tags: vec![],
                components: BTreeMap::from([
                    (
                        "MeshRenderer".to_owned(),
                        serde_json::json!({ "mesh": "asset:x", "materials": [format!("asset:{material}")] }),
                    ),
                    (
                        "MaterialOverride".to_owned(),
                        serde_json::json!({ "albedo": format!("asset:{texture}") }),
                    ),
                ]),
            },
            crate::document::Entity {
                id: idle,
                name: "Idle".to_owned(),
                parent: None,
                tags: vec![],
                components: BTreeMap::from([
                    (
                        "AnimationPlayer".to_owned(),
                        serde_json::json!({ "clip": format!("asset:{animation}") }),
                    ),
                    (
                        "MeshRenderer".to_owned(),
                        serde_json::json!({ "mesh": "asset:x", "materials": [format!("asset:{material}")] }),
                    ),
                ]),
            },
        ];
        doc
    }

    fn sample_scene() -> SceneDocument {
        let mut doc = SceneDocument::empty("level_01");
        let environment = EntityId::new();
        let sun = EntityId::new();
        let player = EntityId::new();
        let crate_entity = EntityId::new();
        doc.entities = vec![
            crate::document::Entity {
                id: environment,
                name: "Environment".to_owned(),
                parent: None,
                tags: vec![],
                components: Default::default(),
            },
            crate::document::Entity {
                id: sun,
                name: "Sun".to_owned(),
                parent: Some(environment),
                tags: vec![],
                components: Default::default(),
            },
            crate::document::Entity {
                id: player,
                name: "Player".to_owned(),
                parent: Some(environment),
                tags: vec!["gameplay".to_owned()],
                components: BTreeMap::from([
                    (
                        "Transform".to_owned(),
                        serde_json::json!({ "pos": [1.0, 2.0, 3.0] }),
                    ),
                    (
                        "RigidBody".to_owned(),
                        serde_json::json!({ "kind": "kinematic", "lock_rotation": true }),
                    ),
                    (
                        "CharacterController".to_owned(),
                        serde_json::json!({ "height": 1.8 }),
                    ),
                    (
                        "ScriptRef".to_owned(),
                        serde_json::json!({ "script": "asset:scripty" }),
                    ),
                ]),
            },
            crate::document::Entity {
                id: crate_entity,
                name: "Crate".to_owned(),
                parent: Some(environment),
                tags: vec![],
                components: BTreeMap::from([
                    (
                        "RigidBody".to_owned(),
                        serde_json::json!({ "kind": "dynamic", "mass": 5.0 }),
                    ),
                    (
                        "Collider".to_owned(),
                        serde_json::json!({ "shape": "cuboid", "sensor": true }),
                    ),
                ]),
            },
        ];
        doc
    }
}
