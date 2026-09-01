//! The compiler and the VM are in different languages, so nothing but a shared artefact
//! keeps them honest (ADR-0030).
//!
//! This test compiles `ui/tests/fixtures/pickup.rhai` and asserts the result matches the
//! committed `pickup.program.json` that `ui/tests/play-runtime.test.mjs` executes. A change
//! to the bytecode therefore fails *here*, loudly, instead of silently producing a program
//! the webview VM misreads at run time.
//!
//! Regenerate deliberately with `BHIPPI_UPDATE_FIXTURES=1 cargo test -p bhippi-engine`.

// Tests may panic on purpose: `expect` states a precondition, and a panic here is a failing
// test rather than a crashed app. The workspace-wide `deny` stands in shipping code.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::script::{compile, ScriptProgram};
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ui/tests/fixtures")
        .canonicalize()
        .expect("the UI fixture directory is part of the repo")
}

#[test]
fn the_committed_program_is_what_the_compiler_still_emits() {
    let dir = fixture_dir();
    let source = std::fs::read_to_string(dir.join("pickup.rhai")).expect("fixture source");
    let program = compile("ui/tests/fixtures/pickup.rhai", &source).expect("the fixture compiles");
    let json = serde_json::to_string_pretty(&program).expect("programs are JSON-safe");

    let committed = dir.join("pickup.program.json");
    if std::env::var("BHIPPI_UPDATE_FIXTURES").is_ok() {
        std::fs::write(&committed, format!("{json}\n")).expect("fixture is writable");
        return;
    }

    let expected = std::fs::read_to_string(&committed).expect("committed program fixture");
    let expected: ScriptProgram =
        serde_json::from_str(&expected).expect("the committed fixture is a program");
    assert_eq!(
        program, expected,
        "the compiler's output changed; re-run with BHIPPI_UPDATE_FIXTURES=1 and check the VM \
         still executes the new program (ui/tests/play-runtime.test.mjs)"
    );
}

#[test]
fn the_fixture_exercises_the_hooks_the_vm_test_calls() {
    let dir = fixture_dir();
    let source = std::fs::read_to_string(dir.join("pickup.rhai")).expect("fixture source");
    let program = compile("pickup.rhai", &source).expect("compiles");
    assert_eq!(
        program.hook_names(),
        vec!["on_start", "on_trigger", "on_update"]
    );
    for host in [
        "set_var",
        "get_var",
        "hud_set",
        "destroy",
        "load_level",
        "has_tag",
    ] {
        assert!(
            program.hosts.iter().any(|name| name == host),
            "the fixture should call {host} so the VM test binds it"
        );
    }
}
