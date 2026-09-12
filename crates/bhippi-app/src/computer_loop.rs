//! The decision half of the Computer Use loop (ADR-0048).
//!
//! `chat.rs` owns the effects — call the provider, capture the screen, inject the input,
//! draw the overlay. Everything it has to *decide* lives here, as pure functions over plain
//! data: what a reply means, whether an action needs an answer from the user, what the model
//! is told next, when the screen has settled, and how the turn is finally described.
//!
//! The split is the point. Before this module the loop was five hundred lines inlined in a
//! seven-thousand-line file, reachable only with a real provider and a real desktop, and so
//! it had no tests at all — which is why every fault ADR-0048 lists survived a live
//! demonstration. All of it is exercised below with no desktop, no model and no clock.
//!
//! The three rules the loop now keeps, and did not before:
//!
//! * **A reply that cannot be executed is corrected, not fatal.** [`interpret_reply`] tells a
//!   deliberate finish from a protocol slip, and a slip becomes a [`Repair`](ReplyVerdict)
//!   round that costs no action budget.
//! * **A gated action asks.** [`GateLedger`] turns "Full PC Access is off" from an error into
//!   a question, and confirms a consequential target once rather than once per action.
//! * **The ending is honest.** [`TurnReport`] can only claim success for
//!   [`ComputerOutcome::Completed`]; a cap, a decline and a lost protocol each say what was
//!   actually verified.

use crate::computer::{parse_proposed_action, ComputerAction, ProposedAction, ScreenCapture};
use bhippi_types::{
    ComputerActionClass, ComputerOutcome, ComputerScope, COMPUTER_MAX_ACTIONS_PER_TURN,
    COMPUTER_VERBATIM_ROUNDS,
};
use std::collections::BTreeSet;

/// The desktop protocol: the verbs, the coordinate contract, the budget, the gate.
pub const PROTOCOL: &str = include_str!("../../../prompts/chat-computer-use.md");

/// The identity a desktop turn is given: *this turn drives a screen, it does not edit a
/// project*. Separate from the protocol because the protocol is also read by the engine
/// scope, and only a desktop turn replaces the studio identity with this one.
pub const TURN_IDENTITY: &str = include_str!("../../../prompts/chat-computer-turn.md");

/// The whole system prompt a Computer Use turn is given (ADR-0059).
///
/// Short and exclusive on purpose: the identity, the desktop protocol, and the line saying
/// what input is authorised. Nothing that names a verb this loop cannot execute — an engine
/// batch, an asset import, a Sketchfab search, a skill — because a vocabulary offered is a
/// vocabulary used, and using one of those here ends the turn with nothing done.
///
/// It lives beside `observation` and `RepairKind::message` because it is the same job: this
/// module owns everything the model is told. It is also what the live test builds, so the
/// prompt under test is the prompt that ships.
///
/// `protocol_block` carries [`PROTOCOL`], the stand-in-driver note when another backend is
/// flying the desktop, and the authorisation line; the caller supplies it already prefixed
/// with its own blank line, which is why this is a concatenation rather than a join.
#[must_use]
pub fn turn_system(workspace: &str, protocol_block: &str) -> String {
    format!(
        "{}{protocol_block}",
        TURN_IDENTITY.replace("{{workspace}}", workspace)
    )
}

/// The tag the protocol wraps an action in.
pub const ACTION_OPEN: &str = "<computer_action>";
pub const ACTION_CLOSE: &str = "</computer_action>";

/// One correct action, quoted back to a model that lost the shape.
const EXAMPLE: &str = r#"<computer_action>
{"action":"mouse_click","button":"left","count":1,"x":640,"y":400,"reason":"open the File menu"}
</computer_action>"#;

// ------------------------------------------------------------------ reading a reply

/// Why a reply could not be executed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepairKind {
    /// A tag or fenced block was there, and what was inside it is not an action.
    Unparseable { offending: String },
    /// More than one action in a single reply.
    TooMany { count: usize },
    /// Action-shaped JSON with no tag around it.
    Unfenced,
    /// A legal action that this scope does not have a verb for.
    OutOfScope { action: String },
    /// A tag from one of Bhippi's *other* protocols, in place of an action.
    WrongProtocol { tag: String },
    /// Nothing came back at all.
    Empty,
}

impl RepairKind {
    /// What the model is told, and how to get back on protocol.
    ///
    /// Every message names the mistake, then the correction, then the example — a repair
    /// that only says "invalid" costs another round to discover what was wrong.
    #[must_use]
    pub fn message(&self, scope: ComputerScope) -> String {
        match self {
            Self::Unparseable { offending } => format!(
                "That action block could not be read, so nothing was done and the screen is \
                 unchanged. It said:\n{offending}\nReturn exactly one action in the shape \
                 below, using one of the listed verbs.\n{EXAMPLE}"
            ),
            Self::TooMany { count } => format!(
                "That reply carried {count} action blocks; none was executed. Every action is \
                 chosen against the screen as it is now, so send one, look at the result, \
                 then send the next.\n{EXAMPLE}"
            ),
            Self::Unfenced => format!(
                "That reply contained action-shaped JSON that was not wrapped in an action \
                 block, so nothing was done. Wrap it exactly like this — or, if the task is \
                 finished, reply in plain English with no JSON at all.\n{EXAMPLE}"
            ),
            Self::OutOfScope { action } => match scope {
                ComputerScope::GameWindow => format!(
                    "`{action}` does not exist in this turn. This turn can only see and play \
                     the game window Bhippi launched — it cannot start programs, open pages \
                     or move to another window. Use the keyboard and pointer verbs, or say \
                     what you observed if you are done."
                ),
                ComputerScope::Desktop => format!(
                    "`{action}` is not available in this turn. Choose another verb from the \
                     list, or say what you observed if you are done."
                ),
            },
            Self::WrongProtocol { tag } if scope == ComputerScope::Desktop => {
                include_str!("../../../prompts/chat-computer-protocol-repair.md")
                    .replace("{{tag}}", tag)
                    .replace("{{example}}", EXAMPLE)
            }
            Self::WrongProtocol { tag } => format!(
                "`<{tag}>` belongs to one of Bhippi's other protocols, and that protocol does \
                 not exist in this turn — nothing was sent and the screen is unchanged. This \
                 turn can only look at the screen and drive it. Return exactly one action \
                 block, or, if this task cannot be carried any further from the screen, say \
                 so in plain English with no tags of any kind.\n{EXAMPLE}"
            ),
            Self::Empty => format!(
                "That reply was empty, so nothing was done and the screen is unchanged. \
                 Return exactly one action block, or say in plain English what you observed \
                 if the task is done.\n{EXAMPLE}"
            ),
        }
    }

    /// A short line for the transcript, so a repair round is visible rather than a silent
    /// pause in a run the user is watching.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::Unparseable { .. } => "Action could not be read — asked again".to_owned(),
            Self::TooMany { count } => format!("{count} actions in one reply — asked for one"),
            Self::Unfenced => "Action was not wrapped — asked again".to_owned(),
            Self::OutOfScope { action } => format!("`{action}` is not available here"),
            Self::WrongProtocol { tag } => {
                format!("`<{tag}>` is not a Computer Use action — asked again")
            }
            Self::Empty => "Empty reply — asked again".to_owned(),
        }
    }
}

/// What one model reply means.
#[derive(Clone, Debug, PartialEq)]
pub enum ReplyVerdict {
    /// Execute this, then look again.
    Act {
        proposed: Box<ProposedAction>,
        /// Anything the model wrote outside the action block. Narration, not a violation.
        narration: String,
    },
    /// The model says it is finished, and this is what it says it observed.
    Complete { summary: String },
    /// The model looked, and what it found has to be fixed in the project rather than on
    /// the screen. The desktop phase ends and the same turn continues in the engine
    /// protocol, carrying `summary` as the observation (ADR-0063).
    ///
    /// The exact mirror of `<computer_request>` going the other way (SPA-301): one
    /// vocabulary at a time, a prompt replaced rather than appended, and the hand-over is
    /// the model's own decision rather than something sniffed out of its prose.
    HandBack { reason: String, summary: String },
    /// Nothing was executed; ask again with this correction.
    Repair(RepairKind),
}

/// Read a model reply.
///
/// The distinction this function exists for: an empty action list used to mean *finished*,
/// which is also what a mistyped verb produced. A run could therefore end with a confident
/// completion summary and nothing done. Here a reply that *looks like it was trying to act*
/// is a repair, and only a reply with no action shape at all is a completion.
///
/// ADR-0059 added the third way a reply can be neither: a tag from one of Bhippi's *other*
/// protocols. That is a model working in the wrong vocabulary, which is a slip like any
/// other — and reading it as a finish is how a run reported "Done · 1 of 24 steps" having
/// done nothing at all.
#[must_use]
pub fn interpret_reply(raw: &str, scope: ComputerScope) -> ReplyVerdict {
    let blocks = tagged_blocks(raw);
    if blocks.len() > 1 {
        return ReplyVerdict::Repair(RepairKind::TooMany {
            count: blocks.len(),
        });
    }

    if let Some(body) = blocks.first() {
        // Scrubbed of every protocol: narration is shown to the user as the caption beside
        // the frame, and a tag body is not a sentence.
        let narration = strip_protocol_tags(raw).trim().to_owned();
        return match parse_proposed_action(body) {
            Some(proposed) => verdict_for(proposed, narration, scope),
            None => ReplyVerdict::Repair(RepairKind::Unparseable {
                offending: clip(body, 240),
            }),
        };
    }

    // No tag. A fenced block is tolerated — several CLIs strip unknown tags — but a bare
    // object in prose is not, because it is indistinguishable from the model describing an
    // action it is *about* to take.
    if let Some(fenced) = fenced_action_block(raw) {
        let narration = strip_protocol_tags(&raw.replace(&fenced, ""))
            .trim()
            .to_owned();
        return match parse_proposed_action(&fenced) {
            Some(proposed) => verdict_for(proposed, narration, scope),
            None => ReplyVerdict::Repair(RepairKind::Unparseable {
                offending: clip(&fenced, 240),
            }),
        };
    }

    if looks_like_an_action(raw) {
        return ReplyVerdict::Repair(RepairKind::Unfenced);
    }

    // The fault this whole check exists for (ADR-0059). A desktop turn used to be handed
    // the studio's engine, asset and Sketchfab protocols alongside this one, so a model
    // asked to "make the joystick work" answered with `<engine_query>` — no action block,
    // which read as *finished*, and a run ended "Done · 1 of 24 steps" having done nothing.
    // The prompt no longer offers those vocabularies; this makes the loop unable to mistake
    // one for a completion even if some future prompt leaks one again.
    // Before the foreign-tag check, because this one is not a slip. It is the only tag from
    // outside the desktop protocol that means something here: "I have seen what I needed;
    // the fix is in the code." Without it a turn asked to look at the screen AND change the
    // project could only ever do the first half and tell the user to ask again — which is
    // exactly what happened to the owner.
    if let Some(reason) = engine_request_reason(raw) {
        return ReplyVerdict::HandBack {
            reason,
            summary: strip_engine_request(raw).trim().to_owned(),
        };
    }

    if let Some(tag) = foreign_protocol_tag(raw) {
        return ReplyVerdict::Repair(RepairKind::WrongProtocol {
            tag: tag.to_owned(),
        });
    }

    // Silence is not a summary. Treating it as one ended the turn with the handoff note
    // standing in for an observation nobody made.
    if raw.trim().is_empty() {
        return ReplyVerdict::Repair(RepairKind::Empty);
    }

    ReplyVerdict::Complete {
        summary: strip_protocol_tags(raw).trim().to_owned(),
    }
}

/// A desktop phase that ended by asking for the project (ADR-0063).
///
/// Carried out of the loop by an out-parameter rather than squeezed into `Outcome`: every
/// other exit from a Computer Use turn is genuinely an ending, and widening the type they all
/// return would put "and then some more work happened" in front of a dozen call sites that
/// can never produce it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandBack {
    /// Why the screen cannot finish this — the model's own sentence.
    pub reason: String,
    /// What it saw, which is the thing the project phase needs and could not get itself.
    pub observation: String,
}

pub const ENGINE_REQUEST_OPEN: &str = "<engine_request>";
pub const ENGINE_REQUEST_CLOSE: &str = "</engine_request>";

/// The reason inside an `<engine_request>`, or `None` when there is no such tag.
///
/// Deliberately absent from [`FOREIGN_TAGS`]: every name on that list is a vocabulary this
/// loop cannot execute and must therefore treat as a slip. This one is not a vocabulary at
/// all — it is the request to *stop* being the desktop, which is the one thing a desktop
/// turn can always honour.
#[must_use]
pub fn engine_request_reason(raw: &str) -> Option<String> {
    let start = raw.find(ENGINE_REQUEST_OPEN)? + ENGINE_REQUEST_OPEN.len();
    let end = raw[start..].find(ENGINE_REQUEST_CLOSE)? + start;
    let body = raw[start..end].trim();
    let reason = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|reason| !reason.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| {
            let bare = body
                .trim_matches(|c: char| c == '{' || c == '}' || c == '"')
                .trim();
            (!bare.is_empty() && !bare.starts_with("reason")).then(|| bare.to_owned())
        })
        .unwrap_or_else(|| "the fix is in the project, not on the screen".to_owned());
    Some(reason)
}

/// The reply with the request tag taken out, so what is left is the observation.
#[must_use]
pub fn strip_engine_request(raw: &str) -> String {
    let mut clean = String::new();
    let mut cursor = 0;
    while let Some(found) = raw[cursor..].find(ENGINE_REQUEST_OPEN) {
        let open_at = cursor + found;
        clean.push_str(&raw[cursor..open_at]);
        let body_at = open_at + ENGINE_REQUEST_OPEN.len();
        let Some(close) = raw[body_at..].find(ENGINE_REQUEST_CLOSE) else {
            return clean;
        };
        cursor = body_at + close + ENGINE_REQUEST_CLOSE.len();
    }
    clean.push_str(&raw[cursor..]);
    strip_protocol_tags(&clean)
}

/// Tags belonging to Bhippi's other protocols, none of which this loop can execute.
///
/// Kept as bare names so both `<engine_query>` and a stray `</engine_query>` are caught by
/// the same list. It is deliberately a list of *known* vocabularies rather than "any
/// angle-bracketed word": a model describing `<Control>` or `<Esc>` in prose is finishing,
/// not slipping.
const FOREIGN_TAGS: &[&str] = &[
    "engine_query",
    "engine_batch",
    "engine_action",
    "blender_script",
    "read_file",
    "write_file",
    "ask_user",
    "asset_import",
    "asset_register",
    "sketchfab_find",
    "sketchfab_import",
    "create_game",
    "spawn_agent",
    "agent_task",
    "agent_status",
    "design_query",
    "design_lesson",
    "computer_request",
];

/// The first other-protocol tag in a reply, if there is one.
#[must_use]
pub fn foreign_protocol_tag(text: &str) -> Option<&'static str> {
    FOREIGN_TAGS.iter().copied().find(|tag| {
        text.match_indices(&format!("<{tag}"))
            .any(|(index, prefix)| {
                text[index + prefix.len()..]
                    .starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/')
            })
            || text.contains(&format!("</{tag}>"))
    })
}

/// Everything outside every protocol tag this app defines, action tags included.
///
/// The last round of a capped turn has no vocabulary left, so whatever comes back is
/// rendered to the user verbatim. Without this a tag the model still reached for was
/// printed into the transcript as prose — which is exactly how `{"kind":"scenes"}` ended up
/// on screen underneath a completed run.
#[must_use]
pub fn strip_protocol_tags(text: &str) -> String {
    let mut clean = strip_action_tags(text);
    for tag in FOREIGN_TAGS {
        clean = strip_tag(&clean, tag);
    }
    clean
}

/// Remove every `<tag>…</tag>` block, and any orphaned opener, for one tag name.
fn strip_tag(text: &str, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut clean = String::new();
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find(&open) {
        let absolute = cursor + start;
        let after_name = absolute + open.len();
        if !text[after_name..].starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/') {
            clean.push_str(&text[cursor..after_name]);
            cursor = after_name;
            continue;
        }
        clean.push_str(&text[cursor..absolute]);
        let Some(header_end) = text[after_name..].find('>').map(|end| after_name + end) else {
            return clean;
        };
        let body_start = header_end + 1;
        if text[after_name..header_end].trim_end().ends_with('/') {
            cursor = body_start;
            continue;
        }
        match text[body_start..].find(&close) {
            Some(end) => cursor = body_start + end + close.len(),
            // An unterminated tag swallows the rest: what follows it is the tag's body,
            // not a sentence for the user.
            None => return clean,
        }
    }
    clean.push_str(&text[cursor..]);
    clean.replace(&close, "")
}

fn verdict_for(proposed: ProposedAction, narration: String, scope: ComputerScope) -> ReplyVerdict {
    if proposed.action.allowed_in(scope) {
        ReplyVerdict::Act {
            proposed: Box::new(proposed),
            narration,
        }
    } else {
        ReplyVerdict::Repair(RepairKind::OutOfScope {
            action: action_verb(&proposed.action).to_owned(),
        })
    }
}

/// The wire name of an action, for an error the model can act on.
#[must_use]
pub fn action_verb(action: &ComputerAction) -> &'static str {
    match action {
        ComputerAction::Screenshot => "screenshot",
        ComputerAction::MouseMove { .. } => "mouse_move",
        ComputerAction::MouseClick { .. } => "mouse_click",
        ComputerAction::MouseDrag { .. } => "mouse_drag",
        ComputerAction::MousePath { .. } => "mouse_path",
        ComputerAction::MouseScroll { .. } => "mouse_scroll",
        ComputerAction::TypeText { .. } => "type_text",
        ComputerAction::KeyPress { .. } => "key_press",
        ComputerAction::Hotkey { .. } => "hotkey",
        ComputerAction::GetScreenSize => "get_screen_size",
        ComputerAction::GetCursorPosition => "get_cursor_position",
        ComputerAction::OpenApp { .. } => "open_app",
        ComputerAction::OpenUrl { .. } => "open_url",
        ComputerAction::FocusWindow { .. } => "focus_window",
        ComputerAction::ListWindows => "list_windows",
        ComputerAction::Wait { .. } => "wait",
    }
}

fn tagged_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find(ACTION_OPEN) {
        let body_start = cursor + start + ACTION_OPEN.len();
        let Some(end) = text[body_start..].find(ACTION_CLOSE) else {
            // An unterminated tag is a slip, not a finish: keep what is there so the repair
            // can quote it back.
            blocks.push(text[body_start..].trim().to_owned());
            break;
        };
        blocks.push(text[body_start..body_start + end].trim().to_owned());
        cursor = body_start + end + ACTION_CLOSE.len();
    }
    blocks
}

/// Everything outside the action tags.
#[must_use]
pub fn strip_action_tags(text: &str) -> String {
    let mut clean = String::new();
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find(ACTION_OPEN) {
        let absolute = cursor + start;
        clean.push_str(&text[cursor..absolute]);
        let body_start = absolute + ACTION_OPEN.len();
        let Some(end) = text[body_start..].find(ACTION_CLOSE) else {
            return clean;
        };
        cursor = body_start + end + ACTION_CLOSE.len();
    }
    clean.push_str(&text[cursor..]);
    clean
}

/// A ```` ```computer_action ```` or ```` ```json ```` fence whose body mentions an action.
fn fenced_action_block(text: &str) -> Option<String> {
    for marker in ["```computer_action", "```json", "```"] {
        let mut cursor = 0;
        while let Some(start) = text[cursor..].find(marker) {
            let body_start = cursor + start + marker.len();
            let Some(end) = text[body_start..].find("```") else {
                break;
            };
            let body = text[body_start..body_start + end].trim();
            if body.contains("\"action\"") || body.contains("action:") {
                return Some(body.to_owned());
            }
            cursor = body_start + end + 3;
        }
    }
    None
}

/// Action-shaped JSON loose in prose.
fn looks_like_an_action(text: &str) -> bool {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("{\"action\":") || compact.contains("{action:")
}

fn clip(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    match trimmed.char_indices().nth(limit) {
        Some((cut, _)) => format!("{}…", &trimmed[..cut]),
        None => trimmed.to_owned(),
    }
}

// ------------------------------------------------------------------------- the gate

/// What the loop must do before an action runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GateOutcome {
    /// Run it.
    Allow,
    /// Ask the user first, with this card text. On yes the action runs; on no the loop tells
    /// the model and carries on observing.
    Ask { action: String, detail: String },
}

/// Tracks what the user has already answered inside one turn (ADR-0048 §3).
///
/// Two rules it exists to keep. Input permission granted mid-turn lasts *for that turn only*
/// — Full PC Access is a setting the user owns in Settings, and a turn may borrow a yes but
/// never write one. And a consequential target is confirmed once: "open Chrome" is a
/// question, not a drumbeat.
#[derive(Clone, Debug)]
pub struct GateLedger {
    full_access: bool,
    input_granted: bool,
    confirmed: BTreeSet<String>,
    scope: ComputerScope,
}

impl GateLedger {
    #[must_use]
    pub fn new(scope: ComputerScope, full_access: bool) -> Self {
        Self {
            full_access,
            input_granted: false,
            confirmed: BTreeSet::new(),
            scope,
        }
    }

    /// True when input may be sent without asking again.
    #[must_use]
    pub const fn may_send_input(&self) -> bool {
        self.full_access || self.input_granted
    }

    /// What to do about this action, before it runs.
    #[must_use]
    pub fn check(&self, action: &ComputerAction, reason: Option<&str>) -> GateOutcome {
        match action.class() {
            ComputerActionClass::Observe => GateOutcome::Allow,
            ComputerActionClass::Input => {
                if self.may_send_input() {
                    GateOutcome::Allow
                } else {
                    GateOutcome::Ask {
                        action: "Send keyboard and mouse input".to_owned(),
                        detail: with_reason(
                            "Full PC Access is off. Allow input for this turn only?",
                            reason,
                        ),
                    }
                }
            }
            ComputerActionClass::Consequential => {
                let target = action.consequential_target().unwrap_or_default();
                if self.confirmed.contains(&target) {
                    GateOutcome::Allow
                } else {
                    GateOutcome::Ask {
                        action: consequential_label(action),
                        detail: with_reason("This reaches outside the current window.", reason),
                    }
                }
            }
        }
    }

    /// Record a yes.
    pub fn granted(&mut self, action: &ComputerAction) {
        match action.class() {
            ComputerActionClass::Input => self.input_granted = true,
            ComputerActionClass::Consequential => {
                if let Some(target) = action.consequential_target() {
                    self.confirmed.insert(target);
                }
            }
            ComputerActionClass::Observe => {}
        }
    }

    /// What the model is told after a no.
    ///
    /// A denial is information, not an error: the model is told which door closed and what
    /// it may still do, so it can finish with what it can see rather than stopping dead.
    #[must_use]
    pub fn denial_note(&self, action: &ComputerAction) -> String {
        match action.class() {
            ComputerActionClass::Input => format!(
                "The user declined `{}`, and input stays off for this turn. You can still \
                 look — take a screenshot, read the bounds, list windows — and then say what \
                 you observed and what remains to be done.",
                action_verb(action)
            ),
            _ => format!(
                "The user declined `{}`. Do not try it again this turn. Continue with what is \
                 already on screen, or say what remains to be done.",
                action_verb(action)
            ),
        }
    }

    #[must_use]
    pub const fn scope(&self) -> ComputerScope {
        self.scope
    }
}

fn with_reason(base: &str, reason: Option<&str>) -> String {
    match reason.map(str::trim).filter(|value| !value.is_empty()) {
        Some(reason) => format!("{base} Reason given: {reason}"),
        None => base.to_owned(),
    }
}

fn consequential_label(action: &ComputerAction) -> String {
    match action {
        ComputerAction::OpenApp { target } => format!("Open {target}"),
        ComputerAction::OpenUrl { url } => format!("Open {url}"),
        ComputerAction::FocusWindow { title } => format!("Switch to \"{title}\""),
        ComputerAction::Hotkey { keys } => format!("Press {}", keys.join("+")),
        _ => "Run this action".to_owned(),
    }
}

// -------------------------------------------------------------------- what it is told

/// The surface the model is looking at, named for it each round.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Surface {
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
}

impl Surface {
    #[must_use]
    pub const fn of(capture: &ScreenCapture) -> Self {
        Self {
            origin_x: capture.origin_x,
            origin_y: capture.origin_y,
            width: capture.width,
            height: capture.height,
        }
    }
}

/// The rolling record of a turn: what the model has done and what it is told next.
///
/// Compaction is the reason this is a type rather than a `Vec<String>`. Only the last
/// [`COMPUTER_VERBATIM_ROUNDS`] observations are carried in full; everything older becomes
/// one line. A 24-action run used to re-send the same protocol paragraph 24 times.
#[derive(Clone, Debug, Default)]
pub struct History {
    entries: Vec<String>,
}

impl History {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn record(&mut self, index: usize, title: &str, reason: Option<&str>, result: &str) {
        let reason = reason
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!(" — {value}"))
            .unwrap_or_default();
        self.entries
            .push(format!("{index}. {title}{reason} → {result}"));
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The compacted log of everything before the verbatim window.
    #[must_use]
    pub fn older_than_window(&self) -> Option<String> {
        let keep = COMPUTER_VERBATIM_ROUNDS;
        if self.entries.len() <= keep {
            return None;
        }
        let older = &self.entries[..self.entries.len() - keep];
        Some(format!("Earlier in this turn:\n{}", older.join("\n")))
    }

    /// Everything, for the final report.
    #[must_use]
    pub fn all(&self) -> &[String] {
        &self.entries
    }
}

/// How the frame reaches the model this turn.
///
/// Only Codex takes a screenshot as a real attachment (`--image`). Claude, Grok and
/// Antigravity have no image flag at all: the adapter unlocks the capture's directory and
/// leaves a read tool enabled, and the picture arrives *only if the model opens the file*.
/// Naming the path and hoping was not enough — a model that never opened it answered from
/// the text alone and went looking for something else to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Frame {
    /// The image rides on the request; the model already has it.
    Attached,
    /// The image is a file the model must open before it can see anything.
    OnDisk,
}

/// The block sent with each fresh screenshot.
///
/// Every argument is a fact the model needs and none of them belongs together in a struct
/// that would exist only to shorten this signature.
#[allow(clippy::too_many_arguments)]
///
/// Small and fixed on purpose: the protocol is stated once in the system prompt, so what
/// changes each round is only the situation. `focused` is this loop's analogue of the URL a
/// browsing agent is told — the one piece of state a picture does not reliably carry.
#[must_use]
pub fn observation(
    scope: ComputerScope,
    surface: Surface,
    path: &std::path::Path,
    frame: Frame,
    result: &str,
    cursor: Option<(i32, i32)>,
    focused: Option<&str>,
    actions_used: usize,
    history: &History,
) -> String {
    let mut block = String::new();
    if let Some(older) = history.older_than_window() {
        block.push_str(&older);
        block.push_str("\n\n");
    }
    block.push_str(result.trim());
    block.push('\n');

    let surface_name = match scope {
        ComputerScope::Desktop => "Virtual desktop",
        ComputerScope::GameWindow => "Game window",
    };
    block.push_str(&format!(
        "Screenshot: {}\n{surface_name} origin: ({}, {})\n{surface_name} size: {}x{}\n",
        path.display(),
        surface.origin_x,
        surface.origin_y,
        surface.width,
        surface.height,
    ));
    if frame == Frame::OnDisk {
        block.push_str(
            "That file is the only view you have of the screen. Open it with your file-reading \
             tool now, before you decide anything — you cannot answer this from the text.\n",
        );
    }
    if let Some((x, y)) = cursor {
        block.push_str(&format!("Pointer: ({x}, {y})\n"));
    }
    if let Some(title) = focused.map(str::trim).filter(|value| !value.is_empty()) {
        block.push_str(&format!("Focused window: {title}\n"));
    }
    let remaining = COMPUTER_MAX_ACTIONS_PER_TURN.saturating_sub(actions_used);
    block.push_str(&format!(
        "Actions used: {actions_used} of {COMPUTER_MAX_ACTIONS_PER_TURN} ({remaining} left)\n\
         Choose one action against this exact image, or reply in plain English with no JSON \
         if the task is done."
    ));
    block
}

/// The last round: the vocabulary is withdrawn and only a summary is wanted (ADR-0048 §7).
///
/// This is what makes a cap useful instead of merely safe. The alternative — returning
/// "stopped after 24 actions" — throws away everything the run learned at the exact moment
/// the user most needs it.
#[must_use]
pub fn final_summary_request(history: &History) -> String {
    format!(
        "The action budget for this turn is spent, so no further action will be executed — \
         do not return an action block.\n\nWhat was done:\n{}\n\nIn plain English: what is \
         verified true on screen now, what is left undone, and what a next turn should do \
         first.",
        if history.is_empty() {
            "nothing".to_owned()
        } else {
            history.all().join("\n")
        }
    )
}

// ------------------------------------------------------------------------- settling

/// A cheap fingerprint of a frame, for deciding whether the screen has stopped moving.
///
/// Hashing the encoded bytes is enough: two captures of a still screen encode identically,
/// and any real change moves the hash. It costs nothing next to the capture itself.
#[must_use]
pub fn frame_hash(capture: &ScreenCapture) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in capture.image_base64.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^= u64::from(capture.width) << 32 | u64::from(capture.height);
    hash
}

// --------------------------------------------------------------------------- ending

/// How a Computer Use turn is described when it ends.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnReport {
    pub outcome: ComputerOutcome,
    /// The model's own words, when it produced any.
    pub summary: String,
    /// One line per executed action.
    pub actions: Vec<String>,
    /// Frames kept as evidence, project-absolute.
    pub evidence: Vec<String>,
}

impl TurnReport {
    #[must_use]
    pub fn new(outcome: ComputerOutcome, summary: impl Into<String>, history: &History) -> Self {
        Self {
            outcome,
            summary: summary.into(),
            actions: history.all().to_vec(),
            evidence: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_evidence(mut self, frames: Vec<String>) -> Self {
        self.evidence = frames;
        self
    }

    /// What the user reads.
    ///
    /// The heading is decided by the outcome, never by the model's confidence: ADR-0044 §4
    /// requires that a cap "never a success claim", and the same holds for a decline and a
    /// lost protocol. A model that writes "Done!" after being cut off does not get to
    /// narrate the turn's result.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        let summary = self.summary.trim();

        match self.outcome {
            ComputerOutcome::Completed => {
                if summary.is_empty() {
                    out.push_str("Computer Use finished.");
                } else {
                    out.push_str(summary);
                }
            }
            ComputerOutcome::Declined => {
                out.push_str("Stopped: you declined that step.");
                if !summary.is_empty() {
                    out.push_str("\n\n");
                    out.push_str(summary);
                }
            }
            ComputerOutcome::CapReached => {
                out.push_str(&format!(
                    "Stopped at the {COMPUTER_MAX_ACTIONS_PER_TURN}-action limit for one turn — \
                     the task is not finished."
                ));
                if !summary.is_empty() {
                    out.push_str("\n\n");
                    out.push_str(summary);
                }
            }
            ComputerOutcome::ProtocolLost => {
                out.push_str(
                    "Stopped: the model could not produce a usable action after several \
                     attempts, so nothing further was done.",
                );
            }
            ComputerOutcome::Stopped => out.push_str("Stopped."),
            ComputerOutcome::Failed => {
                out.push_str("Computer Use failed.");
                if !summary.is_empty() {
                    out.push_str("\n\n");
                    out.push_str(summary);
                }
            }
        }

        if !self.actions.is_empty() {
            out.push_str("\n\nWhat it did:\n");
            for line in &self.actions {
                out.push_str("- ");
                out.push_str(line);
                out.push('\n');
            }
        }
        out.trim_end().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhippi_types::COMPUTER_MAX_REPAIRS;

    fn capture(base64: &str) -> ScreenCapture {
        ScreenCapture {
            origin_x: 0,
            origin_y: 0,
            width: 1920,
            height: 1080,
            image_base64: base64.to_owned(),
            captured_at: chrono::Utc::now(),
        }
    }

    fn act(raw: &str) -> ProposedAction {
        match interpret_reply(raw, ComputerScope::Desktop) {
            ReplyVerdict::Act { proposed, .. } => *proposed,
            other => panic!("expected an action, got {other:?}"),
        }
    }

    // ── reading a reply ───────────────────────────────────────────────────────────

    #[test]
    fn a_well_formed_action_is_executed_with_its_reason() {
        let proposed = act(r#"<computer_action>
{"action":"mouse_click","button":"left","count":1,"x":10,"y":20,"reason":"open the File menu"}
</computer_action>"#);
        assert_eq!(action_verb(&proposed.action), "mouse_click");
        assert_eq!(proposed.reason.as_deref(), Some("open the File menu"));
    }

    #[test]
    fn a_reason_is_optional_so_an_older_model_still_runs() {
        let proposed = act(r#"<computer_action>{"action":"screenshot"}</computer_action>"#);
        assert_eq!(action_verb(&proposed.action), "screenshot");
        assert!(proposed.reason.is_none());
    }

    /// The fault this whole module exists for: a mistyped verb used to be indistinguishable
    /// from "the task is done", so a run ended with a confident summary and nothing done.
    #[test]
    fn a_mistyped_verb_is_a_repair_not_a_silent_completion() {
        let verdict = interpret_reply(
            r#"<computer_action>{"action":"click","x":10,"y":20}</computer_action>"#,
            ComputerScope::Desktop,
        );
        match verdict {
            ReplyVerdict::Repair(RepairKind::Unparseable { offending }) => {
                assert!(offending.contains("click"), "the bad text is quoted back");
            }
            other => panic!("a bad verb must be repaired, got {other:?}"),
        }
    }

    #[test]
    fn two_actions_ask_for_one_rather_than_failing_the_turn() {
        let verdict = interpret_reply(
            r#"<computer_action>{"action":"screenshot"}</computer_action>
<computer_action>{"action":"screenshot"}</computer_action>"#,
            ComputerScope::Desktop,
        );
        assert_eq!(
            verdict,
            ReplyVerdict::Repair(RepairKind::TooMany { count: 2 })
        );
        let message = RepairKind::TooMany { count: 2 }.message(ComputerScope::Desktop);
        assert!(
            message.contains("as it is now"),
            "the correction explains why one at a time: {message}"
        );
    }

    #[test]
    fn an_unterminated_tag_is_a_repair_not_a_completion() {
        let verdict = interpret_reply(
            r#"<computer_action>{"action":"screenshot"}"#,
            ComputerScope::Desktop,
        );
        // The body parses, so it runs; what must not happen is it reading as "done".
        assert!(matches!(verdict, ReplyVerdict::Act { .. }));
    }

    #[test]
    fn a_bare_object_in_prose_is_a_repair() {
        let verdict = interpret_reply(
            r#"I will now click the button: {"action":"mouse_click","x":5,"y":5}"#,
            ComputerScope::Desktop,
        );
        assert_eq!(verdict, ReplyVerdict::Repair(RepairKind::Unfenced));
    }

    /// The regression this fix exists for, reproduced from the run that showed it.
    ///
    /// The turn was given the Godot engine protocol alongside the desktop one, so asked to
    /// make a joystick work the model answered with `<engine_query>` and no action block.
    /// That used to read as *finished*: the panel said "Done · 1 of 24 steps", nothing had
    /// been done, and the query JSON was printed to the user as though it were the answer.
    #[test]
    fn an_engine_query_instead_of_an_action_is_a_repair_not_a_completion() {
        let raw = "Let me look at the current joystick implementation before changing it.\n\
                   <engine_query>{\"kind\":\"scenes\"}</engine_query>";
        match interpret_reply(raw, ComputerScope::Desktop) {
            ReplyVerdict::Repair(RepairKind::WrongProtocol { tag }) => {
                assert_eq!(tag, "engine_query");
                let message = RepairKind::WrongProtocol { tag }.message(ComputerScope::Desktop);
                assert!(
                    message.contains("does not exist in this turn"),
                    "the correction says the vocabulary is absent: {message}"
                );
                assert!(
                    message.contains(ACTION_OPEN),
                    "and shows the shape that is wanted instead: {message}"
                );
            }
            other => panic!("an engine query must never finish a desktop turn, got {other:?}"),
        }
    }

    #[test]
    fn every_other_protocol_bhippi_speaks_is_caught_the_same_way() {
        for tag in [
            "engine_batch",
            "blender_script",
            "ask_user",
            "asset_import",
            "sketchfab_find",
            "create_game",
            "spawn_agent",
        ] {
            let raw = format!("<{tag}>{{}}</{tag}>");
            assert_eq!(
                interpret_reply(&raw, ComputerScope::Desktop),
                ReplyVerdict::Repair(RepairKind::WrongProtocol {
                    tag: tag.to_owned()
                }),
                "`<{tag}>` must be corrected, not read as a finish"
            );
        }
    }

    #[test]
    fn blender_python_in_a_desktop_reply_is_never_mistaken_for_completed_work() {
        let raw = "Creating the prop.\n<blender_script>import bpy\nbpy.ops.mesh.primitive_cube_add()</blender_script>";
        assert_eq!(
            interpret_reply(raw, ComputerScope::Desktop),
            ReplyVerdict::Repair(RepairKind::WrongProtocol {
                tag: "blender_script".to_owned()
            }),
        );
        assert_eq!(super::strip_protocol_tags(raw).trim(), "Creating the prop.");
        let repair = RepairKind::WrongProtocol {
            tag: "blender_script".to_owned(),
        };
        assert!(repair
            .message(ComputerScope::Desktop)
            .contains("<engine_request>"));
        assert!(!repair
            .message(ComputerScope::GameWindow)
            .contains("<engine_request>"));
    }

    #[test]
    fn workspace_file_tags_are_repaired_in_desktop_mode() {
        for raw in [
            "<read_file path='notes.txt' />",
            "<write_file path='notes.txt'>hello</write_file>",
        ] {
            assert!(matches!(
                interpret_reply(raw, ComputerScope::Desktop),
                ReplyVerdict::Repair(RepairKind::WrongProtocol { .. })
            ));
            assert!(super::strip_protocol_tags(raw).is_empty());
        }
        assert_eq!(
            super::strip_protocol_tags("<read_files>prose</read_files>"),
            "<read_files>prose</read_files>"
        );
    }

    /// A word in angle brackets is not a protocol. A model saying it pressed `<Esc>` and
    /// finished is finishing, and must not be dragged into a repair round for it.
    #[test]
    fn prose_that_merely_contains_angle_brackets_still_finishes() {
        let verdict = interpret_reply(
            "I pressed <Esc> and the dialog closed; the title bar reads Untitled.",
            ComputerScope::Desktop,
        );
        assert!(matches!(verdict, ReplyVerdict::Complete { .. }));
    }

    #[test]
    fn an_empty_reply_is_a_repair_rather_than_a_silent_success() {
        assert_eq!(
            interpret_reply("   \n  ", ComputerScope::Desktop),
            ReplyVerdict::Repair(RepairKind::Empty)
        );
    }

    /// Belt to the braces on the leak the user actually saw: whatever reaches the summary,
    /// no tag body is ever printed to them as prose.
    #[test]
    fn a_completion_summary_never_carries_a_protocol_tag() {
        let cleaned = strip_protocol_tags(
            "Saved.\n<engine_batch>{\"label\":\"x\"}</engine_batch>\nThe title bar is clean.",
        );
        assert!(!cleaned.contains("engine_batch"), "{cleaned}");
        assert!(!cleaned.contains("label"), "{cleaned}");
        assert!(cleaned.contains("Saved."));
        assert!(cleaned.contains("The title bar is clean."));
    }

    // ── the frame the model is actually given ─────────────────────────────────────

    /// Only Codex takes an attached image. For every other authorised backend the picture
    /// is a file, and a file nobody is told to open is a screenshot nobody looked at.
    #[test]
    fn a_path_delivered_frame_is_told_to_be_opened_and_an_attached_one_is_not() {
        let path = std::path::Path::new("C:/temp/bhippi-computer-use/turn.jpg");
        let surface = Surface {
            origin_x: 0,
            origin_y: 0,
            width: 1920,
            height: 1080,
        };
        let on_disk = observation(
            ComputerScope::Desktop,
            surface,
            path,
            Frame::OnDisk,
            "Initial desktop observation.",
            None,
            None,
            0,
            &History::new(),
        );
        assert!(on_disk.contains("turn.jpg"));
        assert!(
            on_disk.contains("Open it with your file-reading tool"),
            "a path-delivered frame says to open it: {on_disk}"
        );

        let attached = observation(
            ComputerScope::Desktop,
            surface,
            path,
            Frame::Attached,
            "Initial desktop observation.",
            None,
            None,
            0,
            &History::new(),
        );
        assert!(
            !attached.contains("Open it with your file-reading tool"),
            "an attached frame is already in hand: {attached}"
        );
    }

    /// Narration is the caption shown beside the frame while the action runs, so a tag that
    /// rode along with a *valid* action must not reach the user as prose either.
    #[test]
    fn narration_beside_an_action_is_scrubbed_of_other_protocols() {
        let verdict = interpret_reply(
            "Clicking the joystick.\n<engine_query>{\"kind\":\"scenes\"}</engine_query>\n\
             <computer_action>{\"action\":\"screenshot\"}</computer_action>",
            ComputerScope::Desktop,
        );
        match verdict {
            ReplyVerdict::Act { narration, .. } => {
                assert!(narration.contains("Clicking the joystick."));
                assert!(!narration.contains("engine_query"), "{narration}");
                assert!(!narration.contains("kind"), "{narration}");
            }
            other => panic!("the action still runs, got {other:?}"),
        }
    }

    #[test]
    fn a_fenced_block_is_tolerated_because_some_clis_strip_tags() {
        let proposed = act("```json\n{\"action\":\"screenshot\"}\n```");
        assert_eq!(action_verb(&proposed.action), "screenshot");
    }

    #[test]
    fn plain_english_with_no_json_is_a_completion() {
        let verdict = interpret_reply(
            "The file is saved: the title bar no longer shows an asterisk.",
            ComputerScope::Desktop,
        );
        match verdict {
            ReplyVerdict::Complete { summary } => assert!(summary.contains("asterisk")),
            other => panic!("expected completion, got {other:?}"),
        }
    }

    #[test]
    fn narration_beside_an_action_is_kept_rather_than_refused() {
        let verdict = interpret_reply(
            "Opening the menu first.\n<computer_action>{\"action\":\"screenshot\"}</computer_action>",
            ComputerScope::Desktop,
        );
        match verdict {
            ReplyVerdict::Act { narration, .. } => {
                assert_eq!(narration, "Opening the menu first.");
            }
            other => panic!("expected an action, got {other:?}"),
        }
    }

    // ── scope ─────────────────────────────────────────────────────────────────────

    /// INV-089's last row, as a property of the vocabulary rather than a runtime branch.
    #[test]
    fn the_game_scope_has_no_verb_that_reaches_the_desktop() {
        for raw in [
            r#"<computer_action>{"action":"open_app","target":"notepad"}</computer_action>"#,
            r#"<computer_action>{"action":"open_url","url":"https://example.com"}</computer_action>"#,
            r#"<computer_action>{"action":"focus_window","title":"Chrome"}</computer_action>"#,
        ] {
            match interpret_reply(raw, ComputerScope::GameWindow) {
                ReplyVerdict::Repair(RepairKind::OutOfScope { action }) => {
                    let message =
                        RepairKind::OutOfScope { action }.message(ComputerScope::GameWindow);
                    assert!(
                        message.contains("game window"),
                        "the refusal says why: {message}"
                    );
                }
                other => panic!("{raw} must not be legal in the game scope, got {other:?}"),
            }
        }
    }

    #[test]
    fn the_game_scope_still_has_the_verbs_it_needs_to_play() {
        for raw in [
            r#"<computer_action>{"action":"key_press","key":"space"}</computer_action>"#,
            r#"<computer_action>{"action":"screenshot"}</computer_action>"#,
            r#"<computer_action>{"action":"mouse_click","button":"left","count":1,"x":5,"y":5}</computer_action>"#,
        ] {
            assert!(
                matches!(
                    interpret_reply(raw, ComputerScope::GameWindow),
                    ReplyVerdict::Act { .. }
                ),
                "{raw} must be playable in the game scope"
            );
        }
    }

    // ── the gate ──────────────────────────────────────────────────────────────────

    #[test]
    fn looking_is_never_gated() {
        let ledger = GateLedger::new(ComputerScope::Desktop, false);
        for action in [
            ComputerAction::Screenshot,
            ComputerAction::GetScreenSize,
            ComputerAction::ListWindows,
            ComputerAction::Wait { ms: 100 },
        ] {
            assert_eq!(ledger.check(&action, None), GateOutcome::Allow);
        }
    }

    /// The old behaviour ended the turn here with a message. The user is sitting in front of
    /// the machine; the honest move is to ask them.
    #[test]
    fn input_without_full_access_asks_instead_of_failing() {
        let mut ledger = GateLedger::new(ComputerScope::Desktop, false);
        let click = ComputerAction::MouseClick {
            button: "left".to_owned(),
            count: 1,
            x: Some(5),
            y: Some(5),
        };
        match ledger.check(&click, Some("press Save")) {
            GateOutcome::Ask { detail, .. } => {
                assert!(detail.contains("this turn only"), "{detail}");
                assert!(
                    detail.contains("press Save"),
                    "the reason is shown: {detail}"
                );
            }
            GateOutcome::Allow => panic!("input must be gated with full access off"),
        }
        ledger.granted(&click);
        assert_eq!(ledger.check(&click, None), GateOutcome::Allow);
        assert!(ledger.may_send_input(), "the yes lasts for this turn");
    }

    #[test]
    fn a_consequential_action_is_confirmed_once_per_target() {
        let mut ledger = GateLedger::new(ComputerScope::Desktop, true);
        let chrome = ComputerAction::OpenApp {
            target: "Chrome".to_owned(),
        };
        assert!(matches!(
            ledger.check(&chrome, None),
            GateOutcome::Ask { .. }
        ));
        ledger.granted(&chrome);
        assert_eq!(ledger.check(&chrome, None), GateOutcome::Allow);
        // Same target, different casing and whitespace: still the same question.
        assert_eq!(
            ledger.check(
                &ComputerAction::OpenApp {
                    target: " chrome ".to_owned()
                },
                None
            ),
            GateOutcome::Allow
        );
        // A different target is a new question.
        assert!(matches!(
            ledger.check(
                &ComputerAction::OpenApp {
                    target: "notepad".to_owned()
                },
                None
            ),
            GateOutcome::Ask { .. }
        ));
    }

    #[test]
    fn a_closing_chord_is_consequential_but_copy_is_not() {
        let ledger = GateLedger::new(ComputerScope::Desktop, true);
        let close = ComputerAction::Hotkey {
            keys: vec!["alt".to_owned(), "f4".to_owned()],
        };
        assert_eq!(close.class(), ComputerActionClass::Consequential);
        assert!(matches!(
            ledger.check(&close, None),
            GateOutcome::Ask { .. }
        ));

        let copy = ComputerAction::Hotkey {
            keys: vec!["ctrl".to_owned(), "c".to_owned()],
        };
        assert_eq!(copy.class(), ComputerActionClass::Input);
        assert_eq!(ledger.check(&copy, None), GateOutcome::Allow);
    }

    #[test]
    fn a_denial_tells_the_model_what_it_may_still_do() {
        let ledger = GateLedger::new(ComputerScope::Desktop, false);
        let click = ComputerAction::MouseClick {
            button: "left".to_owned(),
            count: 1,
            x: Some(1),
            y: Some(1),
        };
        let note = ledger.denial_note(&click);
        assert!(note.contains("still look"), "{note}");
        assert!(note.contains("screenshot"), "{note}");
    }

    // ── history and observation ───────────────────────────────────────────────────

    #[test]
    fn history_compacts_everything_outside_the_verbatim_window() {
        let mut history = History::new();
        for index in 1..=6 {
            history.record(index, &format!("Action {index}"), Some("because"), "ok");
        }
        let older = history
            .older_than_window()
            .expect("six rounds is past the window");
        assert!(older.contains("1. Action 1"), "{older}");
        assert!(
            !older.contains(&format!("{}. Action {}", 6, 6)),
            "the newest rounds stay verbatim elsewhere: {older}"
        );
        assert_eq!(
            older.lines().count(),
            1 + (6 - COMPUTER_VERBATIM_ROUNDS),
            "one heading plus one line per compacted action"
        );
    }

    #[test]
    fn a_short_run_has_nothing_to_compact() {
        let mut history = History::new();
        history.record(1, "Screenshot", None, "ok");
        assert!(history.older_than_window().is_none());
    }

    #[test]
    fn the_observation_names_the_surface_the_budget_and_the_focus() {
        let history = History::new();
        let block = observation(
            ComputerScope::Desktop,
            Surface::of(&capture("x")),
            std::path::Path::new("/tmp/frame.png"),
            Frame::Attached,
            "Action result: clicked",
            Some((100, 200)),
            Some("Untitled - Notepad"),
            3,
            &history,
        );
        assert!(block.contains("Virtual desktop size: 1920x1080"), "{block}");
        assert!(block.contains("Pointer: (100, 200)"), "{block}");
        assert!(
            block.contains("Focused window: Untitled - Notepad"),
            "{block}"
        );
        assert!(
            block.contains(&format!("3 of {COMPUTER_MAX_ACTIONS_PER_TURN}")),
            "the budget is visible: {block}"
        );
    }

    #[test]
    fn the_game_scope_observation_never_calls_the_surface_a_desktop() {
        let block = observation(
            ComputerScope::GameWindow,
            Surface::of(&capture("x")),
            std::path::Path::new("/tmp/frame.png"),
            Frame::Attached,
            "Initial observation",
            None,
            None,
            0,
            &History::new(),
        );
        assert!(block.contains("Game window size"), "{block}");
        assert!(!block.contains("Virtual desktop"), "{block}");
    }

    // ── settling ──────────────────────────────────────────────────────────────────

    #[test]
    fn an_unchanged_frame_hashes_equal_and_a_changed_one_does_not() {
        assert_eq!(frame_hash(&capture("aaaa")), frame_hash(&capture("aaaa")));
        assert_ne!(frame_hash(&capture("aaaa")), frame_hash(&capture("aaab")));
    }

    // ── the ending ────────────────────────────────────────────────────────────────

    /// ADR-0044 §4: reaching the cap "never a success claim".
    #[test]
    fn a_capped_turn_states_the_limit_and_keeps_what_was_verified() {
        let mut history = History::new();
        history.record(1, "Press ctrl+s", Some("save the file"), "ok");
        let report = TurnReport::new(
            ComputerOutcome::CapReached,
            "The dialog is open and the filename field is focused.",
            &history,
        );
        let rendered = report.render();
        assert!(rendered.contains("not finished"), "{rendered}");
        assert!(
            rendered.contains(&COMPUTER_MAX_ACTIONS_PER_TURN.to_string()),
            "{rendered}"
        );
        assert!(rendered.contains("filename field"), "{rendered}");
        assert!(
            rendered.contains("Press ctrl+s — save the file"),
            "{rendered}"
        );
        assert!(!report.outcome.claims_success());
    }

    /// A model that writes "Done!" after being cut off does not get to narrate the result.
    #[test]
    fn a_confident_summary_cannot_relabel_a_capped_turn_as_success() {
        let report = TurnReport::new(
            ComputerOutcome::CapReached,
            "Done! Everything worked perfectly.",
            &History::new(),
        );
        assert!(report.render().starts_with("Stopped at the"));
    }

    #[test]
    fn a_declined_turn_reads_as_a_decision_not_a_failure() {
        let report = TurnReport::new(
            ComputerOutcome::Declined,
            "I could see the Save dialog but did not press anything.",
            &History::new(),
        );
        let rendered = report.render();
        assert!(rendered.starts_with("Stopped: you declined"), "{rendered}");
        assert!(!report.outcome.claims_success());
    }

    #[test]
    fn a_completed_turn_leads_with_the_models_own_words() {
        let report = TurnReport::new(
            ComputerOutcome::Completed,
            "Saved as notes.txt; the title bar shows the new name.",
            &History::new(),
        );
        assert!(report.render().starts_with("Saved as notes.txt"));
    }

    #[test]
    fn the_final_round_withdraws_the_vocabulary_and_asks_for_the_state() {
        let mut history = History::new();
        history.record(1, "Open Notepad", None, "ok");
        let request = final_summary_request(&history);
        assert!(
            request.contains("do not return an action block"),
            "{request}"
        );
        assert!(request.contains("1. Open Notepad"), "{request}");
        assert!(request.contains("verified true"), "{request}");
    }

    #[test]
    fn evidence_frames_ride_on_the_report() {
        let report = TurnReport::new(ComputerOutcome::Completed, "done", &History::new())
            .with_evidence(vec!["C:/frames/last.png".to_owned()]);
        assert_eq!(report.evidence.len(), 1);
    }

    #[test]
    fn the_repair_budget_is_small_enough_to_not_burn_a_turn() {
        const {
            assert!(COMPUTER_MAX_REPAIRS <= 4);
        }
    }

    // -- the prompt --------------------------------------------------------------

    const PROMPT: &str = PROTOCOL;

    /// A verb the loop accepts but the prompt never mentions is a verb no model will use;
    /// a verb the prompt offers but the loop cannot parse is a guaranteed repair round.
    #[test]
    fn the_prompt_documents_every_verb_the_parser_accepts() {
        let vocabulary = [
            ComputerAction::Screenshot,
            ComputerAction::GetScreenSize,
            ComputerAction::GetCursorPosition,
            ComputerAction::MouseMove { x: 0, y: 0 },
            ComputerAction::MouseClick {
                button: "left".to_owned(),
                count: 1,
                x: None,
                y: None,
            },
            ComputerAction::MouseDrag {
                start_x: 0,
                start_y: 0,
                end_x: 0,
                end_y: 0,
            },
            ComputerAction::MouseScroll {
                delta_x: 0,
                delta_y: 0,
            },
            ComputerAction::MousePath {
                points: vec![[0, 0], [1, 1]],
                button: "left".into(),
                duration_ms: 100,
            },
            ComputerAction::TypeText {
                text: String::new(),
            },
            ComputerAction::KeyPress {
                key: "enter".to_owned(),
            },
            ComputerAction::Hotkey { keys: Vec::new() },
            ComputerAction::OpenApp {
                target: String::new(),
            },
            ComputerAction::OpenUrl { url: String::new() },
            ComputerAction::FocusWindow {
                title: String::new(),
            },
            ComputerAction::ListWindows,
            ComputerAction::Wait { ms: 0 },
        ];
        for action in &vocabulary {
            let verb = action_verb(action);
            assert!(
                PROMPT.contains(&format!("\"action\":\"{verb}\"")),
                "the prompt never shows `{verb}`"
            );
        }
    }

    #[test]
    fn the_prompt_teaches_the_contract_the_loop_enforces() {
        for phrase in [
            // Finishing is "no JSON at all", which is what stops a slip reading as done.
            "no JSON at all",
            // A repair is recoverable, and the model must know that before it panics.
            "is **not** a failure",
            // The reason field, which the overlay and the report depend on.
            "\"reason\"",
            // The gate, so a permission card is expected rather than surprising.
            "Full PC Access",
            // The budget, which the observation reports every round.
            "final round with the action list withdrawn",
            // The narrower scope.
            "do not exist",
            "game window",
            // ADR-0059: another protocol's tag is a slip, and the loop now says so.
            "<engine_query>",
            "An empty reply is",
        ] {
            assert!(PROMPT.contains(phrase), "the prompt never says: {phrase}");
        }
    }

    #[test]
    fn the_prompt_version_moved_with_the_contract() {
        assert!(
            PROMPT.starts_with("version: 10"),
            "ADR-0048, ADR-0054 and ADR-0059 changed the contract; the version moves with it"
        );
    }

    #[test]
    fn a_desktop_turn_can_hand_the_work_back_to_the_project() {
        // The owner asked Bhippi to look at the screen AND fix what it saw. It looked,
        // diagnosed both problems correctly, and ended with "start a normal turn and I'll fix
        // both" — because that was the only move the protocol gave it (ADR-0063).
        for phrase in [
            "<engine_request>",
            "this same turn",
            "Never write \"start a normal turn and I will fix it\"",
        ] {
            assert!(PROMPT.contains(phrase), "the prompt never says: {phrase}");
        }
    }

    #[test]
    fn the_hand_back_tag_is_not_treated_as_a_foreign_protocol_slip() {
        // Every other outside tag is a slip, because this loop cannot execute it. This one is
        // not a vocabulary at all — it is the request to stop being the desktop.
        let reply = "The lighting is an Environment setting, not something on screen.
                     <engine_request>{\"reason\":\"fix the exposure in the project\"}</engine_request>";
        match super::interpret_reply(reply, bhippi_types::ComputerScope::Desktop) {
            super::ReplyVerdict::HandBack { reason, summary } => {
                assert!(reason.contains("exposure"), "{reason}");
                assert!(
                    summary.contains("Environment setting"),
                    "the observation survives for the project phase: {summary}"
                );
                assert!(
                    !summary.contains("engine_request"),
                    "the tag itself does not"
                );
            }
            other => panic!("a hand-back must not read as {other:?}"),
        }
    }

    #[test]
    fn an_engine_query_is_still_a_slip_even_though_the_hand_back_is_not() {
        // The distinction has to survive: asking to *leave* the desktop is legal, doing
        // engine work *while* on the desktop is the ADR-0059 fault and still a repair.
        let reply = "<engine_query>{\"kind\":\"scenes\"}</engine_query>";
        assert!(matches!(
            super::interpret_reply(reply, bhippi_types::ComputerScope::Desktop),
            super::ReplyVerdict::Repair(super::RepairKind::WrongProtocol { .. })
        ));
    }

    #[test]
    fn a_turn_asked_to_do_something_is_not_finished_by_looking_at_it() {
        // A run asked to open Blender and fix a menu took two actions, said the work really
        // belonged in the project, and stopped — which the doctrine had told it to do. The
        // rule stands for a *question* about the screen; it was never right for an
        // instruction, and both now appear here so they cannot be collapsed back into one.
        for phrase in [
            "A question about the screen",
            "An instruction to do something",
            "acting **is** the task",
            "Do not stop to ask permission to continue",
        ] {
            assert!(PROMPT.contains(phrase), "the prompt never says: {phrase}");
        }
    }
}
