use crate::document::SceneDocument;
use crate::query;
use crate::schema;
use bhippi_types::{EntityId, SceneId};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

/// The editor's loaded-scene mind map (plan §6.3): a compact, structured digest the AI
/// reasons over and the Mind-Map panel visualises. Everything here is derived, never
/// written back — the scene document is the single source of truth.

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MindNodeKind {
    Root,
    Entity,
    Camera,
    Light,
    Physics,
    Visual,
    Script,
    Tag,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MindNode {
    pub id: EntityId,
    pub name: String,
    pub kind: MindNodeKind,
    pub parent: Option<EntityId>,
    pub children: Vec<EntityId>,
    /// The component names carried by the entity (drives palette/label colour).
    pub components: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineMindMap {
    /// The scene this map was generated from (invariant: never edited).
    pub scene: Option<SceneId>,
    pub revision: u64,
    pub nodes: BTreeMap<EntityId, MindNode>,
}

impl EngineMindMap {
    /// Build the map from the loaded scene. Deterministic: same scene → same map.
    #[must_use]
    pub fn build(scene: &SceneDocument, revision: u64) -> Self {
        let mut nodes = BTreeMap::new();
        for entity in &scene.entities {
            let kind = classify(entity);
            nodes.insert(
                entity.id,
                MindNode {
                    id: entity.id,
                    name: entity.name.clone(),
                    kind,
                    parent: entity.parent,
                    children: scene.children_of(entity.id),
                    components: entity.components.keys().cloned().collect(),
                },
            );
        }
        Self {
            scene: Some(scene.id),
            revision,
            nodes,
        }
    }

    #[must_use]
    pub fn node(&self, id: EntityId) -> Option<&MindNode> {
        self.nodes.get(&id)
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.nodes.len()
    }
}

fn classify(entity: &crate::document::Entity) -> MindNodeKind {
    if entity.components.contains_key("Camera") {
        return MindNodeKind::Camera;
    }
    if entity.components.contains_key("Light") {
        return MindNodeKind::Light;
    }
    if entity.components.contains_key("RigidBody")
        || entity.components.contains_key("CharacterController")
    {
        return MindNodeKind::Physics;
    }
    if entity.components.contains_key("ScriptRef") {
        return MindNodeKind::Script;
    }
    if entity.components.contains_key("MeshRenderer") {
        return MindNodeKind::Visual;
    }
    MindNodeKind::Entity
}

// ── prompt digest (≤ 1.5k tokens target, INV-026 lens) ───────────────────────────────

/// The schema section as compact prompt lines: `Transform:pos:vec3 …`. This is what the
/// AI repairs against (plan §10.1) and what the explain-step quotes.
#[must_use]
pub fn component_schema_excerpt() -> String {
    let mut lines: Vec<String> = Vec::new();
    for component in schema::registry() {
        let fields = component
            .fields
            .iter()
            .map(|field| format!("{}:{}", field.name, field.kind))
            .collect::<Vec<_>>()
            .join(", ");
        if fields.is_empty() {
            lines.push(format!("{} (no fields)", component.name));
        } else {
            lines.push(format!("{} ─ {fields}", component.name));
        }
    }
    lines.join("\n")
}

/// A stable, deterministic, bounded text digest of the scene + schema + tags for the
/// mind-map context that ships with every engine prompt (plan §6.3, budget ≤ 1.5k tokens).
#[must_use]
pub fn digest_text(scene: &SceneDocument, revision: u64) -> String {
    let node_cap = 256usize; // soft cap for pathological scenes; the map still holds all ids
    let entries = query::hierarchy(scene);
    let mut node_lines: Vec<String> = Vec::new();
    for (count, entry) in entries.iter().enumerate() {
        if count >= node_cap {
            node_lines.push("…more entities (cap).".to_owned());
            break;
        }
        let depth = chain_depth(scene, entry.id);
        let indent = "  ".repeat(depth.min(6));
        node_lines.push(format!("{indent}- {} `{}`", entry.name, entry.id));
    }
    let tags: Vec<String> = scene
        .entities
        .iter()
        .flat_map(|entity| entity.tags.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    format!(
        "MINDMAP rev{revision}\nscene: `{}`\nentities: {total}\nhierarchy:\n{nodes}\n\nSCHEMA (what fields/values are valid):\n{schema}\n\ntags in use: {tags}\n",
        scene.name,
        total = scene.entity_count(),
        nodes = node_lines.join("\n"),
        schema = component_schema_excerpt(),
        tags = tags.iter().map(|tag| format!("`{tag}`")).collect::<Vec<_>>().join(", "),
    )
}

/// Compact project-level digest written to `.bhippi/engine/engine-map.json`.
#[must_use]
pub fn project_digest_json(
    game_name: &str,
    main: &str,
    hud: Option<&str>,
    levels: &[&str],
) -> String {
    let payload = serde_json::json!({
        "format": "bhippi-engine-map@1",
        "game": game_name,
        "main": main,
        "hud": hud,
        "levels": levels,
        "play": {
            "main": "compose main + hud + levels[0]",
            "level": "open that level + hud",
            "hud": "overlay only"
        },
        "replace_object": "Content Drawer → right-click mesh → Replace Object. Transform is preserved.",
        "weather": "assets/weather/ultrasky.json",
        "materials": "assets/materials/lit_pbr.mat.json",
        "shaders": "assets/shaders/lit_pbr.shader.json"
    });
    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_owned())
}

fn chain_depth(scene: &SceneDocument, id: EntityId) -> usize {
    let mut depth = 0usize;
    let mut current = scene.entity(id).and_then(|entity| entity.parent);
    while let Some(parent) = current {
        depth += 1;
        if depth > scene.entities.len() {
            break;
        }
        current = scene.entity(parent).and_then(|entity| entity.parent);
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::{component_schema_excerpt, digest_text, EngineMindMap, MindNodeKind};
    use crate::document::{Entity, SceneDocument};
    use bhippi_types::EntityId;
    use serde_json::json;

    fn small_scene() -> SceneDocument {
        let mut doc = SceneDocument::empty("level_01");
        doc.entities.push(Entity {
            id: EntityId::new(),
            name: "MainCamera".to_owned(),
            parent: None,
            tags: vec!["camera".to_owned()],
            components: std::collections::BTreeMap::from([(
                "Camera".to_owned(),
                json!({ "fov": 0.9 }),
            )]),
        });
        doc.entities.push(Entity {
            id: EntityId::new(),
            name: "Sun".to_owned(),
            parent: None,
            tags: vec![],
            components: std::collections::BTreeMap::from([(
                "Light".to_owned(),
                json!({ "kind": "directional" }),
            )]),
        });
        doc
    }

    #[test]
    fn map_classifies_cameras_lights_and_clamps_counts() {
        let doc = small_scene();
        let map = EngineMindMap::build(&doc, 3);
        assert_eq!(map.revision, 3);
        assert_eq!(map.count(), 2);
        let kinds: Vec<MindNodeKind> = map.nodes.values().map(|node| node.kind).collect();
        assert!(kinds.contains(&MindNodeKind::Camera));
        assert!(kinds.contains(&MindNodeKind::Light));
    }

    #[test]
    fn digest_is_deterministic_and_contains_schema() {
        let doc = small_scene();
        let first = digest_text(&doc, 1);
        let second = digest_text(&doc, 1);
        assert_eq!(first, second);
        assert!(first.contains("SCHEMA"));
        assert!(first.contains("Transform ─ pos:vec3"));
    }

    #[test]
    fn schema_excerpt_lists_field_kinds_readably() {
        let excerpt = component_schema_excerpt();
        assert!(excerpt.contains("Vec3|") || excerpt.contains("vec3"));
        assert!(excerpt.contains("ScriptRef"));
    }
}
