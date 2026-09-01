//! Reading a vendor CLI's stdout back into one answer, **one line at a time**.
//!
//! CLIs print one of two things. The plain form *is* the answer, but every vendor
//! decorates it — a banner naming the workdir and session, an agent/model header — and
//! stripping that by pattern ages badly, because the banner changes with every release.
//! The JSON Lines form has no banner, carries real token counts, names the tools the
//! agent ran, and — the reason this file is incremental — arrives *while the agent
//! works* rather than after it exits.
//!
//! [`Reader`] is a state machine over single lines so a live process can be rendered as
//! it speaks. [`read`] remains as a whole-buffer wrapper, so the fixtures that pin each
//! vendor's shape keep testing the same code the live path uses.

use serde_json::Value;
use std::collections::HashMap;

/// The shape of one backend's stdout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Transcript {
    /// stdout is the answer, verbatim.
    Plain,
    /// stdout is JSON Lines: one event per line, answer and usage read from the events.
    JsonLines,
}

/// Tokens a turn actually spent, as the vendor reported them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TokenCounts {
    pub input: u64,
    pub output: u64,
}

/// One thing a vendor said on stdout, normalised across dialects.
#[derive(Clone, Debug, PartialEq)]
pub enum TranscriptEvent {
    /// Words for the user.
    Text(String),
    /// The model's own reasoning, kept out of the answer body.
    Thought(String),
    /// A step the agent ran. `done` is false while it is still in flight.
    Tool {
        id: String,
        kind: ToolKind,
        title: String,
        detail: String,
        done: bool,
    },
    /// Tokens the turn spent, cumulative per turn — the last report wins.
    Usage(TokenCounts),
    /// How much of the plan's rolling allowances this account has spent.
    ///
    /// Claude Code volunteers this on every turn, for both windows at once. It is the
    /// difference between telling a user their week is gone *after* a turn fails and
    /// showing them at 80 % that it is about to.
    Limit(LimitReport),
    /// The vendor reported a failure, in its own words, for classification upstream.
    Failure(String),
}

/// One rolling allowance and how much of it is gone.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Window {
    /// 0.0 – 1.0 of the window consumed.
    pub utilization: f32,
    /// Unix seconds at which this window rolls over, when the vendor named one.
    pub resets_at: Option<i64>,
}

/// A vendor's own report of where this account stands against its plan.
///
/// Two windows, not one, because they fail differently: the short window clears while
/// you make coffee and the long one clears next week. A single "you are rate limited"
/// cannot say which, and the advice for each is the opposite of the advice for the other.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LimitReport {
    /// `allowed`, `allowed_warning`, or `rejected`, in the vendor's own words.
    pub status: String,
    /// Which window is currently the binding one.
    pub binding: String,
    pub session: Option<Window>,
    pub weekly: Option<Window>,
}

impl LimitReport {
    /// The most-consumed window, which is the one worth showing.
    #[must_use]
    pub fn worst(&self) -> f32 {
        let session = self.session.map_or(0.0, |window| window.utilization);
        let weekly = self.weekly.map_or(0.0, |window| window.utilization);
        session.max(weekly)
    }

    /// True once the account is close enough that the user should be told unprompted.
    #[must_use]
    pub fn worth_warning(&self) -> bool {
        self.status != "allowed" || self.worst() >= 0.8
    }
}

/// What a vendor's tool step is *doing*, in the vocabulary the UI animates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolKind {
    Read,
    Edit,
    Write,
    Run,
    Search,
    Fetch,
    Plan,
    Test,
    Other,
}

impl ToolKind {
    /// Maps a vendor's tool name onto the shared vocabulary.
    ///
    /// Vendors name the same handful of operations a dozen ways and rename them between
    /// releases, so this matches on the words they all reach for rather than on any one
    /// vendor's exact identifiers. An unrecognised name is `Other`, never a guess.
    #[must_use]
    pub fn of(name: &str) -> Self {
        let name = name.to_ascii_lowercase();
        let has = |needles: &[&str]| needles.iter().any(|needle| name.contains(needle));
        if has(&["todo", "plan", "task"]) {
            Self::Plan
        } else if has(&["test", "pytest", "jest"]) {
            Self::Test
        } else if has(&["websearch", "search", "grep", "glob", "find"]) {
            Self::Search
        } else if has(&["webfetch", "fetch", "curl", "http"]) {
            Self::Fetch
        } else if has(&["bash", "shell", "exec", "command", "terminal", "powershell"]) {
            Self::Run
        } else if has(&["multiedit", "edit", "patch", "apply", "replace"]) {
            Self::Edit
        } else if has(&["write", "create", "notebook"]) {
            Self::Write
        } else if has(&["read", "view", "open", "cat"]) {
            Self::Read
        } else {
            Self::Other
        }
    }

    /// The verb the activity dock shows for this step.
    #[must_use]
    pub const fn verb(self) -> &'static str {
        match self {
            Self::Read => "Read",
            Self::Edit => "Edited",
            Self::Write => "Wrote",
            Self::Run => "Ran",
            Self::Search => "Searched",
            Self::Fetch => "Fetched",
            Self::Plan => "Planned",
            Self::Test => "Tested",
            Self::Other => "Used",
        }
    }
}

/// One CLI answer read back off stdout.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Answer {
    pub text: String,
    /// Present only when the transcript reported it — never estimated from the text.
    pub usage: Option<TokenCounts>,
    /// What the vendor said went wrong, when it said so on a **successful** exit.
    ///
    /// A CLI that reports its own failure in-band and still exits 0 used to read here as
    /// a successful empty answer, which surfaced to the user as "the CLI answered with
    /// nothing" — the one message that explains none of the three things that actually
    /// went wrong (expired login, spent credit, exhausted context).
    pub failure: Option<String>,
}

/// Object keys that hold the assistant's own words. Vendors nest the answer under
/// different keys (`item` for Codex, `part` for OpenCode) but agree on `text` inside it.
const ANSWER_KINDS: &[&str] = &["agent_message", "text"];

/// Cache counters that are *additional* to `input_tokens` rather than a breakdown of it.
///
/// Vendors disagree here and the difference is large enough to matter: Claude Code
/// reports only the uncached remainder in `input_tokens` (2, for a 33 000-token prompt)
/// and names the cache separately under these keys, so they are added back. Codex's
/// `input_tokens` is already the total and its `cached_input_tokens` is a breakdown of
/// it — a different key, deliberately not in this list, or the ledger would double-count.
const EXTRA_INPUT_KEYS: &[&str] = &["cache_read_input_tokens", "cache_creation_input_tokens"];

/// Where a piece of text came from, so the same words are never shown twice.
///
/// Claude Code in `stream-json` prints every sentence up to three times: as
/// `content_block_delta` partials, again as the finished `assistant` content block, and
/// once more in the closing `result`. Whichever source speaks first for a turn is the one
/// believed for the rest of it — partials always precede the block that contains them,
/// and `result` always comes last, so first-wins yields exactly one copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Source {
    Undecided,
    Partial,
    Block,
    Result,
}

/// Incremental reader over one backend's stdout.
#[derive(Debug)]
pub struct Reader {
    transcript: Transcript,
    source: Source,
    usage: Option<TokenCounts>,
    failure: Option<String>,
    /// How much of each addressable item has already been emitted, so a vendor that
    /// re-sends a growing snapshot of one message yields deltas rather than repeats.
    emitted: HashMap<String, usize>,
    /// The closing `result` string, held back in case nothing else ever speaks.
    fallback: Option<String>,
    spoke: bool,
}

impl Reader {
    #[must_use]
    pub fn new(transcript: Transcript) -> Self {
        Self {
            transcript,
            source: Source::Undecided,
            usage: None,
            failure: None,
            emitted: HashMap::new(),
            fallback: None,
            spoke: false,
        }
    }

    /// Consumes one line of stdout and returns what it said.
    pub fn push_line(&mut self, line: &str) -> Vec<TranscriptEvent> {
        match self.transcript {
            Transcript::Plain => {
                self.spoke = true;
                vec![TranscriptEvent::Text(format!("{line}\n"))]
            }
            Transcript::JsonLines => self.push_json_line(line),
        }
    }

    /// Closes the stream: releases the held-back `result` when nothing else ever spoke,
    /// plus whatever usage and failure the turn reported.
    pub fn finish(&mut self) -> Vec<TranscriptEvent> {
        let mut out = Vec::new();
        if !self.spoke {
            if let Some(text) = self.fallback.take().filter(|text| !text.is_empty()) {
                self.spoke = true;
                out.push(TranscriptEvent::Text(text));
            }
        }
        if let Some(usage) = self.usage.take() {
            out.push(TranscriptEvent::Usage(usage));
        }
        if let Some(failure) = self.failure.take() {
            out.push(TranscriptEvent::Failure(failure));
        }
        out
    }

    /// True once any words have been produced for the user.
    #[must_use]
    pub const fn spoke(&self) -> bool {
        self.spoke
    }

    fn push_json_line(&mut self, line: &str) -> Vec<TranscriptEvent> {
        let line = line.trim();
        // Vendors interleave their own log lines with the event stream; anything that is
        // not an object is not an event, and dropping it is the whole point of asking
        // for JSON in the first place.
        if !line.starts_with('{') {
            return Vec::new();
        }
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        // Usage is cumulative per turn in both dialects, so the last report wins and it
        // is released once at `finish` rather than re-announced on every line.
        if let Some(counts) = event_usage(&event) {
            self.usage = Some(counts);
        }
        if let Some(reason) = event_failure(&event) {
            self.failure = Some(reason);
        }
        if let Some(report) = event_limits(&event) {
            out.push(TranscriptEvent::Limit(report));
        }
        self.collect_text(&event, &mut out);
        collect_tools(&event, &mut out);
        out
    }

    /// Accepts text only from the source that spoke first this turn.
    fn take(&mut self, source: Source, text: &str) -> Option<String> {
        if text.is_empty() {
            return None;
        }
        if self.source == Source::Undecided {
            self.source = source;
        }
        if self.source != source {
            return None;
        }
        self.spoke = true;
        Some(text.to_owned())
    }

    /// Emits only the part of `whole` not already sent under `key`.
    ///
    /// Codex re-sends an `agent_message` as it grows, and every send carries the whole
    /// message so far. Appending them verbatim repeats the answer once per update.
    fn delta_for(&mut self, key: &str, whole: &str) -> Option<String> {
        let already = self.emitted.get(key).copied().unwrap_or(0);
        if whole.len() <= already {
            return None;
        }
        // A vendor that rewrote rather than extended is republishing, not continuing;
        // taking the tail of a changed string would splice two different sentences.
        let piece = if already == 0 {
            whole
        } else {
            whole.get(already..)?
        };
        self.emitted.insert(key.to_owned(), whole.len());
        Some(piece.to_owned())
    }

    fn collect_text(&mut self, event: &Value, out: &mut Vec<TranscriptEvent>) {
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        // ── Grok `--output-format streaming-json` ────────────────────────────────
        // Chunks arrive as `{type:text,data:"…"}` / `{type:thought,data:"…"}`.
        // OpenCode also uses `type:text` but nests the words under `part` — only
        // return early when this is Grok's `data` shape.
        if kind == "text" {
            if let Some(text) = event.get("data").and_then(Value::as_str) {
                if let Some(piece) = self.take(Source::Partial, text) {
                    out.push(TranscriptEvent::Text(piece));
                }
                return;
            }
        }
        if kind == "thought" {
            if let Some(text) = event.get("data").and_then(Value::as_str) {
                if !text.is_empty() {
                    out.push(TranscriptEvent::Thought(text.to_owned()));
                }
                return;
            }
        }

        // ── Claude Code, `--output-format stream-json` ───────────────────────────
        if kind == "stream_event" {
            self.claude_partial(event, out);
            return;
        }
        if kind == "assistant" {
            self.claude_block(event, out);
            return;
        }
        if kind == "result" {
            // Claude's closing whole-turn object — and the entire answer under the older
            // non-streaming `--output-format json`, which is why it is held, not dropped.
            let text = event
                .get("result")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if text.is_empty() {
                return;
            }
            if self.source == Source::Undecided {
                if let Some(piece) = self.take(Source::Result, text) {
                    out.push(TranscriptEvent::Text(piece));
                }
            } else {
                self.fallback = Some(text.to_owned());
            }
            return;
        }

        // ── Codex (`item`) and OpenCode (`part`) ─────────────────────────────────
        let Some(body) = event.get("item").or_else(|| event.get("part")) else {
            return;
        };
        let Some(body_kind) = body.get("type").and_then(Value::as_str) else {
            return;
        };
        let Some(text) = body.get("text").and_then(Value::as_str) else {
            return;
        };
        if body_kind.contains("reasoning") || body_kind.contains("thinking") {
            // Reasoning is re-sent as it grows in exactly the way the answer is.
            let piece = match body.get("id").and_then(Value::as_str) {
                Some(id) => self.delta_for(&format!("think:{id}"), text),
                None => Some(text.to_owned()),
            };
            if let Some(piece) = piece.filter(|piece| !piece.is_empty()) {
                out.push(TranscriptEvent::Thought(piece));
            }
            return;
        }
        if !ANSWER_KINDS.contains(&body_kind) {
            return;
        }
        // An addressable item is re-sent as it grows; an anonymous part is already a delta.
        let piece = match body.get("id").and_then(Value::as_str) {
            Some(id) => self.delta_for(&format!("say:{id}"), text),
            None => Some(text.to_owned()),
        };
        if let Some(piece) = piece {
            if let Some(piece) = self.take(Source::Partial, &piece) {
                out.push(TranscriptEvent::Text(piece));
            }
        }
    }

    fn claude_partial(&mut self, event: &Value, out: &mut Vec<TranscriptEvent>) {
        let Some(inner) = event.get("event") else {
            return;
        };
        if inner.get("type").and_then(Value::as_str) != Some("content_block_delta") {
            return;
        }
        let Some(delta) = inner.get("delta") else {
            return;
        };
        match delta.get("type").and_then(Value::as_str) {
            Some("text_delta") => {
                let text = delta
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if let Some(piece) = self.take(Source::Partial, text) {
                    out.push(TranscriptEvent::Text(piece));
                }
            }
            Some("thinking_delta") => {
                let text = delta
                    .get("thinking")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !text.is_empty() {
                    out.push(TranscriptEvent::Thought(text.to_owned()));
                }
            }
            _ => {}
        }
    }

    /// Claude's finished content blocks. Believed only when no partial spoke first.
    fn claude_block(&mut self, event: &Value, out: &mut Vec<TranscriptEvent>) {
        let blocks = event
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_array);
        for block in blocks.into_iter().flatten() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let text = block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if let Some(piece) = self.take(Source::Block, text) {
                        out.push(TranscriptEvent::Text(piece));
                    }
                }
                Some("thinking") => {
                    let text = block
                        .get("thinking")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if !text.is_empty() {
                        out.push(TranscriptEvent::Thought(text.to_owned()));
                    }
                }
                _ => {}
            }
        }
    }
}

/// Tool steps named on one event, in whichever dialect named them.
fn collect_tools(event: &Value, out: &mut Vec<TranscriptEvent>) {
    let kind = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // Claude names its tools inside the assistant message; the matching `user` message
    // carrying `tool_result` is what closes them.
    if kind == "assistant" || kind == "user" {
        let blocks = event
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_array);
        for block in blocks.into_iter().flatten() {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let name = block.get("name").and_then(Value::as_str).unwrap_or("tool");
                    out.push(TranscriptEvent::Tool {
                        id: block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or(name)
                            .to_owned(),
                        kind: ToolKind::of(name),
                        title: name.to_owned(),
                        detail: tool_target(block.get("input")),
                        done: false,
                    });
                }
                Some("tool_result") => {
                    if let Some(id) = block.get("tool_use_id").and_then(Value::as_str) {
                        out.push(TranscriptEvent::Tool {
                            id: id.to_owned(),
                            kind: ToolKind::Other,
                            title: String::new(),
                            detail: String::new(),
                            done: true,
                        });
                    }
                }
                _ => {}
            }
        }
        return;
    }

    // Codex and OpenCode both describe a step under their own body key.
    let Some(body) = event.get("item").or_else(|| event.get("part")) else {
        return;
    };
    let Some(body_kind) = body.get("type").and_then(Value::as_str) else {
        return;
    };
    let (name, detail) = match body_kind {
        "command_execution" => (
            "bash",
            body.get("command").map(value_summary).unwrap_or_default(),
        ),
        "file_change" | "patch_apply" => (
            "edit",
            body.get("path")
                .or_else(|| body.get("changes"))
                .map(value_summary)
                .unwrap_or_default(),
        ),
        "web_search" => (
            "search",
            body.get("query").map(value_summary).unwrap_or_default(),
        ),
        "tool" | "tool-invocation" | "tool_use" => (
            body.get("tool")
                .or_else(|| body.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("tool"),
            body.get("state")
                .and_then(|state| state.get("input"))
                .or_else(|| body.get("input"))
                .map(value_summary)
                .unwrap_or_default(),
        ),
        _ => return,
    };
    out.push(TranscriptEvent::Tool {
        id: body
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(name)
            .to_owned(),
        kind: ToolKind::of(name),
        title: name.to_owned(),
        detail,
        done: kind.ends_with("completed") || kind.ends_with("finish"),
    });
}

/// The most useful single string in a tool's arguments — a path, a command, a query.
fn tool_target(input: Option<&Value>) -> String {
    let Some(input) = input else {
        return String::new();
    };
    for key in [
        "file_path",
        "path",
        "notebook_path",
        "command",
        "pattern",
        "query",
        "url",
        "description",
    ] {
        if let Some(text) = input.get(key).and_then(Value::as_str) {
            if !text.is_empty() {
                return text.chars().take(180).collect();
            }
        }
    }
    value_summary(input)
}

fn value_summary(value: &Value) -> String {
    match value {
        Value::String(text) => text.chars().take(180).collect(),
        other => other.to_string().chars().take(180).collect(),
    }
}

/// The plan allowances a vendor volunteered on this line.
///
/// Only Claude Code reports these today, under `rate_limit_event`. The shape is read
/// defensively — a vendor that renames `unifiedWindows` tomorrow degrades to "no report"
/// rather than to a wrong number on a gauge the user is trusting.
fn event_limits(event: &Value) -> Option<LimitReport> {
    if event.get("type").and_then(Value::as_str) != Some("rate_limit_event") {
        return None;
    }
    let info = event.get("rate_limit_info")?;
    let read = |key: &str| -> Option<Window> {
        let window = info.get("unifiedWindows")?.get(key)?;
        Some(Window {
            #[allow(clippy::cast_possible_truncation)]
            utilization: window
                .get("utilization")
                .and_then(Value::as_f64)
                .unwrap_or_default() as f32,
            resets_at: window.get("resetsAt").and_then(Value::as_i64),
        })
    };
    Some(LimitReport {
        status: info
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("allowed")
            .to_owned(),
        binding: info
            .get("rateLimitType")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        session: read("five_hour"),
        weekly: read("seven_day"),
    })
}

/// What the vendor said went wrong, on a line that reports a failure.
fn event_failure(event: &Value) -> Option<String> {
    let kind = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if matches!(kind, "error" | "turn.failed" | "stream_error") {
        return Some(
            event
                .get("message")
                .or_else(|| event.get("error"))
                .map(value_summary)
                .unwrap_or_else(|| "the vendor reported an error".to_owned()),
        );
    }
    if kind != "result" {
        return None;
    }
    let is_error = event
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let subtype = event
        .get("subtype")
        .and_then(Value::as_str)
        .unwrap_or("success");
    if !is_error && subtype == "success" {
        return None;
    }
    // The `result` string carries the vendor's own explanation when it failed, and that
    // is the text every limit, credit, and context message actually lives in.
    let said = event
        .get("result")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| subtype.replace('_', " "));
    Some(format!("{subtype}: {said}"))
}

/// Reads a whole `stdout` buffer according to how this backend prints.
#[must_use]
pub fn read(transcript: Transcript, stdout: &str) -> Answer {
    let mut answer = Answer::default();
    if transcript == Transcript::Plain {
        // Plain stdout *is* the answer: it must survive verbatim, trailing bytes included.
        answer.text = stdout.to_owned();
        return answer;
    }
    let mut reader = Reader::new(transcript);
    let apply = |events: Vec<TranscriptEvent>, answer: &mut Answer| {
        for event in events {
            match event {
                TranscriptEvent::Text(piece) => answer.text.push_str(&piece),
                TranscriptEvent::Usage(counts) => answer.usage = Some(counts),
                TranscriptEvent::Failure(reason) => answer.failure = Some(reason),
                TranscriptEvent::Thought(_)
                | TranscriptEvent::Tool { .. }
                | TranscriptEvent::Limit(_) => {}
            }
        }
    };
    for line in stdout.lines() {
        apply(reader.push_line(line), &mut answer);
    }
    apply(reader.finish(), &mut answer);
    answer
}

/// Token counts in one event. Codex and Claude Code report `usage.{input,output}_tokens`
/// on the turn; OpenCode reports `part.tokens.{input,output}` on each finished step.
fn event_usage(event: &Value) -> Option<TokenCounts> {
    if let Some(usage) = event.get("usage") {
        let input = usage.get("input_tokens").and_then(Value::as_u64);
        let output = usage.get("output_tokens").and_then(Value::as_u64);
        if let (Some(input), Some(output)) = (input, output) {
            let cached: u64 = EXTRA_INPUT_KEYS
                .iter()
                .filter_map(|key| usage.get(key).and_then(Value::as_u64))
                .sum();
            return Some(TokenCounts {
                input: input.saturating_add(cached),
                output,
            });
        }
    }
    let tokens = event.get("part").and_then(|part| part.get("tokens"))?;
    Some(TokenCounts {
        input: tokens.get("input").and_then(Value::as_u64)?,
        output: tokens.get("output").and_then(Value::as_u64)?,
    })
}

#[cfg(test)]
mod tests {
    use super::{read, Answer, Reader, TokenCounts, ToolKind, Transcript, TranscriptEvent};

    /// Captured from `codex exec --skip-git-repo-check --json`, log line included.
    const CODEX: &str = concat!(
        r#"{"type":"thread.started","thread_id":"01a03e62"}"#,
        "\n",
        r#"{"type":"turn.started"}"#,
        "\n",
        "2026-08-26T14:04:25.159033Z ERROR rmcp::transport::worker: worker quit\n",
        r#"{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"hmm"}}"#,
        "\n",
        r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"PONG"}}"#,
        "\n",
        r#"{"type":"turn.completed","usage":{"input_tokens":15836,"output_tokens":6}}"#,
        "\n",
    );

    /// Captured from `opencode run --format json`.
    const OPENCODE: &str = concat!(
        r#"{"type":"step_start","part":{"type":"step-start"}}"#,
        "\n",
        r#"{"type":"text","part":{"type":"text","text":"PO"}}"#,
        "\n",
        r#"{"type":"text","part":{"type":"text","text":"NG"}}"#,
        "\n",
        r#"{"type":"step_finish","part":{"type":"step-finish","reason":"stop","tokens":{"total":7930,"input":6250,"output":4}}}"#,
        "\n",
    );

    /// Captured from `claude -p … --output-format json`: one whole-turn object, and an
    /// `input_tokens` that counts only what missed the cache.
    const CLAUDE: &str = concat!(
        r#"{"is_error":false,"type":"result","result":"PONG","usage":{"input_tokens":2,"#,
        r#""cache_creation_input_tokens":10981,"cache_read_input_tokens":22076,"output_tokens":5}}"#,
        "\n",
    );

    /// Captured from `claude -p … --output-format stream-json --verbose
    /// --include-partial-messages`: the same four letters printed three separate times.
    const CLAUDE_STREAM: &str = concat!(
        r#"{"type":"system","subtype":"init","tools":["Read"]}"#,
        "\n",
        r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"PO"}}}"#,
        "\n",
        r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"NG"}}}"#,
        "\n",
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"PONG"}]}}"#,
        "\n",
        r#"{"is_error":false,"subtype":"success","type":"result","result":"PONG","usage":{"input_tokens":2,"output_tokens":5}}"#,
        "\n",
    );

    fn drain(transcript: Transcript, stdout: &str) -> Vec<TranscriptEvent> {
        let mut reader = Reader::new(transcript);
        let mut events = Vec::new();
        for line in stdout.lines() {
            events.extend(reader.push_line(line));
        }
        events.extend(reader.finish());
        events
    }

    #[test]
    fn plain_stdout_is_the_answer_untouched() {
        let answer = read(Transcript::Plain, "PONG\n");
        assert_eq!(
            answer,
            Answer {
                text: "PONG\n".to_owned(),
                usage: None,
                failure: None,
            }
        );
    }

    #[test]
    fn codex_events_yield_the_message_and_its_usage() {
        let answer = read(Transcript::JsonLines, CODEX);
        assert_eq!(answer.text, "PONG", "reasoning must not reach the answer");
        assert_eq!(
            answer.usage,
            Some(TokenCounts {
                input: 15836,
                output: 6
            })
        );
    }

    #[test]
    fn opencode_events_concatenate_text_parts_and_yield_step_tokens() {
        let answer = read(Transcript::JsonLines, OPENCODE);
        assert_eq!(answer.text, "PONG");
        assert_eq!(
            answer.usage,
            Some(TokenCounts {
                input: 6250,
                output: 4
            })
        );
    }

    #[test]
    fn claude_result_objects_yield_the_answer_and_the_whole_prompt_it_cost() {
        let answer = read(Transcript::JsonLines, CLAUDE);
        assert_eq!(answer.text, "PONG");
        // 2 uncached + 22 076 read + 10 981 written. Reporting the bare 2 would show a
        // 33 000-token prompt as free on the usage ring.
        assert_eq!(
            answer.usage,
            Some(TokenCounts {
                input: 33_059,
                output: 5
            })
        );
    }

    /// The whole reason for streaming: words must arrive before the process exits.
    #[test]
    fn a_partial_delta_is_readable_before_the_turn_ends() {
        let mut reader = Reader::new(Transcript::JsonLines);
        let events = reader.push_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hel"}}}"#,
        );
        assert_eq!(events, vec![TranscriptEvent::Text("Hel".to_owned())]);
        assert!(reader.spoke());
    }

    /// Claude prints the same sentence as partials, as a finished block, and again in
    /// `result`. Believing all three shows the user their answer three times over.
    #[test]
    fn claude_stream_json_says_pong_exactly_once() {
        let answer = read(Transcript::JsonLines, CLAUDE_STREAM);
        assert_eq!(answer.text, "PONG");
        assert_eq!(answer.failure, None);
    }

    /// A turn that only ever produced a `result` must still be readable — that is the
    /// entire answer under the non-streaming output format.
    #[test]
    fn a_result_only_turn_still_yields_its_answer() {
        assert_eq!(read(Transcript::JsonLines, CLAUDE).text, "PONG");
    }

    /// Codex re-sends `agent_message` as it grows; appending each send verbatim would
    /// print "PO" then "PONG", so the user reads "POPONG".
    #[test]
    fn a_regrowing_item_yields_deltas_not_repeats() {
        let stream = concat!(
            r#"{"type":"item.updated","item":{"id":"i1","type":"agent_message","text":"PO"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"id":"i1","type":"agent_message","text":"PONG"}}"#,
            "\n",
        );
        assert_eq!(read(Transcript::JsonLines, stream).text, "PONG");
    }

    /// The failure this whole change exists for: an exit-0 run that told us, in-band,
    /// exactly what went wrong, and used to be reported as an empty answer.
    #[test]
    fn an_in_band_failure_on_a_successful_exit_is_not_lost() {
        let stream = concat!(
            r#"{"is_error":true,"subtype":"error_during_execution","type":"result","#,
            r#""result":"Claude usage limit reached. Your limit will reset at 4pm."}"#,
            "\n",
        );
        let answer = read(Transcript::JsonLines, stream);
        let Some(failure) = answer.failure else {
            panic!("an is_error result must report a failure");
        };
        assert!(failure.contains("usage limit reached"), "{failure}");

        let codex_error = "{\"type\":\"error\",\"message\":\"context window exceeded\"}\n";
        let Some(failure) = read(Transcript::JsonLines, codex_error).failure else {
            panic!("a codex error event must report a failure");
        };
        assert!(failure.contains("context window"), "{failure}");
    }

    /// The activity dock renders real work only if the reader names it.
    #[test]
    fn tool_steps_are_named_as_they_start_and_finish() {
        let stream = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","#,
            r#""name":"Read","input":{"file_path":"src/main.rs"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1"}]}}"#,
            "\n",
        );
        let events = drain(Transcript::JsonLines, stream);
        let tools: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                TranscriptEvent::Tool { id, kind, done, .. } => Some((id.as_str(), *kind, *done)),
                _ => None,
            })
            .collect();
        assert!(tools.contains(&("t1", ToolKind::Read, false)), "{tools:?}");
        assert!(tools.contains(&("t1", ToolKind::Other, true)), "{tools:?}");
    }

    /// Captured verbatim from a live `claude -p … --output-format stream-json` run.
    ///
    /// This is the event that makes a *pre-emptive* limit warning possible at all: the
    /// vendor states both windows and both reset times on every turn, long before either
    /// one is spent. Losing this parse means going back to only learning about a spent
    /// week from the turn that fails on it.
    #[test]
    fn claude_reports_both_rolling_windows_before_either_is_spent() {
        let line = concat!(
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","#,
            r#""resetsAt":1788210000,"rateLimitType":"seven_day","utilization":0.61,"#,
            r#""unifiedWindows":{"five_hour":{"utilization":0.37,"resetsAt":1787788200},"#,
            r#""seven_day":{"utilization":0.61,"resetsAt":1788210000}}}}"#,
        );
        let events = drain(Transcript::JsonLines, line);
        let Some(TranscriptEvent::Limit(report)) = events.into_iter().next() else {
            panic!("a rate_limit_event must be read as a limit report");
        };
        assert_eq!(report.binding, "seven_day");
        assert_eq!(report.status, "allowed_warning");
        let Some(weekly) = report.weekly else {
            panic!("the weekly window must be read");
        };
        assert!((weekly.utilization - 0.61).abs() < 0.001, "{weekly:?}");
        assert_eq!(weekly.resets_at, Some(1_788_210_000));
        let Some(session) = report.session else {
            panic!("the session window must be read");
        };
        assert!((session.utilization - 0.37).abs() < 0.001, "{session:?}");
        // The weekly window is the one nearer the edge, so it is the one to show.
        assert!((report.worst() - 0.61).abs() < 0.001);
        assert!(report.worth_warning(), "a warning status must be surfaced");
    }

    /// A quiet account must not be nagged, and a vendor that renames its fields must
    /// degrade to silence rather than to a wrong number on a gauge the user trusts.
    #[test]
    fn a_healthy_account_is_not_warned_and_an_unknown_shape_reports_nothing() {
        let quiet = concat!(
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","#,
            r#""unifiedWindows":{"five_hour":{"utilization":0.05}}}}"#,
        );
        let events = drain(Transcript::JsonLines, quiet);
        let Some(TranscriptEvent::Limit(report)) = events.into_iter().next() else {
            panic!("a rate_limit_event must be read as a limit report");
        };
        assert!(!report.worth_warning());
        assert_eq!(report.weekly, None);

        let renamed = r#"{"type":"rate_limit_event","somethingElse":{}}"#;
        assert!(drain(Transcript::JsonLines, renamed).is_empty());
    }

    #[test]
    fn tool_names_map_onto_the_shared_vocabulary() {
        assert_eq!(ToolKind::of("Read"), ToolKind::Read);
        assert_eq!(ToolKind::of("MultiEdit"), ToolKind::Edit);
        assert_eq!(ToolKind::of("Bash"), ToolKind::Run);
        assert_eq!(ToolKind::of("WebSearch"), ToolKind::Search);
        assert_eq!(ToolKind::of("WebFetch"), ToolKind::Fetch);
        assert_eq!(ToolKind::of("TodoWrite"), ToolKind::Plan);
        assert_eq!(ToolKind::of("something_new"), ToolKind::Other);
    }

    /// Codex's `input_tokens` is already the total; its cache figure is a breakdown of
    /// that number, and adding it back would bill the user twice for one prompt.
    #[test]
    fn a_cache_breakdown_is_not_added_to_a_total_that_already_contains_it() {
        let codex_turn = concat!(
            r#"{"type":"turn.completed","usage":{"input_tokens":15836,"#,
            r#""cached_input_tokens":11008,"output_tokens":6}}"#,
            "\n",
        );
        assert_eq!(
            read(Transcript::JsonLines, codex_turn).usage,
            Some(TokenCounts {
                input: 15_836,
                output: 6
            })
        );
    }

    /// Captured from `grok -p … --output-format streaming-json`.
    #[test]
    fn grok_streaming_json_chunks_become_one_answer() {
        let stream = concat!(
            r#"{"type":"thought","data":"thinking"}"#,
            "\n",
            r#"{"type":"text","data":"hel"}"#,
            "\n",
            r#"{"type":"text","data":"lo"}"#,
            "\n",
            r#"{"type":"usage","usage":{"input_tokens":12,"output_tokens":2,"cache_read_input_tokens":0}}"#,
            "\n",
            r#"{"type":"end","stopReason":"end_turn"}"#,
            "\n",
        );
        let answer = read(Transcript::JsonLines, stream);
        assert_eq!(answer.text, "hello");
        assert!(
            !answer.text.contains("thinking"),
            "thoughts must stay out of the answer"
        );
        assert_eq!(
            answer.usage,
            Some(TokenCounts {
                input: 12,
                output: 2
            })
        );
    }

    /// A vendor log line on stdout must never reach the user as the answer.
    #[test]
    fn interleaved_log_lines_are_dropped() {
        assert!(!read(Transcript::JsonLines, CODEX).text.contains("rmcp"));
    }

    #[test]
    fn a_transcript_with_no_events_reads_as_an_empty_answer() {
        for text in ["", "  \n \n", "not json at all\n"] {
            assert_eq!(read(Transcript::JsonLines, text), Answer::default());
        }
    }

    /// Usage is only ever what the vendor said; a stream without it reports nothing
    /// rather than a guess the ledger would then charge the user for.
    #[test]
    fn missing_usage_is_reported_as_missing() {
        let stream = concat!(
            r#"{"type":"text","part":{"type":"text","text":"hi"}}"#,
            "\n",
            r#"{"type":"step_finish","part":{"type":"step-finish"}}"#,
            "\n",
        );
        let answer = read(Transcript::JsonLines, stream);
        assert_eq!(answer.text, "hi");
        assert_eq!(answer.usage, None);
    }

    /// Reasoning must reach the thinking drawer, never the answer body.
    #[test]
    fn reasoning_is_separated_from_the_answer() {
        let stream = concat!(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"weighing"}}}"#,
            "\n",
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"answer"}}}"#,
            "\n",
        );
        let events = drain(Transcript::JsonLines, stream);
        assert!(events.contains(&TranscriptEvent::Thought("weighing".to_owned())));
        assert!(events.contains(&TranscriptEvent::Text("answer".to_owned())));
        assert_eq!(read(Transcript::JsonLines, stream).text, "answer");
    }
}
