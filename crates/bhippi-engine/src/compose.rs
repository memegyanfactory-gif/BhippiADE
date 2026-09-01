//! Play composition (ENG-105 / ENG-170 first half).
//!
//! Pressing Play on **Main** runs the whole game: Main is the persistent scene, one level
//! is the playable map, and the HUD overlays both. Composing those three used to happen in
//! `EngineSceneDocument.ts`, which prefixed ids with the layer name (`level_01JD…`) —
//! producing documents the Rust parser rejects, so a composed world could never round-trip
//! or be inspected. Here the merge is deterministic and id-correct: every borrowed entity
//! gets a fresh ULID derived from (source scene, source entity), so composing twice yields
//! the same world and parent links survive the remap.

use crate::document::{Entity, SceneDocument};
use crate::error::Result;
use bhippi_types::{EntityId, SceneId};
use std::collections::BTreeMap;
use std::str::FromStr;

/// The layer an entity came from, written as a tag so play-mode systems can tell the
/// persistent scene from the map.
///
/// There is no `hud` layer any more: since ENG-139 the HUD is a `bhippi-hud@1` document
/// drawn as a 2D overlay, not a set of 3D entities merged into the world.
pub const LAYER_TAGS: [&str; 2] = ["main", "level"];

/// A deterministic id for `entity` as borrowed into a composed world from `scene`.
fn composed_id(scene: SceneId, entity: EntityId) -> EntityId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"bhippi-compose@1");
    hasher.update(scene.to_string().as_bytes());
    hasher.update(entity.to_string().as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    EntityId::from_str(&ulid::Ulid::from_bytes(bytes).to_string()).unwrap_or(entity)
}

/// Merge `source`'s entities into `target`, tagging them `layer` and remapping ids.
fn merge_layer(target: &mut SceneDocument, source: &SceneDocument, layer: &str) {
    let remap: BTreeMap<EntityId, EntityId> = source
        .entities
        .iter()
        .map(|entity| (entity.id, composed_id(source.id, entity.id)))
        .collect();
    for entity in &source.entities {
        let Some(id) = remap.get(&entity.id).copied() else {
            continue;
        };
        let mut tags = entity.tags.clone();
        if !tags.iter().any(|tag| tag == layer) {
            tags.push(layer.to_owned());
        }
        target.entities.push(Entity {
            id,
            name: entity.name.clone(),
            parent: entity.parent.and_then(|parent| remap.get(&parent).copied()),
            tags,
            components: entity.components.clone(),
        });
    }
}

/// Build the world Play runs.
///
/// `main` is the persistent scene (may be `None` when playing a level directly) and `level`
/// is the map. The HUD is **not** merged here — it is a separate document the renderer draws
/// over the top, which is what lets it be edited as a HUD rather than as 3D entities.
///
/// The result keeps `main`'s settings — weather, ambient and skybox belong to the game —
/// except that a level's own weather wins for that level, which is what "each map has its
/// own sky" means.
pub fn compose_play(
    main: Option<&SceneDocument>,
    level: Option<&SceneDocument>,
) -> Result<SceneDocument> {
    let mut world = match main {
        Some(main) => {
            let mut world = SceneDocument::empty(format!("{} (play)", main.name));
            world.settings = main.settings.clone();
            merge_layer(&mut world, main, "main");
            world
        }
        None => {
            let name = level.map_or_else(|| "play".to_owned(), |level| level.name.clone());
            SceneDocument::empty(format!("{name} (play)"))
        }
    };
    if let Some(level) = level {
        if main.is_none() {
            world.settings = level.settings.clone();
        } else if level.settings.weather.is_some() {
            world.settings.weather.clone_from(&level.settings.weather);
            world.settings.ambient = level.settings.ambient;
        }
        merge_layer(&mut world, level, "level");
    }
    world.validate()?;
    Ok(world)
}

#[cfg(test)]
mod tests {
    use super::{compose_play, LAYER_TAGS};
    use crate::document::{Entity, SceneDocument, SceneKind};
    use bhippi_types::EntityId;
    use serde_json::json;

    fn scene(name: &str, kind: SceneKind, entities: &[(&str, Option<usize>)]) -> SceneDocument {
        let mut doc = SceneDocument::empty(name);
        doc.settings.kind = kind;
        let ids: Vec<EntityId> = entities.iter().map(|_| EntityId::new()).collect();
        for (index, (entity_name, parent)) in entities.iter().enumerate() {
            doc.entities.push(Entity {
                id: ids[index],
                name: (*entity_name).to_owned(),
                parent: parent.map(|at| ids[at]),
                tags: vec![],
                components: std::collections::BTreeMap::from([(
                    "Transform".to_owned(),
                    json!({ "pos": [0.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0, 1.0], "scale": [1.0, 1.0, 1.0] }),
                )]),
            });
        }
        doc
    }

    #[test]
    fn play_on_main_composes_main_and_level_into_one_valid_world() {
        let main = scene("main", SceneKind::Main, &[("GameCamera", None)]);
        let level = scene(
            "level_01",
            SceneKind::Level,
            &[("Floor", None), ("Crate", Some(0))],
        );

        let world = compose_play(Some(&main), Some(&level)).expect("composes");

        assert_eq!(world.entity_count(), 3);
        // Every borrowed entity is tagged with the layer it came from.
        for tag in LAYER_TAGS {
            assert!(
                world
                    .entities
                    .iter()
                    .any(|e| e.tags.iter().any(|t| t == tag)),
                "missing layer {tag}"
            );
        }
        // And the composed document is a real scene: unique ULIDs, parents resolvable.
        world.validate().expect("valid");
        let text = world.dump().expect("dump");
        SceneDocument::parse(&text).expect("a composed world round-trips through the parser");
    }

    #[test]
    fn parent_links_survive_the_remap() {
        let level = scene(
            "level_01",
            SceneKind::Level,
            &[("Floor", None), ("Crate", Some(0))],
        );
        let world = compose_play(None, Some(&level)).expect("composes");
        let floor = world
            .entities
            .iter()
            .find(|e| e.name == "Floor")
            .expect("floor");
        let crate_entity = world
            .entities
            .iter()
            .find(|e| e.name == "Crate")
            .expect("crate");
        assert_eq!(crate_entity.parent, Some(floor.id));
        assert_ne!(crate_entity.id, floor.id);
    }

    #[test]
    fn composition_is_deterministic() {
        let main = scene("main", SceneKind::Main, &[("GameCamera", None)]);
        let level = scene("level_01", SceneKind::Level, &[("Floor", None)]);
        let first = compose_play(Some(&main), Some(&level)).expect("composes");
        let second = compose_play(Some(&main), Some(&level)).expect("composes");
        let ids = |doc: &SceneDocument| {
            doc.entities
                .iter()
                .map(|e| e.id.to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&first), ids(&second), "same inputs, same world");
    }

    #[test]
    fn a_levels_own_weather_wins_over_mains() {
        let mut main = scene("main", SceneKind::Main, &[("GameCamera", None)]);
        main.settings.weather = Some("clear".to_owned());
        let mut level = scene("level_01", SceneKind::Level, &[("Floor", None)]);
        level.settings.weather = Some("storm".to_owned());
        level.settings.ambient = [0.1, 0.12, 0.16];

        let world = compose_play(Some(&main), Some(&level)).expect("composes");
        assert_eq!(world.settings.weather.as_deref(), Some("storm"));
        assert_eq!(world.settings.ambient, [0.1, 0.12, 0.16]);
    }

    #[test]
    fn playing_a_level_alone_composes_just_that_level() {
        let level = scene("level_01", SceneKind::Level, &[("Floor", None)]);
        let world = compose_play(None, Some(&level)).expect("composes");
        assert_eq!(world.entity_count(), 1);
        assert!(world
            .entities
            .iter()
            .any(|e| e.name == "Floor" && e.tags.iter().any(|t| t == "level")));
    }
}
