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
//! So the child is spawned, its stdout is read a line at a time, and each line goes
//! through [`transcript::Reader`] into a `Delta` the moment it arrives. First words reach
//! the screen in about a second. The timeout is a *silence* budget rather than a wall
//! clock, because a healthy agent that has been streaming for four minutes is working,
//! not hung, and killing it at a fixed 180 s was losing real answers.

use crate::catalog::ProviderSpec;
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
            push("--permission-mode");
            push("dontAsk");
            push("--tools");
            push("read_file");
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
    if spec.id != "claude" || req.image_paths.is_empty() {
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

fn model_flag_args(spec: &ProviderSpec, model: Option<&str>) -> Vec<OsString> {
    let Some(template) = spec.model_args else {
        return Vec::new();
    };
    let Some(model) = model.map(str::trim).filter(|name| !name.is_empty()) else {
        return Vec::new();
    };
    template
        .iter()
        .map(|arg| OsString::from(arg.replace("{model}", model)))
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
    #[must_use]
    pub fn argv_for(spec: &ProviderSpec, prompt: &str, model: Option<&str>) -> Vec<String> {
        let request = CompletionRequest::new(
            TaskClass::Expander,
            "",
            vec![Message::user(prompt.to_owned())],
        )
        .with_model(model.map(str::to_owned));
        Self::argv_for_request(spec, &request, prompt)
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn argv_for_request(
        spec: &ProviderSpec,
        req: &CompletionRequest,
        prompt: &str,
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
        let model_args = model_flag_args(spec, req.model.as_deref());

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

        // `{prompt}` is substituted as a single argv element — never interpolated into
        // a shell line, so untrusted text cannot change how the process is invoked. A
        // backend with `prompt_via_stdin` keeps the prompt out of argv altogether.
        let argv = Self::argv_for_request(self.spec, &req, &prompt);

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
            .map_err(|error| self.error(format!("could not start it: {error}")))?;
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
            let bytes = prompt.into_bytes();
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
            // stderr is drained concurrently — a full stderr pipe deadlocks a child that
            // is still trying to write to it, which looks exactly like a hang.
            let stderr_task = tokio::spawn(async move {
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
            let stderr_tail = stderr_task.await.unwrap_or_default().join(" · ");

            // Precedence matters. What the vendor said in-band about its own failure is
            // always more specific than an exit code, and an exit code is more specific
            // than our "it said nothing" guess.
            let reason = if let Some(said) = failure {
                Some(said)
            } else if status.is_some_and(|status| !status.success()) {
                let code = status.map_or_else(|| "an error".to_owned(), |s| s.to_string());
                Some(if stderr_tail.is_empty() {
                    format!("exited with {code}")
                } else {
                    format!("exited with {code}: {stderr_tail}")
                })
            } else if !spoke {
                // An exit-0 run with nothing to show is almost always a signed-out or
                // rate-limited vendor, so say that rather than blaming the install.
                Some(if stderr_tail.is_empty() {
                    "the CLI answered with nothing".to_owned()
                } else {
                    format!("the CLI answered with nothing: {stderr_tail}")
                })
            } else {
                None
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
            done,
        } => Delta::Step {
            id,
            verb: kind.verb().to_ascii_lowercase(),
            title,
            detail,
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
    use super::{chunk_for_streaming, hint_for, CliProvider};
    use crate::fault::FaultKind;
    use crate::model::{CompletionRequest, Message};
    use crate::provider::Provider;
    use bhippi_types::TaskClass;

    fn claude() -> &'static crate::catalog::ProviderSpec {
        crate::spec("claude").unwrap_or_else(|| panic!("the catalogue must know Claude Code"))
    }

    /// The real failure text seen from each vendor must route to advice that fixes it.
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
    fn grok_headless_recipe_does_not_open_the_tui() {
        let Some(grok) = crate::spec("grok") else {
            panic!("catalogue must know Grok");
        };
        let argv = CliProvider::argv_for(grok, "hello", None);
        assert_eq!(argv.first().map(String::as_str), Some("-p"));
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
            CliProvider::argv_for_request(spec, &request, "build a lamp")
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

    /// The switch is per-backend, and flipping it for everyone would silently drop the
    /// prompt of every vendor that does not read stdin.
    #[test]
    fn every_argv_backend_still_carries_its_prompt_where_it_always_did() {
        const PROMPT: &str = "carry-me";
        for entry in crate::catalog::CATALOG {
            if entry.prompt_args.is_none() {
                continue;
            }
            let argv = CliProvider::argv_for(entry, PROMPT, None);
            assert_eq!(
                argv.iter().any(|arg| arg == PROMPT),
                !entry.prompt_via_stdin,
                "{} sends its prompt in the wrong channel: {argv:?}",
                entry.id
            );
        }
    }

    #[test]
    fn codex_model_flag_lands_inside_exec_before_the_prompt() {
        let Some(codex) = crate::spec("codex") else {
            panic!("the catalogue must know Codex");
        };
        let argv = CliProvider::argv_for(codex, "inspect", Some("gpt-5.4"));
        assert_eq!(argv.first().map(String::as_str), Some("exec"), "{argv:?}");
        let prompt_at = argv
            .iter()
            .position(|arg| arg == "inspect")
            .unwrap_or_else(|| panic!("codex lost the prompt: {argv:?}"));
        let model_at = argv
            .iter()
            .position(|arg| arg == "-m")
            .unwrap_or_else(|| panic!("codex lost -m: {argv:?}"));
        assert!(
            model_at > 0 && model_at < prompt_at,
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
        let argv = CliProvider::argv_for_request(codex, &request, "inspect");
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
        let argv = CliProvider::argv_for_request(claude(), &request, "inspect");
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
        let argv: Vec<String> = CliProvider::argv_for_request(claude(), &request, "prompt")
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
        let via_request: Vec<String> = CliProvider::argv_for_request(claude(), &bare, "hello")
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
        let codex_argv: Vec<String> = CliProvider::argv_for_request(codex, &with_image, "look")
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
    #[test]
    fn computer_use_flags_never_displace_the_prompt() {
        const PROMPT: &str = "inspect-the-desktop";
        for id in ["codex", "grok"] {
            let Some(spec) = crate::spec(id) else {
                panic!("the catalogue must know {id}");
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
                CliProvider::argv_for_request(spec, request, PROMPT)
                    .into_iter()
                    .map(|arg| arg.to_string_lossy().into_owned())
                    .collect()
            };
            let before = render(&plain);
            let after = render(&desktop);

            let neighbour = |argv: &[String]| -> Option<String> {
                let at = argv.iter().position(|arg| arg == PROMPT)?;
                Some(
                    at.checked_sub(1)
                        .map_or_else(|| "<start of argv>".to_owned(), |index| argv[index].clone()),
                )
            };
            assert!(
                after.iter().any(|arg| arg == PROMPT),
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
            CliProvider::argv_for_request(claude(), request, PROMPT)
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
        let argv: Vec<String> = CliProvider::argv_for_request(codex, &request, "inspect")
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(argv.first().map(String::as_str), Some("exec"), "{argv:?}");
        assert!(
            argv.windows(2)
                .any(|pair| pair == ["--image", r"C:\Temp\desktop.jpg"]),
            "Codex must receive the screenshot as --image after exec: {argv:?}"
        );
        let prompt_at = argv
            .iter()
            .position(|arg| arg == "inspect")
            .unwrap_or_else(|| panic!("prompt missing: {argv:?}"));
        let model_at = argv
            .iter()
            .position(|arg| arg == "-m")
            .unwrap_or_else(|| panic!("-m missing: {argv:?}"));
        assert!(
            model_at < prompt_at,
            "model after the prompt is swallowed: {argv:?}"
        );
        let exec_at = 0_usize;
        let image_at = argv
            .iter()
            .position(|arg| arg == "--image")
            .unwrap_or_else(|| panic!("--image missing: {argv:?}"));
        assert!(image_at > exec_at, "{argv:?}");
    }

    #[test]
    fn grok_computer_use_keeps_the_prompt_as_the_p_value() {
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
        let argv: Vec<String> = CliProvider::argv_for_request(grok, &request, "inspect")
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        let p_at = argv
            .iter()
            .position(|arg| arg == "-p")
            .unwrap_or_else(|| panic!("grok lost -p: {argv:?}"));
        assert_eq!(argv.get(p_at + 1).map(String::as_str), Some("inspect"));
        assert!(
            argv.windows(2).any(|pair| pair == ["--tools", "read_file"]),
            "{argv:?}"
        );
    }

    #[test]
    fn a_model_name_is_one_argv_element_never_a_shell_fragment() {
        let clean = CliProvider::argv_for(claude(), "hello", Some("sonnet"));
        let injected = CliProvider::argv_for(claude(), "hello", Some("sonnet && rm -rf /"));
        assert!(injected.contains(&"sonnet && rm -rf /".to_owned()));
        assert_eq!(
            injected.len(),
            clean.len(),
            "injection must not add argv elements"
        );
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
}
