//! Prefabs (`bhippi-prefab@1`, ENG-125).
//!
//! A prefab is a named entity subtree that can be stamped into a scene many times. It is
//! what makes "put a streetlamp every ten metres" one asset and forty instances instead of
//! forty hand-built copies that drift apart.
//!
//! Instances record which prefab they came from and which fields they have overridden, so a
//! later edit to the prefab can propagate to every instance that has *not* overridden that
//! field. Nothing here reaches the filesystem: `apply` produces entity specs, and the caller
//! turns those into a transaction like any other change (INV-070).

use crate::document::{Entity, SceneDocument};
use crate::error::{EngineError, Result};
use crate::transaction::{EntitySpec, Op};
use bhippi_types::{AssetId, EntityId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const PREFAB_FORMAT: &str = "bhippi-prefab@1";

/// The component every instantiated root carries, naming its source. The Outliner shows it
/// as a prefab instance and the propagation pass finds instances by it.
pub const INSTANCE_COMPONENT: &str = "PrefabInstance";

/// One node of a prefab's subtree. Ids here are *local* — they exist only inside the
/// document and are remapped to fresh ULIDs on every instantiation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PrefabNode {
    pub local_id: String,
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub components: BTreeMap<String, Value>,
}

/// One `assets/prefabs/*.prefab.json` document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PrefabDocument {
    pub format: String,
    pub id: AssetId,
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<PrefabNode>,
}

impl PrefabDocument {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format: PREFAB_FORMAT.to_owned(),
            id: AssetId::new(),
            name: name.into(),
            nodes: Vec::new(),
        }
    }

    /// Capture an existing entity and its descendants as a prefab.
    pub fn from_subtree(scene: &SceneDocument, root: EntityId, name: &str) -> Result<Self> {
        let root_entity = scene.entity(root).ok_or_else(|| {
            EngineError::Asset(
                format!("entity {root} is not in the scene"),
                Some("Select an entity that exists.".to_owned()),
            )
        })?;
        let mut doc = Self::new(if name.trim().is_empty() {
            root_entity.name.clone()
        } else {
            name.to_owned()
        });
        let mut queue = vec![(root, None::<String>)];
        while let Some((id, parent)) = queue.pop() {
            let Some(entity) = scene.entity(id) else {
                continue;
            };
            let local = id.to_string();
            doc.nodes.push(PrefabNode {
                local_id: local.clone(),
                name: entity.name.clone(),
                parent,
                tags: entity.tags.clone(),
                components: entity.components.clone(),
            });
            for child in scene.children_of(id) {
                queue.push((child, Some(local.clone())));
            }
        }
        // Parents before children keeps instantiation order valid without a second pass.
        doc.nodes.sort_by_key(|node| node.parent.is_some());
        doc.validate()?;
        Ok(doc)
    }

    pub fn parse(text: &str) -> Result<Self> {
        let doc: Self = serde_json::from_str(text).map_err(|error| {
            EngineError::Asset(
                format!("invalid prefab document: {error}"),
                Some("Prefabs are bhippi-prefab@1 JSON.".to_owned()),
            )
        })?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn dump(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            EngineError::Asset(
                format!("cannot serialise prefab: {error}"),
                Some("Report this as an engine bug.".to_owned()),
            )
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != PREFAB_FORMAT {
            return Err(EngineError::Asset(
                format!("unsupported prefab format {:?}", self.format),
                Some(format!("Expected {PREFAB_FORMAT}.")),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(EngineError::Asset(
                "prefab name must not be empty".to_owned(),
                Some("Give the prefab a name.".to_owned()),
            ));
        }
        if self.nodes.is_empty() {
            return Err(EngineError::Asset(
                "a prefab must contain at least one node".to_owned(),
                Some("Capture an entity into it first.".to_owned()),
            ));
        }
        let mut seen = BTreeSet::new();
        for node in &self.nodes {
            if !seen.insert(node.local_id.as_str()) {
                return Err(EngineError::Asset(
                    format!("duplicate prefab node id {:?}", node.local_id),
                    Some("Local ids must be unique inside a prefab.".to_owned()),
                ));
            }
            for (component, payload) in &node.components {
                crate::schema::validate_component(component, payload)?;
            }
        }
        for node in &self.nodes {
            if let Some(parent) = &node.parent {
                if !seen.contains(parent.as_str()) {
                    return Err(EngineError::Asset(
                        format!(
                            "prefab node {:?} references missing parent {parent:?}",
                            node.name
                        ),
                        Some("Re-capture the prefab from the scene.".to_owned()),
                    ));
                }
            }
        }
        if self.roots().is_empty() {
            return Err(EngineError::Asset(
                "a prefab needs at least one root node".to_owned(),
                Some("Every node has a parent, which is a cycle.".to_owned()),
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn roots(&self) -> Vec<&PrefabNode> {
        self.nodes
            .iter()
            .filter(|node| node.parent.is_none())
            .collect()
    }

    /// Lower one instantiation into spawn ops.
    ///
    /// `at` offsets every root; `parent` places the instance in the scene hierarchy. Local
    /// ids are remapped to fresh ULIDs, so stamping the same prefab twice yields two
    /// independent subtrees.
    pub fn instantiate(
        &self,
        at: Option<[f32; 3]>,
        parent: Option<EntityId>,
        name_override: Option<&str>,
    ) -> Result<Vec<Op>> {
        self.validate()?;
        let remap: BTreeMap<&str, EntityId> = self
            .nodes
            .iter()
            .map(|node| (node.local_id.as_str(), EntityId::new()))
            .collect();
        let mut ops = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let Some(id) = remap.get(node.local_id.as_str()).copied() else {
                continue;
            };
            let is_root = node.parent.is_none();
            let mut components = node.components.clone();
            if is_root {
                if let Some(at) = at {
                    let transform = components.entry("Transform".to_owned()).or_insert_with(
                        || json!({ "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
                    );
                    if let Some(object) = transform.as_object_mut() {
                        object.insert("pos".to_owned(), json!(at));
                    }
                }
                components.insert(
                    INSTANCE_COMPONENT.to_owned(),
                    json!({ "prefab": self.id.to_string(), "overrides": [] }),
                );
            }
            let node_parent = node
                .parent
                .as_deref()
                .and_then(|local| remap.get(local).copied());
            ops.push(Op::Spawn {
                entity: EntitySpec {
                    id,
                    name: match (is_root, name_override) {
                        (true, Some(name)) if !name.trim().is_empty() => name.to_owned(),
                        _ => node.name.clone(),
                    },
                    parent: None,
                    tags: node.tags.clone(),
                    components,
                },
                parent: node_parent.or(if is_root { parent } else { None }),
            });
        }
        Ok(ops)
    }

    /// Entities in `scene` that are instances of this prefab.
    #[must_use]
    pub fn instances_in(&self, scene: &SceneDocument) -> Vec<EntityId> {
        let id = self.id.to_string();
        scene
            .entities
            .iter()
            .filter(|entity| {
                entity
                    .components
                    .get(INSTANCE_COMPONENT)
                    .and_then(|value| value.get("prefab"))
                    .and_then(Value::as_str)
                    == Some(id.as_str())
            })
            .map(|entity| entity.id)
            .collect()
    }

    /// Ops that push this prefab's current component payloads onto its instances, skipping
    /// any component an instance has listed in its `overrides`.
    ///
    /// Only the instance **root** is propagated to. Walking the whole subtree would need a
    /// stable local-id trail on every descendant, which the format does not carry yet; doing
    /// half of it silently would be the kind of partial behaviour that looks like a bug.
    pub fn propagate_to_instances(&self, scene: &SceneDocument) -> Result<Vec<Op>> {
        self.validate()?;
        let Some(root) = self.roots().first().copied() else {
            return Ok(Vec::new());
        };
        let mut ops = Vec::new();
        for instance in self.instances_in(scene) {
            let Some(entity) = scene.entity(instance) else {
                continue;
            };
            let overrides = overridden_components(entity);
            for (component, value) in &root.components {
                if overrides.contains(component.as_str()) || component == INSTANCE_COMPONENT {
                    continue;
                }
                // Transform is per-instance by definition: propagating it would teleport
                // every copy onto the prefab's authored position.
                if component == "Transform" {
                    continue;
                }
                match entity.components.get(component) {
                    Some(current) if current == value => {}
                    Some(current) => ops.push(Op::PatchComponent {
                        entity: instance,
                        component: component.clone(),
                        from: current.clone(),
                        to: value.clone(),
                    }),
                    None => ops.push(Op::AddComponent {
                        entity: instance,
                        component: component.clone(),
                        value: value.clone(),
                    }),
                }
            }
        }
        Ok(ops)
    }
}

fn overridden_components(entity: &Entity) -> BTreeSet<&str> {
    entity
        .components
        .get(INSTANCE_COMPONENT)
        .and_then(|value| value.get("overrides"))
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{PrefabDocument, INSTANCE_COMPONENT};
    use crate::document::{Entity, SceneDocument};
    use crate::transaction::{EngineTransaction, Op};
    use bhippi_types::{EngineActor, EntityId, TransactionId};
    use serde_json::json;

    fn transform(pos: [f32; 3]) -> serde_json::Value {
        json!({ "pos": pos, "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] })
    }

    fn scene_with_lamp() -> (SceneDocument, EntityId) {
        let mut doc = SceneDocument::empty("level_01");
        let post = EntityId::new();
        let bulb = EntityId::new();
        doc.entities = vec![
            Entity {
                id: post,
                name: "LampPost".to_owned(),
                parent: None,
                tags: vec!["street".to_owned()],
                components: std::collections::BTreeMap::from([
                    ("Transform".to_owned(), transform([4.0, 0.0, 0.0])),
                    (
                        "MeshRenderer".to_owned(),
                        json!({ "mesh": "", "materials": [], "cast_shadows": true }),
                    ),
                ]),
            },
            Entity {
                id: bulb,
                name: "Bulb".to_owned(),
                parent: Some(post),
                tags: vec![],
                components: std::collections::BTreeMap::from([
                    ("Transform".to_owned(), transform([0.0, 4.0, 0.0])),
                    (
                        "Light".to_owned(),
                        json!({ "kind": "point", "color": [1.0, 0.9, 0.7], "intensity": 3.0, "range": 12.0 }),
                    ),
                ]),
            },
        ];
        (doc, post)
    }

    fn apply(doc: &mut SceneDocument, ops: Vec<Op>) {
        EngineTransaction {
            id: TransactionId::new(),
            label: "test".to_owned(),
            actor: EngineActor::User,
            ops,
            inverse: vec![],
            touched: vec![],
            scene: None,
        }
        .apply(doc)
        .expect("applies");
    }

    #[test]
    fn capturing_a_subtree_keeps_the_hierarchy_and_round_trips() {
        let (scene, post) = scene_with_lamp();
        let prefab = PrefabDocument::from_subtree(&scene, post, "streetlamp").expect("capture");
        assert_eq!(prefab.nodes.len(), 2);
        assert_eq!(prefab.roots().len(), 1);
        let text = prefab.dump().expect("dump");
        assert_eq!(PrefabDocument::parse(&text).expect("parse"), prefab);
    }

    #[test]
    fn each_instantiation_gets_fresh_ids_so_copies_are_independent() {
        let (mut scene, post) = scene_with_lamp();
        let prefab = PrefabDocument::from_subtree(&scene, post, "streetlamp").expect("capture");

        apply(
            &mut scene,
            prefab
                .instantiate(Some([10.0, 0.0, 0.0]), None, None)
                .expect("first"),
        );
        apply(
            &mut scene,
            prefab
                .instantiate(Some([20.0, 0.0, 0.0]), None, None)
                .expect("second"),
        );

        scene.validate().expect("hierarchy stays valid");
        let instances = prefab.instances_in(&scene);
        assert_eq!(instances.len(), 2);
        assert_ne!(instances[0], instances[1]);
        // 2 originals + 2 nodes × 2 instances.
        assert_eq!(scene.entity_count(), 6);

        // The roots landed where they were asked to, and kept their children.
        for instance in &instances {
            assert_eq!(scene.children_of(*instance).len(), 1);
        }
        let positions: Vec<f64> = instances
            .iter()
            .map(|id| {
                scene.entity(*id).expect("root").components["Transform"]["pos"][0]
                    .as_f64()
                    .unwrap_or(0.0)
            })
            .collect();
        assert!(positions.contains(&10.0) && positions.contains(&20.0));
    }

    #[test]
    fn propagation_updates_instances_but_respects_overrides() {
        let (mut scene, post) = scene_with_lamp();
        let mut prefab = PrefabDocument::from_subtree(&scene, post, "streetlamp").expect("capture");
        apply(&mut scene, prefab.instantiate(None, None, None).expect("a"));
        apply(&mut scene, prefab.instantiate(None, None, None).expect("b"));
        let instances = prefab.instances_in(&scene);

        // One instance is deliberately customised and says so.
        let customised = instances[0];
        let marker = scene.entity(customised).expect("e").components[INSTANCE_COMPONENT].clone();
        apply(
            &mut scene,
            vec![Op::PatchComponent {
                entity: customised,
                component: INSTANCE_COMPONENT.to_owned(),
                from: marker,
                to: json!({ "prefab": prefab.id.to_string(), "overrides": ["MeshRenderer"] }),
            }],
        );

        // The prefab's root mesh changes.
        if let Some(root) = prefab.nodes.iter_mut().find(|node| node.parent.is_none()) {
            root.components.insert(
                "MeshRenderer".to_owned(),
                json!({ "mesh": "asset:01JD0000000000000000000000", "materials": [], "cast_shadows": false }),
            );
        }

        let ops = prefab.propagate_to_instances(&scene).expect("propagate");
        apply(&mut scene, ops);

        let untouched = instances[1];
        assert_eq!(
            scene.entity(untouched).expect("e").components["MeshRenderer"]["mesh"],
            "asset:01JD0000000000000000000000",
            "an un-overridden instance follows the prefab"
        );
        assert_eq!(
            scene.entity(customised).expect("e").components["MeshRenderer"]["mesh"],
            "",
            "an overridden component is left alone"
        );
    }

    #[test]
    fn propagation_never_moves_an_instance() {
        let (mut scene, post) = scene_with_lamp();
        let prefab = PrefabDocument::from_subtree(&scene, post, "streetlamp").expect("capture");
        apply(
            &mut scene,
            prefab
                .instantiate(Some([30.0, 0.0, 0.0]), None, None)
                .expect("a"),
        );
        let instance = prefab.instances_in(&scene)[0];

        let ops = prefab.propagate_to_instances(&scene).expect("propagate");
        apply(&mut scene, ops);
        assert_eq!(
            scene.entity(instance).expect("e").components["Transform"]["pos"][0],
            30.0,
            "Transform is per-instance and must never be propagated"
        );
    }

    #[test]
    fn an_empty_or_cyclic_prefab_is_rejected() {
        let empty = PrefabDocument::new("nothing");
        assert!(empty.validate().is_err());

        let mut cyclic = PrefabDocument::new("loop");
        cyclic.nodes = vec![
            super::PrefabNode {
                local_id: "a".to_owned(),
                name: "A".to_owned(),
                parent: Some("b".to_owned()),
                tags: vec![],
                components: Default::default(),
            },
            super::PrefabNode {
                local_id: "b".to_owned(),
                name: "B".to_owned(),
                parent: Some("a".to_owned()),
                tags: vec![],
                components: Default::default(),
            },
        ];
        let error = cyclic.validate().expect_err("no root");
        assert!(error.hint().is_some());
    }

    #[test]
    fn a_prefab_carrying_an_invalid_component_is_rejected_at_parse() {
        let mut prefab = PrefabDocument::new("bad");
        prefab.nodes = vec![super::PrefabNode {
            local_id: "a".to_owned(),
            name: "A".to_owned(),
            parent: None,
            tags: vec![],
            components: std::collections::BTreeMap::from([(
                "RigidBody".to_owned(),
                json!({ "kind": "bouncy" }),
            )]),
        }];
        let text = serde_json::to_string(&prefab).expect("serialise");
        let error =
            PrefabDocument::parse(&text).expect_err("schema is enforced inside prefabs too");
        assert!(error.to_string().contains("kind"));
    }
}
