//! Reading a parsed scene the way an inspector needs to read it.
//!
//! `GodotScene` answers questions about *one* node at a time; an inspector walks the whole
//! tree asking the same three questions of every node — what type is it really, what is
//! this property actually set to, and what is underneath it. These helpers are that walk,
//! written once so nine inspectors cannot each get "does this node have a collision shape"
//! subtly wrong.
//!
//! Everything here is *conservative about absence*. A property Godot left at its default is
//! not in the file at all, so `bool_prop` returning `None` means "the file does not say",
//! never "false" — and an inspector that treats those the same reports a project's defaults
//! as defects.

use crate::godot::scene::GodotScene;
use crate::godot::tscn::{TscnNode, TscnValue};

use super::snapshot::SceneEntry;

/// One node, with its resolved path and its raw property block.
#[derive(Clone, Copy, Debug)]
pub struct NodeRef<'a> {
    pub path: &'a str,
    pub name: &'a str,
    /// `None` when the node is an instance of another scene rather than a typed node.
    pub type_: Option<&'a str>,
    pub raw: &'a TscnNode,
    pub depth: usize,
}

impl<'a> NodeRef<'a> {
    /// A property, exactly as the file has it. `None` means the file does not say.
    #[must_use]
    pub fn get(&self, property: &str) -> Option<&'a TscnValue> {
        self.raw.get(property)
    }

    #[must_use]
    pub fn bool_prop(&self, property: &str) -> Option<bool> {
        match self.get(property)? {
            TscnValue::Bool(value) => Some(*value),
            TscnValue::Int(value) => Some(*value != 0),
            _ => None,
        }
    }

    #[must_use]
    pub fn int_prop(&self, property: &str) -> Option<i64> {
        match self.get(property)? {
            TscnValue::Int(value) => Some(*value),
            #[allow(clippy::cast_possible_truncation)]
            TscnValue::Float(value) => Some(*value as i64),
            _ => None,
        }
    }

    #[must_use]
    pub fn float_prop(&self, property: &str) -> Option<f64> {
        match self.get(property)? {
            TscnValue::Float(value) => Some(*value),
            #[allow(clippy::cast_precision_loss)]
            TscnValue::Int(value) => Some(*value as f64),
            _ => None,
        }
    }

    #[must_use]
    pub fn str_prop(&self, property: &str) -> Option<&'a str> {
        self.get(property)?.as_str()
    }

    /// `Color(r, g, b, a)`, when the property is one.
    #[must_use]
    pub fn color_prop(&self, property: &str) -> Option<(f64, f64, f64, f64)> {
        match self.get(property)? {
            TscnValue::Color(red, green, blue, alpha) => Some((*red, *green, *blue, *alpha)),
            _ => None,
        }
    }

    /// True when the node's type is exactly one of these.
    #[must_use]
    pub fn is_type(&self, types: &[&str]) -> bool {
        self.type_.is_some_and(|type_| types.contains(&type_))
    }

    /// True when the node's type ends with one of these — Godot's own naming convention
    /// carries the family in the suffix (`CharacterBody3D`, `RigidBody2D`).
    #[must_use]
    pub fn type_ends_with(&self, suffixes: &[&str]) -> bool {
        self.type_
            .is_some_and(|type_| suffixes.iter().any(|suffix| type_.ends_with(suffix)))
    }

    /// True when the node's type starts with one of these (`Camera`, `Navigation`).
    #[must_use]
    pub fn type_starts_with(&self, prefixes: &[&str]) -> bool {
        self.type_
            .is_some_and(|type_| prefixes.iter().any(|prefix| type_.starts_with(prefix)))
    }

    /// The `res://` path this node instances, when it is an instance.
    #[must_use]
    pub fn instance_of(&self, scene: &'a GodotScene) -> Option<String> {
        let id = self.raw.instance.as_ref()?.as_resource_id()?;
        scene
            .document
            .ext_resource(id)
            .map(|resource| resource.path.clone())
    }

    /// The `res://` path of the script attached to this node.
    #[must_use]
    pub fn script(&self, scene: &'a GodotScene) -> Option<String> {
        let id = self.get("script")?.as_resource_id()?;
        scene
            .document
            .ext_resource(id)
            .map(|resource| resource.path.clone())
    }
}

/// Every node of a scene, in file order, with its resolved path.
#[must_use]
pub fn nodes<'a>(scene: &'a GodotScene) -> Vec<NodeRef<'a>> {
    scene
        .nodes
        .iter()
        .filter_map(|resolved| {
            let raw = scene.document.nodes.get(resolved.index)?;
            Some(NodeRef {
                path: resolved.path.as_str(),
                name: resolved.name.as_str(),
                type_: resolved.type_.as_deref(),
                raw,
                depth: resolved.depth,
            })
        })
        .collect()
}

/// Every node of a scene entry that parsed; an empty list for one that did not.
#[must_use]
pub fn entry_nodes<'a>(entry: &'a SceneEntry) -> Vec<NodeRef<'a>> {
    entry.parsed().map(nodes).unwrap_or_default()
}

/// True when any descendant of `path` has a type satisfying `matches`.
#[must_use]
pub fn any_descendant(
    scene: &GodotScene,
    path: &str,
    matches: &dyn Fn(&NodeRef<'_>) -> bool,
) -> bool {
    let prefix = if path == "." {
        String::new()
    } else {
        format!("{path}/")
    };
    nodes(scene)
        .iter()
        .filter(|node| {
            node.path != path && (prefix.is_empty() || node.path.starts_with(prefix.as_str()))
        })
        .any(matches)
}

/// The relative luminance of an sRGB colour, per WCAG 2.1.
#[must_use]
pub fn relative_luminance(red: f64, green: f64, blue: f64) -> f64 {
    fn channel(value: f64) -> f64 {
        let value = value.clamp(0.0, 1.0);
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue)
}

/// The WCAG contrast ratio between two sRGB colours, 1.0–21.0.
#[must_use]
pub fn contrast_ratio(front: (f64, f64, f64), back: (f64, f64, f64)) -> f64 {
    let first = relative_luminance(front.0, front.1, front.2);
    let second = relative_luminance(back.0, back.1, back.2);
    let (lighter, darker) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCENE: &str = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://scripts/door.gd" id="1_door"]

[node name="Door" type="StaticBody3D"]
script = ExtResource("1_door")

[node name="Area3D" type="Area3D" parent="."]
monitoring = false

[node name="Shape" type="CollisionShape3D" parent="Area3D"]
"#;

    fn scene() -> GodotScene {
        GodotScene::parse(SCENE).expect("the fixture parses")
    }

    #[test]
    fn a_property_the_file_does_not_carry_reads_as_absent_not_as_false() {
        let scene = scene();
        let all = nodes(&scene);
        let area = all
            .iter()
            .find(|node| node.name == "Area3D")
            .expect("the fixture has an Area3D");
        assert_eq!(area.bool_prop("monitoring"), Some(false));
        assert_eq!(area.bool_prop("monitorable"), None);
    }

    #[test]
    fn the_type_family_helpers_read_godots_own_suffix_convention() {
        let scene = scene();
        let all = nodes(&scene);
        let root = all.first().expect("the fixture has a root");
        assert!(root.type_ends_with(&["Body3D"]));
        assert!(!root.type_ends_with(&["Body2D"]));
        assert!(root.is_type(&["StaticBody3D"]));
    }

    #[test]
    fn a_descendant_search_does_not_match_the_node_itself() {
        let scene = scene();
        assert!(any_descendant(&scene, "Area3D", &|node| {
            node.type_starts_with(&["CollisionShape"])
        }));
        assert!(!any_descendant(&scene, "Area3D/Shape", &|node| {
            node.type_starts_with(&["CollisionShape"])
        }));
    }

    #[test]
    fn a_scripts_res_path_resolves_through_the_ext_resource_table() {
        let scene = scene();
        let all = nodes(&scene);
        let root = all.first().expect("the fixture has a root");
        assert_eq!(
            root.script(&scene).as_deref(),
            Some("res://scripts/door.gd")
        );
    }

    #[test]
    fn contrast_matches_the_wcag_reference_pairs() {
        // Black on white is the maximum, 21:1; a colour against itself is 1:1.
        let ratio = contrast_ratio((0.0, 0.0, 0.0), (1.0, 1.0, 1.0));
        assert!((ratio - 21.0).abs() < 0.01, "black on white was {ratio}");
        let same = contrast_ratio((0.4, 0.4, 0.4), (0.4, 0.4, 0.4));
        assert!((same - 1.0).abs() < 0.001, "grey on grey was {same}");
    }
}
