//! Deterministic workspace debugger — zero LLM calls, same answer every run.
//!
//! Three passes, in order of how much they cost:
//!
//! 1. **Walk** the project once, with real ignore rules and stated budgets (`walk`).
//! 2. **Analyse** every file line by line against a hardcoded rule set (`rules`), which is
//!    where the findings a compiler cannot produce come from — a committed credential, a
//!    `.only(` that has disabled a suite, an import naming a file nobody created.
//! 3. **Compile**, running every toolchain the project actually has (`toolchain`).
//!
//! Determinism is the point. A debugger whose answers move between runs cannot be put in a
//! gate and cannot be argued with, so nothing here consults a model and every finding is a
//! pure function of the bytes on disk.

pub mod rules;
pub mod toolchain;
pub mod walk;

use rules::{Category, FileContext, Finding, Lang, Line, Severity};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;
use walk::Budget;

/// The most findings one report will carry.
///
/// A project with ten thousand `TODO`s produces a report nobody reads and a payload that
/// stalls the renderer. The cap is applied *after* sorting by severity, so what survives
/// truncation is always the worst of what was found, never an arbitrary prefix.
const MAX_FINDINGS: usize = 400;

/// A finished scan, as the UI renders it.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct DiagnosticReport {
    pub project_name: String,
    /// Every stack found, joined — a repository is frequently more than one thing.
    pub project_type: String,
    pub total_issues: usize,
    pub errors_count: usize,
    pub warnings_count: usize,
    pub info_count: usize,
    pub duration_ms: u64,
    /// How many files were actually read.
    pub files_scanned: usize,
    /// How much source those files held. A file count alone hides whether a scan
    /// covered a real project or a directory of stubs.
    pub bytes_scanned: u64,
    /// True when a budget stopped the scan early. A partial scan is not a clean one, and
    /// the UI must say so rather than reporting a pass.
    pub partial: bool,
    pub items: Vec<DiagnosticItem>,
    /// One line per toolchain that ran, so a tool that could not start is visible rather
    /// than silently contributing nothing.
    pub tools: Vec<ToolStatus>,
    /// Counts per category, for the report header.
    pub by_category: Vec<CategoryCount>,
    pub summary: String,
    pub success: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct DiagnosticItem {
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub severity: String,
    pub category: String,
    /// The rule id (`BHP-D001`) or the tool that reported it (`rustc`, `tsc`).
    pub code: Option<String>,
    pub message: String,
    /// Why this is a defect rather than a preference.
    pub why: String,
    pub suggestion: Option<String>,
    /// The offending source, trimmed.
    pub evidence: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ToolStatus {
    pub tool: String,
    pub at: String,
    pub ok: bool,
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CategoryCount {
    pub category: String,
    pub count: usize,
}

/// Runs every pass over `workspace`.
///
/// # Errors
/// When the path is not a directory that can be read.
pub async fn run_diagnostics(workspace: &Path) -> Result<DiagnosticReport, String> {
    let started = Instant::now();
    if !workspace.is_dir() {
        return Err(format!(
            "{} is not a directory that can be scanned.",
            workspace.display()
        ));
    }
    let project_name = workspace
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace")
        .to_owned();

    // ── 1 · Walk ────────────────────────────────────────────────────────────
    let budget = Budget::default();
    let walked = walk::walk(workspace, budget).await;

    // ── 2 · Analyse ─────────────────────────────────────────────────────────
    let mut findings: Vec<Finding> = Vec::new();
    let mut relatives: HashSet<String> = HashSet::new();
    let mut bodies: HashMap<String, String> = HashMap::new();

    for file in &walked.files {
        relatives.insert(file.relative.clone());
        let Some(body) = walk::read_text(&file.path).await else {
            continue;
        };
        let lang = Lang::of(&file.extension);
        let mut context = FileContext::for_path(&file.relative);

        for (index, text) in body.lines().enumerate() {
            // Blanking strings and comments first is what stops a comment reading
            // "never call eval()" from being reported as a call to eval.
            let code = rules::strip_literals(text);
            context.observe(text);
            let line = Line {
                relative: &file.relative,
                lang,
                number: u32::try_from(index + 1).unwrap_or(u32::MAX),
                text,
                code: &code,
                context: &context,
            };
            findings.extend(rules::check_line(&line));
        }
        findings.extend(rules::check_file(&file.relative, lang, &body));

        // Only the files a cross-file rule needs are kept, so a large project does not
        // hold its entire source in memory to resolve a few imports.
        if matches!(lang, Lang::Web) {
            bodies.insert(file.relative.clone(), body);
        }
    }
    findings.extend(rules::check_project(workspace, &relatives, &bodies));

    // ── 3 · Compile ─────────────────────────────────────────────────────────
    let tools = toolchain::run_all(workspace, &walked.files).await;
    findings.extend(tools.findings);

    // ── Roll-up ─────────────────────────────────────────────────────────────
    // Worst first, then by file, so the report opens on what matters and a file's own
    // findings stay together rather than scattering through the list.
    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });

    let errors_count = findings
        .iter()
        .filter(|found| found.severity == Severity::Error)
        .count();
    let warnings_count = findings
        .iter()
        .filter(|found| found.severity == Severity::Warning)
        .count();
    let info_count = findings
        .iter()
        .filter(|found| found.severity == Severity::Info)
        .count();
    let total_issues = findings.len();

    let by_category = [
        Category::Correctness,
        Category::Security,
        Category::Reliability,
        Category::Hygiene,
    ]
    .into_iter()
    .map(|category| CategoryCount {
        category: category.id().to_owned(),
        count: findings
            .iter()
            .filter(|found| found.category == category)
            .count(),
    })
    .filter(|count| count.count > 0)
    .collect();

    let truncated_findings = total_issues > MAX_FINDINGS;
    findings.truncate(MAX_FINDINGS);

    let partial = walked.truncated || truncated_findings;
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let project_type = tools.stacks.iter().cloned().collect::<Vec<_>>().join(" · ");

    let summary = summarise(
        &project_type,
        walked.files.len(),
        errors_count,
        warnings_count,
        info_count,
        duration_ms,
        partial,
    );

    Ok(DiagnosticReport {
        project_name,
        project_type,
        total_issues,
        errors_count,
        warnings_count,
        info_count,
        duration_ms,
        files_scanned: walked.files.len(),
        bytes_scanned: walked.files.iter().map(|file| file.bytes).sum(),
        partial,
        summary,
        items: findings.into_iter().map(into_item).collect(),
        tools: tools
            .runs
            .into_iter()
            .map(|run| ToolStatus {
                tool: run.tool,
                at: run.at,
                ok: run.ok,
                note: run.note,
            })
            .collect(),
        by_category,
        // Errors block. Warnings and info do not — a gate that fires on a TODO is a gate
        // people learn to ignore, and then it protects nothing.
        success: errors_count == 0,
    })
}

fn into_item(found: Finding) -> DiagnosticItem {
    DiagnosticItem {
        file: found.file,
        line: Some(found.line),
        column: None,
        severity: found.severity.id().to_owned(),
        category: found.category.id().to_owned(),
        code: Some(found.rule.to_owned()),
        message: found.message,
        why: found.why.to_owned(),
        suggestion: Some(found.fix.to_owned()),
        evidence: found.evidence,
    }
}

/// The one line at the top of the report.
fn summarise(
    project_type: &str,
    files: usize,
    errors: usize,
    warnings: usize,
    info: usize,
    duration_ms: u64,
    partial: bool,
) -> String {
    let plural =
        |count: usize, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
    let scope = format!("{files} files · {project_type} · {duration_ms}ms");

    if errors == 0 && warnings == 0 && info == 0 {
        return format!("Clean. Nothing found across {scope}.");
    }
    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(plural(errors, "error"));
    }
    if warnings > 0 {
        parts.push(plural(warnings, "warning"));
    }
    if info > 0 {
        parts.push(plural(info, "note"));
    }
    let caveat = if partial {
        " Scan was truncated by a budget, so this is not the whole project."
    } else {
        ""
    };
    format!("{} across {scope}.{caveat}", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::{run_diagnostics, summarise};
    use std::path::{Path, PathBuf};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bhippi-debug-{name}-{}-{}",
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

    /// The end-to-end contract: a project seeded with real defects, in nested directories,
    /// must have every one of them reported. This is the scenario the old scanner returned
    /// "Build is clean!" for, because it never left the root directory.
    #[tokio::test]
    async fn a_seeded_project_has_every_planted_defect_found() {
        let root = scratch("seeded");
        write(&root, "package.json", "{\"name\":\"seeded\"}");
        write(
            &root,
            "src/deep/nested/app.ts",
            concat!(
                "import { helper } from './does-not-exist';\n",
                "const key = \"ghp_aB3xY9zQ1mN7pR4tW6vK8jH2sD5fG0cL\";\n",
                "try { risky(); } catch {}\n",
                "console.log(key);\n",
            ),
        );
        write(
            &root,
            "src/deep/conflicted.ts",
            "<<<<<<< HEAD\nconst a = 1;\n>>>>>>> feature\n",
        );

        let Ok(report) = run_diagnostics(&root).await else {
            panic!("the scan must complete");
        };
        let codes: Vec<&str> = report
            .items
            .iter()
            .filter_map(|item| item.code.as_deref())
            .collect();

        // Every one of these lives three directories deep, which the old walker never
        // reached. Each is also a defect no compiler reports.
        assert!(codes.contains(&"BHP-D007"), "broken import: {codes:?}");
        assert!(codes.contains(&"BHP-D023"), "committed token: {codes:?}");
        assert!(codes.contains(&"BHP-D003"), "swallowed error: {codes:?}");
        assert!(codes.contains(&"BHP-D001"), "conflict marker: {codes:?}");
        assert!(codes.contains(&"BHP-D030"), "console.log: {codes:?}");

        assert!(!report.success, "planted errors must fail the report");
        assert!(report.errors_count > 0);
        assert!(report.files_scanned >= 3);
        assert!(!report.by_category.is_empty());
        // Every finding must explain itself; a report that only names problems is a list.
        for item in &report.items {
            assert!(!item.why.is_empty(), "{:?} has no rationale", item.code);
            assert!(item.suggestion.is_some(), "{:?} has no fix", item.code);
        }

        let _ignored = std::fs::remove_dir_all(root);
    }

    /// A genuinely clean project must report clean — a debugger that always finds
    /// something is one nobody trusts.
    #[tokio::test]
    async fn a_clean_project_reports_clean() {
        let root = scratch("clean");
        write(
            &root,
            "src/app.ts",
            "export const add = (a: number, b: number) => a + b;\n",
        );

        let Ok(report) = run_diagnostics(&root).await else {
            panic!("the scan must complete");
        };
        assert_eq!(report.errors_count, 0, "{:?}", report.items);
        assert!(report.success);

        let _ignored = std::fs::remove_dir_all(root);
    }

    /// Findings arrive worst-first, so the report opens on what matters.
    #[tokio::test]
    async fn findings_are_ordered_worst_first() {
        let root = scratch("ordered");
        write(
            &root,
            "src/mixed.ts",
            concat!(
                "// TODO: tidy this up\n",
                "const r = eval(input);\n",
                "if (a == null) return;\n",
            ),
        );

        let Ok(report) = run_diagnostics(&root).await else {
            panic!("the scan must complete");
        };
        let severities: Vec<&str> = report
            .items
            .iter()
            .map(|item| item.severity.as_str())
            .collect();
        let first_info = severities.iter().position(|s| *s == "info");
        let last_error = severities.iter().rposition(|s| *s == "error");
        if let (Some(info), Some(error)) = (first_info, last_error) {
            assert!(error < info, "errors must precede notes: {severities:?}");
        }

        let _ignored = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn a_path_that_is_not_a_directory_is_refused_clearly() {
        let missing = std::env::temp_dir().join("bhippi-nope-does-not-exist-xyz");
        let Err(reason) = run_diagnostics(&missing).await else {
            panic!("a missing directory must be refused");
        };
        assert!(reason.contains("not a directory"), "{reason}");
    }

    /// A truncated scan must never read as a pass.
    #[test]
    fn a_truncated_scan_says_so_in_its_summary() {
        let partial = summarise("Rust", 4000, 0, 2, 0, 900, true);
        assert!(partial.contains("truncated"), "{partial}");

        let whole = summarise("Rust", 12, 0, 0, 0, 90, false);
        assert!(whole.starts_with("Clean."), "{whole}");
        assert!(!whole.contains("truncated"), "{whole}");

        // Singular and plural both have to read correctly.
        assert!(summarise("Rust", 1, 1, 0, 0, 5, false).starts_with("1 error across"));
        assert!(summarise("Rust", 2, 2, 0, 0, 5, false).starts_with("2 errors across"));
        assert!(summarise("Rust", 3, 1, 1, 0, 5, false).starts_with("1 error, 1 warning"));
    }
}
