//! Integration test for Phase 7: Export Pipeline, Export Doctor, and Packaging (GAD-120…125).

use bhippi_engine::godot::export::{
    ensure_export_credits, package_export_zip, post_export_doctor, pre_export_doctor, ExportTarget,
};
use bhippi_engine::godot::scaffold::{write_project, ProjectTemplate};
use bhippi_engine::godot::templates::{check_export_templates, describe_template_offer};
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bhippi_exp_test_{}_{}", name, ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn pre_export_doctor_blocks_unlicensed_assets_in_release_inv074() {
    let root = temp_dir("inv074_block");
    write_project(
        &root,
        "Island Adventure",
        ProjectTemplate::ThirdPerson3D,
        true,
    )
    .unwrap();

    // Attach an asset with an unknown licence sidecar
    std::fs::create_dir_all(root.join("assets")).unwrap();
    let asset = root.join("assets").join("rock.glb");
    std::fs::write(&asset, b"mock_mesh_data").unwrap();
    let sidecar = root.join("assets").join("rock.glb.meta.json");
    std::fs::write(
        &sidecar,
        r#"{"license": "unknown", "provenance": {"source": "unverified"}}"#,
    )
    .unwrap();

    // Debug mode passes with a warning
    let debug_report = pre_export_doctor(&root, ExportTarget::Web, false);
    assert!(
        debug_report.passed,
        "debug should pass: {:?}",
        debug_report.blockers
    );
    assert!(!debug_report.warnings.is_empty());

    // Release mode strictly BLOCKS on unknown licence
    let release_report = pre_export_doctor(&root, ExportTarget::Web, true);
    assert!(
        !release_report.passed,
        "release must block on unknown licence"
    );
    assert!(
        release_report
            .blockers
            .iter()
            .any(|b| b.contains("BHP-GD-414") || b.contains("unknown")),
        "blockers: {:?}",
        release_report.blockers
    );

    // Correcting licence sidecar unblocks the release export
    std::fs::write(
        &sidecar,
        r#"{"license": "CC0-1.0", "provenance": {"source": "procedural"}}"#,
    )
    .unwrap();
    let fixed_report = pre_export_doctor(&root, ExportTarget::Web, true);
    assert!(
        fixed_report.passed,
        "release must pass once licence is valid: {:?}",
        fixed_report.blockers
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn post_export_doctor_validates_required_artefacts_and_credits() {
    let root = temp_dir("post_doctor");
    write_project(&root, "Runner Game", ProjectTemplate::ThirdPerson3D, true).unwrap();

    // Initially export folder does not exist -> blocks
    let missing_dir = post_export_doctor(&root, ExportTarget::Web);
    assert!(!missing_dir.passed);

    // Create export folder with missing files -> blocks
    let web_dir = root.join("export/web");
    std::fs::create_dir_all(&web_dir).unwrap();
    let empty_dir = post_export_doctor(&root, ExportTarget::Web);
    assert!(!empty_dir.passed);
    assert!(empty_dir
        .blockers
        .iter()
        .any(|b| b.contains("credits.html")));

    // Ensure credits
    let credits_file = ensure_export_credits(&root, ExportTarget::Web).unwrap();
    assert!(credits_file.is_file());

    // Write empty (corrupt) wasm -> blocks
    std::fs::write(web_dir.join("index.html"), b"<html></html>").unwrap();
    std::fs::write(web_dir.join("index.wasm"), b"").unwrap();
    std::fs::write(web_dir.join("index.pck"), b"GDPC").unwrap();
    let corrupt_report = post_export_doctor(&root, ExportTarget::Web);
    assert!(!corrupt_report.passed);
    assert!(corrupt_report
        .blockers
        .iter()
        .any(|b| b.contains("corrupt")));

    // Write valid wasm bytes -> passes
    std::fs::write(web_dir.join("index.wasm"), b"\x00asm\x01\x00\x00\x00").unwrap();
    let valid_report = post_export_doctor(&root, ExportTarget::Web);
    assert!(
        valid_report.passed,
        "must pass: {:?}",
        valid_report.blockers
    );
    assert_eq!(valid_report.artefacts_checked.len(), 4);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn zip_archive_builder_packages_export_reliably() {
    let root = temp_dir("zip_package");
    let web_dir = root.join("export/web");
    std::fs::create_dir_all(&web_dir).unwrap();

    std::fs::write(
        web_dir.join("index.html"),
        b"<!doctype html><html><body>Game</body></html>",
    )
    .unwrap();
    std::fs::write(web_dir.join("index.js"), b"console.log('game');").unwrap();
    std::fs::write(web_dir.join("index.wasm"), b"\x00asm\x01\x00\x00\x00").unwrap();
    std::fs::write(
        web_dir.join("credits.html"),
        b"<!doctype html><html><body>Credits</body></html>",
    )
    .unwrap();

    let zip_dest = root.join("dist").join("game_release.zip");
    let res = package_export_zip(&root, ExportTarget::Web, &zip_dest).unwrap();
    assert_eq!(res, zip_dest);
    assert!(zip_dest.is_file());

    let zip_bytes = std::fs::read(&zip_dest).unwrap();
    assert!(!zip_bytes.is_empty());
    // Starts with PK\x03\x04
    assert_eq!(&zip_bytes[0..4], b"PK\x03\x04");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn template_management_detects_and_offers_official_recipe() {
    let offer = describe_template_offer();
    assert_eq!(offer.version, "4.7.1-stable");
    assert!(offer.download_url.contains("4.7.1-stable"));
    assert_eq!(offer.expected_sha256.len(), 64);

    let status = check_export_templates(None);
    // Verified return structure contains required flags
    assert_eq!(status.version, "4.7.1-stable");
}
