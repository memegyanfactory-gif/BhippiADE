//! The Scene Inspector: what the level actually contains (ADR-0056 §2).
//!
//! Everything here is answered from the parsed `.tscn` and nothing else. The checks are the
//! ones whose answer a file can settle on its own — a scene that does not parse, an instance
//! pointing at a scene that is not there, a 3D level with nothing to light it, a scene no
//! path in the project reaches. The ones a file cannot settle (is this composition any good,
//! is this the *right* light) are not here, because an inspector that guessed at those would
//! spend the user's trust on taste.

use bhippi_types::{
    InspectorId, Severity, INSPECT_CONFIDENCE_CERTAIN, INSPECT_CONFIDENCE_HEURISTIC,
};

use super::collect;
use crate::inspect::context::{InspectContext, InspectorOutput};
use crate::inspect::finding::{Finding, Location};
use crate::inspect::nodes::{entry_nodes, nodes};

/// A scene file the parser refused.
pub const CODE_SCENE_PARSE: &str = "BHP-INS-101";
/// A node instances a scene that is not on disk.
pub const CODE_INSTANCE_MISSING: &str = "BHP-INS-102";
/// The playable scene set has no camera at all.
pub const CODE_NO_CAMERA: &str = "BHP-INS-103";
/// A 3D scene with visible geometry and nothing to light it.
pub const CODE_NO_LIGHT: &str = "BHP-INS-104";
/// Two siblings of the same type at the same transform.
pub const CODE_DUPLICATE_ACTOR: &str = "BHP-INS-105";
/// A scene nothing in the project reaches.
pub const CODE_ORPHAN_SCENE: &str = "BHP-INS-106";
/// A top-level node of a level authored invisible.
pub const CODE_INVISIBLE_NODE: &str = "BHP-INS-107";

/// Node types that put something on screen in 3D and therefore need light.
const LIT_3D_TYPES: [&str; 4] = ["MeshInstance3D", "CSGBox3D", "CSGMesh3D", "GridMap"];
/// Anything that lights a 3D scene, including an environment that carries ambient light.
const LIGHT_TYPES: [&str; 5] = [
    "DirectionalLight3D",
    "OmniLight3D",
    "SpotLight3D",
    "WorldEnvironment",
    "LightmapGI",
];

#[must_use]
pub fn inspect(context: &InspectContext<'_>) -> InspectorOutput {
    let mut findings = Vec::new();

    for entry in &context.scenes {
        if let Some(reason) = &entry.parse_error {
            collect(
                &mut findings,
                Finding::draft(
                    InspectorId::Scene,
                    CODE_SCENE_PARSE,
                    Severity::Critical,
                    INSPECT_CONFIDENCE_CERTAIN,
                    format!("{} does not parse", entry.rel),
                    Location::scene(&entry.rel),
                )
                .cause(format!(
                    "The scene file could not be read as a Godot scene: {reason}"
                ))
                .impact(
                    "Godot will refuse to open the scene, and every check that needs its \
                     contents — references, collisions, gameplay reachability — is blind to it.",
                )
                .recommend(
                    "Open the file in a text editor and repair the block the error names, or \
                     restore the last good version from the project's history.",
                )
                .evidence(reason.clone(), entry.rel.clone()),
            );
            continue;
        }

        let Some(scene) = entry.parsed() else {
            continue;
        };

        // An instance pointing at nothing: the sub-tree simply will not be there at run time.
        for (node_path, res_path) in scene.instances() {
            if context.snapshot.resolves(&res_path) {
                continue;
            }
            collect(
                &mut findings,
                Finding::draft(
                    InspectorId::Scene,
                    CODE_INSTANCE_MISSING,
                    Severity::Critical,
                    INSPECT_CONFIDENCE_CERTAIN,
                    format!("`{node_path}` instances a scene that is not on disk"),
                    Location::node(&entry.rel, &node_path),
                )
                .cause(format!(
                    "The node instances {res_path}, and no file exists at that path."
                ))
                .impact(
                    "Godot loads the parent scene with the instance missing, so everything \
                     that node was supposed to bring — its nodes, its script, its collision \
                     — is absent at run time.",
                )
                .recommend(format!(
                    "Restore {res_path}, or point the instance at the scene that replaced it."
                ))
                .evidence(
                    format!("ext_resource path={res_path}"),
                    format!("{}#{node_path}", entry.rel),
                ),
            );
        }

        // A 3D level you cannot see. `WorldEnvironment` counts: ambient light is light.
        let all = nodes(scene);
        let visible_3d: Vec<_> = all
            .iter()
            .filter(|node| node.is_type(&LIT_3D_TYPES))
            .collect();
        let has_light = all.iter().any(|node| node.is_type(&LIGHT_TYPES));
        if !visible_3d.is_empty() && !has_light {
            let first = visible_3d
                .first()
                .map(|node| node.path.to_owned())
                .unwrap_or_default();
            collect(
                &mut findings,
                Finding::draft(
                    InspectorId::Scene,
                    CODE_NO_LIGHT,
                    Severity::Medium,
                    INSPECT_CONFIDENCE_HEURISTIC,
                    format!("{} has geometry but nothing lighting it", entry.rel),
                    Location::scene(&entry.rel),
                )
                .cause(format!(
                    "{} node(s) draw geometry and the scene has no light and no \
                     WorldEnvironment.",
                    visible_3d.len()
                ))
                .impact(
                    "The level renders almost black. It is the single most common reason a \
                     newly built scene looks empty when it is not.",
                )
                .recommend(
                    "Add a DirectionalLight3D, or a WorldEnvironment with ambient light, to \
                     the scene root.",
                )
                .evidence(format!("visible geometry at `{first}`"), entry.rel.clone())
                .evidence(
                    "no DirectionalLight3D / OmniLight3D / SpotLight3D / WorldEnvironment / LightmapGI"
                        .to_owned(),
                    entry.rel.clone(),
                ),
            );
        }

        // Two identical siblings on top of each other: almost always a duplicated paste.
        for (parent, name, type_, first, second) in duplicate_siblings(entry) {
            collect(
                &mut findings,
                Finding::draft(
                    InspectorId::Scene,
                    CODE_DUPLICATE_ACTOR,
                    Severity::Low,
                    INSPECT_CONFIDENCE_HEURISTIC,
                    format!("`{first}` and `{second}` are the same {type_} in the same place"),
                    Location::node(&entry.rel, &second),
                )
                .cause(format!(
                    "Both are children of `{parent}`, both are {type_}, and both carry the \
                     same transform. The name they share the stem of is `{name}`."
                ))
                .impact(
                    "Two meshes in the same place cost twice the draw calls and z-fight; two \
                     colliders in the same place fire overlap events twice.",
                )
                .recommend(format!(
                    "Delete `{second}` if it is a duplicated paste, or move it to where it \
                     was meant to go."
                ))
                .evidence(
                    format!("identical transform on `{first}` and `{second}`"),
                    format!("{}#{second}", entry.rel),
                ),
            );
        }

        // A level's own top-level node, authored invisible.
        for node in all.iter().filter(|node| node.depth == 1) {
            if node.bool_prop("visible") == Some(false) {
                collect(
                    &mut findings,
                    Finding::draft(
                        InspectorId::Scene,
                        CODE_INVISIBLE_NODE,
                        Severity::Low,
                        INSPECT_CONFIDENCE_CERTAIN,
                        format!("`{}` is authored invisible", node.path),
                        Location::node(&entry.rel, node.path),
                    )
                    .cause(
                        "The node carries `visible = false` in the scene file, so it and \
                         everything under it start hidden.",
                    )
                    .impact(
                        "If a script is supposed to reveal it, nothing shows until that \
                         script runs; if none does, the branch never appears at all.",
                    )
                    .recommend(
                        "Set visible = true if it should be on screen, or confirm the script \
                         that reveals it actually runs.",
                    )
                    .evidence(
                        "visible = false".to_owned(),
                        format!("{}#{}", entry.rel, node.path),
                    ),
                );
            }
        }
    }

    // A camera somewhere in the playable set. Godot renders a grey void without one, and
    // `gates` only checks the *current* flag on a scene that already has one.
    if context.is_project_scope() {
        let has_camera = context.scenes.iter().any(|entry| {
            entry_nodes(entry)
                .iter()
                .any(|node| node.type_starts_with(&["Camera"]))
        });
        if !has_camera && !context.scenes.is_empty() {
            let main = context
                .snapshot
                .main_scene
                .clone()
                .unwrap_or_else(|| "project.godot".to_owned());
            collect(
                &mut findings,
                Finding::draft(
                    InspectorId::Scene,
                    CODE_NO_CAMERA,
                    Severity::High,
                    INSPECT_CONFIDENCE_CERTAIN,
                    "No scene in the project has a camera",
                    Location::scene(&main),
                )
                .cause(
                    "No Camera2D or Camera3D exists in any scene the scan read, so nothing \
                     defines the player's point of view.",
                )
                .impact("Play renders a grey void.")
                .recommend("Add a Camera3D (or Camera2D) to the main scene and mark it `current`.")
                .evidence(
                    format!(
                        "{} scene(s) read, none with a Camera node",
                        context.scenes.len()
                    ),
                    main.clone(),
                ),
            );
        }
    }

    // Scenes nothing reaches. Only meaningful over the whole project — in a level scope,
    // "nothing instances this" is what the scope asked for.
    if context.is_project_scope() {
        for entry in &context.scenes {
            if entry.vendored || entry.parse_error.is_some() {
                continue;
            }
            if context.snapshot.main_scene.as_deref() == Some(entry.rel.as_str()) {
                continue;
            }
            if reached(context, &entry.rel) {
                continue;
            }
            collect(
                &mut findings,
                Finding::draft(
                    InspectorId::Scene,
                    CODE_ORPHAN_SCENE,
                    Severity::Suggestion,
                    INSPECT_CONFIDENCE_HEURISTIC,
                    format!("Nothing in the project reaches {}", entry.rel),
                    Location::scene(&entry.rel),
                )
                .cause(
                    "No scene instances it, no script mentions its path, and it is not the \
                     main scene.",
                )
                .impact(
                    "It ships in the export and costs import time, and a change to it \
                     changes nothing a player sees. It may also be a level somebody meant \
                     to wire up and did not.",
                )
                .recommend(
                    "Wire it into the game — instance it or load it from a script — or \
                     delete it.",
                )
                .evidence(
                    "no ext_resource and no script mention".to_owned(),
                    entry.rel.clone(),
                ),
            );
        }
    }

    InspectorOutput {
        findings,
        coverage: context.scene_coverage(),
    }
}

/// True when some scene instances this one, or some script names its path.
fn reached(context: &InspectContext<'_>, rel: &str) -> bool {
    let res = format!("res://{rel}");
    let instanced = context.snapshot.scenes.iter().any(|other| {
        other.rel != rel
            && other.parsed().is_some_and(|scene| {
                scene
                    .instances()
                    .iter()
                    .any(|(_, path)| crate::godot::res_to_rel(path) == rel)
            })
    });
    instanced
        || context.snapshot.mentioned_in_scripts(&res)
        || context.snapshot.mentioned_in_scripts(rel)
}

/// `(parent, shared name stem, type, first path, second path)` for each duplicated sibling.
fn duplicate_siblings(
    entry: &crate::inspect::snapshot::SceneEntry,
) -> Vec<(String, String, String, String, String)> {
    let all = entry_nodes(entry);
    let mut out = Vec::new();
    for (index, node) in all.iter().enumerate() {
        let Some(type_) = node.type_ else { continue };
        let Some(parent) = node.raw.parent.as_deref() else {
            continue;
        };
        let Some(transform) = node
            .get("transform")
            .or_else(|| node.get("position"))
            .map(crate::godot::tscn::TscnValue::to_text)
        else {
            continue;
        };
        for other in all.iter().skip(index + 1) {
            if other.type_ != Some(type_) || other.raw.parent.as_deref() != Some(parent) {
                continue;
            }
            let same = other
                .get("transform")
                .or_else(|| other.get("position"))
                .map(crate::godot::tscn::TscnValue::to_text)
                .is_some_and(|value| value == transform);
            if same {
                out.push((
                    parent.to_owned(),
                    node.name.to_owned(),
                    type_.to_owned(),
                    node.path.to_owned(),
                    other.path.to_owned(),
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::context::test_support::{context_from, scene_entry};

    #[test]
    fn a_scene_that_does_not_parse_is_critical_and_says_why() {
        let entry = crate::inspect::snapshot::SceneEntry {
            rel: "scenes/broken.tscn".to_owned(),
            res: "res://scenes/broken.tscn".to_owned(),
            scene: None,
            parse_error: Some("unbalanced bracket on line 4".to_owned()),
            vendored: false,
        };
        let snapshot = crate::inspect::snapshot::ProjectSnapshot {
            scenes: vec![entry],
            ..Default::default()
        };
        let context = context_from(&snapshot);
        let output = inspect(&context);
        let parse = output
            .findings
            .iter()
            .find(|finding| finding.code == CODE_SCENE_PARSE)
            .expect("the parse failure is reported");
        assert_eq!(parse.severity, Severity::Critical);
        assert!(parse.cause.contains("unbalanced bracket"));
    }

    #[test]
    fn geometry_with_no_light_is_reported_and_a_world_environment_settles_it() {
        let dark = scene_entry(
            "scenes/dark.tscn",
            r#"[gd_scene format=3]

[node name="Level" type="Node3D"]

[node name="Floor" type="MeshInstance3D" parent="."]
"#,
        );
        let snapshot = crate::inspect::snapshot::ProjectSnapshot {
            scenes: vec![dark],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        assert!(output
            .findings
            .iter()
            .any(|finding| finding.code == CODE_NO_LIGHT));

        let lit = scene_entry(
            "scenes/lit.tscn",
            r#"[gd_scene format=3]

[node name="Level" type="Node3D"]

[node name="Floor" type="MeshInstance3D" parent="."]

[node name="Env" type="WorldEnvironment" parent="."]
"#,
        );
        let snapshot = crate::inspect::snapshot::ProjectSnapshot {
            scenes: vec![lit],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        assert!(!output
            .findings
            .iter()
            .any(|finding| finding.code == CODE_NO_LIGHT));
    }

    #[test]
    fn an_instance_pointing_at_a_missing_scene_is_critical_and_names_the_node() {
        let entry = scene_entry(
            "scenes/main.tscn",
            r#"[gd_scene load_steps=2 format=3]

[ext_resource type="PackedScene" path="res://scenes/door.tscn" id="1_door"]

[node name="Main" type="Node3D"]

[node name="Door" parent="." instance=ExtResource("1_door")]
"#,
        );
        let snapshot = crate::inspect::snapshot::ProjectSnapshot {
            scenes: vec![entry],
            files: ["scenes/main.tscn".to_owned()].into_iter().collect(),
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let missing = output
            .findings
            .iter()
            .find(|finding| finding.code == CODE_INSTANCE_MISSING)
            .expect("the dangling instance is reported");
        assert_eq!(missing.location.node.as_deref(), Some("Door"));
        assert_eq!(missing.severity, Severity::Critical);
    }

    #[test]
    fn two_identical_siblings_in_the_same_place_are_one_finding_not_two() {
        let entry = scene_entry(
            "scenes/main.tscn",
            r#"[gd_scene format=3]

[node name="Main" type="Node3D"]

[node name="Crate" type="MeshInstance3D" parent="."]
transform = Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 2, 0, 3)

[node name="Crate2" type="MeshInstance3D" parent="."]
transform = Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 2, 0, 3)
"#,
        );
        let snapshot = crate::inspect::snapshot::ProjectSnapshot {
            scenes: vec![entry],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let duplicates: Vec<_> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_DUPLICATE_ACTOR)
            .collect();
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].location.node.as_deref(), Some("Crate2"));
    }

    #[test]
    fn a_scene_the_main_scene_instances_is_not_an_orphan() {
        let main = scene_entry(
            "scenes/main.tscn",
            r#"[gd_scene load_steps=2 format=3]

[ext_resource type="PackedScene" path="res://scenes/door.tscn" id="1_door"]

[node name="Main" type="Node3D"]

[node name="Door" parent="." instance=ExtResource("1_door")]
"#,
        );
        let door = scene_entry(
            "scenes/door.tscn",
            "[gd_scene format=3]\n\n[node name=\"Door\" type=\"Node3D\"]\n",
        );
        let lonely = scene_entry(
            "scenes/unused.tscn",
            "[gd_scene format=3]\n\n[node name=\"Unused\" type=\"Node3D\"]\n",
        );
        let snapshot = crate::inspect::snapshot::ProjectSnapshot {
            main_scene: Some("scenes/main.tscn".to_owned()),
            files: [
                "scenes/main.tscn".to_owned(),
                "scenes/door.tscn".to_owned(),
                "scenes/unused.tscn".to_owned(),
            ]
            .into_iter()
            .collect(),
            scenes: vec![main, door, lonely],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let orphans: Vec<&str> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_ORPHAN_SCENE)
            .filter_map(|finding| finding.location.scene.as_deref())
            .collect();
        assert_eq!(orphans, vec!["scenes/unused.tscn"]);
    }
}
