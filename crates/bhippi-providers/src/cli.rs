//! CLI adapter: explicit argv from the catalogue template, scrubbed environment, no
//! visible console window (see `command`) — and **live streaming**.
//!
//! The adapter used to call `Command::output()`, which blocks until the vendor process
//! exits, and then chunked the finished answer to fake a stream. Everything worked and
//! everything felt broken: on a turn that takes a coding agent ninety seconds, the user
//! watched a spinner for ninety seconds and then got the whole reply at once. Nothing
//! about that is a model being slow — it is the adapter refusing to listen until the
//! process was dead.
//!
//! One backend is fed the other way round as well. Claude Code's engineered turn is tens
//! of kilobytes with `--`-looking lines inside it, and Windows' npm launcher re-splits an
//! argv element of that shape on the hop to the native binary — the CLI then rejects a
//! line of the prompt as an unknown flag. Its recipe therefore carries no `{prompt}` and
//! the text is written to the child's stdin, which is then closed; see
//! `ProviderSpec::prompt_via_stdin`.
//!
//! Grok hits the same wall from the other side: no stdin print mode, so the prompt has to
//! be named rather than piped. Its recipe carries `{prompt_file}`, the turn is written to a
//! temp file, and argv holds only the path — see [`PromptFile`].
//!
//! So the child is spawned, its stdout is read a line at a time, and each line goes
//! through [`transcript::Reader`] into a `Delta` the moment it arrives. First words reach
//! the screen in about a second. The timeout is a *silence* budget rather than a wall
//! clock, because a healthy agent that has been streaming for four minutes is working,
//! not hung, and killing it at a fixed 180 s was losing real answers.

use crate::catalog::{ProviderSpec, PROMPT_FILE};
use crate::command::resolve_command;
use crate::fault::{self, FaultKind};
use crate::model::{
    Capabilities, CompletionRequest, CostClass, Delta, DeltaStream, Message, StopReason,
};
use crate::provider::Provider;
use crate::transcript::{self, TranscriptEvent};
use async_trait::async_trait;
use bhippi_types::{BhippiError, Health, Result, TaskClass};
use futures_util::StreamExt;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

/// How long the vendor may say **nothing at all** before it is treated as hung.
///
/// This is not the length of a turn. A coding agent legitimately spends minutes on one
/// answer, and for all of them it is printing tool events, reasoning, or text; the only
/// thing that never happens during healthy work is total silence. Ninety seconds of it
/// is a hang.
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// The absolute ceiling for one turn, however talkative. A runaway agent has to end.
const HARD_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// The path [`CliProvider::argv_for`] substitutes for `{prompt_file}`. Tests read the shape
/// of an argv; only a real turn writes a real file.
const STAND_IN_PROMPT_FILE: &str = "<prompt-file>";

/// stderr lines kept for explaining a failure. The tail is what carries the reason.
const STDERR_TAIL: usize = 12;

/// Where a Computer Use argv fragment may be spliced into a vendor's prompt recipe.
///
/// This is the whole fix for the bug that made Computer Use do nothing at all. Several of
/// the flags we need take *lists* of values — `claude --add-dir <directories...>`,
/// `codex --image <files...>` — and a list flag keeps eating arguments until it meets one
/// that starts with `-`. The fragment used to be appended just before the prompt, so the
/// prompt itself was eaten as one more directory and `claude` exited with "Input must be
/// provided either through stdin or as a prompt argument". No screenshot, no action, no
/// pointer movement, and an error that pointed nowhere near the cause.
///
/// So the fragment goes in front of the vendor's *own* first flag instead: after any
/// leading subcommand (`codex exec`), before everything else. That leaves the prompt in
/// exactly the position the vendor's recipe already proved works, and guarantees whatever
/// follows the fragment starts with `-` and therefore terminates any list flag inside it.
///
/// `None` means the recipe has no flag to hide behind, in which case we add nothing rather
/// than risk swallowing the prompt again.
fn computer_use_splice_index(args: &[&str]) -> Option<usize> {
    args.iter().position(|arg| arg.starts_with('-'))
}

/// Why a spawn failed, in terms of the thing that has to change.
///
/// One errno gets its own sentence. `ERROR_FILENAME_EXCED_RANGE` (206) names no filename in
/// practice: it is Windows refusing a command line over 32,767 characters, which only a
/// recipe that puts the engineered turn in argv can produce. Reported raw, it read as
/// "provider Grok CLI unavailable: could not start it: The filename or extension is too
/// long", which points at the install, the PATH and the binary — none of them the cause.
fn spawn_reason(error: std::io::Error) -> String {
    if error.raw_os_error() == Some(206) {
        return concat!(
            "the turn was too long for a Windows command line — this backend needs a ",
            "stdin or prompt-file recipe, not `{prompt}` in argv"
        )
        .to_owned();
    }
    format!("could not start it: {error}")
}

/// A rendered turn on disk, for a vendor whose recipe carries `{prompt_file}`.
///
/// It exists because of a limit, not a preference: Windows caps a whole command line at
/// 32,767 characters and an engineered turn is 30–60 KB, so `grok -p <PROMPT>` failed at
/// `CreateProcess` with "The filename or extension is too long. (os error 206)" — before
/// the vendor ran, which is why the error named no model, no flag and no prompt.
///
/// The file is deleted on drop, and the value is moved into the task that owns the child,
/// so it outlives every path the turn can take — finished, failed, timed out or stopped —
/// and outlives none of them.
struct PromptFile {
    path: PathBuf,
}

impl PromptFile {
    /// Writes `prompt` somewhere the vendor can read it. UTF-8, no trailing ceremony: the
    /// bytes are the prompt, because `--verbatim` promises the vendor sends them as given.
    fn write(spec: &ProviderSpec, prompt: &str) -> Result<Self> {
        let dir = std::env::temp_dir().join("bhippi-prompts");
        std::fs::create_dir_all(&dir).map_err(|error| {
            provider_error(spec, format!("could not open a prompt directory: {error}"))
        })?;
        // A ULID rather than the turn id: two turns of the same chat can overlap, and a
        // second one reusing the name would rewrite the first one's prompt under it.
        let path = dir.join(format!("{}-{}.md", spec.id, ulid::Ulid::new()));
        std::fs::write(&path, prompt).map_err(|error| {
            provider_error(spec, format!("could not write its prompt: {error}"))
        })?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PromptFile {
    fn drop(&mut self) {
        // A prompt left in the temp directory is a transcript of somebody's work sitting
        // where any process can read it, so the failure to remove one is worth a line.
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!(path = %self.path.display(), %error, "prompt file left behind");
            }
        }
    }
}

/// Vendor flags that narrow a coding-agent CLI to a single Computer Use decision.
///
/// Two jobs: hand the agent the screenshot in whatever way it accepts, and take away the
/// tools it would otherwise reach for. Left unrestricted, every one of these agents answers
/// "click the Start button" by opening a shell and trying to do it itself — which is both
/// the wrong layer and the thing users saw as "it just runs commands in cmd".
fn computer_use_args(spec: &ProviderSpec, req: &CompletionRequest) -> Vec<OsString> {
    let mut argv: Vec<OsString> = Vec::new();
    let mut push = |value: &str| argv.push(OsString::from(value));
    match spec.id {
        // Codex takes screenshots as first-class vision input, so no Read tool is needed.
        // `read-only` still applies to any shell it attempts; `--ephemeral` keeps a desktop
        // task out of the resumable session history.
        "codex" => {
            for path in &req.image_paths {
                push("--image");
                push(path);
            }
            push("--sandbox");
            push("read-only");
            push("--ephemeral");
        }
        // Claude has no image flag, but its Read tool opens local images, and `--add-dir`
        // is what lets it read one from the temp directory the capture was written to.
        "claude" => {
            push("--permission-mode");
            push("dontAsk");
            push("--tools");
            push("Read");
            for directory in image_parent_directories(&req.image_paths) {
                push("--add-dir");
                push(&directory);
            }
        }
        // Grok's allowlist uses internal tool ids (`read_file`), not Claude's `Read`.
        "grok" => {
            // The recipe already supplies --permission-mode; Grok rejects duplicates.
            push("--tools");
            push("read_file");
        }
        "antigravity" => {
            for directory in image_parent_directories(&req.image_paths) {
                push("--add-dir");
                push(&directory);
            }
        }
        _ => {}
    }
    argv
}

/// Vendor flags an ordinary turn needs purely because the user attached a file.
///
/// Claude Code's Read tool refuses a path outside its working directory, so an image the
/// user picked from Pictures is unreadable without `--add-dir` on its parent — the same
/// flag Computer Use already adds for the screenshot's temp directory, needed for exactly
/// the same reason. Nothing else is granted here: `--permission-mode dontAsk` and
/// `--tools Read` stay Computer Use's, because those narrow a desktop turn rather than
/// widen an ordinary one.
fn attachment_args(spec: &ProviderSpec, req: &CompletionRequest) -> Vec<OsString> {
    if !matches!(spec.id, "claude" | "antigravity") || req.image_paths.is_empty() {
        return Vec::new();
    }
    let mut argv: Vec<OsString> = Vec::new();
    for directory in image_parent_directories(&req.image_paths) {
        argv.push(OsString::from("--add-dir"));
        argv.push(OsString::from(directory));
    }
    argv
}

/// The flags that attach the turn's MCP servers, for the two backends that host them
/// (SPA-202).
///
/// Claude Code reads a JSON file (`--mcp-config`) and, in print mode, needs the server on
/// the allow-list or every tool call is refused before the model sees it. Codex takes the
/// same facts as `-c mcp_servers.<name>.<key>=<toml>` overrides. Both ride after the
/// recipe's own flags — `--strict-mcp-config` stays, so nothing but ours is loaded.
fn mcp_args(spec: &ProviderSpec, req: &CompletionRequest) -> Vec<OsString> {
    if req.mcp_servers.is_empty() {
        return Vec::new();
    }
    let mut argv: Vec<OsString> = Vec::new();
    match spec.id {
        "claude" => {
            if let Some(path) = write_mcp_config(&req.mcp_servers) {
                argv.push(OsString::from("--mcp-config"));
                argv.push(path.into_os_string());
                for server in &req.mcp_servers {
                    argv.push(OsString::from("--allowedTools"));
                    argv.push(OsString::from(format!("mcp__{}", server.name)));
                }
            }
        }
        "codex" => {
            for server in &req.mcp_servers {
                argv.push(OsString::from("-c"));
                argv.push(OsString::from(format!(
                    "mcp_servers.{}.command={}",
                    server.name,
                    toml_string(&server.command)
                )));
                argv.push(OsString::from("-c"));
                argv.push(OsString::from(format!(
                    "mcp_servers.{}.args=[{}]",
                    server.name,
                    server
                        .args
                        .iter()
                        .map(|arg| toml_string(arg))
                        .collect::<Vec<_>>()
                        .join(",")
                )));
                for (key, value) in &server.env {
                    argv.push(OsString::from("-c"));
                    argv.push(OsString::from(format!(
                        "mcp_servers.{}.env.{key}={}",
                        server.name,
                        toml_string(value)
                    )));
                }
            }
        }
        _ => {}
    }
    argv
}

fn toml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Writes the `mcpServers` file Claude Code reads. Content-addressed, so the same servers
/// reuse one file turn after turn and a changed command gets a fresh one.
fn write_mcp_config(servers: &[crate::model::McpServer]) -> Option<std::path::PathBuf> {
    let mut map = serde_json::Map::new();
    for server in servers {
        let mut entry = serde_json::Map::new();
        entry.insert(
            "command".to_owned(),
            serde_json::Value::String(server.command.clone()),
        );
        entry.insert(
            "args".to_owned(),
            serde_json::Value::Array(
                server
                    .args
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
        if !server.env.is_empty() {
            entry.insert(
                "env".to_owned(),
                serde_json::Value::Object(
                    server
                        .env
                        .iter()
                        .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
                        .collect(),
                ),
            );
        }
        map.insert(server.name.clone(), serde_json::Value::Object(entry));
    }
    let body = serde_json::json!({ "mcpServers": serde_json::Value::Object(map) });
    let text = serde_json::to_string_pretty(&body).ok()?;
    let dir = std::env::temp_dir().join("bhippi-mcp");
    std::fs::create_dir_all(&dir).ok()?;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let path = dir.join(format!("{hash:016x}.json"));
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, text).ok()?;
    }
    Some(path)
}

fn effort_flag_args(spec: &ProviderSpec, req: &CompletionRequest) -> Vec<OsString> {
    let Some(level) = req
        .reasoning_effort
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return Vec::new();
    };
    match spec.id {
        "claude" => vec![
            OsString::from("--effort"),
            OsString::from(claude_effort_level(level)),
        ],
        "grok" => vec![
            OsString::from("--reasoning-effort"),
            OsString::from(grok_effort_level(level)),
        ],
        "codex" => vec![
            OsString::from("-c"),
            OsString::from(format!(
                "model_reasoning_effort={}",
                grok_effort_level(level)
            )),
        ],
        "antigravity" => {
            let model = req.model.as_deref().unwrap_or("");
            let resolved = if !model.is_empty() {
                normalize_antigravity_model(model, Some(level))
            } else {
                String::new()
            };
            let lower = resolved.to_ascii_lowercase();
            // Claude and GPT-OSS models in Antigravity do not support --effort
            if lower.contains("claude") || lower.contains("gpt-oss") {
                return Vec::new();
            }
            vec![
                OsString::from("--effort"),
                OsString::from(
                    antigravity_speed_from_model(Some(&resolved))
                        .unwrap_or_else(|| antigravity_effort_level(level)),
                ),
            ]
        }
        _ => Vec::new(),
    }
}

fn claude_effort_level(level: &str) -> &str {
    match level {
        "minimal" | "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" => "xhigh",
        "max" | "ultra" => "max",
        other => other,
    }
}

fn grok_effort_level(level: &str) -> &str {
    match level {
        "minimal" => "low",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "max" | "ultra" | "xhigh" => "xhigh",
        other => other,
    }
}

/// Antigravity CLI documents `--effort low|medium|high` only. Composer steps
/// above High collapse onto `high` rather than inventing a flag the vendor rejects.
fn antigravity_effort_level(level: &str) -> &str {
    match level {
        "minimal" | "low" | "fast" => "low",
        "medium" => "medium",
        "high" | "balanced" | "extra" | "quality" | "max" | "ultra" | "xhigh" => "high",
        other => other,
    }
}

/// `gemini-3.8-flash-high` already *is* the speed. `--effort` must match that
/// suffix so a High slug never goes out with `--effort low`.
fn antigravity_speed_from_model(model: Option<&str>) -> Option<&'static str> {
    let slug = model?.trim().to_ascii_lowercase();
    if slug.ends_with("-low") {
        Some("low")
    } else if slug.ends_with("-medium") {
        Some("medium")
    } else if slug.ends_with("-high") {
        Some("high")
    } else {
        None
    }
}

/// Normalizes friendly names like "Gemini 3.8 Flash" into canonical agy model slugs.
/// The model the picker shows is a *label* — "Claude Opus 5", "Big Pickle". A CLI wants its
/// own id, and gets the label verbatim unless something maps it here, which is why a turn
/// died with *"There's an issue with the selected model (Claude Opus 5)"*: that string was
/// on the command line as `--model`.
///
/// Claude Code documents aliases for the latest of each family — `fable`, `opus`, `sonnet`,
/// `haiku` — and also takes a full `claude-*` id. Anything already in one of those shapes is
/// left exactly as it is; only a label is translated.
fn normalize_claude_model(model: &str) -> String {
    let trimmed = model.trim();
    let lower = trimmed.to_ascii_lowercase();

    // Already an id or a bare alias: pass it through untouched.
    if lower.starts_with("claude-") {
        return trimmed.to_string();
    }
    if matches!(lower.as_str(), "fable" | "opus" | "sonnet" | "haiku") {
        return lower;
    }

    // A label names its family, and the alias is what the CLI accepts for it.
    for family in ["fable", "opus", "sonnet", "haiku"] {
        if lower.contains(family) {
            return family.to_string();
        }
    }
    // If a version string or unexpected Claude Code label leaked in, fall back to "sonnet"
    if lower.contains("claude code") || lower.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return "sonnet".to_string();
    }
    trimmed.to_string()
}

/// OpenCode takes `provider/model` and nothing else, so a label like `Big Pickle` is refused
/// before a single token is generated.
///
/// A label that already carries a `/` is an id and is passed through. Otherwise it is
/// slugified and attributed to the `opencode` provider, which is what the free models it
/// hosts are actually called: `Big Pickle` → `opencode/big-pickle`, `Nemotron 3.5 Lightning
/// Free` → `opencode/nemotron-3.5-lightning-free`.
fn normalize_opencode_model(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.contains('/') {
        return trimmed.to_string();
    }

    let mut slug = String::with_capacity(trimmed.len());
    let mut pending_dash = false;
    for character in trimmed.chars() {
        if character.is_ascii_alphanumeric() || character == '.' {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(character.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if slug.is_empty() {
        return trimmed.to_string();
    }
    format!("opencode/{slug}")
}

fn normalize_antigravity_model(model: &str, effort_level: Option<&str>) -> String {
    let trimmed = model.trim();
    let lower = trimmed.to_ascii_lowercase();

    // Already a canonical slug ending with speed or thinking/medium
    if lower.ends_with("-low")
        || lower.ends_with("-medium")
        || lower.ends_with("-high")
        || lower.ends_with("-thinking")
    {
        return lower;
    }

    let speed = match effort_level {
        Some("minimal" | "low" | "fast") => "low",
        Some("medium") => "medium",
        _ => "high",
    };

    if lower.contains("3.8") && lower.contains("flash") {
        format!("gemini-3.8-flash-{speed}")
    } else if lower.contains("3.7") && lower.contains("flash") {
        format!("gemini-3.7-flash-{speed}")
    } else if lower.contains("3.6") && lower.contains("flash") {
        format!("gemini-3.6-flash-{speed}")
    } else if lower.contains("3.1") && lower.contains("pro") {
        let pro_speed = if speed == "low" { "low" } else { "high" };
        format!("gemini-3.1-pro-{pro_speed}")
    } else if lower.contains("sonnet") {
        "claude-sonnet-4-6".to_string()
    } else if lower.contains("opus") {
        "claude-opus-4-6-thinking".to_string()
    } else if lower.contains("gpt-oss") || lower.contains("120b") {
        "gpt-oss-120b-medium".to_string()
    } else if lower.starts_with("gemini-") {
        format!("{lower}-{speed}")
    } else {
        lower
    }
}

/// Antigravity `--input-format stream-json` expects one NDJSON user event per turn,
/// not the raw prompt Claude's `-p` reads. The prompt is JSON-escaped so a line that
/// starts with `--` cannot become a flag.
fn antigravity_user_event(prompt: &str) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(&serde_json::json!({
        "event": "user",
        "message": { "content": prompt }
    }))
    .unwrap_or_else(|_| prompt.as_bytes().to_vec());
    bytes.push(b'\n');
    bytes
}

// Migrate display labels stored by the old picker without rewriting canonical/custom IDs.
fn normalize_codex_model(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.to_ascii_lowercase().starts_with("gpt-") && trimmed.contains(' ') {
        trimmed
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
            .to_ascii_lowercase()
    } else {
        trimmed.to_owned()
    }
}

fn model_flag_args(spec: &ProviderSpec, req: &CompletionRequest) -> Vec<OsString> {
    let Some(template) = spec.model_args else {
        return Vec::new();
    };
    let Some(model) = req
        .model
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return Vec::new();
    };
    let resolved_model = match spec.id {
        "antigravity" => normalize_antigravity_model(model, req.reasoning_effort.as_deref()),
        "codex" => normalize_codex_model(model),
        "claude" => normalize_claude_model(model),
        "opencode" => normalize_opencode_model(model),
        _ => model.to_string(),
    };
    template
        .iter()
        .map(|arg| OsString::from(arg.replace("{model}", &resolved_model)))
        .collect()
}

fn image_parent_directories(paths: &[String]) -> Vec<String> {
    let mut directories = Vec::new();
    for path in paths {
        let Some(parent) = std::path::Path::new(path).parent() else {
            continue;
        };
        let directory = parent.to_string_lossy().into_owned();
        if !directory.is_empty() && !directories.contains(&directory) {
            directories.push(directory);
        }
    }
    directories
}

/// One typed provider failure, built the same way wherever it is raised (R1) — the
/// adapter, the streaming task, and the stdin writer all report through this.
fn provider_error(spec: &ProviderSpec, reason: String) -> BhippiError {
    let advice = fault::advise(spec, &reason);
    BhippiError::Provider {
        id: spec.label.to_owned(),
        hint: Some(advice.fix),
        reason,
        retryable: advice.kind.retryable(),
    }
}

pub struct CliProvider {
    spec: &'static ProviderSpec,
    resolved: crate::command::ResolvedCommand,
    caps: Capabilities,
}

impl CliProvider {
    /// `None` when the catalogue entry has no prompt recipe or no launcher is found.
    #[must_use]
    pub fn open(spec: &'static ProviderSpec) -> Option<Self> {
        let resolved = resolve_command(spec.binary?)?;
        Some(Self {
            spec,
            resolved,
            caps: Capabilities {
                context_window: spec.context_window,
                vision: spec.vision,
                tools: true,
                // True since this adapter streams for real; the UI reads it to decide
                // whether to animate token arrival or show an indeterminate wait.
                streaming: true,
                tokens_per_second: None,
                cost_class: CostClass::Standard,
            },
        })
    }

    fn error(&self, reason: String) -> BhippiError {
        provider_error(self.spec, reason)
    }

    /// The exact argv this adapter would pass, model flag included. Split out so the
    /// contract is testable without spawning a vendor process.
    ///
    /// A `{prompt_file}` recipe gets [`STAND_IN_PROMPT_FILE`] where the real turn would
    /// name a temp file, so the shape of the argv can be checked without writing one.
    #[must_use]
    pub fn argv_for(spec: &ProviderSpec, prompt: &str, model: Option<&str>) -> Vec<String> {
        let request = CompletionRequest::new(
            TaskClass::Expander,
            "",
            vec![Message::user(prompt.to_owned())],
        )
        .with_model(model.map(str::to_owned));
        Self::argv_for_request(spec, &request, prompt, Path::new(STAND_IN_PROMPT_FILE))
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn argv_for_request(
        spec: &ProviderSpec,
        req: &CompletionRequest,
        prompt: &str,
        prompt_file: &Path,
    ) -> Vec<OsString> {
        let Some(args) = spec.prompt_args else {
            return Vec::new();
        };
        let mut extra = if req.computer_use {
            computer_use_args(spec, req)
        } else {
            attachment_args(spec, req)
        };
        extra.extend(mcp_args(spec, req));
        extra.extend(effort_flag_args(spec, req));
        // A backend that reads its prompt from stdin has nothing in argv for a
        // list-valued flag to swallow, so the fragment — Computer Use's, or an attachment's
        // `--add-dir` — goes after the recipe's own flags
        // instead of in front of them — which keeps `-p` first, exactly the invocation
        // the stdin path was verified against.
        let splice_at = if spec.prompt_via_stdin {
            None
        } else {
            computer_use_splice_index(args)
        };
        let model_args = model_flag_args(spec, req);

        // Model flags and Computer Use flags both go after a leading subcommand and
        // before the vendor's first flag. Putting `-m` after `{prompt}` makes Codex
        // treat it as part of the prompt (`exec … PROMPT -m foo`); putting it between
        // `-p` and the prompt makes Grok/Claude eat `--model` as the prompt itself.
        let mut argv = Vec::new();
        let mut inserted_prefix = false;
        for (index, arg) in args.iter().enumerate() {
            if !inserted_prefix && Some(index) == splice_at {
                argv.extend(extra.iter().cloned());
                argv.extend(model_args.iter().cloned());
                inserted_prefix = true;
            }
            if *arg == "{prompt}" {
                argv.push(OsString::from(prompt));
            } else if *arg == PROMPT_FILE {
                // The path, never the text. This element is what keeps the whole command
                // line inside Windows' 32,767-character limit however long the turn is.
                argv.push(prompt_file.as_os_str().to_owned());
            } else {
                argv.push(OsString::from(arg.replace("{prompt}", prompt)));
            }
        }
        if !inserted_prefix {
            argv.extend(extra);
            argv.extend(model_args);
        }
        argv
    }

    /// Flattens the conversation into one vendor prompt (CLI contracts take a string).
    fn render_prompt(req: &CompletionRequest) -> String {
        let mut prompt = String::new();
        if !req.system.trim().is_empty() {
            prompt.push_str(&req.system);
            prompt.push_str("\n\n");
        }
        for message in &req.messages {
            prompt.push_str(&message.content);
            prompt.push('\n');
        }
        prompt
    }
}

#[async_trait]
impl Provider for CliProvider {
    fn id(&self) -> &str {
        self.spec.id
    }

    fn caps(&self) -> &Capabilities {
        &self.caps
    }

    async fn complete(&self, req: CompletionRequest) -> Result<DeltaStream> {
        self.spec
            .prompt_args
            .ok_or_else(|| self.error("vendor has no prompt recipe".to_owned()))?;
        let prompt = Self::render_prompt(&req);

        // Written before the argv that names it, because the argv *is* the path.
        let prompt_file = if self.spec.prompt_via_file() {
            Some(PromptFile::write(self.spec, &prompt)?)
        } else {
            None
        };

        // `{prompt}` is substituted as a single argv element — never interpolated into
        // a shell line, so untrusted text cannot change how the process is invoked. A
        // backend with `prompt_via_stdin` keeps the prompt out of argv altogether, and one
        // with `{prompt_file}` puts only a path there.
        let argv = Self::argv_for_request(
            self.spec,
            &req,
            &prompt,
            prompt_file
                .as_ref()
                .map_or_else(|| Path::new(STAND_IN_PROMPT_FILE), PromptFile::path),
        );

        let workspace = match req.workspace.as_deref() {
            Some(raw) => {
                let canonical = std::fs::canonicalize(raw)
                    .map_err(|error| self.error(format!("workspace is unavailable: {error}")))?;
                if !canonical.is_dir() {
                    return Err(self.error("workspace is not a directory".to_owned()));
                }
                Some(canonical)
            }
            None => None,
        };

        let mut command = self.resolved.command_in(workspace.as_deref());
        command.args(&argv);
        if self.spec.id == "grok" {
            // User-level MCP servers (npx remotion, watchfiwn) otherwise start on every
            // chat turn and can sit silent past the idle timeout.
            command.env("GROK_CLAUDE_MCPS_ENABLED", "0");
            command.env("GROK_CURSOR_MCPS_ENABLED", "0");
            command.env("GROK_MCP_STARTUP_TIMEOUT_SECS", "1");
        }
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        // Closed stdin is the default because a vendor that reads it while nobody writes
        // waits forever. The exception is a backend whose print mode *is* stdin.
        if self.spec.prompt_via_stdin {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        // Killing the child when the handle drops is what stops a stopped turn from
        // leaving a vendor process running against the user's quota.
        command.kill_on_drop(true);

        let mut child = command
            .spawn()
            .map_err(|error| self.error(spawn_reason(error)))?;
        let Some(stdout) = child.stdout.take() else {
            return Err(self.error("the CLI gave no output pipe".to_owned()));
        };
        let stderr = child.stderr.take();

        // A small buffer, deliberately: back-pressure here means a fast vendor cannot
        // outrun the UI and pile a whole answer into memory ahead of the renderer.
        let (tx, rx) = mpsc::channel::<Result<Delta>>(64);
        let spec = self.spec;
        let idle_budget = IDLE_TIMEOUT.max(req.timeout);

        if spec.prompt_via_stdin {
            let Some(sink) = child.stdin.take() else {
                return Err(self.error("the CLI gave no input pipe".to_owned()));
            };
            // Its own task, not inline: a prompt larger than the pipe buffer (64 KB on
            // Windows, and an engineered turn reaches that) blocks the writer until the
            // child drains it, and the child only drains while something reads its
            // stdout. Writing here would deadlock the two against each other.
            let bytes = if spec.id == "antigravity" {
                antigravity_user_event(&prompt)
            } else {
                prompt.into_bytes()
            };
            let failures = tx.clone();
            tokio::spawn(async move {
                let mut sink = sink;
                let mut outcome = sink.write_all(&bytes).await;
                if outcome.is_ok() {
                    outcome = sink.flush().await;
                }
                // Closing the pipe is the end-of-prompt signal; without it the CLI waits
                // for more input and the turn hangs until the idle timeout.
                drop(sink);
                if let Err(error) = outcome {
                    // A child that has already exited leaves a broken pipe behind. Its
                    // own exit status and stderr say why, and that is the better answer
                    // than "we could not finish writing to it".
                    if error.kind() == std::io::ErrorKind::BrokenPipe {
                        return;
                    }
                    let _ignored = failures
                        .send(Err(provider_error(
                            spec,
                            format!("could not send the prompt to it: {error}"),
                        )))
                        .await;
                }
            });
        }

        tokio::spawn(async move {
            // Moved in, not dropped at the end of `complete`: the vendor opens the file
            // itself, some milliseconds after the spawn returns. Held here, it is removed
            // when this task ends — and this task ends on every path the turn has,
            // including the early `return` that kills a stopped child.
            let _prompt_file = prompt_file;
            // stderr is drained concurrently — a full stderr pipe deadlocks a child that
            // is still trying to write to it, which looks exactly like a hang.
            let mut stderr_task = tokio::spawn(async move {
                let mut tail: Vec<String> = Vec::new();
                let Some(stderr) = stderr else {
                    return tail;
                };
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if tail.len() == STDERR_TAIL {
                        tail.remove(0);
                    }
                    tail.push(line);
                }
                tail
            });

            let mut reader = transcript::Reader::new(spec.transcript);
            let mut lines = BufReader::new(stdout).lines();
            let mut failure: Option<String> = None;
            let started = tokio::time::Instant::now();

            loop {
                let remaining = HARD_TIMEOUT.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    failure = Some(format!(
                        "ran for over {} minutes without finishing",
                        HARD_TIMEOUT.as_secs() / 60
                    ));
                    break;
                }
                let next =
                    tokio::time::timeout(idle_budget.min(remaining), lines.next_line()).await;
                let line = match next {
                    Err(_elapsed) => {
                        failure = Some(format!(
                            "timed out after {}s with no output",
                            idle_budget.as_secs()
                        ));
                        break;
                    }
                    Ok(Err(error)) => {
                        failure = Some(format!("could not read its output: {error}"));
                        break;
                    }
                    Ok(Ok(None)) => break,
                    Ok(Ok(Some(line))) => line,
                };

                for event in reader.push_line(&line) {
                    if let Some(reason) = forward(&tx, event).await {
                        failure = Some(reason);
                    }
                    if tx.is_closed() {
                        break;
                    }
                }
                if tx.is_closed() {
                    // The receiver went away: the turn was stopped. Kill rather than
                    // keep reading a process nobody is listening to.
                    let _ignored = child.start_kill();
                    return;
                }
            }

            for event in reader.finish() {
                if let Some(reason) = forward(&tx, event).await {
                    failure = Some(reason);
                }
            }

            let spoke = reader.spoke();
            let status = match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
                Ok(Ok(status)) => Some(status),
                _ => {
                    let _ignored = child.start_kill();
                    None
                }
            };
            let stderr_tail =
                match tokio::time::timeout(Duration::from_secs(2), &mut stderr_task).await {
                    Ok(result) => result.unwrap_or_default().join(" · "),
                    Err(_) => {
                        stderr_task.abort();
                        String::new()
                    }
                };
            let diag_detail = if !stderr_tail.is_empty() {
                stderr_tail
            } else {
                reader.diagnostic_tail().unwrap_or_default()
            };

            // Partial deltas remain visible, but never turn a failed exit into success.
            let reason = if let Some(said) = failure {
                Some(said)
            } else if status.is_some_and(|status| !status.success()) {
                let code = status.map_or_else(|| "an error".to_owned(), |s| s.to_string());
                Some(if diag_detail.is_empty() {
                    format!("exited with {code}")
                } else {
                    format!("exited with {code}: {diag_detail}")
                })
            } else if status.is_none() {
                Some("timed out waiting for the CLI to exit".to_owned())
            } else if spoke {
                None
            } else {
                // An exit-0 run with nothing to show is almost always a signed-out or
                // rate-limited vendor, so say that rather than blaming the install.
                Some(if diag_detail.is_empty() {
                    "the CLI answered with nothing".to_owned()
                } else {
                    format!("the CLI answered with nothing: {diag_detail}")
                })
            };

            match reason {
                Some(reason) => {
                    let _ignored = tx.send(Err(provider_error(spec, reason))).await;
                }
                None => {
                    let _ignored = tx
                        .send(Ok(Delta::Done {
                            stop_reason: StopReason::Completed,
                        }))
                        .await;
                }
            }
        });

        // `unfold` over the receiver keeps this to the futures crate the workspace
        // already carries, rather than adding tokio-stream for one adapter.
        Ok(futures_util::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        })
        .boxed())
    }

    async fn health(&self) -> Health {
        if self.resolved.target_exists() {
            Health::Healthy { latency_ms: 0 }
        } else {
            Health::Unavailable {
                reason: "launcher missing".to_owned(),
            }
        }
    }

    fn offline_capable(&self) -> bool {
        false
    }
}

/// Sends one transcript event on as a delta. Returns the vendor's failure text when the
/// event *was* a failure, so the caller can prefer it over an exit code.
async fn forward(tx: &mpsc::Sender<Result<Delta>>, event: TranscriptEvent) -> Option<String> {
    let delta = match event {
        TranscriptEvent::Text(delta) => Delta::Text { delta },
        TranscriptEvent::Thought(delta) => Delta::Thinking { delta },
        TranscriptEvent::Usage(counts) => Delta::Usage {
            input_tokens: counts.input,
            output_tokens: counts.output,
        },
        TranscriptEvent::Tool {
            id,
            kind,
            title,
            detail,
            paths,
            done,
        } => Delta::Step {
            id,
            verb: kind.verb().to_ascii_lowercase(),
            title,
            detail,
            paths,
            done,
        },
        TranscriptEvent::Limit(report) => Delta::Limit {
            status: report.status,
            session_used: report.session.map(|window| window.utilization),
            session_resets_at: report.session.and_then(|window| window.resets_at),
            weekly_used: report.weekly.map(|window| window.utilization),
            weekly_resets_at: report.weekly.and_then(|window| window.resets_at),
        },
        TranscriptEvent::Failure(reason) => return Some(reason),
    };
    let _ignored = tx.send(Ok(delta)).await;
    None
}

/// Turns a vendor's own failure text into the next thing the user can actually do (R1).
///
/// Kept as the public entry point it always was; the classification behind it now lives
/// in [`crate::fault`], where each distinct failure is pinned by its own test.
#[must_use]
pub fn hint_for(spec: &ProviderSpec, reason: &str) -> String {
    fault::hint_for(spec, reason)
}

/// Names the failure a vendor's text describes.
#[must_use]
pub fn fault_of(reason: &str) -> FaultKind {
    fault::classify(reason)
}

/// Splits output into word-boundary chunks. Retained for the non-streaming backends and
/// for tests; the CLI path no longer needs it, because it streams what the vendor sends.
#[must_use]
pub fn chunk_for_streaming(text: &str) -> Vec<String> {
    const TARGET: usize = 48;
    let mut chunks = Vec::new();
    let mut current = String::new();
    for word in text.split_inclusive(' ') {
        current.push_str(word);
        if current.len() >= TARGET {
            chunks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::{
        chunk_for_streaming, hint_for, model_flag_args, normalize_claude_model,
        normalize_opencode_model, CliProvider, PromptFile, PROMPT_FILE, STAND_IN_PROMPT_FILE,
    };
    use crate::fault::FaultKind;
    use crate::model::{CompletionRequest, Message};
    use crate::provider::Provider;
    use bhippi_types::TaskClass;
    use std::path::Path;

    fn claude() -> &'static crate::catalog::ProviderSpec {
        crate::spec("claude").unwrap_or_else(|| panic!("the catalogue must know Claude Code"))
    }

    /// The real failure text seen from each vendor must route to advice that fixes it.
    #[test]
    fn grok_computer_use_sends_one_permission_mode() {
        let spec = crate::spec("grok").expect("Grok recipe");
        let request =
            CompletionRequest::new(TaskClass::Expander, "test", vec![]).for_computer_use();
        let argv =
            CliProvider::argv_for_request(spec, &request, "test", Path::new(STAND_IN_PROMPT_FILE));
        assert_eq!(
            argv.iter()
                .filter(|arg| *arg == "--permission-mode")
                .count(),
            1
        );
        assert!(argv.windows(2).any(|args| args == ["--tools", "read_file"]));
    }

    #[test]
    fn codex_saved_display_labels_are_migrated_at_the_cli_boundary() {
        let spec = crate::spec("codex").expect("Codex recipe");
        for (saved, expected) in [
            ("GPT-6 Astra", "gpt-6-astra"),
            ("GPT-5.6 Sol", "gpt-5.6-sol"),
            ("gpt-5.3-codex-spark", "gpt-5.3-codex-spark"),
            ("custom/model", "custom/model"),
        ] {
            let args = CliProvider::argv_for(spec, "test", Some(saved));
            let index = args.iter().position(|arg| arg == "-m").expect("model flag");
            assert_eq!(args[index + 1], expected);
        }
    }

    #[test]
    fn a_hint_names_the_fix_for_the_failure_the_vendor_reported() {
        let grok = crate::spec("grok").unwrap_or_else(|| panic!("catalogue must know Grok"));
        let out_of_credit = hint_for(
            grok,
            "API error (status 402 Payment Required): Grok Build usage balance exhausted",
        );
        assert!(
            out_of_credit.contains("top the account up"),
            "{out_of_credit}"
        );
        assert!(!out_of_credit.contains("reinstall"), "{out_of_credit}");

        let signed_out = hint_for(claude(), "Error: not logged in");
        assert!(signed_out.contains("claude login"), "{signed_out}");

        let throttled = hint_for(claude(), "429 Too Many Requests");
        assert!(throttled.contains("Wait"), "{throttled}");

        // The two failures the old hint table could not tell apart at all.
        assert_eq!(
            super::fault_of("prompt is too long: 213000 tokens > 200000 maximum"),
            FaultKind::ContextExceeded
        );
        assert_eq!(
            super::fault_of("You have reached your weekly limit"),
            FaultKind::RateLimitedWeekly
        );
    }

    #[test]
    fn antigravity_headless_recipe_reads_stdin_not_the_tui() {
        let Some(agy) = crate::spec("antigravity") else {
            panic!("catalogue must know Antigravity");
        };
        let argv = CliProvider::argv_for(agy, "hello\n--not-a-flag", Some("gemini-3.8-flash-high"));
        assert!(argv.contains(&"--input-format".to_owned()));
        assert!(argv.contains(&"stream-json".to_owned()));
        assert!(argv.contains(&"--dangerously-skip-permissions".to_owned()));
        assert!(
            argv.windows(2)
                .any(|pair| pair == ["--model", "gemini-3.8-flash-high"]),
            "{argv:?}"
        );
        assert!(
            !argv
                .iter()
                .any(|arg| arg.contains("hello") || arg.contains("not-a-flag")),
            "the prompt must stay off argv: {argv:?}"
        );
        let wrapped = String::from_utf8(super::antigravity_user_event("hello\n--not-a-flag"))
            .unwrap_or_default();
        assert!(wrapped.contains("\"event\":\"user\""));
        assert!(wrapped.contains("hello\\n--not-a-flag") || wrapped.contains("hello"));
    }

    #[test]
    fn grok_headless_recipe_does_not_open_the_tui() {
        let Some(grok) = crate::spec("grok") else {
            panic!("catalogue must know Grok");
        };
        let argv = CliProvider::argv_for(grok, "hello", None);
        // `--prompt-file` rather than `-p`: same headless single-turn mode, but the turn
        // arrives by name so it cannot overflow the command line (GROK-001).
        assert_eq!(argv.first().map(String::as_str), Some("--prompt-file"));
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--output-format", "streaming-json"]));
        assert!(argv.iter().any(|arg| arg == "--no-leader"));
        assert!(argv.iter().any(|arg| arg == "--always-approve"));
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "dontAsk"]));
        assert!(!argv.iter().any(|arg| arg == "dashboard"));
        assert!(!argv.windows(2).any(|pair| pair == ["--max-turns", "0"]));
    }

    #[test]
    fn a_chosen_model_is_pinned_with_the_vendor_flag() {
        let argv = CliProvider::argv_for(claude(), "hello", Some("sonnet"));
        let print_at = argv
            .iter()
            .position(|arg| arg == "-p")
            .unwrap_or_else(|| panic!("claude lost -p: {argv:?}"));
        assert_eq!(print_at, 0, "-p must stay the first argument: {argv:?}");
        assert!(
            argv.windows(2).any(|pair| pair == ["--model", "sonnet"]),
            "{argv:?}"
        );
        let model_at = argv
            .iter()
            .position(|arg| arg == "--model")
            .unwrap_or_else(|| panic!("claude lost --model: {argv:?}"));
        assert!(
            model_at > print_at,
            "the stdin recipe pins the model after -p: {argv:?}"
        );
    }

    /// The bug this whole path exists for.
    ///
    /// An engineered turn is tens of kilobytes containing lines that begin with `--` and
    /// words in quotes. Sent as an argv element it reached the CLI through npm's Windows
    /// launcher, which re-split it — and Claude Code answered `unknown option '--→ · ##'`
    /// on a line from the middle of the prompt. Nothing that came from the prompt may
    /// appear in argv at all.
    #[test]
    fn claudes_prompt_never_appears_in_argv() {
        const PROMPT: &str = "You are an engine.\n--not-a-flag \"quoted\"\n\nBuild a game.";
        let argv = CliProvider::argv_for(claude(), PROMPT, Some("haiku"));
        assert_eq!(
            argv,
            vec![
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages",
                "--strict-mcp-config",
                "--model",
                "haiku",
            ]
        );
        assert!(
            !argv.iter().any(|arg| arg.contains("not-a-flag")),
            "{argv:?}"
        );
    }

    #[test]
    fn vendor_effort_flags_follow_the_composer_speed() {
        let mut request = CompletionRequest::new(
            TaskClass::Expander,
            "",
            vec![Message::user("inspect".to_owned())],
        );
        request.reasoning_effort = Some("max".to_owned());
        let claude_argv: Vec<String> = CliProvider::argv_for_request(
            claude(),
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        assert!(
            claude_argv
                .windows(2)
                .any(|pair| pair == ["--effort", "max"]),
            "Claude must receive --effort max: {claude_argv:?}"
        );

        let grok = crate::spec("grok").unwrap_or_else(|| panic!("catalogue must know Grok"));
        let grok_argv: Vec<String> = CliProvider::argv_for_request(
            grok,
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        assert!(
            grok_argv
                .windows(2)
                .any(|pair| pair == ["--reasoning-effort", "xhigh"]),
            "Grok has no max; Ultracode must send xhigh: {grok_argv:?}"
        );

        let agy =
            crate::spec("antigravity").unwrap_or_else(|| panic!("catalogue must know Antigravity"));
        let agy_argv: Vec<String> = CliProvider::argv_for_request(
            agy,
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        assert!(
            agy_argv.windows(2).any(|pair| pair == ["--effort", "high"]),
            "Antigravity only has low/medium/high; Ultracode must send high: {agy_argv:?}"
        );

        request.model = Some("gemini-3.8-flash-low".to_owned());
        let agy_low: Vec<String> = CliProvider::argv_for_request(
            agy,
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        assert!(
            agy_low
                .windows(2)
                .any(|pair| pair == ["--model", "gemini-3.8-flash-low"]),
            "the Low slug must be pinned: {agy_low:?}"
        );
        assert!(
            agy_low.windows(2).any(|pair| pair == ["--effort", "low"]),
            "a Low slug must not go out with --effort high: {agy_low:?}"
        );
        assert!(
            !agy_argv.iter().any(|arg| arg.contains("inspect")),
            "Antigravity must not put the prompt in argv: {agy_argv:?}"
        );

        // Friendly name "Gemini 3.8 Flash" with "high" effort must normalize to canonical slug
        request.model = Some("Gemini 3.8 Flash".to_owned());
        request.reasoning_effort = Some("high".to_owned());
        let agy_friendly: Vec<String> = CliProvider::argv_for_request(
            agy,
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        assert!(
            agy_friendly
                .windows(2)
                .any(|pair| pair == ["--model", "gemini-3.8-flash-high"]),
            "Gemini 3.8 Flash must normalize to gemini-3.8-flash-high: {agy_friendly:?}"
        );
        assert!(
            agy_friendly
                .windows(2)
                .any(|pair| pair == ["--effort", "high"]),
            "Gemini 3.8 Flash High must emit --effort high: {agy_friendly:?}"
        );

        // Claude in Antigravity does not support --effort
        request.model = Some("Claude Sonnet 4.6".to_owned());
        let agy_claude: Vec<String> = CliProvider::argv_for_request(
            agy,
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        assert!(
            agy_claude
                .windows(2)
                .any(|pair| pair == ["--model", "claude-sonnet-4-6"]),
            "Claude Sonnet 4.6 must normalize to claude-sonnet-4-6: {agy_claude:?}"
        );
        assert!(
            !agy_claude.iter().any(|arg| arg == "--effort"),
            "Claude in Antigravity must never receive --effort: {agy_claude:?}"
        );

        request.model = None;
        request.reasoning_effort = Some("max".to_owned());
        let codex = crate::spec("codex").unwrap_or_else(|| panic!("catalogue must know Codex"));
        let codex_argv: Vec<String> = CliProvider::argv_for_request(
            codex,
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        assert!(
            codex_argv
                .iter()
                .any(|arg| arg == "model_reasoning_effort=xhigh"),
            "Codex must receive model_reasoning_effort=xhigh: {codex_argv:?}"
        );
    }

    /// `attaches_images` must describe what the argv actually does (ADR-0059).
    ///
    /// The Computer Use observation asks this to decide whether to tell the model, in
    /// words, to open the screenshot file. If the two ever disagree, a backend that only
    /// gets a path is told the image is already in hand — which is a turn spent answering
    /// from the text with the screen never looked at.
    #[test]
    fn only_the_backend_with_an_image_flag_is_said_to_attach_images() {
        let request = CompletionRequest::new(
            TaskClass::Expander,
            "",
            vec![Message::user("look at the screen".to_owned())],
        )
        .with_images(vec!["C:/temp/bhippi-computer-use/turn.jpg".to_owned()])
        .for_computer_use();

        for id in ["claude", "codex", "grok", "antigravity"] {
            let spec = crate::spec(id).unwrap_or_else(|| panic!("{id} missing from the catalogue"));
            let argv: Vec<String> = super::computer_use_args(spec, &request)
                .into_iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();
            let has_image_flag = argv.iter().any(|arg| arg == "--image");
            assert_eq!(
                has_image_flag,
                crate::attaches_images(id),
                "{id}: attaches_images disagrees with its own argv: {argv:?}"
            );
        }
    }

    /// Blender over MCP (SPA-202): Claude gets the config file and the allow-list, Codex
    /// gets overrides, and a backend that cannot host a server gets nothing at all.
    #[test]
    fn mcp_servers_ride_as_claude_config_and_codex_overrides() {
        let servers = vec![crate::model::McpServer {
            name: "blender".to_owned(),
            command: "uvx".to_owned(),
            args: vec!["blender-mcp".to_owned()],
            env: Vec::new(),
        }];
        let request = CompletionRequest::new(
            TaskClass::Expander,
            "",
            vec![Message::user("build a lamp".to_owned())],
        )
        .with_mcp_servers(servers);
        let argv_of = |id: &str| -> Vec<String> {
            let spec = crate::catalog::CATALOG
                .iter()
                .find(|entry| entry.id == id)
                .unwrap_or_else(|| panic!("{id} missing from the catalogue"));
            CliProvider::argv_for_request(
                spec,
                &request,
                "build a lamp",
                Path::new(STAND_IN_PROMPT_FILE),
            )
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
        };
        let claude = argv_of("claude");
        let config_at = claude
            .iter()
            .position(|arg| arg == "--mcp-config")
            .unwrap_or_else(|| panic!("claude carries --mcp-config: {claude:?}"));
        assert!(claude[config_at + 1].ends_with(".json"));
        assert!(
            claude.contains(&"--strict-mcp-config".to_owned()),
            "strict stays on"
        );
        assert!(claude
            .windows(2)
            .any(|pair| pair == ["--allowedTools", "mcp__blender"]));
        let codex = argv_of("codex");
        assert!(codex
            .windows(2)
            .any(|pair| pair == ["-c", "mcp_servers.blender.command=\"uvx\""]));
        assert!(codex
            .windows(2)
            .any(|pair| pair == ["-c", "mcp_servers.blender.args=[\"blender-mcp\"]"]));
        let opencode = argv_of("opencode");
        assert!(
            !opencode.iter().any(|arg| arg.contains("mcp")),
            "{opencode:?}"
        );
    }

    /// The route is per-backend, and flipping one for everyone would silently drop the
    /// prompt of every vendor that does not take it that way.
    ///
    /// Three routes, exactly one each: argv, stdin, or a file named in argv. A backend on
    /// the file route must also leave no placeholder behind — an unsubstituted
    /// `{prompt_file}` would be passed to the vendor as a literal filename.
    #[test]
    fn every_backend_carries_its_prompt_by_exactly_one_route() {
        const PROMPT: &str = "carry-me";
        for entry in crate::catalog::CATALOG {
            if entry.prompt_args.is_none() {
                continue;
            }
            let argv = CliProvider::argv_for(entry, PROMPT, None);
            let in_argv = argv.iter().any(|arg| arg == PROMPT);
            let by_file = entry.prompt_via_file();
            let routes =
                usize::from(in_argv) + usize::from(entry.prompt_via_stdin) + usize::from(by_file);
            assert_eq!(
                routes, 1,
                "{} sends its prompt by {routes} routes, not one: {argv:?}",
                entry.id
            );
            assert!(
                !argv.iter().any(|arg| arg.contains(PROMPT_FILE)),
                "{} left the placeholder unsubstituted: {argv:?}",
                entry.id
            );
            if by_file {
                assert!(
                    argv.iter().any(|arg| arg == STAND_IN_PROMPT_FILE),
                    "{} names no prompt file: {argv:?}",
                    entry.id
                );
            }
        }
    }

    /// GROK-001. The bug the file route exists for: an engineered turn is 30–60 KB, a
    /// Windows command line is capped at 32,767 characters, and `grok -p <PROMPT>` blew
    /// through it — `CreateProcess` failed with os error 206 and the user was told "provider
    /// Grok CLI unavailable", which named nothing that was actually wrong.
    ///
    /// So: no argv element may carry the turn, and the whole line stays far inside the cap
    /// however long the turn is.
    #[test]
    fn grok_keeps_a_huge_turn_off_the_command_line() {
        // Comfortably past the Windows limit, and about the size of a real engineered turn.
        let turn = "context line that a real turn is full of
"
        .repeat(1_200);
        assert!(turn.len() > 32_767, "the test prompt must exceed the cap");

        let Some(grok) = crate::spec("grok") else {
            panic!("the catalogue must know Grok");
        };
        let argv = CliProvider::argv_for(grok, &turn, Some("grok-4.6"));

        assert!(
            argv.windows(2)
                .any(|pair| pair[0] == "--prompt-file" && pair[1] == STAND_IN_PROMPT_FILE),
            "the prompt must travel as a named file: {argv:?}"
        );
        assert!(
            !argv.iter().any(|arg| arg.contains("context line")),
            "no argv element may carry the turn itself"
        );
        let line: usize = argv.iter().map(|arg| arg.len() + 3).sum();
        assert!(
            line < 8_192,
            "the command line grew to {line} characters: {argv:?}"
        );
    }

    /// The residual case: a recipe that still puts a turn in argv fails at `CreateProcess`,
    /// and the errno alone sends the reader to the install and the PATH, which are fine.
    #[test]
    fn an_overlong_command_line_is_reported_as_an_overlong_command_line() {
        let reason = super::spawn_reason(std::io::Error::from_raw_os_error(206));
        assert!(
            reason.contains("too long for a Windows command line"),
            "{reason}"
        );
        assert!(!reason.contains("could not start it"), "{reason}");

        let missing = super::spawn_reason(std::io::Error::from_raw_os_error(2));
        assert!(missing.contains("could not start it"), "{missing}");
    }

    /// The file has to exist, hold the turn byte for byte — `--verbatim` promises the
    /// vendor sends it as given — and be gone once the value that owns it is dropped.
    #[test]
    fn a_prompt_file_holds_the_turn_and_is_removed_with_its_turn() {
        let Some(grok) = crate::spec("grok") else {
            panic!("the catalogue must know Grok");
        };
        let turn = "line one
line two — with a — dash and \"quotes\"
"
        .repeat(500);
        let Ok(file) = PromptFile::write(grok, &turn) else {
            panic!("a prompt file must be writable");
        };
        let path = file.path().to_path_buf();
        assert_eq!(
            std::fs::read_to_string(&path).ok().as_deref(),
            Some(turn.as_str()),
            "the file must hold the turn verbatim"
        );
        drop(file);
        assert!(
            !path.exists(),
            "a finished turn must leave no prompt behind"
        );
    }

    /// Two turns of one chat can overlap, and the second must not rewrite the first one's
    /// prompt out from under a vendor that is still reading it.
    #[test]
    fn overlapping_turns_get_their_own_prompt_files() {
        let Some(grok) = crate::spec("grok") else {
            panic!("the catalogue must know Grok");
        };
        let (Ok(first), Ok(second)) = (
            PromptFile::write(grok, "one"),
            PromptFile::write(grok, "two"),
        ) else {
            panic!("prompt files must be writable");
        };
        assert_ne!(first.path(), second.path());
        assert_eq!(
            std::fs::read_to_string(first.path()).ok().as_deref(),
            Some("one")
        );
        assert_eq!(
            std::fs::read_to_string(second.path()).ok().as_deref(),
            Some("two")
        );
    }

    #[test]
    fn codex_model_flag_lands_inside_exec_before_the_prompt() {
        let Some(codex) = crate::spec("codex") else {
            panic!("the catalogue must know Codex");
        };
        let argv = CliProvider::argv_for(codex, "inspect", Some("gpt-5.4"));
        assert_eq!(argv.first().map(String::as_str), Some("exec"), "{argv:?}");
        assert!(codex.prompt_via_stdin);
        assert!(!argv.iter().any(|arg| arg == "inspect"));
        let model_at = argv
            .iter()
            .position(|arg| arg == "-m")
            .unwrap_or_else(|| panic!("codex lost -m: {argv:?}"));
        assert!(
            model_at > 0,
            "Codex treats tokens after the prompt as prompt text: {argv:?}"
        );
        assert_eq!(argv.get(model_at + 1).map(String::as_str), Some("gpt-5.4"));
    }

    #[test]
    fn no_choice_sends_no_model_flag_at_all() {
        for model in [None, Some(""), Some("   ")] {
            let argv = CliProvider::argv_for(claude(), "hello", model);
            assert!(!argv.iter().any(|arg| arg == "--model"), "{argv:?}");
        }
    }

    #[test]
    fn computer_use_attaches_codex_images_and_forces_read_only_execution() {
        let Some(codex) = crate::spec("codex") else {
            panic!("the catalogue must know Codex");
        };
        let request = CompletionRequest::new(
            TaskClass::Expander,
            "system",
            vec![Message::user("inspect".to_owned())],
        )
        .with_images(vec![r"C:\Temp\desktop.jpg".to_owned()])
        .for_computer_use();
        let argv = CliProvider::argv_for_request(
            codex,
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        );
        let argv: Vec<String> = argv
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--image", r"C:\Temp\desktop.jpg"]));
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--sandbox", "read-only"]));
        assert!(argv.iter().any(|arg| arg == "--ephemeral"));
    }

    #[test]
    fn computer_use_restricts_claude_to_reading_the_screenshot() {
        let request = CompletionRequest::new(
            TaskClass::Expander,
            "system",
            vec![Message::user("inspect".to_owned())],
        )
        .with_images(vec![r"C:\Temp\desktop.jpg".to_owned()])
        .for_computer_use();
        let argv = CliProvider::argv_for_request(
            claude(),
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        );
        let argv: Vec<String> = argv
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(argv.windows(2).any(|pair| pair == ["--tools", "Read"]));
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--permission-mode", "dontAsk"]));
        assert!(!argv.iter().any(|arg| arg.eq_ignore_ascii_case("bash")));
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--add-dir", r"C:\Temp"]));
        assert!(!argv.iter().any(|arg| arg == "inspect"), "{argv:?}");
        // Everything spliced in has to sit behind `-p`; the prompt itself arrives on
        // stdin, so there is nothing in front of it for `--add-dir` to swallow.
        let print_at = argv
            .iter()
            .position(|arg| arg == "-p")
            .unwrap_or_else(|| panic!("claude lost -p: {argv:?}"));
        for flag in ["--permission-mode", "--tools", "--add-dir"] {
            let at = argv
                .iter()
                .position(|arg| arg == flag)
                .unwrap_or_else(|| panic!("claude lost {flag}: {argv:?}"));
            assert!(at > print_at, "{flag} landed before -p: {argv:?}");
        }
    }

    /// An ordinary turn with an attached photo, which is the composer's `+` menu, not
    /// Computer Use.
    ///
    /// Claude Code's Read tool refuses any path outside its working directory, so a photo
    /// picked from Pictures is unreadable to it without `--add-dir` on that folder — the
    /// user attaches an image and the agent answers that it cannot see it. The flag used
    /// to be added only under `computer_use`, which is why this needed fixing at all.
    #[test]
    fn an_attached_image_unlocks_its_folder_without_granting_computer_use() {
        let request = CompletionRequest::new(
            TaskClass::Expander,
            "system",
            vec![Message::user("what is wrong with this sprite?".to_owned())],
        )
        .with_images(vec![
            r"C:\Users\me\Pictures\sprite.png".to_owned(),
            r"C:\Users\me\Pictures\other.png".to_owned(),
            r"C:\Work\game\icon.png".to_owned(),
        ]);
        assert!(!request.computer_use, "this is a plain chat turn");
        let argv: Vec<String> = CliProvider::argv_for_request(
            claude(),
            &request,
            "prompt",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

        // One `--add-dir` per distinct parent, not one per file.
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--add-dir", r"C:\Users\me\Pictures"]));
        assert!(argv
            .windows(2)
            .any(|pair| pair == ["--add-dir", r"C:\Work\game"]));
        assert_eq!(
            argv.iter().filter(|arg| *arg == "--add-dir").count(),
            2,
            "the two files in one folder share its flag: {argv:?}"
        );

        // Attaching a file grants a directory and nothing else. `--permission-mode` and
        // `--tools` narrow a *desktop* turn; adding them here would silently change how
        // every ordinary turn with a photo in it is allowed to behave.
        assert!(
            !argv.iter().any(|arg| arg == "--permission-mode"),
            "{argv:?}"
        );
        assert!(!argv.iter().any(|arg| arg == "--tools"), "{argv:?}");

        // The stdin splice rules still hold: everything sits behind `-p`, and nothing
        // that came from the prompt is in argv for `--add-dir` to swallow.
        let print_at = argv
            .iter()
            .position(|arg| arg == "-p")
            .unwrap_or_else(|| panic!("claude lost -p: {argv:?}"));
        let add_at = argv
            .iter()
            .position(|arg| arg == "--add-dir")
            .unwrap_or_else(|| panic!("claude lost --add-dir: {argv:?}"));
        assert!(add_at > print_at, "--add-dir landed before -p: {argv:?}");
        assert!(!argv.iter().any(|arg| arg == "prompt"), "{argv:?}");
    }

    /// The flag is Claude's alone, and only when something was actually attached.
    #[test]
    fn no_attachment_and_no_claude_means_no_extra_flags_at_all() {
        let bare = CompletionRequest::new(
            TaskClass::Expander,
            "system",
            vec![Message::user("hello".to_owned())],
        );
        let argv = CliProvider::argv_for(claude(), "hello", None);
        let via_request: Vec<String> = CliProvider::argv_for_request(
            claude(),
            &bare,
            "hello",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        assert_eq!(argv, via_request, "an attachment-free turn is unchanged");
        assert!(
            !via_request.iter().any(|arg| arg == "--add-dir"),
            "{via_request:?}"
        );

        // Codex takes images through its own `--image` flag under Computer Use and has no
        // `--add-dir` at all; an attachment must not invent one for it.
        let Some(codex) = crate::spec("codex") else {
            panic!("the catalogue must know Codex");
        };
        let with_image = CompletionRequest::new(
            TaskClass::Expander,
            "system",
            vec![Message::user("look".to_owned())],
        )
        .with_images(vec![r"C:\Temp\shot.png".to_owned()]);
        let codex_argv: Vec<String> = CliProvider::argv_for_request(
            codex,
            &with_image,
            "look",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        assert!(
            !codex_argv.iter().any(|arg| arg == "--add-dir"),
            "{codex_argv:?}"
        );
        assert_eq!(
            codex_argv,
            CliProvider::argv_for(codex, "look", None),
            "a plain Codex turn is byte-for-byte what it was"
        );
    }

    /// The regression pin for the bug that made Computer Use do nothing at all.
    ///
    /// `claude --add-dir <directories...>` and `codex --image <files...>` keep consuming
    /// arguments until one starts with `-`. The Computer Use fragment used to be appended
    /// immediately before the prompt, so the prompt became one more directory and Claude
    /// exited with "Input must be provided either through stdin or as a prompt argument"
    /// before a single pixel was ever inspected.
    ///
    /// Asserting the flags are *present* never caught it — they were. What matters is that
    /// adding them does not disturb what sits in front of the prompt, so this compares the
    /// argv with and without Computer Use and demands that neighbour be unchanged.
    ///
    /// Grok's argv element is the *path* of its prompt file rather than the prompt (GROK-001),
    /// and a swallowed path loses the turn exactly as a swallowed prompt does — so the thing
    /// watched here is whichever element carries the turn for that backend.
    #[test]
    fn computer_use_flags_never_displace_the_prompt() {
        const PROMPT: &str = "inspect-the-desktop";
        {
            let id = "grok";
            let Some(spec) = crate::spec(id) else {
                panic!("the catalogue must know {id}");
            };
            let carrier = if spec.prompt_via_file() {
                STAND_IN_PROMPT_FILE
            } else {
                PROMPT
            };
            let plain = CompletionRequest::new(
                TaskClass::Expander,
                "system",
                vec![Message::user(PROMPT.to_owned())],
            );
            let desktop = plain
                .clone()
                .with_images(vec![r"C:\Temp\desktop.jpg".to_owned()])
                .for_computer_use();

            let render = |request: &CompletionRequest| -> Vec<String> {
                CliProvider::argv_for_request(
                    spec,
                    request,
                    PROMPT,
                    Path::new(STAND_IN_PROMPT_FILE),
                )
                .into_iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect()
            };
            let before = render(&plain);
            let after = render(&desktop);

            let neighbour = |argv: &[String]| -> Option<String> {
                let at = argv.iter().position(|arg| arg == carrier)?;
                Some(
                    at.checked_sub(1)
                        .map_or_else(|| "<start of argv>".to_owned(), |index| argv[index].clone()),
                )
            };
            assert!(
                after.iter().any(|arg| arg == carrier),
                "{id} lost the prompt entirely: {after:?}"
            );
            assert_eq!(
                neighbour(&after),
                neighbour(&before),
                "{id} moved the prompt behind a Computer Use flag, which a list-valued flag \
                 will swallow: {after:?}"
            );
        }
    }

    /// The same guarantee for the backend whose prompt is not in argv at all.
    ///
    /// There is no prompt for a list-valued flag to swallow, so what has to hold instead
    /// is that the vendor's own recipe is untouched: `-p` and its flags stay exactly
    /// where they were, and Computer Use only ever appends.
    #[test]
    fn computer_use_only_appends_to_a_stdin_backends_recipe() {
        const PROMPT: &str = "inspect-the-desktop";
        let plain = CompletionRequest::new(
            TaskClass::Expander,
            "system",
            vec![Message::user(PROMPT.to_owned())],
        );
        let desktop = plain
            .clone()
            .with_images(vec![r"C:\Temp\desktop.jpg".to_owned()])
            .for_computer_use();
        let render = |request: &CompletionRequest| -> Vec<String> {
            CliProvider::argv_for_request(
                claude(),
                request,
                PROMPT,
                Path::new(STAND_IN_PROMPT_FILE),
            )
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
        };
        let before = render(&plain);
        let after = render(&desktop);
        assert!(after.len() > before.len(), "{after:?}");
        assert_eq!(&after[..before.len()], before.as_slice(), "{after:?}");
        assert!(!after.iter().any(|arg| arg == PROMPT), "{after:?}");
    }

    /// Codex is the one authorised provider whose recipe starts with a subcommand, and the
    /// flags have to land inside it — `codex --image x exec …` is not a valid invocation.
    #[test]
    fn computer_use_flags_land_after_a_leading_subcommand() {
        let Some(codex) = crate::spec("codex") else {
            panic!("the catalogue must know Codex");
        };
        let request = CompletionRequest::new(
            TaskClass::Expander,
            "system",
            vec![Message::user("inspect".to_owned())],
        )
        .with_model(Some("gpt-5.4".to_owned()))
        .with_images(vec![r"C:\Temp\desktop.jpg".to_owned()])
        .for_computer_use();
        let argv: Vec<String> = CliProvider::argv_for_request(
            codex,
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        assert_eq!(argv.first().map(String::as_str), Some("exec"), "{argv:?}");
        assert!(
            argv.windows(2)
                .any(|pair| pair == ["--image", r"C:\Temp\desktop.jpg"]),
            "Codex must receive the screenshot as --image after exec: {argv:?}"
        );
        assert!(codex.prompt_via_stdin);
        assert!(!argv.iter().any(|arg| arg == "inspect"));
        let model_at = argv
            .iter()
            .position(|arg| arg == "-m")
            .unwrap_or_else(|| panic!("-m missing: {argv:?}"));
        assert!(
            model_at > 0,
            "model after the prompt is swallowed: {argv:?}"
        );
        let exec_at = 0_usize;
        let image_at = argv
            .iter()
            .position(|arg| arg == "--image")
            .unwrap_or_else(|| panic!("--image missing: {argv:?}"));
        assert!(image_at > exec_at, "{argv:?}");
    }

    /// Computer Use narrows Grok to one desktop decision, and it must do that without
    /// disturbing how the turn itself arrives — which since GROK-001 is `--prompt-file`
    /// and a path, not `-p` and the text.
    #[test]
    fn grok_computer_use_keeps_the_prompt_file_and_narrows_the_tools() {
        let Some(grok) = crate::spec("grok") else {
            panic!("the catalogue must know Grok");
        };
        let request = CompletionRequest::new(
            TaskClass::Expander,
            "system",
            vec![Message::user("inspect".to_owned())],
        )
        .with_images(vec![r"C:\Temp\desktop.jpg".to_owned()])
        .for_computer_use();
        let argv: Vec<String> = CliProvider::argv_for_request(
            grok,
            &request,
            "inspect",
            Path::new(STAND_IN_PROMPT_FILE),
        )
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
        let file_at = argv
            .iter()
            .position(|arg| arg == "--prompt-file")
            .unwrap_or_else(|| panic!("grok lost --prompt-file: {argv:?}"));
        assert_eq!(
            argv.get(file_at + 1).map(String::as_str),
            Some(STAND_IN_PROMPT_FILE),
            "the path must still follow the flag: {argv:?}"
        );
        assert!(
            argv.windows(2).any(|pair| pair == ["--tools", "read_file"]),
            "{argv:?}"
        );
    }

    #[test]
    fn a_model_name_is_one_argv_element_never_a_shell_fragment() {
        // A name the normaliser does not recognise is carried verbatim, which is the case
        // that proves argv is built as a list rather than pasted into a shell string.
        let clean = CliProvider::argv_for(claude(), "hello", Some("zzz-unknown"));
        let injected = CliProvider::argv_for(claude(), "hello", Some("zzz-unknown && rm -rf /"));
        assert!(injected.contains(&"zzz-unknown && rm -rf /".to_owned()));
        assert_eq!(
            injected.len(),
            clean.len(),
            "injection must not add argv elements"
        );

        // And a name it *does* recognise is replaced by the bare alias, so a suffix riding
        // on a real model name never reaches the command line at all.
        let labelled =
            CliProvider::argv_for(claude(), "hello", Some("Claude Sonnet 5 && rm -rf /"));
        assert!(labelled.contains(&"sonnet".to_owned()), "{labelled:?}");
        assert!(
            !labelled.iter().any(|arg| arg.contains("rm -rf")),
            "{labelled:?}"
        );
        assert_eq!(labelled.len(), clean.len());
    }

    #[test]
    fn chunks_split_on_word_boundaries_and_keep_everything() {
        let short = "alpha beta gamma";
        let chunks = chunk_for_streaming(short);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks.concat(), short);

        let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu ";
        let split = chunk_for_streaming(long);
        assert!(split.len() > 1, "long text must stream in pieces");
        assert_eq!(split.concat(), long);
    }

    #[test]
    fn empty_text_yields_no_chunks() {
        assert!(chunk_for_streaming("").is_empty());
    }

    /// The adapter streams for real now, and the capability has to say so — routing and
    /// the UI both read it to decide whether to expect token-by-token arrival.
    #[test]
    fn a_cli_backend_reports_itself_as_streaming() {
        let Some(provider) = CliProvider::open(claude()) else {
            // Claude Code is not installed on this machine; nothing to assert.
            return;
        };
        assert!(provider.caps().streaming);
        assert!(provider.caps().context_window >= 100_000);
    }

    /// The owner's report: *"There's an issue with the selected model (Claude Opus 5). It
    /// may not exist or you may not have access to it."* That string is a **label** from the
    /// model picker, and it reached the CLI as `--model` because nothing translated it.
    #[test]
    fn a_claude_label_becomes_an_alias_the_cli_accepts() {
        assert_eq!(normalize_claude_model("Claude Opus 5"), "opus");
        assert_eq!(normalize_claude_model("Claude Fable 5.1"), "fable");
        assert_eq!(normalize_claude_model("Claude Sonnet 5"), "sonnet");
        assert_eq!(normalize_claude_model("Claude Haiku 4.5"), "haiku");
        assert_eq!(normalize_claude_model("Claude 3.5 Sonnet"), "sonnet");

        // Anything already in a shape the CLI takes is left exactly as it is.
        assert_eq!(normalize_claude_model("opus"), "opus");
        assert_eq!(normalize_claude_model("claude-opus-4-5"), "claude-opus-4-5");
    }

    /// OpenCode takes `provider/model` and refuses anything else, so `Big Pickle` never ran.
    /// The slugs below are real ids from `opencode models` on a live install.
    #[test]
    fn an_opencode_label_becomes_a_provider_qualified_id() {
        assert_eq!(
            normalize_opencode_model("Big Pickle"),
            "opencode/big-pickle"
        );
        assert_eq!(
            normalize_opencode_model("Nemotron 3.5 Lightning Free"),
            "opencode/nemotron-3.5-lightning-free"
        );
        // A real id already carries its provider, and must survive untouched.
        assert_eq!(
            normalize_opencode_model("openrouter/qwen/qwen-2.5-72b-instruct"),
            "openrouter/qwen/qwen-2.5-72b-instruct"
        );
    }

    /// The flag has to carry the translated value, not the label, or none of the above
    /// reaches the process that matters.
    #[test]
    fn the_model_flag_carries_the_translated_id() {
        let spec = crate::catalog::CATALOG
            .iter()
            .find(|entry| entry.id == "claude")
            .expect("claude is in the catalogue");
        let mut request = CompletionRequest::new(
            TaskClass::Expander,
            "",
            vec![Message::user("hello".to_owned())],
        );
        request.model = Some("Claude Opus 5".to_owned());
        let args: Vec<String> = model_flag_args(spec, &request)
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, vec!["--model".to_owned(), "opus".to_owned()]);
    }
}
