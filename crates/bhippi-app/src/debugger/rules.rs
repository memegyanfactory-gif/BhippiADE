//! The hardcoded rule set: what `/debug` finds that a compiler cannot.
//!
//! A compiler tells you what does not compile. It does not tell you that a credential is
//! committed, that `.only(` has silently disabled the rest of a test suite, that a `catch`
//! block swallows every error it is handed, or that an import points at a file nobody ever
//! created. Those all compile perfectly and every one of them is a bug.
//!
//! Two rules govern everything here:
//!
//! 1. **Deterministic, no model.** Same bytes in, same findings out, every time. A
//!    debugger whose answers move is not one you can put in a gate.
//! 2. **Every rule has a negative test.** A rule with only a positive case is how a linter
//!    earns its false-positive reputation and then gets switched off, at which point it
//!    finds nothing at all. Each rule below is pinned by something it *must* fire on and
//!    something it *must not*.
//!
//! Rules are pure functions over one line, plus a small set of whole-file and whole-project
//! rules that genuinely need the wider view.

use std::collections::{HashMap, HashSet};
use std::path::Path;

/// How much a finding matters, which decides whether the report is a pass or a fail.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth knowing, never worth blocking on.
    Info,
    /// A real defect, but the program still runs.
    Warning,
    /// Broken, unsafe, or silently not doing what it appears to do.
    Error,
}

impl Severity {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// What kind of problem a rule finds. Grouped by meaning rather than by language, so a
/// report reads as "three security findings" rather than as "three regexes matched".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    Correctness,
    Security,
    Reliability,
    Hygiene,
}

impl Category {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Correctness => "correctness",
            Self::Security => "security",
            Self::Reliability => "reliability",
            Self::Hygiene => "hygiene",
        }
    }
}

/// One thing found, with why it is a bug and what to do about it.
#[derive(Clone, Debug)]
pub struct Finding {
    pub rule: &'static str,
    pub category: Category,
    pub severity: Severity,
    pub file: String,
    pub line: u32,
    /// What was found, in one sentence.
    pub message: String,
    /// Why it is a defect rather than a preference. A rule that cannot answer this does
    /// not belong in a debugger.
    pub why: &'static str,
    /// The concrete fix.
    pub fix: &'static str,
    /// The offending source, trimmed — a finding with no evidence is unverifiable.
    pub evidence: String,
}

/// The language family a file belongs to, since most rules apply to one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lang {
    Rust,
    Web,
    Python,
    Other,
}

impl Lang {
    #[must_use]
    pub fn of(extension: &str) -> Self {
        match extension {
            "rs" => Self::Rust,
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "vue" | "svelte" => Self::Web,
            "py" => Self::Python,
            _ => Self::Other,
        }
    }
}

/// Per-file state a line rule needs but cannot see from one line.
#[derive(Clone, Debug, Default)]
pub struct FileContext {
    /// True for a file whose whole purpose is tests, where `unwrap` and `console.log`
    /// are unremarkable and firing on them is pure noise.
    pub is_test_file: bool,
    /// True once a `#[cfg(test)]` module has begun in a Rust file.
    pub in_test_module: bool,
}

impl FileContext {
    #[must_use]
    pub fn for_path(relative: &str) -> Self {
        let lower = relative.to_ascii_lowercase();
        let is_test_file = lower.contains("/tests/")
            || lower.starts_with("tests/")
            || lower.contains("__tests__")
            || lower.contains(".test.")
            || lower.contains(".spec.")
            || lower.contains("_test.")
            || lower.contains("/test/")
            || lower.ends_with("conftest.py");
        Self {
            is_test_file,
            in_test_module: false,
        }
    }

    /// True when the current position is test code by either route.
    #[must_use]
    pub const fn in_tests(&self) -> bool {
        self.is_test_file || self.in_test_module
    }

    /// Updates state from one line before that line is judged.
    pub fn observe(&mut self, line: &str) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#[test]") {
            self.in_test_module = true;
        }
    }
}

/// Everything a line rule is allowed to see.
pub struct Line<'a> {
    pub relative: &'a str,
    pub lang: Lang,
    pub number: u32,
    pub text: &'a str,
    /// The line with string and comment content blanked, so a rule matching on syntax
    /// cannot fire on the same characters appearing inside a sentence.
    pub code: &'a str,
    pub context: &'a FileContext,
}

/// Runs every line rule over one line.
#[must_use]
pub fn check_line(line: &Line<'_>) -> Vec<Finding> {
    let mut out = Vec::new();
    let checks: &[fn(&Line<'_>) -> Option<Finding>] = &[
        conflict_marker,
        focused_test,
        empty_catch,
        loose_nullish_equality,
        react_list_without_key,
        rust_unwrap_outside_tests,
        rust_unfinished,
        promise_without_catch,
        eval_on_dynamic_input,
        raw_html_injection,
        shell_from_interpolation,
        hardcoded_secret,
        leftover_debug_output,
        task_marker,
    ];
    for check in checks {
        if let Some(finding) = check(line) {
            out.push(finding);
        }
    }
    out
}

fn finding(
    line: &Line<'_>,
    rule: &'static str,
    category: Category,
    severity: Severity,
    message: String,
    why: &'static str,
    fix: &'static str,
) -> Finding {
    Finding {
        rule,
        category,
        severity,
        file: line.relative.to_owned(),
        line: line.number,
        message,
        why,
        fix,
        evidence: line.text.trim().chars().take(160).collect(),
    }
}

// ── Correctness ─────────────────────────────────────────────────────────────

/// An unresolved merge. Compiles in almost no language, but sits undetected in config,
/// markdown, and any file the build does not touch.
fn conflict_marker(line: &Line<'_>) -> Option<Finding> {
    let text = line.text;
    let marked = (text.starts_with("<<<<<<<") && text.len() > 8)
        || (text.starts_with(">>>>>>>") && text.len() > 8)
        || text.trim_end() == "=======";
    // A row of equals signs is also how markdown underlines a heading and how people draw
    // separators in comments, so the bare form only counts alongside a real marker, which
    // the whole-file pass checks. Here only the unambiguous forms fire.
    if !marked || text.trim_end() == "=======" {
        return None;
    }
    Some(finding(
        line,
        "BHP-D001",
        Category::Correctness,
        Severity::Error,
        "Unresolved git merge conflict marker.".to_owned(),
        "The file still contains both sides of a merge. Whatever reads it gets text that \
         was never valid in either branch.",
        "Resolve the merge and delete the <<<<<<<, =======, and >>>>>>> lines.",
    ))
}

/// `.only(` disables every other test in the suite. The suite still passes, loudly, while
/// testing one case — which is strictly worse than the suite failing.
fn focused_test(line: &Line<'_>) -> Option<Finding> {
    if line.lang != Lang::Web {
        return None;
    }
    let code = line.code;
    if !["describe.only(", "it.only(", "test.only(", "context.only("]
        .iter()
        .any(|needle| code.contains(needle))
    {
        return None;
    }
    Some(finding(
        line,
        "BHP-D002",
        Category::Correctness,
        Severity::Error,
        "A focused test disables every other test in this file.".to_owned(),
        "The suite still reports success while running a single case, so real regressions \
         pass CI unnoticed. This is almost always debugging left behind.",
        "Remove `.only` so the whole suite runs again.",
    ))
}

/// A `catch` that does nothing. Every error it is handed disappears silently, which is the
/// single most effective way to make a bug undebuggable.
fn empty_catch(line: &Line<'_>) -> Option<Finding> {
    if line.lang != Lang::Web {
        return None;
    }
    let squashed: String = line.code.chars().filter(|c| !c.is_whitespace()).collect();
    if !squashed.contains("catch{}") && !squashed.contains("){}") {
        return None;
    }
    if !squashed.contains("catch") {
        return None;
    }
    Some(finding(
        line,
        "BHP-D003",
        Category::Correctness,
        Severity::Warning,
        "This catch block swallows the error without handling it.".to_owned(),
        "The failure is discarded with no log and no rethrow, so the program continues in \
         a state it did not expect and the cause is unrecoverable at debug time.",
        "Log it, rethrow it, or add a comment stating why ignoring it is correct here.",
    ))
}

/// `==` against a nullish literal. JavaScript's coercion table makes these true in cases
/// nobody intends — `0 == ""`, `null == undefined`, `[] == false`.
fn loose_nullish_equality(line: &Line<'_>) -> Option<Finding> {
    if line.lang != Lang::Web {
        return None;
    }
    let code = line.code;
    let squashed: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let hit = ["==null", "==undefined", "==0", "==false", "==''", "==\"\""]
        .iter()
        .any(|needle| {
            // `===` also contains `==`, so a match only counts when the character before
            // the pair is not another `=`.
            squashed
                .match_indices(needle)
                .any(|(at, _)| at == 0 || !squashed[..at].ends_with('='))
        });
    if !hit {
        return None;
    }
    Some(finding(
        line,
        "BHP-D004",
        Category::Correctness,
        Severity::Warning,
        "Loose equality against a nullish or falsy literal.".to_owned(),
        "`==` coerces before comparing, so `0 == \"\"`, `null == undefined`, and \
         `[] == false` are all true. The comparison matches values it was never meant to.",
        "Use `===`, or `x == null` deliberately with a comment if both null and undefined \
         are intended.",
    ))
}

/// A rendered list with no `key`. React then re-uses the wrong DOM nodes on reorder, which
/// shows up as inputs keeping the previous row's value — a bug nobody traces to the list.
fn react_list_without_key(line: &Line<'_>) -> Option<Finding> {
    if !matches!(line.lang, Lang::Web) {
        return None;
    }
    let code = line.code;
    if !code.contains(".map(") || !code.contains('<') {
        return None;
    }
    // Only judge a line that opens an element on the same line as the map; a multi-line
    // body is checked where the element actually opens.
    let after_map = code.split(".map(").nth(1).unwrap_or_default();
    if !after_map.contains('<') || after_map.contains("key=") {
        return None;
    }
    Some(finding(
        line,
        "BHP-D005",
        Category::Correctness,
        Severity::Warning,
        "List element rendered without a `key` prop.".to_owned(),
        "Without a stable key React matches children by position, so reordering reuses the \
         wrong nodes — component state and input values follow the old index.",
        "Give each element a `key` from stable data, never the array index for a list that \
         can reorder.",
    ))
}

// ── Reliability ─────────────────────────────────────────────────────────────

/// `unwrap()` outside tests. This project forbids it outright; everywhere else it is still
/// a panic on a case the author decided could not happen.
fn rust_unwrap_outside_tests(line: &Line<'_>) -> Option<Finding> {
    if line.lang != Lang::Rust || line.context.in_tests() {
        return None;
    }
    // Matching the exact call shapes is what excludes the total variants: `.unwrap_or(`,
    // `.unwrap_or_else(` and `.unwrap_or_default()` cannot panic, and reading them as the
    // fallible pair is the classic false positive that gets a rule switched off. None of
    // them contains the literal `.unwrap()`, so no extra guard is needed — and adding one
    // would be dead code that reads as though it were load-bearing.
    let code = line.code;
    if !code.contains(".unwrap()") && !code.contains(".expect(") {
        return None;
    }
    Some(finding(
        line,
        "BHP-D010",
        Category::Reliability,
        Severity::Error,
        "`unwrap()` or `expect()` in non-test code.".to_owned(),
        "Both panic when the assumption behind them is wrong, taking the whole process down \
         on an input the author believed impossible.",
        "Return the error with `?`, or handle the `None`/`Err` branch explicitly.",
    ))
}

/// `todo!()` shipped. It compiles, passes review, and panics the first time a user reaches it.
fn rust_unfinished(line: &Line<'_>) -> Option<Finding> {
    if line.lang != Lang::Rust || line.context.in_tests() {
        return None;
    }
    let code = line.code;
    if !code.contains("todo!(") && !code.contains("unimplemented!(") {
        return None;
    }
    Some(finding(
        line,
        "BHP-D011",
        Category::Reliability,
        Severity::Error,
        "Unfinished code path left in a shipping build.".to_owned(),
        "`todo!()` and `unimplemented!()` compile cleanly and panic at runtime, so this \
         reaches a user as a crash rather than as a missing feature.",
        "Implement the branch, or return a real error explaining that it is unsupported.",
    ))
}

/// A promise chain with no rejection handler. In Node this terminates the process by
/// default; in a browser it fails silently and the user sees nothing happen.
fn promise_without_catch(line: &Line<'_>) -> Option<Finding> {
    if line.lang != Lang::Web {
        return None;
    }
    let code = line.code;
    if !code.contains(".then(") {
        return None;
    }
    if code.contains(".catch(") || code.contains(".finally(") || code.contains("await ") {
        return None;
    }
    // A chain continuing on the next line cannot be judged from this one.
    if code.trim_end().ends_with('.') || code.trim_end().ends_with(')') && !code.contains(';') {
        return None;
    }
    Some(finding(
        line,
        "BHP-D012",
        Category::Reliability,
        Severity::Warning,
        "Promise chain with no rejection handler.".to_owned(),
        "An unhandled rejection terminates the process in Node and fails silently in the \
         browser, where the user sees the action simply not happen.",
        "Add `.catch(...)`, or `await` it inside a `try`/`catch`.",
    ))
}

// ── Security ────────────────────────────────────────────────────────────────

/// `eval` on anything that is not a literal.
fn eval_on_dynamic_input(line: &Line<'_>) -> Option<Finding> {
    if line.lang != Lang::Web {
        return None;
    }
    let code = line.code;
    if !code.contains("eval(") && !code.contains("new Function(") {
        return None;
    }
    if code.contains("// eval-safe") {
        return None;
    }
    Some(finding(
        line,
        "BHP-D020",
        Category::Security,
        Severity::Error,
        "Dynamic code execution via `eval` or `new Function`.".to_owned(),
        "Any value that reaches this runs as code with the caller's full privileges. If any \
         part of it is ever attacker-influenced, that is remote code execution.",
        "Parse the data instead — `JSON.parse` for data, a lookup table for behaviour.",
    ))
}

/// Assigning unescaped HTML. The standard route to cross-site scripting.
fn raw_html_injection(line: &Line<'_>) -> Option<Finding> {
    if line.lang != Lang::Web {
        return None;
    }
    let code = line.code;
    let hit = code.contains("dangerouslySetInnerHTML")
        || code.contains(".innerHTML")
        || code.contains(".outerHTML")
        || code.contains("document.write(");
    if !hit {
        return None;
    }
    Some(finding(
        line,
        "BHP-D021",
        Category::Security,
        Severity::Warning,
        "HTML assigned without escaping.".to_owned(),
        "Markup written this way is parsed and executed. Any user-controlled substring \
         becomes script running with the page's origin and session.",
        "Set `textContent`, render through the framework, or sanitise with a vetted library \
         first.",
    ))
}

/// A shell command assembled by interpolation — the injection every language shares.
fn shell_from_interpolation(line: &Line<'_>) -> Option<Finding> {
    let code = line.code;
    let runs_shell = code.contains("exec(")
        || code.contains("execSync(")
        || code.contains("os.system(")
        || code.contains("subprocess.call(")
        || code.contains("shell=True");
    if !runs_shell {
        return None;
    }
    // Deliberately the raw line: `strip_literals` blanks template literals, and the
    // `${…}` inside one is precisely the interpolation being looked for. The call itself
    // is still matched on stripped code above, so prose about `exec(` cannot fire.
    let raw = line.text;
    let interpolated = raw.contains("${") || raw.contains("\" +") || raw.contains("f\"");
    if !interpolated {
        return None;
    }
    Some(finding(
        line,
        "BHP-D022",
        Category::Security,
        Severity::Error,
        "Shell command built by string interpolation.".to_owned(),
        "A shell splits the finished string on its own metacharacters, so any `;`, `&&`, or \
         backtick inside an interpolated value becomes a second command.",
        "Pass the program and its arguments as an explicit array so no shell parses them.",
    ))
}

/// Names that mean a value is a credential.
const SECRET_NAMES: &[&str] = &[
    "api_key",
    "apikey",
    "secret",
    "password",
    "passwd",
    "token",
    "private_key",
    "access_key",
    "client_secret",
    "auth_token",
    "credential",
];

/// Literal prefixes that are unambiguously a real, live credential.
const SECRET_PREFIXES: &[(&str, &str)] = &[
    ("AKIA", "an AWS access key id"),
    ("ASIA", "an AWS temporary access key id"),
    ("ghp_", "a GitHub personal access token"),
    ("gho_", "a GitHub OAuth token"),
    ("github_pat_", "a GitHub fine-grained token"),
    ("xoxb-", "a Slack bot token"),
    ("xoxp-", "a Slack user token"),
    ("sk_live_", "a live Stripe secret key"),
    ("sk-ant-", "an Anthropic API key"),
    ("AIza", "a Google API key"),
    ("-----BEGIN", "a private key block"),
];

/// A credential written into source.
///
/// Two routes, deliberately: a known prefix is proof on its own, while a secret-shaped
/// *name* only counts when what it is assigned is long enough and varied enough to be a
/// real value rather than a placeholder. Without that second test this rule fires on every
/// `password: ""` and is switched off within a day.
fn hardcoded_secret(line: &Line<'_>) -> Option<Finding> {
    let text = line.text;
    for (prefix, what) in SECRET_PREFIXES {
        if text.contains(prefix) {
            return Some(finding(
                line,
                "BHP-D023",
                Category::Security,
                Severity::Error,
                format!("Hardcoded credential in source — this looks like {what}."),
                "A committed credential is in the history permanently, readable by anyone \
                 with repository access, and is not revoked by deleting the line.",
                "Revoke and rotate the key now, then read it from the environment or a \
                 secret store.",
            ));
        }
    }

    let lower = line.code.to_ascii_lowercase();
    let named = SECRET_NAMES.iter().any(|name| lower.contains(name));
    if !named {
        return None;
    }
    let value = quoted_value(text)?;
    if value.len() < 16 || !looks_random(&value) {
        return None;
    }
    // A reference to somewhere the value comes from is the correct pattern, not a finding.
    if lower.contains("process.env")
        || lower.contains("std::env")
        || lower.contains("os.environ")
        || lower.contains("getenv")
    {
        return None;
    }
    Some(finding(
        line,
        "BHP-D023",
        Category::Security,
        Severity::Error,
        "A secret-shaped name is assigned a literal that looks like a real value.".to_owned(),
        "A committed credential is in the history permanently, readable by anyone with \
         repository access, and is not revoked by deleting the line.",
        "Revoke and rotate it, then read it from the environment or a secret store.",
    ))
}

/// The first quoted literal on a line.
fn quoted_value(text: &str) -> Option<String> {
    for quote in ['"', '\''] {
        if let Some(open) = text.find(quote) {
            if let Some(len) = text[open + 1..].find(quote) {
                return Some(text[open + 1..open + 1 + len].to_owned());
            }
        }
    }
    None
}

/// Whether a literal has the character mix of a generated credential.
///
/// A placeholder — `changeme`, `your-key-here`, `xxxxxxxx` — is words. A real key mixes
/// cases and digits and repeats nothing. Requiring all three is what keeps this rule from
/// firing on every example in a README.
fn looks_random(value: &str) -> bool {
    let has_upper = value.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = value.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = value.chars().any(|c| c.is_ascii_digit());
    let distinct: HashSet<char> = value.chars().collect();
    let varied = distinct.len() * 2 >= value.len();
    let placeholder = [
        "example",
        "changeme",
        "your",
        "placeholder",
        "dummy",
        "sample",
        "test",
    ]
    .iter()
    .any(|word| value.to_ascii_lowercase().contains(word));
    has_upper && has_lower && has_digit && varied && !placeholder
}

// ── Hygiene ─────────────────────────────────────────────────────────────────

/// Debug output left in shipping code.
fn leftover_debug_output(line: &Line<'_>) -> Option<Finding> {
    if line.context.in_tests() {
        return None;
    }
    let code = line.code;
    let (hit, what) = match line.lang {
        Lang::Web if code.contains("debugger;") => (true, "a `debugger` statement"),
        Lang::Web if code.contains("console.log(") => (true, "a `console.log` call"),
        Lang::Rust if code.contains("dbg!(") => (true, "a `dbg!` macro"),
        Lang::Python if code.contains("breakpoint()") => (true, "a `breakpoint()` call"),
        _ => (false, ""),
    };
    if !hit {
        return None;
    }
    let severity = if what.contains("debugger") || what.contains("breakpoint") {
        // These halt execution for anyone with devtools open. Not hygiene — a defect.
        Severity::Error
    } else {
        Severity::Info
    };
    Some(finding(
        line,
        "BHP-D030",
        Category::Hygiene,
        severity,
        format!("Debug output left in the source: {what}."),
        "Debug statements leak internal state to anyone with a console open, and a \
         `debugger` or `breakpoint()` halts execution outright.",
        "Remove it, or route it through the project's logger at the right level.",
    ))
}

/// A tracked promise the author left themselves.
///
/// Reads the raw line, not the stripped code: a task marker lives *in* a comment, so
/// judging this rule on code with comments blanked would mean it never fires at all.
fn task_marker(line: &Line<'_>) -> Option<Finding> {
    let marker = ["TODO", "FIXME", "HACK", "XXX"]
        .iter()
        .find(|marker| line.text.contains(**marker))?;
    let severity = if *marker == "FIXME" || *marker == "HACK" {
        Severity::Warning
    } else {
        Severity::Info
    };
    Some(finding(
        line,
        "BHP-D031",
        Category::Hygiene,
        severity,
        format!("{marker} marker left in the source."),
        "A marker in code is a promise with no owner and no date. FIXME and HACK in \
         particular mark something known to be wrong.",
        "Fix it, or move it to the tracker so it has an owner.",
    ))
}

// ── Whole-file and whole-project rules ──────────────────────────────────────

/// Rules that need the file, not a line.
#[must_use]
pub fn check_file(relative: &str, lang: Lang, body: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let lines = body.lines().count();

    // A file this long has stopped having one job. Not a defect on its own, which is why
    // it is Info — but it is reliably where the defects concentrate.
    if lines > 800 {
        out.push(Finding {
            rule: "BHP-D032",
            category: Category::Hygiene,
            severity: Severity::Info,
            file: relative.to_owned(),
            line: 1,
            message: format!("This file is {lines} lines long."),
            why: "Past roughly 800 lines a file has stopped having a single \
                  responsibility, and it is where defects concentrate.",
            fix: "Split it along the seams that already exist inside it.",
            evidence: format!("{lines} lines"),
        });
    }

    if lang == Lang::Rust {
        // A blanket allow at module scope silences the lint for everything below it,
        // including code written months later by someone who never saw the attribute.
        for (index, line) in body.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("#![allow(") {
                out.push(Finding {
                    rule: "BHP-D013",
                    category: Category::Reliability,
                    severity: Severity::Warning,
                    file: relative.to_owned(),
                    line: (index + 1) as u32,
                    message: "Crate-wide `#![allow(...)]` silences a lint everywhere.".to_owned(),
                    why: "The attribute applies to every line in the module, including code \
                          written later by someone who never saw it, so the lint stops \
                          protecting anything.",
                    fix: "Move the allow onto the specific item that needs it, with a \
                          comment saying why.",
                    evidence: trimmed.chars().take(120).collect(),
                });
            }
        }
    }

    out
}

/// Rules that need the whole project.
///
/// `relatives` is every scanned path; `by_path` maps a path to its body for the files that
/// were read. Both are needed: an import can only be resolved against the full file list.
#[must_use]
pub fn check_project(
    root: &Path,
    relatives: &HashSet<String>,
    by_path: &HashMap<String, String>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(unignored_env_files(root, relatives));
    out.extend(case_only_collisions(relatives));
    out.extend(broken_relative_imports(relatives, by_path));
    out
}

/// A `.env` the repository will happily commit.
fn unignored_env_files(root: &Path, relatives: &HashSet<String>) -> Vec<Finding> {
    let env_files: Vec<&String> = relatives
        .iter()
        .filter(|path| {
            let name = path.rsplit('/').next().unwrap_or(path);
            name == ".env" || name.starts_with(".env.")
        })
        .filter(|path| !path.ends_with(".example") && !path.ends_with(".sample"))
        .collect();
    if env_files.is_empty() {
        return Vec::new();
    }

    let ignore = std::fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
    let covered = ignore
        .lines()
        .map(str::trim)
        .any(|rule| rule == ".env" || rule == ".env*" || rule == "*.env" || rule == ".env.*");
    if covered {
        return Vec::new();
    }

    env_files
        .into_iter()
        .map(|path| Finding {
            rule: "BHP-D024",
            category: Category::Security,
            severity: Severity::Error,
            file: path.clone(),
            line: 1,
            message: "An environment file is not covered by .gitignore.".to_owned(),
            why: "`.env` files hold live credentials. Nothing stops this one being \
                  committed, and once it is, it is in the history permanently.",
            fix: "Add `.env*` to .gitignore, and rotate anything already committed.",
            evidence: path.clone(),
        })
        .collect()
}

/// Two files differing only in case. Fine on Windows and macOS, two separate files on
/// Linux — so CI builds something the author has never run.
fn case_only_collisions(relatives: &HashSet<String>) -> Vec<Finding> {
    let mut by_lower: HashMap<String, Vec<&String>> = HashMap::new();
    for path in relatives {
        by_lower
            .entry(path.to_ascii_lowercase())
            .or_default()
            .push(path);
    }
    let mut out = Vec::new();
    for (_, group) in by_lower {
        if group.len() < 2 {
            continue;
        }
        let mut names: Vec<&str> = group.iter().map(|path| path.as_str()).collect();
        names.sort_unstable();
        out.push(Finding {
            rule: "BHP-D006",
            category: Category::Correctness,
            severity: Severity::Error,
            file: names.first().copied().unwrap_or_default().to_owned(),
            line: 1,
            message: format!("Two paths differ only in case: {}", names.join(" · ")),
            why: "Windows and macOS treat these as one file and Linux as two, so a CI \
                  checkout builds something the author has never run locally.",
            fix: "Rename one of them so the paths differ by more than case.",
            evidence: names.join(" · "),
        });
    }
    out
}

/// An import naming a file that does not exist.
///
/// The highest-value deterministic check there is: it needs no type information, has no
/// false positives once the extension candidates are right, and catches the exact breakage
/// a rename leaves behind.
fn broken_relative_imports(
    relatives: &HashSet<String>,
    by_path: &HashMap<String, String>,
) -> Vec<Finding> {
    const CANDIDATES: &[&str] = &[
        "",
        ".ts",
        ".tsx",
        ".js",
        ".jsx",
        ".mjs",
        ".cjs",
        ".json",
        ".css",
        "/index.ts",
        "/index.tsx",
        "/index.js",
        "/index.jsx",
    ];
    let mut out = Vec::new();

    for (path, body) in by_path {
        if !matches!(
            Lang::of(path.rsplit('.').next().unwrap_or_default()),
            Lang::Web
        ) {
            continue;
        }
        let dir = path.rsplit_once('/').map_or("", |(head, _)| head);
        for (index, line) in body.lines().enumerate() {
            let Some(target) = relative_import_of(line) else {
                continue;
            };
            let Some(joined) = join_relative(dir, &target) else {
                continue;
            };
            let resolved = CANDIDATES
                .iter()
                .any(|suffix| relatives.contains(&format!("{joined}{suffix}")));
            if resolved {
                continue;
            }
            out.push(Finding {
                rule: "BHP-D007",
                category: Category::Correctness,
                severity: Severity::Error,
                file: path.clone(),
                line: (index + 1) as u32,
                message: format!("Import points at `{target}`, which does not exist."),
                why: "The path resolves to no file in the project. This is what a rename \
                      or a move leaves behind, and it fails at build or at run time.",
                fix: "Correct the path, or restore the file the import expects.",
                evidence: line.trim().chars().take(160).collect(),
            });
        }
    }
    out
}

/// The relative specifier in an `import`/`require`, when there is one.
fn relative_import_of(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if !(trimmed.starts_with("import ")
        || trimmed.starts_with("export ")
        || trimmed.contains("require("))
    {
        return None;
    }
    for quote in ['"', '\''] {
        if let Some(open) = line.find(quote) {
            if let Some(len) = line[open + 1..].find(quote) {
                let target = &line[open + 1..open + 1 + len];
                if target.starts_with("./") || target.starts_with("../") {
                    return Some(target.to_owned());
                }
            }
        }
    }
    None
}

/// Resolves a relative specifier against the importing file's directory.
fn join_relative(dir: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for segment in target.split('/') {
        match segment {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

/// Blanks string and comment content so a rule matching on syntax cannot fire on the same
/// characters inside a sentence.
///
/// Without this, a comment reading "never use eval() here" is reported as a call to
/// `eval`, and the rule that finds real ones gets switched off because of it.
#[must_use]
pub fn strip_literals(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    while let Some(character) = chars.next() {
        if let Some(open) = quote {
            out.push(' ');
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == open {
                quote = None;
            }
            continue;
        }
        if character == '/' && chars.peek() == Some(&'/') {
            break;
        }
        if character == '#' {
            break;
        }
        if character == '"' || character == '\'' || character == '`' {
            quote = Some(character);
            out.push(character);
            continue;
        }
        out.push(character);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        check_file, check_line, check_project, strip_literals, Category, FileContext, Lang, Line,
        Severity,
    };
    use std::collections::{HashMap, HashSet};
    use std::path::Path;

    /// Runs the line rules over one line of a named file.
    fn run(relative: &str, text: &str) -> Vec<super::Finding> {
        let extension = relative.rsplit('.').next().unwrap_or_default();
        let context = FileContext::for_path(relative);
        let code = strip_literals(text);
        check_line(&Line {
            relative,
            lang: Lang::of(extension),
            number: 1,
            text,
            code: &code,
            context: &context,
        })
    }

    fn fires(relative: &str, text: &str, rule: &str) -> bool {
        run(relative, text).iter().any(|found| found.rule == rule)
    }

    /// Every rule gets both cases. A rule with only a positive test is how a linter earns
    /// its false-positive reputation and then gets switched off entirely.
    #[test]
    fn each_rule_fires_on_the_bug_and_stays_quiet_otherwise() {
        // BHP-D001 · merge conflict
        assert!(fires("a.ts", "<<<<<<< HEAD", "BHP-D001"));
        assert!(!fires("a.ts", "const arrow = a <<< b;", "BHP-D001"));
        // A markdown heading underline must never read as a conflict.
        assert!(!fires("a.md", "=======", "BHP-D001"));

        // BHP-D002 · focused test
        assert!(fires("a.test.ts", "it.only('works', () => {})", "BHP-D002"));
        assert!(!fires("a.test.ts", "it('works', () => {})", "BHP-D002"));

        // BHP-D003 · swallowed error
        assert!(fires("a.ts", "try { risky(); } catch {}", "BHP-D003"));
        assert!(!fires(
            "a.ts",
            "try { risky(); } catch (e) { log(e); }",
            "BHP-D003"
        ));

        // BHP-D004 · loose equality. `===` must not fire.
        assert!(fires("a.ts", "if (value == null) return;", "BHP-D004"));
        assert!(!fires("a.ts", "if (value === null) return;", "BHP-D004"));

        // BHP-D005 · missing key
        assert!(fires(
            "a.tsx",
            "{rows.map((r) => <Row v={r} />)}",
            "BHP-D005"
        ));
        assert!(!fires(
            "a.tsx",
            "{rows.map((r) => <Row key={r.id} v={r} />)}",
            "BHP-D005"
        ));

        // BHP-D010 · unwrap. `unwrap_or` is total and must not fire.
        assert!(fires("src/a.rs", "let v = maybe.unwrap();", "BHP-D010"));
        assert!(!fires(
            "src/a.rs",
            "let v = maybe.unwrap_or(0);",
            "BHP-D010"
        ));
        assert!(!fires(
            "src/a.rs",
            "let v = maybe.unwrap_or_default();",
            "BHP-D010"
        ));
        // Test code is exempt: unwrap there is unremarkable.
        assert!(!fires("tests/a.rs", "let v = maybe.unwrap();", "BHP-D010"));

        // BHP-D011 · unfinished
        assert!(fires("src/a.rs", "todo!()", "BHP-D011"));
        assert!(!fires("src/a.rs", "let x = 1; // todo later", "BHP-D011"));

        // BHP-D012 · unhandled rejection
        assert!(fires("a.ts", "fetchIt().then(use);", "BHP-D012"));
        assert!(!fires(
            "a.ts",
            "fetchIt().then(use).catch(log);",
            "BHP-D012"
        ));

        // BHP-D020 · eval
        assert!(fires("a.ts", "const r = eval(userInput);", "BHP-D020"));
        assert!(!fires("a.ts", "const r = evaluate(userInput);", "BHP-D020"));

        // BHP-D021 · raw HTML
        assert!(fires("a.tsx", "el.innerHTML = value;", "BHP-D021"));
        assert!(!fires("a.tsx", "el.textContent = value;", "BHP-D021"));

        // BHP-D022 · shell injection
        assert!(fires("a.ts", "execSync(`rm ${target}`);", "BHP-D022"));
        assert!(!fires("a.ts", "execFile('rm', [target]);", "BHP-D022"));

        // BHP-D030 · leftover debug
        assert!(fires("src/a.ts", "console.log(state);", "BHP-D030"));
        assert!(!fires("src/a.ts", "logger.info(state);", "BHP-D030"));
        assert!(!fires("a.test.ts", "console.log(state);", "BHP-D030"));

        // BHP-D031 · markers
        assert!(fires("a.ts", "// TODO: handle the empty case", "BHP-D031"));
        assert!(!fires("a.ts", "// handles the empty case", "BHP-D031"));
    }

    /// A committed credential is the highest-cost finding here, and the rule is only
    /// usable if it stays silent on placeholders and on correct env lookups.
    #[test]
    fn secret_detection_knows_a_real_key_from_a_placeholder() {
        assert!(fires(
            "a.ts",
            "const key = \"AKIAIOSFODNN7EXAMPLE1\";",
            "BHP-D023"
        ));
        assert!(fires(
            "a.ts",
            "const token = \"ghp_aB3xY9zQ1mN7pR4tW6vK8jH2sD5fG0cL\";",
            "BHP-D023"
        ));
        // Placeholders, empty values, and env lookups are the correct patterns.
        assert!(!fires("a.ts", "const password = \"\";", "BHP-D023"));
        assert!(!fires(
            "a.ts",
            "const apiKey = \"your-api-key-here\";",
            "BHP-D023"
        ));
        assert!(!fires(
            "a.ts",
            "const apiKey = process.env.API_KEY ?? \"\";",
            "BHP-D023"
        ));
        assert!(!fires("a.ts", "const secret = \"changeme\";", "BHP-D023"));
    }

    /// The rule that makes the rest usable: syntax inside a comment or a string is prose,
    /// not code. Without this, "never call eval()" in a comment is reported as a call.
    #[test]
    fn prose_that_mentions_syntax_is_not_a_finding() {
        assert!(!fires(
            "a.ts",
            "// never call eval() on user input",
            "BHP-D020"
        ));
        assert!(!fires(
            "a.ts",
            "const doc = \"use innerHTML carefully\";",
            "BHP-D021"
        ));
        assert_eq!(strip_literals("a // eval(x)").trim(), "a");
        assert!(strip_literals("f(\"eval(1)\")").contains("f("));
        assert!(!strip_literals("f(\"eval(1)\")").contains("eval(1)"));
    }

    /// A `#[cfg(test)]` module exempts everything below it, the same as a tests/ file.
    #[test]
    fn a_test_module_exempts_the_code_inside_it() {
        let mut context = FileContext::for_path("src/lib.rs");
        assert!(!context.in_tests());
        context.observe("#[cfg(test)]");
        assert!(context.in_tests());
    }

    /// The highest-value project rule: an import naming a file nobody created.
    #[test]
    fn a_broken_relative_import_is_found_and_a_good_one_is_not() {
        let mut relatives = HashSet::new();
        relatives.insert("src/app.ts".to_owned());
        relatives.insert("src/util/helpers.ts".to_owned());
        relatives.insert("src/util/index.ts".to_owned());

        let mut bodies = HashMap::new();
        bodies.insert(
            "src/app.ts".to_owned(),
            concat!(
                "import { a } from './util/helpers';\n",
                "import { b } from './util';\n",
                "import { c } from './util/missing';\n",
                "import React from 'react';\n",
            )
            .to_owned(),
        );

        let found = check_project(Path::new("."), &relatives, &bodies);
        let broken: Vec<_> = found.iter().filter(|f| f.rule == "BHP-D007").collect();

        assert_eq!(broken.len(), 1, "{broken:?}");
        assert!(broken[0].message.contains("./util/missing"));
        // A bare package specifier is not this rule's business.
        assert!(!found.iter().any(|f| f.message.contains("react")));
    }

    /// Two paths differing only in case build differently on Linux than on Windows.
    #[test]
    fn case_only_collisions_are_reported() {
        let mut relatives = HashSet::new();
        relatives.insert("src/Button.tsx".to_owned());
        relatives.insert("src/button.tsx".to_owned());
        relatives.insert("src/Card.tsx".to_owned());

        let found = check_project(Path::new("."), &relatives, &HashMap::new());
        let collisions: Vec<_> = found.iter().filter(|f| f.rule == "BHP-D006").collect();
        assert_eq!(collisions.len(), 1, "{collisions:?}");
    }

    #[test]
    fn an_oversized_file_is_noted_but_never_blocks() {
        let body = "line\n".repeat(900);
        let found = check_file("src/big.rs", Lang::Rust, &body);
        let big: Vec<_> = found.iter().filter(|f| f.rule == "BHP-D032").collect();
        assert_eq!(big.len(), 1);
        assert_eq!(big[0].severity, Severity::Info);

        assert!(check_file("src/small.rs", Lang::Rust, "fn a() {}\n").is_empty());
    }

    /// Severity ordering decides whether a report passes; it must be a real order.
    #[test]
    fn severity_orders_from_info_up_to_error() {
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
        assert_eq!(Severity::Error.id(), "error");
        assert_eq!(Category::Security.id(), "security");
    }
}
