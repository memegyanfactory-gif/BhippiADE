//! Running the project's own compilers and typecheckers, and reading what they say.
//!
//! Three defects in what this replaces, all of which made the old `/debug` report clean
//! projects that were not clean:
//!
//! * Stack detection was an exclusive `if / else if`, so a repository that is Rust **and**
//!   TypeScript — which this one is — only ever ran `cargo`.
//! * `tsc` was looked for only at the workspace root. This repository keeps its
//!   `tsconfig.json` in `ui/`, so it was never found.
//! * A flat 15-second budget covered `cargo check --workspace --all-targets`, which on a
//!   cold target directory cannot finish. The tool reported a timeout instead of findings,
//!   every time.
//!
//! Every stack that is present now runs, config files are discovered anywhere in the tree,
//! and each tool gets a budget matched to what it actually does.

use super::rules::{Category, Finding, Severity};
use super::walk::Found;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// `cargo check` over a cold target directory is minutes, not seconds. Fifteen seconds was
/// not a conservative budget, it was a guarantee of a timeout.
const CARGO_TIMEOUT: Duration = Duration::from_secs(180);
const TSC_TIMEOUT: Duration = Duration::from_secs(120);
const PYTHON_TIMEOUT: Duration = Duration::from_secs(60);

/// One toolchain that was found and run.
#[derive(Clone, Debug)]
pub struct ToolRun {
    pub tool: String,
    /// Where it ran, relative to the project root.
    pub at: String,
    pub ok: bool,
    /// Set when the tool could not run at all, as opposed to running and finding faults.
    pub note: Option<String>,
}

/// Everything the toolchains reported.
#[derive(Clone, Debug, Default)]
pub struct ToolReport {
    pub findings: Vec<Finding>,
    pub runs: Vec<ToolRun>,
    pub stacks: BTreeSet<String>,
}

/// Runs every toolchain the project actually has.
pub async fn run_all(root: &Path, files: &[Found]) -> ToolReport {
    let mut report = ToolReport::default();

    // Detection is additive. A repository is frequently more than one thing, and the
    // whole point of the rewrite is that all of them get checked.
    let cargo_manifests = manifests(root, files, "Cargo.toml");
    let tsconfigs = manifests(root, files, "tsconfig.json");
    let py_markers = ["pyproject.toml", "requirements.txt", "setup.py"]
        .iter()
        .any(|name| root.join(name).exists());

    if !cargo_manifests.is_empty() {
        report.stacks.insert("Rust (Cargo)".to_owned());
        // Only the topmost manifest: a workspace member's own manifest would re-check
        // everything the workspace root already covered.
        if let Some(manifest) = cargo_manifests.first() {
            run_cargo(root, manifest, &mut report).await;
        }
    }
    if !tsconfigs.is_empty() {
        report.stacks.insert("TypeScript".to_owned());
        // Every tsconfig, because a monorepo's packages are genuinely separate programs.
        for config in tsconfigs.iter().take(4) {
            run_tsc(root, config, &mut report).await;
        }
    }
    if py_markers {
        report.stacks.insert("Python".to_owned());
        run_python(root, &mut report).await;
    }
    if report.stacks.is_empty() {
        report.stacks.insert("Generic".to_owned());
    }

    report
}

/// Directories holding a named manifest, shallowest first.
fn manifests(root: &Path, files: &[Found], name: &str) -> Vec<PathBuf> {
    let mut found: Vec<(usize, PathBuf)> = Vec::new();
    if root.join(name).exists() {
        found.push((0, root.to_path_buf()));
    }
    for file in files {
        if !file.relative.ends_with(name) {
            continue;
        }
        let Some(parent) = file.path.parent() else {
            continue;
        };
        if parent == root {
            continue;
        }
        found.push((file.relative.matches('/').count(), parent.to_path_buf()));
    }
    found.sort_by_key(|(depth, _)| *depth);
    found.dedup_by(|a, b| a.1 == b.1);
    found.into_iter().map(|(_, path)| path).collect()
}

/// Builds a command that never flashes a console window on Windows.
fn command(program: &str, dir: &Path) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    cmd.current_dir(dir);
    cmd.stdin(std::process::Stdio::null());
    cmd.env("NO_COLOR", "1");
    cmd.kill_on_drop(true);
    // A background scan must never flash a console window in a desktop app.
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    cmd
}

/// `cargo clippy` when the component is installed, `cargo check` otherwise.
///
/// Clippy is preferred because it is a superset: it emits every `check` diagnostic *and*
/// the lints the project actually gates on. Running `check` alone made every lint this
/// repository denies invisible to its own debugger.
async fn run_cargo(root: &Path, at: &Path, report: &mut ToolReport) {
    let clippy = command("cargo", at)
        .args(["clippy", "--version"])
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false);

    let subcommand = if clippy { "clippy" } else { "check" };
    let mut cmd = command("cargo", at);
    cmd.args([
        subcommand,
        "--message-format=json",
        "--workspace",
        "--all-targets",
    ]);

    let at_label = super::walk::relative_of(root, at);
    let at_label = if at_label.is_empty() {
        ".".to_owned()
    } else {
        at_label
    };

    let output = match tokio::time::timeout(CARGO_TIMEOUT, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            report.runs.push(ToolRun {
                tool: format!("cargo {subcommand}"),
                at: at_label,
                ok: false,
                note: Some(format!("could not start: {error}")),
            });
            return;
        }
        Err(_) => {
            report.runs.push(ToolRun {
                tool: format!("cargo {subcommand}"),
                at: at_label,
                ok: false,
                note: Some(format!(
                    "still running after {}s — a cold target directory can exceed this; \
                     run it once in a terminal first",
                    CARGO_TIMEOUT.as_secs()
                )),
            });
            return;
        }
    };

    let before = report.findings.len();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(found) = cargo_diagnostic(line) {
            report.findings.push(found);
        }
    }
    report.runs.push(ToolRun {
        tool: format!("cargo {subcommand}"),
        at: at_label,
        ok: output.status.success(),
        note: (report.findings.len() == before && !output.status.success())
            .then(|| tail_of(&output.stderr)),
    });
}

/// One `compiler-message` line from cargo's JSON output.
fn cargo_diagnostic(line: &str) -> Option<Finding> {
    let line = line.trim();
    if !line.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("reason").and_then(serde_json::Value::as_str)? != "compiler-message" {
        return None;
    }
    let message = value.get("message")?;
    let level = message
        .get("level")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("error");
    let severity = match level {
        "error" | "error: internal compiler error" => Severity::Error,
        "warning" => Severity::Warning,
        // `note` and `help` are attached to a diagnostic that was already reported;
        // surfacing them separately triples the count and explains nothing new.
        _ => return None,
    };
    let text = message
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let code = message
        .get("code")
        .and_then(|code| code.get("code"))
        .and_then(serde_json::Value::as_str);

    let span = message
        .get("spans")
        .and_then(serde_json::Value::as_array)
        .and_then(|spans| {
            spans
                .iter()
                .find(|span| {
                    span.get("is_primary")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                })
                .or_else(|| spans.first())
        });

    Some(Finding {
        rule: "rustc",
        category: Category::Correctness,
        severity,
        file: span
            .and_then(|span| span.get("file_name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("(crate)")
            .replace('\\', "/"),
        line: span
            .and_then(|span| span.get("line_start"))
            .and_then(serde_json::Value::as_u64)
            .and_then(|line| u32::try_from(line).ok())
            .unwrap_or(1),
        message: text.to_owned(),
        why: "Reported by the Rust compiler against this workspace.",
        fix: "Follow the compiler's own suggestion; run the command in a terminal for the \
              full annotated span.",
        evidence: code.unwrap_or(level).to_owned(),
    })
}

/// `tsc --noEmit` against one tsconfig.
async fn run_tsc(root: &Path, at: &Path, report: &mut ToolReport) {
    // npx resolves the project's own TypeScript, which is the version that actually
    // governs the build; a globally installed tsc is frequently a different one.
    #[cfg(windows)]
    let mut cmd = {
        let mut cmd = command("cmd.exe", at);
        cmd.args([
            "/c",
            "npx",
            "--no-install",
            "tsc",
            "--noEmit",
            "--pretty",
            "false",
        ]);
        cmd
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut cmd = command("npx", at);
        cmd.args(["--no-install", "tsc", "--noEmit", "--pretty", "false"]);
        cmd
    };

    let at_label = super::walk::relative_of(root, at);
    let at_label = if at_label.is_empty() {
        ".".to_owned()
    } else {
        at_label
    };

    let output = match tokio::time::timeout(TSC_TIMEOUT, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            report.runs.push(ToolRun {
                tool: "tsc --noEmit".to_owned(),
                at: at_label,
                ok: false,
                note: Some(format!("could not start: {error}")),
            });
            return;
        }
        Err(_) => {
            report.runs.push(ToolRun {
                tool: "tsc --noEmit".to_owned(),
                at: at_label,
                ok: false,
                note: Some(format!("timed out after {}s", TSC_TIMEOUT.as_secs())),
            });
            return;
        }
    };

    // tsc prints diagnostics on stdout; a missing install lands on stderr.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut count = 0;
    for line in stdout.lines() {
        if let Some(found) = tsc_diagnostic(line, &at_label) {
            report.findings.push(found);
            count += 1;
        }
    }
    let missing = count == 0 && !output.status.success();
    report.runs.push(ToolRun {
        tool: "tsc --noEmit".to_owned(),
        at: at_label,
        ok: output.status.success(),
        note: missing.then(|| {
            let tail = tail_of(&output.stderr);
            if tail.is_empty() {
                "typescript is not installed here — run `npm install` first".to_owned()
            } else {
                tail
            }
        }),
    });
}

/// One `path(line,col): error TSxxxx: message` line.
pub(super) fn tsc_diagnostic(line: &str, at: &str) -> Option<Finding> {
    let trimmed = line.trim_end();
    if trimmed.is_empty() || trimmed.starts_with(' ') {
        // Indented lines are the continuation of the diagnostic above them.
        return None;
    }
    let (location, rest) = trimmed.split_once(": ")?;
    let (file, line_number) = split_location(location)?;
    let (tag, message) = rest.split_once(": ")?;
    let severity = if tag.starts_with("error") {
        Severity::Error
    } else if tag.starts_with("warning") {
        Severity::Warning
    } else {
        return None;
    };
    let code = tag.split_whitespace().last().unwrap_or(tag).to_owned();
    let file = if at == "." {
        file.replace('\\', "/")
    } else {
        format!("{at}/{}", file.replace('\\', "/"))
    };

    Some(Finding {
        rule: "tsc",
        category: Category::Correctness,
        severity,
        file,
        line: line_number,
        message: message.to_owned(),
        why: "Reported by the TypeScript compiler against this project's own tsconfig.",
        fix: "Fix the type error; run `tsc --noEmit` in that directory for the full context.",
        evidence: code,
    })
}

/// `file(line,col)` → the file and its line.
fn split_location(location: &str) -> Option<(&str, u32)> {
    let (file, position) = location.rsplit_once('(')?;
    let position = position.strip_suffix(')')?;
    let line = position.split(',').next()?.parse::<u32>().ok()?;
    Some((file, line))
}

/// A syntax-only pass over the Python in the project.
async fn run_python(root: &Path, report: &mut ToolReport) {
    let mut cmd = command("python", root);
    cmd.args([
        "-m",
        "compileall",
        "-q",
        "-x",
        r"(\.venv|venv|node_modules)",
        ".",
    ]);

    let output = match tokio::time::timeout(PYTHON_TIMEOUT, cmd.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            report.runs.push(ToolRun {
                tool: "python -m compileall".to_owned(),
                at: ".".to_owned(),
                ok: false,
                note: Some(format!("could not start: {error}")),
            });
            return;
        }
        Err(_) => {
            report.runs.push(ToolRun {
                tool: "python -m compileall".to_owned(),
                at: ".".to_owned(),
                ok: false,
                note: Some(format!("timed out after {}s", PYTHON_TIMEOUT.as_secs())),
            });
            return;
        }
    };

    for line in String::from_utf8_lossy(&output.stderr).lines() {
        let line = line.trim();
        if line.is_empty() || !line.contains("Error") {
            continue;
        }
        report.findings.push(Finding {
            rule: "python",
            category: Category::Correctness,
            severity: Severity::Error,
            file: "(python)".to_owned(),
            line: 1,
            message: line.chars().take(240).collect(),
            why: "Reported by CPython while byte-compiling the project.",
            fix: "Fix the syntax error at the file and line named above.",
            evidence: "compileall".to_owned(),
        });
    }
    report.runs.push(ToolRun {
        tool: "python -m compileall".to_owned(),
        at: ".".to_owned(),
        ok: output.status.success(),
        note: None,
    });
}

/// The last few non-empty lines of a stream, which is where a tool explains itself.
fn tail_of(stream: &[u8]) -> String {
    let text = String::from_utf8_lossy(stream);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let start = lines.len().saturating_sub(3);
    lines[start..].join(" · ").chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::{cargo_diagnostic, split_location, tsc_diagnostic};
    use crate::debugger::rules::Severity;

    /// The shape cargo actually emits, including the `note` level that must be dropped.
    #[test]
    fn cargo_json_yields_the_diagnostic_and_drops_its_attachments() {
        let error = concat!(
            r#"{"reason":"compiler-message","message":{"level":"error","#,
            r#""message":"cannot find value `x`","code":{"code":"E0425"},"#,
            r#""spans":[{"is_primary":true,"file_name":"src\\main.rs","line_start":7}]}}"#
        );
        let Some(found) = cargo_diagnostic(error) else {
            panic!("a compiler error must be read");
        };
        assert_eq!(found.severity, Severity::Error);
        assert_eq!(found.file, "src/main.rs", "paths must be forward-slashed");
        assert_eq!(found.line, 7);
        assert_eq!(found.evidence, "E0425");

        // `note` and `help` belong to the diagnostic above them; surfacing them
        // separately triples the count and explains nothing new.
        let note = concat!(
            r#"{"reason":"compiler-message","message":{"level":"note","#,
            r#""message":"defined here","spans":[]}}"#
        );
        assert!(cargo_diagnostic(note).is_none());

        // Non-diagnostic lines and prose must not be mistaken for findings.
        assert!(cargo_diagnostic(r#"{"reason":"build-finished","success":true}"#).is_none());
        assert!(cargo_diagnostic("   Compiling bhippi-app v0.1.0").is_none());
    }

    /// The tsc line format, and the continuation lines that must not double-count.
    #[test]
    fn tsc_output_yields_one_finding_per_diagnostic() {
        let Some(found) = tsc_diagnostic("src/App.tsx(42,10): error TS2322: Type mismatch", "ui")
        else {
            panic!("a tsc error must be read");
        };
        assert_eq!(found.severity, Severity::Error);
        // The path is rooted at the tsconfig's own directory, so it is clickable from
        // the project root rather than relative to a directory the user cannot see.
        assert_eq!(found.file, "ui/src/App.tsx");
        assert_eq!(found.line, 42);
        assert_eq!(found.evidence, "TS2322");
        assert_eq!(found.message, "Type mismatch");

        assert!(tsc_diagnostic("    Property 'x' is missing.", "ui").is_none());
        assert!(tsc_diagnostic("", "ui").is_none());
        assert!(tsc_diagnostic("Found 3 errors.", "ui").is_none());
    }

    #[test]
    fn a_location_splits_into_a_file_and_a_line() {
        assert_eq!(split_location("src/a.ts(9,4)"), Some(("src/a.ts", 9)));
        assert_eq!(split_location("src/a.ts"), None);
    }
}
