//! A test states its preconditions with `expect`: a panic here is a failing test, not a
//! crashed app. The workspace `deny` stands everywhere else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const CRATES: [&str; 8] = [
    "bhippi-app",
    "bhippi-core",
    "bhippi-db",
    "bhippi-engine",
    "bhippi-memory",
    "bhippi-providers",
    "bhippi-skills",
    "bhippi-types",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| panic!("bhippi-types must live under <workspace>/crates"))
}

fn allowed_edges() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    BTreeMap::from([
        ("bhippi-engine", BTreeSet::from(["bhippi-types"])),
        ("bhippi-types", BTreeSet::from([])),
        ("bhippi-db", BTreeSet::from(["bhippi-types"])),
        ("bhippi-providers", BTreeSet::from(["bhippi-types"])),
        (
            // ADR-0024: the World Brain mirrors the engine's scene graph persistently.
            "bhippi-memory",
            BTreeSet::from([
                "bhippi-db",
                "bhippi-engine",
                "bhippi-providers",
                "bhippi-types",
            ]),
        ),
        (
            "bhippi-skills",
            BTreeSet::from(["bhippi-providers", "bhippi-types"]),
        ),
        (
            "bhippi-core",
            BTreeSet::from(["bhippi-skills", "bhippi-types"]),
        ),
        (
            // ADR-0008: the shell also uses providers (chat streaming) and db
            // (repositories behind IPC commands) directly.
            // ADR-0020: the shell drives the engine workbench through bhippi-engine.
            // ADR-0024: the shell drives Project Brain indexing/search through bhippi-memory.
            "bhippi-app",
            BTreeSet::from([
                "bhippi-core",
                "bhippi-db",
                "bhippi-engine",
                "bhippi-memory",
                "bhippi-providers",
                "bhippi-types",
            ]),
        ),
    ])
}

fn dependency_names(manifest: &str) -> BTreeSet<String> {
    let mut dependencies = BTreeSet::new();
    let mut in_dependencies = false;

    for raw_line in manifest.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') {
            in_dependencies = line.ends_with("dependencies]")
                || line.contains(".dependencies]")
                || line.ends_with("dev-dependencies]")
                || line.ends_with("build-dependencies]");
            continue;
        }

        if !in_dependencies || line.is_empty() {
            continue;
        }

        if let Some((name, _)) = line.split_once('=') {
            dependencies.insert(name.trim().replace('_', "-"));
        }
    }

    dependencies
}

#[test]
fn workspace_contains_exactly_the_authoritative_crates() {
    let crates_dir = workspace_root().join("crates");
    let mut actual = fs::read_dir(&crates_dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", crates_dir.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("Cargo.toml").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    actual.sort();

    assert_eq!(actual, CRATES);
}

#[test]
fn workspace_dependency_edges_match_the_architecture() {
    let root = workspace_root();
    let allowed = allowed_edges();
    let workspace_crates = CRATES.into_iter().collect::<BTreeSet<_>>();

    for crate_name in CRATES {
        let manifest_path = root.join("crates").join(crate_name).join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", manifest_path.display()));
        let actual_workspace_edges = dependency_names(&manifest)
            .into_iter()
            .filter(|dependency| workspace_crates.contains(dependency.as_str()))
            .collect::<BTreeSet<_>>();
        let permitted = allowed
            .get(crate_name)
            .unwrap_or_else(|| panic!("missing dependency policy for {crate_name}"));

        for dependency in actual_workspace_edges {
            assert!(
                permitted.contains(dependency.as_str()),
                "{crate_name} may not depend on {dependency}"
            );
        }
    }
}

/// INV-073 / ENG-105: the webview computes nothing for the engine.
///
/// The Engine pane used to keep the scene in React state and write the whole file itself,
/// which is how INV-070's single write path was quietly broken for months. Scene mutation
/// now belongs to `bhippi-engine`; the pane dispatches actions and renders what comes back.
/// This test is the structural guard so that cannot come back by accident.
/// GAD-100 / ADR-0043: The in-house webview engine has been completely retired.
/// `ui/src/engine` must not exist on disk.
#[test]
fn the_webview_engine_is_completely_retired() {
    let engine_ui = workspace_root().join("ui").join("src").join("engine");
    assert!(
        !engine_ui.exists(),
        "ui/src/engine still exists; the webview engine was retired in Phase G5 (ADR-0043)"
    );
}

/// INV-088: The Godot UI and studio panes never write `.tscn` or `.gd` directly;
/// all mutations go through the typed Godot action protocol (`apply_batch_for`).
#[test]
fn the_godot_ui_never_writes_project_files_directly() {
    let godot_ui = workspace_root().join("ui").join("src").join("godot");
    if !godot_ui.exists() {
        return;
    }
    let entries = fs::read_dir(&godot_ui)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", godot_ui.display()));

    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|ext| ext == "ts" || ext == "tsx")
        {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        let code: String = source
            .lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !(trimmed.starts_with("//")
                    || trimmed.starts_with('*')
                    || trimmed.starts_with("/*"))
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            !code.contains("api.writeFile"),
            "{} writes a file directly; all Godot project changes must go through the typed batch action path (INV-088)",
            path.display()
        );
    }
}

/// GAD-108: Verify that none of the retired crates are referenced in workspace members.
#[test]
fn removed_crates_are_not_in_workspace() {
    let cargo_toml = workspace_root().join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml).expect("read Cargo.toml");

    let retired_crates = [
        "bhippi-engine-viewport",
        "bhippi-engine-build",
        "bhippi-harvest",
        "bhippi-publish",
        "bhippi-research",
        "bhippi-seo",
        "bhippi-ticker",
        "bhippi-vision",
        "bhippi-writer",
    ];

    for retired in retired_crates {
        assert!(
            !content.contains(&format!("\"{retired}\""))
                && !content.contains(&format!("crates/{retired}")),
            "Cargo.toml still references retired crate `{retired}`"
        );
    }
}
