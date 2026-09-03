//! Godot 4 runtime support: parsers, models, command **builders** and pure file writes.
//!
//! Everything in here is a pure library. Nothing spawns a process, opens a socket or
//! touches a database — [`command`] describes what to run and `bhippi-app` runs it, which
//! is what keeps `cargo test -p bhippi-engine` headless and GPU-free (ADR-0020).
//!
//! The design rule that shapes the parsers: **a value this crate does not understand is
//! preserved verbatim**. `project.godot`, `.tscn` and `export_presets.cfg` are files the
//! Godot editor also writes, so a round-trip that silently drops an unknown property would
//! corrupt a user's project the first time Bhippi edited a scene the editor had authored.
//! Both parsers verify their own typed parse by re-serialising it and comparing with the
//! source text; anything that does not reproduce byte for byte falls back to a raw string.

pub mod action;
pub mod command;
pub mod credits;
pub mod detect;
pub mod export;
pub mod export_presets;
pub mod gates;
pub mod manifest;
pub mod probe;
pub mod project;
pub mod scaffold;
pub mod scene;
pub mod templates;
pub mod tscn;
pub mod versions;

use crate::error::{EngineError, Result};

/// The prefix every Godot resource path carries.
pub const RES_PREFIX: &str = "res://";

/// Characters Godot forbids in a node name (`Node::validate_node_name`). A name carrying
/// any of them cannot be addressed by `NodePath`, so it is refused rather than sanitised —
/// silently renaming a node the agent asked for is how a scene stops matching its script.
pub const FORBIDDEN_NODE_NAME_CHARS: &[char] = &['.', ':', '@', '/', '"', '%'];

/// The conventional project sub-directory for scenes.
pub const SCENES_DIR: &str = "scenes";
/// The conventional project sub-directory for GDScript.
pub const SCRIPTS_DIR: &str = "scripts";
/// The conventional project sub-directory for imported assets.
pub const ASSETS_DIR: &str = "assets";
/// Where the Bhippi probe autoload lives inside a project.
pub const BHIPPI_DIR: &str = "bhippi";

/// `res://scenes/main.tscn` → `scenes/main.tscn`. A path that is already project-relative
/// is returned unchanged, so callers may hand either form to the same function.
#[must_use]
pub fn res_to_rel(path: &str) -> String {
    let stripped = path.strip_prefix(RES_PREFIX).unwrap_or(path);
    stripped.trim_start_matches('/').replace('\\', "/")
}

/// `scenes/main.tscn` → `res://scenes/main.tscn`, idempotent for an already-`res://` path.
#[must_use]
pub fn rel_to_res(path: &str) -> String {
    if path.starts_with(RES_PREFIX) {
        return path.replace('\\', "/");
    }
    format!(
        "{RES_PREFIX}{}",
        path.trim_start_matches('/').replace('\\', "/")
    )
}

/// True when `name` is a legal Godot node name: non-empty, no leading/trailing space and
/// none of [`FORBIDDEN_NODE_NAME_CHARS`].
#[must_use]
pub fn is_valid_node_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && !name.contains(FORBIDDEN_NODE_NAME_CHARS)
        && !name.chars().any(char::is_control)
}

/// [`is_valid_node_name`] as a typed rejection with an actionable hint.
pub fn check_node_name(name: &str) -> Result<()> {
    if is_valid_node_name(name) {
        return Ok(());
    }
    Err(EngineError::Action(
        format!("`{name}` is not a valid Godot node name"),
        Some(
            "Node names must be non-empty and may not contain . : @ / \" % or surrounding spaces."
                .to_owned(),
        ),
    ))
}

/// Split a Godot node path into its segments. `"."` (the scene root) yields no segments.
#[must_use]
pub fn node_path_segments(path: &str) -> Vec<&str> {
    if path == "." || path.is_empty() {
        return Vec::new();
    }
    path.split('/').filter(|part| !part.is_empty()).collect()
}

/// Join a parent node path and a child name into a stable node path.
/// `join_node_path(".", "Player")` is `"Player"`; `join_node_path("Player", "Mesh")` is
/// `"Player/Mesh"`.
#[must_use]
pub fn join_node_path(parent: &str, name: &str) -> String {
    if parent == "." || parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

/// The parent path of a node path, or `None` for the root. `"Player"` → `Some(".")`.
#[must_use]
pub fn parent_node_path(path: &str) -> Option<String> {
    if path == "." || path.is_empty() {
        return None;
    }
    match path.rsplit_once('/') {
        Some((parent, _)) => Some(parent.to_owned()),
        None => Some(".".to_owned()),
    }
}

/// The last segment of a node path. The root's name is not derivable from `"."`, so it is
/// reported as `"."` and callers read the root node's `name` field instead.
#[must_use]
pub fn node_path_name(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((_, name)) => name,
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        check_node_name, is_valid_node_name, join_node_path, node_path_segments, parent_node_path,
        rel_to_res, res_to_rel,
    };

    #[test]
    fn res_paths_convert_both_ways_and_are_idempotent() {
        assert_eq!(res_to_rel("res://scenes/main.tscn"), "scenes/main.tscn");
        assert_eq!(res_to_rel("scenes/main.tscn"), "scenes/main.tscn");
        assert_eq!(rel_to_res("scenes/main.tscn"), "res://scenes/main.tscn");
        assert_eq!(
            rel_to_res("res://scenes/main.tscn"),
            "res://scenes/main.tscn"
        );
        assert_eq!(res_to_rel(&rel_to_res("a/b.gd")), "a/b.gd");
    }

    #[test]
    fn node_names_godot_would_refuse_are_refused_with_a_hint() {
        assert!(is_valid_node_name("Player"));
        assert!(is_valid_node_name("Player 2"));
        for bad in ["", " Player", "Player ", "a/b", "a:b", "a@b", "a%b", "a.b"] {
            assert!(!is_valid_node_name(bad), "{bad} must be refused");
            let error = check_node_name(bad).expect_err("refused");
            assert!(error.hint().is_some());
        }
    }

    #[test]
    fn node_paths_join_and_split_around_the_root() {
        assert_eq!(join_node_path(".", "Player"), "Player");
        assert_eq!(join_node_path("Player", "Mesh"), "Player/Mesh");
        assert_eq!(node_path_segments("."), Vec::<&str>::new());
        assert_eq!(node_path_segments("Player/Mesh"), vec!["Player", "Mesh"]);
        assert_eq!(parent_node_path("Player/Mesh").as_deref(), Some("Player"));
        assert_eq!(parent_node_path("Player").as_deref(), Some("."));
        assert_eq!(parent_node_path("."), None);
    }
}
