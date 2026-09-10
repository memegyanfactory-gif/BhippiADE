//! The Animation Inspector: players, trees and the transitions between states (ADR-0056 §2).
//!
//! Three of Godot's animation failures are visible in the scene file: a player with no
//! library, an `autoplay` naming an animation nothing in the file provides, and a state
//! machine transition with no cross-fade — the last of which is the "pop" every animator
//! recognises and no tool ever reports.
//!
//! What is *not* here: skeleton compatibility, root motion correctness and retargeting all
//! live inside binary `.res`/`.glb` payloads this crate does not decode. An inspector that
//! guessed at them from a scene file would be inventing.

use bhippi_types::{
    FixRisk, InspectorId, Severity, INSPECT_BLEND_TIME_FLOOR, INSPECT_CONFIDENCE_CERTAIN,
    INSPECT_CONFIDENCE_HEURISTIC,
};

use super::collect;
use crate::godot::action::GodotAction;
use crate::godot::scene::GodotScene;
use crate::godot::tscn::TscnValue;
use crate::inspect::context::{InspectContext, InspectorOutput};
use crate::inspect::finding::{Finding, Location};
use crate::inspect::fix::ProposedFix;
use crate::inspect::nodes::{nodes, NodeRef};

/// An `AnimationPlayer` with no library at all.
pub const CODE_NO_LIBRARY: &str = "BHP-INS-701";
/// An `autoplay` naming an animation the file does not provide.
pub const CODE_AUTOPLAY_MISSING: &str = "BHP-INS-702";
/// A state machine transition with no cross-fade.
pub const CODE_INSTANT_TRANSITION: &str = "BHP-INS-703";
/// An `AnimationTree` authored inactive.
pub const CODE_TREE_INACTIVE: &str = "BHP-INS-704";

/// The sub-resource type that carries a state machine's blend time.
const TRANSITION_TYPE: &str = "AnimationNodeStateMachineTransition";

/// The upper end of the range a transition should normally land in.
const BLEND_TIME_CEILING: f64 = 0.25;

#[must_use]
pub fn inspect(context: &InspectContext<'_>) -> InspectorOutput {
    let mut findings = Vec::new();

    for entry in &context.scenes {
        let Some(scene) = entry.parsed() else {
            continue;
        };
        for node in nodes(scene) {
            no_library(entry.rel.as_str(), &node, &mut findings);
            autoplay_missing(entry.rel.as_str(), scene, &node, &mut findings);
            tree_inactive(entry.rel.as_str(), &node, &mut findings);
        }
        instant_transitions(entry.rel.as_str(), scene, &mut findings);
    }

    InspectorOutput {
        findings,
        coverage: context.scene_coverage(),
    }
}

fn no_library(scene_rel: &str, node: &NodeRef<'_>, findings: &mut Vec<Finding>) {
    if !node.is_type(&["AnimationPlayer"]) {
        return;
    }
    if node.get("libraries").is_some() || node.get("anims").is_some() {
        return;
    }
    collect(
        findings,
        Finding::draft(
            InspectorId::Animation,
            CODE_NO_LIBRARY,
            Severity::Medium,
            INSPECT_CONFIDENCE_HEURISTIC,
            format!("`{}` has no animation library", node.path),
            Location::node(scene_rel, node.path),
        )
        .cause(
            "The AnimationPlayer carries neither a `libraries` nor an `anims` property, so \
             the scene provides it with nothing to play.",
        )
        .impact(
            "Every `play(\"…\")` call against it fails at run time with \"animation not \
             found\", and the character simply stands still.",
        )
        .recommend(
            "Add the animation library the player is meant to use, or remove the player if \
             a script assigns one at run time — and check that script if so.",
        )
        .evidence(
            "no `libraries` or `anims` property".to_owned(),
            format!("{scene_rel}#{}", node.path),
        ),
    );
}

fn autoplay_missing(
    scene_rel: &str,
    scene: &GodotScene,
    node: &NodeRef<'_>,
    findings: &mut Vec<Finding>,
) {
    if !node.is_type(&["AnimationPlayer"]) {
        return;
    }
    let Some(autoplay) = node.str_prop("autoplay") else {
        return;
    };
    if autoplay.is_empty() || provides_animation(scene, autoplay) {
        return;
    }
    collect(
        findings,
        Finding::draft(
            InspectorId::Animation,
            CODE_AUTOPLAY_MISSING,
            Severity::High,
            INSPECT_CONFIDENCE_HEURISTIC,
            format!(
                "`{}` autoplays `{autoplay}`, which it does not have",
                node.path
            ),
            Location::node(scene_rel, node.path).with_symbol(autoplay),
        )
        .cause(format!(
            "`autoplay = \"{autoplay}\"` and nothing in this scene's resources declares an \
             animation by that name."
        ))
        .impact(
            "Godot logs \"animation not found\" once at start-up and the node plays nothing \
             — for the whole session, with no further sign anything is wrong.",
        )
        .recommend(format!(
            "Point autoplay at an animation the library actually has, or add `{autoplay}` to \
             it. If the library is loaded at run time, clear autoplay and start it from code."
        ))
        .evidence(
            format!("autoplay = \"{autoplay}\""),
            format!("{scene_rel}#{}", node.path),
        ),
    );
}

/// True when some resource in this file appears to declare an animation by that name.
///
/// Godot stores an `AnimationLibrary`'s contents in a `_data` dictionary whose keys are the
/// animation names; the parser keeps unknown dictionary shapes verbatim, so a quoted name in
/// the sub-resource text is the honest test available here — hence the heuristic confidence
/// on the finding that uses it.
fn provides_animation(scene: &GodotScene, name: &str) -> bool {
    let quoted = format!("\"{name}\"");
    let in_sub_resources = scene.document.sub_resources.iter().any(|resource| {
        resource
            .properties
            .iter()
            .any(|(_, value)| value.to_text().contains(&quoted))
    });
    // A library loaded from a separate file: the scan cannot see inside it, so an external
    // library means "cannot rule it out" and the check stays quiet.
    in_sub_resources
        || scene
            .document
            .ext_resources
            .iter()
            .any(|resource| resource.type_ == "AnimationLibrary" || resource.type_ == "Animation")
}

fn tree_inactive(scene_rel: &str, node: &NodeRef<'_>, findings: &mut Vec<Finding>) {
    if !node.is_type(&["AnimationTree"]) {
        return;
    }
    if node.bool_prop("active") != Some(false) {
        return;
    }
    let mut draft = Finding::draft(
        InspectorId::Animation,
        CODE_TREE_INACTIVE,
        Severity::Medium,
        INSPECT_CONFIDENCE_CERTAIN,
        format!("`{}` is switched off", node.path),
        Location::node(scene_rel, node.path),
    )
    .cause("`active = false`, so the animation tree does not evaluate at all.")
    .impact(
        "None of the states, blends or transitions authored in it run. The character holds \
         whatever pose its rest position is.",
    )
    .recommend(
        "Set active = true, unless a script enables it deliberately — in which case confirm \
         that script runs.",
    )
    .evidence(
        "active = false".to_owned(),
        format!("{scene_rel}#{}", node.path),
    );

    match ProposedFix::new(
        CODE_TREE_INACTIVE,
        format!("Activate `{}`", node.path),
        FixRisk::Low,
        vec![GodotAction::SetProperty {
            scene: scene_rel.to_owned(),
            path: node.path.to_owned(),
            property: "active".to_owned(),
            value: TscnValue::Bool(true),
        }],
    ) {
        Ok(fix) => draft = draft.fix(fix),
        Err(error) => tracing::error!(%error, "the animation tree fix could not be described"),
    }
    collect(findings, draft);
}

/// A transition that snaps instead of blending.
fn instant_transitions(scene_rel: &str, scene: &GodotScene, findings: &mut Vec<Finding>) {
    for resource in &scene.document.sub_resources {
        if resource.type_ != TRANSITION_TYPE {
            continue;
        }
        let xfade = resource
            .properties
            .iter()
            .find(|(name, _)| name == "xfade_time")
            .map(|(_, value)| match value {
                TscnValue::Float(seconds) => *seconds,
                #[allow(clippy::cast_precision_loss)]
                TscnValue::Int(seconds) => *seconds as f64,
                _ => 0.0,
            })
            .unwrap_or(0.0);
        if xfade >= INSPECT_BLEND_TIME_FLOOR {
            continue;
        }
        collect(
            findings,
            Finding::draft(
                InspectorId::Animation,
                CODE_INSTANT_TRANSITION,
                Severity::Low,
                INSPECT_CONFIDENCE_CERTAIN,
                format!("Transition `{}` blends in {xfade}s", resource.id),
                Location::scene(scene_rel).with_symbol(resource.id.clone()),
            )
            .cause(format!(
                "`xfade_time` is {xfade}, under the {INSPECT_BLEND_TIME_FLOOR}s a state \
                 change needs to read as a movement rather than a cut."
            ))
            .impact(
                "The character's pose changes between one frame and the next. It is the \
                 visual pop that makes otherwise good animation look cheap.",
            )
            .recommend(format!(
                "Set xfade_time between {INSPECT_BLEND_TIME_FLOOR} and {BLEND_TIME_CEILING} \
                 seconds, unless the cut is deliberate — a hit reaction, for instance."
            ))
            .evidence(
                format!("xfade_time = {xfade}"),
                format!("{scene_rel}#sub_resource {}", resource.id),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::context::test_support::{context_from, scene_entry};
    use crate::inspect::snapshot::ProjectSnapshot;

    fn from_scene(text: &str) -> Vec<Finding> {
        let snapshot = ProjectSnapshot {
            scenes: vec![scene_entry("scenes/player.tscn", text)],
            ..Default::default()
        };
        inspect(&context_from(&snapshot)).findings
    }

    #[test]
    fn a_transition_that_snaps_is_reported_and_one_that_blends_is_not() {
        let findings = from_scene(
            r#"[gd_scene load_steps=3 format=3]

[sub_resource type="AnimationNodeStateMachineTransition" id="T_run_idle"]
xfade_time = 0.0

[sub_resource type="AnimationNodeStateMachineTransition" id="T_idle_run"]
xfade_time = 0.2

[node name="Player" type="Node3D"]
"#,
        );
        let popping: Vec<&str> = findings
            .iter()
            .filter(|finding| finding.code == CODE_INSTANT_TRANSITION)
            .filter_map(|finding| finding.location.symbol.as_deref())
            .collect();
        assert_eq!(popping, vec!["T_run_idle"]);
        let finding = findings
            .iter()
            .find(|finding| finding.code == CODE_INSTANT_TRANSITION)
            .expect("the snap is reported");
        assert!(finding.recommendation.contains("0.15"));
    }

    #[test]
    fn a_transition_with_no_xfade_property_at_all_defaults_to_snapping() {
        let findings = from_scene(
            r#"[gd_scene load_steps=2 format=3]

[sub_resource type="AnimationNodeStateMachineTransition" id="T_plain"]

[node name="Player" type="Node3D"]
"#,
        );
        assert!(findings
            .iter()
            .any(|finding| finding.code == CODE_INSTANT_TRANSITION));
    }

    #[test]
    fn an_animation_player_with_no_library_is_reported() {
        let findings = from_scene(
            r#"[gd_scene format=3]

[node name="Player" type="Node3D"]

[node name="AnimationPlayer" type="AnimationPlayer" parent="."]
"#,
        );
        assert!(findings
            .iter()
            .any(|finding| finding.code == CODE_NO_LIBRARY));
    }

    #[test]
    fn an_autoplay_the_file_provides_is_silent_and_one_it_does_not_is_high() {
        let missing = from_scene(
            r#"[gd_scene load_steps=2 format=3]

[sub_resource type="AnimationLibrary" id="Lib_1"]
_data = {
"idle": SubResource("Anim_idle")
}

[node name="Player" type="Node3D"]

[node name="AnimationPlayer" type="AnimationPlayer" parent="."]
libraries = {
"": SubResource("Lib_1")
}
autoplay = "walk"
"#,
        );
        let autoplay = missing
            .iter()
            .find(|finding| finding.code == CODE_AUTOPLAY_MISSING)
            .expect("the missing animation is reported");
        assert_eq!(autoplay.severity, Severity::High);
        assert_eq!(autoplay.location.symbol.as_deref(), Some("walk"));

        let present = from_scene(
            r#"[gd_scene load_steps=2 format=3]

[sub_resource type="AnimationLibrary" id="Lib_1"]
_data = {
"idle": SubResource("Anim_idle")
}

[node name="Player" type="Node3D"]

[node name="AnimationPlayer" type="AnimationPlayer" parent="."]
libraries = {
"": SubResource("Lib_1")
}
autoplay = "idle"
"#,
        );
        assert!(!present
            .iter()
            .any(|finding| finding.code == CODE_AUTOPLAY_MISSING));
    }

    #[test]
    fn an_inactive_animation_tree_offers_the_fix_that_switches_it_on() {
        let findings = from_scene(
            r#"[gd_scene format=3]

[node name="Player" type="Node3D"]

[node name="AnimationTree" type="AnimationTree" parent="."]
active = false
"#,
        );
        let inactive = findings
            .iter()
            .find(|finding| finding.code == CODE_TREE_INACTIVE)
            .expect("the inactive tree is reported");
        assert!(inactive.fix.is_some());
    }
}
