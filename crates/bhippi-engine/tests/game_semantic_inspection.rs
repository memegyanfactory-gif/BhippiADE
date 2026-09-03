//! Frozen stage-06 semantic inspection over the canonical warehouse project.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::game_debug::{run, GameDebugMode, StageStatus};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct ExpectedFindings {
    schema: String,
    // Part of the fixture's declared shape: deserialising it is what proves the fixture
    // still carries the codes, even where a given test only asserts on `schema`.
    #[allow(dead_code)]
    expected_codes: Vec<String>,
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/fixtures/engine")
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

fn rewrite_scene(path: &Path, clear_levels: bool) {
    let mut document: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("scene text"))
            .expect("scene JSON");
    document["entities"] = json!([]);
    if clear_levels {
        document["settings"]["levels"] = json!([]);
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(&document).expect("scene serialises"),
    )
    .expect("scene writes");
}

#[test]
fn canonical_semantic_defects_are_stable_and_read_only() {
    let root =
        std::env::temp_dir().join(format!("bhippi-semantic-inspection-{}", ulid::Ulid::new()));
    copy_tree(&fixtures().join("warehouse_game"), &root);
    rewrite_scene(&root.join("assets/scenes/main.bscn.json"), true);
    rewrite_scene(&root.join("assets/scenes/level_01.bscn.json"), false);
    std::fs::write(
        root.join("scripts/level_01.rhai"),
        "fn on_update(dt) { while true { spawn(\"builtin:cube\", 0, 0, 0); } }",
    )
    .expect("hostile script writes");

    let expected: ExpectedFindings = serde_json::from_str(
        &std::fs::read_to_string(fixtures().join("quality/semantic-inspector-v1.json"))
            .expect("expected finding fixture"),
    )
    .expect("expected finding JSON");
    assert_eq!(expected.schema, "bhippi-semantic-inspector-fixture@1");

    let report = run(&root, GameDebugMode::Quick);
    let mut codes = report
        .findings
        .iter()
        .filter(|finding| finding.code.starts_with("BHP-GD-3"))
        .map(|finding| finding.code.clone())
        .collect::<Vec<_>>();
    codes.sort();
    assert_eq!(codes, vec!["BHP-GD-301", "BHP-GD-302", "BHP-GD-303"]);
    assert_eq!(report.outcome, "failed");
    assert!(report.authored_tree_unchanged());
    assert_eq!(
        report
            .stages
            .iter()
            .find(|stage| stage.id == "06_inspect")
            .expect("stage 06")
            .status,
        StageStatus::Failed
    );

    let _ignored = std::fs::remove_dir_all(root);
}
