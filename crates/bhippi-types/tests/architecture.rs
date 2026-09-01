use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const CRATES: [&str; 17] = [
    "bhippi-app",
    "bhippi-core",
    "bhippi-db",
    "bhippi-engine",
    "bhippi-engine-build",
    "bhippi-engine-viewport",
    "bhippi-harvest",
    "bhippi-memory",
    "bhippi-providers",
    "bhippi-publish",
    "bhippi-research",
    "bhippi-seo",
    "bhippi-skills",
    "bhippi-ticker",
    "bhippi-types",
    "bhippi-vision",
    "bhippi-writer",
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
        (
            "bhippi-engine-build",
            BTreeSet::from(["bhippi-engine", "bhippi-types"]),
        ),
        (
            // ADR-0020: the viewport is the only crate that links Bevy (feature-gated).
            "bhippi-engine-viewport",
            BTreeSet::from(["bhippi-engine", "bhippi-types"]),
        ),
        ("bhippi-types", BTreeSet::from([])),
        ("bhippi-db", BTreeSet::from(["bhippi-types"])),
        ("bhippi-providers", BTreeSet::from(["bhippi-types"])),
        (
            "bhippi-harvest",
            BTreeSet::from(["bhippi-db", "bhippi-types"]),
        ),
        (
            "bhippi-research",
            BTreeSet::from([
                "bhippi-harvest",
                "bhippi-memory",
                "bhippi-providers",
                "bhippi-types",
            ]),
        ),
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
            "bhippi-vision",
            BTreeSet::from(["bhippi-harvest", "bhippi-providers", "bhippi-types"]),
        ),
        (
            "bhippi-writer",
            BTreeSet::from(["bhippi-providers", "bhippi-types"]),
        ),
        (
            "bhippi-seo",
            BTreeSet::from(["bhippi-providers", "bhippi-types"]),
        ),
        (
            "bhippi-ticker",
            BTreeSet::from(["bhippi-db", "bhippi-harvest", "bhippi-types"]),
        ),
        (
            "bhippi-skills",
            BTreeSet::from(["bhippi-providers", "bhippi-types"]),
        ),
        (
            "bhippi-publish",
            BTreeSet::from(["bhippi-seo", "bhippi-types"]),
        ),
        (
            "bhippi-core",
            BTreeSet::from([
                "bhippi-db",
                "bhippi-harvest",
                "bhippi-memory",
                "bhippi-providers",
                "bhippi-publish",
                "bhippi-research",
                "bhippi-seo",
                "bhippi-skills",
                "bhippi-ticker",
                "bhippi-types",
                "bhippi-vision",
                "bhippi-writer",
            ]),
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
#[test]
fn the_webview_never_writes_a_scene_or_computes_engine_state() {
    let engine_ui = workspace_root().join("ui").join("src").join("engine");
    let entries = fs::read_dir(&engine_ui)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", engine_ui.display()));

    // Names of functions that only ever existed to do the engine's job in TypeScript.
    let forbidden_symbols = [
        "createDefaultEntity",
        "createStarterSceneDoc",
        "duplicateEntity(",
        "newEntityId",
        "applyWeather",
        "mergeScenes",
    ];

    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|ext| ext == "ts" || ext == "tsx")
        {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));

        // Comments explaining what moved to Rust are fine; code is not.
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
            "{name} writes a file directly; scene writes must go through engine_apply_action \
             / engine_save_scene so they are transacted and journaled (INV-070)"
        );
        for symbol in forbidden_symbols {
            assert!(
                !code.contains(symbol),
                "{name} defines or calls `{symbol}` — that logic belongs in bhippi-engine \
                 (INV-073); the pane may only render engine state"
            );
        }
    }
}

/// INV-082 / ENG-176: the webview executes scripts, it does not interpret them.
///
/// ADR-0030 splits gameplay scripting in two — `bhippi-engine::script` lexes, parses,
/// validates and compiles; `ui/src/engine/scriptVm.ts` runs the bytecode. The split is only
/// worth anything while the second half stays a VM. `eval`, `new Function` or a hand-rolled
/// tokenizer in the pane would put the language's semantics back in TypeScript, where the
/// spans, the sandbox and the step budget all cease to exist.
#[test]
fn the_webview_executes_compiled_scripts_and_never_interprets_source() {
    let engine_ui = workspace_root().join("ui").join("src").join("engine");
    let entries = fs::read_dir(&engine_ui)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", engine_ui.display()));

    // Each is a way to turn text into behaviour at run time. None belongs in the pane.
    let forbidden = ["eval(", "new Function(", "Function(\"", "importScripts("];

    let mut saw_vm = false;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|ext| ext == "ts" || ext == "tsx")
        {
            continue;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if name == "scriptVm.ts" {
            saw_vm = true;
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

        for symbol in forbidden {
            assert!(
                !code.contains(symbol),
                "{name} contains `{symbol}` — gameplay scripts are compiled in \
                 bhippi-engine::script and executed as bytecode (INV-082, ADR-0030); the pane \
                 must never turn text into behaviour"
            );
        }
    }

    assert!(
        saw_vm,
        "ui/src/engine/scriptVm.ts is missing — INV-082 names it as the only execution path"
    );
}
