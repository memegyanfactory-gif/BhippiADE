use crate::error::{EngineError, Result};
use bhippi_types::EntityId;
use bhippi_types::SceneId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

/// The human-readable scene format marker (plan §9.2). Scenes are deterministic,
/// sorted-key JSON so they stay diffable and AI-readable.
pub const SCENE_FORMAT: &str = "bhippi-scene@1";

/// The canonical serde content type for `.bscn.json` scene documents.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SceneDocument {
    pub format: String,
    pub id: SceneId,
    pub name: String,
    #[serde(default)]
    pub settings: SceneSettings,
    #[serde(default)]
    pub entities: Vec<Entity>,
}

/// Scene-level settings (ambient, skybox). Values mirror what the Inspector can edit.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, specta::Type)]
pub struct SceneSettings {
    #[serde(default = "default_ambient")]
    pub ambient: [f32; 3],
    /// `asset:<ulid>` reference to a skybox texture.
    pub skybox: Option<String>,
    /// Unreal analogue: Main (persistent), Level, HUD, or an empty edit grid.
    #[serde(default)]
    pub kind: SceneKind,
    /// Path to the HUD scene (Main only).
    #[serde(default)]
    pub hud: Option<String>,
    /// Ordered playable levels (Main only).
    #[serde(default)]
    pub levels: Vec<String>,
    /// UltraSky-style weather preset id.
    #[serde(default)]
    pub weather: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SceneKind {
    Main,
    #[default]
    Level,
    Hud,
    Empty,
}

const fn default_ambient() -> [f32; 3] {
    [0.02, 0.02, 0.03]
}

/// One entity in the scene. Components are serialised as a map of `component-name →
/// payload`, where the payload is a reflection-serialised JSON value understood by the
/// schema registry (`crate::schema`).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Entity {
    pub id: EntityId,
    pub name: String,
    pub parent: Option<EntityId>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub components: BTreeMap<String, serde_json::Value>,
}

impl Entity {
    /// A transform component accessing shorthand — every entity carries one.
    #[must_use]
    pub fn transform(&self) -> Option<&serde_json::Value> {
        self.components.get("Transform")
    }

    #[must_use]
    pub fn has_component(&self, name: &str) -> bool {
        self.components.contains_key(name)
    }
}

impl SceneDocument {
    /// Build an empty scene with a fresh id.
    #[must_use]
    pub fn empty(name: impl Into<String>) -> Self {
        Self {
            format: SCENE_FORMAT.to_owned(),
            id: SceneId::new(),
            name: name.into(),
            settings: SceneSettings::default(),
            entities: Vec::new(),
        }
    }

    /// Parse a `.bscn.json` document and validate every invariant.
    pub fn parse(text: &str) -> Result<Self> {
        let doc: SceneDocument = serde_json::from_str(text).map_err(|error| {
            EngineError::Scene(
                format!("invalid scene document: {error}"),
                Some("The file may have been hand-edited. Reload from the last save.".to_owned()),
            )
        })?;
        doc.validate()?;
        Ok(doc)
    }

    /// Parse a scene that may still carry UI `ent_*` / `scene_*` ids. Non-ULID ids are
    /// rewritten to a **stable** ULID derived from the original token so a file upgrades
    /// once and then round-trips.
    pub fn parse_lenient(text: &str) -> Result<Self> {
        if let Ok(doc) = Self::parse(text) {
            return Ok(doc);
        }
        let mut value: serde_json::Value = serde_json::from_str(text).map_err(|error| {
            EngineError::Scene(
                format!("invalid scene document: {error}"),
                Some("The file may have been hand-edited. Reload from the last save.".to_owned()),
            )
        })?;
        remap_non_ulid_ids(&mut value);
        let doc: SceneDocument = serde_json::from_value(value).map_err(|error| {
            EngineError::Scene(
                format!("invalid scene document: {error}"),
                Some("The file may have been hand-edited. Reload from the last save.".to_owned()),
            )
        })?;
        doc.validate()?;
        Ok(doc)
    }

    /// Deterministic serialisation: maps are `BTreeMap`, entities keep authoring order.
    pub fn dump(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            EngineError::Scene(
                format!("cannot serialise scene: {error}"),
                Some("Report this as an engine bug.".to_owned()),
            )
        })
    }

    /// Structural invariants: unique ids, parents exist, no parent cycles.
    pub fn validate(&self) -> Result<()> {
        if self.format != SCENE_FORMAT {
            return Err(EngineError::Scene(
                format!("unsupported scene format {:?}", self.format),
                Some("Re-export the scene through a current engine version.".to_owned()),
            ));
        }
        let mut ids = BTreeSet::new();
        for entity in &self.entities {
            if !ids.insert(entity.id) {
                return Err(EngineError::Scene(
                    format!("duplicate entity id {}", entity.id),
                    Some("Rename or re-import the duplicated entity.".to_owned()),
                ));
            }
            if entity.name.trim().is_empty() {
                return Err(EngineError::Scene(
                    format!("entity {} has an empty name", entity.id),
                    Some("Give every entity a name.".to_owned()),
                ));
            }
        }
        for entity in &self.entities {
            if let Some(parent) = entity.parent {
                if !ids.contains(&parent) {
                    return Err(EngineError::Scene(
                        format!("entity {} references missing parent {}", entity.id, parent),
                        Some("The scene may be corrupted; undo or reload.".to_owned()),
                    ));
                }
            }
        }
        self.detect_cycles()?;
        Ok(())
    }

    fn detect_cycles(&self) -> Result<()> {
        // Walk each entity's parent chain once with a visited set. A single forward pass
        // is not enough: a cycle can be multi-hop and its head may appear late.
        let mut on_chain = BTreeSet::new();
        for entity in &self.entities {
            let mut current = Some(entity.id);
            on_chain.clear();
            let mut hops = 0usize;
            while let Some(step) = current {
                // A cycle: this step is already on the active walk.
                if !on_chain.insert(step) {
                    return Err(EngineError::Scene(
                        format!("entity {step} participates in a parent cycle"),
                        Some("Undo the last reparent or reload the scene.".to_owned()),
                    ));
                }
                hops += 1;
                if hops > self.entities.len() {
                    return Err(EngineError::Scene(
                        "parent chain longer than the scene".to_owned(),
                        Some("The hierarchy is corrupt; reload the scene.".to_owned()),
                    ));
                }
                match self.entity(step).and_then(|entity| entity.parent) {
                    Some(parent) => current = Some(parent),
                    None => current = None,
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.entities.iter().find(|entity| entity.id == id)
    }

    #[must_use]
    pub fn entity_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|entity| entity.id == id)
    }

    #[must_use]
    pub fn roots(&self) -> Vec<&Entity> {
        self.entities
            .iter()
            .filter(|entity| entity.parent.is_none())
            .collect()
    }

    #[must_use]
    pub fn children_of(&self, id: EntityId) -> Vec<EntityId> {
        self.entities
            .iter()
            .filter(|entity| entity.parent == Some(id))
            .map(|entity| entity.id)
            .collect()
    }

    /// The human/AI-friendly stable address `scene:/Parent/Child#ULID` (plan §9.1).
    /// `None` when the id is not in the scene.
    pub fn stable_path(&self, id: EntityId) -> Option<String> {
        let mut chain: Vec<String> = Vec::new();
        let mut current = Some(id);
        let mut guard = 0usize;
        while let Some(step) = current {
            if guard > self.entities.len() {
                return None;
            }
            guard += 1;
            let step_entity = self.entity(step)?;
            chain.push(step_entity.name.clone());
            current = step_entity.parent;
        }
        chain.reverse();
        let path = format!("{}:/{}#{}", self.name, chain.join("/"), id);
        Some(path)
    }

    /// Resolve a stable path or a bare `#ULID` back to an entity id.
    pub fn resolve_ref(&self, reference: &str) -> Option<EntityId> {
        let reference = reference.trim();
        if let Some(anchor) = reference.rsplit_once('#') {
            if let Ok(id) = EntityId::from_str(anchor.1) {
                if self.entity(id).is_some() {
                    return Some(id);
                }
            }
        }
        // Fall back to path-matching on names: "/Gameplay/Crate" must resolve like the
        // path half of a stable ref so the AI can address entities during a rename race.
        let path_half = reference
            .rsplit_once(':')
            .map(|(_, rest)| rest)
            .unwrap_or(reference);
        let parts: Vec<&str> = path_half
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            return None;
        }
        let root_name = parts[0];
        let candidates: Vec<EntityId> = self
            .entities
            .iter()
            .filter(|entity| entity.name == root_name && entity.parent.is_none())
            .map(|entity| entity.id)
            .collect();
        if parts.len() == 1 {
            return candidates.first().copied();
        }
        // Walk the child chain from the (first) matching root.
        let mut current = candidates.first().copied()?;
        for part in &parts[1..] {
            let children = self.children_of(current);
            let child = self
                .entities
                .iter()
                .find(|entity| children.contains(&entity.id) && entity.name == *part)?;
            current = child.id;
        }
        Some(current)
    }

    /// Total entity count (for the mind map and perf budgets).
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }
}

fn remap_non_ulid_ids(value: &mut serde_json::Value) {
    if let Some(id) = value.get("id").and_then(serde_json::Value::as_str) {
        if SceneId::from_str(id).is_err() {
            value["id"] = serde_json::Value::String(stable_ulid(id).to_string());
        }
    }
    let Some(entities) = value
        .get_mut("entities")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for entity in entities {
        let Some(object) = entity.as_object_mut() else {
            continue;
        };
        if let Some(id) = object.get("id").and_then(serde_json::Value::as_str) {
            object.insert(
                "id".to_owned(),
                serde_json::Value::String(stable_ulid(id).to_string()),
            );
        }
        if let Some(parent) = object.get("parent").and_then(serde_json::Value::as_str) {
            if !parent.is_empty() {
                object.insert(
                    "parent".to_owned(),
                    serde_json::Value::String(stable_ulid(parent).to_string()),
                );
            }
        }
    }
}

fn stable_ulid(token: &str) -> ulid::Ulid {
    if let Ok(id) = ulid::Ulid::from_str(token) {
        return id;
    }
    let hash = blake3::hash(token.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash.as_bytes()[..16]);
    ulid::Ulid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::{Entity, SceneDocument};
    use bhippi_types::EntityId;

    fn sample_document() -> SceneDocument {
        let mut doc = SceneDocument::empty("level_01");
        let environment = EntityId::new();
        let player = EntityId::new();
        let crate_entity = EntityId::new();
        let sun = EntityId::new();
        doc.entities = vec![
            Entity {
                id: environment,
                name: "Environment".to_owned(),
                parent: None,
                tags: vec![],
                components: Default::default(),
            },
            Entity {
                id: sun,
                name: "Sun".to_owned(),
                parent: Some(environment),
                tags: vec![],
                components: Default::default(),
            },
            Entity {
                id: player,
                name: "Player".to_owned(),
                parent: Some(environment),
                tags: vec!["gameplay".to_owned()],
                components: Default::default(),
            },
            Entity {
                id: crate_entity,
                name: "Crate".to_owned(),
                parent: Some(environment),
                tags: vec![],
                components: Default::default(),
            },
        ];
        doc
    }

    #[test]
    fn empty_document_validates() {
        let doc = SceneDocument::empty("level_01");
        assert!(doc.validate().is_ok());
    }

    #[test]
    fn dump_is_deterministic() {
        let doc = sample_document();
        let first = doc.dump().expect("dump");
        let second = doc.dump().expect("dump");
        assert_eq!(first, second);
        // And the round-trip through serde re-parses to the identical structure.
        let reparsed = SceneDocument::parse(&first).expect("parse");
        assert_eq!(reparsed, doc);
    }

    #[test]
    fn duplicate_ids_and_missing_parents_are_rejected() {
        let mut doc = sample_document();
        let id = doc.entities[0].id;
        doc.entities.push(Entity {
            id,
            name: "Duplicate".to_owned(),
            parent: None,
            tags: vec![],
            components: Default::default(),
        });
        assert!(doc.validate().is_err());

        let mut doc = sample_document();
        doc.entities[0].parent = Some(EntityId::new());
        assert!(doc.validate().is_err());
    }

    #[test]
    fn stable_path_round_trips_through_resolve() {
        let doc = sample_document();
        let player = doc
            .entities
            .iter()
            .find(|entity| entity.name == "Player")
            .expect("sample has Player");
        let path = doc.stable_path(player.id).expect("path");
        assert!(path.starts_with("level_01:/Environment/Player#"));

        assert_eq!(doc.resolve_ref(&path), Some(player.id));
        assert_eq!(
            doc.resolve_ref("level_01:/Environment/Player"),
            Some(player.id)
        );
        assert_eq!(doc.resolve_ref(&format!("#{}", player.id)), Some(player.id));
    }

    #[test]
    fn parent_cycles_are_detected() {
        let mut doc = sample_document();
        let a = doc.entities[0].id;
        let b = doc.entities[1].id;
        doc.entities[0].parent = Some(b);
        doc.entities[1].parent = Some(a);
        assert!(doc.validate().is_err());
    }

    #[test]
    fn parse_lenient_upgrades_editor_ids_stably() {
        let raw = r#"{
            "format": "bhippi-scene@1",
            "id": "scene_demo",
            "name": "level_01",
            "settings": { "ambient": [0.1, 0.1, 0.1], "skybox": null },
            "entities": [
                {
                    "id": "ent_player",
                    "name": "Player",
                    "parent": null,
                    "tags": ["gameplay"],
                    "components": {}
                }
            ]
        }"#;
        let first = SceneDocument::parse_lenient(raw).expect("lenient");
        let second = SceneDocument::parse_lenient(raw).expect("lenient again");
        assert_eq!(first.entities[0].id, second.entities[0].id);
        assert_eq!(first.entities[0].name, "Player");
        SceneDocument::parse(&first.dump().expect("dump")).expect("strict after upgrade");
    }
}
