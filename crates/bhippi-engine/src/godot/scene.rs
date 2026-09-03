//! The queryable model over a parsed [`TscnDocument`].
//!
//! `.tscn` stores nodes as a flat list whose `parent="…"` attributes imply a tree. This
//! module resolves that into stable node paths (`"Player/Mesh"`, `"."` for the root) and
//! answers the questions the agent and the Inspector actually ask: what is in this scene,
//! where is it, what script does it carry, what is in group `bhippi_track`.
//!
//! Nothing here mutates: a query that could edit is an action, and actions live in
//! [`super::action`].

use super::tscn::{node_path, TscnDocument, TscnNode, TscnValue};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fmt::Write as _;

/// How many nodes a prompt digest carries before it is truncated. The cap exists for the
/// same reason the mind-map's does: a 4 000-node scene would eat the whole turn budget and
/// the model would still only act on a handful of them.
pub const SCENE_DIGEST_MAX_NODES: usize = 200;

/// The group a node joins to be sampled by the playtest probe.
pub const TRACK_GROUP: &str = "bhippi_track";

/// One node, resolved.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SceneNode {
    /// `"."` for the root, otherwise `"Player/Mesh"`.
    pub path: String,
    pub name: String,
    pub type_: Option<String>,
    /// The parent's path; `None` only for the root.
    pub parent: Option<String>,
    pub groups: Vec<String>,
    /// Depth from the root, which is 0.
    pub depth: usize,
    /// Index into [`TscnDocument::nodes`].
    pub index: usize,
}

/// Everything the UI shows for one selected node.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct NodeView {
    pub path: String,
    pub name: String,
    pub type_: Option<String>,
    /// The `res://` path of the attached script, resolved through `ext_resource`.
    pub script: Option<String>,
    /// The `res://` path of the scene this node instances, when it is an instance.
    pub instance: Option<String>,
    pub groups: Vec<String>,
    pub properties: Vec<(String, TscnValue)>,
}

/// A parsed scene plus its resolved tree.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GodotScene {
    pub document: TscnDocument,
    pub nodes: Vec<SceneNode>,
}

impl GodotScene {
    /// Resolve a document into a tree. Nodes whose parent is missing keep their declared
    /// depth-by-path rather than being dropped — a scene Godot wrote is well formed, and a
    /// scene it did not is still better shown than hidden.
    #[must_use]
    pub fn from_document(document: TscnDocument) -> Self {
        let nodes: Vec<SceneNode> = document
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let path = node_path(node);
                SceneNode {
                    depth: super::node_path_segments(&path).len(),
                    path,
                    name: node.name.clone(),
                    type_: node.type_.clone(),
                    parent: node.parent.clone(),
                    groups: node.groups.clone(),
                    index,
                }
            })
            .collect();
        Self { document, nodes }
    }

    /// Parse and resolve in one step.
    pub fn parse(text: &str) -> crate::error::Result<Self> {
        Ok(Self::from_document(super::tscn::parse(text)?))
    }

    #[must_use]
    pub fn root(&self) -> Option<&SceneNode> {
        self.nodes.iter().find(|node| node.parent.is_none())
    }

    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.nodes.iter().any(|node| node.path == path)
    }

    fn raw(&self, path: &str) -> Option<&TscnNode> {
        let node = self.nodes.iter().find(|node| node.path == path)?;
        self.document.nodes.get(node.index)
    }

    /// Everything the Inspector needs for one node.
    #[must_use]
    pub fn node(&self, path: &str) -> Option<NodeView> {
        let resolved = self.nodes.iter().find(|node| node.path == path)?;
        let raw = self.document.nodes.get(resolved.index)?;
        Some(NodeView {
            path: resolved.path.clone(),
            name: resolved.name.clone(),
            type_: resolved.type_.clone(),
            script: raw
                .get("script")
                .and_then(TscnValue::as_resource_id)
                .and_then(|id| self.document.ext_resource(id))
                .map(|resource| resource.path.clone()),
            instance: raw
                .instance
                .as_ref()
                .and_then(TscnValue::as_resource_id)
                .and_then(|id| self.document.ext_resource(id))
                .map(|resource| resource.path.clone()),
            groups: resolved.groups.clone(),
            properties: raw.properties.clone(),
        })
    }

    /// One property of one node.
    #[must_use]
    pub fn property(&self, path: &str, property: &str) -> Option<&TscnValue> {
        self.raw(path)?.get(property)
    }

    /// Direct children of `path`, in document order.
    #[must_use]
    pub fn children(&self, path: &str) -> Vec<String> {
        let parent = if path == "." { "." } else { path };
        self.nodes
            .iter()
            .filter(|node| node.parent.as_deref() == Some(parent))
            .map(|node| node.path.clone())
            .collect()
    }

    /// Every descendant of `path`, depth-first in document order.
    #[must_use]
    pub fn descendants(&self, path: &str) -> Vec<String> {
        let prefix = if path == "." {
            String::new()
        } else {
            format!("{path}/")
        };
        self.nodes
            .iter()
            .filter(|node| {
                node.path != path
                    && node.parent.is_some()
                    && (prefix.is_empty() || node.path.starts_with(&prefix))
            })
            .map(|node| node.path.clone())
            .collect()
    }

    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|node| node.name == name)
            .map(|node| node.path.clone())
            .collect()
    }

    #[must_use]
    pub fn find_by_type(&self, type_: &str) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|node| node.type_.as_deref() == Some(type_))
            .map(|node| node.path.clone())
            .collect()
    }

    #[must_use]
    pub fn find_in_group(&self, group: &str) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|node| node.groups.iter().any(|name| name == group))
            .map(|node| node.path.clone())
            .collect()
    }

    /// Nodes the playtest probe samples.
    #[must_use]
    pub fn tracked(&self) -> Vec<String> {
        self.find_in_group(TRACK_GROUP)
    }

    /// Every group used anywhere in the scene, sorted and de-duplicated.
    #[must_use]
    pub fn groups(&self) -> Vec<String> {
        let mut groups: Vec<String> = self
            .nodes
            .iter()
            .flat_map(|node| node.groups.iter().cloned())
            .collect();
        groups.sort();
        groups.dedup();
        groups
    }

    /// The `res://` path of every script the scene references.
    #[must_use]
    pub fn scripts(&self) -> Vec<String> {
        self.document
            .ext_resources
            .iter()
            .filter(|resource| resource.type_ == "Script")
            .map(|resource| resource.path.clone())
            .collect()
    }

    /// Every instanced sub-scene as `(node path, res:// scene path)`.
    #[must_use]
    pub fn instances(&self) -> Vec<(String, String)> {
        self.document
            .nodes
            .iter()
            .filter_map(|node| {
                let id = node.instance.as_ref()?.as_resource_id()?;
                let resource = self.document.ext_resource(id)?;
                Some((node_path(node), resource.path.clone()))
            })
            .collect()
    }

    /// A bounded, indented text digest for the prompt: `Name (Type) [groups]`, one node per
    /// line, capped at `max_nodes`. When the scene is longer than the cap the digest says
    /// so and says how to get the rest, so the model asks instead of assuming it saw
    /// everything.
    #[must_use]
    pub fn tree_digest(&self, max_nodes: usize) -> String {
        let cap = max_nodes.max(1);
        let mut out = String::new();
        for node in self.nodes.iter().take(cap) {
            let indent = "  ".repeat(node.depth.min(8));
            let type_ = node.type_.as_deref().unwrap_or("instance");
            let _ = write!(out, "{indent}{} ({type_})", node.name);
            if !node.groups.is_empty() {
                let _ = write!(out, " [{}]", node.groups.join(", "));
            }
            out.push('\n');
        }
        if self.nodes.len() > cap {
            let _ = writeln!(
                out,
                "… {} more nodes (cap {cap}). Ask for a subtree with children(path) rather than assuming this is the whole scene.",
                self.nodes.len() - cap
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{GodotScene, SCENE_DIGEST_MAX_NODES};
    use crate::godot::tscn::{TscnDocument, TscnNode};

    const MAIN: &str = include_str!("../../../../tests/fixtures/godot/main.tscn");
    const HUD: &str = include_str!("../../../../tests/fixtures/godot/hud.tscn");

    fn main_scene() -> GodotScene {
        GodotScene::parse(&MAIN.replace("\r\n", "\n")).expect("fixture parses")
    }

    #[test]
    fn parenting_resolves_into_stable_node_paths() {
        let scene = main_scene();
        assert_eq!(
            scene.root().map(|node| node.path.clone()),
            Some(".".to_owned())
        );
        assert!(scene.contains("Player"));
        assert!(scene.contains("Player/CollisionShape3D"));
        assert!(scene.contains("HUD/Control/Button"));
        assert_eq!(
            scene.children("Player"),
            vec![
                "Player/CollisionShape3D",
                "Player/MeshInstance3D",
                "Player/Crate"
            ]
        );
        // Two nodes named CollisionShape3D live under different parents and keep apart.
        assert_eq!(scene.find_by_name("CollisionShape3D").len(), 2);
        assert_eq!(scene.descendants("HUD").len(), 3);
    }

    #[test]
    fn queries_answer_what_the_inspector_asks() {
        let scene = main_scene();
        assert_eq!(scene.find_by_type("Camera3D"), vec!["Camera3D"]);
        assert_eq!(scene.tracked(), vec!["Player"]);
        assert!(scene.groups().contains(&"player".to_owned()));
        assert_eq!(scene.scripts(), vec!["res://scripts/player.gd"]);
        assert_eq!(
            scene.instances(),
            vec![(
                "Player/Crate".to_owned(),
                "res://scenes/crate.tscn".to_owned()
            )]
        );

        let player = scene.node("Player").expect("player");
        assert_eq!(player.script.as_deref(), Some("res://scripts/player.gd"));
        assert_eq!(player.type_.as_deref(), Some("CharacterBody3D"));
        assert!(player.groups.contains(&"bhippi_track".to_owned()));
        assert!(scene.property("Player", "speed").is_some());
        assert!(scene.node("Nowhere").is_none());
    }

    #[test]
    fn the_digest_is_indented_and_says_when_it_stopped() {
        let scene = main_scene();
        let full = scene.tree_digest(SCENE_DIGEST_MAX_NODES);
        assert!(full.starts_with("Main (Node3D)\n"));
        assert!(full.contains("  Player (CharacterBody3D) [bhippi_track, player]"));
        assert!(full.contains("    CollisionShape3D (CollisionShape3D)"));
        assert!(!full.contains("more nodes"));

        let clipped = scene.tree_digest(3);
        assert!(clipped.contains("more nodes (cap 3)"));
        assert!(clipped.contains("children(path)"));
    }

    #[test]
    fn a_control_scene_resolves_the_same_way() {
        let scene = GodotScene::parse(&HUD.replace("\r\n", "\n")).expect("hud parses");
        assert_eq!(
            scene.root().map(|node| node.name.clone()),
            Some("Hud".to_owned())
        );
        assert_eq!(
            scene.find_by_type("Button"),
            vec!["Margin/Rows/RestartButton"]
        );
        assert_eq!(scene.tracked(), vec!["Margin/Rows/ScoreLabel"]);
        assert!(scene.instances().is_empty());
    }

    #[test]
    fn a_scene_with_only_a_root_still_answers_every_query() {
        let scene = GodotScene::from_document(TscnDocument::new_scene("Main", "Node2D"));
        assert_eq!(scene.node_count(), 1);
        assert!(scene.children(".").is_empty());
        assert!(scene.scripts().is_empty());
        assert_eq!(scene.tree_digest(10), "Main (Node2D)\n");

        let mut document = TscnDocument::new_scene("Main", "Node2D");
        document
            .nodes
            .push(TscnNode::new("Player", "CharacterBody2D", Some(".")));
        let scene = GodotScene::from_document(document);
        assert_eq!(scene.children("."), vec!["Player"]);
    }
}
