//! Deterministic semantic inspectors for `/gamedebug` stage 06.
//!
//! These checks reason over parsed engine documents and compiled bytecode. They never ask a
//! model to infer whether a game is wired correctly, and they deliberately report only facts
//! that the current authored graph can prove.

use crate::document::SceneDocument;
use crate::manifest::GameManifest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticSeverity {
    Blocker,
    Warning,
}

impl SemanticSeverity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blocker => "blocker",
            Self::Warning => "warning",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SemanticFinding {
    pub code: String,
    pub severity: SemanticSeverity,
    pub address: String,
    pub observed: String,
    pub expected: String,
    pub repair: String,
}

/// Inspect the semantic connections between otherwise valid authored documents.
///
/// The inputs are already parsed by stages 01–03. Invalid documents stay owned by those
/// stages, avoiding duplicate parser findings here.
#[must_use]
pub fn inspect(
    manifest: &GameManifest,
    scenes: &[(String, SceneDocument)],
) -> Vec<SemanticFinding> {
    let mut findings = Vec::new();
    inspect_level_reachability(manifest, scenes, &mut findings);
    inspect_play_entry(manifest, scenes, &mut findings);
    inspect_objectives_and_keys(scenes, &mut findings);
    findings.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then_with(|| left.address.cmp(&right.address))
    });
    findings
}

fn inspect_level_reachability(
    manifest: &GameManifest,
    scenes: &[(String, SceneDocument)],
    findings: &mut Vec<SemanticFinding>,
) {
    let Some((_, main)) = scenes
        .iter()
        .find(|(path, _)| path == &manifest.game.default_scene)
    else {
        return;
    };
    for level in &manifest.game.levels {
        if !main.settings.levels.iter().any(|path| path == level) {
            findings.push(semantic_finding(
                "BHP-GD-301",
                SemanticSeverity::Warning,
                &format!("{}#settings.levels", manifest.game.default_scene),
                &format!("Registered level {level} is not reachable from the default Main scene."),
                "Every manifest level should be reachable through the Main scene's ordered level list.",
                &format!("Add {level} to settings.levels, or remove it from the manifest if it is intentionally unused."),
            ));
        }
    }
}

fn inspect_play_entry(
    manifest: &GameManifest,
    scenes: &[(String, SceneDocument)],
    findings: &mut Vec<SemanticFinding>,
) {
    let playable = playable_scenes(manifest, scenes);
    let has_spawn = playable
        .iter()
        .flat_map(|(_, scene)| &scene.entities)
        .any(|entity| {
            entity.name.eq_ignore_ascii_case("PlayerStart")
                || entity.tags.iter().any(|tag| tag == "spawn")
                || entity.components.contains_key("CharacterController")
        });
    if !has_spawn {
        findings.push(semantic_finding(
            "BHP-GD-302",
            SemanticSeverity::Blocker,
            "gameplay://player-start",
            "No PlayerStart, spawn-tagged entity or CharacterController exists in the playable scene set.",
            "A playable game needs a deterministic player start or an authored possessed controller.",
            "Add a PlayerStart or a CharacterController entity to Main or a registered level.",
        ));
    }

    let has_camera = playable
        .iter()
        .flat_map(|(_, scene)| &scene.entities)
        .any(|entity| entity.components.contains_key("Camera"));
    if !has_camera {
        findings.push(semantic_finding(
            "BHP-GD-303",
            SemanticSeverity::Blocker,
            "gameplay://camera",
            "No Camera component exists in the playable scene set.",
            "Play needs an authored camera that can be selected or possessed.",
            "Add a Camera entity to Main or a registered level and verify it in Play.",
        ));
    }
}

fn inspect_objectives_and_keys(
    scenes: &[(String, SceneDocument)],
    findings: &mut Vec<SemanticFinding>,
) {
    let identities = scenes
        .iter()
        .flat_map(|(_, scene)| &scene.entities)
        .flat_map(|entity| {
            std::iter::once(entity.id.to_string())
                .chain(std::iter::once(entity.name.clone()))
                .chain(entity.tags.iter().cloned())
        })
        .collect::<BTreeSet<_>>();

    for (path, scene) in scenes {
        for entity in &scene.entities {
            if let Some(objective) = entity
                .components
                .get("Objective")
                .or_else(|| entity.components.get("ObjectiveDefinition"))
            {
                let completion = objective
                    .get("completion_event")
                    .or_else(|| objective.get("complete_event"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                if completion.trim().is_empty() {
                    findings.push(semantic_finding(
                        "BHP-GD-306",
                        SemanticSeverity::Blocker,
                        &format!("{path}#entity/{}/Objective", entity.id),
                        &format!("Objective {:?} has no completion event.", entity.name),
                        "Every authored objective needs a deterministic event that can complete it.",
                        "Set completion_event to a real gameplay event and add it to the scenario assertions.",
                    ));
                }
            }

            if let Some(door) = entity.components.get("Door") {
                let required = door
                    .get("required_key")
                    .or_else(|| door.get("key_id"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                if !required.trim().is_empty() && !identities.contains(required) {
                    findings.push(semantic_finding(
                        "BHP-GD-307",
                        SemanticSeverity::Blocker,
                        &format!("{path}#entity/{}/Door", entity.id),
                        &format!("Door {:?} requires key {required:?}, but no entity id, name or tag matches it.", entity.name),
                        "A locked door dependency must resolve to an authored key identity.",
                        "Create the required key or update required_key to an existing id, name or tag.",
                    ));
                }
            }
        }
    }
}

fn playable_scenes<'a>(
    manifest: &GameManifest,
    scenes: &'a [(String, SceneDocument)],
) -> Vec<&'a (String, SceneDocument)> {
    scenes
        .iter()
        .filter(|(path, _)| {
            path == &manifest.game.default_scene
                || manifest.game.levels.iter().any(|level| level == path)
        })
        .collect()
}

fn semantic_finding(
    code: &str,
    severity: SemanticSeverity,
    address: &str,
    observed: &str,
    expected: &str,
    repair: &str,
) -> SemanticFinding {
    SemanticFinding {
        code: code.to_owned(),
        severity,
        address: address.to_owned(),
        observed: observed.to_owned(),
        expected: expected.to_owned(),
        repair: repair.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::inspect;
    use crate::document::{Entity, SceneDocument, SceneKind};
    use crate::manifest::GameManifest;
    use bhippi_types::EntityId;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};

    fn entity(
        name: &str,
        tags: &[&str],
        components: BTreeMap<String, serde_json::Value>,
    ) -> Entity {
        Entity {
            id: EntityId::new(),
            name: name.to_owned(),
            parent: None,
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            components,
        }
    }

    #[test]
    fn a_connected_game_has_no_entry_or_reachability_blocker() {
        let manifest = GameManifest::defaults("Connected");
        let mut main = SceneDocument::empty("main");
        main.settings.kind = SceneKind::Main;
        main.settings.levels = manifest.game.levels.clone();
        main.entities
            .push(entity("PlayerStart", &["spawn"], BTreeMap::new()));
        main.entities.push(entity(
            "Camera",
            &["camera"],
            BTreeMap::from([("Camera".to_owned(), json!({}))]),
        ));
        let level = SceneDocument::empty("level_01");
        let scenes = vec![
            (manifest.game.default_scene.clone(), main),
            (manifest.game.levels[0].clone(), level),
        ];
        let findings = inspect(&manifest, &scenes);
        assert!(!findings.iter().any(|finding| matches!(
            finding.code.as_str(),
            "BHP-GD-301" | "BHP-GD-302" | "BHP-GD-303"
        )));
    }

    #[test]
    fn seeded_semantic_defects_have_stable_codes_and_addresses() {
        let manifest = GameManifest::defaults("Broken");
        let mut main = SceneDocument::empty("main");
        main.settings.kind = SceneKind::Main;
        main.entities.push(entity(
            "Objective",
            &[],
            BTreeMap::from([("Objective".to_owned(), json!({"required": true}))]),
        ));
        main.entities.push(entity(
            "LockedDoor",
            &[],
            BTreeMap::from([("Door".to_owned(), json!({"required_key": "missing-key"}))]),
        ));
        let scenes = vec![(manifest.game.default_scene.clone(), main)];
        let findings = inspect(&manifest, &scenes);
        let codes = findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect::<BTreeSet<_>>();
        for expected in [
            "BHP-GD-301",
            "BHP-GD-302",
            "BHP-GD-303",
            "BHP-GD-306",
            "BHP-GD-307",
        ] {
            assert!(codes.contains(expected), "missing {expected}: {findings:?}");
        }
        assert!(findings.iter().all(|finding| !finding.address.is_empty()));
    }
}
