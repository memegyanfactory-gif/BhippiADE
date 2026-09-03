//! The AI ↔ Godot bridge (GAD-086…089, GAD-092), replacing `engine/bridge.rs`.
//!
//! ADR-0043 §6 keeps the protocol and swaps the vocabulary: `<engine_query>`,
//! `<engine_action>` and `<engine_batch>` are still scanned out of the live delta stream,
//! still answered mid-turn, still repaired once against a typed result envelope — but the
//! bodies are now [`GodotAction`] / [`GodotActionBatch`] JSON, and the reads are questions
//! about a real Godot project rather than about a `.bscn.json`.
//!
//! Three properties are carried over verbatim from the module this replaces, because they
//! are what made it work:
//!
//! 1. **Tags split across deltas are reassembled.** Providers break text at arbitrary byte
//!    boundaries and an opening tag routinely straddles two chunks.
//! 2. **Protocol text never reaches the visible answer.** The user reads prose and watches
//!    Activity Dock cards.
//! 3. **A truncated call is dropped, never half-applied.**
//!
//! Everything a batch does goes through
//! [`apply_batch_for`](crate::godot_commands::apply_batch_for), so the agent's writes are
//! lowered, capability-gated, check-only'd, rolled back on failure, journaled and broadcast
//! by exactly the code the Godot pane's own edits go through. There is no second write path
//! (INV-070, INV-088).

use crate::commands::AppError;
use crate::godot_commands::{apply_batch_for, run_playtest_for, GodotApplyHost};
use bhippi_engine::capability::{evaluate_godot, CapabilityPolicy, CapabilityVerdict};
use bhippi_engine::godot::action::{
    action_kinds, action_schema_hint, GodotAction, GodotActionBatch, GodotActionOutcome,
};
use bhippi_engine::godot::probe::{PlaytestInputs, PlaytestStep};
use bhippi_engine::godot::project::GodotProjectFile;
use bhippi_engine::godot::scene::GodotScene;
use bhippi_engine::godot::tscn::TscnValue;
use bhippi_engine::godot::{gates, res_to_rel};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

// ── limits ───────────────────────────────────────────────────────────────────────────
//
// Every one of these is a *retrieval* bound, not a truncation of the truth: an answer that
// hits a cap says so and says which query narrows it, so the model asks a second question
// instead of assuming it saw everything.

/// The largest single query answer, before the retrieval hint is appended.
pub const MAX_QUERY_ANSWER_BYTES: usize = 6_000;
/// The largest slice of a `.gd` a `script` query returns.
pub const MAX_SCRIPT_QUERY_BYTES: usize = 8_000;
/// Rows a `children` / `find` answer carries.
pub const MAX_QUERY_ROWS: usize = 120;
/// Scenes a `scenes` answer lists.
pub const MAX_LISTED_SCENES: usize = 60;
/// Registry cards a `capabilities` answer carries (ADR-0035's own default).
pub const MAX_CAPABILITY_CARDS: usize = 8;
/// Output lines a bare `output` query returns.
pub const DEFAULT_OUTPUT_LINES: usize = 40;
/// The most output lines any `output` query may ask for.
pub const MAX_OUTPUT_LINES: usize = 200;
/// The most frames a model-driven playtest may run.
pub const MAX_PLAYTEST_FRAMES: u32 = 1_800;
/// Telemetry events an answer names before it stops listing them.
pub const MAX_TELEMETRY_EVENTS: usize = 40;
/// Nodes the per-turn scene digest carries. Smaller than the pane's cap on purpose: this
/// one is charged to every turn's token budget.
pub const CONTEXT_DIGEST_MAX_NODES: usize = 120;
/// Journal rows the per-turn context carries.
pub const CONTEXT_JOURNAL_ROWS: u32 = 6;
/// `@export` variables read out of one script for the fast path.
pub const MAX_EXPORT_VARS_PER_SCRIPT: usize = 64;
/// Files that only the typed action path may write (INV-088).
pub const PROTECTED_EXTENSIONS: &[&str] = &[".tscn", ".godot", ".gd", ".tres", ".cfg"];

const ACTION_OPEN: &str = "<engine_action>";
const ACTION_CLOSE: &str = "</engine_action>";
const BATCH_OPEN: &str = "<engine_batch>";
const BATCH_CLOSE: &str = "</engine_batch>";
const QUERY_OPEN: &str = "<engine_query>";
const QUERY_CLOSE: &str = "</engine_query>";

// ── the stream scanner ───────────────────────────────────────────────────────────────

/// One complete call pulled out of the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GodotCall {
    /// A single `<engine_action>` payload — its own one-action batch.
    Action(String),
    /// An `<engine_batch>` payload: `{ "label": "...", "actions": [...] }`.
    Batch(String),
    /// An `<engine_query>` payload — a read, answered back inside the same turn.
    Query(String),
}

/// Incremental scanner over a model's text stream.
#[derive(Debug, Default)]
pub struct GodotCallScanner {
    buffer: String,
    inside: Option<Inside>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inside {
    Action,
    Batch,
    Query,
}

impl GodotCallScanner {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one delta. Returns the text safe to show the user, plus any calls that closed.
    pub fn push(&mut self, delta: &str) -> (String, Vec<GodotCall>) {
        self.buffer.push_str(delta);
        let mut visible = String::new();
        let mut calls = Vec::new();

        loop {
            match self.inside {
                None => {
                    // Whichever opening tag comes first wins, so calls interleaved with
                    // prose stay in the order the model wrote them.
                    let candidates = [
                        (
                            self.buffer.find(ACTION_OPEN),
                            Inside::Action,
                            ACTION_OPEN.len(),
                        ),
                        (
                            self.buffer.find(BATCH_OPEN),
                            Inside::Batch,
                            BATCH_OPEN.len(),
                        ),
                        (
                            self.buffer.find(QUERY_OPEN),
                            Inside::Query,
                            QUERY_OPEN.len(),
                        ),
                    ];
                    let Some((at, kind, open_len)) = candidates
                        .into_iter()
                        .filter_map(|(at, kind, len)| at.map(|at| (at, kind, len)))
                        .min_by_key(|(at, _, _)| *at)
                    else {
                        // No tag in sight. Hold back a tail that could still grow into an
                        // opening tag; release everything before it.
                        let keep = partial_tag_suffix(&self.buffer);
                        let release = self.buffer.len() - keep;
                        visible.push_str(&self.buffer[..release]);
                        self.buffer.drain(..release);
                        break;
                    };
                    visible.push_str(&self.buffer[..at]);
                    self.buffer.drain(..at + open_len);
                    self.inside = Some(kind);
                }
                Some(kind) => {
                    let close = match kind {
                        Inside::Action => ACTION_CLOSE,
                        Inside::Batch => BATCH_CLOSE,
                        Inside::Query => QUERY_CLOSE,
                    };
                    let Some(at) = self.buffer.find(close) else {
                        // The call has not finished arriving; nothing inside it is visible.
                        break;
                    };
                    let payload = self.buffer[..at].trim().to_owned();
                    self.buffer.drain(..at + close.len());
                    self.inside = None;
                    if !payload.is_empty() {
                        calls.push(match kind {
                            Inside::Action => GodotCall::Action(payload),
                            Inside::Batch => GodotCall::Batch(payload),
                            Inside::Query => GodotCall::Query(payload),
                        });
                    }
                }
            }
        }
        (visible, calls)
    }

    /// Flush at end of stream. An unterminated tag is a truncated response, so its partial
    /// payload is dropped rather than half-applied; whatever plain text was held back is
    /// released so the answer is not silently truncated.
    pub fn finish(&mut self) -> String {
        let tail = if self.inside.is_some() {
            String::new()
        } else {
            std::mem::take(&mut self.buffer)
        };
        self.buffer.clear();
        self.inside = None;
        tail
    }

    /// True when a call is still arriving.
    #[must_use]
    pub fn is_mid_call(&self) -> bool {
        self.inside.is_some()
    }
}

/// How many trailing bytes of `text` could still turn into an opening tag once more of the
/// stream arrives (`"…here is a <engine_"` must not be shown yet).
fn partial_tag_suffix(text: &str) -> usize {
    let mut best = 0;
    for open in [ACTION_OPEN, BATCH_OPEN, QUERY_OPEN] {
        // The longest proper prefix of `open` that is also a suffix of `text`.
        for len in (1..open.len()).rev() {
            if len > text.len() {
                continue;
            }
            let start = text.len() - len;
            if !text.is_char_boundary(start) {
                continue;
            }
            if text[start..] == open[..len] {
                best = best.max(len);
                break;
            }
        }
    }
    best
}

/// Every call in a finished transcript, in order.
///
/// The fallback for a provider whose stream never surfaces text deltas at all — a CLI
/// adapter that reports only a final message. Calls the scanner already consumed are gone
/// from the recorded content, so this cannot double-apply them.
#[must_use]
pub fn extract_calls(text: &str) -> Vec<GodotCall> {
    let mut scanner = GodotCallScanner::new();
    let (_visible, calls) = scanner.push(text);
    calls
}

// ── payload parsing ──────────────────────────────────────────────────────────────────

/// Turn a `<engine_batch>` or `<engine_action>` body into a typed batch.
///
/// A single action becomes a one-action batch labelled with its own verb, so both write
/// forms produce the same envelope and the same journal row.
///
/// # Errors
/// The serde message, plus the schema hint for the kind the model was reaching for.
pub fn parse_call(call: &GodotCall) -> Result<GodotActionBatch, AppError> {
    match call {
        GodotCall::Batch(payload) => {
            serde_json::from_str::<GodotActionBatch>(payload).map_err(|error| AppError {
                message: format!("that is not a Godot action batch: {error}"),
                hint: Some(batch_shape_hint(payload)),
            })
        }
        GodotCall::Action(payload) => {
            let action =
                serde_json::from_str::<GodotAction>(payload).map_err(|error| AppError {
                    message: format!("that is not a Godot action: {error}"),
                    hint: Some(batch_shape_hint(payload)),
                })?;
            Ok(GodotActionBatch::new(action.to_label(), vec![action]))
        }
        GodotCall::Query(_) => Err(AppError {
            message: "a query is a read, not a batch".to_owned(),
            hint: None,
        }),
    }
}

/// The hint a malformed payload gets: the schema of the kind it named, when it named one it
/// could have meant, and otherwise the whole verb list.
fn batch_shape_hint(payload: &str) -> String {
    let named = serde_json::from_str::<Value>(payload)
        .ok()
        .and_then(|value| {
            value
                .get("kind")
                .or_else(|| value.get("actions")?.get(0)?.get("kind"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .and_then(|kind| action_schema_hint(&kind));
    match named {
        Some(hint) => format!("Expected {hint}."),
        None => format!(
            "A batch is {{\"label\":\"…\",\"actions\":[…]}}. Verbs: {}.",
            action_kinds().join(", ")
        ),
    }
}

// ── writes ───────────────────────────────────────────────────────────────────────────

/// What one `<engine_batch>` did, in the shape the repair round reads.
#[derive(Clone, Debug)]
pub struct GodotWriteResult {
    pub applied: bool,
    pub label: String,
    /// Per-action outcomes when the batch lowered; empty when it did not.
    pub outcomes: Vec<GodotActionOutcome>,
    /// Project-relative, forward slashes.
    pub changed_files: Vec<String>,
    pub txn_id: Option<String>,
    pub revision: Option<i64>,
    /// The action that stopped the batch.
    pub failing_index: Option<usize>,
    /// Godot's own `file:line: message` when a script check failed, or the lowering fault.
    pub message: Option<String>,
    pub hint: Option<String>,
    /// `add_node{groups,name,parent,properties,scene,type}` for the failing verb.
    pub schema_hint: Option<String>,
}

impl GodotWriteResult {
    /// The one-line Activity Dock detail.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.applied {
            let files = if self.changed_files.is_empty() {
                "nothing changed".to_owned()
            } else {
                self.changed_files.join(", ")
            };
            return format!("{} — {files}", self.label);
        }
        let where_ = match self.failing_index {
            Some(index) => format!("action {index}"),
            None => "the batch".to_owned(),
        };
        format!(
            "{} — rejected at {where_}: {}",
            self.label,
            self.message.as_deref().unwrap_or("no reason given")
        )
    }
}

/// A human-readable summary of what a batch is about to do: `+3 nodes · 1 script ·
/// scenes/main.tscn`.
///
/// Counts first, because that is the shape of the question a plan card asks.
#[must_use]
pub fn plan_summary(batch: &GodotActionBatch) -> String {
    let mut nodes = 0_usize;
    let mut removed = 0_usize;
    let mut scripts = 0_usize;
    let mut settings = 0_usize;
    let mut edits = 0_usize;
    let mut scenes: Vec<String> = Vec::new();
    for action in &batch.actions {
        match action {
            GodotAction::AddNode { scene, .. } | GodotAction::InstanceScene { scene, .. } => {
                nodes += 1;
                push_unique(&mut scenes, scene);
            }
            GodotAction::RemoveNode { scene, .. } => {
                removed += 1;
                push_unique(&mut scenes, scene);
            }
            GodotAction::WriteScript { path, .. } | GodotAction::DeleteScript { path } => {
                scripts += 1;
                push_unique(&mut scenes, path);
            }
            GodotAction::CreateScene { path, .. } => {
                nodes += 1;
                push_unique(&mut scenes, path);
            }
            GodotAction::SetMainScene { .. }
            | GodotAction::AddAutoload { .. }
            | GodotAction::AddInputAction { .. } => settings += 1,
            other => {
                edits += 1;
                if let Some(scene) = scene_of(other) {
                    push_unique(&mut scenes, scene);
                }
            }
        }
    }
    let mut parts: Vec<String> = Vec::new();
    if nodes > 0 {
        parts.push(format!("+{nodes} node{}", plural(nodes)));
    }
    if removed > 0 {
        parts.push(format!("−{removed} node{}", plural(removed)));
    }
    if scripts > 0 {
        parts.push(format!("{scripts} script{}", plural(scripts)));
    }
    if edits > 0 {
        parts.push(format!("{edits} edit{}", plural(edits)));
    }
    if settings > 0 {
        parts.push(format!("{settings} project setting{}", plural(settings)));
    }
    if parts.is_empty() {
        parts.push(format!("{} action(s)", batch.actions.len()));
    }
    parts.extend(scenes.into_iter().take(3));
    format!("{} — {}", batch.display_label(), parts.join(" · "))
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn push_unique(into: &mut Vec<String>, value: &str) {
    if !into.iter().any(|held| held == value) {
        into.push(value.to_owned());
    }
}

fn scene_of(action: &GodotAction) -> Option<&str> {
    match action {
        GodotAction::AddNode { scene, .. }
        | GodotAction::RemoveNode { scene, .. }
        | GodotAction::RenameNode { scene, .. }
        | GodotAction::ReparentNode { scene, .. }
        | GodotAction::SetProperty { scene, .. }
        | GodotAction::RemoveProperty { scene, .. }
        | GodotAction::AddToGroup { scene, .. }
        | GodotAction::AttachScript { scene, .. }
        | GodotAction::InstanceScene { scene, .. }
        | GodotAction::ConnectSignal { scene, .. } => Some(scene),
        _ => None,
    }
}

/// This project's `[agent]` policy over one batch: what it needs, what it must be asked for,
/// and what it may not do at all.
#[must_use]
pub fn verdict_for(root: &Path, batch: &GodotActionBatch) -> CapabilityVerdict {
    let kinds: Vec<String> = batch
        .actions
        .iter()
        .map(|action| action.kind().to_owned())
        .collect();
    evaluate_godot(&policy_of(root), &kinds)
}

/// The same verdict for a read that costs something to run.
#[must_use]
pub fn verdict_for_kind(root: &Path, kind: &str) -> CapabilityVerdict {
    evaluate_godot(&policy_of(root), &[kind.to_owned()])
}

fn policy_of(root: &Path) -> CapabilityPolicy {
    bhippi_engine::manifest::load_manifest(root)
        .ok()
        .flatten()
        .map(|manifest| manifest.agent)
        .unwrap_or_default()
}

/// Apply one batch as the agent, through the pane's own path.
pub async fn apply_batch(
    host: GodotApplyHost<'_>,
    root: &Path,
    batch: &GodotActionBatch,
) -> GodotWriteResult {
    match apply_batch_for(host, root, batch, "agent").await {
        Ok(result) => GodotWriteResult {
            applied: true,
            label: result.label,
            outcomes: result.outcomes,
            changed_files: result.changed_files,
            txn_id: Some(result.txn_id),
            revision: result.revision,
            failing_index: None,
            message: None,
            hint: None,
            schema_hint: None,
        },
        Err(failure) => {
            let kind = failure.kind.clone().or_else(|| {
                failure
                    .index
                    .and_then(|index| batch.actions.get(index))
                    .map(|action| action.kind().to_owned())
            });
            GodotWriteResult {
                applied: false,
                label: batch.display_label(),
                outcomes: Vec::new(),
                changed_files: Vec::new(),
                txn_id: None,
                revision: None,
                failing_index: failure.index,
                message: Some(failure.error.message),
                hint: failure.error.hint,
                schema_hint: kind.as_deref().and_then(action_schema_hint),
            }
        }
    }
}

/// A refusal shaped like a write result, for a batch that never reached the apply path.
#[must_use]
pub fn refused(
    batch: &GodotActionBatch,
    message: String,
    hint: Option<String>,
) -> GodotWriteResult {
    GodotWriteResult {
        applied: false,
        label: batch.display_label(),
        outcomes: Vec::new(),
        changed_files: Vec::new(),
        txn_id: None,
        revision: None,
        failing_index: None,
        message: Some(message),
        hint,
        schema_hint: None,
    }
}

/// A refusal for a payload that never parsed into a batch at all.
#[must_use]
pub fn malformed(error: &AppError) -> GodotWriteResult {
    GodotWriteResult {
        applied: false,
        label: "malformed engine call".to_owned(),
        outcomes: Vec::new(),
        changed_files: Vec::new(),
        txn_id: None,
        revision: None,
        failing_index: None,
        message: Some(error.message.clone()),
        hint: error.hint.clone(),
        schema_hint: None,
    }
}

// ── INV-088 ──────────────────────────────────────────────────────────────────────────

/// Refuse a generic file write that would land on a Godot project file.
///
/// The typed action path is not a *preference*: it is the only writer that lowers, inverts,
/// journals and check-compiles. A hand-written `.tscn` skips all four, and the resulting
/// scene is a change nobody can attribute, undo or trust. So the file tool refuses the write
/// and says which verb does the job — a refusal with a route, not a dead end.
///
/// `None` means the write is fine.
#[must_use]
pub fn protected_write_refusal(root: &Path, relative: &str) -> Option<AppError> {
    let lower = relative.to_ascii_lowercase().replace('\\', "/");
    let extension = PROTECTED_EXTENSIONS
        .iter()
        .find(|extension| lower.ends_with(**extension))?;
    // Only inside a Godot project: a `.cfg` in an unrelated workspace is just a file.
    if !is_godot_project(root) {
        return None;
    }
    let verb = match *extension {
        ".gd" => "`write_script`",
        ".tscn" => "`create_scene` / `add_node` / `set_property`",
        ".godot" => "`set_main_scene` / `add_autoload` / `add_input_action`",
        _ => "the typed action path",
    };
    Some(AppError {
        message: format!(
            "{relative} is a Godot project file; the agent never writes one directly (INV-088)."
        ),
        hint: Some(format!(
            "Send an <engine_batch> using {verb} instead. Every typed action is lowered, \
             check-compiled, journaled and undoable; a hand-written file is none of those."
        )),
    })
}

/// True when this folder is a Godot project Bhippi drives.
#[must_use]
pub fn is_godot_project(root: &Path) -> bool {
    root.join(bhippi_engine::godot::action::PROJECT_FILE)
        .is_file()
}

/// The Godot project root behind a workspace path, when there is one.
#[must_use]
pub fn godot_root_of(workspace: &str) -> Option<PathBuf> {
    let root = crate::engine::game_dir_of(workspace).ok()?;
    let manifest = bhippi_engine::manifest::load_manifest(&root)
        .ok()
        .flatten()?;
    (bhippi_engine::godot::manifest::is_godot(&manifest) && is_godot_project(&root)).then_some(root)
}

// ── queries ──────────────────────────────────────────────────────────────────────────

/// The read half of the vocabulary. Everything here is bounded in Rust: the model asks for a
/// thing, not for a number of rows.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum GodotQuery {
    Scene {
        #[serde(default)]
        scene: Option<String>,
    },
    Node {
        #[serde(default)]
        scene: Option<String>,
        path: String,
    },
    Children {
        #[serde(default)]
        scene: Option<String>,
        path: String,
    },
    Find {
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default, rename = "type")]
        type_: Option<String>,
        #[serde(default)]
        group: Option<String>,
    },
    Scenes,
    Project,
    Script {
        path: String,
    },
    Status,
    Gates {
        #[serde(default)]
        release: Option<bool>,
    },
    Output {
        #[serde(default)]
        lines: Option<usize>,
    },
    Playtest {
        #[serde(default)]
        steps: Option<Vec<QueryPlaytestStep>>,
        #[serde(default)]
        frames: Option<u32>,
    },
    Capabilities {
        intent: String,
    },
    Describe {
        id: String,
    },
}

/// One scripted input, as a model writes it.
#[derive(Debug, Deserialize)]
struct QueryPlaytestStep {
    frame: u32,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default = "default_pressed")]
    pressed: bool,
}

fn default_pressed() -> bool {
    true
}

/// Answer one `<engine_query>`.
///
/// Always returns text — a query that cannot be answered is answered with the reason, which
/// is what lets the bounded loop repair itself instead of stalling.
pub async fn answer_query(host: GodotApplyHost<'_>, root: &Path, payload: &str) -> String {
    let query: GodotQuery = match serde_json::from_str(payload) {
        Ok(query) => query,
        Err(error) => {
            return compact(&json!({
                "error": format!("that is not a query: {error}"),
                "kinds": QUERY_KINDS,
            }))
        }
    };
    let answer = match run_query(host, root, query).await {
        Ok(value) => value,
        Err(error) => json!({
            "error": error.message,
            "hint": error.hint,
        }),
    };
    cap_answer(compact(&answer))
}

/// Every query kind, for the "you asked for something that is not a verb" answer.
pub const QUERY_KINDS: [&str; 13] = [
    "scene",
    "node",
    "children",
    "find",
    "scenes",
    "project",
    "script",
    "status",
    "gates",
    "output",
    "playtest",
    "capabilities",
    "describe",
];

#[allow(clippy::too_many_lines)]
async fn run_query(
    host: GodotApplyHost<'_>,
    root: &Path,
    query: GodotQuery,
) -> Result<Value, AppError> {
    match query {
        GodotQuery::Scene { scene } => {
            let (rel, parsed) = load_scene(root, scene.as_deref())?;
            let total = parsed.node_count();
            Ok(json!({
                "kind": "scene",
                "scene": rel,
                "nodes": total,
                "truncated": total > CONTEXT_DIGEST_MAX_NODES,
                "digest": parsed.tree_digest(CONTEXT_DIGEST_MAX_NODES),
            }))
        }
        GodotQuery::Node { scene, path } => {
            let (rel, parsed) = load_scene(root, scene.as_deref())?;
            let view = parsed.node(&path).ok_or_else(|| AppError {
                message: format!("`{path}` is not in {rel}."),
                hint: Some(
                    "Node paths are scene-relative: `.` is the root, `Player/Mesh` a child. \
                     Ask {\"kind\":\"scene\"} for the tree."
                        .to_owned(),
                ),
            })?;
            let properties: serde_json::Map<String, Value> = view
                .properties
                .iter()
                .map(|(name, value)| (name.clone(), Value::String(value.to_text())))
                .collect();
            Ok(json!({
                "kind": "node",
                "scene": rel,
                "path": view.path,
                "name": view.name,
                "type": view.type_,
                "script": view.script,
                "instance": view.instance,
                "groups": view.groups,
                "properties": properties,
            }))
        }
        GodotQuery::Children { scene, path } => {
            let (rel, parsed) = load_scene(root, scene.as_deref())?;
            if !parsed.contains(&path) {
                return Err(AppError {
                    message: format!("`{path}` is not in {rel}."),
                    hint: Some("Ask {\"kind\":\"scene\"} for the tree.".to_owned()),
                });
            }
            let all = parsed.children(&path);
            let total = all.len();
            let rows: Vec<Value> = all
                .into_iter()
                .take(MAX_QUERY_ROWS)
                .map(|child| {
                    let type_ = parsed.node(&child).and_then(|view| view.type_);
                    json!({ "path": child, "type": type_ })
                })
                .collect();
            Ok(json!({
                "kind": "children",
                "scene": rel,
                "path": path,
                "total": total,
                "children": rows,
            }))
        }
        GodotQuery::Find {
            scene,
            name,
            type_,
            group,
        } => {
            let (rel, parsed) = load_scene(root, scene.as_deref())?;
            let mut matches = match (name.as_deref(), type_.as_deref(), group.as_deref()) {
                (Some(name), _, _) => parsed.find_by_name(name),
                (_, Some(type_), _) => parsed.find_by_type(type_),
                (_, _, Some(group)) => parsed.find_in_group(group),
                _ => {
                    return Err(AppError {
                        message: "find needs one of `name`, `type` or `group`.".to_owned(),
                        hint: Some(
                            "e.g. {\"kind\":\"find\",\"type\":\"Camera3D\"}. Groups in this \
                             scene are on the `scene` answer's digest."
                                .to_owned(),
                        ),
                    })
                }
            };
            let total = matches.len();
            matches.truncate(MAX_QUERY_ROWS);
            Ok(json!({
                "kind": "find",
                "scene": rel,
                "total": total,
                "matches": matches,
            }))
        }
        GodotQuery::Scenes => {
            let mut scenes = gates::scene_files(root);
            let total = scenes.len();
            scenes.truncate(MAX_LISTED_SCENES);
            Ok(json!({
                "kind": "scenes",
                "total": total,
                "main": main_scene_rel(root),
                "scenes": scenes,
            }))
        }
        GodotQuery::Project => {
            let file = project_file(root)?;
            let autoloads: Vec<Value> = file
                .autoloads()
                .into_iter()
                .map(|autoload| {
                    json!({
                        "name": autoload.name,
                        "path": autoload.path,
                        "singleton": autoload.singleton,
                    })
                })
                .collect();
            Ok(json!({
                "kind": "project",
                "name": file.name(),
                "main_scene": file.main_scene(),
                "autoloads": autoloads,
                "input_actions": file.input_actions(),
            }))
        }
        GodotQuery::Script { path } => {
            let rel = safe_relative(root, &path)?;
            let full = root.join(&rel);
            let source = std::fs::read_to_string(&full).map_err(|error| AppError {
                message: format!("could not read {rel}: {error}"),
                hint: Some(
                    "Scripts a scene references are on the node answer's `script` field."
                        .to_owned(),
                ),
            })?;
            let bytes = source.len();
            let mut clipped = source;
            let truncated = bytes > MAX_SCRIPT_QUERY_BYTES;
            if truncated {
                clipped = clip(&clipped, MAX_SCRIPT_QUERY_BYTES);
            }
            Ok(json!({
                "kind": "script",
                "path": rel,
                "bytes": bytes,
                "truncated": truncated,
                "source": clipped,
            }))
        }
        GodotQuery::Status => {
            let install = crate::godot::detect_godot(None).await;
            Ok(json!({
                "kind": "status",
                "installed": install.is_some(),
                "version": install.as_ref().map(|install| install.version.clone()),
                "templates_installed": install.as_ref().is_some_and(|install| {
                    bhippi_engine::godot::detect::export_templates_installed(&install.version)
                }),
                "is_godot_project": is_godot_project(root),
                "main_scene": main_scene_rel(root),
                "running": running_kind(host, root),
            }))
        }
        GodotQuery::Gates { release } => {
            let report = gates::check_project(root, release.unwrap_or(false));
            let finding = |finding: &gates::Finding| {
                json!({
                    "code": finding.code,
                    "message": finding.message,
                    "hint": finding.hint,
                    "where": finding.where_,
                })
            };
            Ok(json!({
                "kind": "gates",
                "release": release.unwrap_or(false),
                "passes": report.passes(),
                "blockers": report.blockers.iter().map(finding).collect::<Vec<_>>(),
                "warnings": report.warnings.iter().map(finding).collect::<Vec<_>>(),
            }))
        }
        GodotQuery::Output { lines } => {
            let want = lines
                .unwrap_or(DEFAULT_OUTPUT_LINES)
                .clamp(1, MAX_OUTPUT_LINES);
            let tail = output_tail(host, root, want);
            Ok(json!({
                "kind": "output",
                "lines": tail,
            }))
        }
        GodotQuery::Playtest { steps, frames } => {
            // RunPlay is a real capability: a playtest starts a process. `deny` refuses it
            // here rather than after Godot has been launched.
            if let Some(refusal) = verdict_for_kind(root, "playtest").refusal() {
                return Err(AppError {
                    message: refusal,
                    hint: Some("Allow `run_play` under `[agent]` in Bhippi.game.toml.".to_owned()),
                });
            }
            let inputs = match steps {
                Some(steps) if !steps.is_empty() => PlaytestInputs::new(
                    steps
                        .into_iter()
                        .map(|step| PlaytestStep {
                            frame: step.frame,
                            action: step.action,
                            key: step.key,
                            pressed: step.pressed,
                        })
                        .collect(),
                ),
                _ => crate::godot_commands::default_playtest_inputs(),
            };
            inputs.validate().map_err(|error| AppError {
                message: error.to_string(),
                hint: error.hint().map(str::to_owned),
            })?;
            let frames = frames.map(|frames| frames.min(MAX_PLAYTEST_FRAMES));
            let result = run_playtest_for(host, root, inputs, frames).await?;
            let report = result.report;
            let events: Vec<String> = report
                .event_names()
                .into_iter()
                .take(MAX_TELEMETRY_EVENTS)
                .collect();
            Ok(json!({
                "kind": "playtest",
                "done": report.done,
                "frames": report.frames,
                "samples": report.sample_count(),
                "malformed_lines": report.malformed_lines,
                "truncated": report.truncated,
                "vars": report.vars,
                "last_positions": report.last_positions,
                "events": events,
                "exit_code": result.exit.code,
                "log_tail": result.log_tail,
            }))
        }
        GodotQuery::Capabilities { intent } => {
            let registry =
                bhippi_engine::registry::CapabilityRegistry::core().map_err(|error| AppError {
                    message: error.to_string(),
                    hint: error.hint().map(str::to_owned),
                })?;
            let found = registry.search(&bhippi_engine::registry::CapabilitySearch {
                intent,
                limit: Some(MAX_CAPABILITY_CARDS),
                ..bhippi_engine::registry::CapabilitySearch::default()
            });
            let cards: Vec<Value> = found
                .cards
                .iter()
                .map(|card| json!({ "id": card.id, "name": card.name, "purpose": card.purpose }))
                .collect();
            Ok(json!({
                "kind": "capabilities",
                "registry_hash": found.registry_hash,
                "cards": cards,
            }))
        }
        GodotQuery::Describe { id } => {
            let registry =
                bhippi_engine::registry::CapabilityRegistry::core().map_err(|error| AppError {
                    message: error.to_string(),
                    hint: error.hint().map(str::to_owned),
                })?;
            let entry = registry.require(&id).map_err(|error| AppError {
                message: error.to_string(),
                hint: error.hint().map(str::to_owned),
            })?;
            Ok(json!({
                "kind": "describe",
                "id": entry.id,
                "name": entry.name,
                "purpose": entry.purpose,
                "properties": entry
                    .properties
                    .iter()
                    .map(|field| json!({
                        "name": field.name,
                        "type": field.type_name,
                        "required": field.required,
                    }))
                    .collect::<Vec<_>>(),
                "keywords": entry.keywords,
                "limitations": entry.limitations,
                "available": entry.available,
            }))
        }
    }
}

/// The scene a query is about: the one named, else the project's main scene.
fn load_scene(root: &Path, scene: Option<&str>) -> Result<(String, GodotScene), AppError> {
    let rel = match scene.map(str::trim).filter(|rel| !rel.is_empty()) {
        Some(named) => safe_relative(root, named)?,
        None => main_scene_rel(root).ok_or_else(|| AppError {
            message: "this project has no main scene".to_owned(),
            hint: Some("Ask {\"kind\":\"scenes\"} and name one.".to_owned()),
        })?,
    };
    let text = std::fs::read_to_string(root.join(&rel)).map_err(|error| AppError {
        message: format!("could not read {rel}: {error}"),
        hint: Some("Ask {\"kind\":\"scenes\"} for the scenes that exist.".to_owned()),
    })?;
    let parsed = GodotScene::parse(&text.replace("\r\n", "\n")).map_err(|error| AppError {
        message: format!("{rel} did not parse: {error}"),
        hint: error.hint().map(str::to_owned),
    })?;
    Ok((rel, parsed))
}

/// A project-relative path that cannot leave the project. `res://` is accepted because that
/// is how the model sees paths everywhere else.
fn safe_relative(root: &Path, path: &str) -> Result<String, AppError> {
    let normalised = res_to_rel(path.trim()).replace('\\', "/");
    let leaves = normalised.starts_with('/')
        || normalised.split('/').any(|segment| segment == "..")
        || Path::new(&normalised).is_absolute();
    if normalised.is_empty() || leaves {
        return Err(AppError {
            message: format!("`{path}` is not inside the project."),
            hint: Some("Paths are project-relative, like `scenes/main.tscn`.".to_owned()),
        });
    }
    let _unused = root;
    Ok(normalised)
}

fn main_scene_rel(root: &Path) -> Option<String> {
    if let Some(main) = bhippi_engine::manifest::load_manifest(root)
        .ok()
        .flatten()
        .and_then(|manifest| {
            manifest
                .godot
                .as_ref()
                .map(|section| section.main_scene.clone())
                .or(Some(manifest.game.default_scene.clone()))
        })
        .filter(|scene| !scene.trim().is_empty())
    {
        return Some(res_to_rel(&main));
    }
    project_file(root)
        .ok()
        .and_then(|file| file.main_scene())
        .map(|res| res_to_rel(&res))
}

fn project_file(root: &Path) -> Result<GodotProjectFile, AppError> {
    let path = root.join(bhippi_engine::godot::action::PROJECT_FILE);
    let text = std::fs::read_to_string(&path).map_err(|error| AppError {
        message: format!("could not read project.godot: {error}"),
        hint: Some("This folder is not a Godot project yet.".to_owned()),
    })?;
    GodotProjectFile::parse(&text.replace("\r\n", "\n")).map_err(|error| AppError {
        message: format!("project.godot did not parse: {error}"),
        hint: error.hint().map(str::to_owned),
    })
}

fn running_kind(host: GodotApplyHost<'_>, root: &Path) -> Option<String> {
    let app = host.app?;
    use tauri::Manager as _;
    let store = app.try_state::<crate::godot_commands::GodotSessionStore>()?;
    let sessions = store.inner().lock().ok()?;
    let session = sessions.get(&crate::workspace::display_path(root))?;
    let running = session.running.as_ref()?;
    (!running.handle.is_stopped()).then(|| format!("{:?}", running.kind).to_lowercase())
}

fn output_tail(host: GodotApplyHost<'_>, root: &Path, want: usize) -> Vec<String> {
    let Some(app) = host.app else {
        return Vec::new();
    };
    use tauri::Manager as _;
    let Some(store) = app.try_state::<crate::godot_commands::GodotSessionStore>() else {
        return Vec::new();
    };
    let Ok(sessions) = store.inner().lock() else {
        return Vec::new();
    };
    let Some(session) = sessions.get(&crate::workspace::display_path(root)) else {
        return Vec::new();
    };
    let lines: Vec<String> = session
        .output
        .iter()
        .map(|line| line.text.clone())
        .collect();
    let skip = lines.len().saturating_sub(want);
    lines.into_iter().skip(skip).collect()
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{\"error\":\"unserialisable\"}".to_owned())
}

/// Cap one answer, saying which query narrows it rather than leaving the model to guess it
/// saw everything.
fn cap_answer(answer: String) -> String {
    if answer.len() <= MAX_QUERY_ANSWER_BYTES {
        return answer;
    }
    format!(
        "{}\n…answer capped at {MAX_QUERY_ANSWER_BYTES} bytes. Narrow it: ask `children` for \
         one subtree, `node` for one node, or `find` for one type.",
        clip(&answer, MAX_QUERY_ANSWER_BYTES)
    )
}

fn clip(text: &str, max_bytes: usize) -> String {
    let mut boundary = max_bytes.min(text.len());
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text[..boundary].to_owned()
}

// ── the bounded loop ─────────────────────────────────────────────────────────────────

/// The follow-up message that continues a turn (GAD-086/089).
///
/// Two things can oblige a continuation: the model asked a question and is owed the answer,
/// or a batch was rejected and is owed the failing index, Godot's own located message and
/// that verb's real schema. Both are evidence, not scolding. `None` means the turn is done.
#[must_use]
pub fn continuation_prompt(
    answers: &[(String, String)],
    results: &[GodotWriteResult],
) -> Option<String> {
    let failed: Vec<&GodotWriteResult> = results.iter().filter(|row| !row.applied).collect();
    if answers.is_empty() && failed.is_empty() {
        return None;
    }
    let mut out = String::new();
    if !answers.is_empty() {
        out.push_str("Godot query answers and typed errors:\n");
        for (query, answer) in answers {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\n### {query}\n{answer}\n"));
        }
        if failed.is_empty() {
            out.push_str("\nContinue. Emit the batch you decided on, or say what you found.\n");
            return Some(out);
        }
    }
    out.push_str(
        "\nYour engine batch was REJECTED and nothing was written. A batch is all-or-nothing, \
         so fix the failing action and resend the whole batch.\n",
    );
    for result in failed {
        let _ =
            std::fmt::Write::write_fmt(&mut out, format_args!("\n## Batch \"{}\"\n", result.label));
        if let Some(index) = result.failing_index {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!("Failing action index: {index}\n"),
            );
        }
        if let Some(message) = &result.message {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("Godot said: {message}\n"));
        }
        if let Some(hint) = &result.hint {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("hint: {hint}\n"));
        }
        if let Some(schema) = &result.schema_hint {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("schema: {schema}\n"));
        }
    }
    out.push_str(
        "\nResend one corrected <engine_batch>, including the actions that were fine — the whole \
         batch was rolled back.\n",
    );
    Some(out)
}

/// What is still owed when the round budget runs out.
#[must_use]
pub fn unresolved_work(
    answers: &[(String, String)],
    results: &[GodotWriteResult],
) -> Option<String> {
    let rejected: Vec<String> = results
        .iter()
        .filter(|row| !row.applied)
        .map(GodotWriteResult::summary)
        .collect();
    if !rejected.is_empty() {
        return Some(rejected.join(" · "));
    }
    (!answers.is_empty()).then(|| {
        format!(
            "{} engine question(s) answered but not acted on",
            answers.len()
        )
    })
}

// ── the per-turn context (GAD-092) ───────────────────────────────────────────────────

/// The Godot facts a turn opens with: what the project is, what the open scene looks like,
/// what changed recently, whether anything is blocking, and what is running.
///
/// Retrieval, not a dump — everything deeper is one `<engine_query>` away, which is the whole
/// point of having a read API. The caller caps this against
/// `ENGINE_CONTEXT_TOKEN_BUDGET`.
pub async fn project_facts(host: GodotApplyHost<'_>, root: &Path) -> String {
    let mut facts = String::new();
    let name = bhippi_engine::manifest::load_manifest(root)
        .ok()
        .flatten()
        .map(|manifest| manifest.game.name)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "Untitled".to_owned());
    let main = main_scene_rel(root).unwrap_or_else(|| "(none)".to_owned());
    let _ = std::fmt::Write::write_fmt(
        &mut facts,
        format_args!("## This Godot project\nName: {name}\nMain scene: {main}\n"),
    );

    match load_scene(root, None) {
        Ok((rel, scene)) => {
            let total = scene.node_count();
            let _ = std::fmt::Write::write_fmt(
                &mut facts,
                format_args!(
                    "\n### Scene tree — {rel} ({total} nodes)\n{}",
                    scene.tree_digest(CONTEXT_DIGEST_MAX_NODES)
                ),
            );
        }
        Err(error) => {
            let _ = std::fmt::Write::write_fmt(
                &mut facts,
                format_args!("\n### Scene tree\nunavailable: {}\n", error.message),
            );
        }
    }

    if let Some(selection) = selected_node(host, root) {
        let _ = std::fmt::Write::write_fmt(
            &mut facts,
            format_args!("\n### The user has selected\n{selection}\n"),
        );
    }

    let recent = crate::engine::recent_journal(root, CONTEXT_JOURNAL_ROWS).await;
    if !recent.is_empty() {
        facts.push_str("\n### Recent changes (newest first)\n");
        for row in recent {
            let _ = std::fmt::Write::write_fmt(
                &mut facts,
                format_args!(
                    "- r{} [{}] {}\n",
                    row.revision,
                    row.actor,
                    row.label.unwrap_or_default()
                ),
            );
        }
    }

    let report = gates::check_project(root, false);
    let _ = std::fmt::Write::write_fmt(
        &mut facts,
        format_args!(
            "\n### State\nGate blockers: {}\nRunning: {}\n",
            report.blockers.len(),
            running_kind(host, root).unwrap_or_else(|| "nothing".to_owned())
        ),
    );
    facts.push_str(
        "\nThis is a digest, not the project. Ask <engine_query> for anything deeper: \
         `node`, `children`, `find`, `script`, `scenes`, `project`, `gates`, `output`, \
         `status`, `playtest`, `capabilities`, `describe`.\n",
    );
    facts
}

/// The node the Godot pane reports as open/selected, when it has one.
fn selected_node(host: GodotApplyHost<'_>, root: &Path) -> Option<String> {
    let app = host.app?;
    use tauri::Manager as _;
    let store = app.try_state::<crate::godot_commands::GodotSessionStore>()?;
    let sessions = store.inner().lock().ok()?;
    let session = sessions.get(&crate::workspace::display_path(root))?;
    session
        .open_scene
        .clone()
        .map(|scene| format!("scene {scene}"))
}

// ── the no-model fast path (GAD-035 / §5.4) ──────────────────────────────────────────

/// One `@export` variable found in a script a node carries.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportVar {
    pub name: String,
    /// The literal on the `@export` line.
    pub default: f64,
    /// True when that literal was written as a float (`5.0`, `5.5`) rather than an integer.
    pub float_literal: bool,
    /// The script the line lives in, project-relative.
    pub script_rel: String,
    /// 0-based index of the `@export` line inside that script.
    pub line: usize,
    /// What the scene overrides it to, when the `.tscn` carries the property. `Some` means
    /// the live value lives in the scene and `set_property` is the right verb; `None` means
    /// the only value is the script default and `write_script` is.
    pub scene_override: Option<f64>,
}

/// Everything the fast path knows about one node.
#[derive(Clone, Debug)]
pub struct NodeExports {
    pub path: String,
    pub class: String,
    pub vars: Vec<ExportVar>,
}

/// The one-action plan a parameter edit lowers to.
#[derive(Clone, Debug)]
pub struct FastPathPlan {
    pub batch: GodotActionBatch,
    /// The sentence the Undo toast or the confirm chip shows.
    pub label: String,
    pub confidence_bps: u16,
    /// True in the 0.6–0.9 band: show a chip and wait for a yes.
    pub needs_confirm: bool,
    /// `set_property` when the live value is in the scene, `write_script` when it is only a
    /// script default. Reported so the tool card can say which file moved.
    pub through: &'static str,
}

/// Read the open scene and the scripts it attaches into the shape
/// [`bhippi_engine::intent::fast_path`] wants.
///
/// Deliberately cheap and deliberately narrow: one scene, its attached scripts, their
/// `@export` lines. Nothing here opens a provider, and nothing here is allowed to guess.
#[must_use]
pub fn fast_path_scan(root: &Path) -> Vec<NodeExports> {
    let Ok((_rel, scene)) = load_scene(root, None) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for node in &scene.nodes {
        let Some(view) = scene.node(&node.path) else {
            continue;
        };
        let mut vars = Vec::new();
        if let Some(script_res) = view.script.as_deref() {
            let script_rel = res_to_rel(script_res);
            if let Ok(source) = std::fs::read_to_string(root.join(&script_rel)) {
                vars = scan_exports(&source, &script_rel);
                for var in &mut vars {
                    var.scene_override =
                        scene.property(&node.path, &var.name).and_then(tscn_number);
                }
            }
        }
        out.push(NodeExports {
            path: node.path.clone(),
            class: node.type_.clone().unwrap_or_default(),
            vars,
        });
    }
    out
}

/// A parameter edit this turn can make without a model call, or `None` when it cannot.
///
/// `None` is the common and correct answer: the rule the fast path lives by is *never guess
/// a node*, and one wrong silent edit costs far more than one model call.
#[must_use]
pub fn fast_path_plan(root: &Path, utterance: &str) -> Option<FastPathPlan> {
    use bhippi_engine::intent::fast_path::{
        propose, FastPathContext, FastPathOp, NodeSummary, ScriptVar, TscnValueLite,
    };

    let scanned = fast_path_scan(root);
    let context = FastPathContext {
        // Preset packs are not lowered to Godot node graphs yet (GAD-094), so the fast path
        // resolves against real nodes only. Claiming otherwise would let it "apply" an edit
        // to something that does not exist in the project.
        presets_in_project: Vec::new(),
        nodes: scanned
            .iter()
            .map(|node| NodeSummary {
                path: node.path.clone(),
                class: node.class.clone(),
                script_vars: node
                    .vars
                    .iter()
                    .map(|var| ScriptVar {
                        name: var.name.clone(),
                        value: var.scene_override.unwrap_or(var.default),
                    })
                    .collect(),
            })
            .collect(),
    };

    let proposal = propose(utterance, &context)?;
    // Ambiguous, or resolved onto a preset there is no node for: a model turn, not a guess.
    if proposal.needs_choice() {
        return None;
    }
    let node_path = proposal.target.node_path.clone()?;
    let property = proposal.target.property.clone();

    let export = scanned
        .iter()
        .find(|node| node.path == node_path)
        .and_then(|node| node.vars.iter().find(|var| var.name == property))
        .cloned();
    let scene_value = load_scene(root, None)
        .ok()
        .and_then(|(_rel, scene)| scene.property(&node_path, &property).and_then(tscn_number));
    let current = export
        .as_ref()
        .map(|var| var.scene_override.unwrap_or(var.default))
        .or(scene_value);

    let next = match (&proposal.op, current) {
        (FastPathOp::Multiply { factor }, Some(now)) => Some(now * factor),
        (FastPathOp::Add { amount }, Some(now)) => Some(now + amount),
        // Booleans and enum-ish text land on the scene directly; there is no arithmetic
        // and no script line to rewrite, so they fall through to the wildcard below.
        (
            FastPathOp::Set {
                value: TscnValueLite::Number { value },
            },
            _,
        ) => Some(*value),
        // A relative change with nothing to be relative to. The model can read the class
        // default; this cannot, and inventing one is exactly the silent-wrong-edit case.
        _ => None,
    };
    let literal = match (&proposal.op, next) {
        (
            FastPathOp::Set {
                value: TscnValueLite::Bool { value },
            },
            _,
        ) => TscnValue::Bool(*value),
        (
            FastPathOp::Set {
                value: TscnValueLite::Text { value },
            },
            _,
        ) => TscnValue::str(value),
        (_, Some(next)) => number_value(next, export.as_ref(), scene_value.is_some()),
        _ => return None,
    };

    let scene_rel = main_scene_rel(root)?;
    let (batch, through) = match export.as_ref() {
        // The live value is a script default and nothing overrides it, so writing the scene
        // would leave two numbers disagreeing. Rewrite the one line that holds it.
        Some(var) if var.scene_override.is_none() => {
            let source = std::fs::read_to_string(root.join(&var.script_rel)).ok()?;
            let rewritten = rewrite_export_line(&source, var, literal_text(&literal))?;
            (
                GodotActionBatch::new(
                    proposal.label.clone(),
                    vec![GodotAction::WriteScript {
                        path: var.script_rel.clone(),
                        source: rewritten,
                    }],
                ),
                "write_script",
            )
        }
        _ => (
            GodotActionBatch::new(
                proposal.label.clone(),
                vec![GodotAction::SetProperty {
                    scene: scene_rel,
                    path: node_path,
                    property,
                    value: literal,
                }],
            ),
            "set_property",
        ),
    };

    let needs_confirm = !proposal.applies_without_asking();
    Some(FastPathPlan {
        batch,
        label: proposal.label,
        confidence_bps: proposal.confidence_bps,
        needs_confirm,
        through,
    })
}

fn tscn_number(value: &TscnValue) -> Option<f64> {
    match value {
        TscnValue::Int(value) => Some(*value as f64),
        TscnValue::Float(value) => Some(*value),
        _ => None,
    }
}

/// Keep the value's Godot type: an integer property stays an integer, a float stays a float.
/// A `.tscn` that suddenly says `health = 3.0` where it said `health = 3` is a diff nobody
/// asked for, and Godot reads the two differently.
fn number_value(next: f64, export: Option<&ExportVar>, scene_had_value: bool) -> TscnValue {
    let float = export.map_or(scene_had_value, |var| var.float_literal);
    if !float && (next - next.round()).abs() < f64::EPSILON {
        #[allow(clippy::cast_possible_truncation)]
        return TscnValue::Int(next.round() as i64);
    }
    TscnValue::Float(next)
}

fn literal_text(value: &TscnValue) -> String {
    value.to_text()
}

/// Rewrite exactly one `@export` line's value, leaving every other byte of the script alone.
///
/// The edit is done here in Rust rather than by a model precisely so the diff is one line: a
/// model asked to "change this number" rewrites the file and reformats four other things
/// along the way.
fn rewrite_export_line(source: &str, var: &ExportVar, literal: String) -> Option<String> {
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines: Vec<String> = source.split(newline).map(str::to_owned).collect();
    let line = lines.get_mut(var.line)?;
    let (code, comment) = split_comment(line);
    let equals = code.rfind('=')?;
    let after = &code[equals + 1..];
    let value_start = equals + 1 + after.len() - after.trim_start().len();
    let value_len = code[value_start..]
        .find(char::is_whitespace)
        .unwrap_or(code.len() - value_start);
    let mut rewritten = String::with_capacity(code.len() + literal.len());
    rewritten.push_str(&code[..value_start]);
    rewritten.push_str(&literal);
    rewritten.push_str(&code[value_start + value_len..]);
    rewritten.push_str(comment);
    *line = rewritten;
    Some(lines.join(newline))
}

/// Split a GDScript line into code and its trailing `#` comment. Quotes are respected so a
/// `"#ff0000"` colour literal is not read as the start of a comment.
fn split_comment(line: &str) -> (&str, &str) {
    let mut in_string: Option<char> = None;
    for (index, ch) in line.char_indices() {
        match (in_string, ch) {
            (Some(quote), ch) if ch == quote => in_string = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => in_string = Some(ch),
            (None, '#') => return (&line[..index], &line[index..]),
            (None, _) => {}
        }
    }
    (line, "")
}

/// Every `@export`ed number in one script, with the line it lives on.
///
/// A small hand-written scanner rather than a regex: the shapes are
/// `@export var name := 5.5`, `@export var name: float = 5.5`, `@export var name = 5`, and
/// annotated forms like `@export_range(0, 10) var speed := 6.0`. Anything else — an export
/// with no default, a non-numeric default, a `const` — is simply not a fast-path knob.
#[must_use]
pub fn scan_exports(source: &str, script_rel: &str) -> Vec<ExportVar> {
    let mut found = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        if found.len() >= MAX_EXPORT_VARS_PER_SCRIPT {
            break;
        }
        let (code, _comment) = split_comment(raw);
        let trimmed = code.trim();
        if !trimmed.starts_with("@export") {
            continue;
        }
        let Some(after_var) = find_var_keyword(trimmed) else {
            continue;
        };
        let name: String = after_var
            .chars()
            .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let Some(equals) = trimmed.rfind('=') else {
            continue;
        };
        let literal = trimmed[equals + 1..].trim();
        let literal = literal.split_whitespace().next().unwrap_or_default();
        let Ok(value) = literal.parse::<f64>() else {
            continue;
        };
        if !value.is_finite() {
            continue;
        }
        found.push(ExportVar {
            name,
            default: value,
            float_literal: literal.contains('.'),
            script_rel: script_rel.to_owned(),
            line: index,
            scene_override: None,
        });
    }
    found
}

/// The text after the ` var ` keyword on an `@export` line.
fn find_var_keyword(line: &str) -> Option<&str> {
    let at = line.find(" var ")?;
    Some(line[at + " var ".len()..].trim_start())
}

#[cfg(test)]
mod tests {
    use super::{
        extract_calls, parse_call, plan_summary, protected_write_refusal, GodotCall,
        GodotCallScanner, GodotWriteResult,
    };
    use bhippi_engine::godot::action::{GodotAction, GodotActionBatch};

    fn scan_all(chunks: &[&str]) -> (String, Vec<GodotCall>) {
        let mut scanner = GodotCallScanner::new();
        let mut text = String::new();
        let mut calls = Vec::new();
        for chunk in chunks {
            let (visible, found) = scanner.push(chunk);
            text.push_str(&visible);
            calls.extend(found);
        }
        text.push_str(&scanner.finish());
        (text, calls)
    }

    #[test]
    fn a_call_split_across_deltas_is_still_found() {
        let (text, calls) = scan_all(&[
            "Adding a coin. <engine_ac",
            "tion>{\"kind\":\"add_to_group\",\"scene\":\"scenes/main.tscn\",",
            "\"path\":\"Coin\",\"group\":\"pickup\"}</engine_",
            "action> Done.",
        ]);
        assert_eq!(calls.len(), 1);
        assert_eq!(text, "Adding a coin.  Done.");
        let batch = parse_call(&calls[0]).expect("a typed action");
        assert_eq!(batch.actions.len(), 1);
        assert_eq!(batch.actions[0].kind(), "add_to_group");
    }

    #[test]
    fn protocol_text_never_reaches_the_visible_answer() {
        let (text, calls) = scan_all(&[
            "Before <engine_batch>{\"label\":\"x\",\"actions\":[]}</engine_batch> after",
        ]);
        assert_eq!(calls.len(), 1);
        assert!(!text.contains("engine_batch"));
        assert!(!text.contains("actions"));
        assert_eq!(text, "Before  after");
    }

    #[test]
    fn a_truncated_call_is_dropped_rather_than_half_applied() {
        let (text, calls) = scan_all(&["Working <engine_batch>{\"label\":\"half"]);
        assert!(calls.is_empty(), "an unterminated call must not be applied");
        assert_eq!(text, "Working ");
    }

    #[test]
    fn text_that_merely_looks_like_a_tag_is_released() {
        let (text, calls) = scan_all(&["I will use the <engine_ helper", " later."]);
        assert!(calls.is_empty());
        assert_eq!(text, "I will use the <engine_ helper later.");
    }

    #[test]
    fn a_malformed_payload_is_answered_with_that_verbs_real_schema() {
        let call = GodotCall::Batch(
            "{\"label\":\"oops\",\"actions\":[{\"kind\":\"add_node\",\"parent\":\".\"}]}"
                .to_owned(),
        );
        let error = parse_call(&call).expect_err("a missing field is a rejection");
        let hint = error.hint.unwrap_or_default();
        assert!(hint.contains("add_node{"), "{hint}");
        assert!(hint.contains("type"), "{hint}");
    }

    #[test]
    fn an_unknown_verb_is_answered_with_the_whole_vocabulary() {
        let call = GodotCall::Action("{\"kind\":\"teleport\",\"path\":\"Player\"}".to_owned());
        let error = parse_call(&call).expect_err("an invented verb is a rejection");
        let hint = error.hint.unwrap_or_default();
        assert!(hint.contains("write_script"), "{hint}");
        assert!(hint.contains("add_node"), "{hint}");
    }

    #[test]
    fn the_plan_summary_leads_with_counts_then_the_files() {
        let batch = GodotActionBatch::new(
            "add three coins",
            vec![
                GodotAction::AddNode {
                    scene: "scenes/main.tscn".to_owned(),
                    parent: ".".to_owned(),
                    name: "Coin".to_owned(),
                    type_: "Area3D".to_owned(),
                    properties: Vec::new(),
                    groups: Vec::new(),
                },
                GodotAction::AddNode {
                    scene: "scenes/main.tscn".to_owned(),
                    parent: ".".to_owned(),
                    name: "Coin2".to_owned(),
                    type_: "Area3D".to_owned(),
                    properties: Vec::new(),
                    groups: Vec::new(),
                },
                GodotAction::AddNode {
                    scene: "scenes/main.tscn".to_owned(),
                    parent: ".".to_owned(),
                    name: "Coin3".to_owned(),
                    type_: "Area3D".to_owned(),
                    properties: Vec::new(),
                    groups: Vec::new(),
                },
                GodotAction::WriteScript {
                    path: "scripts/coin.gd".to_owned(),
                    source: "extends Area3D\n".to_owned(),
                },
            ],
        );
        let summary = plan_summary(&batch);
        assert!(summary.contains("add three coins"), "{summary}");
        assert!(summary.contains("+3 nodes"), "{summary}");
        assert!(summary.contains("1 script"), "{summary}");
        assert!(summary.contains("scenes/main.tscn"), "{summary}");
    }

    #[test]
    fn a_removal_is_counted_as_a_removal() {
        let batch = GodotActionBatch::new(
            "clean up",
            vec![GodotAction::RemoveNode {
                scene: "scenes/main.tscn".to_owned(),
                path: "Coin".to_owned(),
            }],
        );
        assert!(plan_summary(&batch).contains("−1 node"));
    }

    #[test]
    fn a_repair_prompt_carries_the_index_the_message_and_the_schema() {
        let result = GodotWriteResult {
            applied: false,
            label: "add a coin".to_owned(),
            outcomes: Vec::new(),
            changed_files: Vec::new(),
            txn_id: None,
            revision: None,
            failing_index: Some(1),
            message: Some("scripts/coin.gd:4: Parse Error: Expected end of statement".to_owned()),
            hint: Some("The batch was rolled back.".to_owned()),
            schema_hint: bhippi_engine::godot::action::action_schema_hint("write_script"),
        };
        let prompt = super::continuation_prompt(&[], &[result]).expect("a rejection owes a round");
        assert!(prompt.contains("REJECTED"));
        assert!(prompt.contains("Failing action index: 1"));
        assert!(prompt.contains("scripts/coin.gd:4"));
        assert!(prompt.contains("write_script{path,source}"), "{prompt}");
        assert!(prompt.contains("rolled back"));
    }

    #[test]
    fn a_query_alone_continues_the_turn_without_a_repair_notice() {
        let prompt = super::continuation_prompt(
            &[(
                "{\"kind\":\"scene\"}".to_owned(),
                "{\"nodes\":4}".to_owned(),
            )],
            &[],
        )
        .expect("an answered query owes a continuation");
        assert!(prompt.contains("\"nodes\":4"));
        assert!(!prompt.contains("REJECTED"));
    }

    #[test]
    fn an_applied_batch_needs_no_repair_round() {
        let result = GodotWriteResult {
            applied: true,
            label: "ok".to_owned(),
            outcomes: Vec::new(),
            changed_files: vec!["scenes/main.tscn".to_owned()],
            txn_id: Some("t".to_owned()),
            revision: Some(1),
            failing_index: None,
            message: None,
            hint: None,
            schema_hint: None,
        };
        assert!(super::continuation_prompt(&[], &[result]).is_none());
    }

    #[test]
    fn the_transcript_fallback_finds_the_same_calls() {
        let calls = extract_calls(
            "one <engine_query>{\"kind\":\"scene\"}</engine_query> two \
             <engine_batch>{\"label\":\"a\",\"actions\":[]}</engine_batch>",
        );
        assert_eq!(calls.len(), 2);
        assert!(matches!(calls[0], GodotCall::Query(_)));
        assert!(matches!(calls[1], GodotCall::Batch(_)));
    }

    #[test]
    fn the_export_scanner_reads_every_gdscript_shape_and_skips_the_rest() {
        let source = "extends CharacterBody3D\n\
                      \n\
                      @export var jump_velocity := 5.5\n\
                      @export var speed: float = 6.0  # metres per second\n\
                      @export var lives = 3\n\
                      @export_range(0, 10) var glide_time := 3.0\n\
                      @export var label := \"hi\"\n\
                      @export var unset: float\n\
                      const JUMP_VELOCITY := 4.5\n\
                      var runtime := 1.0\n";
        let found = super::scan_exports(source, "scripts/player.gd");
        let names: Vec<&str> = found.iter().map(|var| var.name.as_str()).collect();
        assert_eq!(names, vec!["jump_velocity", "speed", "lives", "glide_time"]);
        assert_eq!(found[0].default, 5.5);
        assert_eq!(found[0].line, 2);
        assert!(found[0].float_literal);
        assert_eq!(found[1].default, 6.0);
        assert_eq!(found[2].default, 3.0);
        assert!(!found[2].float_literal, "`3` is an int literal");
        assert_eq!(found[3].line, 5);
    }

    #[test]
    fn rewriting_an_export_touches_exactly_one_line() {
        let source = "extends CharacterBody3D\n\
                      @export var jump_velocity := 5.5\n\
                      @export var speed: float = 6.0  # metres per second\n\
                      \n\
                      func _ready() -> void:\n\
                      \tpass\n";
        let vars = super::scan_exports(source, "scripts/player.gd");
        let rewritten = super::rewrite_export_line(source, &vars[0], "6.6".to_owned())
            .expect("the line rewrites");
        let before: Vec<&str> = source.lines().collect();
        let after: Vec<&str> = rewritten.lines().collect();
        assert_eq!(before.len(), after.len());
        let differing: Vec<usize> = before
            .iter()
            .zip(&after)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(differing, vec![1], "only the @export line may move");
        assert_eq!(after[1], "@export var jump_velocity := 6.6");

        // A trailing comment survives, and the value is the only thing replaced.
        let with_comment = super::rewrite_export_line(source, &vars[1], "7.2".to_owned())
            .expect("the annotated line rewrites");
        assert!(with_comment.contains("@export var speed: float = 7.2  # metres per second"));
    }

    #[test]
    fn a_hash_inside_a_string_is_not_a_comment() {
        let (code, comment) = super::split_comment("@export var tint := \"#ff0000\"  # red");
        assert_eq!(code, "@export var tint := \"#ff0000\"  ");
        assert_eq!(comment, "# red");
    }

    #[test]
    fn a_godot_project_file_is_refused_to_the_generic_file_tool() {
        let root =
            std::env::temp_dir().join(format!("bhippi-inv088-{}", bhippi_types::SessionId::new()));
        std::fs::create_dir_all(&root).expect("temp project");
        std::fs::write(root.join("project.godot"), "config_version=5\n").expect("marker");

        for path in [
            "scenes/main.tscn",
            "project.godot",
            "scripts/player.gd",
            "assets/mat.tres",
            "export_presets.cfg",
        ] {
            let refusal = protected_write_refusal(&root, path)
                .unwrap_or_else(|| panic!("{path} must be refused"));
            assert!(refusal.message.contains("INV-088"), "{path}");
            assert!(refusal
                .hint
                .as_deref()
                .unwrap_or_default()
                .contains("<engine_batch>"));
        }
        assert!(protected_write_refusal(&root, "README.md").is_none());
        assert!(protected_write_refusal(&root, "src/main.rs").is_none());
        // The same names outside a Godot project are just files.
        let plain = root.join("plain");
        std::fs::create_dir_all(&plain).expect("plain dir");
        assert!(protected_write_refusal(&plain, "scenes/main.tscn").is_none());

        let _ignored = std::fs::remove_dir_all(&root);
    }

    const ENGINE_PROMPT: &str = include_str!("../../../prompts/chat-engine.md");

    #[test]
    fn prompt_v10_lists_every_godot_action_verb() {
        let verbs = [
            "add_node",
            "remove_node",
            "rename_node",
            "reparent_node",
            "instance_scene",
            "create_scene",
            "connect_signal",
            "set_property",
            "remove_property",
            "add_to_group",
            "write_script",
            "attach_script",
            "delete_script",
            "set_main_scene",
            "add_autoload",
            "add_input_action",
        ];

        for verb in verbs {
            assert!(
                ENGINE_PROMPT.contains(verb),
                "prompts/chat-engine.md v10 does not mention the Godot verb `{verb}`"
            );
        }
    }

    #[test]
    fn prompt_v10_lists_every_query_kind() {
        for kind in super::QUERY_KINDS {
            assert!(
                ENGINE_PROMPT.contains(&format!("\"kind\":\"{kind}\"")),
                "prompts/chat-engine.md does not document query kind `{kind}`"
            );
        }
    }

    #[test]
    fn archetype_packs_map_to_valid_godot_nodes_and_properties_gad_094() {
        let packs = bhippi_engine::intent::archetype::builtin();
        assert_eq!(packs.len(), 10, "exactly 10 archetype packs must exist");

        let presets = bhippi_engine::intent::catalog::presets();
        for pack in packs {
            assert!(
                presets.iter().any(|p| p.id == pack.player),
                "pack {} names unknown player preset {}",
                pack.id,
                pack.player
            );
            assert!(
                presets.iter().any(|p| p.id == pack.camera),
                "pack {} names unknown camera preset {}",
                pack.id,
                pack.camera
            );
            assert!(
                presets.iter().any(|p| p.id == pack.level),
                "pack {} names unknown level preset {}",
                pack.id,
                pack.level
            );
            assert!(
                presets.iter().any(|p| p.id == pack.hud),
                "pack {} names unknown hud preset {}",
                pack.id,
                pack.hud
            );
            for req in &pack.required {
                assert!(
                    presets.iter().any(|p| p.id == *req),
                    "pack {} names unknown required preset {}",
                    pack.id,
                    req
                );
            }
        }
    }

    #[test]
    fn offline_golden_chat_bridge_round_trip_gad_093() {
        let temp = std::env::temp_dir().join(format!("bhippi-g3-golden-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&temp).unwrap();

        bhippi_engine::godot::scaffold::write_project(
            &temp,
            "GoldenGame",
            bhippi_engine::godot::scaffold::ProjectTemplate::Empty3D,
            false,
        )
        .unwrap();

        // 1. Model streams out an <engine_query>
        let stream_query = "Let's inspect the scene first.\n\
                            <engine_query>{\"kind\":\"scene\"}</engine_query>\n\
                            Waiting for scene digest...";
        let mut scanner = GodotCallScanner::new();
        let (visible, calls) = scanner.push(stream_query);
        assert_eq!(calls.len(), 1);
        assert!(!visible.contains("<engine_query>"));
        assert!(matches!(calls[0], GodotCall::Query(_)));

        // 2. Model streams out an <engine_batch> adding a Coin and writing a checked script
        let batch_json = r#"{
            "label": "add coin pickup with probe tracking",
            "actions": [
                {
                    "kind": "add_node",
                    "scene": "scenes/main.tscn",
                    "parent": ".",
                    "name": "Coin",
                    "type": "Area3D",
                    "properties": [["position", {"Vector3": [0.0, 1.0, 2.0]}]]
                },
                {
                    "kind": "write_script",
                    "path": "scripts/coin.gd",
                    "source": "extends Area3D\n\n@onready var _probe: Node = get_node_or_null(\"/root/BhippiProbe\")\n\nfunc _ready() -> void:\n\tpass\n"
                },
                {
                    "kind": "attach_script",
                    "scene": "scenes/main.tscn",
                    "path": "Coin",
                    "script_res_path": "res://scripts/coin.gd"
                }
            ]
        }"#;

        let stream_batch = format!(
            "Applying the coin batch:\n<engine_batch>{batch_json}</engine_batch>\nBatch applied."
        );
        let mut batch_scanner = GodotCallScanner::new();
        let (b_vis, b_calls) = batch_scanner.push(&stream_batch);
        assert_eq!(b_calls.len(), 1);
        assert!(!b_vis.contains("<engine_batch>"));
        assert!(matches!(b_calls[0], GodotCall::Batch(_)));

        let parsed_batch = super::parse_call(&b_calls[0]).unwrap();
        assert_eq!(parsed_batch.actions.len(), 3);
        assert_eq!(parsed_batch.label, "add coin pickup with probe tracking");

        assert!(matches!(
            parsed_batch.actions[0],
            GodotAction::AddNode { .. }
        ));
        assert!(matches!(
            parsed_batch.actions[1],
            GodotAction::WriteScript { .. }
        ));
        assert!(matches!(
            parsed_batch.actions[2],
            GodotAction::AttachScript { .. }
        ));

        let plan = super::plan_summary(&parsed_batch);
        assert_eq!(
            plan,
            "add coin pickup with probe tracking — +1 node · 1 script · 1 edit · scenes/main.tscn · scripts/coin.gd"
        );

        let _ = std::fs::remove_dir_all(&temp);
    }
}
