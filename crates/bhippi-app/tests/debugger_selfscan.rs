//! Runs the debugger against a seeded project shaped like a real one, and asserts it
//! reports what a compiler never would.
//!
//! Kept as a test rather than a script so the guarantee — "this finds real bugs" — is
//! checked on every run instead of demonstrated once and then quietly regressing.

use bhippi_app::debugger;
use std::path::{Path, PathBuf};

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "bhippi-selfscan-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    assert!(std::fs::create_dir_all(&dir).is_ok());
    dir
}

fn write(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        assert!(std::fs::create_dir_all(parent).is_ok());
    }
    assert!(std::fs::write(path, body).is_ok());
}

/// A realistic mixed-stack project, with every defect placed where the old scanner could
/// not reach it: inside nested directories, and beside `node_modules` and `target` trees
/// that must be skipped entirely.
#[tokio::test]
async fn a_realistic_project_yields_findings_no_compiler_would() {
    let root = scratch();

    // Noise that must be ignored, and which the old walker would have drowned in.
    write(
        &root,
        "node_modules/left-pad/index.js",
        "eval(process.argv[2]);\n",
    );
    write(&root, "target/debug/gen.rs", "let x = None.unwrap();\n");
    write(&root, "dist/bundle.js", "console.log('bundled');\n");

    // Real source, nested.
    write(&root, "package.json", "{\"name\":\"probe\"}");
    write(&root, ".gitignore", "target\n");
    write(&root, ".env", "API_TOKEN=placeholder\n");
    write(
        &root,
        "src/api/client.ts",
        concat!(
            "import { parse } from '../util/parse';\n",
            "import { gone } from './removed-in-refactor';\n",
            "export function load(raw: string) {\n",
            "  try { return parse(raw); } catch {}\n",
            "  if (raw == null) return undefined;\n",
            "}\n",
        ),
    );
    write(
        &root,
        "src/util/parse.ts",
        "export const parse = JSON.parse;\n",
    );
    write(
        &root,
        "src/ui/List.tsx",
        "export const List = ({ rows }) => <ul>{rows.map((r) => <li>{r}</li>)}</ul>;\n",
    );
    write(
        &root,
        "src/api/client.test.ts",
        "describe.only('client', () => { it('loads', () => {}); });\n",
    );

    let Ok(report) = debugger::run_diagnostics(&root).await else {
        panic!("the scan must complete");
    };

    let codes: Vec<&str> = report
        .items
        .iter()
        .filter_map(|item| item.code.as_deref())
        .collect();

    // Each of these is a genuine defect that compiles and typechecks perfectly.
    assert!(codes.contains(&"BHP-D007"), "broken import: {codes:?}");
    assert!(codes.contains(&"BHP-D003"), "swallowed error: {codes:?}");
    assert!(codes.contains(&"BHP-D004"), "loose equality: {codes:?}");
    assert!(codes.contains(&"BHP-D005"), "missing key prop: {codes:?}");
    assert!(codes.contains(&"BHP-D002"), "focused suite: {codes:?}");
    assert!(codes.contains(&"BHP-D024"), "unignored .env: {codes:?}");

    // Nothing inside a skipped directory may ever be reported: those findings are noise
    // the user cannot act on, and they are what buried real findings before.
    for item in &report.items {
        assert!(
            !item.file.contains("node_modules")
                && !item.file.starts_with("target/")
                && !item.file.starts_with("dist/"),
            "a skipped directory leaked into the report: {}",
            item.file
        );
    }

    // The import that resolves must not be reported alongside the one that does not.
    let broken: Vec<&str> = report
        .items
        .iter()
        .filter(|item| item.code.as_deref() == Some("BHP-D007"))
        .map(|item| item.message.as_str())
        .collect();
    assert_eq!(broken.len(), 1, "{broken:?}");
    assert!(broken[0].contains("removed-in-refactor"), "{broken:?}");

    assert!(!report.success, "planted errors must fail the report");
    assert!(!report.partial, "this project is far inside every budget");
    assert!(report.bytes_scanned > 0);

    // Every finding must be actionable: what, why, and the fix.
    for item in &report.items {
        assert!(!item.message.is_empty(), "{:?}", item.code);
        assert!(!item.why.is_empty(), "{:?} has no rationale", item.code);
        assert!(item.suggestion.is_some(), "{:?} has no fix", item.code);
    }

    let _ignored = std::fs::remove_dir_all(root);
}
