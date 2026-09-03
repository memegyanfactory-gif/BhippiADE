//! Attribution and credits generation from asset sidecars (ADR-0043, GAD-114).
//!
//! Scans a game project's asset sidecars (`*.meta.json`) and compiles an authoritative,
//! comprehensive credits and licensing document for web/desktop exports and distribution.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Individual asset attribution record extracted from sidecars.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AssetAttribution {
    pub asset_rel_path: String,
    pub license: String,
    pub author: Option<String>,
    pub attribution: Option<String>,
    pub source: Option<String>,
}

/// Project-wide credits collection.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectCredits {
    pub assets: Vec<AssetAttribution>,
    pub license_counts: BTreeMap<String, usize>,
}

/// Scan `assets/` in a game directory and collect all metadata sidecars.
pub fn collect_project_attributions(game_dir: &Path) -> Result<ProjectCredits> {
    let assets_dir = game_dir.join("assets");
    let mut attributions = Vec::new();
    let mut license_counts: BTreeMap<String, usize> = BTreeMap::new();

    if assets_dir.is_dir() {
        let mut meta_files = Vec::new();
        find_meta_files(&assets_dir, &mut meta_files);

        for meta_path in meta_files {
            let content = match std::fs::read_to_string(&meta_path) {
                Ok(text) => text,
                Err(_) => continue,
            };

            let parsed: serde_json::Value = match serde_json::from_str(&content) {
                Ok(val) => val,
                Err(_) => continue,
            };

            let license = parsed
                .get("license")
                .and_then(|l| l.as_str())
                .unwrap_or("unknown")
                .to_owned();

            let author = parsed
                .get("author")
                .and_then(|a| a.as_str())
                .map(|s| s.to_owned());

            let attribution = parsed
                .get("attribution")
                .and_then(|a| a.as_str())
                .map(|s| s.to_owned());

            let source = parsed
                .get("provenance")
                .and_then(|p| p.get("source"))
                .and_then(|s| s.as_str())
                .map(|s| s.to_owned());

            let rel_meta = meta_path
                .strip_prefix(game_dir)
                .unwrap_or(&meta_path)
                .to_string_lossy()
                .replace('\\', "/");

            let asset_rel_path = rel_meta
                .strip_suffix(".meta.json")
                .unwrap_or(&rel_meta)
                .to_owned();

            *license_counts.entry(license.clone()).or_insert(0) += 1;

            attributions.push(AssetAttribution {
                asset_rel_path,
                license,
                author,
                attribution,
                source,
            });
        }
    }

    attributions.sort_by(|a, b| a.asset_rel_path.cmp(&b.asset_rel_path));

    Ok(ProjectCredits {
        assets: attributions,
        license_counts,
    })
}

fn find_meta_files(dir: &Path, output: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_meta_files(&path, output);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .map_or(false, |name| name.ends_with(".meta.json"))
        {
            output.push(path);
        }
    }
}

/// Generate a Markdown credits document for the game.
pub fn generate_credits_markdown(game_name: &str, credits: &ProjectCredits) -> String {
    let mut md = String::new();
    md.push_str(&format!("# Credits & Licences — {game_name}\n\n"));
    md.push_str("This project was built with **Bhippi Game Studio** (ADR-0043).\n\n");

    md.push_str("## Licence Summary\n\n");
    if credits.license_counts.is_empty() {
        md.push_str("No external or recorded assets in project.\n\n");
    } else {
        md.push_str("| Licence | Asset Count |\n|---|---|\n");
        for (lic, count) in &credits.license_counts {
            md.push_str(&format!("| `{lic}` | {count} |\n"));
        }
        md.push('\n');
    }

    md.push_str("## Asset Attributions\n\n");
    if credits.assets.is_empty() {
        md.push_str("No assets documented.\n\n");
    } else {
        md.push_str("| Asset | Licence | Author / Attribution |\n|---|---|---|\n");
        for asset in &credits.assets {
            let author_attr = asset
                .attribution
                .as_deref()
                .or(asset.author.as_deref())
                .unwrap_or("Project Created");
            md.push_str(&format!(
                "| `{}` | `{}` | {} |\n",
                asset.asset_rel_path, asset.license, author_attr
            ));
        }
        md.push('\n');
    }

    md.push_str("## Third-Party Notices\n\n");
    md.push_str("- **Godot Engine**: Copyright (c) 2014-present Godot Engine contributors. Released under MIT license.\n");
    md.push_str("- **Bhippi Studio**: Copyright (c) 2026 Bhippi Contributors. AGPL-3.0.\n");

    md
}

/// Write a `credits.md` file in the game directory.
pub fn write_project_credits(game_dir: &Path, game_name: &str) -> Result<String> {
    let credits = collect_project_attributions(game_dir)?;
    let md = generate_credits_markdown(game_name, &credits);
    let target = game_dir.join("credits.md");

    std::fs::write(&target, &md).map_err(|error| EngineError::Io {
        operation: "write",
        path: target.display().to_string(),
        reason: error.to_string(),
        hint: Some("Check workspace write permissions.".to_owned()),
    })?;

    Ok(md)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribution_collector_aggregates_sidecars() {
        let temp_dir = std::env::temp_dir().join(format!("bhippi_attr_test_{}", ulid::Ulid::new()));
        let assets_dir = temp_dir.join("assets/models");
        std::fs::create_dir_all(&assets_dir).unwrap();

        let sidecar1 = serde_json::json!({
            "license": "CC0-1.0",
            "author": "Kenney",
            "attribution": "Pine Tree by Kenney"
        });
        std::fs::write(
            assets_dir.join("tree.tscn.meta.json"),
            serde_json::to_string(&sidecar1).unwrap(),
        )
        .unwrap();

        let sidecar2 = serde_json::json!({
            "license": "project",
            "attribution": "Procedural Crate"
        });
        std::fs::write(
            assets_dir.join("crate.tscn.meta.json"),
            serde_json::to_string(&sidecar2).unwrap(),
        )
        .unwrap();

        let credits = collect_project_attributions(&temp_dir).unwrap();
        assert_eq!(credits.assets.len(), 2);
        assert_eq!(credits.license_counts.get("CC0-1.0"), Some(&1));
        assert_eq!(credits.license_counts.get("project"), Some(&1));

        let md = generate_credits_markdown("TestGame", &credits);
        assert!(md.contains("Pine Tree by Kenney"));
        assert!(md.contains("CC0-1.0"));

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
