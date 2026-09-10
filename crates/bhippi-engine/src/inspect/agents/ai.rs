//! The AI / NPC Inspector: navigation and the agents that use it (ADR-0056 §2).
//!
//! Godot 4's NPC stack is `NavigationAgent` + `NavigationRegion` + a script that sets a
//! target. Remove any one of the three and the NPC stands still — no error, no warning, just
//! a guard who never patrols. All three are visible in the scene and script text, which
//! makes "this NPC cannot move, and here is which of the three is missing" a fact rather
//! than a diagnosis.
//!
//! Behaviour Trees and State Trees are Unreal's vocabulary and a Godot add-on's; when a
//! project has one, its graphs are a `.tres` this crate does not model, and this inspector
//! says nothing about them rather than guessing.

use bhippi_types::{
    InspectorId, Severity, INSPECT_CONFIDENCE_CERTAIN, INSPECT_CONFIDENCE_HEURISTIC,
};

use super::collect;
use crate::inspect::context::{InspectContext, InspectorOutput};
use crate::inspect::finding::{Finding, Location};
use crate::inspect::nodes::{nodes, NodeRef};
use crate::inspect::snapshot::SceneEntry;

/// An agent in a scene with nothing to navigate on.
pub const CODE_NO_NAVIGATION: &str = "BHP-INS-901";
/// A navigation region with no baked mesh.
pub const CODE_EMPTY_REGION: &str = "BHP-INS-902";
/// An agent nothing ever gives a destination to.
pub const CODE_NO_TARGET: &str = "BHP-INS-903";

/// The calls that give a `NavigationAgent` somewhere to go.
const TARGET_CALLS: [&str; 3] = [
    "target_position",
    "set_target_position",
    "set_target_location",
];

#[must_use]
pub fn inspect(context: &InspectContext<'_>) -> InspectorOutput {
    let mut findings = Vec::new();

    for entry in &context.scenes {
        let Some(scene) = entry.parsed() else {
            continue;
        };
        let all = nodes(scene);

        let region_in_project = context.snapshot.scenes.iter().any(|other| {
            other.parsed().is_some_and(|parsed| {
                nodes(parsed)
                    .iter()
                    .any(|node| node.type_starts_with(&["NavigationRegion"]))
            })
        });

        for node in &all {
            if node.type_starts_with(&["NavigationAgent"]) {
                if !region_in_project {
                    collect(
                        &mut findings,
                        Finding::draft(
                            InspectorId::Ai,
                            CODE_NO_NAVIGATION,
                            Severity::High,
                            INSPECT_CONFIDENCE_HEURISTIC,
                            format!("`{}` has nowhere to navigate", node.path),
                            Location::node(&entry.rel, node.path),
                        )
                        .cause(
                            "The scene has a NavigationAgent and no scene in the project has \
                             a NavigationRegion, so there is no navigation mesh for it to \
                             path across.",
                        )
                        .impact(
                            "Every path query returns the agent's own position. The NPC \
                             stands exactly where it spawned and Godot reports nothing.",
                        )
                        .recommend(
                            "Add a NavigationRegion3D (or 2D) covering the walkable ground \
                             and bake its navigation mesh.",
                        )
                        .evidence(
                            "no NavigationRegion in any scene".to_owned(),
                            format!("{}#{}", entry.rel, node.path),
                        ),
                    );
                }
                no_target(context, entry, node, &mut findings);
            }

            if node.type_starts_with(&["NavigationRegion"])
                && node.get("navigation_mesh").is_none()
                && node.get("navpoly").is_none()
            {
                collect(
                    &mut findings,
                    Finding::draft(
                        InspectorId::Ai,
                        CODE_EMPTY_REGION,
                        Severity::High,
                        INSPECT_CONFIDENCE_CERTAIN,
                        format!("`{}` has no baked navigation mesh", node.path),
                        Location::node(&entry.rel, node.path),
                    )
                    .cause(
                        "The region carries neither a `navigation_mesh` nor a `navpoly` \
                         property, so it defines no walkable surface.",
                    )
                    .impact(
                        "Agents cannot path anywhere inside it. The region looks correct in \
                         the tree, which is why this is usually found by watching an NPC \
                         refuse to move.",
                    )
                    .recommend(
                        "Bake the navigation mesh for the region, in the editor or as part \
                         of the level build.",
                    )
                    .evidence(
                        "no `navigation_mesh` / `navpoly` property".to_owned(),
                        format!("{}#{}", entry.rel, node.path),
                    ),
                );
            }
        }
    }

    InspectorOutput {
        findings,
        coverage: context.scene_coverage(),
    }
}

/// An agent no script ever gives a destination.
fn no_target(
    context: &InspectContext<'_>,
    entry: &SceneEntry,
    node: &NodeRef<'_>,
    findings: &mut Vec<Finding>,
) {
    // The whole project may set it — an autoload director, a spawner. Only silence
    // everywhere is evidence.
    let anywhere = context
        .snapshot
        .scripts
        .iter()
        .any(|script| TARGET_CALLS.iter().any(|call| script.source.contains(call)));
    if anywhere {
        return;
    }
    collect(
        findings,
        Finding::draft(
            InspectorId::Ai,
            CODE_NO_TARGET,
            Severity::Medium,
            INSPECT_CONFIDENCE_HEURISTIC,
            format!("Nothing gives `{}` a destination", node.path),
            Location::node(&entry.rel, node.path),
        )
        .cause(
            "No script in the project sets `target_position` on any navigation agent, so \
             the agent's destination stays at its default.",
        )
        .impact(
            "The agent computes a path to where it already is, every frame, and the NPC \
             never moves.",
        )
        .recommend(
            "Set target_position on the agent from the script that decides where the NPC \
             should go — a patrol point, the player, a waypoint.",
        )
        .evidence(
            "no script sets target_position".to_owned(),
            format!("{}#{}", entry.rel, node.path),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::context::test_support::{context_from, scene_entry, script_entry};
    use crate::inspect::snapshot::ProjectSnapshot;

    #[test]
    fn an_agent_with_no_region_anywhere_is_reported_and_one_with_a_region_is_not() {
        let stranded = ProjectSnapshot {
            scenes: vec![scene_entry(
                "scenes/guard.tscn",
                "[gd_scene format=3]\n\n[node name=\"Guard\" type=\"CharacterBody3D\"]\n\n[node name=\"Nav\" type=\"NavigationAgent3D\" parent=\".\"]\n",
            )],
            ..Default::default()
        };
        assert!(inspect(&context_from(&stranded))
            .findings
            .iter()
            .any(|finding| finding.code == CODE_NO_NAVIGATION));

        let navigable = ProjectSnapshot {
            scenes: vec![
                scene_entry(
                    "scenes/guard.tscn",
                    "[gd_scene format=3]\n\n[node name=\"Guard\" type=\"CharacterBody3D\"]\n\n[node name=\"Nav\" type=\"NavigationAgent3D\" parent=\".\"]\n",
                ),
                scene_entry(
                    "scenes/level.tscn",
                    "[gd_scene format=3]\n\n[node name=\"Level\" type=\"Node3D\"]\n\n[node name=\"Region\" type=\"NavigationRegion3D\" parent=\".\"]\nnavigation_mesh = SubResource(\"NavigationMesh_1\")\n",
                ),
            ],
            ..Default::default()
        };
        assert!(!inspect(&context_from(&navigable))
            .findings
            .iter()
            .any(|finding| finding.code == CODE_NO_NAVIGATION));
    }

    #[test]
    fn an_unbaked_region_is_its_own_finding() {
        let snapshot = ProjectSnapshot {
            scenes: vec![scene_entry(
                "scenes/level.tscn",
                "[gd_scene format=3]\n\n[node name=\"Level\" type=\"Node3D\"]\n\n[node name=\"Region\" type=\"NavigationRegion3D\" parent=\".\"]\n",
            )],
            ..Default::default()
        };
        let findings = inspect(&context_from(&snapshot)).findings;
        assert!(findings
            .iter()
            .any(|finding| finding.code == CODE_EMPTY_REGION));
    }

    #[test]
    fn an_agent_a_script_steers_is_not_reported_as_targetless() {
        let snapshot = ProjectSnapshot {
            scenes: vec![
                scene_entry(
                    "scenes/guard.tscn",
                    "[gd_scene format=3]\n\n[node name=\"Guard\" type=\"CharacterBody3D\"]\n\n[node name=\"Nav\" type=\"NavigationAgent3D\" parent=\".\"]\n",
                ),
                scene_entry(
                    "scenes/level.tscn",
                    "[gd_scene format=3]\n\n[node name=\"Level\" type=\"Node3D\"]\n\n[node name=\"Region\" type=\"NavigationRegion3D\" parent=\".\"]\nnavigation_mesh = SubResource(\"NavigationMesh_1\")\n",
                ),
            ],
            scripts: vec![script_entry(
                "scripts/guard.gd",
                "extends CharacterBody3D\n\nfunc patrol(to):\n\t$Nav.target_position = to\n",
            )],
            ..Default::default()
        };
        let findings = inspect(&context_from(&snapshot)).findings;
        assert!(!findings
            .iter()
            .any(|finding| finding.code == CODE_NO_TARGET));
    }
}
