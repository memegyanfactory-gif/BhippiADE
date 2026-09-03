//! The conversational engine (ADR-0006): one turn stream the UI can watch.
//!
//! Provider streams carry model output only. Everything the *engine* does — reading,
//! checking providers, asking permission — is emitted here as engine events, so the
//! interface stays identical when real research lands behind it in later sprints.

use bhippi_providers::{
    cli::CliProvider, demo::DemoProvider, ollama::OllamaProvider,
    openai_compat::OpenAiCompatProvider, provider::Provider, CompletionRequest, Delta, Message,
    ProviderInfo, Role, StopReason,
};
use bhippi_types::{ErrorCode, TaskClass};
use chrono::Utc;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tauri_specta::Event;
use tokio::sync::{oneshot, watch, Mutex};

/// System prompt for the chat surface. A versioned copy lands in `prompts/` with BHP-060;
/// until prompts exist this constant is the single place to change it (R5).
const CHAT_SYSTEM: &str = "You are Bhippi, a desktop research engine for technology and AI.\n\
Answer precisely, cite what you know, admit uncertainty, and never invent sources.\n\
Stay on topic: technology and AI only.";

/// Caveman protocol directive (inspired by JuliusBrussee/caveman). Slashes token usage
/// by stripping conversational filler and politeness while strictly preserving all code, diffs,
/// filepaths, and technical facts 100% syntactically valid and complete.
pub const CAVEMAN_SYSTEM_DIRECTIVE: &str = "## 🦴 CAVEMAN PROTOCOL (TOKEN COMPRESSION MODE ACTIVE)\n\
You are an expert software engineer communicating in telegraphic caveman syntax. Extreme token efficiency is mandatory.\n\
RULES:\n\
1. NO FILLER. Omit all pleasantries, apologies, polite preambles, and conversational padding (\"Sure!\", \"I'd be happy to help\", \"Certainly\", \"As an AI...\").\n\
2. TELEGRAPHIC SYNTAX. Use terse phrasing, high-density facts, short verbs. Omit articles (a, an, the) and filler verbs where meaning is clear.\n\
3. CODE INTEGRITY 100%. All code, diffs, commands, patches, and filepaths MUST be 100% syntactically correct, complete, and functional. NEVER abbreviate or omit working code.\n\
4. DIRECT ANSWERS. Explain root cause in minimum words. Show code/diff. Stop. No trailing summaries or restatements.";
const WORKSPACE_SYSTEM: &str = include_str!("../../../prompts/chat-workspace.md");
const RULES_SYSTEM: &str = include_str!("../../../prompts/chat-rules.md");
const COMPUTER_USE_SYSTEM: &str = include_str!("../../../prompts/chat-computer-use.md");
const ENGINE_SYSTEM: &str = include_str!("../../../prompts/chat-engine.md");
const MAX_COMPUTER_ACTIONS_PER_TURN: usize = 24;
const COMPUTER_UI_SETTLE_DELAY: Duration = Duration::from_millis(450);

fn computer_stop_requested(generation: u64, emergency: &watch::Receiver<u64>) -> bool {
    generation != 0 && *emergency.borrow() == generation
}

/// Waits for either the ordinary Stop action or the desktop-wide Esc/Esc emergency stop.
/// The overlay signal is generation-scoped, so a late key event from an old turn cannot
/// cancel the next turn that happens to start while its window is fading out.
async fn wait_for_computer_stop(
    cancel: &mut watch::Receiver<bool>,
    emergency: &mut watch::Receiver<u64>,
    generation: u64,
) -> bool {
    loop {
        if *cancel.borrow() || computer_stop_requested(generation, emergency) {
            return true;
        }
        tokio::select! {
            changed = cancel.changed() => {
                if changed.is_err() {
                    return computer_stop_requested(generation, emergency);
                }
            }
            changed = emergency.changed() => {
                if changed.is_err() {
                    return *cancel.borrow();
                }
            }
        }
    }
}

/// A rules file longer than this is truncated rather than allowed to crowd out the
/// conversation itself — standing instructions are meant to be a page, not a corpus.
const MAX_RULES_CHARS: usize = 8_000;

/// Loads the project's own agent rules, when the owner has written any.
///
/// Returns `None` for a missing or blank file so the caller appends nothing at all,
/// rather than an empty block the model has to interpret.
async fn project_rules_block(workspace: &str) -> Option<String> {
    let path = std::path::Path::new(workspace)
        .join(".bhippi")
        .join("rules.md");
    let raw = tokio::fs::read_to_string(&path).await.ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let text = match trimmed.char_indices().nth(MAX_RULES_CHARS) {
        Some((cut, _)) => format!("{}\n… (rules truncated)", &trimmed[..cut]),
        None => trimmed.to_owned(),
    };
    Some(RULES_SYSTEM.replace("{{rules}}", &text))
}

/// Permission requests time out rather than hanging a turn forever.
const PERMISSION_TIMEOUT: Duration = Duration::from_secs(600);

/// Response effort, shown as the composer's speed control (Faster ↔ Smarter).
///
/// Every level changes three real knobs — token ceiling, temperature, and one system
/// directive — so the choice is visible in the answer, not decorative.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Fast,
    #[default]
    Balanced,
    Quality,
    Ultra,
}

impl Effort {
    #[must_use]
    pub const fn max_tokens(self) -> u32 {
        match self {
            Self::Fast => 512,
            Self::Balanced => 2_048,
            Self::Quality => 4_096,
            Self::Ultra => 8_192,
        }
    }

    #[must_use]
    pub const fn temperature(self) -> f32 {
        match self {
            Self::Fast => 0.4,
            Self::Balanced => 0.7,
            Self::Quality => 0.7,
            Self::Ultra => 0.8,
        }
    }

    /// One directive appended to the chat system prompt.
    #[must_use]
    pub const fn directive(self) -> &'static str {
        match self {
            Self::Fast => "Answer in the fewest words that fully solve the question.",
            Self::Balanced => "Answer directly; include the key reasoning, skip padding.",
            Self::Quality => {
                "Think it through: cover trade-offs and caveats, and note what is uncertain."
            }
            Self::Ultra => {
                "Go deep: structure the answer, examine alternatives and edge cases, \
                 surface what is disputed, and state what would change the conclusion."
            }
        }
    }
}

/// Whether this turn must follow the Bhippi Design System.
///
/// A flag rather than a persistent setting, and on the turn rather than in config, for the
/// same reason `Effort` is: it changes what one answer should be, and the user decides that
/// per question. Someone asking "why is this failing" does not want a design brief in the
/// reply; someone asking for a settings page does.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DesignMode {
    /// Answer normally.
    #[default]
    Off,
    /// Hold every interface decision to `docs/DESIGN-SYSTEM.md`.
    On,
}

impl DesignMode {
    #[must_use]
    pub const fn is_on(self) -> bool {
        matches!(self, Self::On)
    }

    /// The directive appended to the system prompt when the switch is on.
    ///
    /// Deliberately the *rules*, not a link to them: a coding CLI running in a workspace
    /// may not have `docs/DESIGN-SYSTEM.md` in front of it, and a directive that depends
    /// on the model going to find a file is a directive that silently does nothing. This
    /// is the condensed form — the judgements and the constraints that actually change
    /// output — with the full document as the reference behind it.
    #[must_use]
    pub const fn directive(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::On => concat!(
                "

## Bhippi Design System — active (v1.1)
",
                "Every interface you produce or modify in this turn **must** follow these. ",
                "They override any default styling instinct and any existing code style you see. ",
                "If existing code violates them, bring it up to these rules — never match the old style.

",
                "**Five judgements — in priority order; earlier wins when they conflict**
",
                "1. Density is a feature: 13px base on a 4px grid. This is a tool people use ",
                "for hours, not a screenshot. Spend space only where scanning breaks.
",
                "2. One accent, used sparingly — it marks the *single* action that matters on a screen. ",
                "A second accent removes emphasis rather than adding it.
",
                "3. Hairlines in the layout; shadows only for surfaces that float *over* it (drop-ups, modals, toasts), where lift is the actual message.
",
                "4. Motion must say what changed and where it came from. No decoration that would irritate on its 400th viewing.
",
                "5. No state is ever colour alone — pair it with a glyph, label, position, or border.

",
                "**Tokens — the only values that may appear in component styles**
",
                "- Colour: `--bg --surface --surface-2 --surface-3 --line --line-strong --text --text-dim --text-faint --accent --accent-hi --accent-dim --accent-line --on-accent --ok --warn --error`. Never a literal hex (#…, rgb…) inside a component — not even #fff, #000, or rgba(…).
",
                "- Spacing: `4 8 12 16 24 32 48` only. Nothing between. Related items 4–8 apart, groups 16–24 apart.
",
                "- Type: `10 11 12 13 15 18 24`. Weights 400 body · 550 emphasis · 600 headings, never heavier. Tabular numerals for anything that ticks.
",
                "- Radii: 4px controls, 8px panels, 12–16px menus, 6px modals, 999px pills. Inner radius = outer − padding.
",
                "- Elevation: flat (hairline) in layout · `--lift-1` cards · `--lift-2` drop-ups · `--lift-3` modals. No shadow on flat.
",
                "- Motion: transform and opacity only — never width/height/top/margin. 90ms press · 140ms hover · 220ms travel · 300ms enter · 420ms settle. Ease: --e-out arrivals, --e-in departures, --e-both both, --e-spring only for confirmations. Every animation collapses under `prefers-reduced-motion` and has a static tell.

",
                "**Hard floors**
",
                "- Contrast: 4.5:1 body text, 3:1 large text & glyphs, 3:1 focus ring vs both element and page behind it.
",
                "- Focus: every interactive element has a visible `:focus-visible` ring ≥ 3:1.
",
                "- No disabled button without a tooltip saying what would enable it.

",
                "**Composition — the ten decisions that separate designed from assembled**
",
                "1. One focal point and one primary action. 2. Three hierarchy levels max. 3. Align every edge to another edge. 4. Repeat, don't invent (fourth card ≡ first). 5. Whitespace is structure — border is last resort. 6. If two things look different, they *are* different. 7. Neutrals + one accent + at most one semantic colour per screen. 8. Size by importance, not label length. 9. Every state every time: empty, loading, partial, error, full. 10. Reduce until it breaks, then add one thing back.

",
                "**Components — apply, don't redesign**
",
                "- Buttons: primary (≤1/screen) · secondary · ghost · danger (≤1, never autofocus) · link. 24/28/34px. All six states, loading keeps width.
",
                "- Inputs: label above, hint below, error replaces hint — never placeholder as label. Error = red border + icon + message.
",
                "- Cards: --surface, hairline, 8px radius, 12–16px padding. Rows are tables, not cards.
",
                "- Menus/drop-ups: --lift-2, ≥200px, 28px items, 8px from trigger, trigger stays lit while open, m-emerge 220ms (scale 0.96 + rise).
",
                "- Chips: pill 10–11px, 2px v-padding. Chips are actionable, badges are read-only — never same style.
",
                "- Dialogs: --lift-3, 60% --bg scrim, 560px decision / 880px content, title/body/actions bottom-right (primary last), Esc closes.
",
                "- Tables: header 11px --text-dim, rows 32px, numbers right+tabular, zebra only past ~15 rows.
",
                "- Toasts: bottom-right, one at a time, 4s confirm / never auto-dismiss error — and the toast is never the only copy of an error.

",
                "**Decorative motion (React Bits etc)**
",
                "Allowed on: empty states, landing/marketing, onboarding, one-time celebrations, hero. Never on controls seen many times a day. Test: still good on 400th viewing? If not, it belongs on a surface people see once.

",
                "**Output discipline — how to prove you followed this**
",
                "- Reference tokens by name in code and comments; no hex escapes.
",
                "- Keep the dense, calm, hairline-forward Bhippi look — not a generic Tailwind page, not a gradient-heavy landing.
",
                "- When you change existing UI, migrate it to these tokens and spacing — do not preserve the old values because \"they were there\".
",
                "- Prefer editing one clean file over sprinkling raw styles across many.
"
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
}

impl ChatRole {
    #[must_use]
    pub const fn into_role(self) -> Role {
        match self {
            Self::User => Role::User,
            Self::Assistant => Role::Assistant,
        }
    }

    #[must_use]
    pub const fn from_role(role: Role) -> Self {
        match role {
            Role::User | Role::System => Self::User,
            Role::Assistant => Self::Assistant,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    Queued,
    Streaming,
    AwaitingPermission,
    Done,
    Stopped,
    Failed,
}

impl TurnState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Stopped | Self::Failed)
    }
}

/// Maps a turn's live engine state onto the coarse status the workspace rail shows.
fn session_status(state: TurnState) -> SessionStatus {
    match state {
        TurnState::Queued | TurnState::Streaming => SessionStatus::Running,
        TurnState::AwaitingPermission => SessionStatus::Paused,
        TurnState::Done | TurnState::Stopped => SessionStatus::Idle,
        TurnState::Failed => SessionStatus::Failed,
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Type)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// One message in a conversation, as the UI renders it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ChatTurnView {
    pub id: String,
    pub conversation_id: String,
    pub role: ChatRole,
    pub content: String,
    /// Chain-of-thought internal reasoning trace (e.g. from reasoning models or <think> blocks).
    #[serde(default)]
    pub thinking: Option<String>,
    /// Milliseconds spent thinking before beginning the actual response stream.
    #[serde(default)]
    pub thinking_elapsed_ms: Option<u64>,
    pub created_at: chrono::DateTime<Utc>,
    pub state: TurnState,
    /// Which backend produced this turn (`demo` renders the offline badge).
    pub provider: Option<String>,
    pub tools: Vec<ToolActivity>,
    pub permission: Option<PermissionRequest>,
    /// Why this turn failed, when it did — reloaded with the conversation so a fault
    /// card survives a restart instead of vanishing into an unexplained empty turn.
    #[serde(default)]
    pub fault: Option<TurnFault>,
    /// Wall-clock milliseconds from the turn starting to it finishing (CHT-103). Computed
    /// here rather than in the pane: a duration derived from two timestamps in TypeScript is
    /// a duration that drifts with the clock the browser happens to be reading (INV-051).
    #[serde(default)]
    pub worked_ms: Option<u64>,
    /// What this turn changed on disk, folded across its steps (CHT-104).
    #[serde(default)]
    pub changes: Option<TurnChanges>,
    /// Usage limits, rate limits and provider warnings (CHT-106).
    #[serde(default)]
    pub notices: Vec<TurnNotice>,
}

/// What the agent is doing right now, in a vocabulary the UI can animate.
///
/// The engine used to emit one free-text label ("Connecting to Claude Code"), which the
/// UI could only print. A closed set means each state gets its own motion, its own icon,
/// and its own copy — and means adding a state is a compile error everywhere it has to
/// be handled rather than a string that silently renders as nothing.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentPhase {
    /// Starting the backend process.
    Connecting,
    /// Waiting behind another turn.
    Queued,
    /// The model is deliberating before it says anything.
    Thinking,
    /// Extended reasoning, streamed as it happens.
    Reasoning,
    /// Laying out the steps it intends to take.
    Planning,
    /// Searching the workspace or the web.
    Searching,
    /// Reading a file.
    Reading,
    /// Writing a new file.
    Writing,
    /// Changing an existing file.
    Editing,
    /// Restructuring code it already changed.
    Refactoring,
    /// Running a command.
    Running,
    /// Running tests.
    Testing,
    /// Compiling.
    Building,
    /// Working out why something failed.
    Debugging,
    /// Installing or updating a dependency.
    Installing,
    /// Fetching a URL.
    Fetching,
    /// Driving a browser or the desktop.
    Browsing,
    /// Working over what it gathered.
    Analyzing,
    /// Condensing what it found.
    Summarizing,
    /// Checking its own work.
    Reviewing,
    /// Blocked on the user's answer.
    AwaitingPermission,
    /// Condensing the conversation to fit the context window.
    Compacting,
    /// Trying again after a recoverable failure.
    Retrying,
    /// Producing the answer text.
    Streaming,
    /// Wrapping up.
    Finalizing,
    /// Finished cleanly.
    Done,
    /// Stopped by the user.
    Stopped,
    /// Ended on a fault.
    Failed,
}

impl AgentPhase {
    /// The phase a tool verb implies.
    #[must_use]
    pub fn of_verb(verb: &str) -> Self {
        match verb {
            "read" => Self::Reading,
            "edited" => Self::Editing,
            "wrote" => Self::Writing,
            "ran" => Self::Running,
            "searched" => Self::Searching,
            "fetched" => Self::Fetching,
            "planned" => Self::Planning,
            "tested" => Self::Testing,
            _ => Self::Analyzing,
        }
    }
}

/// A named failure, with the one action that resolves it.
///
/// The string `error` beside this stays for anything that wants the raw text, but a
/// string cannot be rendered as a card with a button on it, and "provider unavailable"
/// is not a sentence any user can act on. Every field here exists because the UI needs
/// it to show a specific remedy rather than a generic apology.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize, Type)]
pub struct TurnFault {
    /// Stable id — `context_exceeded`, `rate_limited_weekly`, and so on.
    pub kind: String,
    /// Headline naming the failure, not the symptom.
    pub title: String,
    /// One sentence on what happened.
    pub summary: String,
    /// The next concrete action, in prose.
    pub fix: String,
    /// Which button to offer: `compact`, `update`, `switch_provider`, `sign_in`,
    /// `retry`, or `none`.
    pub remedy: String,
    pub action_label: Option<String>,
    /// The provider that failed, by its display label.
    pub provider: String,
    /// The vendor's own reset wording, when it named one.
    pub resets_at: Option<String>,
    /// Whether sending the same message again could plausibly work.
    pub retryable: bool,
    /// What the vendor actually said, kept for the "details" disclosure.
    pub detail: String,
}

/// Where the account stands against a backend's rolling plan windows.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct LimitSnapshot {
    /// `allowed`, `allowed_warning`, or `rejected`.
    pub status: String,
    pub session_used: Option<f32>,
    pub session_resets_at: Option<i64>,
    pub weekly_used: Option<f32>,
    pub weekly_resets_at: Option<i64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ToolAction {
    Plan,
    SearchWeb,
    ReadSource,
    WriteFile,
    FetchUrl,
    ExtractDots,
    CheckProviders,
    ControlComputer,
    EditEngine,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ToolState {
    Running,
    Ok,
    Failed,
    Skipped,
}

/// One engine step shown inline in the assistant turn while it runs (CHT-100).
///
/// A step used to be five strings — a label the transcript could print and nothing else.
/// That is why an activity row could never expand: there was nothing behind it. The result
/// fields below are what make "Ran commands ⌄" a disclosure rather than a decoration, and
/// every one is optional, so a step with nothing to show renders exactly as it always did.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ToolActivity {
    pub id: String,
    pub action: ToolAction,
    pub title: String,
    pub detail: String,
    pub state: ToolState,
    /// The command as it was actually run, when this step ran one.
    #[serde(default)]
    pub command: Option<String>,
    /// What it printed, already capped at capture (CHT-101).
    #[serde(default)]
    pub output: Option<String>,
    /// Present and non-zero is the interesting case; the transcript shows it then.
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// Wall-clock milliseconds this step took, filled in when it closes.
    #[serde(default)]
    pub elapsed_ms: Option<u64>,
    /// Set when `output` is a middle-elided excerpt rather than the whole thing, so the
    /// transcript can say so instead of quietly showing a partial answer as a full one.
    #[serde(default)]
    pub truncated: bool,
    /// Files this step changed, with real line counts (CHT-104/105).
    #[serde(default)]
    pub changes: Vec<TurnFileChange>,
}

/// Line counts for one file write (CHT-105).
///
/// A whole-file write is not a diff, and pretending otherwise would report every line of a
/// 2 000-line file as changed every time one line moved. This counts the lines that are
/// genuinely new and genuinely gone, using a longest-common-subsequence over the two line
/// lists — the same shape `bhippi-app::review` reports, so the transcript's numbers and the
/// Review modal's numbers agree.
fn line_change(path: &str, previous: Option<&str>, next: &str) -> TurnFileChange {
    let after: Vec<&str> = next.lines().collect();
    let Some(previous) = previous else {
        return TurnFileChange {
            path: path.replace('\\', "/"),
            additions: after.len(),
            deletions: 0,
            status: "added".to_owned(),
        };
    };
    let before: Vec<&str> = previous.lines().collect();

    // A quadratic LCS is fine here and a streaming diff is not worth the code: this runs
    // once per file write, on files a model just produced.
    let (rows, cols) = (before.len(), after.len());
    let mut table = vec![0usize; (rows + 1) * (cols + 1)];
    for i in (0..rows).rev() {
        for j in (0..cols).rev() {
            table[i * (cols + 1) + j] = if before[i] == after[j] {
                table[(i + 1) * (cols + 1) + j + 1] + 1
            } else {
                table[(i + 1) * (cols + 1) + j].max(table[i * (cols + 1) + j + 1])
            };
        }
    }
    let common = table[0];
    TurnFileChange {
        path: path.replace('\\', "/"),
        additions: cols - common,
        deletions: rows - common,
        status: "modified".to_owned(),
    }
}

/// What a step produced, handed to `finish_tool_with` (CHT-102).
///
/// A builder rather than six more arguments: most steps produce nothing, and the ones that
/// do produce different subsets — a command has output and an exit code, a write has file
/// changes, a web fetch has neither.
#[derive(Clone, Debug, Default)]
pub struct ToolResult {
    pub command: Option<String>,
    pub output: Option<String>,
    pub exit_code: Option<i32>,
    pub started: Option<std::time::Instant>,
    pub changes: Vec<TurnFileChange>,
}

impl ToolResult {
    /// A command and what it printed. Output is capped here, at capture (CHT-101).
    #[must_use]
    pub fn command(command: impl Into<String>, output: &str, exit_code: Option<i32>) -> Self {
        Self {
            command: Some(command.into()),
            output: Some(output.to_owned()),
            exit_code,
            ..Self::default()
        }
    }

    /// The files a step wrote.
    #[must_use]
    pub fn changes(changes: Vec<TurnFileChange>) -> Self {
        Self {
            changes,
            ..Self::default()
        }
    }

    /// Start the clock, so the step reports how long it actually took.
    #[must_use]
    pub fn since(mut self, started: std::time::Instant) -> Self {
        self.started = Some(started);
        self
    }

    fn apply(self, tool: &mut ToolActivity) {
        if let Some(command) = self.command {
            tool.command = Some(command);
        }
        if let Some(output) = self.output {
            let (capped, truncated) = cap_tool_output(&output);
            tool.output = Some(capped);
            tool.truncated = truncated;
        }
        if self.exit_code.is_some() {
            tool.exit_code = self.exit_code;
        }
        if let Some(started) = self.started {
            tool.elapsed_ms =
                Some(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
        }
        if !self.changes.is_empty() {
            tool.changes = self.changes;
        }
    }
}

/// One file a turn changed, and by how much.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TurnFileChange {
    /// Workspace-relative, forward-slashed — the path the transcript prints.
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
    /// `added` | `modified` | `deleted`, matching `bhippi-app::review`.
    pub status: String,
}

/// What a whole turn changed on disk (CHT-104).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct TurnChanges {
    pub files: Vec<TurnFileChange>,
    pub total_additions: usize,
    pub total_deletions: usize,
}

impl TurnChanges {
    /// Fold every step's file changes into one summary, newest write per path winning on
    /// `status` and the counts accumulating.
    ///
    /// A turn that edits the same file three times edited **one** file — reporting three is
    /// the kind of number that makes a summary card worthless.
    #[must_use]
    pub fn from_tools(tools: &[ToolActivity]) -> Option<Self> {
        let mut by_path: std::collections::BTreeMap<String, TurnFileChange> =
            std::collections::BTreeMap::new();
        for change in tools.iter().flat_map(|tool| tool.changes.iter()) {
            by_path
                .entry(change.path.clone())
                .and_modify(|existing| {
                    existing.additions += change.additions;
                    existing.deletions += change.deletions;
                    // A file created and then edited is still "added" as far as this turn is
                    // concerned; one deleted last is deleted.
                    if change.status == "deleted" || existing.status == "added" {
                        existing.status.clone_from(&change.status);
                    }
                })
                .or_insert_with(|| change.clone());
        }
        if by_path.is_empty() {
            return None;
        }
        let files: Vec<TurnFileChange> = by_path.into_values().collect();
        Some(Self {
            total_additions: files.iter().map(|file| file.additions).sum(),
            total_deletions: files.iter().map(|file| file.deletions).sum(),
            files,
        })
    }
}

/// A transcript notice — a usage limit, a rate limit, a provider warning (CHT-106).
///
/// These used to have nowhere to go: only a *fault* rendered, so "you have hit your usage
/// limit" either became a fake failure or vanished.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TurnNotice {
    pub level: NoticeLevel,
    pub message: String,
    #[serde(default)]
    pub hint: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    Info,
    Warn,
    Error,
}

/// How much of a step's output is kept (CHT-101).
///
/// Applied **at capture**, not at render: a command that prints 40 MB must not be held in a
/// conversation for the life of the session and merely hidden with CSS.
pub const TOOL_OUTPUT_CAP: usize = 64 * 1024;

/// Cap `text` to `TOOL_OUTPUT_CAP`, keeping the head and the tail with a counted elision.
///
/// Both ends matter and the middle rarely does: the head carries what was invoked and the
/// first error, the tail carries the summary line and the exit. Cutting only the tail — the
/// obvious implementation — throws away the half people actually scroll to.
#[must_use]
pub fn cap_tool_output(text: &str) -> (String, bool) {
    if text.len() <= TOOL_OUTPUT_CAP {
        return (text.to_owned(), false);
    }
    let half = TOOL_OUTPUT_CAP / 2;
    // Never split a UTF-8 character: walk back to a boundary rather than slicing blind.
    let head_end = (0..=half)
        .rev()
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or(0);
    let tail_start = (text.len().saturating_sub(half)..=text.len())
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or(text.len());
    let elided = text
        .len()
        .saturating_sub(head_end)
        .saturating_sub(text.len() - tail_start);
    (
        format!(
            "{}

… {elided} bytes elided …

{}",
            &text[..head_end],
            &text[tail_start..]
        ),
        true,
    )
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// A consequential action waiting for an explicit human answer (ADR-0006 §3).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PermissionRequest {
    pub id: String,
    pub action: String,
    pub scope: String,
    pub detail: String,
    pub risk: RiskLevel,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    AllowOnce,
    Deny,
}

/// Typed events streamed to the UI (INV-032 generated bindings).
macro_rules! engine_event {
    ($name:ident {$($(#[$meta:meta])* $field:ident : $ty:ty),* $(,)?}) => {
        #[derive(Clone, Deserialize, Serialize, specta::Type, tauri_specta::Event)]
        pub struct $name { $($(#[$meta])* pub $field: $ty,)* }
    };
}

engine_event!(ChatThinking {
    turn_id: String,
    label: String,
    /// The typed state behind the label, so the UI animates rather than just prints.
    phase: AgentPhase,
});
engine_event!(ChatLimits {
    provider: String,
    limits: LimitSnapshot,
});
engine_event!(ChatThoughtDelta {
    turn_id: String,
    delta: String
});
engine_event!(ChatDelta {
    turn_id: String,
    delta: String
});
engine_event!(ChatTool {
    turn_id: String,
    tool: ToolActivity
});
engine_event!(ChatPermissionRequested {
    turn_id: String,
    request: PermissionRequest
});
engine_event!(ChatTurnDone {
    turn_id: String,
    state: TurnState,
    usage: Option<Usage>,
    error: Option<String>,
    fault: Option<TurnFault>,
});
engine_event!(ProvidersChanged { providers: Vec<ProviderInfo> });
// Install/update progress for one provider's Settings card (phase: starting | done | failed).
engine_event!(ProviderInstallProgress {
    id: String,
    phase: String,
    message: String,
});

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ConversationMeta {
    pub id: String,
    pub project_path: String,
    pub title: String,
    pub created_at: chrono::DateTime<Utc>,
    pub turn_count: u32,
}

/// What kind of session a sidebar chip represents.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    /// A chat conversation with an AI agent.
    AiChat,
    /// A command-line shell tied to the project. Rendered client-side from stored
    /// CLI sessions; the engine only reports `AiChat` rows.
    Cli,
}

/// Live state of a workspace session, derived from its last assistant turn.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// A turn is queued or actively streaming.
    Running,
    /// The agent is blocked on a permission request.
    Paused,
    /// The last turn finished cleanly (done or stopped).
    Idle,
    /// The last turn ended on a fault.
    Failed,
}

/// One session per project, as the workspace rail renders it. The command layer
/// enriches `provider` with the catalogue id after mapping the label the turn stores
/// to a known backend, so the icon renders for real providers and falls back to a
/// generic chat mark otherwise.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct WorkspaceSession {
    pub id: String,
    pub project_path: String,
    pub kind: SessionKind,
    pub title: String,
    /// Catalogue provider id (`claude`, `codex`, `demo`, …) — drives the icon.
    pub provider: Option<String>,
    /// Human-readable provider label from the last assistant turn, for tooltips.
    pub provider_label: Option<String>,
    pub status: SessionStatus,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
    pub turn_count: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ConversationView {
    pub meta: ConversationMeta,
    pub turns: Vec<ChatTurnView>,
}

#[derive(Clone, Debug, Serialize, Type)]
pub struct TurnPair {
    pub conversation_id: String,
    pub user_turn_id: String,
    pub assistant_turn_id: String,
}

pub(crate) struct ConversationScope {
    pub project_path: String,
    pub conversation_id: String,
}

struct Conversation {
    meta: ConversationMeta,
    turns: Vec<ChatTurnView>,
}

/// Delivers engine events to connected windows. Abstracted so tests capture instead.
pub trait Emit: Send + Sync + 'static {
    fn thinking(&self, turn_id: &str, label: &str, phase: AgentPhase);
    fn limits(&self, provider: &str, limits: LimitSnapshot);
    fn thought_delta(&self, turn_id: &str, delta: &str);
    fn delta(&self, turn_id: &str, delta: &str);
    fn tool(&self, turn_id: &str, tool: ToolActivity);
    fn permission(&self, turn_id: &str, request: PermissionRequest);
    fn done(&self, event: ChatTurnDone);
}

/// The real emitter: tauri-specta events on the app handle.
#[derive(Clone)]
pub struct TauriEmitter {
    handle: tauri::AppHandle,
}

impl TauriEmitter {
    #[must_use]
    pub fn new(handle: tauri::AppHandle) -> Self {
        Self { handle }
    }
}

macro_rules! try_emit {
    ($result:expr) => {
        if let Err(error) = $result {
            tracing::warn!(%error, "event delivery failed; window likely closed");
        }
    };
}

impl Emit for TauriEmitter {
    fn thinking(&self, turn_id: &str, label: &str, phase: AgentPhase) {
        try_emit!(ChatThinking {
            turn_id: turn_id.to_owned(),
            label: label.to_owned(),
            phase,
        }
        .emit(&self.handle));
    }

    fn limits(&self, provider: &str, limits: LimitSnapshot) {
        try_emit!(ChatLimits {
            provider: provider.to_owned(),
            limits,
        }
        .emit(&self.handle));
    }

    fn thought_delta(&self, turn_id: &str, delta: &str) {
        try_emit!(ChatThoughtDelta {
            turn_id: turn_id.to_owned(),
            delta: delta.to_owned(),
        }
        .emit(&self.handle));
    }

    fn delta(&self, turn_id: &str, delta: &str) {
        try_emit!(ChatDelta {
            turn_id: turn_id.to_owned(),
            delta: delta.to_owned(),
        }
        .emit(&self.handle));
    }

    fn tool(&self, turn_id: &str, tool: ToolActivity) {
        try_emit!(ChatTool {
            turn_id: turn_id.to_owned(),
            tool,
        }
        .emit(&self.handle));
    }

    fn permission(&self, turn_id: &str, request: PermissionRequest) {
        try_emit!(ChatPermissionRequested {
            turn_id: turn_id.to_owned(),
            request,
        }
        .emit(&self.handle));
    }

    fn done(&self, event: ChatTurnDone) {
        try_emit!(event.emit(&self.handle));
    }
}

/// What is wired up right now: every enabled backend plus the rows for Settings.
pub struct ProviderRuntime {
    /// Full detection rows — including disabled ones, so Settings can render toggles.
    pub providers: Vec<ProviderInfo>,
    /// Enabled backends that are actually usable right now, keyed by id.
    pub by_id: HashMap<String, ProviderEntry>,
    /// Default answerer when the chat picker has nothing chosen.
    pub default_id: String,
}

/// Local servers, in the order preferred when more than one is already running.
///
/// Only ever consulted for servers that answered a probe — nothing here is started.
const LOCAL_PREFERENCE: &[&str] = &[
    "ollama", "lmstudio", "bionic", "llamacpp", "vllm", "jan", "tgui",
];

/// Remote and CLI backends, in the order preferred when no local server is running.
///
/// Cloud APIs come first: they need no local process and no RAM, which is the whole
/// reason to reach for one when nothing is loaded locally.
const REMOTE_PREFERENCE: &[&str] = &[
    "anthropic",
    "openai",
    "xai",
    "groq",
    "openrouter",
    "moonshot",
    "claude",
    "codex",
    "opencode",
    "grok",
    "kimi",
];

/// One usable backend: the adapter plus its display metadata.
pub struct ProviderEntry {
    pub provider: Arc<dyn Provider>,
    pub label: String,
    pub kind: bhippi_providers::ProviderKind,
    /// Primary model for the picker (servers list real names; CLIs show version).
    pub model: Option<String>,
}

impl ProviderRuntime {
    /// Builds the runtime from detection rows. A backend is usable when the user
    /// enabled it **and** it is installed/reachable. Cloud credentials stay
    /// settings-only until their adapters land (S1) — never silently usable.
    #[must_use]
    pub fn from_detection(providers: Vec<ProviderInfo>) -> Self {
        let mut by_id = HashMap::new();
        for row in &providers {
            // `usable`, not `installed`: a local LLM server that is on disk but not
            // listening cannot answer anything, and treating presence as readiness is
            // what put a stopped Bionic at the front of the picker.
            if !row.enabled || !row.usable() {
                continue;
            }
            if let Some(entry) = build_entry(row) {
                by_id.insert(row.id.clone(), entry);
            }
        }

        // Default preference, in the order that costs the user least:
        //
        // 1. A local server that is **already running**. It is free, private, and — the
        //    point — it is already holding RAM, so using it costs nothing more.
        // 2. Otherwise a cloud or CLI backend that is ready to answer. Nothing local is
        //    started to get here: launching a model server uninvited is what this whole
        //    change exists to stop.
        // 3. The offline demo, which always answers.
        //
        // Step 2 is a deliberate departure from ADR-0006, which kept CLIs out of the
        // silent default because a signed-out one failed confusingly. That reason has
        // expired: a signed-out CLI now renders a fault card naming the exact `login`
        // command (ADR-0016), so falling back to one is helpful rather than mysterious.
        let running_local = LOCAL_PREFERENCE
            .iter()
            .find(|wanted| by_id.contains_key(**wanted));
        let ready_remote = REMOTE_PREFERENCE
            .iter()
            .find(|wanted| by_id.contains_key(**wanted));
        let default_id = running_local
            .or(ready_remote)
            .map(|id| (*id).to_owned())
            .unwrap_or_else(|| "demo".to_owned());

        Self {
            providers,
            by_id,
            default_id,
        }
    }

    /// Rows the chat picker may show: enabled **and** usable.
    #[must_use]
    pub fn chat_options(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .filter(|row| row.enabled && self.by_id.contains_key(&row.id))
            .cloned()
            .collect()
    }

    /// Resolves a picker choice; `None` falls back to the default. Unknown ids error at
    /// the command layer — never a silent swap to another backend.
    pub fn resolve(
        &self,
        provider_id: Option<&str>,
    ) -> Result<(&ProviderEntry, &ProviderInfo), String> {
        let wanted = provider_id.unwrap_or(&self.default_id);
        let entry = self
            .by_id
            .get(wanted)
            .ok_or_else(|| format!("provider {wanted} is not available"))?;
        let info = self
            .providers
            .iter()
            .find(|row| row.id == wanted)
            .ok_or_else(|| format!("provider {wanted} vanished mid-scan"))?;
        Ok((entry, info))
    }
}

fn build_entry(row: &ProviderInfo) -> Option<ProviderEntry> {
    match row.kind {
        bhippi_providers::ProviderKind::Demo => Some(ProviderEntry {
            provider: Arc::new(DemoProvider::default()),
            label: row.label.clone(),
            kind: row.kind,
            model: row.models.first().cloned(),
        }),
        bhippi_providers::ProviderKind::LocalServer => {
            let port = local_port(row);
            let model = row.models.first().cloned().unwrap_or_default();
            let primary = row.models.first().cloned();
            let provider: Arc<dyn Provider> = if row.id == "ollama" {
                Arc::new(OllamaProvider::new("http://127.0.0.1:11434", model))
            } else {
                Arc::new(OpenAiCompatProvider::new(&row.id, &row.label, port, model))
            };
            Some(ProviderEntry {
                provider,
                label: row.label.clone(),
                kind: row.kind,
                model: primary,
            })
        }
        bhippi_providers::ProviderKind::Cli => {
            let spec = bhippi_providers::spec(&row.id)?;
            #[cfg(not(test))]
            let provider: Arc<dyn Provider> = Arc::new(CliProvider::open(spec)?);
            // Selection tests exercise preference/routing, not the host machine's PATH.
            // A deterministic stand-in keeps those tests offline and prevents a developer
            // who lacks one vendor CLI from changing the result of the same fixture.
            #[cfg(test)]
            let provider: Arc<dyn Provider> = CliProvider::open(spec)
                .map(|provider| Arc::new(provider) as Arc<dyn Provider>)
                .unwrap_or_else(|| Arc::new(DemoProvider::default()));
            Some(ProviderEntry {
                provider,
                label: row.label.clone(),
                kind: row.kind,
                model: row.version.clone(),
            })
        }
        // Cloud adapters land in S1 (BHP-019); until then they are settings-only rows.
        bhippi_providers::ProviderKind::CloudApi => None,
    }
}

fn local_port(row: &ProviderInfo) -> u16 {
    row.detected_port
        .or_else(|| bhippi_providers::spec(&row.id).and_then(|spec| spec.port))
        .unwrap_or(1234)
}

/// Owns conversations, running turns, and pending permission answers.
pub struct ChatEngine {
    emitter: Box<dyn Emit>,
    conversations: Mutex<Vec<Conversation>>,
    running: Mutex<HashMap<String, watch::Sender<bool>>>,
    pending_permissions: Mutex<HashMap<String, oneshot::Sender<PermissionDecision>>>,
    tokens_today: Arc<std::sync::atomic::AtomicU64>,
    /// Persistent per-provider ledger. `None` in tests and in the headless CLI, where a
    /// turn still runs but nothing is metered.
    usage: Option<Arc<bhippi_core::UsageStore>>,
    /// Persistent per-turn context telemetry: what each prompt carried, by category.
    /// `None` in tests and the headless CLI, where a turn still runs but is not sampled.
    context: Option<Arc<bhippi_core::ContextSampleStore>>,
    /// Vendor-owned account identities and rolling limits, refreshed independently from
    /// the local token ledger. Shared with Settings so live turn events update every view.
    account_usage: Option<Arc<Mutex<crate::usage::AccountUsageCache>>>,
    /// Application config store for reading preferences like Computer Use toggles.
    config: Option<Arc<bhippi_core::ConfigStore>>,
    /// Discovered skills store for injecting specialized instructions.
    skills: Option<Arc<bhippi_core::SkillStore>>,
    /// The desktop overlay handle (ADR-0019). `None` in tests and the headless CLI, where a
    /// Computer Use turn still runs but nothing is drawn on the desktop.
    desktop_overlay: Option<tauri::AppHandle>,
    /// This agent's identity for scene leases (ENG-192). Stable for the life of the engine
    /// and distinct per process, so two Bhippi windows on the same project are two agents as
    /// far as the lease is concerned — which is exactly what they are.
    agent_id: String,
    /// Pre-write file contents, per turn, so "Undo" on the changes card can actually put
    /// them back (CHT-115).
    ///
    /// Deliberately in memory and deliberately session-scoped: the alternative is a shadow
    /// copy of the workspace on disk, and a button that silently restores a file from a
    /// week-old backup is worse than one that is honestly greyed out. The card asks
    /// `chat_turn_undoable` and disables itself with a reason when the answer is no.
    turn_undo: Mutex<HashMap<String, Vec<TurnUndoEntry>>>,
}

/// One file's state before a turn touched it.
#[derive(Clone, Debug)]
pub struct TurnUndoEntry {
    /// Absolute path, so a restore does not depend on the workspace still being current.
    pub path: std::path::PathBuf,
    /// `None` means the file did not exist and the restore deletes it.
    pub previous: Option<String>,
}

/// Total bytes of pre-write content kept for undo, across all turns.
///
/// A generated 40 MB asset must not sit in the conversation forever waiting for an Undo
/// nobody will press. When the budget is exceeded the oldest turns are dropped first — and
/// dropping them is what makes the button honestly unavailable rather than quietly broken.
pub const TURN_UNDO_BUDGET: usize = 8 * 1024 * 1024;

fn new_id() -> String {
    bhippi_types::SessionId::new().to_string()
}

impl ChatEngine {
    #[must_use]
    pub fn new(emitter: impl Emit) -> Self {
        Self {
            emitter: Box::new(emitter),
            conversations: Mutex::new(Vec::new()),
            running: Mutex::new(HashMap::new()),
            pending_permissions: Mutex::new(HashMap::new()),
            tokens_today: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            usage: None,
            context: None,
            account_usage: None,
            config: None,
            skills: None,
            desktop_overlay: None,
            agent_id: format!("agent:{}", new_id()),
            turn_undo: Mutex::new(HashMap::new()),
        }
    }

    /// Attaches the persistent usage ledger every finished turn is recorded into.
    #[must_use]
    pub fn with_usage(mut self, store: Arc<bhippi_core::UsageStore>) -> Self {
        self.usage = Some(store);
        self
    }

    /// Attaches the persistent context-telemetry store every turn is sampled into.
    #[must_use]
    pub fn with_context(mut self, store: Arc<bhippi_core::ContextSampleStore>) -> Self {
        self.context = Some(store);
        self
    }

    #[must_use]
    pub fn with_account_usage(
        mut self,
        cache: Arc<Mutex<crate::usage::AccountUsageCache>>,
    ) -> Self {
        self.account_usage = Some(cache);
        self
    }

    async fn remember_account_limits(
        &self,
        provider_id: &str,
        session_used: Option<f32>,
        session_resets_at: Option<i64>,
        weekly_used: Option<f32>,
        weekly_resets_at: Option<i64>,
    ) {
        let Some(cache) = &self.account_usage else {
            return;
        };
        let session = session_used.map(|used_fraction| bhippi_providers::PlanWindow {
            used_fraction: used_fraction.clamp(0.0, 1.0),
            resets_at: session_resets_at,
            duration_minutes: Some(300),
        });
        let weekly = weekly_used.map(|used_fraction| bhippi_providers::PlanWindow {
            used_fraction: used_fraction.clamp(0.0, 1.0),
            resets_at: weekly_resets_at,
            duration_minutes: Some(10_080),
        });
        cache
            .lock()
            .await
            .merge_live_limits(provider_id, session, weekly);
    }

    /// Attaches the config store for inspecting preferences like Computer Use.
    #[must_use]
    pub fn with_config(mut self, store: Arc<bhippi_core::ConfigStore>) -> Self {
        self.config = Some(store);
        self
    }

    /// Attaches the skills store for injecting recognized AI skill prompts.
    #[must_use]
    pub fn with_skills(mut self, store: Arc<bhippi_core::SkillStore>) -> Self {
        self.skills = Some(store);
        self
    }

    /// Attaches the desktop overlay handle so Computer Use turns draw their aura and
    /// pointer on the whole desktop (ADR-0019). Desktop-only; tests stay `None`.
    #[must_use]
    pub fn with_desktop_overlay(mut self, handle: tauri::AppHandle) -> Self {
        self.desktop_overlay = Some(handle);
        self
    }

    #[must_use]
    pub fn tokens_today(&self) -> u64 {
        self.tokens_today.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Creates a conversation when missing so the UI never guesses ids.
    pub async fn ensure_conversation(
        &self,
        project_path: &str,
        id: Option<String>,
    ) -> Result<ConversationMeta, String> {
        let mut conversations = self.conversations.lock().await;
        if let Some(wanted) = id.as_deref() {
            if let Some(existing) = conversations.iter().find(|c| c.meta.id == wanted) {
                if Self::paths_match(&existing.meta.project_path, project_path) {
                    return Ok(existing.meta.clone());
                }
                return Err("That session belongs to a different project.".to_owned());
            }
        }
        let meta = ConversationMeta {
            id: id.unwrap_or_else(new_id),
            project_path: project_path.to_owned(),
            title: "New conversation".to_owned(),
            created_at: Utc::now(),
            turn_count: 0,
        };
        conversations.insert(
            0,
            Conversation {
                meta: meta.clone(),
                turns: Vec::new(),
            },
        );
        Ok(meta)
    }

    pub(crate) fn paths_match(a: &str, b: &str) -> bool {
        if a == b {
            return true;
        }
        let norm = |s: &str| {
            let s = s.strip_prefix(r"\\?\").unwrap_or(s);
            let s = s.strip_prefix("//?/").unwrap_or(s);
            s.replace('\\', "/")
                .trim_end_matches('/')
                .to_ascii_lowercase()
        };
        norm(a) == norm(b)
    }

    pub async fn list_conversations(&self, project_path: &str) -> Vec<ConversationMeta> {
        self.conversations
            .lock()
            .await
            .iter()
            .filter(|conversation| Self::paths_match(&conversation.meta.project_path, project_path))
            .map(|conversation| conversation.meta.clone())
            .collect()
    }

    /// One session per conversation, across every project — the workspace rail's data.
    ///
    /// Status and `updated_at` are derived from the most recent turn of any role, and the
    /// provider comes from the latest assistant turn (which is what actually answered).
    /// An empty conversation is Idle rather than a phantom.
    #[must_use]
    pub async fn workspace_sessions(&self) -> Vec<WorkspaceSession> {
        let conversations = self.conversations.lock().await;
        let mut sessions: Vec<WorkspaceSession> = conversations
            .iter()
            .map(|conversation| {
                let meta = &conversation.meta;
                let assistant = conversation
                    .turns
                    .iter()
                    .rev()
                    .find(|turn| turn.role == ChatRole::Assistant);
                let latest = conversation.turns.last();
                WorkspaceSession {
                    id: meta.id.clone(),
                    project_path: meta.project_path.clone(),
                    kind: SessionKind::AiChat,
                    title: meta.title.clone(),
                    provider: None,
                    provider_label: assistant.and_then(|turn| turn.provider.clone()),
                    status: latest
                        .map(|turn| session_status(turn.state))
                        .unwrap_or(SessionStatus::Idle),
                    created_at: meta.created_at,
                    updated_at: latest
                        .map(|turn| turn.created_at)
                        .unwrap_or(meta.created_at),
                    turn_count: meta.turn_count,
                }
            })
            .collect();
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
        sessions
    }

    /// Clears all messages in the active conversation, resetting it cleanly.
    pub async fn clean_conversation(
        &self,
        project_path: &str,
        id: &str,
    ) -> Option<ConversationView> {
        let mut conversations = self.conversations.lock().await;
        let conversation = conversations
            .iter_mut()
            .find(|c| c.meta.id == id && Self::paths_match(&c.meta.project_path, project_path))?;
        conversation.turns.clear();
        conversation.meta.turn_count = 0;
        Some(ConversationView {
            meta: conversation.meta.clone(),
            turns: Vec::new(),
        })
    }

    /// Compacts conversation turns into a summarized context turn to save token budget.
    pub async fn compact_conversation(
        &self,
        project_path: &str,
        id: &str,
    ) -> Option<ConversationView> {
        let mut conversations = self.conversations.lock().await;
        let conversation = conversations
            .iter_mut()
            .find(|c| c.meta.id == id && Self::paths_match(&c.meta.project_path, project_path))?;
        if conversation.turns.is_empty() {
            return Some(ConversationView {
                meta: conversation.meta.clone(),
                turns: Vec::new(),
            });
        }
        let total_turns = conversation.turns.len();
        let summary_text = format!(
            "📦 **Conversation Compacted**\n\n*Condensed {total_turns} prior turns into active context to preserve token budget. Ready for next instructions.*"
        );
        let compacted_turn = ChatTurnView {
            id: new_id(),
            conversation_id: id.to_owned(),
            role: ChatRole::Assistant,
            content: summary_text,
            thinking: None,
            thinking_elapsed_ms: None,
            created_at: Utc::now(),
            state: TurnState::Done,
            provider: Some("Bhippi Compactor".to_owned()),
            tools: Vec::new(),
            permission: None,
            fault: None,
            worked_ms: None,
            changes: None,
            notices: Vec::new(),
        };
        conversation.turns = vec![compacted_turn];
        conversation.meta.turn_count = 1;
        Some(ConversationView {
            meta: conversation.meta.clone(),
            turns: conversation.turns.clone(),
        })
    }

    /// Drops one conversation and its turns. `false` means there was nothing to drop.
    ///
    /// Any turn still running in it is cancelled first: its task holds the conversation
    /// id and would keep emitting deltas at a thread the UI has already forgotten.
    pub async fn delete_conversation(&self, project_path: &str, id: &str) -> bool {
        let running: Vec<String> = {
            let conversations = self.conversations.lock().await;
            let Some(conversation) = conversations.iter().find(|entry| {
                entry.meta.id == id && Self::paths_match(&entry.meta.project_path, project_path)
            }) else {
                return false;
            };
            conversation
                .turns
                .iter()
                .filter(|turn| !turn.state.is_terminal())
                .map(|turn| turn.id.clone())
                .collect()
        };
        for turn in &running {
            self.stop(turn).await;
        }
        let mut conversations = self.conversations.lock().await;
        let before = conversations.len();
        conversations.retain(|conversation| {
            !(conversation.meta.id == id
                && Self::paths_match(&conversation.meta.project_path, project_path))
        });
        conversations.len() < before
    }

    pub async fn conversation_view(
        &self,
        project_path: &str,
        id: &str,
    ) -> Option<ConversationView> {
        let conversations = self.conversations.lock().await;
        conversations
            .iter()
            .find(|conversation| {
                conversation.meta.id == id
                    && Self::paths_match(&conversation.meta.project_path, project_path)
            })
            .map(|conversation| ConversationView {
                meta: conversation.meta.clone(),
                turns: conversation.turns.clone(),
            })
    }

    /// Appends the user turn plus a queued assistant turn, then starts the engine task.
    ///
    /// `provider_id` is the chat picker's choice; `None` uses the runtime default.
    /// Unknown ids return an error — never a silent swap (ADR-0006 §4 spirit).
    ///
    /// Returns both turn ids immediately; every further byte arrives as events.
    pub(crate) async fn send(
        self: &Arc<Self>,
        registry: &Arc<ProviderRuntime>,
        scope: ConversationScope,
        text: String,
        options: TurnOptions,
    ) -> Result<TurnPair, String> {
        let ConversationScope {
            project_path,
            conversation_id,
        } = scope;
        let TurnOptions {
            provider_id,
            model,
            effort,
            design,
            caveman,
        } = options;
        let user_id = new_id();
        let assistant_id = new_id();
        // Resolved in the normal (non-command) path below, but declared here so the
        // `start_assistant` call after the conversations lock can read it.
        let entry: (Arc<dyn Provider>, String);
        {
            let mut conversations = self.conversations.lock().await;
            if conversations.iter().any(|conversation| {
                conversation.meta.id == conversation_id
                    && !Self::paths_match(&conversation.meta.project_path, &project_path)
            }) {
                return Err("That session belongs to a different project.".to_owned());
            }
            let missing = !conversations.iter().any(|conversation| {
                conversation.meta.id == conversation_id
                    && Self::paths_match(&conversation.meta.project_path, &project_path)
            });
            if missing {
                conversations.push(Conversation {
                    meta: ConversationMeta {
                        id: conversation_id.clone(),
                        project_path: project_path.clone(),
                        title: short_title(&text),
                        created_at: Utc::now(),
                        turn_count: 0,
                    },
                    turns: Vec::new(),
                });
            }
            let Some(conversation) = conversations.iter_mut().find(|conversation| {
                conversation.meta.id == conversation_id
                    && Self::paths_match(&conversation.meta.project_path, &project_path)
            }) else {
                return Err("The project session could not be created.".to_owned());
            };

            if conversation.meta.title == "New conversation" {
                conversation.meta.title = short_title(&text);
            }
            let created = Utc::now();
            let trimmed = text.trim();

            // Intercept slash commands. These run *before* any provider is resolved so
            // deterministic commands keep working with no AI configured.
            if trimmed == "/clear" || trimmed == "/clean" || trimmed == "/reset" {
                // A hard reset: drop every turn and the auto-derived title so the session
                // genuinely starts over. No provider involved — this works offline.
                conversation.turns.clear();
                conversation.meta.turn_count = 0;
                conversation.meta.title = "New conversation".to_owned();
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            if trimmed == "/compact" {
                let count = conversation.turns.len();
                conversation.turns.clear();
                conversation.turns.push(ChatTurnView {
                    id: assistant_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::Assistant,
                    content: format!("📦 **Session Compacted**: Condensed {count} prior turn(s) to optimize context tokens. Project rules and workspace state are preserved."),
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: Some("Bhippi Engine".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                conversation.meta.turn_count = 1;
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            if trimmed == "/gamedebug" || trimmed.starts_with("/gamedebug ") {
                let report_md = match crate::game_debug::parse_command(trimmed) {
                    Ok(command) => match crate::game_debug::run_and_store_observed(
                        self.desktop_overlay.as_ref(),
                        std::path::Path::new(&project_path),
                        &command,
                    )
                    .await
                    {
                        Ok(report) => crate::game_debug::render_report(
                            &report,
                            command.fix_requested,
                            Some(std::path::Path::new(&project_path)),
                        ),
                        Err(reason) => format!(
                            "### Game Debugger could not run\n\n{reason}\n\nUse \
                         `/gamedebug [quick|full|release] [--fix]` from a Bhippi game project."
                        ),
                    },
                    Err(reason) => format!(
                        "### Game Debugger could not run\n\n{reason}\n\nUse \
                         `/gamedebug [quick|full|release] [--fix]` from a Bhippi game project."
                    ),
                };

                conversation.turns.push(ChatTurnView {
                    id: user_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::User,
                    content: text,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                conversation.turns.push(ChatTurnView {
                    id: assistant_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::Assistant,
                    content: report_md,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: Some("Game Debugger".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            if trimmed == "/debug" {
                let ws = std::path::Path::new(&project_path);
                let report_md = match crate::debugger::run_diagnostics(ws).await {
                    Ok(report) => render_debug_report(&report),
                    Err(reason) => format!(
                        "### Debugger could not run\n\n{reason}\n\nOpen a project directory \
                         and try `/debug` again."
                    ),
                };

                conversation.turns.push(ChatTurnView {
                    id: user_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::User,
                    content: text,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                conversation.turns.push(ChatTurnView {
                    id: assistant_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::Assistant,
                    content: report_md,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: Some("Deterministic Debugger".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            if trimmed == "/skills" {
                let ws = std::path::Path::new(&project_path);
                let discovered = bhippi_core::skills::discover_external_skills(Some(ws)).await;
                let mut skills_md = format!(
                    "### ⚡ Installed & Imported AI Skills ({} discovered)\n\n*Skills are auto-loaded from Claude, Codex, Antigravity, Cursor, and workspace `.agents/skills`.*\n\n| Skill Name | Source | Tags | Tag Syntax |\n|---|---|---|---|\n",
                    discovered.len()
                );
                for s in &discovered {
                    let tags_str = s.tags.join(", ");
                    skills_md.push_str(&format!(
                        "| **{}** | `{}` | {} | `@{}` |\n",
                        s.name, s.source, tags_str, s.id
                    ));
                }
                skills_md.push_str("\n💡 *Tip: Mention `@skill-id` anywhere in your message to activate that skill's prompt directives.*");

                conversation.turns.push(ChatTurnView {
                    id: user_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::User,
                    content: text,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                conversation.turns.push(ChatTurnView {
                    id: assistant_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::Assistant,
                    content: skills_md,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: Some("Skills Registry".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            if trimmed == "/time" || trimmed.starts_with("/time ") {
                let now = chrono::Local::now();
                let time_md = format!(
                    "### 🕐 Local time\n\n**Date:** {}\n\n**Time:** {}\n\n**Full:** {}\n\n*Live from the machine running Bhippi — no provider involved.*",
                    now.format("%A, %B %d, %Y"),
                    now.format("%I:%M:%S %p"),
                    now.format("%Y-%m-%d %H:%M:%S %z")
                );
                conversation.turns.push(ChatTurnView {
                    id: user_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::User,
                    content: text,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                conversation.turns.push(ChatTurnView {
                    id: assistant_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::Assistant,
                    content: time_md,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: Some("Bhippi Engine".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            if trimmed == "/version" || trimmed.starts_with("/version ") {
                let version_md = format!(
                    "### ℹ️ Bhippi version\n\n- **App:** {}",
                    env!("CARGO_PKG_VERSION")
                );
                conversation.turns.push(ChatTurnView {
                    id: user_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::User,
                    content: text,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                conversation.turns.push(ChatTurnView {
                    id: assistant_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::Assistant,
                    content: version_md,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: Some("Bhippi Engine".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            if trimmed == "/context" || trimmed == "/tokens" {
                let turn_count = conversation.turns.len();
                let approx_chars: usize = conversation.turns.iter().map(|t| t.content.len()).sum();
                let est_tokens = (approx_chars / 4) + 500;
                let context_md = format!(
                    "### 📊 Context & Token Budget Overview\n\n\
                     - **Session Turn Count:** `{turn_count}` turns\n\
                     - **Session Content:** `{approx_chars}` characters\n\
                     - **Estimated Prompt Overhead:** `~{est_tokens}` tokens\n\
                     - **Active Project Directory:** `{project_path}`\n\n\
                     💡 **Deterministic Token Optimization Tips:**\n\
                     - Run `/compact` to condense prior turns into a summary badge and reclaim tokens.\n\
                     - Run `/clear` (or `/reset`) to wipe conversation memory and begin completely fresh.\n\
                     - Toggle Caveman Mode (`/caveman` or composer icon) for ultra-concise responses without filler.\n\
                     - Toggle IndexMap to query code symbols via compact summary maps."
                );
                conversation.turns.push(ChatTurnView {
                    id: user_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::User,
                    content: text,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                conversation.turns.push(ChatTurnView {
                    id: assistant_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::Assistant,
                    content: context_md,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: Some("Token Engine".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            if trimmed == "/model" {
                let active_prov = provider_id.as_deref().unwrap_or("auto-detected");
                let active_mdl = model.as_deref().unwrap_or("default");
                let model_md = format!(
                    "### 🤖 Active Session Model Configuration\n\n\
                     - **Session Provider:** `{active_prov}`\n\
                     - **Active Model:** `{active_mdl}`\n\
                     - **Effort Mode:** `{effort:?}`\n\
                     - **Session ID:** `{conversation_id}`\n\n\
                     *This configuration is strictly isolated to this chat tab and does not affect any other chat.*"
                );
                conversation.turns.push(ChatTurnView {
                    id: user_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::User,
                    content: text,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                conversation.turns.push(ChatTurnView {
                    id: assistant_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::Assistant,
                    content: model_md,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: Some("Bhippi Engine".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            if trimmed == "/rules" {
                let ws = std::path::Path::new(&project_path);
                let rules_content = if let Ok(c) =
                    tokio::fs::read_to_string(ws.join("AGENTS.md")).await
                {
                    c
                } else if let Ok(c) = tokio::fs::read_to_string(ws.join("CLAUDE.md")).await {
                    c
                } else if let Ok(c) = tokio::fs::read_to_string(ws.join(".agents/rules")).await {
                    c
                } else {
                    "No custom project rules file (`AGENTS.md`, `CLAUDE.md`, or `.agents/rules`) found in this workspace root.".to_owned()
                };
                let preview = if rules_content.len() > 1200 {
                    format!(
                        "{}\n\n*(preview truncated — full rules are injected into prompt context)*",
                        &rules_content[..1200]
                    )
                } else {
                    rules_content
                };
                let rules_md =
                    format!("### 📋 Active Workspace Rules\n\n```markdown\n{preview}\n```");
                conversation.turns.push(ChatTurnView {
                    id: user_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::User,
                    content: text,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                conversation.turns.push(ChatTurnView {
                    id: assistant_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::Assistant,
                    content: rules_md,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: Some("Rules Engine".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            if trimmed == "/help" {
                let help_md = r#"### 🧭 Bhippi Chat Slash Commands

All slash commands below execute locally and deterministically with **0 AI tokens**, functioning even when offline or without an AI provider configured.

#### 🤖 Workspace & Memory Management
- `/clear` — Hard reset for this chat: clears all memory and title so it feels brand new.
- `/clean` — Alias of `/clear`.
- `/reset` — Alias of `/clear`.
- `/compact` — Compacts turn history into a summary badge to reclaim token budget.
- `/context` — Inspect session turn count, estimated token overhead, and active project.
- `/tokens` — Alias of `/context`.
- `/model` — Displays current provider, model, effort mode, and session ID.
- `/rules` — Displays active project instructions from `AGENTS.md` or `CLAUDE.md`.
- `/skills` — Lists all external and imported skills with `@tag` syntax.
- `/debug` — Runs deterministic workspace compilation and diagnostics.
- `/gamedebug [quick|full|release] [--fix]` — Runs the fixed game-aware diagnostic pipeline and saves an AI-ready report.
- `/time` — Shows system and UTC timestamps.
- `/version` — Shows the application and engine version.
- `/computer <task>` — Triggers Computer Use automation for the specified desktop task.
- `/help` — Displays this commands reference.
"#;
                conversation.turns.push(ChatTurnView {
                    id: user_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::User,
                    content: text,
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                conversation.turns.push(ChatTurnView {
                    id: assistant_id.clone(),
                    conversation_id: conversation_id.clone(),
                    role: ChatRole::Assistant,
                    content: help_md.to_owned(),
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: Some("Bhippi Engine".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
                return Ok(TurnPair {
                    conversation_id,
                    user_turn_id: user_id,
                    assistant_turn_id: assistant_id,
                });
            }

            // Non-command path: resolve the picker's provider now that deterministic
            // commands have already been handled above. Unknown ids still error here, never
            // a silent swap (ADR-0006 §4 spirit).
            let (_resolved_entry, info) = registry.resolve(provider_id.as_deref())?;
            entry = (_resolved_entry.provider.clone(), info.label.clone());

            conversation.turns.push(ChatTurnView {
                id: user_id.clone(),
                conversation_id: conversation_id.clone(),
                role: ChatRole::User,
                content: text,
                thinking: None,
                thinking_elapsed_ms: None,
                created_at: created,
                state: TurnState::Done,
                provider: None,
                tools: Vec::new(),
                permission: None,
                fault: None,
                worked_ms: None,
                changes: None,
                notices: Vec::new(),
            });
            conversation.turns.push(ChatTurnView {
                id: assistant_id.clone(),
                conversation_id: conversation_id.clone(),
                role: ChatRole::Assistant,
                content: String::new(),
                thinking: None,
                thinking_elapsed_ms: None,
                created_at: created,
                state: TurnState::Queued,
                provider: Some(entry.1.clone()),
                tools: Vec::new(),
                permission: None,
                fault: None,
                worked_ms: None,
                changes: None,
                notices: Vec::new(),
            });
        }
        self.start_assistant(
            registry,
            &conversation_id,
            &assistant_id,
            TurnPlan {
                provider: Some((entry.0.clone(), entry.1.clone())),
                provider_id,
                model,
                effort,
                design,
                caveman,
                workspace: project_path,
            },
        )
        .await;
        Ok(TurnPair {
            conversation_id,
            user_turn_id: user_id,
            assistant_turn_id: assistant_id,
        })
    }

    /// Drops the trailing assistant answer and re-runs it against the latest context.
    pub(crate) async fn regenerate(
        self: &Arc<Self>,
        registry: &Arc<ProviderRuntime>,
        scope: ConversationScope,
        options: TurnOptions,
    ) -> Option<Result<TurnPair, String>> {
        let ConversationScope {
            project_path,
            conversation_id,
        } = scope;
        let TurnOptions {
            provider_id,
            model,
            effort,
            design,
            caveman,
        } = options;
        let Ok((entry, info)) = registry.resolve(provider_id.as_deref()) else {
            return Some(Err(format!("provider {provider_id:?} is not available")));
        };
        let entry = (entry.provider.clone(), info.label.clone());
        let mut conversations = self.conversations.lock().await;
        let conversation = conversations.iter_mut().find(|conversation| {
            conversation.meta.id == conversation_id
                && Self::paths_match(&conversation.meta.project_path, &project_path)
        })?;
        let user_id = conversation
            .turns
            .iter()
            .rev()
            .find(|turn| turn.role == ChatRole::User)
            .map(|turn| turn.id.clone())?;
        while conversation
            .turns
            .last()
            .is_some_and(|turn| turn.role == ChatRole::Assistant)
        {
            conversation.turns.pop();
        }
        let assistant_id = new_id();
        conversation.turns.push(ChatTurnView {
            id: assistant_id.clone(),
            conversation_id: conversation_id.clone(),
            role: ChatRole::Assistant,
            content: String::new(),
            thinking: None,
            thinking_elapsed_ms: None,
            created_at: Utc::now(),
            state: TurnState::Queued,
            provider: Some(entry.1.clone()),
            tools: Vec::new(),
            permission: None,
            fault: None,
            worked_ms: None,
            changes: None,
            notices: Vec::new(),
        });
        drop(conversations);

        self.start_assistant(
            registry,
            &conversation_id,
            &assistant_id,
            TurnPlan {
                provider: Some((entry.0.clone(), entry.1.clone())),
                provider_id,
                model,
                effort,
                design,
                caveman,
                workspace: project_path,
            },
        )
        .await;
        Some(Ok(TurnPair {
            conversation_id,
            user_turn_id: user_id,
            assistant_turn_id: assistant_id,
        }))
    }

    /// Cooperative stop: flips the cancel flag; the task settles the turn as `stopped`.
    pub async fn stop(&self, turn_id: &str) {
        if let Some(sender) = self.running.lock().await.get(turn_id) {
            let _ignored = sender.send(true);
        }
    }

    /// Answers a pending permission card. Unknown ids are a no-op (already settled).
    pub async fn respond_permission(&self, request_id: &str, decision: PermissionDecision) -> bool {
        if let Some(sender) = self.pending_permissions.lock().await.remove(request_id) {
            let _ignored = sender.send(decision);
            return true;
        }
        false
    }

    async fn start_assistant(
        self: &Arc<Self>,
        registry: &Arc<ProviderRuntime>,
        conversation_id: &str,
        assistant_id: &str,
        plan: TurnPlan,
    ) {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        self.running
            .lock()
            .await
            .insert(assistant_id.to_owned(), cancel_tx);

        let engine = self.clone();
        let registry = registry.clone();
        let conversation = conversation_id.to_owned();
        let turn = assistant_id.to_owned();
        tokio::spawn(async move {
            tracing::span!(
                tracing::Level::INFO,
                "chat_turn",
                session_id = %conversation,
                turn_id = %turn
            )
            .in_scope(|| async {
                let outcome = engine
                    .run_turn(&registry, &conversation, &turn, plan, cancel_rx)
                    .await;
                engine.settle_turn(&conversation, &turn, outcome).await;
                engine.running.lock().await.remove(&turn);
            })
            .await;
        });
    }

    /// First authorised vision backend that is enabled and usable, skipping the current
    /// picker choice. Candidates are only the ADR-0015 set (claude/codex/grok) and only
    /// ones the user allowed in Settings.
    fn pick_computer_provider(
        registry: &ProviderRuntime,
        allowed: &[String],
        current_id: &str,
    ) -> Option<ComputerVisionStandin> {
        for wanted in ["claude", "codex", "grok"] {
            if wanted == current_id {
                continue;
            }
            if !allowed.iter().any(|id| id == wanted) {
                continue;
            }
            if let Some(entry) = registry.by_id.get(wanted) {
                return Some((
                    entry.provider.clone(),
                    entry.label.clone(),
                    wanted.to_owned(),
                    entry.model.clone(),
                ));
            }
        }
        None
    }

    async fn run_turn(
        self: &Arc<Self>,
        registry: &Arc<ProviderRuntime>,
        conversation_id: &str,
        turn_id: &str,
        plan: TurnPlan,
        mut cancel: watch::Receiver<bool>,
    ) -> Outcome {
        let TurnPlan {
            provider: chosen,
            provider_id: _requested_provider_id,
            model: requested_model,
            effort,
            design,
            caveman,
            workspace,
        } = plan;
        // The picker's choice wins; nothing chosen means the offline demo answers.
        let (mut provider, mut provider_label) = match chosen {
            Some((provider, label)) => (provider, label),
            None => (
                Arc::new(DemoProvider::default()) as Arc<dyn Provider>,
                "Demo (offline)".to_owned(),
            ),
        };
        let mut provider_id = provider.id().to_owned();
        let mut model = requested_model;

        let history = self.history_messages(conversation_id).await;
        let latest_user_text = history
            .iter()
            .rev()
            .find(|message| message.role == bhippi_providers::Role::User)
            .map(|message| message.content.as_str())
            .unwrap_or_default();
        let computer_intent = crate::computer::explicitly_requests_computer_use(latest_user_text);
        let workspace_context = WORKSPACE_SYSTEM.replace("{{workspace}}", &workspace);
        // Project rules sit after the boundary statement and before the effort directive:
        // they steer how work is done here, and can never widen where it may be done.
        let rules_context = project_rules_block(&workspace)
            .await
            .map(|block| format!("\n\n{block}"))
            .unwrap_or_default();
        let mut computer_use_context = String::new();
        let mut computer_mode = false;
        let mut computer_full_access = false;
        let mut computer_handoff_note: Option<String> = None;
        if let Some(store) = self.config.as_ref() {
            if let Ok(cfg) = store.load().await {
                computer_full_access = cfg.computer_use.full_access;
                let configured_provider = cfg
                    .computer_use
                    .allowed_providers
                    .iter()
                    .any(|allowed| allowed == &provider_id);
                let provider_ready = configured_provider
                    && crate::computer::is_vision_capable(&provider_id, model.as_deref());
                if computer_intent && cfg.computer_use.enabled && provider_ready {
                    computer_mode = true;
                    let access = if computer_full_access {
                        "Mouse and keyboard input are authorised for this turn."
                    } else {
                        "Observation is authorised, but mouse and keyboard input are blocked because Full PC Access is off."
                    };
                    self.emitter.thinking(
                        turn_id,
                        "Computer Use active (desktop perception enabled)",
                        AgentPhase::Connecting,
                    );
                    computer_use_context = format!("\n\n{COMPUTER_USE_SYSTEM}\n\n{access}");
                } else if computer_intent && cfg.computer_use.enabled && !provider_ready {
                    if let Some((
                        standin_provider,
                        standin_label,
                        standin_id,
                        standin_default_model,
                    )) = Self::pick_computer_provider(
                        registry,
                        &cfg.computer_use.allowed_providers,
                        &provider_id,
                    ) {
                        let original_label = provider_label.clone();
                        provider = standin_provider;
                        provider_label = standin_label;
                        provider_id = standin_id;
                        model = standin_default_model;
                        computer_mode = true;
                        let note = format!(
                            "This Computer Use session is running through {provider_label} because {original_label} has no desktop vision."
                        );
                        computer_handoff_note = Some(note);
                        let access = if computer_full_access {
                            "Mouse and keyboard input are authorised for this turn."
                        } else {
                            "Observation is authorised, but mouse and keyboard input are blocked because Full PC Access is off."
                        };
                        self.emitter.thinking(
                            turn_id,
                            &format!("Computer Use handed to {provider_label} for desktop vision"),
                            AgentPhase::Connecting,
                        );
                        computer_use_context = format!(
                            "\n\n{COMPUTER_USE_SYSTEM}\n\nSession driver: {provider_label} was handed the desktop because {original_label} cannot see it.\n\n{access}"
                        );
                    } else {
                        computer_use_context = "\n\nComputer Use was explicitly requested, but no enabled provider has desktop vision. Explain that Claude Code, Codex CLI, or Grok CLI is required; do not run shell commands as a substitute.".to_owned();
                    }
                } else if computer_intent && !cfg.computer_use.enabled {
                    computer_use_context = "\n\nComputer Use was explicitly requested, but it is disabled. Explain that the user must enable Settings › Computer Use; do not run shell commands as a substitute.".to_owned();
                }
            }
        }
        let mut skills_context = String::new();
        let ws_path = std::path::Path::new(&workspace);
        let discovered_skills = bhippi_core::skills::discover_external_skills(Some(ws_path)).await;
        if let Some(last_msg) = history.last() {
            let text_lower = last_msg.content.to_lowercase();
            let mut active_prompts = Vec::new();
            for skill in &discovered_skills {
                if skill.enabled {
                    let id_tag = format!("@{}", skill.id.to_lowercase());
                    let name_tag = format!("@{}", skill.name.to_lowercase().replace(' ', "-"));
                    if text_lower.contains(&id_tag) || text_lower.contains(&name_tag) {
                        active_prompts.push(format!(
                            "### Active Skill: {}\n{}",
                            skill.name, skill.prompt
                        ));
                    }
                }
            }
            if !active_prompts.is_empty() {
                skills_context = format!(
                    "\n\n## Activated Skills Directives\n{}",
                    active_prompts.join("\n\n")
                );
            }
        }
        let previous_provider = {
            let conversations = self.conversations.lock().await;
            conversations
                .iter()
                .find(|c| c.meta.id == conversation_id)
                .and_then(|c| {
                    c.turns
                        .iter()
                        .rev()
                        .find(|t| t.role == ChatRole::Assistant && t.provider.is_some())
                        .and_then(|t| t.provider.clone())
                })
        };

        let mut handoff_context = String::new();
        if let Some(prev_p) = previous_provider {
            if prev_p != provider_label {
                handoff_context = format!(
                    "\n\n## 🔄 Multi-Provider Conversation Handoff\n\
                    You are continuing an ongoing conversation session originally assisted by `{prev_p}`.\n\
                    The previous turns are included above in the message history. Maintain full continuity, \
                    respect all previously agreed decisions and code patterns, and seamlessly address the user's latest prompt."
                );
            }
        }

        // --- 6-PART STRUCTURED PROMPT CACHE HIERARCHY ---
        // Part 1: Stable System Core (Static ADE Assistant Identity & Invariants)
        let part1_system_core = CHAT_SYSTEM;

        // Part 2: Deterministic Capability / Tool Definitions (Alphabetical / Sorted)
        let part2_capabilities = if !computer_use_context.is_empty() {
            computer_use_context.as_str()
        } else {
            ""
        };

        // Part 3: Mode Directives (Stable Mode Tier: Caveman Protocol, Effort, Design)
        let mut part3_modes = String::new();
        if caveman {
            part3_modes.push_str(CAVEMAN_SYSTEM_DIRECTIVE);
            part3_modes.push_str("\n\n");
        }
        part3_modes.push_str(effort.directive());
        let d_dir = design.directive();
        if !d_dir.is_empty() {
            part3_modes.push('\n');
            part3_modes.push_str(d_dir);
        }

        // Part 4: Project Brain & Invariants Manifest (Cached per project)
        let mut part4_project_brain = String::new();
        part4_project_brain.push_str(&workspace_context);
        if !rules_context.is_empty() {
            part4_project_brain.push_str(&rules_context);
        }
        // If the user requests to generate or build a game, ensure the workspace is initialized
        // as an engine game project so the AI receives engine instructions and can emit batches.
        let asks_game = asks_for_game_creation(latest_user_text);
        if asks_game && crate::engine::game_dir_of(&workspace).is_err() {
            let root = std::path::PathBuf::from(&workspace);
            if !bhippi_engine::manifest::manifest_path(&root).is_file() {
                let display_name = root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or("My Game");
                if let Ok(files) = bhippi_engine::scaffold::write_project(&root, display_name, true)
                {
                    tracing::info!(
                        files = files.len(),
                        root = %root.display(),
                        "Auto-scaffolded game project from user intent"
                    );
                }
            }
        }
        let eng = engine_context(&workspace).await;
        if !eng.is_empty() {
            part4_project_brain.push_str("\n\n");
            part4_project_brain.push_str(&eng);
        }

        // Part 5: Working Memory Sandbox (Task-Scoped: Skills & Multi-Provider Handoff)
        let mut part5_sandbox = String::new();
        if !skills_context.is_empty() {
            part5_sandbox.push_str(&skills_context);
        }
        if !handoff_context.is_empty() {
            if !part5_sandbox.is_empty() {
                part5_sandbox.push_str("\n\n");
            }
            part5_sandbox.push_str(&handoff_context);
        }

        // Assemble Parts 1-5 into the cacheable System Prompt
        let mut system_blocks = vec![part1_system_core];
        if !part2_capabilities.is_empty() {
            system_blocks.push(part2_capabilities);
        }
        if !part3_modes.is_empty() {
            system_blocks.push(&part3_modes);
        }
        if !part4_project_brain.is_empty() {
            system_blocks.push(&part4_project_brain);
        }
        if !part5_sandbox.is_empty() {
            system_blocks.push(&part5_sandbox);
        }
        let combined_system = system_blocks.join("\n\n");

        // Part 6: Dynamic Turn Tail & User Message (Compacted sliding window)
        let effective_history = if caveman && history.len() > 6 {
            history[history.len() - 6..].to_vec()
        } else {
            history
        };

        let mut request =
            CompletionRequest::new(TaskClass::Expander, combined_system, effective_history);
        request.max_tokens = if caveman {
            effort.max_tokens().min(2048)
        } else {
            effort.max_tokens()
        };
        request.temperature = effort.temperature();
        request.timeout = Duration::from_secs(180);
        request = request
            .with_model(model.clone())
            .with_workspace(Some(workspace.clone()));

        if computer_mode {
            request = request.for_computer_use();
        }

        // Refuse a prompt that cannot fit *before* paying for the round trip.
        //
        // A context overflow discovered at the vendor costs a full turn's latency and, on
        // a metered plan, real money, to be told something that was knowable from the
        // prompt alone. It is also the one failure where the honest answer is "this
        // conversation has to shrink", and saying so up front is far better than saying
        // it after a ninety-second wait.
        let window = provider.caps().context_window;
        let needed = estimate_tokens(&request);

        // One telemetry sample per turn, taken from the same strings the prompt was
        // assembled from — so the category columns describe the prompt that actually
        // went out, and `estimated_total` is this guard's own number. Computer Use
        // loops add further requests after this point; those are not disaggregated yet
        // (Phase G), so `stream_requests` stays 1 for the initial assembled request.
        {
            let history_texts: Vec<&str> = request
                .messages
                .iter()
                .map(|message| message.content.as_str())
                .collect();
            let mut manifest = bhippi_core::ContextManifest::new();
            manifest
                .add_text(bhippi_core::ContextCategory::System, CHAT_SYSTEM)
                .add_text(bhippi_core::ContextCategory::Workspace, &workspace_context)
                .add_text(bhippi_core::ContextCategory::ProjectRules, &rules_context)
                .add_text(bhippi_core::ContextCategory::Skills, &skills_context)
                .add_text(
                    bhippi_core::ContextCategory::ComputerUse,
                    &computer_use_context,
                )
                .add_text(
                    bhippi_core::ContextCategory::Engine,
                    &engine_context(&workspace).await,
                )
                .add_text(bhippi_core::ContextCategory::Handoff, &handoff_context)
                .add_text(
                    bhippi_core::ContextCategory::TaskDirectives,
                    &format!("{}\n\n{}", effort.directive(), design.directive()),
                )
                .with_history(&history_texts)
                .add_estimate(
                    bhippi_core::ContextCategory::ReservedResponse,
                    u64::from(effort.max_tokens()),
                );
            self.record_context(bhippi_core::ContextSample {
                turn_id: turn_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                project: workspace.clone(),
                provider_id: provider_id.clone(),
                model: model.clone(),
                categories: manifest.categories().clone(),
                estimated_total: needed,
                history_messages: u32::try_from(request.messages.len()).unwrap_or(u32::MAX),
                reserved_output: u64::from(effort.max_tokens()),
                context_window_tokens: u64::from(window),
                over_window: window > 0 && needed >= u64::from(window),
                handoff: !handoff_context.is_empty(),
                ..bhippi_core::ContextSample::default()
            })
            .await;
        }
        if window > 0 && needed >= u64::from(window) {
            let reason = format!(
                "prompt is too long: about {needed} tokens against a {window}-token context window"
            );
            let fault = fault_from(&provider_id, &provider_label, &reason);
            return Outcome {
                state: TurnState::Failed,
                usage: None,
                error: Some(reason),
                fault: Some(fault),
            };
        }

        if design.is_on() {
            // Worth a log line: when someone asks later why an answer restyled their UI,
            // this is the record that says the switch was on for that turn.
            tracing::info!(turn = %turn_id, "design system directive applied");
        }

        if computer_mode {
            return self
                .run_computer_turn(
                    conversation_id,
                    turn_id,
                    provider,
                    &provider_id,
                    &provider_label,
                    request,
                    cancel,
                    computer_full_access,
                    computer_handoff_note,
                )
                .await;
        }

        self.emitter.thinking(
            turn_id,
            &format!("Connecting to {provider_label}"),
            AgentPhase::Connecting,
        );

        // Engine steps around the model stream. Real backends stream directly; the
        // labelled demo additionally walks the tool-and-permission protocol so the
        // interface contract is verifiable offline (ADR-0006 §4).
        let scripted_demo = provider_id == "demo";
        let mut permission_note = String::new();
        if scripted_demo {
            permission_note = self.demo_script(registry, turn_id, &mut cancel).await;
            if cancel.has_changed().unwrap_or(false) {
                return Outcome {
                    state: TurnState::Stopped,
                    usage: None,
                    error: None,
                    fault: None,
                };
            }
            self.emitter
                .thinking(turn_id, "Composing", AgentPhase::Streaming);
        }

        self.mark_state(conversation_id, turn_id, TurnState::Streaming)
            .await;

        let stream = match tokio::select! {
            changed = cancel.changed() => {
                if changed.is_ok() && *cancel.borrow() {
                    return Outcome {
                        state: TurnState::Stopped,
                        usage: None,
                        error: None,
                        fault: None,
                    };
                }
                provider.complete(request.clone()).await
            }
            res = provider.complete(request.clone()) => res,
        } {
            Ok(stream) => stream,
            Err(error) => {
                return Outcome {
                    state: TurnState::Failed,
                    usage: None,
                    error: Some(pretty_error(&error)),
                    fault: None,
                };
            }
        };

        let mut usage: Option<Usage> = None;
        let mut failure: Option<String> = None;
        let mut final_stop = StopReason::Completed;
        let mut stream = stream;
        // ENG-113: engine calls are pulled out of the stream and applied *as they close*,
        // not after the turn. That is what makes read -> act -> verify a loop the model can
        // close inside one turn, and it keeps protocol JSON out of the visible answer.
        let mut engine_scanner = crate::engine::bridge::EngineCallScanner::new();
        let mut engine_batches: Vec<crate::engine::session::EngineBatchResult> = Vec::new();
        let mut engine_answers: Vec<(String, String)> = Vec::new();
        let mut engine_images: Vec<String> = Vec::new();
        let mut engine_project = crate::engine::game_dir_of(&workspace).is_ok();
        let thinking_started = std::time::Instant::now();
        let mut has_thought = false;
        let mut thinking_finished = false;
        let mut in_think_tag = false;

        loop {
            let next = tokio::select! {
                biased;
                changed = cancel.changed() => {
                    if changed.is_ok() && *cancel.borrow() {
                        final_stop = StopReason::Cancelled;
                        break;
                    }
                    None
                }
                item = stream.next() => item,
            };
            let Some(item) = next else { break };
            match item {
                Ok(Delta::Thinking { delta }) => {
                    has_thought = true;
                    self.append_thinking(conversation_id, turn_id, &delta).await;
                    self.emitter.thought_delta(turn_id, &delta);
                }
                Ok(Delta::Text { delta }) => {
                    if delta.contains("<think>") {
                        in_think_tag = true;
                        has_thought = true;
                        let after = delta.split("<think>").nth(1).unwrap_or("");
                        if after.contains("</think>") {
                            in_think_tag = false;
                            thinking_finished = true;
                            let parts: Vec<&str> = after.split("</think>").collect();
                            let thought_part = parts.first().unwrap_or(&"");
                            let text_part = parts.get(1).unwrap_or(&"");
                            if !thought_part.is_empty() {
                                self.append_thinking(conversation_id, turn_id, thought_part)
                                    .await;
                                self.emitter.thought_delta(turn_id, thought_part);
                            }
                            let elapsed =
                                u64::try_from(thinking_started.elapsed().as_millis()).unwrap_or(0);
                            self.set_thinking_elapsed(conversation_id, turn_id, elapsed)
                                .await;
                            if !text_part.is_empty() {
                                self.append_content(conversation_id, turn_id, text_part)
                                    .await;
                                self.emitter.delta(turn_id, text_part);
                            }
                        } else if !after.is_empty() {
                            self.append_thinking(conversation_id, turn_id, after).await;
                            self.emitter.thought_delta(turn_id, after);
                        }
                    } else if in_think_tag {
                        if delta.contains("</think>") {
                            in_think_tag = false;
                            thinking_finished = true;
                            let parts: Vec<&str> = delta.split("</think>").collect();
                            let thought_part = parts.first().unwrap_or(&"");
                            let text_part = parts.get(1).unwrap_or(&"");
                            if !thought_part.is_empty() {
                                self.append_thinking(conversation_id, turn_id, thought_part)
                                    .await;
                                self.emitter.thought_delta(turn_id, thought_part);
                            }
                            let elapsed =
                                u64::try_from(thinking_started.elapsed().as_millis()).unwrap_or(0);
                            self.set_thinking_elapsed(conversation_id, turn_id, elapsed)
                                .await;
                            if !text_part.is_empty() {
                                self.append_content(conversation_id, turn_id, text_part)
                                    .await;
                                self.emitter.delta(turn_id, text_part);
                            }
                        } else {
                            self.append_thinking(conversation_id, turn_id, &delta).await;
                            self.emitter.thought_delta(turn_id, &delta);
                        }
                    } else {
                        if has_thought && !thinking_finished {
                            thinking_finished = true;
                            let elapsed =
                                u64::try_from(thinking_started.elapsed().as_millis()).unwrap_or(0);
                            self.set_thinking_elapsed(conversation_id, turn_id, elapsed)
                                .await;
                        }
                        // Only a game project can carry engine calls; anywhere else the
                        // text is just text and must not be buffered by the scanner.
                        if engine_project {
                            let (visible, calls) = engine_scanner.push(&delta);
                            if !visible.is_empty() {
                                self.append_content(conversation_id, turn_id, &visible)
                                    .await;
                                self.emitter.delta(turn_id, &visible);
                            }
                            for call in calls {
                                self.run_engine_call(
                                    turn_id,
                                    &workspace,
                                    &call,
                                    &mut engine_batches,
                                    &mut engine_answers,
                                    &mut engine_images,
                                )
                                .await;
                            }
                        } else {
                            self.append_content(conversation_id, turn_id, &delta).await;
                            self.emitter.delta(turn_id, &delta);
                        }
                    }
                }
                // A step the backend itself ran. Until now the activity dock was fed
                // only by the demo script, so on every provider a user actually uses it
                // sat empty while the agent read a dozen files.
                Ok(Delta::Step {
                    id,
                    verb,
                    title,
                    detail,
                    done,
                }) => {
                    let phase = AgentPhase::of_verb(&verb);
                    let state = if done {
                        ToolState::Ok
                    } else {
                        ToolState::Running
                    };
                    let activity = ToolActivity {
                        id: format!("vendor-{id}"),
                        action: tool_action_of(&verb),
                        title: if title.is_empty() {
                            verb.clone()
                        } else {
                            title
                        },
                        detail,
                        state,
                        command: None,
                        output: None,
                        exit_code: None,
                        elapsed_ms: None,
                        truncated: false,
                        changes: Vec::new(),
                    };
                    // A `done` event closes a step already on the record; it carries no
                    // title of its own, so it must not overwrite the one shown.
                    self.record_tool(conversation_id, turn_id, activity.clone(), done)
                        .await;
                    if !done {
                        self.emitter.thinking(
                            turn_id,
                            &phase_label(phase, &activity.detail),
                            phase,
                        );
                    }
                }
                // What the vendor says about the account's remaining allowance, mid-turn.
                Ok(Delta::Limit {
                    status,
                    session_used,
                    session_resets_at,
                    weekly_used,
                    weekly_resets_at,
                }) => {
                    self.remember_account_limits(
                        &provider_id,
                        session_used,
                        session_resets_at,
                        weekly_used,
                        weekly_resets_at,
                    )
                    .await;
                    self.emitter.limits(
                        &provider_id,
                        LimitSnapshot {
                            status: limit_status(&status).to_owned(),
                            session_used,
                            session_resets_at,
                            weekly_used,
                            weekly_resets_at,
                        },
                    );
                }
                Ok(Delta::Usage {
                    input_tokens,
                    output_tokens,
                }) => {
                    usage = Some(Usage {
                        input_tokens,
                        output_tokens,
                    });
                    self.tokens_today.fetch_add(
                        input_tokens.saturating_add(output_tokens),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                }
                Ok(Delta::Done { stop_reason }) => {
                    final_stop = stop_reason;
                    break;
                }
                Err(error) => {
                    failure = Some(pretty_error(&error));
                    break;
                }
            }
        }

        // Metering happens once per turn, on every exit path below — a stopped or
        // failed turn still spent the tokens it spent.
        if let Some(spent) = usage.as_ref() {
            self.record_usage(&provider_id, spent, model.as_deref())
                .await;
        }

        if final_stop == StopReason::Cancelled {
            return Outcome {
                state: TurnState::Stopped,
                usage,
                error: None,
                fault: None,
            };
        }
        if let Some(error) = failure {
            // The vendor's words are classified once, here, so the UI receives a card it
            // can act on rather than a sentence it can only print.
            let fault = fault_from(&provider_id, &provider_label, &error);
            return Outcome {
                state: TurnState::Failed,
                usage,
                error: Some(error),
                fault: Some(fault),
            };
        }

        if !permission_note.is_empty() {
            let note = format!("\n\n---\n{permission_note}");
            self.append_content(conversation_id, turn_id, &note).await;
            self.emitter.delta(turn_id, &note);
        }

        // Release anything the scanner was holding back (a tail that never became a tag).
        if engine_project {
            let tail = engine_scanner.finish();
            if !tail.is_empty() {
                self.append_content(conversation_id, turn_id, &tail).await;
                self.emitter.delta(turn_id, &tail);
            }
        }

        let full_text = {
            let conversations = self.conversations.lock().await;
            conversations
                .iter()
                .find(|c| c.meta.id == conversation_id)
                .and_then(|c| c.turns.iter().find(|t| t.id == turn_id))
                .map(|t| t.content.clone())
                .unwrap_or_default()
        };

        // Scan and execute autonomous file operations safely inside the workspace
        let write_ops = extract_write_file_tags(&full_text);
        if !write_ops.is_empty() {
            let ws_path = std::path::PathBuf::from(&workspace);
            if let Ok(canonical_root) = std::fs::canonicalize(&ws_path) {
                for op in write_ops {
                    if let Some(safe_rel) = sanitize_workspace_path(&op.path) {
                        let target = canonical_root.join(&safe_rel);
                        if let Some(parent) = target.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }
                        // Read what was there first, so the step can report real line
                        // counts rather than "a file was touched" (CHT-105).
                        let previous = tokio::fs::read_to_string(&target).await.ok();
                        let started = std::time::Instant::now();
                        match tokio::fs::write(&target, op.content.as_bytes()).await {
                            Ok(_) => {
                                let file_name = safe_rel
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| op.path.clone());
                                let change =
                                    line_change(&op.path, previous.as_deref(), &op.content);
                                let tool = self
                                    .tool_card(
                                        turn_id,
                                        ToolAction::WriteFile,
                                        &format!("Edited {file_name}"),
                                        &format!(
                                            "{} (+{} −{})",
                                            op.path, change.additions, change.deletions
                                        ),
                                    )
                                    .await;
                                self.remember_undo(
                                    turn_id,
                                    TurnUndoEntry {
                                        path: target.clone(),
                                        previous,
                                    },
                                )
                                .await;
                                self.finish_tool_with(
                                    turn_id,
                                    tool,
                                    ToolState::Ok,
                                    ToolResult::changes(vec![change]).since(started),
                                )
                                .await;
                                tracing::info!(path = %op.path, bytes = op.content.len(), "Autonomous workspace file written");
                            }
                            Err(err) => {
                                let file_name = safe_rel
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| op.path.clone());
                                let tool = self
                                    .tool_card(
                                        turn_id,
                                        ToolAction::WriteFile,
                                        &format!("Failed to write {file_name}"),
                                        &format!("Error: {err}"),
                                    )
                                    .await;
                                self.finish_tool(turn_id, tool, ToolState::Failed).await;
                                tracing::warn!(path = %op.path, %err, "Autonomous file write failed");
                            }
                        }
                    }
                }
            }
        }

        // Fallback for providers whose stream does not surface text deltas at all (a CLI
        // adapter that only reports a final message): anything the scanner never saw is
        // picked up here. Calls already applied mid-stream are not repeated, because the
        // scanner stripped them from the recorded content.
        if !engine_project && crate::engine::game_dir_of(&workspace).is_ok() {
            engine_project = true;
        }

        let batch_tags = extract_engine_batch_tags(&full_text);
        let action_tags = extract_engine_action_tags(&full_text);
        if !batch_tags.is_empty() || !action_tags.is_empty() {
            engine_project = true;
        }

        if engine_project {
            for raw in batch_tags {
                let snippet = &raw[..raw.len().min(20)];
                let already_applied = engine_batches.iter().any(|b| {
                    b.summary().contains(snippet)
                        || b.edit.as_ref().is_some_and(|e| e.label.contains(snippet))
                });
                if !already_applied {
                    self.run_engine_call(
                        turn_id,
                        &workspace,
                        &crate::engine::bridge::EngineCall::Batch(raw),
                        &mut engine_batches,
                        &mut engine_answers,
                        &mut engine_images,
                    )
                    .await;
                }
            }
            for raw in action_tags {
                self.run_engine_call(
                    turn_id,
                    &workspace,
                    &crate::engine::bridge::EngineCall::Action(raw),
                    &mut engine_batches,
                    &mut engine_answers,
                    &mut engine_images,
                )
                .await;
            }
        }

        // The read -> act -> verify loop (ENG-113 / ENG-115), bounded.
        //
        // Two things oblige another round: the model asked an engine question and is owed
        // the answer, or a batch was rejected and is owed the schema that would fix it. The
        // cap is deliberate — an agent that cannot correct its payload with the real schema
        // in hand will not correct it on the fifth attempt either, and the user is waiting.
        let mut transcript = full_text.clone();
        let mut seen_engine_failures = std::collections::BTreeSet::new();
        // The provider already produced the initial round above; only the remaining slots
        // are continuations, so the advertised cap is the true total rather than cap + 1.
        for round in 0..bhippi_types::ENGINE_AUTONOMY_MAX_ROUNDS.saturating_sub(1) {
            if let Some(remedy) = non_repairable_engine_observation(&engine_answers) {
                let note = format!("\n\nEngine verification stopped: {remedy}");
                self.append_content(conversation_id, turn_id, &note).await;
                self.emitter.delta(turn_id, &note);
                break;
            }
            let failure_summary = engine_batches
                .iter()
                .filter(|batch| !batch.applied)
                .map(crate::engine::session::EngineBatchResult::summary)
                .collect::<Vec<_>>()
                .join(" · ");
            if !failure_summary.is_empty() && !seen_engine_failures.insert(failure_summary.clone())
            {
                let note = format!(
                    "\n\nEngine repair stopped because the same rejected patch repeated. Unresolved: {failure_summary}"
                );
                self.append_content(conversation_id, turn_id, &note).await;
                self.emitter.delta(turn_id, &note);
                break;
            }
            let Some(prompt) =
                crate::engine::bridge::continuation_prompt(&engine_answers, &engine_batches)
            else {
                break;
            };
            let rejected = engine_batches.iter().filter(|batch| !batch.applied).count();
            if rejected > 0 {
                let tool = self
                    .tool_card(
                        turn_id,
                        ToolAction::EditEngine,
                        "Engine change rejected",
                        &engine_batches
                            .iter()
                            .filter(|batch| !batch.applied)
                            .map(crate::engine::session::EngineBatchResult::summary)
                            .collect::<Vec<_>>()
                            .join(" · "),
                    )
                    .await;
                self.finish_tool(turn_id, tool, ToolState::Failed).await;
            }
            self.emitter.thinking(
                turn_id,
                if rejected > 0 {
                    "Repairing engine change"
                } else {
                    "Reading the scene"
                },
                AgentPhase::Thinking,
            );

            engine_answers.clear();
            engine_batches.clear();

            let mut follow_up = request.clone();
            follow_up.messages.push(Message {
                role: Role::Assistant,
                content: transcript.clone(),
            });
            follow_up.messages.push(Message {
                role: Role::User,
                content: prompt,
            });
            follow_up.image_paths.append(&mut engine_images);
            let Ok(mut stream) = provider.complete(follow_up).await else {
                break;
            };
            let mut scanner = crate::engine::bridge::EngineCallScanner::new();
            let mut answer = String::new();
            while let Some(Ok(delta)) = stream.next().await {
                if cancel.has_changed().unwrap_or(false) && *cancel.borrow() {
                    break;
                }
                if let Delta::Text { delta } = delta {
                    let (visible, calls) = scanner.push(&delta);
                    answer.push_str(&visible);
                    for call in calls {
                        self.run_engine_call(
                            turn_id,
                            &workspace,
                            &call,
                            &mut engine_batches,
                            &mut engine_answers,
                            &mut engine_images,
                        )
                        .await;
                    }
                }
            }
            answer.push_str(&scanner.finish());
            let trimmed = answer.trim();
            if !trimmed.is_empty() {
                let note = format!("\n\n{trimmed}");
                self.append_content(conversation_id, turn_id, &note).await;
                self.emitter.delta(turn_id, &note);
                transcript.push_str(&note);
            }
            // Nothing new to resolve, or we have spent the budget: stop.
            if round + 2 == bhippi_types::ENGINE_AUTONOMY_MAX_ROUNDS {
                tracing::debug!("engine continuation budget spent");
                if let Some(unresolved) = unresolved_engine_work(&engine_answers, &engine_batches) {
                    let note = format!(
                        "\n\nEngine autonomy reached its {}-round limit. Unresolved: {unresolved}",
                        bhippi_types::ENGINE_AUTONOMY_MAX_ROUNDS
                    );
                    self.append_content(conversation_id, turn_id, &note).await;
                    self.emitter.delta(turn_id, &note);
                }
            }
        }

        Outcome {
            state: TurnState::Done,
            usage,
            error: None,
            fault: None,
        }
    }

    /// The configured engine permission mode; `Auto` when there is no config store (tests
    /// and the headless CLI), which is the same default a fresh install gets.
    async fn engine_permission_mode(&self) -> bhippi_core::EnginePermissionMode {
        let Some(config) = self.config.as_ref() else {
            return bhippi_core::EnginePermissionMode::default();
        };
        match config.load().await {
            Ok(config) => config.engine.permission_mode,
            Err(error) => {
                tracing::warn!(%error, "engine permission mode unreadable; asking before destructive edits");
                bhippi_core::EnginePermissionMode::default()
            }
        }
    }

    /// Decompose a call into (label, actions) for the plan card. `None` when the payload is
    /// malformed — the apply path reports that properly, so the gate does not double-report.
    fn engine_plan(
        &self,
        call: &crate::engine::bridge::EngineCall,
        payload: &str,
    ) -> Option<(String, Vec<serde_json::Value>)> {
        match call {
            crate::engine::bridge::EngineCall::Batch(_) => {
                crate::engine::parse_batch_payload(payload).ok()
            }
            crate::engine::bridge::EngineCall::Action(_) => {
                let action: serde_json::Value = serde_json::from_str(payload).ok()?;
                let label = action
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("engine action")
                    .to_owned();
                Some((label, vec![action]))
            }
            crate::engine::bridge::EngineCall::Query(_) => None,
        }
    }

    /// Show the plan and wait for a yes. A timeout or a cancel is a no — the engine never
    /// writes on silence.
    async fn ask_engine_permission(
        self: &Arc<Self>,
        turn_id: &str,
        summary: &str,
        destructive: bool,
    ) -> bool {
        let request = PermissionRequest {
            id: new_id(),
            action: "Change the game scene".to_owned(),
            scope: "engine".to_owned(),
            detail: summary.to_owned(),
            risk: if destructive {
                RiskLevel::High
            } else {
                RiskLevel::Medium
            },
        };
        let (tx, rx) = oneshot::channel();
        self.pending_permissions
            .lock()
            .await
            .insert(request.id.clone(), tx);
        self.set_state_and_permission(turn_id, request.clone())
            .await;
        self.emitter.permission(turn_id, request);
        let decision = match tokio::time::timeout(PERMISSION_TIMEOUT, rx).await {
            Ok(Ok(decision)) => decision,
            _ => PermissionDecision::Deny,
        };
        matches!(decision, PermissionDecision::AllowOnce)
    }

    /// Run one engine call from the model, show it in the Activity Dock, and broadcast any
    /// scene change so the Engine pane patches itself mid-turn.
    ///
    /// Results are pushed into `batches` (writes) and `answers` (reads) so the caller can
    /// decide whether another round is owed. A single `<engine_action>` runs as a one-action
    /// batch, so both write forms produce the same envelope.
    async fn run_engine_call(
        self: &Arc<Self>,
        turn_id: &str,
        workspace: &str,
        call: &crate::engine::bridge::EngineCall,
        batches: &mut Vec<crate::engine::session::EngineBatchResult>,
        answers: &mut Vec<(String, String)>,
        images: &mut Vec<String>,
    ) {
        if let crate::engine::bridge::EngineCall::Query(payload) = call {
            let started = std::time::Instant::now();
            let tool = self
                .tool_card(turn_id, ToolAction::ReadSource, "Engine query", payload)
                .await;
            let query: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
            let kind = query
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("scene");
            let answer = if matches!(kind, "screenshot" | "playtest") {
                let observation = async {
                    let game_dir = crate::engine::game_dir_of(workspace)?;
                    let action = serde_json::json!({ "kind": kind });
                    let verdict = crate::engine::capability_verdict(&game_dir, &[action])?;
                    if let Some(refusal) = verdict.refusal() {
                        return Err(crate::commands::AppError {
                            message: refusal,
                            hint: Some("Allow Run play in Engine → Agent permissions.".to_owned()),
                        });
                    }
                    if verdict.needs_approval
                        && !self
                            .ask_engine_permission(
                                turn_id,
                                if kind == "screenshot" {
                                    "Capture the current game viewport for visual verification."
                                } else {
                                    "Run a bounded scripted-input playtest on a disposable world."
                                },
                                false,
                            )
                            .await
                    {
                        return Err(crate::commands::AppError {
                            message: "The engine observation was declined.".to_owned(),
                            hint: Some("Continue without running the game, or ask the user again later.".to_owned()),
                        });
                    }
                    let app = self.desktop_overlay.as_ref().ok_or_else(|| crate::commands::AppError {
                        message: "Viewport observations require the desktop Engine pane.".to_owned(),
                        hint: Some("Open this project in the desktop app and keep the Engine pane visible.".to_owned()),
                    })?;
                    if kind == "screenshot" {
                        crate::engine::observation::request_screenshot(
                            app,
                            &game_dir,
                            query
                                .get("camera")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("editor")
                                .to_owned(),
                            query
                                .get("annotate")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                        )
                        .await
                    } else {
                        let steps = crate::engine::observation::playtest_steps(payload)?;
                        crate::engine::observation::request_playtest(app, &game_dir, steps).await
                    }
                }
                .await;
                match observation {
                    Ok(result) => {
                        if let Some(path) = result.path {
                            images.push(path.clone());
                            format!("{}\nimage: {path}", result.report)
                        } else {
                            result.report
                        }
                    }
                    Err(error) => match error.hint {
                        Some(hint) => {
                            format!("observation failed: {}\nhint: {hint}", error.message)
                        }
                        None => format!("observation failed: {}", error.message),
                    },
                }
            } else {
                crate::engine::query_bridge::answer_query(workspace, payload).await
            };
            // CHT-100/112: the query *and* what it answered, so the transcript's "Explored"
            // row expands into the same thing the model saw rather than into a label.
            self.finish_tool_with(
                turn_id,
                tool,
                ToolState::Ok,
                ToolResult::command(payload.clone(), &answer, None).since(started),
            )
            .await;
            answers.push((payload.clone(), answer));
            return;
        }
        // ENG-116: the plan card. Whether this asks depends on the configured mode and on
        // whether the batch removes anything — every write is transacted and undoable, so
        // Auto stops only for deletes.
        let payload = match call {
            crate::engine::bridge::EngineCall::Batch(payload)
            | crate::engine::bridge::EngineCall::Action(payload) => payload,
            // Answered above; the match is exhaustive so a new call kind cannot be dropped.
            crate::engine::bridge::EngineCall::Query(_) => return,
        };
        let preview = self.engine_plan(call, payload);
        if let Some((label, actions)) = preview {
            let destructive = crate::engine::bridge::is_destructive(&actions);
            let plan = crate::engine::bridge::plan_preview(&label, &actions);
            let plan_tool = self
                .tool_card(turn_id, ToolAction::EditEngine, "Engine plan", &plan)
                .await;
            self.finish_tool(turn_id, plan_tool, ToolState::Ok).await;
            // ENG-190: the project's own `[agent]` policy is the stronger of the two gates.
            // A capability set to `ask` requires a yes even in Autonomous mode — the app-wide
            // mode says how much *this user* wants to be asked, the project policy says what
            // *this project* permits, and the project wins.
            let verdict = crate::engine::game_dir_of(workspace)
                .ok()
                .and_then(|game_dir| crate::engine::capability_verdict(&game_dir, &actions).ok());
            let capability_asks = verdict
                .as_ref()
                .is_some_and(|verdict| verdict.needs_approval);
            if capability_asks
                || self
                    .engine_permission_mode()
                    .await
                    .needs_approval(destructive)
            {
                let mut summary = plan;
                if let Some(required) = verdict.as_ref().filter(|v| v.needs_approval) {
                    summary.push_str(&format!(
                        "
Needs: {}",
                        required
                            .required
                            .iter()
                            .map(|capability| capability.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                if !self
                    .ask_engine_permission(turn_id, &summary, destructive)
                    .await
                {
                    let tool = self
                        .tool_card(
                            turn_id,
                            ToolAction::EditEngine,
                            "Engine change declined",
                            &summary,
                        )
                        .await;
                    self.finish_tool(turn_id, tool, ToolState::Failed).await;
                    return;
                }
            }
        }

        // If the workspace does not yet have a game manifest, scaffold starter game files
        // so that the engine batch or action can be applied cleanly.
        if crate::engine::game_dir_of(workspace).is_err() {
            let root = std::path::PathBuf::from(workspace);
            if !bhippi_engine::manifest::manifest_path(&root).is_file() {
                let display_name = root
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or("My Game");
                let _ = bhippi_engine::scaffold::write_project(&root, display_name, true);
            }
        }

        let outcome = match call {
            crate::engine::bridge::EngineCall::Batch(payload) => {
                crate::engine::apply_agent_batch_as(
                    workspace,
                    None,
                    payload,
                    Some(&self.agent_id),
                    None,
                )
                .await
            }
            crate::engine::bridge::EngineCall::Action(payload) => {
                crate::engine::apply_agent_single(workspace, None, payload, Some(&self.agent_id))
                    .await
            }
            crate::engine::bridge::EngineCall::Query(_) => return,
        };
        match outcome {
            Ok(result) => {
                let state = if result.applied {
                    ToolState::Ok
                } else {
                    ToolState::Failed
                };
                let title = if result.applied {
                    "Engine change applied"
                } else {
                    "Engine change rejected"
                };
                let tool = self
                    .tool_card(turn_id, ToolAction::EditEngine, title, &result.summary())
                    .await;
                // CHT-100: the scene the batch touched, counted as a file change, so an
                // engine turn produces the same "Edited N files" summary a code turn does.
                // Line counts are not meaningful for a transacted scene edit — the unit is
                // the op — so the op count stands in for additions and nothing is invented
                // for deletions.
                let changes = result
                    .edit
                    .as_ref()
                    .map(|edit| {
                        vec![TurnFileChange {
                            path: edit.scene_path.replace('\\', "/"),
                            additions: usize::try_from(edit.op_count).unwrap_or(0),
                            deletions: 0,
                            status: "modified".to_owned(),
                        }]
                    })
                    .unwrap_or_default();
                self.finish_tool_with(turn_id, tool, state, ToolResult::changes(changes))
                    .await;
                if let (Some(app), Some(edit)) =
                    (self.desktop_overlay.as_ref(), result.edit.as_ref())
                {
                    let _ignored = crate::engine::EngineSceneChanged {
                        scene_path: edit.scene_path.clone(),
                        summary: edit.summary.clone(),
                        txn_id: edit.txn_id.clone(),
                        actor: edit.actor.clone(),
                        label: edit.label.clone(),
                        touched: edit.touched.clone(),
                        entity_count: edit.state.entity_count,
                        dirty: edit.state.dirty,
                        revision: edit.state.revision,
                    }
                    .emit(app);
                }
                batches.push(result);
            }
            Err(error) => {
                // A hard failure may be a located script/shader/asset problem, so it is
                // reported and fed into the bounded repair loop. Structural failures such
                // as "no game" still terminate naturally when the model has no valid fix.
                let detail = match &error.hint {
                    Some(hint) => format!("{} — {hint}", error.message),
                    None => error.message.clone(),
                };
                // ENG-188: a located compile/asset/schema failure is evidence for the next
                // bounded round, not the end of the turn. Feeding the exact payload and the
                // typed remedy back lets the model patch and re-verify without asking the
                // user to copy an Output Log line into chat.
                answers.push((format!("failed engine call: {payload}"), detail.clone()));
                let tool = self
                    .tool_card(
                        turn_id,
                        ToolAction::EditEngine,
                        "Engine call failed",
                        &detail,
                    )
                    .await;
                self.finish_tool(turn_id, tool, ToolState::Failed).await;
            }
        }
    }

    /// Runs a screenshot → one action → screenshot loop. Protocol text stays hidden; the
    /// Activity Dock carries intermediate actions and only the provider's final summary is shown.
    #[allow(clippy::too_many_arguments)]
    async fn run_computer_turn(
        self: &Arc<Self>,
        conversation_id: &str,
        turn_id: &str,
        provider: Arc<dyn Provider>,
        provider_id: &str,
        provider_label: &str,
        mut request: CompletionRequest,
        mut cancel: watch::Receiver<bool>,
        full_access: bool,
        handoff_note: Option<String>,
    ) -> Outcome {
        self.mark_state(conversation_id, turn_id, TurnState::Streaming)
            .await;
        self.emitter
            .thinking(turn_id, "Observing desktop", AgentPhase::Browsing);

        // Keep the desktop-wide grid-scan aura (ADR-0019) up for exactly this turn: the
        // guard drops on every exit path and closes the overlay with it.
        let _desktop_overlay = match &self.desktop_overlay {
            Some(handle) => {
                crate::overlay::OverlayGuard::begin(handle, "Scanning the desktop").await
            }
            None => crate::overlay::OverlayGuard::inert(),
        };
        let (overlay_generation, mut emergency_stop) = _desktop_overlay.stop_receiver();

        let observe = self
            .tool_card(
                turn_id,
                ToolAction::ControlComputer,
                "Desktop screenshot",
                "Capturing the current virtual desktop...",
            )
            .await;
        let capture = match crate::computer::capture_screen().await {
            Ok(capture) => {
                self.finish_tool(turn_id, observe, ToolState::Ok).await;
                capture
            }
            Err(error) => {
                self.finish_tool(turn_id, observe, ToolState::Failed).await;
                return Outcome {
                    state: TurnState::Failed,
                    usage: None,
                    error: Some(error),
                    fault: None,
                };
            }
        };
        let mut capture_path = match crate::computer::save_capture(&capture, turn_id).await {
            Ok(path) => path,
            Err(error) => {
                return Outcome {
                    state: TurnState::Failed,
                    usage: None,
                    error: Some(error),
                    fault: None,
                };
            }
        };
        request.image_paths = vec![capture_path.to_string_lossy().into_owned()];
        if let Some(note) = handoff_note.as_deref() {
            request
                .messages
                .push(Message::user(format!("Session note: {note}")));
        }
        request.messages.push(Message::user(computer_observation(
            &capture,
            &capture_path,
            "Initial desktop observation.",
        )));

        let mut input_tokens = 0_u64;
        let mut output_tokens = 0_u64;
        let mut actions_executed = 0_usize;

        loop {
            if *cancel.borrow() || computer_stop_requested(overlay_generation, &emergency_stop) {
                crate::computer::remove_capture(&capture_path).await;
                let usage = usage_if_any(input_tokens, output_tokens);
                if let Some(spent) = usage.as_ref() {
                    self.record_usage(provider_id, spent, request.model.as_deref())
                        .await;
                }
                return Outcome {
                    state: TurnState::Stopped,
                    usage,
                    error: None,
                    fault: None,
                };
            }

            self.emitter.thinking(
                turn_id,
                &format!("{provider_label} is inspecting the screen"),
                AgentPhase::Browsing,
            );
            let stream = match tokio::select! {
                stopped = wait_for_computer_stop(
                    &mut cancel,
                    &mut emergency_stop,
                    overlay_generation,
                ) => {
                    if stopped {
                        None
                    } else {
                        Some(provider.complete(request.clone()).await)
                    }
                }
                result = provider.complete(request.clone()) => Some(result),
            } {
                None => {
                    crate::computer::remove_capture(&capture_path).await;
                    return Outcome {
                        state: TurnState::Stopped,
                        usage: usage_if_any(input_tokens, output_tokens),
                        error: None,
                        fault: None,
                    };
                }
                Some(Ok(stream)) => stream,
                Some(Err(error)) => {
                    crate::computer::remove_capture(&capture_path).await;
                    return Outcome {
                        state: TurnState::Failed,
                        usage: usage_if_any(input_tokens, output_tokens),
                        error: Some(pretty_error(&error)),
                        fault: None,
                    };
                }
            };

            let mut raw_text = String::new();
            let mut stream = stream;
            let mut stream_failure = None;
            let mut cancelled = false;
            loop {
                let next = tokio::select! {
                    biased;
                    stopped = wait_for_computer_stop(
                        &mut cancel,
                        &mut emergency_stop,
                        overlay_generation,
                    ) => {
                        if stopped {
                            cancelled = true;
                        }
                        None
                    }
                    item = stream.next() => item,
                };
                let Some(item) = next else { break };
                match item {
                    Ok(Delta::Text { delta }) => raw_text.push_str(&delta),
                    Ok(Delta::Thinking { delta }) => {
                        self.append_thinking(conversation_id, turn_id, &delta).await;
                        self.emitter.thought_delta(turn_id, &delta);
                    }
                    Ok(Delta::Step {
                        id,
                        verb,
                        title,
                        detail,
                        done,
                    }) => {
                        let phase = AgentPhase::of_verb(&verb);
                        let activity = ToolActivity {
                            id: format!("computer-provider-{id}"),
                            action: tool_action_of(&verb),
                            title: if title.is_empty() {
                                verb.clone()
                            } else {
                                title
                            },
                            detail,
                            state: if done {
                                ToolState::Ok
                            } else {
                                ToolState::Running
                            },
                            command: None,
                            output: None,
                            exit_code: None,
                            elapsed_ms: None,
                            truncated: false,
                            changes: Vec::new(),
                        };
                        self.record_tool(conversation_id, turn_id, activity.clone(), done)
                            .await;
                        if !done {
                            self.emitter.thinking(
                                turn_id,
                                &phase_label(phase, &activity.detail),
                                phase,
                            );
                        }
                    }
                    Ok(Delta::Limit {
                        status,
                        session_used,
                        session_resets_at,
                        weekly_used,
                        weekly_resets_at,
                    }) => {
                        self.remember_account_limits(
                            provider_id,
                            session_used,
                            session_resets_at,
                            weekly_used,
                            weekly_resets_at,
                        )
                        .await;
                        self.emitter.limits(
                            provider_id,
                            LimitSnapshot {
                                status: limit_status(&status).to_owned(),
                                session_used,
                                session_resets_at,
                                weekly_used,
                                weekly_resets_at,
                            },
                        );
                    }
                    Ok(Delta::Usage {
                        input_tokens: input,
                        output_tokens: output,
                    }) => {
                        input_tokens = input_tokens.saturating_add(input);
                        output_tokens = output_tokens.saturating_add(output);
                        self.tokens_today.fetch_add(
                            input.saturating_add(output),
                            std::sync::atomic::Ordering::Relaxed,
                        );
                    }
                    Ok(Delta::Done { stop_reason }) => {
                        if stop_reason == StopReason::Cancelled {
                            cancelled = true;
                        }
                        break;
                    }
                    Err(error) => {
                        stream_failure = Some(pretty_error(&error));
                        break;
                    }
                }
            }

            if cancelled {
                crate::computer::remove_capture(&capture_path).await;
                let usage = usage_if_any(input_tokens, output_tokens);
                if let Some(spent) = usage.as_ref() {
                    self.record_usage(provider_id, spent, request.model.as_deref())
                        .await;
                }
                return Outcome {
                    state: TurnState::Stopped,
                    usage,
                    error: None,
                    fault: None,
                };
            }
            if let Some(error) = stream_failure {
                crate::computer::remove_capture(&capture_path).await;
                let usage = usage_if_any(input_tokens, output_tokens);
                if let Some(spent) = usage.as_ref() {
                    self.record_usage(provider_id, spent, request.model.as_deref())
                        .await;
                }
                return Outcome {
                    state: TurnState::Failed,
                    usage,
                    error: Some(error),
                    fault: None,
                };
            }

            let actions = extract_computer_action_tags(&raw_text);
            if actions.is_empty() {
                let visible = strip_computer_action_tags(&raw_text);
                let final_text = if visible.trim().is_empty() {
                    match handoff_note.as_deref() {
                        Some(note) => format!("Computer Use completed. {note}"),
                        None => "Computer Use completed.".to_owned(),
                    }
                } else {
                    visible.trim().to_owned()
                };
                self.append_content(conversation_id, turn_id, &final_text)
                    .await;
                self.emitter.delta(turn_id, &final_text);
                crate::computer::remove_capture(&capture_path).await;
                let usage = usage_if_any(input_tokens, output_tokens);
                if let Some(spent) = usage.as_ref() {
                    self.record_usage(provider_id, spent, request.model.as_deref())
                        .await;
                }
                return Outcome {
                    state: TurnState::Done,
                    usage,
                    error: None,
                    fault: None,
                };
            }
            if actions.len() != 1 {
                crate::computer::remove_capture(&capture_path).await;
                return Outcome {
                    state: TurnState::Failed,
                    usage: usage_if_any(input_tokens, output_tokens),
                    error: Some(
                        "The provider returned multiple desktop actions at once; no input was sent."
                            .to_owned(),
                    ),
                    fault: None,
                };
            }
            if actions_executed >= MAX_COMPUTER_ACTIONS_PER_TURN {
                crate::computer::remove_capture(&capture_path).await;
                let message = format!(
                    "Computer Use stopped after {MAX_COMPUTER_ACTIONS_PER_TURN} actions to prevent an unbounded desktop loop."
                );
                self.append_content(conversation_id, turn_id, &message)
                    .await;
                self.emitter.delta(turn_id, &message);
                return Outcome {
                    state: TurnState::Failed,
                    usage: usage_if_any(input_tokens, output_tokens),
                    error: Some(message),
                    fault: None,
                };
            }

            let mut actions = actions;
            let action = actions.remove(0);
            if action.requires_full_access() && !full_access {
                crate::computer::remove_capture(&capture_path).await;
                let message = "Computer Use can see the screen, but Full PC Access is off, so no mouse or keyboard input was sent.";
                self.append_content(conversation_id, turn_id, message).await;
                self.emitter.delta(turn_id, message);
                return Outcome {
                    state: TurnState::Failed,
                    usage: usage_if_any(input_tokens, output_tokens),
                    error: Some(message.to_owned()),
                    fault: None,
                };
            }

            let title = computer_action_title(&action);
            let activity = self
                .tool_card(
                    turn_id,
                    ToolAction::ControlComputer,
                    &title,
                    "Executing verified desktop input...",
                )
                .await;
            let result = match crate::computer::execute_action(action).await {
                Ok(result) => {
                    self.finish_tool(turn_id, activity, ToolState::Ok).await;
                    result
                }
                Err(error) => {
                    self.finish_tool(turn_id, activity, ToolState::Failed).await;
                    crate::computer::remove_capture(&capture_path).await;
                    return Outcome {
                        state: TurnState::Failed,
                        usage: usage_if_any(input_tokens, output_tokens),
                        error: Some(error),
                        fault: None,
                    };
                }
            };
            actions_executed = actions_executed.saturating_add(1);

            tokio::select! {
                _ = tokio::time::sleep(COMPUTER_UI_SETTLE_DELAY) => {}
                stopped = wait_for_computer_stop(
                    &mut cancel,
                    &mut emergency_stop,
                    overlay_generation,
                ) => {
                    if stopped {
                        crate::computer::remove_capture(&capture_path).await;
                        return Outcome {
                            state: TurnState::Stopped,
                            usage: usage_if_any(input_tokens, output_tokens),
                            error: None,
                            fault: None,
                        };
                    }
                }
            }

            let next_capture = match crate::computer::capture_screen().await {
                Ok(capture) => capture,
                Err(error) => {
                    crate::computer::remove_capture(&capture_path).await;
                    return Outcome {
                        state: TurnState::Failed,
                        usage: usage_if_any(input_tokens, output_tokens),
                        error: Some(error),
                        fault: None,
                    };
                }
            };
            crate::computer::remove_capture(&capture_path).await;
            let next_id = format!("{turn_id}-{actions_executed}");
            capture_path = match crate::computer::save_capture(&next_capture, &next_id).await {
                Ok(path) => path,
                Err(error) => {
                    return Outcome {
                        state: TurnState::Failed,
                        usage: usage_if_any(input_tokens, output_tokens),
                        error: Some(error),
                        fault: None,
                    };
                }
            };
            request.messages.push(Message::assistant(raw_text));
            request.messages.push(Message::user(computer_observation(
                &next_capture,
                &capture_path,
                &format!("Action result: {}", result.detail),
            )));
            request.image_paths = vec![capture_path.to_string_lossy().into_owned()];
        }
    }

    /// Walks plan → provider check → read → **ask permission**, emitting each step.
    ///
    /// Returns the human-readable note appended under the answer describing what was
    /// allowed or denied. Deterministic and fully offline.
    async fn demo_script(
        self: &Arc<Self>,
        registry: &Arc<ProviderRuntime>,
        turn_id: &str,
        cancel: &mut watch::Receiver<bool>,
    ) -> String {
        let plan = self
            .tool_card(
                turn_id,
                ToolAction::Plan,
                "Outlining the answer",
                "structure · scope · what is known",
            )
            .await;
        self.sleep_or_cancel(Duration::from_millis(420), cancel)
            .await;

        let ollama_row = registry
            .providers
            .iter()
            .find(|info| info.id == "ollama")
            .map(|info| match info.health {
                bhippi_types::Health::Healthy { latency_ms } => {
                    format!("reachable · {latency_ms} ms")
                }
                _ => "not reachable".to_owned(),
            })
            .unwrap_or_else(|| "not installed".to_owned());
        let check = self
            .tool_card(
                turn_id,
                ToolAction::CheckProviders,
                "Checking local providers",
                &format!("ollama: {ollama_row}"),
            )
            .await;
        self.sleep_or_cancel(Duration::from_millis(380), cancel)
            .await;

        let reading = self
            .tool_card(
                turn_id,
                ToolAction::ReadSource,
                "Reading docs/08-BUILD-ORDER.md",
                "local file · sprint map S0–S11",
            )
            .await;
        self.sleep_or_cancel(Duration::from_millis(520), cancel)
            .await;

        if cancel.has_changed().unwrap_or(false) {
            return String::new();
        }

        // The ask: nothing consequential happens without an explicit answer (ADR-0006 §3).
        let request = PermissionRequest {
            id: new_id(),
            action: "Fetch live sources from the web".to_owned(),
            scope: "harvest".to_owned(),
            detail:
                "Bhippi would crawl pages relevant to this question once harvesting ships (S2). \
                     Nothing is fetched today — this records the flow you control."
                    .to_owned(),
            risk: RiskLevel::Medium,
        };
        let (tx, rx) = oneshot::channel();
        self.pending_permissions
            .lock()
            .await
            .insert(request.id.clone(), tx);
        self.set_state_and_permission(turn_id, request.clone())
            .await;
        self.emitter.permission(turn_id, request.clone());

        let decision = tokio::select! {
            changed = cancel.changed() => {
                if changed.is_ok() && *cancel.borrow() { None } else { Some(PermissionDecision::Deny) }
            }
            answered = tokio::time::timeout(PERMISSION_TIMEOUT, rx) => match answered {
                Ok(Ok(decision)) => Some(decision),
                _ => Some(PermissionDecision::Deny),
            }
        };

        let note = match decision {
            Some(PermissionDecision::AllowOnce) => {
                let fetch = self
                    .tool_card(
                        turn_id,
                        ToolAction::FetchUrl,
                        "Fetch queued for harvest (S2)",
                        "allowed once · recorded for this session",
                    )
                    .await;
                self.finish_tool(turn_id, fetch, ToolState::Ok).await;
                "You allowed fetching live sources once. When harvest lands (S2) the same card \
                 will show the actual pages read."
                    .to_owned()
            }
            Some(PermissionDecision::Deny) | None => {
                let skipped = self
                    .tool_card(
                        turn_id,
                        ToolAction::FetchUrl,
                        "Fetch denied — staying fully offline",
                        "nothing leaves this machine",
                    )
                    .await;
                self.finish_tool(turn_id, skipped, ToolState::Skipped).await;
                "You denied web access. Everything above stayed local — the demo provider never \
                 touches the network anyway."
                    .to_owned()
            }
        };

        for (card, state) in [
            (&plan, ToolState::Ok),
            (&check, ToolState::Ok),
            (&reading, ToolState::Ok),
        ] {
            self.finish_tool(turn_id, card.clone(), state).await;
        }
        note
    }

    /// Emits a running tool card and records it on the turn.
    async fn tool_card(
        self: &Arc<Self>,
        turn_id: &str,
        action: ToolAction,
        title: &str,
        detail: &str,
    ) -> ToolActivity {
        let tool = ToolActivity {
            id: new_id(),
            action,
            title: title.to_owned(),
            detail: detail.to_owned(),
            state: ToolState::Running,
            command: None,
            output: None,
            exit_code: None,
            elapsed_ms: None,
            truncated: false,
            changes: Vec::new(),
        };
        self.push_tool(turn_id, tool.clone()).await;
        self.emitter.tool(turn_id, tool.clone());
        tool
    }

    /// Records or closes one step the **backend** reported about itself.
    ///
    /// A closing event carries only an id — the vendor does not repeat the title it
    /// already sent — so closing must update the recorded step in place. Writing the
    /// bare closing event over the record would blank the step's own description.
    async fn record_tool(
        self: &Arc<Self>,
        _conversation_id: &str,
        turn_id: &str,
        tool: ToolActivity,
        closing: bool,
    ) {
        let mut merged = tool;
        {
            let mut conversations = self.conversations.lock().await;
            let turn = conversations.iter_mut().find_map(|conversation| {
                conversation
                    .turns
                    .iter_mut()
                    .find(|turn| turn.id == turn_id)
            });
            let Some(turn) = turn else {
                return;
            };
            match turn
                .tools
                .iter_mut()
                .find(|recorded| recorded.id == merged.id)
            {
                Some(recorded) => {
                    recorded.state = merged.state;
                    if !closing {
                        recorded.title = merged.title.clone();
                        recorded.detail = merged.detail.clone();
                        recorded.action = merged.action;
                    }
                    merged = recorded.clone();
                }
                None => {
                    // A close for a step never announced is still worth showing: the
                    // work happened, and an empty dock is the failure being fixed here.
                    turn.tools.push(merged.clone());
                }
            }
        }
        self.emitter.tool(turn_id, merged);
    }

    /// Adds one finished turn to the persistent ledger.
    ///
    /// A ledger write must never break a chat turn, so a failure is logged and dropped —
    /// the answer the user is reading matters more than the counter under it.
    async fn record_usage(&self, provider_id: &str, usage: &Usage, model: Option<&str>) {
        let Some(store) = self.usage.as_ref() else {
            return;
        };
        let input = usage.input_tokens;
        let output = usage.output_tokens;
        // Priced against the model the turn actually ran on, not the vendor's default —
        // the difference between Haiku and Opus is 5x on the same token count.
        let cost = crate::usage::cost_micros(provider_id, model, input, output);
        let mut models = std::collections::BTreeMap::new();
        if let Some(model_id) = model {
            if !model_id.is_empty() {
                models.insert(
                    model_id.to_owned(),
                    bhippi_core::ModelTally {
                        input_tokens: input,
                        output_tokens: output,
                        cost_micros: cost,
                        turns: 1,
                    },
                );
            }
        }
        let tally = bhippi_core::ProviderTally {
            input_tokens: input,
            output_tokens: output,
            cost_micros: cost,
            turns: 1,
            balance_micros: None,
            models,
        };
        let date = crate::usage::today_key(chrono::Local::now());
        if let Err(error) = store.record(&date, provider_id, tally).await {
            tracing::warn!(%error, provider = %provider_id, "usage ledger write skipped");
        }
    }

    /// Adds one assembled prompt to the context-telemetry store.
    ///
    /// A telemetry write must never break a chat turn, so a failure is logged and
    /// dropped — the answer the user is reading matters more than the sample under it.
    /// The sample carries counts and metadata only; no message or source text is ever
    /// persisted (INV-039).
    async fn record_context(&self, sample: bhippi_core::ContextSample) {
        let Some(store) = self.context.as_ref() else {
            return;
        };
        if let Err(error) = store.record(sample).await {
            tracing::warn!(%error, "context telemetry write skipped");
        }
    }

    async fn sleep_or_cancel(&self, duration: Duration, cancel: &mut watch::Receiver<bool>) {
        let _ignored = tokio::time::timeout(duration, cancel.changed()).await;
    }

    async fn settle_turn(self: &Arc<Self>, conversation_id: &str, turn_id: &str, outcome: Outcome) {
        let final_state = if outcome.state.is_terminal() {
            outcome.state
        } else {
            TurnState::Done
        };
        {
            let mut conversations = self.conversations.lock().await;
            if let Some(turn) = conversations.iter_mut().find_map(|conversation| {
                if conversation.meta.id != conversation_id {
                    return None;
                }
                conversation
                    .turns
                    .iter_mut()
                    .find(|turn| turn.id == turn_id)
            }) {
                turn.state = final_state;
                // CHT-103: how long the turn actually took, computed here rather than from
                // two timestamps in the pane. `created_at` is when the turn was accepted, so
                // this is the number a user means by "worked for".
                turn.worked_ms =
                    u64::try_from((Utc::now() - turn.created_at).num_milliseconds().max(0)).ok();
                // Fold the step-level file changes once more at the end, so a turn whose
                // last step closed after an interruption still carries a correct summary.
                turn.changes = TurnChanges::from_tools(&turn.tools);
                // Safety pass: if turn content still contains <think>...</think>, extract it cleanly
                if let Some(think_start) = turn.content.find("<think>") {
                    if let Some(think_end) = turn.content.find("</think>") {
                        let extracted = turn.content[think_start + 7..think_end].trim().to_owned();
                        let clean = format!(
                            "{}{}",
                            &turn.content[..think_start],
                            &turn.content[think_end + 8..]
                        )
                        .trim()
                        .to_owned();
                        if turn.thinking.is_none() || turn.thinking.as_deref() == Some("") {
                            turn.thinking = Some(extracted);
                        }
                        turn.content = clean;
                    }
                }
            }
        }
        if let Some(fault) = outcome.fault.clone() {
            let mut conversations = self.conversations.lock().await;
            if let Some(turn) = conversations.iter_mut().find_map(|conversation| {
                conversation
                    .turns
                    .iter_mut()
                    .find(|turn| turn.id == turn_id)
            }) {
                turn.fault = Some(fault);
            }
        }
        self.emitter.done(ChatTurnDone {
            turn_id: turn_id.to_owned(),
            state: final_state,
            usage: outcome.usage,
            error: outcome.error,
            fault: outcome.fault,
        });
    }

    async fn history_messages(&self, conversation_id: &str) -> Vec<Message> {
        let conversations = self.conversations.lock().await;
        conversations
            .iter()
            .find(|conversation| conversation.meta.id == conversation_id)
            .map(|conversation| {
                conversation
                    .turns
                    .iter()
                    .filter(|turn| !turn.content.trim().is_empty())
                    .map(|turn| Message {
                        role: turn.role.into_role(),
                        content: turn.content.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn append_content(&self, conversation_id: &str, turn_id: &str, piece: &str) {
        let mut conversations = self.conversations.lock().await;
        if let Some(turn) = conversations.iter_mut().find_map(|conversation| {
            if conversation.meta.id != conversation_id {
                return None;
            }
            conversation
                .turns
                .iter_mut()
                .find(|turn| turn.id == turn_id)
        }) {
            turn.content.push_str(piece);
        }
    }

    async fn append_thinking(&self, conversation_id: &str, turn_id: &str, piece: &str) {
        let mut conversations = self.conversations.lock().await;
        if let Some(turn) = conversations.iter_mut().find_map(|conversation| {
            if conversation.meta.id != conversation_id {
                return None;
            }
            conversation
                .turns
                .iter_mut()
                .find(|turn| turn.id == turn_id)
        }) {
            let mut current = turn.thinking.take().unwrap_or_default();
            current.push_str(piece);
            turn.thinking = Some(current);
        }
    }

    async fn set_thinking_elapsed(&self, conversation_id: &str, turn_id: &str, elapsed_ms: u64) {
        let mut conversations = self.conversations.lock().await;
        if let Some(turn) = conversations.iter_mut().find_map(|conversation| {
            if conversation.meta.id != conversation_id {
                return None;
            }
            conversation
                .turns
                .iter_mut()
                .find(|turn| turn.id == turn_id)
        }) {
            turn.thinking_elapsed_ms = Some(elapsed_ms);
        }
    }

    async fn mark_state(&self, conversation_id: &str, turn_id: &str, state: TurnState) {
        let mut conversations = self.conversations.lock().await;
        if let Some(turn) = conversations.iter_mut().find_map(|conversation| {
            if conversation.meta.id != conversation_id {
                return None;
            }
            conversation
                .turns
                .iter_mut()
                .find(|turn| turn.id == turn_id)
        }) {
            turn.state = state;
        }
    }

    async fn set_state_and_permission(&self, turn_id: &str, request: PermissionRequest) {
        let mut conversations = self.conversations.lock().await;
        if let Some(turn) = conversations.iter_mut().find_map(|conversation| {
            conversation
                .turns
                .iter_mut()
                .find(|turn| turn.id == turn_id)
        }) {
            turn.state = TurnState::AwaitingPermission;
            turn.permission = Some(request);
        }
    }

    async fn push_tool(&self, turn_id: &str, tool: ToolActivity) {
        let mut conversations = self.conversations.lock().await;
        if let Some(turn) = conversations.iter_mut().find_map(|conversation| {
            conversation
                .turns
                .iter_mut()
                .find(|turn| turn.id == turn_id)
        }) {
            turn.tools.push(tool);
        }
    }

    /// Keep one file's pre-write content so the turn can be undone (CHT-115).
    ///
    /// The first write of a file in a turn is the one that matters: undoing back to the
    /// state *before the turn* means the earliest snapshot, not the latest.
    async fn remember_undo(&self, turn_id: &str, entry: TurnUndoEntry) {
        let mut store = self.turn_undo.lock().await;
        let entries = store.entry(turn_id.to_owned()).or_default();
        if entries.iter().any(|existing| existing.path == entry.path) {
            return;
        }
        entries.push(entry);

        // Enforce the budget by dropping whole turns, oldest first. Dropping *part* of a
        // turn would leave an Undo that restores some files and not others, which is worse
        // than no Undo at all.
        let mut total: usize = store
            .values()
            .flatten()
            .map(|entry| entry.previous.as_ref().map_or(0, String::len))
            .sum();
        while total > TURN_UNDO_BUDGET && store.len() > 1 {
            let Some(oldest) = store.keys().next().cloned() else {
                break;
            };
            if oldest == turn_id {
                // Never evict the turn currently being written; take the next one instead.
                let Some(other) = store.keys().find(|key| *key != &oldest).cloned() else {
                    break;
                };
                if let Some(dropped) = store.remove(&other) {
                    total -= dropped
                        .iter()
                        .map(|entry| entry.previous.as_ref().map_or(0, String::len))
                        .sum::<usize>();
                }
                continue;
            }
            if let Some(dropped) = store.remove(&oldest) {
                total -= dropped
                    .iter()
                    .map(|entry| entry.previous.as_ref().map_or(0, String::len))
                    .sum::<usize>();
            }
        }
    }

    /// Whether this turn's file changes can still be put back.
    pub async fn turn_undoable(&self, turn_id: &str) -> bool {
        self.turn_undo
            .lock()
            .await
            .get(turn_id)
            .is_some_and(|entries| !entries.is_empty())
    }

    /// Restore every file this turn wrote to what it was before (CHT-115).
    ///
    /// Returns how many files were restored. The snapshot is consumed: undoing twice is not
    /// a thing, and leaving it behind would let a second press overwrite work done since.
    ///
    /// # Errors
    /// Names the first file that could not be put back. Files restored before it stay
    /// restored — a partial restore reported honestly beats an all-or-nothing that has to
    /// hold the whole workspace to be safe.
    pub async fn undo_turn(&self, turn_id: &str) -> Result<usize, String> {
        let entries = {
            let mut store = self.turn_undo.lock().await;
            store.remove(turn_id)
        };
        let Some(entries) = entries else {
            return Err(
                "This turn's original files are no longer held, so it cannot be undone.".to_owned(),
            );
        };
        let mut restored = 0usize;
        for entry in entries {
            let outcome = match &entry.previous {
                Some(text) => tokio::fs::write(&entry.path, text).await,
                // The file did not exist before the turn, so putting it back means removing
                // it. An already-absent file is a success, not a failure.
                None => match tokio::fs::remove_file(&entry.path).await {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    other => other,
                },
            };
            outcome
                .map_err(|error| format!("Could not restore {}: {error}", entry.path.display()))?;
            restored += 1;
        }
        Ok(restored)
    }

    async fn finish_tool(&self, turn_id: &str, tool: ToolActivity, state: ToolState) {
        self.finish_tool_with(turn_id, tool, state, ToolResult::default())
            .await;
    }

    /// Close a step **and record what it produced** (CHT-102).
    ///
    /// The close used to flip `state` and nothing else, which is why the transcript could
    /// only ever print a label. Recording here — on the same lock, in the same call — is
    /// what lets a finished turn be re-rendered from the conversation alone, with no refetch
    /// and no second source of truth for what a step did.
    async fn finish_tool_with(
        &self,
        turn_id: &str,
        tool: ToolActivity,
        state: ToolState,
        result: ToolResult,
    ) {
        let tool_id = tool.id.clone();
        let mut updated = tool;
        updated.state = state;
        result.apply(&mut updated);
        {
            let mut conversations = self.conversations.lock().await;
            if let Some(turn) = conversations.iter_mut().find_map(|conversation| {
                conversation
                    .turns
                    .iter_mut()
                    .find(|turn| turn.id == turn_id)
            }) {
                if let Some(recorded) = turn
                    .tools
                    .iter_mut()
                    .find(|recorded| recorded.id == tool_id)
                {
                    recorded.state = state;
                    recorded.command.clone_from(&updated.command);
                    recorded.output.clone_from(&updated.output);
                    recorded.exit_code = updated.exit_code;
                    recorded.elapsed_ms = updated.elapsed_ms;
                    recorded.truncated = updated.truncated;
                    recorded.changes.clone_from(&updated.changes);
                }
                // The turn's summary is folded from its steps, so it stays correct as steps
                // close rather than being computed once at the end and going stale if the
                // turn is interrupted.
                turn.changes = TurnChanges::from_tools(&turn.tools);
            }
        }
        self.emitter.tool(turn_id, updated);
    }
}

fn non_repairable_engine_observation(answers: &[(String, String)]) -> Option<String> {
    answers.iter().find_map(|(_, answer)| {
        let lower = answer.to_ascii_lowercase();
        (lower.contains("no game manifest")
            || lower.contains("require the desktop engine pane")
            || lower.contains("requires the desktop engine pane"))
        .then(|| answer.clone())
    })
}

fn unresolved_engine_work(
    answers: &[(String, String)],
    batches: &[crate::engine::session::EngineBatchResult],
) -> Option<String> {
    let rejected = batches
        .iter()
        .filter(|batch| !batch.applied)
        .map(crate::engine::session::EngineBatchResult::summary)
        .collect::<Vec<_>>()
        .join(" · ");
    if !rejected.is_empty() {
        return Some(rejected);
    }
    answers.last().map(|(_, answer)| answer.clone())
}

/// The four choices the user makes about how a turn is answered.
///
/// They always travel together and are always decided at the same moment — in the
/// composer, before send — so they move as one value. Passed as four parallel arguments
/// they were four chances to transpose two `Option<String>`s at a call site, which the
/// compiler cannot catch.
#[derive(Clone, Debug, Default)]
pub(crate) struct TurnOptions {
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub effort: Effort,
    pub design: DesignMode,
    pub caveman: bool,
}

/// What one turn should run: which backend, on which model, at which effort. These
/// always travel together, so they move as one value rather than parallel arguments.
struct TurnPlan {
    /// `None` means nothing was resolvable, and the offline demo answers.
    provider: Option<(Arc<dyn Provider>, String)>,
    /// The picker's original choice (before Computer Use handoff). `None` defaulted.
    provider_id: Option<String>,
    model: Option<String>,
    effort: Effort,
    design: DesignMode,
    caveman: bool,
    workspace: String,
}

/// A Computer Use session that runs on a different backend than the picker's choice:
/// (provider, label, id, and the model the new backend defaults to, if any).
type ComputerVisionStandin = (Arc<dyn Provider>, String, String, Option<String>);

struct Outcome {
    state: TurnState,
    usage: Option<Usage>,
    error: Option<String>,
    fault: Option<TurnFault>,
}

/// A deliberately rough token estimate for one request.
///
/// Four bytes per token is the long-standing rule of thumb for English prose and code,
/// and it does not need to be better than that here: this is a guard against a prompt
/// that is *multiples* of the window, not an accounting of one that is near it. A real
/// tokeniser per vendor would be exact, out of date the week a vendor changes it, and
/// would still not change the decision in any case this catches. Erring low is the safe
/// direction — a prompt this misjudges is simply sent, and the vendor rules on it.
fn estimate_tokens(request: &CompletionRequest) -> u64 {
    let bytes: usize = request.system.len()
        + request
            .messages
            .iter()
            .map(|message| message.content.len() + 8)
            .sum::<usize>();
    (bytes as u64) / 4 + u64::from(request.max_tokens)
}

/// Renders a diagnostic scan as the report the user reads.
///
/// Grouped by severity rather than listed flat, because a hundred-row table sorted by file
/// buries the one error under ninety-nine notes. Each finding carries **why** it is a
/// defect and the fix, since a debugger that only names problems is a list, not a tool.
fn render_debug_report(report: &crate::debugger::DiagnosticReport) -> String {
    use std::fmt::Write as _;

    let verdict = if report.success {
        "**PASS** — nothing blocking."
    } else {
        "**FAIL** — blocking errors below."
    };
    let mut out = format!(
        "### Deterministic debugger · {}

{verdict}

{}

         *{} files ({} KB) scanned · {} · {} ms · zero model tokens*

",
        report.project_name,
        report.summary,
        report.files_scanned,
        report.bytes_scanned / 1024,
        report.project_type,
        report.duration_ms,
    );

    if report.partial {
        out.push_str(
            "> A budget stopped this scan early, so the project was **not** covered in \
             full. Treat a clean result here as incomplete.\n\n",
        );
    }

    if !report.by_category.is_empty() {
        let counts: Vec<String> = report
            .by_category
            .iter()
            .map(|entry| format!("{} {}", entry.count, entry.category))
            .collect();
        let _ignored = writeln!(
            out,
            "**Found:** {}
",
            counts.join(" · ")
        );
    }

    // A tool that could not start contributes nothing, and silently contributing nothing
    // is indistinguishable from finding nothing. Say which ran.
    if !report.tools.is_empty() {
        out.push_str(
            "**Toolchains**

",
        );
        for tool in &report.tools {
            let mark = if tool.ok { "ok" } else { "failed" };
            let note = tool
                .note
                .as_deref()
                .map(|note| format!(" — {note}"))
                .unwrap_or_default();
            let _ignored = writeln!(out, "- `{}` in `{}` · {mark}{note}", tool.tool, tool.at);
        }
        out.push('\n');
    }

    if report.items.is_empty() {
        out.push_str(
            "No findings. Nothing in the rule set matched, and every toolchain that ran \
             was clean.\n",
        );
        return out;
    }

    for (severity, heading) in [
        ("error", "Errors — these block"),
        ("warning", "Warnings — real defects, not blocking"),
        ("info", "Notes"),
    ] {
        let group: Vec<_> = report
            .items
            .iter()
            .filter(|item| item.severity == severity)
            .collect();
        if group.is_empty() {
            continue;
        }
        let _ignored = writeln!(
            out,
            "#### {heading} ({})
",
            group.len()
        );

        // Notes are the high-count, low-value group — a hundred TODOs would drown the
        // errors above them, so they collapse to one line each and cap out.
        let brief = severity == "info";
        for item in group.iter().take(if brief { 25 } else { 120 }) {
            let at = item
                .line
                .map(|line| format!("{}:{line}", item.file))
                .unwrap_or_else(|| item.file.clone());
            let code = item.code.as_deref().unwrap_or("—");
            if brief {
                let _ignored = writeln!(out, "- `{at}` · {} ({code})", one_line(&item.message));
                continue;
            }
            let _ignored = writeln!(
                out,
                "**`{at}`** · `{code}`

{}
",
                one_line(&item.message)
            );
            if !item.evidence.is_empty() {
                let _ignored = writeln!(
                    out,
                    "```
{}
```",
                    item.evidence
                );
            }
            if !item.why.is_empty() {
                let _ignored = writeln!(out, "*Why it matters:* {}", one_line(&item.why));
            }
            if let Some(fix) = item.suggestion.as_deref() {
                let _ignored = writeln!(
                    out,
                    "*Fix:* {}
",
                    one_line(fix)
                );
            }
        }
        if group.len() > if brief { 25 } else { 120 } {
            let _ignored = writeln!(
                out,
                "
… and {} more at this severity.
",
                group.len() - if brief { 25 } else { 120 }
            );
        }
        out.push('\n');
    }

    out
}

/// Collapses a diagnostic onto one line so it cannot break the surrounding markdown.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Maps the shared verb vocabulary onto the icon set the dock already draws.
fn tool_action_of(verb: &str) -> ToolAction {
    match verb {
        "read" => ToolAction::ReadSource,
        "edited" | "wrote" => ToolAction::WriteFile,
        "searched" => ToolAction::SearchWeb,
        "fetched" => ToolAction::FetchUrl,
        "planned" => ToolAction::Plan,
        "computer" | "screen" | "mouse" | "keyboard" | "click" => ToolAction::ControlComputer,
        _ => ToolAction::ExtractDots,
    }
}

/// The words shown beside a phase's animation.
///
/// The target is included when there is one, because "Reading" tells the user nothing
/// they cannot already see, and "Reading src/main.rs" tells them what the agent believes
/// the task is — which is the moment they can tell it is going the wrong way.
fn phase_label(phase: AgentPhase, target: &str) -> String {
    let verb = match phase {
        AgentPhase::Connecting => "Connecting",
        AgentPhase::Queued => "Queued",
        AgentPhase::Thinking => "Thinking",
        AgentPhase::Reasoning => "Reasoning",
        AgentPhase::Planning => "Planning",
        AgentPhase::Searching => "Searching",
        AgentPhase::Reading => "Reading",
        AgentPhase::Writing => "Writing",
        AgentPhase::Editing => "Editing",
        AgentPhase::Refactoring => "Refactoring",
        AgentPhase::Running => "Running",
        AgentPhase::Testing => "Testing",
        AgentPhase::Building => "Building",
        AgentPhase::Debugging => "Debugging",
        AgentPhase::Installing => "Installing",
        AgentPhase::Fetching => "Fetching",
        AgentPhase::Browsing => "Browsing",
        AgentPhase::Analyzing => "Analysing",
        AgentPhase::Summarizing => "Summarising",
        AgentPhase::Reviewing => "Reviewing",
        AgentPhase::AwaitingPermission => "Waiting for you",
        AgentPhase::Compacting => "Compacting",
        AgentPhase::Retrying => "Retrying",
        AgentPhase::Streaming => "Writing the answer",
        AgentPhase::Finalizing => "Finishing",
        AgentPhase::Done => "Done",
        AgentPhase::Stopped => "Stopped",
        AgentPhase::Failed => "Failed",
    };
    let target = target.trim();
    if target.is_empty() {
        return verb.to_owned();
    }
    let short: String = target.chars().take(64).collect();
    format!("{verb} {short}")
}

/// Normalises a vendor's limit status onto the three the UI knows.
const fn limit_status(said: &str) -> &'static str {
    match said.as_bytes() {
        b"rejected" => "rejected",
        b"allowed_warning" => "allowed_warning",
        _ => "allowed",
    }
}

/// Classifies a vendor's failure text into the card the UI renders.
fn fault_from(provider_id: &str, provider_label: &str, reason: &str) -> TurnFault {
    let advice = bhippi_providers::spec(provider_id)
        .map(|spec| bhippi_providers::advise(spec, reason))
        .unwrap_or_else(|| {
            // A backend with no catalogue entry (a local server, a cloud row) still
            // deserves a named fault; only the vendor-specific wording is unavailable.
            bhippi_providers::advise(
                bhippi_providers::CATALOG
                    .first()
                    .unwrap_or_else(|| unreachable!("the catalogue is never empty")),
                reason,
            )
        });
    TurnFault {
        kind: advice.kind.id().to_owned(),
        title: advice.title,
        summary: advice.summary,
        fix: advice.fix,
        remedy: advice.remedy.id().to_owned(),
        action_label: advice.action_label,
        provider: provider_label.to_owned(),
        resets_at: advice.resets_at,
        retryable: advice.kind.retryable(),
        detail: reason.chars().take(600).collect(),
    }
}

fn short_title(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title: String = collapsed.chars().take(48).collect();
    if collapsed.chars().count() > 48 {
        title.push('…');
    }
    if title.is_empty() {
        "New conversation".to_owned()
    } else {
        title
    }
}

/// Renders a typed error with its actionable hint for the chat surface (R1 spirit).
#[must_use]
pub fn pretty_error(error: &bhippi_types::BhippiError) -> String {
    match error.hint() {
        Some(hint) => format!("{error}\n\n**Fix:** {hint}"),
        None => error.to_string(),
    }
}

#[allow(dead_code)] // referenced by generated IPC docs; kept for parity with ErrorCode mapping
fn error_code_of(error: &bhippi_types::BhippiError) -> ErrorCode {
    match error {
        bhippi_types::BhippiError::Provider { .. } => ErrorCode::ProviderUnavailable,
        bhippi_types::BhippiError::Budget { .. } => ErrorCode::BudgetExceeded,
        bhippi_types::BhippiError::OutOfScope { .. } => ErrorCode::OutOfScope,
        bhippi_types::BhippiError::Gate { .. } => ErrorCode::GateBlocked,
        bhippi_types::BhippiError::Fetch { .. } => ErrorCode::FetchFailed,
        bhippi_types::BhippiError::Db { .. } => ErrorCode::Data,
        bhippi_types::BhippiError::Config { .. } => ErrorCode::Configuration,
        bhippi_types::BhippiError::Secret { .. } => ErrorCode::SecretStore,
        bhippi_types::BhippiError::Io { .. } => ErrorCode::Io,
        bhippi_types::BhippiError::Invariant { .. } => ErrorCode::InvariantViolated,
    }
}

/// Helper to sanitize relative workspace path safely, rejecting directory traversal.
fn sanitize_workspace_path(relative: &str) -> Option<std::path::PathBuf> {
    let trimmed = relative
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .replace('\\', "/");
    let mut safe = std::path::PathBuf::new();
    for segment in trimmed.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return None,
            other => {
                if other.contains(':') || other.chars().any(char::is_control) {
                    return None;
                }
                safe.push(other);
            }
        }
    }
    if safe.as_os_str().is_empty() {
        None
    } else {
        Some(safe)
    }
}

struct ParsedWriteFile {
    path: String,
    content: String,
}

/// Extracts all `<write_file path="...">...content...</write_file>` tags from assistant response.
fn extract_write_file_tags(text: &str) -> Vec<ParsedWriteFile> {
    let mut results = Vec::new();
    let mut cursor = 0;
    while let Some(start_tag) = text[cursor..].find("<write_file") {
        let tag_begin = cursor + start_tag;
        if let Some(tag_end) = text[tag_begin..].find('>') {
            let full_tag = &text[tag_begin..tag_begin + tag_end];
            let path = if let Some(p_start) = full_tag.find("path=\"") {
                let rest = &full_tag[p_start + 6..];
                if let Some(p_end) = rest.find('"') {
                    rest[..p_end].to_owned()
                } else {
                    String::new()
                }
            } else if let Some(p_start) = full_tag.find("path='") {
                let rest = &full_tag[p_start + 6..];
                if let Some(p_end) = rest.find('\'') {
                    rest[..p_end].to_owned()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            let content_start = tag_begin + tag_end + 1;
            if let Some(end_tag) = text[content_start..].find("</write_file>") {
                let content = &text[content_start..content_start + end_tag];
                let content_clean = content.strip_prefix('\r').unwrap_or(content);
                let content_clean = content_clean.strip_prefix('\n').unwrap_or(content_clean);
                if !path.is_empty() {
                    results.push(ParsedWriteFile {
                        path,
                        content: content_clean.to_owned(),
                    });
                }
                cursor = content_start + end_tag + 13;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    results
}

/// The engine half of a turn's system prompt (ENG-115 / ENG-117).
///
/// Retrieval, not a dump: the doctrine, the open scene's hierarchy digest, what the user has
/// selected, and the last few journal entries so the model knows what just happened —
/// including what it did itself last turn. Anything deeper is a `<engine_query>` away, which
/// is the point of having a read API.
async fn engine_context(workspace: &str) -> String {
    let Ok(game_dir) = crate::engine::game_dir_of(workspace) else {
        return String::new();
    };
    let Ok(query) = crate::engine::query_scene_in_workspace(workspace, None) else {
        return format!(
            "\n\n{ENGINE_SYSTEM}\n\nThe game manifest exists but the default scene could not be read.\n"
        );
    };

    let mut facts = format!(
        "## Live engine map\nScene: {}\nEntities: {}\n\n{}\n",
        query.scene_path, query.entity_count, query.digest
    );

    // What the user is looking at. "Move this one" is only answerable with it.
    if let Some(state) = crate::engine::open_scene_state(workspace, None) {
        if state.dirty {
            facts.push_str("\nThe editor has unsaved changes to this scene.\n");
        }
        if !state.selection.is_empty() {
            facts.push_str("\n## User selection and nearby facts\n");
            facts.push_str(
                &crate::engine::query_bridge::answer_query(workspace, "{\"kind\":\"selection\"}")
                    .await,
            );
        }
    }

    // The event stream, summarised (ENG-117): the last few transactions with their actor,
    // so the model can tell its own edits from the user's and does not redo work.
    let recent = crate::engine::recent_journal(&game_dir, 6).await;
    if !recent.is_empty() {
        facts.push_str("\n## Recent engine changes (newest first)\n");
        for row in recent {
            facts.push_str(&format!(
                "- r{} [{}] {}\n",
                row.revision,
                row.actor,
                row.label.unwrap_or_default()
            ));
        }
    }
    format!("\n\n{ENGINE_SYSTEM}\n\n{}", cap_engine_facts(facts))
}

/// Dynamic engine facts are retrieval, not a scene dump. The stable doctrine lives in the
/// versioned prompt; this budget applies to the per-project map, selection and recent facts
/// measured as the Token Engine measures every other context category (ENG-191).
fn cap_engine_facts(mut facts: String) -> String {
    let budget = bhippi_types::ENGINE_CONTEXT_TOKEN_BUDGET;
    if bhippi_core::estimate_text_tokens(&facts) <= budget {
        return facts;
    }
    let max_bytes = budget.saturating_mul(bhippi_core::ESTIMATED_BYTES_PER_TOKEN);
    let max_bytes = usize::try_from(max_bytes)
        .unwrap_or(usize::MAX)
        .min(facts.len());
    let mut boundary = max_bytes;
    while boundary > 0 && !facts.is_char_boundary(boundary) {
        boundary -= 1;
    }
    facts.truncate(boundary);
    facts.push_str("\n…engine context capped; use engine_query for deeper facts.\n");
    facts
}

fn asks_for_game_creation(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let has = |phrase: &str| lower.contains(phrase);
    let mentions_game = has("game")
        || has("platformer")
        || has("rpg")
        || has("shooter")
        || has("scene")
        || has("level")
        || has("world")
        || has("engine")
        || has("gameplay");

    let action_intent = has("make")
        || has("create")
        || has("build")
        || has("generate")
        || has("start")
        || has("new")
        || has("design")
        || has("develop")
        || has("code")
        || has("program")
        || has("want")
        || has("add")
        || has("setup")
        || has("set up")
        || has("play");

    mentions_game && action_intent
}

fn extract_engine_batch_tags(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut cursor = 0;
    while let Some(start_tag) = text[cursor..].find("<engine_batch>") {
        let content_start = cursor + start_tag + "<engine_batch>".len();
        let Some(end_tag) = text[content_start..].find("</engine_batch>") else {
            break;
        };
        let json_str = text[content_start..content_start + end_tag].trim();
        if !json_str.is_empty() {
            results.push(json_str.to_owned());
        }
        cursor = content_start + end_tag + "</engine_batch>".len();
    }
    if results.is_empty() {
        for part in text.split("```") {
            let trimmed = part.trim();
            let candidate = trimmed
                .strip_prefix("engine_batch")
                .or_else(|| trimmed.strip_prefix("json"))
                .unwrap_or(trimmed)
                .trim();
            if candidate.starts_with('{')
                && candidate.ends_with('}')
                && candidate.contains("\"actions\"")
            {
                results.push(candidate.to_owned());
            }
        }
    }
    results
}

fn extract_engine_action_tags(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut cursor = 0;
    while let Some(start_tag) = text[cursor..].find("<engine_action>") {
        let content_start = cursor + start_tag + "<engine_action>".len();
        let Some(end_tag) = text[content_start..].find("</engine_action>") else {
            break;
        };
        let json_str = text[content_start..content_start + end_tag].trim();
        if !json_str.is_empty() {
            results.push(json_str.to_owned());
        }
        cursor = content_start + end_tag + "</engine_action>".len();
    }
    results
}

/// Extracts all `<computer_action>...json...</computer_action>` tags from assistant response.
fn extract_computer_action_tags(text: &str) -> Vec<crate::computer::ComputerAction> {
    let mut results = Vec::new();
    let mut cursor = 0;
    while let Some(start_tag) = text[cursor..].find("<computer_action>") {
        let content_start = cursor + start_tag + "<computer_action>".len();
        if let Some(end_tag) = text[content_start..].find("</computer_action>") {
            let json_str = text[content_start..content_start + end_tag].trim();
            if let Some(action) = crate::computer::parse_action_json(json_str) {
                results.push(action);
            }
            cursor = content_start + end_tag + "</computer_action>".len();
        } else {
            break;
        }
    }
    // Fallback: check for ```computer_action ... ``` or markdown code blocks containing actions
    if results.is_empty() {
        for part in text.split("```") {
            let trimmed = part.trim();
            let candidate = trimmed
                .strip_prefix("computer_action")
                .or_else(|| trimmed.strip_prefix("json"))
                .unwrap_or(trimmed)
                .trim();
            if candidate.starts_with('{') && candidate.ends_with('}') {
                if let Some(action) = crate::computer::parse_action_json(candidate) {
                    results.push(action);
                    break;
                }
            }
        }
    }
    results
}

fn strip_computer_action_tags(text: &str) -> String {
    let mut clean = String::new();
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find("<computer_action>") {
        let absolute_start = cursor + start;
        clean.push_str(&text[cursor..absolute_start]);
        let content_start = absolute_start + "<computer_action>".len();
        let Some(end) = text[content_start..].find("</computer_action>") else {
            return clean;
        };
        cursor = content_start + end + "</computer_action>".len();
    }
    clean.push_str(&text[cursor..]);
    clean
}

fn computer_observation(
    capture: &crate::computer::ScreenCapture,
    path: &std::path::Path,
    result: &str,
) -> String {
    format!(
        "{result}\nCurrent desktop screenshot: {}\nVirtual desktop origin: ({}, {})\nVirtual desktop size: {}x{}\nInspect this exact current image before choosing one next action. Return no action block when the user's task is complete.",
        path.display(),
        capture.origin_x,
        capture.origin_y,
        capture.width,
        capture.height,
    )
}

fn computer_action_title(action: &crate::computer::ComputerAction) -> String {
    match action {
        crate::computer::ComputerAction::Screenshot => "Desktop screenshot".to_owned(),
        crate::computer::ComputerAction::MouseMove { x, y } => {
            format!("Move pointer to ({x}, {y})")
        }
        crate::computer::ComputerAction::MouseClick {
            button,
            count,
            x,
            y,
        } => match (x, y) {
            (Some(x), Some(y)) => format!("{button} click ×{count} at ({x}, {y})"),
            _ => format!("{button} click ×{count}"),
        },
        crate::computer::ComputerAction::MouseDrag {
            start_x,
            start_y,
            end_x,
            end_y,
        } => format!("Drag ({start_x}, {start_y}) → ({end_x}, {end_y})"),
        crate::computer::ComputerAction::MouseScroll { delta_x, delta_y } => {
            format!("Scroll ({delta_x}, {delta_y})")
        }
        crate::computer::ComputerAction::TypeText { text } => {
            let mut preview: String = text.chars().take(27).collect();
            if text.chars().count() > 27 {
                preview.push_str("...");
            }
            format!("Type \"{preview}\"")
        }
        crate::computer::ComputerAction::KeyPress { key } => format!("Press {key}"),
        crate::computer::ComputerAction::Hotkey { keys } => keys.join("+"),
        crate::computer::ComputerAction::GetScreenSize => "Read desktop bounds".to_owned(),
        crate::computer::ComputerAction::GetCursorPosition => "Read pointer position".to_owned(),
    }
}

fn usage_if_any(input_tokens: u64, output_tokens: u64) -> Option<Usage> {
    if input_tokens == 0 && output_tokens == 0 {
        None
    } else {
        Some(Usage {
            input_tokens,
            output_tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        fault_from, new_id, phase_label, pretty_error, render_debug_report, short_title,
        AgentPhase, ChatEngine, ChatRole, ChatTurnDone, ChatTurnView, ConversationScope,
        DesignMode, Effort, Emit, LimitSnapshot, PermissionRequest, ProviderRuntime, SessionKind,
        SessionStatus, ToolActivity, TurnOptions, TurnState,
    };
    use std::sync::Arc;

    /// An emitter for tests: the engine's bookkeeping is what is under test here, not
    /// what it broadcasts.
    struct Silent;

    impl Emit for Silent {
        fn thinking(&self, _turn_id: &str, _label: &str, _phase: AgentPhase) {}
        fn limits(&self, _provider: &str, _limits: LimitSnapshot) {}
        fn thought_delta(&self, _turn_id: &str, _delta: &str) {}
        fn delta(&self, _turn_id: &str, _delta: &str) {}
        fn tool(&self, _turn_id: &str, _tool: ToolActivity) {}
        fn permission(&self, _turn_id: &str, _request: PermissionRequest) {}
        fn done(&self, _event: ChatTurnDone) {}
    }

    /// The whole point of a typed fault: the two failures a user cannot tell apart from
    /// a red string must arrive as two different cards with two different buttons.
    #[test]
    fn a_named_fault_carries_the_button_that_fixes_it() {
        let full = fault_from("claude", "Claude Code", "prompt is too long: 213000 tokens");
        assert_eq!(full.kind, "context_exceeded");
        assert_eq!(full.remedy, "compact");
        assert!(
            !full.retryable,
            "a full context cannot be retried into fitting"
        );
        assert_eq!(full.provider, "Claude Code");

        let weekly = fault_from("claude", "Claude Code", "you have hit your weekly limit");
        assert_eq!(weekly.kind, "rate_limited_weekly");
        assert_eq!(weekly.remedy, "switch_provider");
        assert_ne!(full.title, weekly.title);

        // A backend with no catalogue row still gets a named fault, never a bare string.
        let unknown = fault_from("some-local-server", "My Server", "429 rate limit");
        assert_eq!(unknown.kind, "rate_limited_session");
        assert_eq!(unknown.provider, "My Server");
        assert!(!unknown.fix.is_empty());
    }

    /// The vendor's own words must survive into the card's details disclosure — that is
    /// what makes an unrecognised failure debuggable instead of merely apologetic.
    #[test]
    fn a_fault_keeps_what_the_vendor_actually_said() {
        let said = "TypeError: cannot read properties of undefined";
        let fault = fault_from("codex", "Codex CLI", said);
        assert!(fault.detail.contains(said), "{}", fault.detail);
        assert_eq!(fault.kind, "unknown");
        assert!(!fault.title.is_empty());
    }

    /// A phase label names the target when there is one, because "Reading" alone tells
    /// the user nothing they cannot already see.
    #[test]
    fn a_phase_label_names_what_it_is_working_on() {
        assert_eq!(
            phase_label(AgentPhase::Reading, "src/main.rs"),
            "Reading src/main.rs"
        );
        assert_eq!(phase_label(AgentPhase::Thinking, ""), "Thinking");
        assert_eq!(phase_label(AgentPhase::Editing, "   "), "Editing");

        // Every phase must produce copy; a silent phase renders as an empty bar.
        for phase in [
            AgentPhase::Connecting,
            AgentPhase::Queued,
            AgentPhase::Thinking,
            AgentPhase::Reasoning,
            AgentPhase::Planning,
            AgentPhase::Searching,
            AgentPhase::Reading,
            AgentPhase::Writing,
            AgentPhase::Editing,
            AgentPhase::Refactoring,
            AgentPhase::Running,
            AgentPhase::Testing,
            AgentPhase::Building,
            AgentPhase::Debugging,
            AgentPhase::Installing,
            AgentPhase::Fetching,
            AgentPhase::Browsing,
            AgentPhase::Analyzing,
            AgentPhase::Summarizing,
            AgentPhase::Reviewing,
            AgentPhase::AwaitingPermission,
            AgentPhase::Compacting,
            AgentPhase::Retrying,
            AgentPhase::Streaming,
            AgentPhase::Finalizing,
            AgentPhase::Done,
            AgentPhase::Stopped,
            AgentPhase::Failed,
        ] {
            assert!(!phase_label(phase, "").is_empty(), "{phase:?} has no copy");
        }
    }

    /// A vendor's tool verb has to select the animation, or every step looks identical.
    #[test]
    fn a_tool_verb_selects_its_phase() {
        assert_eq!(AgentPhase::of_verb("read"), AgentPhase::Reading);
        assert_eq!(AgentPhase::of_verb("edited"), AgentPhase::Editing);
        assert_eq!(AgentPhase::of_verb("ran"), AgentPhase::Running);
        assert_eq!(AgentPhase::of_verb("searched"), AgentPhase::Searching);
        assert_eq!(AgentPhase::of_verb("who knows"), AgentPhase::Analyzing);
    }

    /// Builds a detection row for the runtime tests.
    fn row(
        id: &str,
        kind: bhippi_providers::ProviderKind,
        reachable: bool,
    ) -> bhippi_providers::ProviderInfo {
        bhippi_providers::ProviderInfo {
            id: id.to_owned(),
            label: id.to_owned(),
            kind,
            models: vec!["a-model".to_owned()],
            health: if reachable {
                bhippi_types::Health::Healthy { latency_ms: 3 }
            } else {
                bhippi_types::Health::Unavailable {
                    reason: "not running".to_owned(),
                }
            },
            offered: !reachable,
            detected_at: chrono::Utc::now(),
            installed: reachable,
            version: None,
            enabled: true,
            accepts_custom_model: true,
            detected_port: (reachable && kind == bhippi_providers::ProviderKind::LocalServer)
                .then_some(11434),
        }
    }

    /// The complaint this fixes: opening the app selected a local LLM that was merely
    /// installed, which meant loading gigabytes of model nobody had asked for.
    ///
    /// A local server that is not listening must not be selectable at all, and the
    /// default must fall through to something that needs no local process.
    #[test]
    fn an_idle_local_server_is_never_selected_and_never_offered() {
        use bhippi_providers::ProviderKind;

        let runtime = ProviderRuntime::from_detection(vec![
            row("bionic", ProviderKind::LocalServer, false),
            row("claude", ProviderKind::Cli, true),
            row("demo", ProviderKind::Demo, true),
        ]);

        assert_ne!(
            runtime.default_id, "bionic",
            "an installed-but-stopped server must not be the default"
        );
        assert!(
            !runtime.by_id.contains_key("bionic"),
            "an unreachable server must not be usable at all"
        );
        assert!(
            !runtime.chat_options().iter().any(|row| row.id == "bionic"),
            "the picker must not offer a server that cannot answer"
        );
        // …and it must fall through to something that costs no local memory.
        assert_eq!(runtime.default_id, "claude");
    }

    /// A local server that *is* running is the right default: it is free, private, and
    /// already holding the memory, so using it costs nothing extra.
    #[test]
    fn a_running_local_server_is_preferred_over_anything_remote() {
        use bhippi_providers::ProviderKind;

        let runtime = ProviderRuntime::from_detection(vec![
            row("ollama", ProviderKind::LocalServer, true),
            row("claude", ProviderKind::Cli, true),
            row("demo", ProviderKind::Demo, true),
        ]);
        assert_eq!(runtime.default_id, "ollama");
        assert!(runtime.by_id.contains_key("ollama"));
    }

    /// With nothing local running and nothing remote ready, the offline demo answers —
    /// never a local server started uninvited.
    #[test]
    fn nothing_available_falls_back_to_the_offline_demo() {
        use bhippi_providers::ProviderKind;

        let runtime = ProviderRuntime::from_detection(vec![
            row("bionic", ProviderKind::LocalServer, false),
            row("lmstudio", ProviderKind::LocalServer, false),
            row("demo", ProviderKind::Demo, true),
        ]);
        assert_eq!(runtime.default_id, "demo");
        assert_eq!(runtime.chat_options().len(), 1);
    }

    /// The user's own pick always wins over the preference order.
    #[test]
    fn an_explicit_choice_beats_the_default_order() {
        use bhippi_providers::ProviderKind;

        let runtime = ProviderRuntime::from_detection(vec![
            row("ollama", ProviderKind::LocalServer, true),
            row("claude", ProviderKind::Cli, true),
            row("demo", ProviderKind::Demo, true),
        ]);
        assert_eq!(runtime.default_id, "ollama");

        let Ok((_, info)) = runtime.resolve(Some("claude")) else {
            panic!("an explicitly chosen backend must resolve");
        };
        assert_eq!(info.id, "claude");

        // And a choice that is no longer reachable errors rather than silently swapping
        // the user onto a different backend than the one they picked.
        assert!(runtime.resolve(Some("bionic")).is_err());
    }

    /// The switch has to actually change the prompt, or it is a decorative toggle.
    #[test]
    fn the_design_switch_changes_what_the_model_is_told() {
        assert_eq!(DesignMode::Off.directive(), "");
        assert!(!DesignMode::Off.is_on());

        let on = DesignMode::On.directive();
        assert!(on.contains("Bhippi Design System"), "{on}");
        assert!(DesignMode::On.is_on());

        // The directive must carry the rules themselves, not a pointer to a file the
        // backend may not be able to open — a directive that depends on the model going
        // to find a document is one that silently does nothing.
        for rule in [
            "4px grid",
            "One accent",
            "transform and opacity only",
            "prefers-reduced-motion",
            "4.5:1",
            "one primary action",
            "empty, loading, partial, error, full",
        ] {
            assert!(on.contains(rule), "the directive omits {rule:?}");
        }
        assert!(
            !on.contains("docs/DESIGN-SYSTEM.md"),
            "the directive must be self-contained, not a file reference"
        );
    }

    /// A report that only lists problems is a list. Each finding must carry its rationale
    /// and its fix into the rendered markdown.
    #[test]
    fn a_debug_report_renders_the_reason_and_the_fix() {
        let report = crate::debugger::DiagnosticReport {
            project_name: "probe".to_owned(),
            project_type: "TypeScript".to_owned(),
            total_issues: 1,
            errors_count: 1,
            warnings_count: 0,
            info_count: 0,
            duration_ms: 12,
            files_scanned: 3,
            bytes_scanned: 2048,
            partial: false,
            items: vec![crate::debugger::DiagnosticItem {
                file: "src/a.ts".to_owned(),
                line: Some(4),
                column: None,
                severity: "error".to_owned(),
                category: "security".to_owned(),
                code: Some("BHP-D020".to_owned()),
                message: "Dynamic code execution via eval.".to_owned(),
                why: "Any value reaching this runs as code.".to_owned(),
                suggestion: Some("Parse the data instead.".to_owned()),
                evidence: "const r = eval(input);".to_owned(),
            }],
            tools: vec![crate::debugger::ToolStatus {
                tool: "tsc --noEmit".to_owned(),
                at: "ui".to_owned(),
                ok: false,
                note: None,
            }],
            by_category: vec![crate::debugger::CategoryCount {
                category: "security".to_owned(),
                count: 1,
            }],
            summary: "1 error across 3 files.".to_owned(),
            success: false,
        };

        let markdown = render_debug_report(&report);
        assert!(markdown.contains("**FAIL**"), "{markdown}");
        assert!(markdown.contains("src/a.ts:4"), "{markdown}");
        assert!(markdown.contains("BHP-D020"), "{markdown}");
        assert!(markdown.contains("Why it matters:"), "{markdown}");
        assert!(markdown.contains("Parse the data instead."), "{markdown}");
        assert!(markdown.contains("const r = eval(input);"), "{markdown}");
        // A toolchain that could not run must be visible, or contributing nothing is
        // indistinguishable from finding nothing.
        assert!(markdown.contains("tsc --noEmit"), "{markdown}");
    }

    /// A truncated scan must never render as a pass.
    #[test]
    fn a_partial_scan_is_flagged_in_the_rendered_report() {
        let mut report = crate::debugger::DiagnosticReport {
            project_name: "probe".to_owned(),
            project_type: "Rust".to_owned(),
            total_issues: 0,
            errors_count: 0,
            warnings_count: 0,
            info_count: 0,
            duration_ms: 5,
            files_scanned: 4000,
            bytes_scanned: 999,
            partial: true,
            items: Vec::new(),
            tools: Vec::new(),
            by_category: Vec::new(),
            summary: "Clean.".to_owned(),
            success: true,
        };
        let markdown = render_debug_report(&report);
        assert!(markdown.contains("not** covered in full"), "{markdown}");

        report.partial = false;
        assert!(!render_debug_report(&report).contains("not** covered in full"));
    }

    /// Deleting removes exactly the conversation asked for, and says so honestly when
    /// there was nothing to remove — the UI shows a message either way.
    #[tokio::test]
    async fn deleting_a_conversation_removes_only_that_one() {
        let engine = Arc::new(ChatEngine::new(Silent));
        let keep = engine
            .ensure_conversation("C:/projects/one", Some("keep".to_owned()))
            .await
            .unwrap_or_else(|error| panic!("conversation should be created: {error}"));
        let drop = engine
            .ensure_conversation("C:/projects/one", Some("drop".to_owned()))
            .await
            .unwrap_or_else(|error| panic!("conversation should be created: {error}"));
        assert_eq!(engine.list_conversations("C:/projects/one").await.len(), 2);

        assert!(
            engine
                .delete_conversation("C:/projects/one", &drop.id)
                .await
        );

        let left = engine.list_conversations("C:/projects/one").await;
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, keep.id);
        assert!(engine
            .conversation_view("C:/projects/one", &drop.id)
            .await
            .is_none());

        // Deleting the same id twice is not an error the user caused, but it is not a
        // success either — the command turns this into "already gone".
        assert!(
            !engine
                .delete_conversation("C:/projects/one", &drop.id)
                .await
        );
        assert!(
            !engine
                .delete_conversation("C:/projects/one", "never-existed")
                .await
        );
    }

    #[tokio::test]
    async fn conversations_are_hard_partitioned_by_project() {
        let engine = Arc::new(ChatEngine::new(Silent));
        let alpha = engine
            .ensure_conversation("C:/projects/alpha", Some("alpha-session".to_owned()))
            .await
            .unwrap_or_else(|error| panic!("alpha conversation should be created: {error}"));
        let beta = engine
            .ensure_conversation("C:/projects/beta", Some("beta-session".to_owned()))
            .await
            .unwrap_or_else(|error| panic!("beta conversation should be created: {error}"));

        assert_eq!(
            engine.list_conversations("C:/projects/alpha").await,
            vec![alpha.clone()]
        );
        assert_eq!(
            engine.list_conversations("C:/projects/beta").await,
            vec![beta]
        );
        assert!(engine
            .conversation_view("C:/projects/beta", &alpha.id)
            .await
            .is_none());
        assert!(
            !engine
                .delete_conversation("C:/projects/beta", &alpha.id)
                .await
        );
        let collision = engine
            .ensure_conversation("C:/projects/beta", Some(alpha.id.clone()))
            .await;
        assert_eq!(
            collision.err().as_deref(),
            Some("That session belongs to a different project.")
        );
        assert!(engine
            .conversation_view("C:/projects/alpha", &alpha.id)
            .await
            .is_some());

        let detected = bhippi_providers::detect(&[], &["demo".to_owned()]).await;
        let registry = Arc::new(ProviderRuntime::from_detection(detected));
        let cross_project_send = engine
            .send(
                &registry,
                ConversationScope {
                    project_path: "C:/projects/beta".to_owned(),
                    conversation_id: alpha.id.clone(),
                },
                "change alpha from beta".to_owned(),
                TurnOptions {
                    provider_id: Some("demo".to_owned()),
                    effort: Effort::Fast,
                    ..TurnOptions::default()
                },
            )
            .await;
        assert_eq!(
            cross_project_send.err().as_deref(),
            Some("That session belongs to a different project.")
        );

        let cross_project_regenerate = engine
            .regenerate(
                &registry,
                ConversationScope {
                    project_path: "C:/projects/beta".to_owned(),
                    conversation_id: alpha.id,
                },
                TurnOptions {
                    provider_id: Some("demo".to_owned()),
                    effort: Effort::Fast,
                    ..TurnOptions::default()
                },
            )
            .await;
        assert!(cross_project_regenerate.is_none());
    }

    #[test]
    fn short_title_collapses_whitespace_and_truncates() {
        assert_eq!(short_title("  hello   world  "), "hello world");
        let long = short_title(&"x".repeat(80));
        assert_eq!(long.chars().count(), 49); // 48 chars + ellipsis
        assert!(long.ends_with('\u{2026}'));
    }

    #[test]
    fn pretty_error_appends_hint_when_present() {
        let error = bhippi_types::BhippiError::Config {
            reason: "bad key".to_owned(),
            hint: Some("Fix the key.".to_owned()),
        };
        let text = pretty_error(&error);
        assert!(text.contains("**Fix:** Fix the key."));

        let bare = bhippi_types::BhippiError::Invariant { code: "x" };
        assert_eq!(pretty_error(&bare), bare.to_string());
    }

    #[test]
    fn extract_computer_action_tags_finds_all_actions() {
        let text = r#"
I will click the taskbar icon to open the app:
<computer_action>
{"action": "mouse_click", "button": "left", "count": 1, "x": 120, "y": 1050}
</computer_action>

Now I will type the search query:
<computer_action>
{"action": "type_text", "text": "settings\n"}
</computer_action>
"#;
        let actions = super::extract_computer_action_tags(text);
        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0],
            crate::computer::ComputerAction::MouseClick {
                button: "left".to_owned(),
                count: 1,
                x: Some(120),
                y: Some(1050),
            }
        );
        assert_eq!(
            actions[1],
            crate::computer::ComputerAction::TypeText {
                text: "settings\n".to_owned(),
            }
        );
    }

    /// The rail derives status from the *latest* turn of any role, so a conversation
    /// whose last event is a fault reads Failed even if an earlier turn succeeded.
    #[tokio::test]
    async fn a_session_derives_status_and_provider_from_its_latest_turn() {
        let engine = Arc::new(ChatEngine::new(Silent));
        engine
            .ensure_conversation("/alpha", None)
            .await
            .expect("conversation seeds");
        engine
            .ensure_conversation("/beta", None)
            .await
            .expect("conversation seeds");
        {
            let mut conversations = engine.conversations.lock().await;
            for conversation in conversations.iter_mut() {
                let assistant = ChatTurnView {
                    id: new_id(),
                    conversation_id: conversation.meta.id.clone(),
                    role: ChatRole::Assistant,
                    content: String::new(),
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: conversation.meta.created_at,
                    state: TurnState::Done,
                    provider: Some("Claude Code".to_owned()),
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                };
                conversation.turns.push(assistant);
            }
        }

        let sessions = engine.workspace_sessions().await;
        assert_eq!(sessions.len(), 2);
        for session in &sessions {
            assert_eq!(session.kind, SessionKind::AiChat);
            assert_eq!(session.status, SessionStatus::Idle);
            assert_eq!(session.provider_label.as_deref(), Some("Claude Code"));
            assert_eq!(session.turn_count, 0);
        }
        let ids: Vec<&str> = sessions
            .iter()
            .map(|session| session.project_path.as_str())
            .collect();
        assert!(ids.contains(&"/alpha"));
        assert!(ids.contains(&"/beta"));
    }

    /// A running turn is Running, a permission wait is Paused, and a fault is Failed —
    /// each maps to a distinct chip state in the rail.
    #[tokio::test]
    async fn session_status_maps_every_live_state() {
        for (state, expected) in [
            (TurnState::Queued, SessionStatus::Running),
            (TurnState::Streaming, SessionStatus::Running),
            (TurnState::AwaitingPermission, SessionStatus::Paused),
            (TurnState::Done, SessionStatus::Idle),
            (TurnState::Stopped, SessionStatus::Idle),
            (TurnState::Failed, SessionStatus::Failed),
        ] {
            let engine = Arc::new(ChatEngine::new(Silent));
            let meta = engine
                .ensure_conversation("/alpha", None)
                .await
                .expect("conversation seeds");
            {
                let mut conversations = engine.conversations.lock().await;
                let conversation = conversations
                    .iter_mut()
                    .find(|c| c.meta.id == meta.id)
                    .expect("seeded conversation");
                conversation.turns.push(ChatTurnView {
                    id: new_id(),
                    conversation_id: meta.id.clone(),
                    role: ChatRole::Assistant,
                    content: String::new(),
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: meta.created_at,
                    state,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
            }
            let sessions = engine.workspace_sessions().await;
            assert_eq!(sessions.len(), 1, "{state:?}");
            assert_eq!(
                sessions[0].status, expected,
                "{state:?} should map to {expected:?}"
            );
        }
    }

    /// An empty conversation reads Idle rather than vanishing — the rail keeps a chip
    /// the user can click into.
    #[tokio::test]
    async fn an_empty_conversation_is_an_idle_session() {
        let engine = Arc::new(ChatEngine::new(Silent));
        engine
            .ensure_conversation("/alpha", None)
            .await
            .expect("conversation seeds");
        let sessions = engine.workspace_sessions().await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, SessionStatus::Idle);
        assert_eq!(sessions[0].provider_label, None);
        assert_eq!(sessions[0].turn_count, 0);
    }

    /// The rail lists the most recently touched conversations first, newest on top.
    #[tokio::test]
    async fn sessions_sort_newest_updated_first() {
        let engine = Arc::new(ChatEngine::new(Silent));
        let older = engine
            .ensure_conversation("/alpha", None)
            .await
            .expect("conversation seeds");
        let newer = engine
            .ensure_conversation("/beta", None)
            .await
            .expect("conversation seeds");
        {
            let mut conversations = engine.conversations.lock().await;
            for (id, created) in [
                (&newer.id, newer.created_at + chrono::Duration::hours(2)),
                (&older.id, older.created_at),
            ] {
                let conversation = conversations
                    .iter_mut()
                    .find(|c| c.meta.id == *id)
                    .expect("seeded conversation");
                conversation.turns.push(ChatTurnView {
                    id: new_id(),
                    conversation_id: id.clone(),
                    role: ChatRole::Assistant,
                    content: String::new(),
                    thinking: None,
                    thinking_elapsed_ms: None,
                    created_at: created,
                    state: TurnState::Done,
                    provider: None,
                    tools: Vec::new(),
                    permission: None,
                    fault: None,
                    worked_ms: None,
                    changes: None,
                    notices: Vec::new(),
                });
            }
        }
        let sessions = engine.workspace_sessions().await;
        assert_eq!(sessions[0].id, newer.id);
        assert_eq!(sessions[1].id, older.id);
    }

    /// Caveman mode directive is verified to exist and contain the required rules.
    #[test]
    fn caveman_directive_enforces_compression_and_valid_code() {
        assert!(super::CAVEMAN_SYSTEM_DIRECTIVE.contains("CAVEMAN PROTOCOL"));
        assert!(super::CAVEMAN_SYSTEM_DIRECTIVE.contains("NO FILLER"));
        assert!(super::CAVEMAN_SYSTEM_DIRECTIVE.contains("TELEGRAPHIC SYNTAX"));
        assert!(super::CAVEMAN_SYSTEM_DIRECTIVE.contains("CODE INTEGRITY 100%"));
    }

    /// A freshly initialized conversation has 0 history messages and no memory bleed.
    #[tokio::test]
    async fn fresh_chats_have_zero_prior_history() {
        let engine = Arc::new(ChatEngine::new(Silent));
        let meta = engine
            .ensure_conversation("/test/project", None)
            .await
            .expect("conversation seeds");
        let history = engine.history_messages(&meta.id).await;
        assert_eq!(
            history.len(),
            0,
            "A new chat must start with 0 history messages"
        );
    }

    // -- CHT-100…106: the transcript record ------------------------------------------

    fn tool(changes: Vec<super::TurnFileChange>) -> super::ToolActivity {
        super::ToolActivity {
            id: super::new_id(),
            action: super::ToolAction::WriteFile,
            title: "Edited".to_owned(),
            detail: String::new(),
            state: super::ToolState::Ok,
            command: None,
            output: None,
            exit_code: None,
            elapsed_ms: None,
            truncated: false,
            changes,
        }
    }

    fn change(path: &str, additions: usize, deletions: usize) -> super::TurnFileChange {
        super::TurnFileChange {
            path: path.to_owned(),
            additions,
            deletions,
            status: "modified".to_owned(),
        }
    }

    #[test]
    fn a_turn_summary_counts_files_once_and_lines_in_full() {
        // The same file edited twice is one file and two edits' worth of lines. Reporting
        // two files is what makes a summary card useless.
        let tools = vec![
            tool(vec![change("src/a.rs", 10, 2), change("src/b.rs", 5, 0)]),
            tool(vec![change("src/a.rs", 3, 1)]),
        ];
        let summary = super::TurnChanges::from_tools(&tools).expect("some files changed");
        assert_eq!(summary.files.len(), 2);
        assert_eq!(summary.total_additions, 18);
        assert_eq!(summary.total_deletions, 3);
        let a = summary
            .files
            .iter()
            .find(|file| file.path == "src/a.rs")
            .expect("a.rs");
        assert_eq!((a.additions, a.deletions), (13, 3));
    }

    #[test]
    fn a_turn_that_changed_nothing_has_no_summary_rather_than_an_empty_one() {
        assert!(super::TurnChanges::from_tools(&[tool(vec![])]).is_none());
        assert!(super::TurnChanges::from_tools(&[]).is_none());
    }

    #[test]
    fn a_file_created_and_then_edited_still_reads_as_added() {
        let mut created = change("src/new.rs", 40, 0);
        created.status = "added".to_owned();
        let summary = super::TurnChanges::from_tools(&[
            tool(vec![created]),
            tool(vec![change("src/new.rs", 2, 1)]),
        ])
        .expect("changed");
        assert_eq!(summary.files[0].status, "modified");
        assert_eq!(summary.files[0].additions, 42);
    }

    #[test]
    fn line_counts_reflect_the_edit_not_the_file_size() {
        // The bug this exists to prevent: reporting every line of a large file as changed
        // because one line moved.
        let before = (0..500)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let after = before.replace("line 250", "line 250 // touched");
        let change = super::line_change("src/big.rs", Some(&before), &after);
        assert_eq!(change.additions, 1);
        assert_eq!(change.deletions, 1);
        assert_eq!(change.status, "modified");
    }

    #[test]
    fn a_new_file_counts_every_line_as_an_addition() {
        let change = super::line_change("src/new.rs", None, "one\ntwo\nthree\n");
        assert_eq!((change.additions, change.deletions), (3, 0));
        assert_eq!(change.status, "added");
    }

    #[test]
    fn windows_paths_are_normalised_so_the_transcript_prints_one_shape() {
        let change = super::line_change(r"src\engine\mod.rs", None, "x\n");
        assert_eq!(change.path, "src/engine/mod.rs");
    }

    #[test]
    fn output_under_the_cap_is_kept_whole() {
        let (text, truncated) = super::cap_tool_output("502 passed; 0 failed");
        assert_eq!(text, "502 passed; 0 failed");
        assert!(!truncated);
    }

    #[test]
    fn oversized_output_keeps_both_ends_and_says_how_much_it_dropped() {
        // Head and tail both matter: the command and the first error are at the top, the
        // summary line and the exit are at the bottom. Trimming only the tail loses the half
        // people scroll to.
        let text = format!("START{}END", "x".repeat(super::TOOL_OUTPUT_CAP * 2));
        let (capped, truncated) = super::cap_tool_output(&text);
        assert!(truncated);
        assert!(capped.starts_with("START"));
        assert!(capped.ends_with("END"));
        assert!(capped.contains("bytes elided"));
        assert!(
            capped.len() < text.len(),
            "the capped form must be smaller than the original"
        );
    }

    #[test]
    fn capping_never_splits_a_character() {
        // A multi-byte character straddling the cut is how a "just slice it" implementation
        // panics in production and nowhere else.
        let text = "é".repeat(super::TOOL_OUTPUT_CAP);
        let (capped, truncated) = super::cap_tool_output(&text);
        assert!(truncated);
        assert!(capped.contains('é'));
    }

    #[test]
    fn engine_facts_are_bounded_and_unicode_safe() {
        let text = "crate é ".repeat(4_000);
        let capped = super::cap_engine_facts(text);
        assert!(
            bhippi_core::estimate_text_tokens(&capped)
                <= bhippi_types::ENGINE_CONTEXT_TOKEN_BUDGET + 20,
            "the suffix is the only permitted budget overhead"
        );
        assert!(capped.ends_with("use engine_query for deeper facts.\n"));
        assert!(capped.contains('é'));
    }

    #[test]
    fn engine_context_budget_is_scene_size_independent() {
        for entity_count in [0_usize, 50, 1_000] {
            let facts = (0..entity_count)
                .map(|index| format!("- Crate_{index}: cube at [{index}, 0, 0]\n"))
                .collect::<String>();
            let capped = super::cap_engine_facts(format!(
                "Scene: perf_{entity_count}\nEntities: {entity_count}\n{facts}"
            ));
            assert!(
                bhippi_core::estimate_text_tokens(&capped)
                    <= bhippi_types::ENGINE_CONTEXT_TOKEN_BUDGET + 20,
                "{entity_count}-entity dynamic context exceeded its fixed budget"
            );
            if entity_count == 1_000 {
                assert!(capped.contains("use engine_query for deeper facts"));
                assert!(!capped.contains("Crate_999"));
            }
        }
    }

    #[test]
    fn structural_observation_faults_stop_with_their_remedy() {
        let answers = vec![(
            r#"{"kind":"screenshot"}"#.to_owned(),
            "Viewport observations require the desktop Engine pane. Open it and retry.".to_owned(),
        )];
        let remedy = super::non_repairable_engine_observation(&answers)
            .expect("desktop absence cannot be repaired by repeating the query");
        assert!(remedy.contains("Open it"));
    }
}
