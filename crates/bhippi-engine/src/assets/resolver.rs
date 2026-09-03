//! Three-tier asset resolution pipeline (ADR-0043, GAD-117).
//!
//! Enforces that models never author raw file paths directly. Instead, semantic requests
//! (`asset.request { kind, tags, style }`) resolve through an authoritative priority order:
//! 1. Procedural generator (zero tokens, deterministic, local)
//! 2. Bundled CC0 library (curated, hash-pinned, instant)
//! 3. External generative provider (opt-in, metered, licensed)

use super::cc0_library::{materialize_cc0_asset, query_cc0_library};
use super::procedural_audio::{
    generate_procedural_audio, write_procedural_audio, ProceduralAudioPreset,
};
use super::procedural_mesh::{
    generate_procedural_mesh, write_procedural_mesh, ProceduralMeshPreset,
};
use super::procedural_texture::{
    generate_procedural_texture, write_procedural_texture, ProceduralTexturePattern,
};
use crate::asset::AssetKind;
use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The semantic request arriving from the agent or editor.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AssetRequest {
    pub kind: AssetKind,
    pub tags: Vec<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub target_name: Option<String>,
}

/// The resolution outcome pointing to the concrete in-project file.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResolvedAsset {
    pub rel_path: String,
    pub kind: AssetKind,
    pub source: String,
    pub license: String,
}

/// Resolve a semantic asset request in the strict order:
/// Procedural -> Bundled CC0 Library -> External Provider.
pub fn resolve_asset(game_dir: &Path, req: &AssetRequest) -> Result<ResolvedAsset> {
    if req.tags.is_empty() {
        return Err(EngineError::Asset(
            "asset request must carry at least one tag describing the asset".to_owned(),
            Some("Specify tags such as ['pine', 'tree'] or ['coin', 'gold'].".to_owned()),
        ));
    }

    let seed = req.seed.unwrap_or(42);
    let base_name = req
        .target_name
        .clone()
        .unwrap_or_else(|| req.tags[0].to_ascii_lowercase().replace(' ', "_"));

    // Tier 1: Procedural generator (zero tokens, immediate, deterministic)
    match req.kind {
        AssetKind::Mesh => {
            for tag in &req.tags {
                if let Some(preset) = ProceduralMeshPreset::parse(tag) {
                    let mesh = generate_procedural_mesh(&base_name, preset, seed)?;
                    write_procedural_mesh(game_dir, &mesh)?;
                    return Ok(ResolvedAsset {
                        rel_path: mesh.rel_path,
                        kind: AssetKind::Mesh,
                        source: "procedural".to_owned(),
                        license: "project".to_owned(),
                    });
                }
            }
        }
        AssetKind::Texture => {
            for tag in &req.tags {
                if let Some(pattern) = ProceduralTexturePattern::parse(tag) {
                    let tex = generate_procedural_texture(&base_name, pattern, 256, 256, seed)?;
                    write_procedural_texture(game_dir, &tex)?;
                    return Ok(ResolvedAsset {
                        rel_path: tex.rel_path,
                        kind: AssetKind::Texture,
                        source: "procedural".to_owned(),
                        license: "project".to_owned(),
                    });
                }
            }
        }
        AssetKind::Audio => {
            for tag in &req.tags {
                if let Some(preset) = ProceduralAudioPreset::parse(tag) {
                    let sfx = generate_procedural_audio(&base_name, preset, seed)?;
                    write_procedural_audio(game_dir, &sfx)?;
                    return Ok(ResolvedAsset {
                        rel_path: sfx.rel_path,
                        kind: AssetKind::Audio,
                        source: "procedural".to_owned(),
                        license: "project".to_owned(),
                    });
                }
            }
        }
        _ => {}
    }

    // Tier 2: Bundled CC0 Library
    let tag_refs: Vec<&str> = req.tags.iter().map(|s| s.as_str()).collect();
    let cc0_matches = query_cc0_library(&tag_refs, Some(req.kind));
    if let Some(entry) = cc0_matches.first() {
        let rel_path = materialize_cc0_asset(game_dir, entry)?;
        return Ok(ResolvedAsset {
            rel_path,
            kind: entry.kind,
            source: "bundled_cc0_library".to_owned(),
            license: entry.license.to_owned(),
        });
    }

    // Tier 3: If no procedural and no CC0 match, fail with clear next step
    Err(EngineError::Asset(
        format!(
            "could not resolve asset for kind {:?} with tags {:?}",
            req.kind, req.tags
        ),
        Some("Use known procedural presets or tags matching the bundled CC0 library.".to_owned()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_resolution_takes_precedence_over_library() {
        let temp_dir = std::env::temp_dir().join(format!("bhippi_res_test_{}", ulid::Ulid::new()));
        let req = AssetRequest {
            kind: AssetKind::Mesh,
            tags: vec!["pine".to_owned(), "tree".to_owned()],
            style: None,
            seed: Some(101),
            target_name: Some("pine_hero".to_owned()),
        };

        let resolved = resolve_asset(&temp_dir, &req).unwrap();
        assert_eq!(resolved.source, "procedural");
        assert_eq!(resolved.license, "project");
        assert!(temp_dir.join(&resolved.rel_path).is_file());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn bundled_cc0_resolves_when_no_procedural_preset_matches() {
        let temp_dir = std::env::temp_dir().join(format!("bhippi_res_test2_{}", ulid::Ulid::new()));
        let req = AssetRequest {
            kind: AssetKind::Mesh,
            tags: vec!["runner".to_owned(), "player".to_owned()],
            style: None,
            seed: None,
            target_name: None,
        };

        let resolved = resolve_asset(&temp_dir, &req).unwrap();
        assert_eq!(resolved.source, "bundled_cc0_library");
        assert_eq!(resolved.license, "CC0-1.0");
        assert!(temp_dir.join(&resolved.rel_path).is_file());

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
