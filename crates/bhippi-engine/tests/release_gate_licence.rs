//! Release-gate proof: an export with an unknown licence fails (INV-074, GAD-119).
//!
//! A ticket is done only when code and tests exist. This test asserts the block,
//! proving that `bhippi-engine` refuses Release exports when any asset carries
//! an unknown licence or lacks a licence sidecar.

use bhippi_engine::godot::gates::{check_project, CODE_LICENSE_MISSING, CODE_LICENSE_UNKNOWN};
use bhippi_engine::godot::scaffold::{write_project, ProjectTemplate};
use std::path::PathBuf;

fn make_temp_dir() -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("bhippi_gate_proof_{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn export_with_unknown_sidecar_blocks_release_export() {
    let root = make_temp_dir();

    // 1. Scaffold a clean minimal Godot project.
    write_project(&root, "GateTestGame", ProjectTemplate::ThirdPerson3D, true).unwrap();

    // Initially clean project passes both debug and release checks.
    let clean_debug = check_project(&root, false);
    assert!(clean_debug.passes(), "scaffolded project passes debug");

    // 2. Add an asset with an unknown licence sidecar.
    let assets_dir = root.join("assets/audio");
    std::fs::create_dir_all(&assets_dir).unwrap();
    let asset_file = assets_dir.join("bgm.wav");
    std::fs::write(&asset_file, b"RIFF....WAVEfmt ").unwrap();

    let sidecar_file = assets_dir.join("bgm.wav.meta.json");
    let unknown_sidecar = serde_json::json!({
        "license": "unknown",
        "provenance": "unverified_download"
    });
    std::fs::write(
        &sidecar_file,
        serde_json::to_string_pretty(&unknown_sidecar).unwrap(),
    )
    .unwrap();

    // 3. Debug check: emits a warning, but passes.
    let debug_report = check_project(&root, false);
    assert!(
        debug_report.passes(),
        "debug mode allows unknown licences with a warning"
    );
    assert!(
        debug_report.has(CODE_LICENSE_UNKNOWN),
        "debug report carries license_unknown code"
    );

    // 4. Release check: MUST BLOCK! Assert the block, not the pass.
    let release_report = check_project(&root, true);
    assert!(
        !release_report.passes(),
        "INV-074 violation: release build must block when an asset has an unknown licence"
    );
    assert!(
        release_report.has(CODE_LICENSE_UNKNOWN),
        "release report must explicitly name the CODE_LICENSE_UNKNOWN blocker"
    );
    assert!(
        release_report
            .blockers
            .iter()
            .any(|b| b.code == CODE_LICENSE_UNKNOWN),
        "license_unknown must be in the blocker list, not just warning list"
    );

    // 5. Missing sidecar also blocks release.
    std::fs::remove_file(&sidecar_file).unwrap();
    let missing_release_report = check_project(&root, true);
    assert!(
        !missing_release_report.passes(),
        "INV-074 violation: release build must block when an asset has no sidecar"
    );
    assert!(
        missing_release_report.has(CODE_LICENSE_MISSING),
        "missing sidecar must trigger CODE_LICENSE_MISSING"
    );

    // 6. Fixing the sidecar with a valid licence unblocks the release gate.
    let valid_sidecar = serde_json::json!({
        "license": "CC0-1.0",
        "author": "Kenney",
        "provenance": {
            "source": "bundled_cc0_library"
        }
    });
    std::fs::write(
        &sidecar_file,
        serde_json::to_string_pretty(&valid_sidecar).unwrap(),
    )
    .unwrap();

    let fixed_release_report = check_project(&root, true);
    assert!(
        fixed_release_report.passes(),
        "once licensed, release export passes cleanly"
    );

    let _ = std::fs::remove_dir_all(root);
}
