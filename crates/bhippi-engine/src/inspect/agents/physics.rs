//! The Physics Inspector: things that look solid and are not (ADR-0056 §2).
//!
//! Godot's physics failures are quiet by design — a body with no collider still moves, still
//! renders, still reports a position. It just passes through the floor. Every check here is
//! one of those: authored state that says *this participates in physics* contradicted by
//! authored state that says *and it has nothing to collide with*.

use bhippi_types::{FixRisk, InspectorId, Severity, INSPECT_CONFIDENCE_CERTAIN};

use super::collect;
use crate::godot::action::GodotAction;
use crate::godot::tscn::TscnValue;
use crate::inspect::context::{InspectContext, InspectorOutput};
use crate::inspect::finding::{Finding, Location};
use crate::inspect::fix::ProposedFix;
use crate::inspect::nodes::{nodes, NodeRef};

/// A physics body or area with no collision shape under it.
pub const CODE_NO_COLLIDER: &str = "BHP-INS-801";
/// A collision shape with no shape resource.
pub const CODE_EMPTY_SHAPE: &str = "BHP-INS-802";
/// An area authored not to monitor anything.
pub const CODE_NOT_MONITORING: &str = "BHP-INS-803";
/// A body on no layer and watching no layer.
pub const CODE_NO_LAYERS: &str = "BHP-INS-804";
/// A rigid body frozen in the scene.
pub const CODE_FROZEN_BODY: &str = "BHP-INS-805";

/// The families that need a collider under them.
const PHYSICAL_SUFFIXES: [&str; 8] = [
    "StaticBody2D",
    "StaticBody3D",
    "RigidBody2D",
    "RigidBody3D",
    "CharacterBody2D",
    "CharacterBody3D",
    "Area2D",
    "Area3D",
];

#[must_use]
pub fn inspect(context: &InspectContext<'_>) -> InspectorOutput {
    let mut findings = Vec::new();

    for entry in &context.scenes {
        let Some(scene) = entry.parsed() else {
            continue;
        };
        let all = nodes(scene);
        for node in &all {
            no_collider(entry.rel.as_str(), node, &all, &mut findings);
            empty_shape(entry.rel.as_str(), node, &mut findings);
            not_monitoring(entry.rel.as_str(), node, &mut findings);
            no_layers(entry.rel.as_str(), node, &mut findings);
            frozen(entry.rel.as_str(), node, &mut findings);
        }
    }

    InspectorOutput {
        findings,
        coverage: context.scene_coverage(),
    }
}

fn no_collider(
    scene_rel: &str,
    node: &NodeRef<'_>,
    all: &[NodeRef<'_>],
    findings: &mut Vec<Finding>,
) {
    if !node.type_ends_with(&PHYSICAL_SUFFIXES) {
        return;
    }
    let prefix = format!("{}/", node.path);
    let has_shape = all.iter().any(|other| {
        other.path.starts_with(prefix.as_str())
            && (other.type_starts_with(&["CollisionShape", "CollisionPolygon"])
                // An instanced child may bring its own collider; the scan cannot see inside
                // it from here, so an instance counts as "possibly shaped" and the check
                // stays quiet rather than crying wolf.
                || other.type_.is_none())
    });
    if has_shape {
        return;
    }
    let is_area = node.type_ends_with(&["Area2D", "Area3D"]);
    collect(
        findings,
        Finding::draft(
            InspectorId::Physics,
            CODE_NO_COLLIDER,
            Severity::High,
            INSPECT_CONFIDENCE_CERTAIN,
            format!("`{}` has no collision shape", node.path),
            Location::node(scene_rel, node.path),
        )
        .cause(format!(
            "It is a {} and no CollisionShape or CollisionPolygon exists anywhere under it.",
            node.type_.unwrap_or("physics body")
        ))
        .impact(if is_area {
            "The area never overlaps anything, so every signal it was authored for is dead. \
             Godot prints one warning at start-up and nothing after that."
        } else {
            "The body has no extent: it falls through floors, walls pass through it, and \
             nothing it was supposed to block gets blocked."
        })
        .recommend(
            "Add a CollisionShape3D (or 2D) as a child and give it the shape resource that \
             matches the mesh.",
        )
        .evidence(
            "no CollisionShape / CollisionPolygon descendant".to_owned(),
            format!("{scene_rel}#{}", node.path),
        ),
    );
}

fn empty_shape(scene_rel: &str, node: &NodeRef<'_>, findings: &mut Vec<Finding>) {
    if !node.type_starts_with(&["CollisionShape"]) {
        return;
    }
    if node.get("shape").is_some() {
        return;
    }
    collect(
        findings,
        Finding::draft(
            InspectorId::Physics,
            CODE_EMPTY_SHAPE,
            Severity::High,
            INSPECT_CONFIDENCE_CERTAIN,
            format!("`{}` has no shape resource", node.path),
            Location::node(scene_rel, node.path),
        )
        .cause("The node carries no `shape` property, so its collider has no geometry.")
        .impact(
            "The parent body behaves exactly as if the collision shape were not there — and \
             the scene tree looks like it is, which is why this one takes so long to find.",
        )
        .recommend("Assign a shape — a BoxShape3D, a CapsuleShape3D, or the mesh's own.")
        .evidence(
            "no `shape` property".to_owned(),
            format!("{scene_rel}#{}", node.path),
        ),
    );
}

fn not_monitoring(scene_rel: &str, node: &NodeRef<'_>, findings: &mut Vec<Finding>) {
    if !node.type_ends_with(&["Area2D", "Area3D"]) {
        return;
    }
    if node.bool_prop("monitoring") != Some(false) {
        return;
    }
    let mut draft = Finding::draft(
        InspectorId::Physics,
        CODE_NOT_MONITORING,
        Severity::Medium,
        INSPECT_CONFIDENCE_CERTAIN,
        format!("`{}` is not monitoring", node.path),
        Location::node(scene_rel, node.path),
    )
    .cause(
        "`monitoring = false`, so the area detects nothing entering or leaving it, whatever \
         its collision shape and layers say.",
    )
    .impact(
        "Every overlap signal on it is silent. The node looks completely correct in the \
         inspector, which is what makes it expensive to find by hand.",
    )
    .recommend(
        "Set monitoring = true, unless a script deliberately turns it on later — in which \
         case check that script runs.",
    )
    .evidence(
        "monitoring = false".to_owned(),
        format!("{scene_rel}#{}", node.path),
    );

    match ProposedFix::new(
        CODE_NOT_MONITORING,
        format!("Turn monitoring on for `{}`", node.path),
        FixRisk::Low,
        vec![GodotAction::SetProperty {
            scene: scene_rel.to_owned(),
            path: node.path.to_owned(),
            property: "monitoring".to_owned(),
            value: TscnValue::Bool(true),
        }],
    ) {
        Ok(fix) => draft = draft.fix(fix),
        Err(error) => tracing::error!(%error, "the monitoring fix could not be described"),
    }
    collect(findings, draft);
}

fn no_layers(scene_rel: &str, node: &NodeRef<'_>, findings: &mut Vec<Finding>) {
    if !node.type_ends_with(&PHYSICAL_SUFFIXES) {
        return;
    }
    if node.int_prop("collision_layer") != Some(0) || node.int_prop("collision_mask") != Some(0) {
        return;
    }
    collect(
        findings,
        Finding::draft(
            InspectorId::Physics,
            CODE_NO_LAYERS,
            Severity::Medium,
            INSPECT_CONFIDENCE_CERTAIN,
            format!("`{}` is on no collision layer and watches none", node.path),
            Location::node(scene_rel, node.path),
        )
        .cause("Both `collision_layer` and `collision_mask` are 0.")
        .impact(
            "Nothing can collide with it and it can collide with nothing — the collision \
             shape underneath it is doing no work at all.",
        )
        .recommend(
            "Put it on the layer its role belongs to and set the mask to the layers it \
             should react to.",
        )
        .evidence(
            "collision_layer = 0, collision_mask = 0".to_owned(),
            format!("{scene_rel}#{}", node.path),
        ),
    );
}

fn frozen(scene_rel: &str, node: &NodeRef<'_>, findings: &mut Vec<Finding>) {
    if !node.type_ends_with(&["RigidBody2D", "RigidBody3D"]) {
        return;
    }
    if node.bool_prop("freeze") != Some(true) {
        return;
    }
    collect(
        findings,
        Finding::draft(
            InspectorId::Physics,
            CODE_FROZEN_BODY,
            Severity::Low,
            INSPECT_CONFIDENCE_CERTAIN,
            format!("`{}` is a frozen rigid body", node.path),
            Location::node(scene_rel, node.path),
        )
        .cause("`freeze = true`, so the physics engine does not simulate it.")
        .impact(
            "It costs what a rigid body costs and behaves like a static one. If it was meant \
             to fall or be pushed, it will not.",
        )
        .recommend(
            "Unfreeze it if it should be simulated, or make it a StaticBody if it should \
             not — a StaticBody is cheaper.",
        )
        .evidence(
            "freeze = true".to_owned(),
            format!("{scene_rel}#{}", node.path),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::context::test_support::{context_from, scene_entry};
    use crate::inspect::snapshot::ProjectSnapshot;

    fn from_scene(text: &str) -> Vec<Finding> {
        let snapshot = ProjectSnapshot {
            scenes: vec![scene_entry("scenes/level.tscn", text)],
            ..Default::default()
        };
        inspect(&context_from(&snapshot)).findings
    }

    #[test]
    fn a_body_with_no_collider_is_reported_and_one_with_a_collider_is_not() {
        let findings = from_scene(
            r#"[gd_scene format=3]

[node name="Level" type="Node3D"]

[node name="Floor" type="StaticBody3D" parent="."]

[node name="Crate" type="StaticBody3D" parent="."]

[node name="Shape" type="CollisionShape3D" parent="Crate"]
shape = SubResource("BoxShape3D_1")
"#,
        );
        let missing: Vec<&str> = findings
            .iter()
            .filter(|finding| finding.code == CODE_NO_COLLIDER)
            .filter_map(|finding| finding.location.node.as_deref())
            .collect();
        assert_eq!(missing, vec!["Floor"]);
    }

    #[test]
    fn a_collision_shape_with_no_shape_resource_is_its_own_finding() {
        let findings = from_scene(
            r#"[gd_scene format=3]

[node name="Level" type="Node3D"]

[node name="Crate" type="StaticBody3D" parent="."]

[node name="Shape" type="CollisionShape3D" parent="Crate"]
"#,
        );
        let empty: Vec<&str> = findings
            .iter()
            .filter(|finding| finding.code == CODE_EMPTY_SHAPE)
            .filter_map(|finding| finding.location.node.as_deref())
            .collect();
        assert_eq!(empty, vec!["Crate/Shape"]);
        // The parent has a collider node, so it is not also reported as having none.
        assert!(!findings
            .iter()
            .any(|finding| finding.code == CODE_NO_COLLIDER));
    }

    #[test]
    fn an_area_that_monitors_nothing_is_reported_with_the_fix_that_turns_it_on() {
        let findings = from_scene(
            r#"[gd_scene format=3]

[node name="Level" type="Node3D"]

[node name="Trigger" type="Area3D" parent="."]
monitoring = false

[node name="Shape" type="CollisionShape3D" parent="Trigger"]
shape = SubResource("BoxShape3D_1")
"#,
        );
        let quiet = findings
            .iter()
            .find(|finding| finding.code == CODE_NOT_MONITORING)
            .expect("the silent area is reported");
        let fix = quiet.fix.as_ref().expect("a fix is proposed");
        match &fix.actions[0] {
            GodotAction::SetProperty { value, .. } => assert_eq!(*value, TscnValue::Bool(true)),
            other => panic!("expected SetProperty, got {other:?}"),
        }
    }

    #[test]
    fn a_body_on_no_layers_at_all_is_reported_once() {
        let findings = from_scene(
            r#"[gd_scene format=3]

[node name="Level" type="Node3D"]

[node name="Ghost" type="StaticBody3D" parent="."]
collision_layer = 0
collision_mask = 0

[node name="Shape" type="CollisionShape3D" parent="Ghost"]
shape = SubResource("BoxShape3D_1")
"#,
        );
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.code == CODE_NO_LAYERS)
                .count(),
            1
        );
    }
}
