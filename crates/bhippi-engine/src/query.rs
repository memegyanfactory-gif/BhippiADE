use crate::document::SceneDocument;
use bhippi_types::EntityId;
use serde::{Deserialize, Serialize};
use specta::Type;

/// A flattened, parent-before-child projection of the hierarchy — the exact tree shape
/// the Hierarchy panel renders and the AI addresses entities through.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct HierarchyEntry {
    pub id: EntityId,
    pub name: String,
    pub parent: Option<EntityId>,
    pub children: Vec<EntityId>,
    pub component_names: Vec<String>,
    /// Transform position for the lineage chips; `None` when absent (never in valid scenes).
    pub pos: Option<[f32; 3]>,
}

/// Parent-first, depth-first hierarchy projection (deterministic; authoring order within
/// a depth level — indentation collapses on the frontend, it never computes structure).
#[must_use]
pub fn hierarchy(scene: &SceneDocument) -> Vec<HierarchyEntry> {
    let mut out = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    for root in scene.entities.iter().filter(|e| e.parent.is_none()) {
        emit(scene, root.id, &mut visited, &mut out);
    }
    // Entities that (through an unusual load path) ended up parented but not reachable.
    for entity in &scene.entities {
        if !visited.contains(&entity.id) {
            emit(scene, entity.id, &mut visited, &mut out);
        }
    }
    out
}

fn emit(
    scene: &SceneDocument,
    id: EntityId,
    visited: &mut std::collections::BTreeSet<EntityId>,
    out: &mut Vec<HierarchyEntry>,
) {
    if !visited.insert(id) {
        return;
    }
    let Some(entity) = scene.entity(id) else {
        return;
    };
    let children = scene.children_of(id);
    let pos = entity
        .components
        .get("Transform")
        .and_then(|value| value.get("pos"))
        .and_then(|value| value.as_array())
        .and_then(|array| {
            if array.len() != 3 {
                return None;
            }
            let mut out = [0.0f32; 3];
            for (index, item) in array.iter().enumerate() {
                out[index] = item.as_f64().map(|f| f as f32)?;
            }
            Some(out)
        });
    out.push(HierarchyEntry {
        id: entity.id,
        name: entity.name.clone(),
        parent: entity.parent,
        children: children.clone(),
        component_names: entity.components.keys().cloned().collect(),
        pos,
    });
    for child in children {
        emit(scene, child, visited, out);
    }
}

/// Entity ids whose name matches `name` exactly (used by the AI to find "the Crate").
#[must_use]
pub fn find_by_name(scene: &SceneDocument, name: &str) -> Vec<EntityId> {
    scene
        .entities
        .iter()
        .filter(|entity| entity.name == name)
        .map(|entity| entity.id)
        .collect()
}

#[must_use]
pub fn find_with_component(scene: &SceneDocument, component: &str) -> Vec<EntityId> {
    scene
        .entities
        .iter()
        .filter(|entity| entity.components.contains_key(component))
        .map(|entity| entity.id)
        .collect()
}

/// Cheap, stable facts about a scene for IPC snapshots and header stats — never the whole
/// document (the webview computes nothing, INV-073).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SceneSnapshot {
    pub scene: bhippi_types::SceneId,
    pub name: String,
    pub entity_count: usize,
    pub root_count: usize,
    pub component_usage: std::collections::BTreeMap<String, usize>,
}

#[must_use]
pub fn snapshot(scene: &SceneDocument) -> SceneSnapshot {
    let mut usage = std::collections::BTreeMap::new();
    for entity in &scene.entities {
        for component in entity.components.keys() {
            *usage.entry(component.clone()).or_insert(0usize) += 1;
        }
    }
    SceneSnapshot {
        scene: scene.id,
        name: scene.name.clone(),
        entity_count: scene.entity_count(),
        root_count: scene.roots().len(),
        component_usage: usage,
    }
}

/// Deep-search helper for the AI: returns the first entity whose *stable path* contains
/// `needle` (case-insensitive) — lets "the box on the left in the demo" resolve anywhere.
#[must_use]
pub fn search_paths(scene: &SceneDocument, needle: &str) -> Vec<String> {
    let needle = needle.to_ascii_lowercase();
    let mut out = Vec::new();
    for entity in &scene.entities {
        if let Some(path) = scene.stable_path(entity.id) {
            if path.to_ascii_lowercase().contains(&needle) {
                out.push(path);
            }
        }
    }
    out
}

/// Convenience so the AI/query layer can inspect an entity without owning the doc.
#[must_use]
pub fn entity_names_for(ids: &[EntityId], scene: &SceneDocument) -> Vec<(EntityId, String)> {
    ids.iter()
        .filter_map(|id| scene.entity(*id).map(|entity| (*id, entity.name.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{find_by_name, find_with_component, hierarchy, search_paths, snapshot};
    use crate::document::{Entity, SceneDocument};
    use bhippi_types::EntityId;
    use serde_json::json;

    fn tree() -> SceneDocument {
        let mut doc = SceneDocument::empty("level_01");
        let root = EntityId::new();
        let child = EntityId::new();
        let grandchild = EntityId::new();
        doc.entities = vec![
            Entity {
                id: root,
                name: "Bookshelf".to_owned(),
                parent: None,
                tags: vec![],
                components: Default::default(),
            },
            Entity {
                id: child,
                name: "ShelfA".to_owned(),
                parent: Some(root),
                tags: vec![],
                components: std::collections::BTreeMap::from([(
                    "Transform".to_owned(),
                    json!({ "pos": [1.0, 2.0, 3.0] }),
                )]),
            },
            Entity {
                id: grandchild,
                name: "Crate".to_owned(),
                parent: Some(child),
                tags: vec![],
                components: Default::default(),
            },
        ];
        doc
    }

    #[test]
    fn hierarchy_is_parent_first_and_depth_first() {
        let doc = tree();
        let entries = hierarchy(&doc);
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["Bookshelf", "ShelfA", "Crate"]);
        assert_eq!(entries[1].children, entries[1].children); // has pairs with id
        assert_eq!(entries[1].pos, Some([1.0, 2.0, 3.0]));
    }

    #[test]
    fn classic_lookups_work() {
        let doc = tree();
        let crates = find_by_name(&doc, "Crate");
        assert_eq!(crates.len(), 1);
        assert!(!find_with_component(&doc, "Transform").is_empty());
        assert!(search_paths(&doc, "crate")
            .iter()
            .any(|path| path.contains("Crate")));
    }

    #[test]
    fn snapshot_reports_stable_facts() {
        let doc = tree();
        let snap = snapshot(&doc);
        assert_eq!(snap.entity_count, 3);
        assert_eq!(snap.root_count, 1);
        assert_eq!(snap.component_usage.get("Transform"), Some(&1));
    }
}
