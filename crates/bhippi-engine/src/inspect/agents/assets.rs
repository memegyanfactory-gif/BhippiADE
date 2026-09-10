//! The Asset Inspector: the files, and what the project does with them (ADR-0056 §2).
//!
//! Everything here is a fact about a file on disk — its size, its header, its content hash,
//! and whether anything in the project names it. That is deliberately narrower than what a
//! full asset pipeline knows (triangle counts, LOD chains, compression settings all live
//! inside binary formats this crate does not parse), and narrower is the point: a triangle
//! count Bhippi guessed from a file size would be exactly the invented measurement §3
//! forbids. What is here is checkable by hand in ten seconds, which is what makes it worth
//! reporting.
//!
//! Licences are **not** here. `godot::gates` blocks on them and the gate bridge carries its
//! findings; a second opinion on the same file would give the user two rows for one problem.

use bhippi_types::{
    InspectorId, Severity, INSPECT_CONFIDENCE_CERTAIN, INSPECT_CONFIDENCE_HEURISTIC,
    INSPECT_MESH_LARGE_BYTES, INSPECT_TEXTURE_LARGE_BYTES, INSPECT_TEXTURE_LARGE_PIXELS,
};
use std::collections::BTreeMap;

use super::collect;
use crate::asset::AssetKind;
use crate::inspect::context::{InspectContext, InspectorOutput};
use crate::inspect::finding::{Finding, Location};

/// A scene references a resource that is not on disk.
pub const CODE_DANGLING_RESOURCE: &str = "BHP-INS-401";
/// A texture larger than any screen will show.
pub const CODE_OVERSIZED_TEXTURE: &str = "BHP-INS-402";
/// A single file large enough to matter to load time and download size.
pub const CODE_LARGE_ASSET: &str = "BHP-INS-403";
/// An asset nothing in the project names.
pub const CODE_UNUSED_ASSET: &str = "BHP-INS-404";
/// Two files with identical content.
pub const CODE_DUPLICATE_ASSET: &str = "BHP-INS-405";

#[must_use]
pub fn inspect(context: &InspectContext<'_>) -> InspectorOutput {
    let mut findings = Vec::new();

    dangling(context, &mut findings);
    oversized(context, &mut findings);
    unused(context, &mut findings);
    duplicates(context, &mut findings);

    InspectorOutput {
        findings,
        coverage: context.asset_coverage(),
    }
}

/// A reference in a scene that points at nothing.
///
/// Scripts are the Code Inspector's (`BHP-INS-201`) and instanced scenes are the Scene
/// Inspector's (`BHP-INS-102`); this is everything else — meshes, textures, materials,
/// fonts, shaders.
fn dangling(context: &InspectContext<'_>, findings: &mut Vec<Finding>) {
    for entry in &context.scenes {
        let Some(scene) = entry.parsed() else {
            continue;
        };
        for resource in &scene.document.ext_resources {
            if resource.type_ == "Script" || resource.type_ == "PackedScene" {
                continue;
            }
            if context.snapshot.resolves(&resource.path) {
                continue;
            }
            let rel = crate::godot::res_to_rel(&resource.path);
            collect(
                findings,
                Finding::draft(
                    InspectorId::Asset,
                    CODE_DANGLING_RESOURCE,
                    Severity::Critical,
                    INSPECT_CONFIDENCE_CERTAIN,
                    format!(
                        "{} references a {} that is not on disk",
                        entry.rel,
                        resource.type_.to_lowercase()
                    ),
                    Location::scene(&entry.rel).with_symbol(rel.clone()),
                )
                .cause(format!(
                    "The scene declares ext_resource type=\"{}\" path=\"{}\", and no file \
                     exists there.",
                    resource.type_, resource.path
                ))
                .impact(
                    "Godot substitutes a placeholder when it opens the scene. In the editor \
                     that is a missing-resource icon; in an export it is a mesh that is not \
                     drawn or a material that renders as flat magenta.",
                )
                .recommend(format!(
                    "Restore {rel}, or re-import the asset and re-point the reference."
                ))
                .evidence(
                    format!("path=\"{}\"", resource.path),
                    format!("{}#ext_resource {}", entry.rel, resource.id),
                ),
            );
        }
    }
}

/// Files big enough to be worth a decision.
fn oversized(context: &InspectContext<'_>, findings: &mut Vec<Finding>) {
    for asset in &context.assets {
        if let Some((width, height)) = asset.pixels {
            let longest = width.max(height);
            if longest > INSPECT_TEXTURE_LARGE_PIXELS {
                collect(
                    findings,
                    Finding::draft(
                        InspectorId::Asset,
                        CODE_OVERSIZED_TEXTURE,
                        Severity::Medium,
                        INSPECT_CONFIDENCE_CERTAIN,
                        format!("{} is {width} × {height}", asset.rel),
                        Location::asset(&asset.rel),
                    )
                    .cause(format!(
                        "The image header says {width} × {height}. Uncompressed in VRAM that \
                         is roughly {} MB for one texture.",
                        vram_megabytes(width, height)
                    ))
                    .impact(
                        "It occupies video memory whether or not anything is close enough to \
                         see the detail, and it is downloaded in full by every web player.",
                    )
                    .recommend(format!(
                        "Resize it to {INSPECT_TEXTURE_LARGE_PIXELS} px on the longest side \
                         or smaller unless it is a skybox, and let Godot's importer generate \
                         mipmaps."
                    ))
                    .evidence(
                        format!("{width} × {height} in the file header"),
                        asset.rel.clone(),
                    ),
                );
            }
        }

        let threshold = match asset.kind {
            AssetKind::Texture => INSPECT_TEXTURE_LARGE_BYTES,
            _ => INSPECT_MESH_LARGE_BYTES,
        };
        if asset.bytes > threshold {
            collect(
                findings,
                Finding::draft(
                    InspectorId::Asset,
                    CODE_LARGE_ASSET,
                    Severity::Medium,
                    INSPECT_CONFIDENCE_CERTAIN,
                    format!("{} is {}", asset.rel, megabytes(asset.bytes)),
                    Location::asset(&asset.rel),
                )
                .cause(format!(
                    "The file is {} on disk, over the {} this kind of asset is expected to \
                     stay under.",
                    megabytes(asset.bytes),
                    megabytes(threshold)
                ))
                .impact(
                    "It lengthens import, it lengthens load, and in a web export it is time \
                     the player spends looking at a progress bar.",
                )
                .recommend(
                    "Check whether the source resolution or polygon budget is higher than \
                     the game needs, and re-export it at the size it is actually used at.",
                )
                .evidence(format!("{} bytes", asset.bytes), asset.rel.clone()),
            );
        }
    }
}

/// Assets nothing names.
fn unused(context: &InspectContext<'_>, findings: &mut Vec<Finding>) {
    if !context.is_project_scope() {
        return;
    }
    for asset in &context.assets {
        if context.snapshot.referenced_anywhere(&asset.res)
            || context.snapshot.mentioned_in_scripts(&asset.rel)
        {
            continue;
        }
        collect(
            findings,
            Finding::draft(
                InspectorId::Asset,
                CODE_UNUSED_ASSET,
                Severity::Suggestion,
                INSPECT_CONFIDENCE_HEURISTIC,
                format!("Nothing in the project uses {}", asset.rel),
                Location::asset(&asset.rel),
            )
            .cause("No scene declares it as an ext_resource and no script names its path.")
            .impact(format!(
                "{} ships in the export for nothing. A handful is noise; a folder of them is \
                 a download the player pays for and never sees.",
                megabytes(asset.bytes)
            ))
            .recommend(
                "Use it, or move it out of the project — an unused asset with an unclear \
                 licence is also the one that causes trouble later.",
            )
            .evidence(
                "no ext_resource and no script mention".to_owned(),
                asset.rel.clone(),
            ),
        );
    }
}

/// The same bytes stored twice.
fn duplicates(context: &InspectContext<'_>, findings: &mut Vec<Finding>) {
    let mut by_hash: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for asset in &context.assets {
        let Some(hash) = asset.hash.as_deref() else {
            continue;
        };
        by_hash.entry(hash).or_default().push(asset.rel.as_str());
    }
    for (hash, mut paths) in by_hash {
        if paths.len() < 2 {
            continue;
        }
        paths.sort_unstable();
        let Some((keep, rest)) = paths.split_first() else {
            continue;
        };
        for duplicate in rest {
            collect(
                findings,
                Finding::draft(
                    InspectorId::Asset,
                    CODE_DUPLICATE_ASSET,
                    Severity::Suggestion,
                    INSPECT_CONFIDENCE_CERTAIN,
                    format!("{duplicate} is byte-for-byte {keep}"),
                    Location::asset(*duplicate),
                )
                .cause(format!(
                    "Both files hash to {}. They are the same asset stored twice.",
                    &hash[..hash.len().min(12)]
                ))
                .impact(
                    "Twice the disk, twice the import time, and two files to update the next \
                     time the art changes — which is how the two copies stop matching.",
                )
                .recommend(format!(
                    "Point every reference at {keep} and delete {duplicate}."
                ))
                .evidence(
                    format!("identical content hash {}", &hash[..hash.len().min(12)]),
                    (*duplicate).to_owned(),
                )
                .evidence("same hash".to_owned(), (*keep).to_owned()),
            );
        }
    }
}

/// RGBA8 without mipmaps, which is the honest floor for "what this costs in VRAM".
fn vram_megabytes(width: u32, height: u32) -> u64 {
    u64::from(width) * u64::from(height) * 4 / (1024 * 1024)
}

fn megabytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{} KB", bytes / 1024);
    }
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::context::test_support::{context_from, scene_entry};
    use crate::inspect::snapshot::{AssetEntry, ProjectSnapshot};

    fn asset(rel: &str, bytes: u64, kind: AssetKind) -> AssetEntry {
        AssetEntry {
            rel: rel.to_owned(),
            res: format!("res://{rel}"),
            bytes,
            kind,
            pixels: None,
            hash: Some(format!("hash-of-{rel}")),
        }
    }

    #[test]
    fn a_texture_bigger_than_the_ceiling_is_reported_with_its_real_dimensions() {
        let mut texture = asset("assets/wall.png", 1024, AssetKind::Texture);
        texture.pixels = Some((8192, 8192));
        let snapshot = ProjectSnapshot {
            assets: vec![texture],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let oversized = output
            .findings
            .iter()
            .find(|finding| finding.code == CODE_OVERSIZED_TEXTURE)
            .expect("the oversized texture is reported");
        assert!(oversized.title.contains("8192 × 8192"));
        assert_eq!(oversized.location.asset.as_deref(), Some("assets/wall.png"));
    }

    #[test]
    fn two_files_with_the_same_hash_produce_one_finding_naming_the_survivor() {
        let mut first = asset("assets/a.png", 10, AssetKind::Texture);
        let mut second = asset("assets/b.png", 10, AssetKind::Texture);
        first.hash = Some("abcdef0123456789".to_owned());
        second.hash = Some("abcdef0123456789".to_owned());
        let snapshot = ProjectSnapshot {
            assets: vec![first, second],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let duplicates: Vec<_> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_DUPLICATE_ASSET)
            .collect();
        assert_eq!(duplicates.len(), 1);
        assert_eq!(
            duplicates[0].location.asset.as_deref(),
            Some("assets/b.png")
        );
        assert!(duplicates[0].recommendation.contains("assets/a.png"));
    }

    #[test]
    fn an_asset_a_scene_references_is_used_and_one_nothing_names_is_not() {
        let snapshot = ProjectSnapshot {
            files: ["assets/rock.glb".to_owned(), "assets/spare.glb".to_owned()]
                .into_iter()
                .collect(),
            scenes: vec![scene_entry(
                "scenes/main.tscn",
                "[gd_scene load_steps=2 format=3]\n\n[ext_resource type=\"PackedScene\" path=\"res://assets/rock.glb\" id=\"1_r\"]\n\n[node name=\"Main\" type=\"Node3D\"]\n",
            )],
            assets: vec![
                asset("assets/rock.glb", 10, AssetKind::Mesh),
                asset("assets/spare.glb", 10, AssetKind::Mesh),
            ],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let unused: Vec<&str> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_UNUSED_ASSET)
            .filter_map(|finding| finding.location.asset.as_deref())
            .collect();
        assert_eq!(unused, vec!["assets/spare.glb"]);
    }

    #[test]
    fn a_missing_material_is_the_asset_inspectors_and_a_missing_script_is_not() {
        let snapshot = ProjectSnapshot {
            files: ["scenes/main.tscn".to_owned()].into_iter().collect(),
            scenes: vec![scene_entry(
                "scenes/main.tscn",
                "[gd_scene load_steps=3 format=3]\n\n[ext_resource type=\"Material\" path=\"res://assets/gold.tres\" id=\"1_m\"]\n[ext_resource type=\"Script\" path=\"res://scripts/gone.gd\" id=\"2_s\"]\n\n[node name=\"Main\" type=\"Node3D\"]\n",
            )],
            ..Default::default()
        };
        let output = inspect(&context_from(&snapshot));
        let dangling: Vec<&str> = output
            .findings
            .iter()
            .filter(|finding| finding.code == CODE_DANGLING_RESOURCE)
            .filter_map(|finding| finding.location.symbol.as_deref())
            .collect();
        assert_eq!(dangling, vec!["assets/gold.tres"]);
    }
}
