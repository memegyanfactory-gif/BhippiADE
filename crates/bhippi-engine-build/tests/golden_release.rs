//! Network-free Phase-8 release lane over the committed canonical fixtures (ENG-199).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine_build::{prepare, BuildMode};
use bhippi_types::EntityId;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("engine")
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("target directory");
    let mut entries = std::fs::read_dir(source)
        .expect("source directory")
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to);
        } else {
            std::fs::copy(from, to).expect("copy fixture");
        }
    }
}

#[test]
fn committed_fixture_tree_matches_its_hash_manifest() {
    let root = fixtures();
    let expected: BTreeMap<String, String> = serde_json::from_str(
        &std::fs::read_to_string(root.join("fixture-hashes.json")).expect("hash manifest"),
    )
    .expect("hash JSON");
    for (relative, hash) in expected {
        let bytes = std::fs::read(root.join(&relative)).expect("hashed fixture");
        assert_eq!(blake3::hash(&bytes).to_hex().as_str(), hash, "{relative}");
    }
}

#[test]
fn warehouse_preflight_builds_windows_and_web_offline() {
    let temp = std::env::temp_dir().join(format!("bhippi-release-{}", EntityId::new()));
    copy_tree(&fixtures().join("warehouse_game"), &temp);
    let debug = prepare(&temp, BuildMode::Debug).expect("debug preflight");
    assert!(debug.targets.iter().any(|target| target == "windows"));
    assert!(debug.targets.iter().any(|target| target == "web"));
    assert!(Path::new(&debug.artifact_dir)
        .join("build-report.json")
        .is_file());
    let release = prepare(&temp, BuildMode::Release).expect("release preflight");
    assert!(release.report.is_clear());
    let _ignored = std::fs::remove_dir_all(temp);
}

#[test]
fn unlicensed_release_is_blocked_and_names_the_exact_asset() {
    let temp = std::env::temp_dir().join(format!("bhippi-unlicensed-{}", EntityId::new()));
    copy_tree(&fixtures().join("unlicensed_release"), &temp);
    let debug = prepare(&temp, BuildMode::Debug).expect("debug warn-list");
    assert!(debug
        .report
        .warnings
        .iter()
        .any(|warning| warning.contains("assets/textures/unlicensed.png")));
    let error = prepare(&temp, BuildMode::Release).expect_err("release must block");
    assert!(error.to_string().contains("content blocker"));
    let _ignored = std::fs::remove_dir_all(temp);
}
