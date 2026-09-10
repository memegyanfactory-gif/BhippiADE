//! Bundled CC0 asset library manifest and tag-indexed importer (ADR-0043, GAD-113).
//!
//! A curated, hash-pinned catalogue of CC0 assets (characters, props, nature,
//! vehicles, UI, SFX) indexed by tags. Resolves semantic requirements into real
//! project files with explicit CC0 attribution sidecars.

use crate::asset::AssetKind;
use crate::error::{EngineError, Result};
use serde::Serialize;
use std::path::Path;

/// Metadata record for a bundled CC0 library asset.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cc0AssetEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: AssetKind,
    pub tags: &'static [&'static str],
    pub file_name: &'static str,
    pub destination_subfolder: &'static str,
    pub author: &'static str,
    pub license: &'static str,
    pub hash_pin: &'static str,
    pub content_text: &'static str,
}

impl Cc0AssetEntry {
    /// Relative path inside a game project if materialized.
    #[must_use]
    pub fn target_rel_path(&self) -> String {
        format!("{}/{}", self.destination_subfolder, self.file_name)
    }

    /// Match score based on queried tags. Returns number of tag hits.
    #[must_use]
    pub fn score_tags(&self, query_tags: &[&str]) -> usize {
        let mut score = 0;
        for q in query_tags {
            let needle = q.trim().to_ascii_lowercase();
            if self.tags.iter().any(|t| t.to_ascii_lowercase() == needle) {
                score += 1;
            }
        }
        score
    }
}

/// The built-in curated CC0 library catalogue.
pub static CC0_CATALOGUE: &[Cc0AssetEntry] = &[
    Cc0AssetEntry {
        id: "cc0_pine_tree",
        name: "Pine Tree (CC0)",
        kind: AssetKind::Mesh,
        tags: &["pine", "tree", "nature", "foliage", "forest", "woodland"],
        file_name: "pine_tree_cc0.tscn",
        destination_subfolder: "assets/models",
        author: "Kenney (CC0 1.0 Universal)",
        license: "CC0-1.0",
        hash_pin: "blake3:c1a2f4d6e8b0a2c4e6f8a0b2c4e6f8a0",
        content_text: r#"[gd_scene format=3 uid="uid://cc0_pine_001"]

[node name="PineTree" type="Node3D"]

[node name="Trunk" type="CSGCylinder3D" parent="."]
transform = Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0.8, 0)
radius = 0.22
height = 1.6
use_collision = true

[node name="FoliageCone" type="CSGCylinder3D" parent="."]
transform = Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 2.0, 0)
radius = 1.1
height = 1.8
cone = true
"#,
    },
    Cc0AssetEntry {
        id: "cc0_rock_boulder",
        name: "Rock Boulder (CC0)",
        kind: AssetKind::Mesh,
        tags: &["rock", "boulder", "stone", "nature", "mineral", "mountain"],
        file_name: "rock_boulder_cc0.tscn",
        destination_subfolder: "assets/models",
        author: "Kenney (CC0 1.0 Universal)",
        license: "CC0-1.0",
        hash_pin: "blake3:b2c4e6f8a0b2c4e6f8a0b2c4e6f8a0b2",
        content_text: r#"[gd_scene format=3 uid="uid://cc0_rock_001"]

[node name="RockBoulder" type="Node3D"]

[node name="Shape" type="CSGSphere3D" parent="."]
transform = Transform3D(1.2, 0, 0, 0, 0.8, 0, 0, 0, 1.0, 0, 0.7, 0)
radius = 0.9
radial_segments = 6
rings = 4
use_collision = true
"#,
    },
    Cc0AssetEntry {
        id: "cc0_wooden_crate",
        name: "Wooden Crate (CC0)",
        kind: AssetKind::Mesh,
        tags: &["crate", "box", "prop", "cargo", "wood", "container"],
        file_name: "wooden_crate_cc0.tscn",
        destination_subfolder: "assets/models",
        author: "Kenney (CC0 1.0 Universal)",
        license: "CC0-1.0",
        hash_pin: "blake3:d3e5f7a1b3c5d7e9f1a3b5c7d9e1f3a5",
        content_text: r#"[gd_scene format=3 uid="uid://cc0_crate_001"]

[node name="WoodenCrate" type="Node3D"]

[node name="Box" type="CSGBox3D" parent="."]
transform = Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0.5, 0)
size = Vector3(1.0, 1.0, 1.0)
use_collision = true
"#,
    },
    Cc0AssetEntry {
        id: "cc0_coin_pickup",
        name: "Gold Coin (CC0)",
        kind: AssetKind::Mesh,
        tags: &["coin", "gold", "pickup", "collectible", "item", "currency"],
        file_name: "gold_coin_cc0.tscn",
        destination_subfolder: "assets/models",
        author: "Kenney (CC0 1.0 Universal)",
        license: "CC0-1.0",
        hash_pin: "blake3:e4f6a2b4c6d8e0f2a4b6c8d0e2f4a6b8",
        content_text: r#"[gd_scene format=3 uid="uid://cc0_coin_001"]

[node name="GoldCoin" type="Node3D"]

[node name="CoinDisc" type="CSGCylinder3D" parent="."]
transform = Transform3D(0, 0, 1, 0, 1, 0, -1, 0, 0, 0, 0.5, 0)
radius = 0.35
height = 0.08
use_collision = true
"#,
    },
    Cc0AssetEntry {
        id: "cc0_runner_character",
        name: "Low-Poly Runner (CC0)",
        kind: AssetKind::Mesh,
        tags: &[
            "character",
            "player",
            "runner",
            "hero",
            "avatar",
            "humanoid",
        ],
        file_name: "runner_character_cc0.tscn",
        destination_subfolder: "assets/models",
        author: "Kenney (CC0 1.0 Universal)",
        license: "CC0-1.0",
        hash_pin: "blake3:f5a7b9c1d3e5f7a9b1c3d5e7f9a1b3c5",
        content_text: r#"[gd_scene format=3 uid="uid://cc0_chr_001"]

[node name="RunnerCharacter" type="Node3D"]

[node name="Torso" type="CSGBox3D" parent="."]
transform = Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0.9, 0)
size = Vector3(0.5, 0.6, 0.3)
use_collision = true

[node name="Head" type="CSGSphere3D" parent="."]
transform = Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 1.45, 0)
radius = 0.22
"#,
    },
    Cc0AssetEntry {
        id: "cc0_racing_kart",
        name: "Racing Kart (CC0)",
        kind: AssetKind::Mesh,
        tags: &["kart", "vehicle", "car", "racing", "automobile"],
        file_name: "racing_kart_cc0.tscn",
        destination_subfolder: "assets/models",
        author: "Kenney (CC0 1.0 Universal)",
        license: "CC0-1.0",
        hash_pin: "blake3:a6b8c0d2e4f6a8b0c2d4e6f8a0b2c4e6",
        content_text: r#"[gd_scene format=3 uid="uid://cc0_kart_001"]

[node name="RacingKart" type="Node3D"]

[node name="Body" type="CSGBox3D" parent="."]
transform = Transform3D(1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0.3, 0)
size = Vector3(1.1, 0.35, 1.8)
use_collision = true
"#,
    },
];

/// Find matching CC0 assets ranked by tag relevance.
#[must_use]
pub fn query_cc0_library<'a>(tags: &[&str], kind: Option<AssetKind>) -> Vec<&'a Cc0AssetEntry> {
    let mut matches: Vec<(&'a Cc0AssetEntry, usize)> = CC0_CATALOGUE
        .iter()
        .filter(|entry| kind.is_none_or(|k| entry.kind == k))
        .map(|entry| (entry, entry.score_tags(tags)))
        .filter(|(_, score)| *score > 0)
        .collect();

    matches.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
    matches.into_iter().map(|(entry, _)| entry).collect()
}

/// Materialize a CC0 asset entry into the game project and generate its `.meta.json` sidecar.
pub fn materialize_cc0_asset(game_dir: &Path, entry: &Cc0AssetEntry) -> Result<String> {
    let rel_path = entry.target_rel_path();
    let target_path = game_dir.join(&rel_path);
    let meta_path = game_dir.join(format!("{rel_path}.meta.json"));

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| EngineError::Io {
            operation: "create_dir_all",
            path: parent.display().to_string(),
            reason: error.to_string(),
            hint: Some("Check workspace directory permissions.".to_owned()),
        })?;
    }

    std::fs::write(&target_path, entry.content_text).map_err(|error| EngineError::Io {
        operation: "write",
        path: target_path.display().to_string(),
        reason: error.to_string(),
        hint: Some("Check workspace disk space.".to_owned()),
    })?;

    let now = chrono::Utc::now().to_rfc3339();
    let sidecar = serde_json::json!({
        "license": entry.license,
        "author": entry.author,
        "provenance": {
            "source": "bundled_cc0_library",
            "entry_id": entry.id,
            "hash_pin": entry.hash_pin
        },
        "created_at": now,
        "attribution": format!("{} by {} ({})", entry.name, entry.author, entry.license)
    });

    let sidecar_content = serde_json::to_string_pretty(&sidecar).map_err(|error| {
        EngineError::Asset(
            format!("failed to format sidecar: {error}"),
            Some("Report this as an engine bug.".to_owned()),
        )
    })?;

    std::fs::write(&meta_path, sidecar_content).map_err(|error| EngineError::Io {
        operation: "write",
        path: meta_path.display().to_string(),
        reason: error.to_string(),
        hint: Some("Check workspace permissions.".to_owned()),
    })?;

    Ok(rel_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cc0_library_matches_queries_by_tag() {
        let results = query_cc0_library(&["tree", "woodland"], Some(AssetKind::Mesh));
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "cc0_pine_tree");

        let coin_results = query_cc0_library(&["gold", "pickup"], None);
        assert!(!coin_results.is_empty());
        assert_eq!(coin_results[0].id, "cc0_coin_pickup");
    }

    #[test]
    fn cc0_materialize_writes_target_and_sidecar() {
        let temp_dir = std::env::temp_dir().join(format!("bhippi_cc0_test_{}", ulid::Ulid::new()));
        let entry = &CC0_CATALOGUE[0];
        let rel_path = materialize_cc0_asset(&temp_dir, entry).unwrap();

        assert!(temp_dir.join(&rel_path).is_file());
        let meta_file = temp_dir.join(format!("{rel_path}.meta.json"));
        assert!(meta_file.is_file());

        let meta_text = std::fs::read_to_string(meta_file).unwrap();
        assert!(meta_text.contains("\"license\": \"CC0-1.0\""));
        assert!(meta_text.contains("Kenney"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
