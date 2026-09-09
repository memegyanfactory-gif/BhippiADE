//! What the agent is doing, named the way a person would say it (ADR-0049).
//!
//! A step used to reach the transcript carrying the backend's own identifier — `Bash`,
//! `MultiEdit`, `commandExecution`, `apply_patch` — and the webview guessed a verb back out
//! of that string with a regex. Two things were wrong with that: the surface showed vendor
//! names to users, and meaning was computed in a place INV-051 says computes nothing.
//!
//! This module is the single translation. It takes what the runtime actually reported — a
//! tool name and its input, or a shell line — and returns the kind of work it is plus the
//! sentence a person would read. Nothing here invents an activity: every function is a pure
//! reading of something that happened.

use serde::{Deserialize, Serialize};
use specta::Type;

/// What a step semantically *is*, independent of which backend reported it.
///
/// The vocabulary is deliberately finer than the nine `ToolAction`s it sits beside: the
/// difference between *running tests*, *building*, *checking types* and *starting a dev
/// server* is the difference between a transcript that explains itself and one that says
/// "Ran" four times.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Reasoning,
    Planning,

    SearchingCode,
    SearchingFiles,
    ListingDirectory,

    ReadingFile,
    ReadingMultipleFiles,

    SearchingWeb,
    OpeningWebpage,
    ReadingWebpage,

    ViewingImage,
    InspectingScreenshot,

    EditingFile,
    CreatingFile,
    DeletingFile,
    MovingFile,
    ApplyingPatch,

    RunningCommand,
    RunningScript,

    StartingDevServer,
    BuildingProject,
    InstallingDependencies,

    RunningTests,
    RunningSingleTest,
    Linting,
    Typechecking,

    CheckingErrors,
    Debugging,
    InvestigatingFailure,

    OpeningBrowser,
    TestingBrowser,
    ClickingUi,
    TakingScreenshot,
    InspectingUi,

    /// The catch-all, and the default: a tool ran and we will not pretend to know more.
    #[default]
    UsingTool,
    UsingPlugin,
    UsingMcp,

    StartingSubagent,
    SubagentWorking,
    WaitingForSubagent,
    SubagentCompleted,

    ReviewingChanges,
    ReviewingDiff,

    GitStatus,
    GitDiff,
    GitCommit,

    RequestingPermission,
    WaitingForUser,

    Verifying,
    Finalizing,

    Completed,
    Failed,
}

/// Where a step is in its life. Six states, because four could not tell a step that never
/// started from one the user interrupted.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus {
    /// Announced but not started — a queued command, a delegated agent not yet running.
    Queued,
    #[default]
    InProgress,
    Completed,
    Failed,
    /// Stopped by the user or by the turn ending before this step did.
    Cancelled,
    /// Suspended on a permission prompt. The row stays; it does not disappear and return.
    WaitingForUser,
}

impl ActivityStatus {
    /// True while the row should carry the live indicator.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::InProgress | Self::Queued)
    }
}

/// The typed extras a row can draw, filled only where the runtime actually reported them.
///
/// Typed rather than a free-form map because the view renders these directly: a `match_count`
/// that arrives as a string is a bug the compiler should have caught, and INV-051 means the
/// webview cannot parse one out of prose.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize, Type)]
pub struct ActivityMeta {
    /// What was searched for — the pattern, the query, the symbol.
    pub query: Option<String>,
    /// How many results the search returned, when the runtime counted them.
    pub match_count: Option<u32>,
    /// Files this step touched, display-ready and workspace-relative.
    pub paths: Vec<String>,
    /// The full URL, kept for the expanded view; the row shows `host`.
    pub url: Option<String>,
    pub host: Option<String>,
    /// Test totals, parsed from the runner's own output (never estimated).
    pub tests_passed: Option<u32>,
    pub tests_failed: Option<u32>,
    /// Which delegated agent this step belongs to. `None` is the primary agent.
    pub agent_label: Option<String>,
    /// The activity that spawned this one, for a sub-agent's own stream.
    pub parent_id: Option<String>,
}

impl ActivityMeta {
    /// True when there is nothing here worth drawing — so a row can skip its second line.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// One reading of a reported step: what it is, and how to say it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Classified {
    pub kind: ActivityKind,
    /// The sentence the row shows — "Running tests", "Reading PlayerController.ts".
    pub title: String,
    /// The quieter second line — the command, the query, the directory.
    pub description: Option<String>,
    pub meta: ActivityMeta,
}

impl Classified {
    fn new(kind: ActivityKind, title: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            description: None,
            meta: ActivityMeta::default(),
        }
    }

    fn described(mut self, description: impl Into<String>) -> Self {
        let text = description.into();
        self.description = (!text.trim().is_empty()).then_some(text);
        self
    }

    fn with_query(mut self, query: impl Into<String>) -> Self {
        let text = query.into();
        self.meta.query = (!text.trim().is_empty()).then_some(text);
        self
    }

    fn with_path(mut self, path: impl Into<String>) -> Self {
        let text = path.into();
        if !text.trim().is_empty() {
            self.meta.paths.push(text);
        }
        self
    }
}

/// The last part of a path, for a row that names a file rather than a route to it.
#[must_use]
pub fn file_name_of(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .unwrap_or(path)
        .to_owned()
}

/// The host of a URL, so a row reads "docs.godotengine.org" and not a query string.
#[must_use]
pub fn host_of(url: &str) -> Option<String> {
    let rest = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .trim_start_matches("www.");
    let host = rest.split(['/', '?', '#']).next()?.trim();
    (!host.is_empty() && host.contains('.')).then(|| host.to_owned())
}

/// The part of a shell line that says what it does.
///
/// `cd ui && npm test` is a test run, not a directory change, and `NODE_ENV=test npx vitest`
/// is a test run and not an assignment. Both read wrong if the first token wins, so leading
/// `cd`/assignment segments are dropped and the last real segment is the one classified.
fn significant_segment(command: &str) -> &str {
    let trimmed = command.trim();
    let mut best = trimmed;
    // One pass over every separator: chaining two `split` calls made the second one see the
    // whole line again and overwrite the answer the first had already found.
    for segment in trimmed
        .split("&&")
        .flat_map(|part| part.split("||"))
        .flat_map(|part| part.split(';'))
    {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let head = segment.split_whitespace().next().unwrap_or_default();
        // A directory change and a bare assignment are scaffolding around the work.
        if head == "cd" || (head.contains('=') && segment.split_whitespace().count() == 1) {
            continue;
        }
        best = segment;
    }
    best
}

/// A shell line split into words, with the original spelling kept beside the folded one.
///
/// Case cannot be thrown away: `PlayerController.ts` is what the row has to say, and a
/// lowercased path is a filename that does not exist. Quotes cannot be ignored either —
/// `rg "camera shake"` searched for one phrase, not for two words.
struct Words {
    raw: Vec<String>,
    lower: Vec<String>,
}

impl Words {
    fn get(&self, index: usize) -> &str {
        self.lower.get(index).map_or("", String::as_str)
    }

    fn has(&self, needle: &str) -> bool {
        self.lower.iter().any(|word| word == needle)
    }

    /// The first word past the command name that is not a flag, in its real spelling.
    fn first_operand(&self) -> Option<&String> {
        self.raw.iter().skip(1).find(|word| !word.starts_with('-'))
    }
}

/// Splits on whitespace but keeps quoted runs whole.
fn split_words(segment: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in segment.chars() {
        match quote {
            Some(open) if ch == open => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// The command's own words, with prefixes that name no work removed.
fn tokens(segment: &str) -> Words {
    let mut raw = split_words(segment);
    // `NODE_ENV=test cmd` is an assignment in front of the work, not the work.
    while raw
        .first()
        .is_some_and(|word| word.contains('=') && !word.starts_with('-'))
    {
        raw.remove(0);
    }
    // `npx`/`bunx`/`pnpm dlx` name the fetcher, never the work.
    while raw
        .first()
        .is_some_and(|word| matches!(word.to_ascii_lowercase().as_str(), "npx" | "bunx" | "dlx"))
    {
        raw.remove(0);
    }
    if raw
        .first()
        .is_some_and(|word| word.eq_ignore_ascii_case("pnpm"))
        && raw
            .get(1)
            .is_some_and(|word| word.eq_ignore_ascii_case("dlx"))
    {
        raw.drain(0..2);
    }
    let lower = raw.iter().map(|word| word.to_ascii_lowercase()).collect();
    Words { raw, lower }
}

/// True when a word names one of the JavaScript package runners.
fn is_package_runner(word: &str) -> bool {
    matches!(word, "npm" | "pnpm" | "yarn" | "bun" | "deno")
}

/// What a package-manager invocation is really doing, read from the script it names.
fn classify_package_script(words: &Words, command: &str) -> Classified {
    // `npm run build` names the script second; `yarn build` and `npm test` name it first.
    let script = words
        .lower
        .iter()
        .skip(1)
        .find(|word| !matches!(word.as_str(), "run" | "run-script" | "--"))
        .cloned()
        .unwrap_or_default();

    if matches!(
        script.as_str(),
        "install" | "i" | "ci" | "add" | "update" | "upgrade"
    ) {
        return Classified::new(
            ActivityKind::InstallingDependencies,
            "Installing dependencies",
        )
        .described(command);
    }
    if script.contains("test") {
        return Classified::new(ActivityKind::RunningTests, "Running tests").described(command);
    }
    if script.contains("lint") || script.contains("format") {
        return Classified::new(ActivityKind::Linting, "Linting project").described(command);
    }
    if script.contains("typecheck") || script.contains("tsc") || script.contains("types") {
        return Classified::new(ActivityKind::Typechecking, "Checking types").described(command);
    }
    if script.contains("build") || script.contains("compile") || script.contains("bundle") {
        return Classified::new(ActivityKind::BuildingProject, "Building project")
            .described(command);
    }
    if matches!(
        script.as_str(),
        "dev" | "start" | "serve" | "preview" | "watch"
    ) {
        return Classified::new(
            ActivityKind::StartingDevServer,
            "Starting development server",
        )
        .described(command);
    }
    Classified::new(ActivityKind::RunningCommand, "Running").described(command)
}

/// Read a shell line semantically (owner spec §13).
///
/// Every branch below exists because "Running command" is the least useful true thing the
/// transcript can say. An unrecognised line still says "Running" with the line beneath it —
/// vague, but never wrong.
#[must_use]
pub fn classify_command(command: &str) -> Classified {
    let command = command.trim();
    if command.is_empty() {
        return Classified::new(ActivityKind::RunningCommand, "Running");
    }
    let segment = significant_segment(command);
    let words = tokens(segment);
    let head = words.get(0);
    if head.is_empty() {
        return Classified::new(ActivityKind::RunningCommand, "Running").described(command);
    }
    let arg = |index: usize| words.get(index);
    let has = |needle: &str| words.has(needle);

    if is_package_runner(head) {
        return classify_package_script(&words, command);
    }

    match head {
        "git" => {
            return match arg(1) {
                "status" => Classified::new(ActivityKind::GitStatus, "Checking Git status"),
                "diff" => Classified::new(ActivityKind::GitDiff, "Reviewing changes"),
                "commit" => Classified::new(ActivityKind::GitCommit, "Committing changes"),
                "log" | "show" | "blame" => {
                    Classified::new(ActivityKind::ReviewingChanges, "Reading Git history")
                }
                _ => Classified::new(ActivityKind::RunningCommand, "Running Git"),
            }
            .described(command);
        }
        "cargo" => {
            return match arg(1) {
                "test" | "nextest" => Classified::new(ActivityKind::RunningTests, "Running tests"),
                "build" | "b" => Classified::new(ActivityKind::BuildingProject, "Building project"),
                "check" => Classified::new(ActivityKind::Typechecking, "Checking types"),
                "clippy" => Classified::new(ActivityKind::Linting, "Linting project"),
                "fmt" => Classified::new(ActivityKind::Linting, "Formatting code"),
                "run" => Classified::new(ActivityKind::RunningCommand, "Running the project"),
                "add" | "install" => Classified::new(
                    ActivityKind::InstallingDependencies,
                    "Installing dependencies",
                ),
                _ => Classified::new(ActivityKind::RunningCommand, "Running Cargo"),
            }
            .described(command);
        }
        "go" | "dotnet" | "mvn" | "gradle" | "./gradlew" => {
            return match arg(1) {
                "test" => Classified::new(ActivityKind::RunningTests, "Running tests"),
                "build" | "compile" => {
                    Classified::new(ActivityKind::BuildingProject, "Building project")
                }
                _ => Classified::new(ActivityKind::RunningCommand, "Running"),
            }
            .described(command);
        }
        "pytest" | "vitest" | "jest" | "phpunit" | "rspec" | "mocha" | "ava" => {
            // A runner given a single file or a `-t` filter is one test, not the suite.
            let single = has("-t")
                || has("--test-name-pattern")
                || words.lower.iter().skip(1).any(|word| {
                    word.contains("test") && (word.contains('.') || word.contains('/'))
                });
            let kind = if single {
                ActivityKind::RunningSingleTest
            } else {
                ActivityKind::RunningTests
            };
            let title = if single {
                "Running a test"
            } else {
                "Running tests"
            };
            return Classified::new(kind, title).described(command);
        }
        "tsc" => {
            let kind = if has("--noemit") || has("--noemit=true") {
                ActivityKind::Typechecking
            } else {
                ActivityKind::BuildingProject
            };
            let title = if kind == ActivityKind::Typechecking {
                "Checking types"
            } else {
                "Building project"
            };
            return Classified::new(kind, title).described(command);
        }
        "eslint" | "biome" | "ruff" | "flake8" | "pylint" | "prettier" | "rustfmt" => {
            return Classified::new(ActivityKind::Linting, "Linting project").described(command);
        }
        "mypy" | "pyright" => {
            return Classified::new(ActivityKind::Typechecking, "Checking types")
                .described(command);
        }
        "make" | "cmake" | "ninja" | "vite" | "webpack" | "rollup" | "esbuild" => {
            let kind = if arg(1) == "test" {
                ActivityKind::RunningTests
            } else if arg(1) == "dev" || arg(1) == "serve" {
                ActivityKind::StartingDevServer
            } else {
                ActivityKind::BuildingProject
            };
            return Classified::new(
                kind,
                match kind {
                    ActivityKind::RunningTests => "Running tests",
                    ActivityKind::StartingDevServer => "Starting development server",
                    _ => "Building project",
                },
            )
            .described(command);
        }
        "pip" | "pip3" | "poetry" | "uv" | "brew" | "apt" | "apt-get" | "choco" | "winget" => {
            return Classified::new(
                ActivityKind::InstallingDependencies,
                "Installing dependencies",
            )
            .described(command);
        }
        "rg" | "grep" | "egrep" | "ag" | "ack" | "findstr" | "select-string" => {
            // The pattern is the first word that is not a flag, in its real spelling.
            let pattern = words.first_operand().cloned().unwrap_or_default();
            return Classified::new(ActivityKind::SearchingCode, "Searching code")
                .described(command)
                .with_query(pattern);
        }
        "find" | "fd" | "glob" => {
            return Classified::new(ActivityKind::SearchingFiles, "Searching files")
                .described(command);
        }
        "ls" | "dir" | "tree" | "get-childitem" => {
            return Classified::new(ActivityKind::ListingDirectory, "Listing files")
                .described(command);
        }
        "cat" | "head" | "tail" | "bat" | "less" | "more" | "type" | "get-content" => {
            let path = words
                .raw
                .iter()
                .skip(1)
                .find(|word| !word.starts_with('-') && word.contains('.'))
                .cloned()
                .unwrap_or_default();
            let title = if path.is_empty() {
                "Reading file".to_owned()
            } else {
                format!("Reading {}", file_name_of(&path))
            };
            return Classified::new(ActivityKind::ReadingFile, title)
                .described(command)
                .with_path(path);
        }
        "sed" => {
            // `sed -n '1,40p' file` is how an agent reads; a substitution is an edit.
            let kind = if has("-n") {
                ActivityKind::ReadingFile
            } else {
                ActivityKind::EditingFile
            };
            let title = if kind == ActivityKind::ReadingFile {
                "Reading file"
            } else {
                "Editing file"
            };
            return Classified::new(kind, title).described(command);
        }
        "curl" | "wget" => {
            let url = words
                .raw
                .iter()
                .skip(1)
                .find(|word| word.starts_with("http"))
                .cloned()
                .unwrap_or_default();
            let host = host_of(&url);
            let title = host.clone().map_or_else(
                || "Fetching a page".to_owned(),
                |host| format!("Reading {host}"),
            );
            let mut classified =
                Classified::new(ActivityKind::ReadingWebpage, title).described(command);
            classified.meta.url = (!url.is_empty()).then_some(url);
            classified.meta.host = host;
            return classified;
        }
        "bash" | "sh" | "zsh" | "python" | "python3" | "node" | "deno" | "powershell" | "pwsh" => {
            // `node --test` is the test runner wearing the interpreter's name.
            if has("--test") || has("-m") && has("pytest") {
                return Classified::new(ActivityKind::RunningTests, "Running tests")
                    .described(command);
            }
            let script = words
                .raw
                .iter()
                .skip(1)
                .find(|word| !word.starts_with('-') && word.contains('.'))
                .cloned();
            let title = script.as_ref().map_or_else(
                || "Running script".to_owned(),
                |path| format!("Running {}", file_name_of(path)),
            );
            return Classified::new(ActivityKind::RunningScript, title).described(command);
        }
        _ => {}
    }

    if head.starts_with("./") || head.ends_with(".sh") || head.ends_with(".ps1") {
        return Classified::new(
            ActivityKind::RunningScript,
            format!("Running {}", file_name_of(head)),
        )
        .described(command);
    }

    Classified::new(ActivityKind::RunningCommand, "Running").described(command)
}

/// A backend's tool identifier turned into words, for the tools we do not know by name.
///
/// `apply_patch` reads as "Apply patch" and `readMultipleFiles` as "Read multiple files".
/// An identifier with no human content in it at all — `tool_call_2837` — has nothing worth
/// showing, so it says "Working" rather than showing the id.
#[must_use]
pub fn humanize_tool_name(name: &str) -> String {
    let cleaned = name.trim().trim_start_matches("mcp__");
    if cleaned.is_empty() {
        return "Working".to_owned();
    }
    // An opaque handle is not a label: `tool_call_2837`, `call_a91f`, `fn_7`.
    let looks_opaque = cleaned
        .rsplit(['_', '-'])
        .next()
        .is_some_and(|tail| tail.len() > 1 && tail.chars().all(|c| c.is_ascii_hexdigit()));
    if looks_opaque {
        return "Working".to_owned();
    }

    let mut words = String::new();
    for (index, ch) in cleaned.char_indices() {
        if ch == '_' || ch == '-' || ch == '.' {
            words.push(' ');
        } else if ch.is_ascii_uppercase() && index > 0 && !words.ends_with(' ') {
            words.push(' ');
            words.push(ch.to_ascii_lowercase());
        } else {
            words.push(ch);
        }
    }
    let trimmed = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.is_empty() {
        return "Working".to_owned();
    }
    let mut chars = trimmed.chars();
    chars.next().map_or_else(
        || "Working".to_owned(),
        |first| first.to_ascii_uppercase().to_string() + chars.as_str(),
    )
}

/// What a backend's own tool step is, read from its name and the input it reported.
///
/// `detail` is whatever `tool_target` pulled out of the vendor's arguments — a path for a
/// read, the command line for a shell tool, the pattern for a search. That is the only
/// reason a `Bash` step can be told from a test run.
#[must_use]
pub fn classify_vendor_tool(name: &str, detail: &str) -> Classified {
    let lower = name.to_ascii_lowercase();
    let detail = detail.trim();
    let has = |needles: &[&str]| needles.iter().any(|needle| lower.contains(needle));

    // A shell tool is classified by what it actually ran, never by the tool's name.
    if has(&[
        "bash",
        "shell",
        "exec",
        "command",
        "terminal",
        "powershell",
        "process",
    ]) {
        let mut classified = classify_command(detail);
        if classified.description.is_none() && !detail.is_empty() {
            classified = classified.described(detail);
        }
        return classified;
    }

    if lower.starts_with("mcp__") || has(&["mcp"]) {
        let server = lower
            .trim_start_matches("mcp__")
            .split("__")
            .next()
            .unwrap_or_default()
            .to_owned();
        let title = if server.is_empty() {
            "Using a connected tool".to_owned()
        } else {
            format!("Using {}", humanize_tool_name(&server))
        };
        return Classified::new(ActivityKind::UsingMcp, title).described(detail);
    }

    if has(&["todo", "plan", "task list"]) {
        return Classified::new(ActivityKind::Planning, "Planning").described(detail);
    }
    if has(&["subagent", "sub_agent", "spawn_agent", "delegate", "agent("]) || lower == "task" {
        // §16: the row says "Delegated" and names the task beneath it. The task also
        // becomes the agent label, so the delegate's own steps can be grouped under it
        // once the runtime reports them.
        let mut classified =
            Classified::new(ActivityKind::StartingSubagent, "Delegated").described(detail);
        classified.meta.agent_label = (!detail.is_empty()).then(|| detail.to_owned());
        return classified;
    }
    if has(&["websearch", "web_search", "search_web"]) {
        return Classified::new(ActivityKind::SearchingWeb, "Searching the web")
            .described(detail)
            .with_query(detail);
    }
    if has(&["webfetch", "web_fetch", "fetch", "curl", "http"]) {
        let host = host_of(detail);
        let title = host.clone().map_or_else(
            || "Reading a page".to_owned(),
            |host| format!("Reading {host}"),
        );
        let mut classified = Classified::new(ActivityKind::ReadingWebpage, title);
        classified.meta.url = (!detail.is_empty()).then(|| detail.to_owned());
        classified.meta.host = host;
        return classified;
    }
    if has(&["screenshot", "capture"]) {
        return Classified::new(ActivityKind::TakingScreenshot, "Taking a screenshot")
            .described(detail);
    }
    if has(&["browser", "playwright", "puppeteer"]) {
        return Classified::new(ActivityKind::TestingBrowser, "Checking the browser")
            .described(detail);
    }
    if has(&["click", "type_text", "keypress"]) {
        return Classified::new(ActivityKind::ClickingUi, "Interacting with the app")
            .described(detail);
    }
    if has(&["computer", "desktop"]) {
        return Classified::new(ActivityKind::InspectingUi, "Inspecting the screen")
            .described(detail);
    }
    if has(&["glob", "listdir", "list_dir", "ls("]) || lower == "ls" {
        return Classified::new(ActivityKind::ListingDirectory, "Listing files")
            .described(detail)
            .with_query(detail);
    }
    if has(&["grep", "search", "find", "ripgrep"]) {
        return Classified::new(ActivityKind::SearchingCode, "Searching the project")
            .described(detail)
            .with_query(detail);
    }
    if has(&["multiedit", "edit", "patch", "apply", "replace", "update"]) {
        let kind = if has(&["patch"]) {
            ActivityKind::ApplyingPatch
        } else {
            ActivityKind::EditingFile
        };
        let title = if detail.is_empty() {
            "Editing".to_owned()
        } else {
            format!("Editing {}", file_name_of(detail))
        };
        return Classified::new(kind, title).with_path(detail);
    }
    if has(&["delete", "remove", "rm("]) {
        let title = if detail.is_empty() {
            "Deleting a file".to_owned()
        } else {
            format!("Deleting {}", file_name_of(detail))
        };
        return Classified::new(ActivityKind::DeletingFile, title).with_path(detail);
    }
    if has(&["rename", "move", "mv("]) {
        return Classified::new(ActivityKind::MovingFile, "Renaming").described(detail);
    }
    if has(&["write", "create", "notebook"]) {
        let title = if detail.is_empty() {
            "Creating a file".to_owned()
        } else {
            format!("Creating {}", file_name_of(detail))
        };
        return Classified::new(ActivityKind::CreatingFile, title).with_path(detail);
    }
    if has(&["read", "view", "open", "cat"]) {
        let title = if detail.is_empty() {
            "Reading a file".to_owned()
        } else {
            format!("Reading {}", file_name_of(detail))
        };
        return Classified::new(ActivityKind::ReadingFile, title).with_path(detail);
    }
    if has(&["image", "vision", "photo"]) {
        return Classified::new(ActivityKind::ViewingImage, "Looking at an image")
            .described(detail);
    }

    Classified::new(ActivityKind::UsingTool, humanize_tool_name(name)).described(detail)
}

/// Passed and failed counts, read out of a test runner's own summary line.
///
/// Only the shapes the major runners actually print are matched. A run whose output does not
/// contain one of them returns `None` — the transcript then says the tests finished, which is
/// true, rather than inventing a total.
#[must_use]
pub fn parse_test_totals(output: &str) -> Option<(u32, u32)> {
    let mut passed: Option<u32> = None;
    let mut failed: Option<u32> = None;

    for line in output.lines().rev().take(60) {
        let lower = line.to_ascii_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();

        // `# pass 12` / `# fail 0` (node --test)
        if let Some(rest) = lower.strip_prefix("# pass ") {
            passed = passed.or_else(|| rest.trim().parse().ok());
        }
        if let Some(rest) = lower.strip_prefix("# fail ") {
            failed = failed.or_else(|| rest.trim().parse().ok());
        }

        // `502 passed`, `3 failed`, `12 passed, 2 failed` (jest, vitest, pytest, cargo)
        for (index, word) in words.iter().enumerate() {
            let count = || -> Option<u32> {
                index
                    .checked_sub(1)
                    .and_then(|prev| words.get(prev))
                    .and_then(|value| {
                        value
                            .trim_matches(|c: char| !c.is_ascii_digit())
                            .parse()
                            .ok()
                    })
            };
            if word.starts_with("passed") || word.starts_with("passing") {
                passed = passed.or_else(count);
            }
            if word.starts_with("failed") || word.starts_with("failing") {
                failed = failed.or_else(count);
            }
        }

        if passed.is_some() && failed.is_some() {
            break;
        }
    }

    match (passed, failed) {
        (None, None) => None,
        (passed, failed) => Some((passed.unwrap_or(0), failed.unwrap_or(0))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_of(command: &str) -> ActivityKind {
        classify_command(command).kind
    }

    #[test]
    fn a_command_is_read_for_what_it_does_not_for_its_first_word() {
        // The whole point of §13: these are all "Ran" today, and all different.
        assert_eq!(kind_of("npm test"), ActivityKind::RunningTests);
        assert_eq!(kind_of("pytest -q"), ActivityKind::RunningTests);
        assert_eq!(
            kind_of("cargo test --workspace"),
            ActivityKind::RunningTests
        );
        assert_eq!(kind_of("npm run build"), ActivityKind::BuildingProject);
        assert_eq!(
            kind_of("cargo build --release"),
            ActivityKind::BuildingProject
        );
        assert_eq!(kind_of("eslint ."), ActivityKind::Linting);
        assert_eq!(kind_of("cargo clippy"), ActivityKind::Linting);
        assert_eq!(kind_of("tsc --noEmit"), ActivityKind::Typechecking);
        assert_eq!(kind_of("cargo check"), ActivityKind::Typechecking);
        assert_eq!(kind_of("git status"), ActivityKind::GitStatus);
        assert_eq!(kind_of("git diff HEAD"), ActivityKind::GitDiff);
        assert_eq!(kind_of("git commit -m x"), ActivityKind::GitCommit);
        assert_eq!(kind_of("rg PlayerController"), ActivityKind::SearchingCode);
        assert_eq!(kind_of("ls -la src"), ActivityKind::ListingDirectory);
        assert_eq!(kind_of("npm run dev"), ActivityKind::StartingDevServer);
        assert_eq!(
            kind_of("npm install three"),
            ActivityKind::InstallingDependencies
        );
    }

    #[test]
    fn the_words_are_the_ones_a_person_would_say() {
        assert_eq!(classify_command("npm test").title, "Running tests");
        assert_eq!(classify_command("tsc --noEmit").title, "Checking types");
        assert_eq!(classify_command("git diff").title, "Reviewing changes");
        assert_eq!(classify_command("git status").title, "Checking Git status");
        assert_eq!(
            classify_command("npm run dev").title,
            "Starting development server"
        );
        // The line itself is the quiet second row, so §12's "Running / npm run build" works.
        assert_eq!(
            classify_command("npm run build").description.as_deref(),
            Some("npm run build")
        );
    }

    #[test]
    fn a_prefix_that_says_nothing_does_not_decide_the_reading() {
        assert_eq!(kind_of("cd ui && npm test"), ActivityKind::RunningTests);
        assert_eq!(
            kind_of("NODE_ENV=test npx vitest run"),
            ActivityKind::RunningTests
        );
        assert_eq!(
            kind_of("cd crates/app && cargo build"),
            ActivityKind::BuildingProject
        );
    }

    #[test]
    fn reading_a_file_by_shell_names_the_file() {
        let read = classify_command("sed -n '1,40p' src/player/PlayerController.ts");
        assert_eq!(read.kind, ActivityKind::ReadingFile);
        let cat = classify_command("cat src/player/PlayerController.ts");
        assert_eq!(cat.kind, ActivityKind::ReadingFile);
        assert_eq!(cat.title, "Reading PlayerController.ts");
        // The recorded path keeps its real spelling: a lowercased one names no file on disk.
        assert_eq!(
            cat.meta.paths,
            vec!["src/player/PlayerController.ts".to_owned()]
        );
        // A substitution is an edit, not a read, even though the binary is the same.
        assert_eq!(kind_of("sed -i 's/a/b/' x.ts"), ActivityKind::EditingFile);
    }

    #[test]
    fn a_search_carries_what_was_searched_for() {
        let found = classify_command("rg -n \"camera shake\" ui/src");
        assert_eq!(found.kind, ActivityKind::SearchingCode);
        assert_eq!(found.meta.query.as_deref(), Some("camera shake"));
    }

    #[test]
    fn a_vendor_tool_never_reaches_the_surface_by_its_own_name() {
        // The four names §3 calls out by name.
        assert_eq!(
            classify_vendor_tool("commandExecution", "npm test").title,
            "Running tests"
        );
        assert_eq!(
            classify_vendor_tool("apply_patch", "src/Game.tsx").kind,
            ActivityKind::ApplyingPatch
        );
        assert_eq!(
            classify_vendor_tool("fs_read", "src/player/PlayerController.ts").title,
            "Reading PlayerController.ts"
        );
        // An opaque handle has nothing human in it, so it is not shown at all.
        assert_eq!(humanize_tool_name("tool_call_2837"), "Working");
    }

    #[test]
    fn a_shell_tool_is_classified_by_what_it_ran() {
        assert_eq!(
            classify_vendor_tool("Bash", "cargo test --workspace").kind,
            ActivityKind::RunningTests
        );
        assert_eq!(
            classify_vendor_tool("Bash", "git status --porcelain").kind,
            ActivityKind::GitStatus
        );
    }

    #[test]
    fn an_unknown_tool_is_named_in_words_not_in_identifiers() {
        assert_eq!(humanize_tool_name("apply_patch"), "Apply patch");
        assert_eq!(
            humanize_tool_name("readMultipleFiles"),
            "Read multiple files"
        );
        assert_eq!(
            classify_vendor_tool("GodotSceneTree", "").title,
            "Godot scene tree"
        );
    }

    #[test]
    fn file_tools_name_the_file_and_record_the_path() {
        let edit = classify_vendor_tool("MultiEdit", "src/player/PlayerController.ts");
        assert_eq!(edit.kind, ActivityKind::EditingFile);
        assert_eq!(edit.title, "Editing PlayerController.ts");
        assert_eq!(
            edit.meta.paths,
            vec!["src/player/PlayerController.ts".to_owned()]
        );

        let write = classify_vendor_tool("Write", "src/camera/CameraShake.ts");
        assert_eq!(write.kind, ActivityKind::CreatingFile);
        assert_eq!(write.title, "Creating CameraShake.ts");
    }

    #[test]
    fn a_web_step_shows_the_host_and_keeps_the_url_for_the_disclosure() {
        let read = classify_vendor_tool(
            "WebFetch",
            "https://docs.godotengine.org/en/stable/x.html?a=1",
        );
        assert_eq!(read.kind, ActivityKind::ReadingWebpage);
        assert_eq!(read.title, "Reading docs.godotengine.org");
        assert_eq!(read.meta.host.as_deref(), Some("docs.godotengine.org"));
        assert!(read.meta.url.is_some());
    }

    #[test]
    fn test_totals_come_from_the_runner_and_are_never_guessed() {
        assert_eq!(
            parse_test_totals("test result: ok. 502 passed; 0 failed; 0 ignored"),
            Some((502, 0))
        );
        assert_eq!(
            parse_test_totals("Tests:       2 failed, 12 passed, 14 total"),
            Some((12, 2))
        );
        assert_eq!(parse_test_totals("# pass 12\n# fail 0\n"), Some((12, 0)));
        // Output with no summary in it reports nothing rather than a made-up zero.
        assert_eq!(parse_test_totals("compiling...\nlinking...\n"), None);
    }

    #[test]
    fn an_unrecognised_line_is_vague_but_never_wrong() {
        let odd = classify_command("./scripts/deploy.sh --dry-run");
        assert_eq!(odd.kind, ActivityKind::RunningScript);
        let unknown = classify_command("frobnicate --hard");
        assert_eq!(unknown.kind, ActivityKind::RunningCommand);
        assert_eq!(unknown.title, "Running");
        assert_eq!(unknown.description.as_deref(), Some("frobnicate --hard"));
    }

    #[test]
    fn a_status_knows_whether_its_row_is_still_alive() {
        assert!(ActivityStatus::InProgress.is_live());
        assert!(ActivityStatus::Queued.is_live());
        assert!(!ActivityStatus::Completed.is_live());
        assert!(!ActivityStatus::WaitingForUser.is_live());
    }
}
