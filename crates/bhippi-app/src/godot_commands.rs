//! The IPC surface over Godot 4 (ADR-0043, GAD-075…084).
//!
//! `bhippi_engine::godot` is a pure library: it parses, models, lowers typed actions to file
//! changes and *describes* commands. `crate::godot` runs those commands. This module is the
//! seam between them and the webview — one place where a project path is validated, a
//! process is started at most once per project, output is coalesced into batched events, and
//! every file write goes through `lower` → `apply_changeset` → the journal (INV-070/071/088).
//!
//! Three rules shape everything below.
//!
//! **A command never touches a path the user has not registered.** Every entry point takes a
//! `project` string from the webview and resolves it through [`resolve_project`], which
//! canonicalises it and refuses anything that is not in `config.workspace.projects`. A path
//! that arrives from a model, a stale UI state or a crafted IPC call fails there.
//!
//! **A script reaches disk only if Godot can parse it.** `godot_apply_batch` writes the
//! change set, runs `--check-only` over every `.gd` it wrote, and on a failure applies the
//! inverse before returning — so a batch that does not compile leaves the project exactly as
//! it found it, and the error carries `file:line` parsed out of Godot's own stderr.
//!
//! **Output is batched, never per-line.** A Godot run prints hundreds of lines a second; the
//! event bus allows twenty (INV-076). Lines go through an unbounded channel into a pump that
//! emits at most one [`GodotOutput`] every [`GODOT_OUTPUT_FLUSH`], or sooner when
//! [`GODOT_OUTPUT_BATCH`] lines have piled up.

use crate::commands::AppError;
use crate::godot::{
    capture, detect_godot, require_godot, run_spec_with_stop, stop_channel, GodotExit,
    GodotOutputLine, GodotProcessHandle,
};
use crate::godot_observe::{VisualPlaytestPlan, VisualPlaytestResult};
use crate::godot_preview::PreviewServer;
use bhippi_engine::godot::action::{
    apply_changeset, invert, GodotActionBatch, GodotActionOutcome, GodotChangeSet,
};
use bhippi_engine::godot::command::{
    check_script_command, editor_command, export_command, playtest_command, run_command, RunOptions,
};
use bhippi_engine::godot::detect::{
    describe_install_offer, export_templates_installed, version_command_for, GodotInstall,
    InstallOffer,
};
use bhippi_engine::godot::export_presets::{
    default_presets, ExportPresets, PresetTarget, WEB_EXPORT_PATH, WINDOWS_EXPORT_DIR,
};
use bhippi_engine::godot::gates::GateReport;
use bhippi_engine::godot::probe::{PlaytestInputs, PlaytestStep, TelemetryReport};
use bhippi_engine::godot::scaffold::ProjectTemplate;
use bhippi_engine::godot::scene::{GodotScene, NodeView, SCENE_DIGEST_MAX_NODES};
use bhippi_engine::godot::{gates, res_to_rel, scaffold};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri_specta::Event;

// ── limits ───────────────────────────────────────────────────────────────────────────

/// How many output lines one project's ring buffer keeps. Beyond this the oldest go: a
/// headless export prints tens of thousands of import lines and none of the early ones is
/// what anybody scrolls back for.
pub const GODOT_OUTPUT_CAP: usize = 2_000;
/// The most lines one [`GodotOutput`] event carries.
pub const GODOT_OUTPUT_BATCH: usize = 50;
/// How often the output pump flushes whatever it has. Ten events a second is half the bus's
/// budget (INV-076) and still reads as live.
pub const GODOT_OUTPUT_FLUSH: Duration = Duration::from_millis(100);
/// The floor between two batches when [`GODOT_OUTPUT_BATCH`] lines pile up before the next
/// tick. Fifty milliseconds is exactly twenty events a second — the bus's whole budget and
/// not a line more (INV-076).
pub const GODOT_OUTPUT_MIN_GAP: Duration = Duration::from_millis(50);
/// How many trailing lines a one-shot command reports back to the caller.
pub const GODOT_LOG_TAIL: usize = 40;
/// The most scenes `godot_list_scenes` returns. The gates' own walker stops at
/// `MAX_SCANNED_FILES`; this is the second cap, on what crosses IPC.
pub const MAX_LISTED_SCENES: usize = 500;
/// The frame count the pane's Playtest button asks for when nobody said otherwise: three
/// seconds at 60 fps, long enough for a jump arc to finish.
pub const DEFAULT_PLAYTEST_FRAMES: u32 = 180;

// ── session state ────────────────────────────────────────────────────────────────────

/// What a running Godot process was started for.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GodotRunKind {
    /// A windowed game.
    Run,
    /// A headless deterministic replay.
    Playtest,
    /// A windowed run Bhippi watches and plays through Computer Use (ADR-0044).
    VisualPlaytest,
    /// A headless export.
    Export,
    /// The Godot editor itself.
    Editor,
}

impl GodotRunKind {
    /// What to say when a second run is refused.
    #[must_use]
    pub fn busy_message(self) -> &'static str {
        match self {
            Self::Run => "the game is already running",
            Self::Playtest => "a playtest is already running",
            Self::VisualPlaytest => "Bhippi is already watching this game play",
            Self::Export => "an export is already running",
            Self::Editor => "the Godot editor is already open on this project",
        }
    }
}

/// Where a run is in its life.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GodotRunState {
    Starting,
    Running,
    Exited,
}

/// The process one project currently owns.
#[derive(Debug)]
pub struct RunningProcess {
    pub handle: GodotProcessHandle,
    pub started_at: Instant,
    pub kind: GodotRunKind,
}

/// Everything Bhippi remembers about one Godot project between calls.
///
/// None of it is authoritative — the project on disk is — but re-detecting Godot, re-parsing
/// every scene and re-reading the last telemetry file on every status poll would make the
/// pane feel like a web page from 1999.
#[derive(Debug, Default)]
pub struct GodotSession {
    /// The detected install, cached until something asks for a re-detect.
    pub install: Option<GodotInstall>,
    /// The scene the pane is looking at.
    pub open_scene: Option<String>,
    /// The run, playtest or export this project owns. At most one at a time: they all write
    /// to the same import cache, and two Godots racing over `.godot/` is how a project
    /// ends up re-importing every asset on the next open.
    pub running: Option<RunningProcess>,
    /// The Godot editor, tracked apart from `running` on purpose. It has no timeout and
    /// ends when the person closes the window, so counting it as the project's one process
    /// would lock Play, Playtest and Export out for as long as the editor stayed open —
    /// which is exactly the session where someone is most likely to want all three.
    pub editor: Option<RunningProcess>,
    pub output: std::collections::VecDeque<GodotOutputLine>,
    /// Monotonic per project, so the webview can tell a dropped batch from a quiet one.
    pub output_seq: u32,
    pub last_playtest: Option<TelemetryReport>,
    pub last_gates: Option<GateReport>,
    pub preview: Option<PreviewServer>,
}

impl GodotSession {
    /// Append one line, dropping the oldest once the buffer is full.
    pub fn push_line(&mut self, line: GodotOutputLine) {
        if self.output.len() >= GODOT_OUTPUT_CAP {
            self.output.pop_front();
        }
        self.output.push_back(line);
    }

    /// True when a process is live. A handle whose run already ended is stale bookkeeping,
    /// not a busy project — asking the handle is what tells the two apart.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|running| !running.handle.is_stopped())
    }

    #[must_use]
    pub fn running_kind(&self) -> Option<GodotRunKind> {
        self.running.as_ref().map(|running| running.kind)
    }
}

/// One session per project root, keyed by its display path.
#[derive(Debug, Default)]
pub struct GodotSessions {
    by_project: BTreeMap<String, GodotSession>,
}

impl GodotSessions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entry(&mut self, project: &str) -> &mut GodotSession {
        self.by_project.entry(project.to_owned()).or_default()
    }

    #[must_use]
    pub fn get(&self, project: &str) -> Option<&GodotSession> {
        self.by_project.get(project)
    }

    #[must_use]
    pub fn project_count(&self) -> usize {
        self.by_project.len()
    }

    /// Stop everything this store started. Called on the way out of the app so a headless
    /// export does not outlive the window that asked for it.
    pub fn shutdown(&mut self) {
        for session in self.by_project.values_mut() {
            for process in [&session.running, &session.editor].into_iter().flatten() {
                process.handle.kill();
            }
            if let Some(preview) = &session.preview {
                preview.stop();
            }
        }
    }
}

/// The managed handle. A plain `std::sync::Mutex`: every critical section is a map lookup
/// and a push, and the process work happens with the guard dropped.
pub type GodotSessionStore = Arc<Mutex<GodotSessions>>;

pub(crate) fn lock(
    store: &GodotSessionStore,
) -> Result<std::sync::MutexGuard<'_, GodotSessions>, AppError> {
    store.lock().map_err(|_| AppError {
        message: "The Godot session store is poisoned.".to_owned(),
        hint: Some("Restart the app; an earlier Godot call panicked.".to_owned()),
    })
}

// ── events ───────────────────────────────────────────────────────────────────────────

/// A batch of engine output. Never one line per event (INV-076).
#[derive(Clone, Debug, Deserialize, Serialize, Type, Event)]
pub struct GodotOutput {
    pub project: String,
    pub lines: Vec<GodotOutputLine>,
    /// Increments once per emitted batch for this project.
    pub seq: u32,
}

/// A run starting, running or ending.
#[derive(Clone, Debug, Deserialize, Serialize, Type, Event)]
pub struct GodotProcessState {
    pub project: String,
    pub kind: GodotRunKind,
    pub state: GodotRunState,
    /// Present only on `exited`.
    pub exit: Option<GodotExit>,
}

/// One applied, journaled batch of Godot file edits.
#[derive(Clone, Debug, Deserialize, Serialize, Type, Event)]
pub struct GodotSceneChanged {
    pub project: String,
    /// The scene the batch touched, when it touched exactly one.
    pub scene_rel: Option<String>,
    pub txn_id: String,
    /// `user` | `agent`
    pub actor: String,
    pub label: String,
    /// Project-relative, forward slashes.
    pub changed_files: Vec<String>,
}

// ── typed replies ────────────────────────────────────────────────────────────────────

/// A run in flight, for the toolbar's Play/Stop state.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RunningInfo {
    pub kind: GodotRunKind,
    pub running_ms: u64,
}

/// Everything the pane needs to decide what to show before anything is clicked.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GodotStatus {
    /// `None` is the missing-Godot state (GAD-082), not an error.
    pub install: Option<GodotInstall>,
    /// What the app may offer to install. Always present: the pane shows it in the
    /// missing-Godot state and in Settings.
    pub offer: InstallOffer,
    /// Export templates for the detected version. Exporting without them fails with a
    /// message about presets, which is the wrong thing to send someone looking for.
    pub templates_installed: bool,
    pub is_godot_project: bool,
    pub manifest_main_scene: Option<String>,
    pub running: Option<RunningInfo>,
    pub preview_url: Option<String>,
    /// How the install was found, in words, for the Settings row.
    pub install_source: Option<String>,
}

/// One row of the Outliner. Flat with a depth, because the tree the webview draws is the one
/// Rust resolved; the pane owns only which rows are collapsed.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SceneTreeNode {
    /// `"."` for the root, otherwise `"Player/Mesh"`.
    pub path: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub groups: Vec<String>,
    /// The `res://` path of the attached script, when there is one.
    pub script: Option<String>,
    pub depth: usize,
    pub has_children: bool,
}

/// A whole scene, resolved.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SceneTreeView {
    pub scene_rel: String,
    /// The root's path, which is always `"."` for a well-formed scene.
    pub root: Option<String>,
    pub nodes: Vec<SceneTreeNode>,
    /// The indented text digest the prompt and the pane's summary share.
    pub digest: String,
    pub node_count: usize,
    /// True when the scene has more nodes than [`SCENE_DIGEST_MAX_NODES`].
    pub truncated: bool,
}

/// What one applied batch did.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GodotBatchResult {
    pub outcomes: Vec<GodotActionOutcome>,
    pub txn_id: String,
    /// Project-relative, forward slashes.
    pub changed_files: Vec<String>,
    /// Scripts this batch wrote and Godot has now parsed.
    pub needs_check: Vec<String>,
    pub label: String,
    /// The journal revision, or `None` when the ledger was unavailable.
    pub revision: Option<i64>,
}

/// One thing wrong with the project, as it crosses IPC.
///
/// A mirror of [`gates::Finding`] because the pre-Godot engine has a type of the same name
/// and the generated bindings are one flat namespace: two `Finding`s would silently become
/// whichever one specta saw last. The `Godot` prefix is what keeps the two apart in
/// `ipc.ts`, and it also reads correctly on the UI side, where both still exist.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GodotFinding {
    /// A stable `BHP-GD-4xx` code.
    pub code: String,
    pub message: String,
    pub hint: String,
    /// The file (or setting) the finding is about, project-relative.
    pub where_: String,
}

/// What the gates found. Blockers stop a release export (INV-074); warnings do not.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GodotGateReport {
    pub blockers: Vec<GodotFinding>,
    pub warnings: Vec<GodotFinding>,
    /// Computed here, not in the webview: `blockers.length === 0` is a rule, and rules live
    /// in Rust (INV-073).
    pub passes: bool,
}

impl From<&GateReport> for GodotGateReport {
    fn from(report: &GateReport) -> Self {
        let finding = |finding: &gates::Finding| GodotFinding {
            code: finding.code.clone(),
            message: finding.message.clone(),
            hint: finding.hint.clone(),
            where_: finding.where_.clone(),
        };
        Self {
            blockers: report.blockers.iter().map(finding).collect(),
            warnings: report.warnings.iter().map(finding).collect(),
            passes: report.passes(),
        }
    }
}

/// One scripted input, as it crosses IPC.
///
/// A mirror of [`PlaytestStep`] rather than the type itself. The engine's version uses
/// `skip_serializing_if` so the JSON the probe reads carries only the field that applies,
/// and a *conditionally absent* field has no single TypeScript type — specta refuses to
/// generate one, correctly. Here both fields are always present and `null` means "not this
/// kind of step"; [`PlaytestScript::to_inputs`] converts to the shape the probe wants.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PlaytestScriptStep {
    pub frame: u32,
    /// An input action name from `project.godot`'s `[input]` section.
    pub action: Option<String>,
    /// A `KEY_*` name. Exactly one of `action` and `key` may be set.
    pub key: Option<String>,
    pub pressed: bool,
}

/// A whole scripted playtest, as it crosses IPC.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PlaytestScript {
    /// Sample every *n* frames; `null` uses the probe's default.
    pub sample_every: Option<u32>,
    pub steps: Vec<PlaytestScriptStep>,
}

impl PlaytestScript {
    /// The probe's own shape. Validation stays in the engine crate: this only re-arranges.
    #[must_use]
    pub fn to_inputs(&self) -> PlaytestInputs {
        let mut inputs = PlaytestInputs::new(
            self.steps
                .iter()
                .map(|step| PlaytestStep {
                    frame: step.frame,
                    action: step.action.clone(),
                    key: step.key.clone(),
                    pressed: step.pressed,
                })
                .collect(),
        );
        inputs.sample_every = self.sample_every;
        inputs
    }

    /// The reverse, so the pane can show what it is about to replay.
    #[must_use]
    pub fn from_inputs(inputs: &PlaytestInputs) -> Self {
        Self {
            sample_every: inputs.sample_every,
            steps: inputs
                .steps
                .iter()
                .map(|step| PlaytestScriptStep {
                    frame: step.frame,
                    action: step.action.clone(),
                    key: step.key.clone(),
                    pressed: step.pressed,
                })
                .collect(),
        }
    }
}

/// A finished playtest.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PlaytestResult {
    pub report: TelemetryReport,
    pub exit: GodotExit,
    pub telemetry_path: String,
    pub log_tail: Vec<String>,
}

/// A finished export.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ExportResult {
    pub output_path: String,
    pub ok: bool,
    pub log_tail: Vec<String>,
    /// The version this export recorded (GAD-094). `None` when the export failed, or when
    /// the version list could not be written — an artefact that exists is still an artefact.
    pub version_id: Option<String>,
}

/// One located GDScript fault, parsed out of Godot's stderr.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ScriptFault {
    /// Project-relative, forward slashes. Empty when Godot did not name a file.
    pub file: String,
    /// 1-based; `0` when Godot printed a message with no location.
    pub line: u32,
    pub message: String,
}

impl ScriptFault {
    /// `scripts/player.gd:12: Parse Error: …` — the shape an editor jumps from.
    #[must_use]
    pub fn to_message(&self) -> String {
        if self.file.is_empty() {
            self.message.clone()
        } else if self.line == 0 {
            format!("{}: {}", self.file, self.message)
        } else {
            format!("{}:{}: {}", self.file, self.line, self.message)
        }
    }
}

// ── the GDScript error parser ────────────────────────────────────────────────────────

/// Pull located faults out of what `--check-only` printed.
///
/// Godot 4 reports a script error in at least four shapes, and which one you get depends on
/// the version, whether the parse failed or the *load* failed, and whether the message came
/// from the parser or from the resource loader:
///
/// ```text
/// SCRIPT ERROR: Parse Error: Expected end of statement after expression, found "Identifier" instead.
///           at: GDScript::reload (res://scripts/player.gd:12)
///
/// res://scripts/player.gd:7 - Parse Error: Identifier "speedd" not declared in the current scope.
///
/// ERROR: res://scripts/enemy.gd:3 - Parse Error: Expected statement, found ":" instead.
///    at: (core/object/script_language.cpp:87)
///
/// SCRIPT ERROR: Compile Error:
///           at: GDScript::reload (res://scripts/hud.gd:20)
///
/// ERROR: Failed to load script "res://scripts/boss.gd" with error "Parse error".
/// ```
///
/// The `at:` continuation is the important one: the message and the location are on
/// *different lines*, so a parser that reads line by line and never looks back finds a
/// message with no file or a file with no message. This one carries the last message
/// forward until a location claims it.
///
/// A location inside Godot's own C++ (`core/object/script_language.cpp:87`) is not a fault
/// in the user's project and is skipped — pointing someone at a file they do not have is
/// worse than pointing them at nothing.
#[must_use]
pub fn parse_script_faults(output: &str) -> Vec<ScriptFault> {
    let mut faults: Vec<ScriptFault> = Vec::new();
    let mut pending: Option<String> = None;

    for raw in output.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        // `at: … (res://path.gd:12)` — the location for whatever message came before it.
        if let Some(rest) = line.strip_prefix("at:") {
            if let Some((file, number)) = location_in_parentheses(rest) {
                let message = pending
                    .take()
                    .unwrap_or_else(|| "GDScript reported an error".to_owned());
                push_fault(&mut faults, file, number, message);
            }
            continue;
        }

        let body = strip_severity(line);

        // `res://path.gd:12 - Message`
        if let Some(fault) = leading_location(body) {
            pending = None;
            push_fault(&mut faults, fault.0, fault.1, fault.2);
            continue;
        }

        // `Failed to load script "res://path.gd" with error "Parse error".`
        if let Some(fault) = failed_to_load(body) {
            pending = None;
            push_fault(&mut faults, fault.0, 0, fault.1);
            continue;
        }

        // A message that has not found its location yet. Only messages Godot marks as
        // errors are carried: an ordinary print line must not become a fault.
        if line.starts_with("SCRIPT ERROR:")
            || line.starts_with("ERROR:")
            || line.starts_with("USER ERROR:")
            || line.starts_with("USER SCRIPT ERROR:")
        {
            let message = body.trim().trim_end_matches(':').trim().to_owned();
            pending = Some(if message.is_empty() {
                "GDScript reported an error".to_owned()
            } else {
                message
            });
        }
    }

    faults
}

/// The first fault that names a real project file, preferring one in `script_rel`.
#[must_use]
pub fn first_script_fault(output: &str, script_rel: &str) -> Option<ScriptFault> {
    let faults = parse_script_faults(output);
    faults
        .iter()
        .find(|fault| fault.file == script_rel)
        .or_else(|| faults.first())
        .cloned()
}

fn push_fault(into: &mut Vec<ScriptFault>, file: String, line: u32, message: String) {
    let fault = ScriptFault {
        file,
        line,
        message,
    };
    if !into.contains(&fault) {
        into.push(fault);
    }
}

/// Drop `ERROR:` / `SCRIPT ERROR:` / `USER ERROR:` and friends from the front of a line.
fn strip_severity(line: &str) -> &str {
    for prefix in [
        "USER SCRIPT ERROR:",
        "USER ERROR:",
        "SCRIPT ERROR:",
        "ERROR:",
        "WARNING:",
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return rest.trim_start();
        }
    }
    line
}

/// `GDScript::reload (res://scripts/player.gd:12)` → `("scripts/player.gd", 12)`.
fn location_in_parentheses(text: &str) -> Option<(String, u32)> {
    let open = text.rfind('(')?;
    let close = text[open..].find(')')? + open;
    let inside = text.get(open + 1..close)?;
    let (path, number) = inside.rsplit_once(':')?;
    let line: u32 = number.trim().parse().ok()?;
    project_relative(path).map(|path| (path, line))
}

/// `res://scripts/player.gd:7 - Parse Error: …`
fn leading_location(text: &str) -> Option<(String, u32, String)> {
    let (location, message) = text.split_once(" - ")?;
    let (path, number) = location.trim().rsplit_once(':')?;
    let line: u32 = number.trim().parse().ok()?;
    let path = project_relative(path)?;
    Some((path, line, message.trim().to_owned()))
}

/// `Failed to load script "res://scripts/boss.gd" with error "Parse error".`
fn failed_to_load(text: &str) -> Option<(String, String)> {
    if !text.starts_with("Failed to load script") {
        return None;
    }
    let start = text.find('"')? + 1;
    let end = text[start..].find('"')? + start;
    let path = project_relative(text.get(start..end)?)?;
    Some((path, text.trim().to_owned()))
}

/// Only a `res://` path is the user's. Anything else is Godot's own source and is dropped.
fn project_relative(path: &str) -> Option<String> {
    let path = path.trim();
    if !path.starts_with("res://") {
        return None;
    }
    let relative = res_to_rel(path);
    (!relative.is_empty()).then_some(relative)
}

// ── project resolution ───────────────────────────────────────────────────────────────

pub(crate) fn engine_error(error: bhippi_engine::EngineError) -> AppError {
    AppError {
        message: error.to_string(),
        hint: error.hint().map(str::to_owned),
    }
}

/// Canonicalise a project path from the webview and refuse anything not registered.
///
/// This is the only door. Every command below goes through it before it reads a byte, which
/// is what makes "a Godot command cannot touch a folder the user did not add" a property of
/// the module rather than of each command's author.
pub(crate) async fn resolve_project(
    state: &crate::Runtime,
    project: &str,
) -> Result<PathBuf, AppError> {
    let canonical = crate::workspace::canonical_directory(project)?;
    let display = crate::workspace::display_path(&canonical);
    let config = state.config.load().await.map_err(AppError::from)?;
    let registered = config
        .workspace
        .projects
        .iter()
        .any(|record| crate::workspace::paths_match(&record.path, &display));
    if !registered {
        return Err(AppError {
            message: format!("{display} is not a registered project."),
            hint: Some("Add the folder from the sidebar before using the Godot pane.".to_owned()),
        });
    }
    Ok(canonical)
}

/// The configured `[godot] path`, when the user has chosen one.
async fn configured_godot(state: &crate::Runtime) -> Option<PathBuf> {
    let config = state.config.load().await.ok()?;
    let path = config.godot.path?;
    let trimmed = path.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

/// The install for one project, detected once and cached in its session.
async fn install_for(
    state: &crate::Runtime,
    store: &GodotSessionStore,
    project: &str,
) -> Option<GodotInstall> {
    if let Some(cached) = lock(store)
        .ok()
        .and_then(|sessions| sessions.get(project).and_then(|s| s.install.clone()))
    {
        return Some(cached);
    }
    let configured = configured_godot(state).await;
    let found = detect_godot(configured.as_deref()).await?;
    if let Ok(mut sessions) = lock(store) {
        sessions.entry(project).install = Some(found.clone());
    }
    Some(found)
}

/// The install to drive one project's `--check-only`, resolved from whatever the caller has.
///
/// With an app handle this is exactly [`require_install`] — the same cached, config-aware
/// lookup a command handler gets. Without one (a test, the headless CLI) it falls back to
/// plain detection, so a batch applied off the UI thread still gets its scripts checked
/// rather than silently skipping the gate.
async fn install_for_host(host: GodotApplyHost<'_>, root: &Path) -> Result<GodotInstall, AppError> {
    if let Some(app) = host.app {
        use tauri::Manager as _;
        if let (Some(state), Some(store)) = (
            app.try_state::<crate::Runtime>(),
            app.try_state::<GodotSessionStore>(),
        ) {
            return require_install(&state, store.inner(), &display_of(root)).await;
        }
    }
    require_godot(None).await
}

/// Detection as a typed failure, cached like [`install_for`].
pub(crate) async fn require_install(
    state: &crate::Runtime,
    store: &GodotSessionStore,
    project: &str,
) -> Result<GodotInstall, AppError> {
    match install_for(state, store, project).await {
        Some(install) => Ok(install),
        None => {
            let configured = configured_godot(state).await;
            require_godot(configured.as_deref()).await
        }
    }
}

pub(crate) fn display_of(path: &Path) -> String {
    crate::workspace::display_path(path)
}

fn manifest_main_scene(root: &Path) -> Option<String> {
    let manifest = bhippi_engine::manifest::load_manifest(root)
        .ok()
        .flatten()?;
    manifest
        .godot
        .as_ref()
        .map(|section| section.main_scene.clone())
        .or(Some(manifest.game.default_scene.clone()))
        .filter(|scene| !scene.is_empty())
}

/// The name Godot puts in the game window's title bar, which is `config/name` and therefore
/// the manifest's game name. `godot_observe` needs it to find the window it just launched.
pub(crate) fn game_name(root: &Path) -> String {
    bhippi_engine::manifest::load_manifest(root)
        .ok()
        .flatten()
        .map(|manifest| manifest.game.name)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| {
            root.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "Game".to_owned())
        })
}

// ── output pump ──────────────────────────────────────────────────────────────────────

/// Start the coalescing pump and hand back the sender the run's line callback writes to.
///
/// The pump owns the ring buffer and the sequence number, so a run does not have to take the
/// session lock once per line — which at a few thousand lines a second is the difference
/// between a live log and a stalled UI thread.
///
/// A batch leaves on the tick, or as soon as [`GODOT_OUTPUT_BATCH`] lines have piled up and
/// the previous batch is at least [`GODOT_OUTPUT_MIN_GAP`] old. The second half of that
/// condition is what keeps the bus rule (INV-076): without it a Godot export printing
/// thousands of import lines a second would trip the size rule forty times a second, which
/// is a batched emitter that has quietly stopped batching. The waiting cost is bounded by
/// [`GODOT_OUTPUT_CAP`] — a runaway `print` loop drops its oldest lines here exactly as it
/// would in the ring buffer, rather than growing a queue until the app dies.
pub(crate) fn start_output_pump(
    app: tauri::AppHandle,
    store: GodotSessionStore,
    project: String,
) -> tokio::sync::mpsc::UnboundedSender<GodotOutputLine> {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<GodotOutputLine>();
    tauri::async_runtime::spawn(async move {
        let mut pending: Vec<GodotOutputLine> = Vec::new();
        let mut last_flush = Instant::now();
        let mut ticker = tokio::time::interval(GODOT_OUTPUT_FLUSH);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                line = receiver.recv() => match line {
                    Some(line) => {
                        if pending.len() >= GODOT_OUTPUT_CAP {
                            pending.remove(0);
                        }
                        pending.push(line);
                        if should_flush_early(pending.len(), last_flush.elapsed()) {
                            flush_output(&app, &store, &project, &mut pending);
                            last_flush = Instant::now();
                        }
                    }
                    // The run ended: whatever is left is still real output, and the last
                    // lines of a failed run are the ones worth reading.
                    None => {
                        flush_output(&app, &store, &project, &mut pending);
                        break;
                    }
                },
                _ = ticker.tick() => {
                    flush_output(&app, &store, &project, &mut pending);
                    last_flush = Instant::now();
                }
            }
        }
    });
    sender
}

/// Whether a full batch may leave before the next tick.
///
/// Pulled out of the pump because it is the whole of the coalescing rule and the pump around
/// it needs a Tauri handle to test. Both halves matter: the size is what makes a burst feel
/// live, and the gap is what stops the size from turning a batched emitter back into a
/// per-line one.
#[must_use]
fn should_flush_early(pending: usize, since_last: Duration) -> bool {
    pending >= GODOT_OUTPUT_BATCH && since_last >= GODOT_OUTPUT_MIN_GAP
}

fn flush_output(
    app: &tauri::AppHandle,
    store: &GodotSessionStore,
    project: &str,
    pending: &mut Vec<GodotOutputLine>,
) {
    if pending.is_empty() {
        return;
    }
    let batch: Vec<GodotOutputLine> = std::mem::take(pending);
    let seq = match lock(store) {
        Ok(mut sessions) => {
            let session = sessions.entry(project);
            for line in &batch {
                session.push_line(line.clone());
            }
            session.output_seq = session.output_seq.wrapping_add(1);
            session.output_seq
        }
        Err(_) => 0,
    };
    let _ignored = GodotOutput {
        project: project.to_owned(),
        lines: batch,
        seq,
    }
    .emit(app);
}

pub(crate) fn announce_process(
    app: &tauri::AppHandle,
    project: &str,
    kind: GodotRunKind,
    state: GodotRunState,
    exit: Option<GodotExit>,
) {
    let _ignored = GodotProcessState {
        project: project.to_owned(),
        kind,
        state,
        exit,
    }
    .emit(app);
}

/// Refuse a second process for a project that already has one.
///
/// The editor has its own slot, so opening Godot never blocks a Play and a Play never
/// blocks opening Godot.
pub(crate) fn claim_slot(
    store: &GodotSessionStore,
    project: &str,
    kind: GodotRunKind,
    handle: GodotProcessHandle,
) -> Result<(), AppError> {
    let mut sessions = lock(store)?;
    let session = sessions.entry(project);
    let process = RunningProcess {
        handle,
        started_at: Instant::now(),
        kind,
    };
    if kind == GodotRunKind::Editor {
        if session
            .editor
            .as_ref()
            .is_some_and(|editor| !editor.handle.is_stopped())
        {
            return Err(AppError {
                message: GodotRunKind::Editor.busy_message().to_owned(),
                hint: Some("Switch to the Godot window that is already open.".to_owned()),
            });
        }
        session.editor = Some(process);
        return Ok(());
    }
    if session.is_busy() {
        let busy = session
            .running_kind()
            .unwrap_or(kind)
            .busy_message()
            .to_owned();
        return Err(AppError {
            message: busy,
            hint: Some("Stop it from the Godot pane's toolbar first.".to_owned()),
        });
    }
    session.running = Some(process);
    Ok(())
}

pub(crate) fn release_slot(store: &GodotSessionStore, project: &str, kind: GodotRunKind) {
    if let Ok(mut sessions) = lock(store) {
        let session = sessions.entry(project);
        if kind == GodotRunKind::Editor {
            session.editor = None;
        } else {
            session.running = None;
        }
    }
}

// ── commands ─────────────────────────────────────────────────────────────────────────

/// What the pane renders before anything is clicked.
#[tauri::command]
#[specta::specta]
pub async fn godot_status(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
) -> Result<GodotStatus, AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    let store = store.inner().clone();
    let install = install_for(&state, &store, &key).await;
    let templates_installed = install
        .as_ref()
        .is_some_and(|install| export_templates_installed(&install.version));
    let (running, preview_url) = match lock(&store) {
        Ok(sessions) => {
            let session = sessions.get(&key);
            (
                session.and_then(|session| {
                    let run = session.running.as_ref()?;
                    (!run.handle.is_stopped()).then(|| RunningInfo {
                        kind: run.kind,
                        running_ms: u64::try_from(run.started_at.elapsed().as_millis())
                            .unwrap_or(u64::MAX),
                    })
                }),
                session
                    .and_then(|session| session.preview.as_ref())
                    .map(|preview| preview.url().to_owned()),
            )
        }
        Err(_) => (None, None),
    };
    Ok(GodotStatus {
        install_source: install
            .as_ref()
            .map(|install| crate::godot::describe_source(install.source).to_owned()),
        templates_installed,
        is_godot_project: root
            .join(bhippi_engine::godot::action::PROJECT_FILE)
            .is_file(),
        manifest_main_scene: manifest_main_scene(&root),
        install,
        offer: describe_install_offer(),
        running,
        preview_url,
    })
}

/// Point Bhippi at a Godot binary the user chose (GAD-082's Locate… button).
///
/// The path is *probed*, not trusted: `--version` has to answer, and the answer has to be a
/// version this build drives. Saving a path that does not work would turn one clear failure
/// here into an obscure one at the next Play.
#[tauri::command]
#[specta::specta]
pub async fn set_godot_path(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    path: String,
    project: Option<String>,
) -> Result<GodotStatus, AppError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError {
            message: "No Godot path was chosen.".to_owned(),
            hint: Some("Pick the Godot executable, not the folder it is in.".to_owned()),
        });
    }
    let chosen = PathBuf::from(trimmed);
    // On Windows the version has to come from the console build; the windowed one prints
    // into a console that is not there.
    let (cli_exe, gui_exe) = bhippi_engine::godot::detect::pair_windows_binaries(&chosen);
    if !cli_exe.is_file() {
        return Err(AppError {
            message: format!("{} is not a file.", cli_exe.display()),
            hint: Some("Choose the Godot executable itself.".to_owned()),
        });
    }
    let (exit, output) = capture(&version_command_for(&cli_exe)).await?;
    if !exit.is_success() {
        return Err(AppError {
            message: format!("{} did not answer --version.", cli_exe.display()),
            hint: Some("Choose a Godot 4 executable; that file is not one.".to_owned()),
        });
    }
    let version = bhippi_engine::godot::detect::parse_version(&output).map_err(engine_error)?;
    bhippi_engine::godot::detect::require_supported(&version).map_err(engine_error)?;

    let mut config = state.config.load().await.map_err(AppError::from)?;
    config.godot.path = Some(display_of(&cli_exe));
    state.config.save(&config).await.map_err(AppError::from)?;

    let install = GodotInstall {
        cli_exe,
        gui_exe,
        version,
        source: bhippi_engine::godot::detect::GodotInstallSource::Config,
    };
    let store_handle = store.inner().clone();
    // Every cached detection is now stale, including projects other than this one.
    if let Ok(mut sessions) = lock(&store_handle) {
        for key in sessions.by_project.keys().cloned().collect::<Vec<_>>() {
            sessions.entry(&key).install = Some(install.clone());
        }
    }

    match project {
        Some(project) => godot_status(state, store, project).await,
        None => Ok(GodotStatus {
            install_source: Some(crate::godot::describe_source(install.source).to_owned()),
            templates_installed: export_templates_installed(&install.version),
            is_godot_project: false,
            manifest_main_scene: None,
            install: Some(install),
            offer: describe_install_offer(),
            running: None,
            preview_url: None,
        }),
    }
}

/// Overall dependencies report across Godot, templates, git, and AI providers.
#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct SystemDependenciesStatus {
    pub godot_installed: bool,
    pub godot_version: Option<String>,
    pub godot_path: Option<String>,
    pub godot_offer_url: String,
    pub templates_installed: bool,
    pub git_installed: bool,
    pub git_version: Option<String>,
    pub active_provider: String,
    pub active_provider_id: String,
    pub provider_ready: bool,
    pub needs_setup: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn check_system_dependencies(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
) -> Result<SystemDependenciesStatus, AppError> {
    let store_handle = store.inner().clone();
    let install = install_for(&state, &store_handle, "").await;
    let godot_installed = install.is_some();
    let godot_version = install.as_ref().map(|i| i.version.short());
    let godot_path = install.as_ref().map(|i| display_of(&i.cli_exe));
    let templates_installed = install
        .as_ref()
        .is_some_and(|i| export_templates_installed(&i.version));

    let host = bhippi_engine::godot::detect::InstallTarget::host();
    let offer = describe_install_offer();
    let godot_offer_url = offer
        .downloads
        .iter()
        .find(|(target, _)| *target == host)
        .map(|(_, url)| url.clone())
        .unwrap_or_else(|| {
            format!(
                "https://github.com/godotengine/godot/releases/download/4.7.1-stable/{}",
                host.file_name()
            )
        });

    let git_out = tokio::process::Command::new("git")
        .arg("--version")
        .output()
        .await;

    let (git_installed, git_version) = match git_out {
        Ok(out) if out.status.success() => {
            let ver = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            (true, Some(ver))
        }
        _ => (false, None),
    };

    let registry = state.registry.read().await;
    let active_provider_id = registry.default_id.clone();
    let active_provider = registry
        .providers
        .iter()
        .find(|row| row.id == active_provider_id)
        .map(|row| row.label.clone())
        .unwrap_or_else(|| "Demo (offline)".to_owned());
    let provider_ready = active_provider_id != "demo";

    let needs_setup = !godot_installed;

    Ok(SystemDependenciesStatus {
        godot_installed,
        godot_version,
        godot_path,
        godot_offer_url,
        templates_installed,
        git_installed,
        git_version,
        active_provider,
        active_provider_id,
        provider_ready,
        needs_setup,
    })
}

/// 1-Click download and install the official pinned Godot 4.7.1 binary.
#[tauri::command]
#[specta::specta]
pub async fn download_and_install_godot(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
) -> Result<GodotStatus, AppError> {
    let host = bhippi_engine::godot::detect::InstallTarget::host();
    let offer = describe_install_offer();
    let download_url = offer
        .downloads
        .iter()
        .find(|(target, _)| *target == host)
        .map(|(_, url)| url.clone())
        .unwrap_or_else(|| {
            format!(
                "https://github.com/godotengine/godot/releases/download/4.7.1-stable/{}",
                host.file_name()
            )
        });

    let base_dir = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Programs")
            .join("Godot")
            .join("4.7.1")
    } else {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".bhippi")
            .join("godot")
            .join("4.7.1")
    };

    tokio::fs::create_dir_all(&base_dir)
        .await
        .map_err(|e| AppError {
            message: format!(
                "Could not create Godot installation directory `{}`: {e}",
                base_dir.display()
            ),
            hint: None,
        })?;

    let archive_path = base_dir.join(host.file_name());

    let curl_output = tokio::process::Command::new("curl")
        .args([
            "-L",
            "-s",
            "-o",
            archive_path.to_str().unwrap_or(""),
            &download_url,
        ])
        .output()
        .await;

    let download_ok = match curl_output {
        Ok(out) => out.status.success() && archive_path.is_file(),
        Err(_) => false,
    };

    if !download_ok {
        return Err(AppError {
            message: format!("Failed to download Godot from {download_url}"),
            hint: Some("Check your internet connection or install Godot manually.".to_owned()),
        });
    }

    let extract_res = if cfg!(windows) {
        tokio::process::Command::new("tar")
            .args([
                "-xf",
                archive_path.to_str().unwrap_or(""),
                "-C",
                base_dir.to_str().unwrap_or(""),
            ])
            .output()
            .await
    } else {
        tokio::process::Command::new("unzip")
            .args([
                "-o",
                archive_path.to_str().unwrap_or(""),
                "-d",
                base_dir.to_str().unwrap_or(""),
            ])
            .output()
            .await
    };

    if let Err(e) = extract_res {
        return Err(AppError {
            message: format!("Could not extract Godot archive: {e}"),
            hint: Some("Ensure tar or unzip is available on your system.".to_owned()),
        });
    }

    // Locate the extracted Godot binary
    let mut exe_path = None;
    if let Ok(mut entries) = tokio::fs::read_dir(&base_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if (name.starts_with("Godot_v4")
                || name.eq_ignore_ascii_case("godot.exe")
                || name.eq_ignore_ascii_case("godot"))
                && !name.ends_with(".zip")
            {
                exe_path = Some(path);
                if name.ends_with(bhippi_engine::godot::detect::WINDOWS_CONSOLE_SUFFIX) {
                    break;
                }
            }
        }
    }

    let Some(found_exe) = exe_path else {
        return Err(AppError {
            message: "Godot was downloaded but the executable could not be found in the archive."
                .to_owned(),
            hint: Some("Try selecting your Godot executable manually.".to_owned()),
        });
    };

    // Automatically configure Bhippi with this installed Godot binary
    set_godot_path(state, store, found_exe.to_string_lossy().to_string(), None).await
}

/// Scaffold a Godot project and register it (the Rust half of GAD-014).
///
/// The scaffold and the registration are the *existing* paths — `scaffold::write_project`
/// and the same `add_existing_project` a folder-picker uses — so a Godot project is a
/// project like any other the moment it exists, rather than a second kind of thing the
/// sidebar has to learn about.
#[tauri::command]
#[specta::specta]
pub async fn godot_create_project(
    state: tauri::State<'_, crate::Runtime>,
    parent: String,
    name: String,
    template: ProjectTemplate,
) -> Result<crate::workspace::ProjectSummary, AppError> {
    let parent = crate::workspace::canonical_directory(&parent)?;
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError {
            message: "Enter a game name.".to_owned(),
            hint: Some("The name becomes config/name in project.godot.".to_owned()),
        });
    }
    let slug = project_slug(trimmed);
    let target = parent.join(&slug);
    if target.exists() {
        return Err(AppError {
            message: format!("{} already exists.", target.display()),
            hint: Some("Choose another name or another folder.".to_owned()),
        });
    }
    scaffold::write_project(&target, trimmed, template, false).map_err(engine_error)?;
    let canonical = crate::workspace::canonical_directory(&display_of(&target))?;
    crate::workspace::remember_project(state.inner(), canonical).await
}

/// A folder name from a game name: safe on every platform, never empty, never a dot.
fn project_slug(name: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for character in name.trim().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "godot-game".to_owned()
    } else {
        trimmed
    }
}

/// The Outliner's tree for one scene.
#[tauri::command]
#[specta::specta]
pub async fn godot_scene_tree(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
    scene_rel: Option<String>,
) -> Result<SceneTreeView, AppError> {
    let root = resolve_project(&state, &project).await?;
    let scene_rel = pick_scene(&root, scene_rel.as_deref())?;
    let scene = load_scene(&root, &scene_rel)?;
    if let Ok(mut sessions) = lock(store.inner()) {
        sessions.entry(&display_of(&root)).open_scene = Some(scene_rel.clone());
    }
    Ok(scene_tree_view(&scene, &scene_rel))
}

/// The projection the pane renders. Every field is computed here, in Rust (INV-073).
fn scene_tree_view(scene: &GodotScene, scene_rel: &str) -> SceneTreeView {
    let total = scene.node_count();
    let nodes: Vec<SceneTreeNode> = scene
        .nodes
        .iter()
        .take(SCENE_DIGEST_MAX_NODES)
        .map(|node| SceneTreeNode {
            has_children: !scene.children(&node.path).is_empty(),
            script: scene
                .node(&node.path)
                .and_then(|view: NodeView| view.script),
            path: node.path.clone(),
            name: node.name.clone(),
            type_: node.type_.clone(),
            groups: node.groups.clone(),
            depth: node.depth,
        })
        .collect();
    SceneTreeView {
        scene_rel: scene_rel.to_owned(),
        root: scene.root().map(|node| node.path.clone()),
        digest: scene.tree_digest(SCENE_DIGEST_MAX_NODES),
        node_count: total,
        truncated: total > SCENE_DIGEST_MAX_NODES,
        nodes,
    }
}

/// One node, for the Details rail.
#[tauri::command]
#[specta::specta]
pub async fn godot_node(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    scene_rel: String,
    path: String,
) -> Result<NodeView, AppError> {
    let root = resolve_project(&state, &project).await?;
    let scene_rel = pick_scene(&root, Some(&scene_rel))?;
    let scene = load_scene(&root, &scene_rel)?;
    scene.node(&path).ok_or_else(|| AppError {
        message: format!("`{path}` is not in {scene_rel}."),
        hint: Some("Pick the node from the Outliner; the scene may have changed.".to_owned()),
    })
}

/// Every `.tscn` under `scenes/`.
#[tauri::command]
#[specta::specta]
pub async fn godot_list_scenes(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
) -> Result<Vec<String>, AppError> {
    let root = resolve_project(&state, &project).await?;
    let mut scenes = gates::scene_files(&root);
    scenes.truncate(MAX_LISTED_SCENES);
    Ok(scenes)
}

/// The scene to act on: the one named, else the manifest's main scene, else the first found.
fn pick_scene(root: &Path, scene_rel: Option<&str>) -> Result<String, AppError> {
    if let Some(named) = scene_rel.map(str::trim).filter(|rel| !rel.is_empty()) {
        let normalised = res_to_rel(named);
        if normalised.contains("..") {
            return Err(AppError {
                message: "That scene path leaves the project.".to_owned(),
                hint: Some("Pick a scene from the selector.".to_owned()),
            });
        }
        return Ok(normalised);
    }
    if let Some(main) = manifest_main_scene(root) {
        return Ok(res_to_rel(&main));
    }
    gates::scene_files(root).first().cloned().ok_or(AppError {
        message: "This project has no scenes yet.".to_owned(),
        hint: Some("Create one with New Godot Project, or open it in Godot.".to_owned()),
    })
}

fn load_scene(root: &Path, scene_rel: &str) -> Result<GodotScene, AppError> {
    let full = root.join(scene_rel);
    let text = std::fs::read_to_string(&full).map_err(|error| AppError {
        message: format!("Could not read {scene_rel}: {error}"),
        hint: Some("The scene may have been moved or deleted outside Bhippi.".to_owned()),
    })?;
    GodotScene::parse(&text.replace("\r\n", "\n")).map_err(engine_error)
}

/// Apply one typed batch: lower, write, check every script it wrote, journal, announce.
#[tauri::command]
#[specta::specta]
pub async fn godot_apply_batch(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    batch: GodotActionBatch,
    actor: String,
) -> Result<GodotBatchResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let actor = normalise_actor(&actor)?;
    apply_batch_for(GodotApplyHost { app: Some(&app) }, &root, &batch, &actor)
        .await
        .map_err(|failure| failure.error)
}

/// The Tauri surfaces a Godot batch needs when it is applied from somewhere that is not a
/// command handler — the chat bridge, or a test.
///
/// `None` is the headless case: nothing is emitted to the webview and the Godot binary is
/// resolved from the environment rather than from the user's saved `[godot] path`.
#[derive(Clone, Copy, Default)]
pub struct GodotApplyHost<'a> {
    pub app: Option<&'a tauri::AppHandle>,
}

/// A batch that did not apply, carrying enough of the failure to repair it.
///
/// `godot_apply_batch` throws the position away because a command handler has nowhere to put
/// it; the chat bridge needs it, because "action 2 failed" plus that action's real schema is
/// the whole of a repair round.
#[derive(Clone, Debug)]
pub struct GodotApplyError {
    /// The action that stopped the batch, when lowering named one.
    pub index: Option<usize>,
    /// That action's `kind`, for the schema hint.
    pub kind: Option<String>,
    pub error: AppError,
}

impl GodotApplyError {
    fn plain(error: AppError) -> Self {
        Self {
            index: None,
            kind: None,
            error,
        }
    }
}

/// Lower, gate, write, check, journal and announce one typed batch.
///
/// The one door both the pane and the chat bridge go through, so an agent's batch is
/// check-only'd, rolled back on failure, journaled and broadcast exactly as a human's is.
pub async fn apply_batch_for(
    host: GodotApplyHost<'_>,
    root: &Path,
    batch: &GodotActionBatch,
    actor: &str,
) -> Result<GodotBatchResult, GodotApplyError> {
    // ENG-190 / GAD-091: the project's `[agent]` policy is a gate in code, not a sentence in
    // a prompt, and it binds the agent only — a person driving the pane is the person the
    // policy is protecting. `ask` is answered upstream, where there is a user to ask; `deny`
    // is refused right here so no caller can forget to.
    if actor == "agent" {
        let kinds: Vec<String> = batch
            .actions
            .iter()
            .map(|action| action.kind().to_owned())
            .collect();
        let policy = bhippi_engine::manifest::load_manifest(root)
            .ok()
            .flatten()
            .map(|manifest| manifest.agent)
            .unwrap_or_default();
        if let Some(refusal) = bhippi_engine::capability::evaluate_godot(&policy, &kinds).refusal()
        {
            return Err(GodotApplyError::plain(AppError {
                message: refusal,
                hint: Some(
                    "Set that key to `allow` or `ask` under `[agent]` in Bhippi.game.toml, or \
                     in Engine → Agent permissions."
                        .to_owned(),
                ),
            }));
        }
    }

    let changeset =
        bhippi_engine::godot::action::lower(root, batch).map_err(|error| GodotApplyError {
            kind: batch
                .actions
                .get(error.index)
                .map(|action| action.kind().to_owned()),
            index: Some(error.index),
            error: AppError {
                hint: error.error.hint().map(str::to_owned),
                message: error.to_string(),
            },
        })?;
    apply_and_journal(host, root, changeset, actor).await
}

/// The shared tail of `apply_batch_for` and `godot_undo_last`.
pub(crate) async fn apply_and_journal(
    host: GodotApplyHost<'_>,
    root: &Path,
    changeset: GodotChangeSet,
    actor: &str,
) -> Result<GodotBatchResult, GodotApplyError> {
    let changed_files: Vec<String> = changeset
        .changes
        .iter()
        .map(|change| change.path.clone())
        .collect();
    let scripts = changeset.scripts_needing_check();
    apply_changeset(root, &changeset)
        .map_err(|error| GodotApplyError::plain(engine_error(error)))?;

    // A script Godot cannot parse never stays on disk. The check runs *after* the write
    // because `--check-only` compiles a file, not a string — so the write is provisional
    // until it passes, and the inverse is what makes "provisional" true rather than hopeful.
    if !scripts.is_empty() {
        let roll_back = || {
            let inverse = invert(&changeset);
            if let Err(error) = apply_changeset(root, &inverse) {
                tracing::error!(%error, "a failed script check could not be rolled back");
            }
        };
        let install = match install_for_host(host, root).await {
            Ok(install) => install,
            Err(error) => {
                roll_back();
                return Err(GodotApplyError::plain(error));
            }
        };
        for script in &scripts {
            let spec = check_script_command(install.cli(), root, script);
            let (exit, output) = match capture(&spec).await {
                Ok(captured) => captured,
                Err(error) => {
                    roll_back();
                    return Err(GodotApplyError::plain(error));
                }
            };
            let fault = first_script_fault(&output, script);
            if !exit.is_success() || fault.is_some() {
                roll_back();
                let located = fault.map(|fault| fault.to_message()).unwrap_or_else(|| {
                    format!("{script}: Godot rejected the script and printed no location")
                });
                return Err(GodotApplyError {
                    index: changeset
                        .outcomes
                        .iter()
                        .find(|outcome| outcome.needs_check)
                        .map(|outcome| outcome.index),
                    kind: Some("write_script".to_owned()),
                    error: AppError {
                        message: located,
                        hint: Some(
                            "The batch was rolled back; nothing was written. Fix the line and \
                             apply it again."
                                .to_owned(),
                        ),
                    },
                });
            }
        }
    }

    let txn_id = ulid::Ulid::new().to_string();
    let scene_rel = single_scene(&changed_files);
    let ops_json = serde_json::to_string(&changeset).unwrap_or_else(|_| "{}".to_owned());
    let inverse_json =
        serde_json::to_string(&invert(&changeset)).unwrap_or_else(|_| "{}".to_owned());
    let op_count = i64::try_from(changeset.changes.len()).unwrap_or(i64::MAX);
    let revision = crate::engine::journal_edit(
        root,
        &crate::engine::session::JournalFacts {
            scene_rel_path: scene_rel.clone().unwrap_or_default(),
            txn_id: txn_id.clone(),
            actor: actor.to_owned(),
            label: changeset.label.clone(),
            ops_json,
            inverse_json,
            touched_json: serde_json::to_string(&changed_files).unwrap_or_else(|_| "[]".to_owned()),
            op_count,
        },
    )
    .await;

    if let Some(app) = host.app {
        let _ignored = GodotSceneChanged {
            project: display_of(root),
            scene_rel: scene_rel.clone(),
            txn_id: txn_id.clone(),
            actor: actor.to_owned(),
            label: changeset.label.clone(),
            changed_files: changed_files.clone(),
        }
        .emit(app);
    }

    Ok(GodotBatchResult {
        outcomes: changeset.outcomes,
        txn_id,
        changed_files,
        needs_check: scripts,
        label: changeset.label,
        revision,
    })
}

fn normalise_actor(actor: &str) -> Result<String, AppError> {
    match actor.trim() {
        "user" => Ok("user".to_owned()),
        "agent" => Ok("agent".to_owned()),
        other => Err(AppError {
            message: format!("`{other}` is not an actor."),
            hint: Some("Use `user` for an editor edit and `agent` for a model's.".to_owned()),
        }),
    }
}

/// The scene a batch is about, when it is about exactly one.
fn single_scene(changed: &[String]) -> Option<String> {
    let mut scenes = changed.iter().filter(|path| path.ends_with(".tscn"));
    let first = scenes.next()?;
    scenes.next().is_none().then(|| first.clone())
}

/// Take back the last journaled Godot transaction, as a new user transaction.
///
/// Undo is not a special mode: applying the inverse *is* a change, so it is lowered, written
/// and journaled like any other, which is why undoing an undo is redo and needs no code.
#[tauri::command]
#[specta::specta]
pub async fn godot_undo_last(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    project: String,
) -> Result<GodotBatchResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let database = state.brain_db.as_ref().as_ref().ok_or_else(|| AppError {
        message: "The change journal is unavailable.".to_owned(),
        hint: Some("Restart the app; undo reads from the journal database.".to_owned()),
    })?;
    let project_path = root.to_string_lossy().replace('\\', "/");
    let rows = database
        .engine()
        .list(&project_path, None, 32)
        .await
        .map_err(|error| AppError {
            message: format!("Could not read the change journal: {error}"),
            hint: None,
        })?;
    // The journal is shared with the pre-Godot engine. A row whose inverse parses as a
    // Godot change set is one of ours; anything else belongs to the old scene format and
    // must not be applied to a `.tscn`.
    let inverse = rows
        .iter()
        .find_map(|row| serde_json::from_str::<GodotChangeSet>(&row.inverse_json).ok())
        .ok_or_else(|| AppError {
            message: "There is no Godot change to undo.".to_owned(),
            hint: Some("Undo covers changes Bhippi applied through the Godot pane.".to_owned()),
        })?;
    apply_and_journal(GodotApplyHost { app: Some(&app) }, &root, inverse, "user")
        .await
        .map_err(|failure| failure.error)
}

/// Play: a windowed Godot on this project.
#[tauri::command]
#[specta::specta]
pub async fn godot_run(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
) -> Result<(), AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    let store = store.inner().clone();
    let install = require_install(&state, &store, &key).await?;

    let (handle, signal) = stop_channel();
    claim_slot(&store, &key, GodotRunKind::Run, handle)?;
    announce_process(&app, &key, GodotRunKind::Run, GodotRunState::Starting, None);

    // The GUI binary, so no console window flashes behind the game. A windowed run has no
    // timeout: it ends when the player closes it or presses Stop.
    let mut spec = run_command(
        install.gui(),
        &root,
        &RunOptions {
            headless: false,
            ..RunOptions::default()
        },
    );
    spec.timeout_secs = 0;

    let sender = start_output_pump(app.clone(), store.clone(), key.clone());
    tauri::async_runtime::spawn(async move {
        announce_process(&app, &key, GodotRunKind::Run, GodotRunState::Running, None);
        let result = run_spec_with_stop(&spec, Some(signal), |line| {
            let _ignored = sender.send(line);
        })
        .await;
        drop(sender);
        release_slot(&store, &key, GodotRunKind::Run);
        let exit = match result {
            Ok(exit) => Some(exit),
            Err(error) => {
                tracing::warn!(message = %error.message, "the Godot run failed to start");
                None
            }
        };
        announce_process(&app, &key, GodotRunKind::Run, GodotRunState::Exited, exit);
    });
    Ok(())
}

/// Stop whatever this project is running.
#[tauri::command]
#[specta::specta]
pub async fn godot_stop(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
) -> Result<bool, AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    let mut sessions = lock(store.inner())?;
    let session = sessions.entry(&key);
    let Some(running) = &session.running else {
        return Ok(false);
    };
    Ok(running.handle.kill())
}

/// A deterministic headless replay, with telemetry parsed into a report.
#[tauri::command]
#[specta::specta]
pub async fn godot_playtest(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    inputs: Option<PlaytestScript>,
    frames: Option<u32>,
) -> Result<PlaytestResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let inputs = inputs
        .map(|script| script.to_inputs())
        .unwrap_or_else(default_playtest_inputs);
    run_playtest_for(GodotApplyHost { app: Some(&app) }, &root, inputs, frames).await
}

/// The headless replay itself, callable without a command handler.
///
/// The chat bridge answers `{"kind":"playtest"}` through this, so a model's playtest and the
/// pane's Playtest button are the same run with the same probe, the same telemetry file and
/// the same one-process slot — not two paths that could disagree about what the game did.
pub async fn run_playtest_for(
    host: GodotApplyHost<'_>,
    root: &Path,
    inputs: PlaytestInputs,
    frames: Option<u32>,
) -> Result<PlaytestResult, AppError> {
    let key = display_of(root);
    let store = host.app.and_then(|app| {
        use tauri::Manager as _;
        app.try_state::<GodotSessionStore>()
            .map(|store| store.inner().clone())
    });
    let install = install_for_host(host, root).await?;
    let json = inputs.to_json().map_err(engine_error)?;
    let frames = frames.unwrap_or(DEFAULT_PLAYTEST_FRAMES);

    let run_id = ulid::Ulid::new().to_string();
    let telemetry_dir = root.join(".bhippi").join("telemetry");
    std::fs::create_dir_all(&telemetry_dir).map_err(|error| AppError {
        message: format!("Could not create the telemetry folder: {error}"),
        hint: Some("Check the project folder is writable.".to_owned()),
    })?;
    let inputs_path = telemetry_dir.join(format!("{run_id}.inputs.json"));
    let telemetry_path = telemetry_dir.join(format!("{run_id}.jsonl"));
    std::fs::write(&inputs_path, json).map_err(|error| AppError {
        message: format!("Could not write the playtest inputs: {error}"),
        hint: Some("Check the project folder is writable.".to_owned()),
    })?;

    let (handle, signal) = stop_channel();
    // The one-process slot, the toolbar's Play/Stop state and the Output pump all live on
    // the session store, which only exists when there is a window. Headless there is no
    // second run to collide with and no pane to tell, so the run is the whole of the work.
    if let Some(store) = store.as_ref() {
        claim_slot(store, &key, GodotRunKind::Playtest, handle)?;
    }
    if let Some(app) = host.app {
        announce_process(
            app,
            &key,
            GodotRunKind::Playtest,
            GodotRunState::Running,
            None,
        );
    }
    let spec = playtest_command(install.cli(), root, &inputs_path, &telemetry_path, frames);
    let sender = match (host.app, store.as_ref()) {
        (Some(app), Some(store)) => {
            Some(start_output_pump(app.clone(), store.clone(), key.clone()))
        }
        _ => None,
    };
    let mut tail: Vec<String> = Vec::new();
    let result = run_spec_with_stop(&spec, Some(signal), |line| {
        if tail.len() >= GODOT_LOG_TAIL {
            tail.remove(0);
        }
        tail.push(line.text.clone());
        if let Some(sender) = sender.as_ref() {
            let _ignored = sender.send(line);
        }
    })
    .await;
    drop(sender);
    if let Some(store) = store.as_ref() {
        release_slot(store, &key, GodotRunKind::Playtest);
    }
    let exit = result?;
    if let Some(app) = host.app {
        announce_process(
            app,
            &key,
            GodotRunKind::Playtest,
            GodotRunState::Exited,
            Some(exit),
        );
    }

    let text = std::fs::read_to_string(&telemetry_path).unwrap_or_default();
    let report = TelemetryReport::from_jsonl(&text);
    if let Some(mut sessions) = store.as_ref().and_then(|store| lock(store).ok()) {
        sessions.entry(&key).last_playtest = Some(report.clone());
    }
    Ok(PlaytestResult {
        report,
        exit,
        telemetry_path: display_of(&telemetry_path),
        log_tail: tail,
    })
}

/// The default script the pane's Playtest button replays: walk forward, then jump.
///
/// It lives here rather than in the webview because it is a *test*, not a control: the
/// frames it presses at are what the telemetry assertions read, and a UI that could change
/// them would be a UI that could change the evidence.
#[must_use]
pub fn default_playtest_inputs() -> PlaytestInputs {
    PlaytestInputs::new(vec![
        PlaytestStep::action(10, "move_forward", true),
        PlaytestStep::action(70, "move_forward", false),
        PlaytestStep::action(80, "jump", true),
        PlaytestStep::action(84, "jump", false),
        PlaytestStep::action(100, "move_right", true),
        PlaytestStep::action(150, "move_right", false),
    ])
}

/// Watch play: open the game in a real window, play it, and bring back frames paired with
/// telemetry (ADR-0044, GAD-095…099).
///
/// The thin half of the loop. Everything that makes it what it is lives in
/// [`crate::godot_observe`]; this resolves the project, claims the same one-process slot
/// `godot_run` claims, announces the run so the pane's Play button stays truthful, and hands
/// the observation its stop handle — the very handle `godot_stop` kills, so the toolbar's Stop
/// ends a visual playtest exactly as it ends a run.
#[tauri::command]
#[specta::specta]
pub async fn godot_visual_playtest(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    viewport: tauri::State<'_, crate::godot_embed::GodotEmbedHost>,
    project: String,
    plan: Option<VisualPlaytestPlan>,
) -> Result<VisualPlaytestResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    let store = store.inner().clone();
    let viewport = viewport.inner().clone();
    // Validated before the install is even looked up: a plan past a cap costs nothing.
    let plan = plan.unwrap_or_else(VisualPlaytestPlan::watch_play);
    plan.validate()?;
    // The viewport is one hole; a game already in it is refused before anything is spawned.
    if crate::godot_embed::game_in_viewport(&viewport) {
        return Err(AppError {
            message: "A game is already running in the viewport.".to_owned(),
            hint: Some("Stop it first, then press Watch play.".to_owned()),
        });
    }
    let install = require_install(&state, &store, &key).await?;

    let (handle, signal) = stop_channel();
    // The pane's Stop button and the loop's own shutdown are the same handle.
    let killer = handle.clone();
    claim_slot(&store, &key, GodotRunKind::VisualPlaytest, handle.clone())?;
    announce_process(
        &app,
        &key,
        GodotRunKind::VisualPlaytest,
        GodotRunState::Starting,
        None,
    );
    let sender = start_output_pump(app.clone(), store.clone(), key.clone());
    announce_process(
        &app,
        &key,
        GodotRunKind::VisualPlaytest,
        GodotRunState::Running,
        None,
    );

    let name = game_name(&root);
    // The window goes into the studio viewport the moment it exists (ADR-0045): Watch play
    // is a run like any other, and no run opens a Godot window of its own.
    let on_window: Box<crate::godot_observe::WindowHook> = {
        let app = app.clone();
        let viewport = viewport.clone();
        let key = key.clone();
        Box::new(move |window| {
            crate::godot_embed::adopt_foreign_window(
                &app,
                &viewport,
                &key,
                window.process_id,
                window.hwnd,
                handle.clone(),
            )
        })
    };
    let result = crate::godot_observe::run_visual_playtest(
        crate::godot_observe::VisualLaunch {
            root: &root,
            // The GUI binary: a windowed run through the console build flashes a console
            // behind the game, which is the one thing a playtest must not photograph.
            gui_exe: install.gui(),
            game_name: &name,
            stop: (killer, signal),
            lines: Some(sender),
            on_window: Some(on_window),
        },
        &plan,
    )
    .await;

    crate::godot_embed::release_foreign_window(&app, &viewport);
    release_slot(&store, &key, GodotRunKind::VisualPlaytest);
    let exit = result.as_ref().ok().and_then(|result| result.exit);
    announce_process(
        &app,
        &key,
        GodotRunKind::VisualPlaytest,
        GodotRunState::Exited,
        exit,
    );

    let result = result?;
    // The telemetry half is a playtest report like any other, so the pane's Playtest tab and
    // `/gamedebug` see it without learning a second shape.
    if let (Ok(mut sessions), Some(report)) = (lock(&store), result.telemetry.clone()) {
        sessions.entry(&key).last_playtest = Some(report);
    }
    Ok(result)
}

/// Export the project, after the gates agree it may ship.
#[tauri::command]
#[specta::specta]
pub async fn godot_export(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
    target: PresetTarget,
) -> Result<ExportResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    let store = store.inner().clone();

    // INV-074 & GAD-124: pre-export verification by the Export Doctor
    let engine_target = match target {
        PresetTarget::Web => bhippi_engine::godot::export::ExportTarget::Web,
        PresetTarget::Windows => bhippi_engine::godot::export::ExportTarget::WindowsDesktop,
    };
    let pre_doctor = bhippi_engine::godot::export::pre_export_doctor(&root, engine_target, true);
    if !pre_doctor.passed {
        let first = pre_doctor
            .blockers
            .first()
            .cloned()
            .unwrap_or_else(|| "Pre-export verification failed.".to_owned());
        return Err(AppError {
            message: format!("Export Doctor blocked export: {first}"),
            hint: Some("Open Check in the Godot pane to inspect and resolve blockers.".to_owned()),
        });
    }

    let install = require_install(&state, &store, &key).await?;
    if !export_templates_installed(&install.version) {
        return Err(AppError {
            message: format!(
                "Godot {} has no export templates installed.",
                install.version.short()
            ),
            hint: Some(
                "In Godot: Editor → Manage Export Templates → Download and Install. \
                 They are a separate download from the engine."
                    .to_owned(),
            ),
        });
    }

    ensure_preset(&root, target)?;
    let output = match target {
        PresetTarget::Web => root.join(WEB_EXPORT_PATH),
        PresetTarget::Windows => root.join(WINDOWS_EXPORT_DIR).join(format!(
            "{}.exe",
            bhippi_engine::godot::export_presets::executable_stem(&game_name(&root))
        )),
    };
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| AppError {
            message: format!("Could not create the export folder: {error}"),
            hint: Some("Check the project folder is writable.".to_owned()),
        })?;
    }

    let (handle, signal) = stop_channel();
    claim_slot(&store, &key, GodotRunKind::Export, handle)?;
    announce_process(
        &app,
        &key,
        GodotRunKind::Export,
        GodotRunState::Running,
        None,
    );
    let spec = export_command(install.cli(), &root, target.preset_name(), &output, true);
    let sender = start_output_pump(app.clone(), store.clone(), key.clone());
    let mut tail: Vec<String> = Vec::new();
    let result = run_spec_with_stop(&spec, Some(signal), |line| {
        if tail.len() >= GODOT_LOG_TAIL {
            tail.remove(0);
        }
        tail.push(line.text.clone());
        let _ignored = sender.send(line);
    })
    .await;
    drop(sender);
    release_slot(&store, &key, GodotRunKind::Export);
    let exit = result?;
    announce_process(
        &app,
        &key,
        GodotRunKind::Export,
        GodotRunState::Exited,
        Some(exit),
    );

    // Godot can report success and still write nothing when a template is half installed,
    // so the file being there is part of "ok" rather than a separate check the caller has
    // to remember.
    let ok = exit.is_success() && output.exists();
    if ok {
        let _ = bhippi_engine::godot::export::ensure_export_credits(&root, engine_target);
        let post_doctor = bhippi_engine::godot::export::post_export_doctor(&root, engine_target);
        if !post_doctor.passed {
            let first = post_doctor
                .blockers
                .first()
                .cloned()
                .unwrap_or_else(|| "Export artefact failed validation.".to_owned());
            return Err(AppError {
                message: format!("Export Doctor blocked export: {first}"),
                hint: Some("Check the export output folder or re-export.".to_owned()),
            });
        }
    }
    // GAD-094: every artefact that exists is a point somebody can come back to, so the
    // version is recorded here rather than only on the Publish path.
    let version_id = if ok {
        crate::godot_versions::record_export_version(&state, &root, target, &output).await
    } else {
        None
    };

    Ok(ExportResult {
        ok,
        output_path: display_of(&output),
        log_tail: tail,
        version_id,
    })
}

/// Make sure `export_presets.cfg` carries the preset the export names.
///
/// Merged, never replaced: a user who tuned the Windows preset by hand keeps their entries,
/// and only a preset that is *missing* is added from the defaults.
fn ensure_preset(root: &Path, target: PresetTarget) -> Result<(), AppError> {
    let path = root.join(bhippi_engine::godot::action::EXPORT_PRESETS_FILE);
    let mut presets = match std::fs::read_to_string(&path) {
        Ok(text) => ExportPresets::parse(&text.replace("\r\n", "\n")).map_err(engine_error)?,
        Err(_) => ExportPresets::default(),
    };
    if presets.has_preset(target.preset_name()) {
        return Ok(());
    }
    let defaults = default_presets(&game_name(root));
    let Some(wanted) = defaults.preset(target.preset_name()).cloned() else {
        return Err(AppError {
            message: format!("No default preset is defined for {}.", target.preset_name()),
            hint: None,
        });
    };
    presets.upsert(wanted);
    std::fs::write(&path, presets.to_text()).map_err(|error| AppError {
        message: format!("Could not write export_presets.cfg: {error}"),
        hint: Some("Close the project in the Godot editor and try again.".to_owned()),
    })
}

/// Open the project in Godot's own editor. Not awaited: the editor is the user's session.
#[tauri::command]
#[specta::specta]
pub async fn godot_open_editor(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
) -> Result<(), AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    let store = store.inner().clone();
    let install = require_install(&state, &store, &key).await?;
    let (handle, signal) = stop_channel();
    claim_slot(&store, &key, GodotRunKind::Editor, handle)?;
    announce_process(
        &app,
        &key,
        GodotRunKind::Editor,
        GodotRunState::Starting,
        None,
    );
    let spec = editor_command(install.gui(), &root);
    let sender = start_output_pump(app.clone(), store.clone(), key.clone());
    tauri::async_runtime::spawn(async move {
        announce_process(
            &app,
            &key,
            GodotRunKind::Editor,
            GodotRunState::Running,
            None,
        );
        let result = run_spec_with_stop(&spec, Some(signal), |line| {
            let _ignored = sender.send(line);
        })
        .await;
        drop(sender);
        release_slot(&store, &key, GodotRunKind::Editor);
        announce_process(
            &app,
            &key,
            GodotRunKind::Editor,
            GodotRunState::Exited,
            result.ok(),
        );
    });
    Ok(())
}

/// Run the project gates. `release` turns the licence and preset warnings into blockers.
#[tauri::command]
#[specta::specta]
pub async fn godot_gates(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
    release: bool,
) -> Result<GodotGateReport, AppError> {
    let root = resolve_project(&state, &project).await?;
    let report = gates::check_project(&root, release);
    if let Ok(mut sessions) = lock(store.inner()) {
        sessions.entry(&display_of(&root)).last_gates = Some(report.clone());
    }
    Ok(GodotGateReport::from(&report))
}

/// Serve the web export on loopback and hand back the URL.
#[tauri::command]
#[specta::specta]
pub async fn godot_preview_start(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
) -> Result<String, AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    // An existing server on the same export is reused: a new port every click would leave
    // the old one listening and the Browser pane's history full of dead addresses.
    if let Ok(sessions) = lock(store.inner()) {
        if let Some(preview) = sessions.get(&key).and_then(|s| s.preview.as_ref()) {
            if preview.is_running() {
                return Ok(preview.url().to_owned());
            }
        }
    }
    let server = crate::godot_preview::start(&root)?;
    let url = server.url().to_owned();
    let mut sessions = lock(store.inner())?;
    sessions.entry(&key).preview = Some(server);
    Ok(url)
}

/// Stop the preview server for this project.
#[tauri::command]
#[specta::specta]
pub async fn godot_preview_stop(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
) -> Result<(), AppError> {
    let root = resolve_project(&state, &project).await?;
    let key = display_of(&root);
    let mut sessions = lock(store.inner())?;
    let session = sessions.entry(&key);
    if let Some(preview) = session.preview.take() {
        preview.stop();
    }
    Ok(())
}

/// Inspect export templates status and get install instructions (GAD-121).
#[tauri::command]
#[specta::specta]
pub async fn godot_export_templates_status(
) -> Result<bhippi_engine::godot::templates::ExportTemplatesStatus, AppError> {
    Ok(bhippi_engine::godot::templates::check_export_templates(
        None,
    ))
}

/// Describe the export template download offer (GAD-121).
#[tauri::command]
#[specta::specta]
pub async fn godot_export_template_offer(
) -> Result<bhippi_engine::godot::templates::TemplateInstallOffer, AppError> {
    Ok(bhippi_engine::godot::templates::describe_template_offer())
}

/// The ring buffer, for a pane that mounted after a run had already started.
#[tauri::command]
#[specta::specta]
pub async fn godot_output(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, GodotSessionStore>,
    project: String,
) -> Result<Vec<GodotOutputLine>, AppError> {
    let root = resolve_project(&state, &project).await?;
    let sessions = lock(store.inner())?;
    Ok(sessions
        .get(&display_of(&root))
        .map(|session| session.output.iter().cloned().collect())
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::{
        default_playtest_inputs, first_script_fault, parse_script_faults, project_slug,
        should_flush_early, single_scene, GodotRunKind, GodotSession, GodotSessions,
        GODOT_OUTPUT_BATCH, GODOT_OUTPUT_CAP, GODOT_OUTPUT_MIN_GAP,
    };
    use crate::godot::{GodotOutputLine, GodotStream};
    use std::time::Duration;

    fn line(text: &str) -> GodotOutputLine {
        GodotOutputLine {
            stream: GodotStream::Stdout,
            text: text.to_owned(),
        }
    }

    #[test]
    fn the_output_ring_keeps_the_newest_lines_and_never_grows() {
        let mut session = GodotSession::default();
        for index in 0..GODOT_OUTPUT_CAP + 500 {
            session.push_line(line(&format!("line {index}")));
        }
        assert_eq!(session.output.len(), GODOT_OUTPUT_CAP);
        assert_eq!(
            session.output.front().map(|line| line.text.clone()),
            Some("line 500".to_owned()),
            "the oldest lines are the ones that go"
        );
        assert_eq!(
            session.output.back().map(|line| line.text.clone()),
            Some(format!("line {}", GODOT_OUTPUT_CAP + 499))
        );
    }

    #[test]
    fn output_never_leaves_faster_than_the_bus_allows() {
        let long_ago = Duration::from_secs(1);
        // One line does not make a batch, however long it has been waiting: the ticker
        // publishes that, ten times a second.
        assert!(!should_flush_early(1, long_ago));
        assert!(!should_flush_early(GODOT_OUTPUT_BATCH - 1, long_ago));
        // A full batch goes early, which is what makes a burst feel live…
        assert!(should_flush_early(GODOT_OUTPUT_BATCH, long_ago));
        assert!(should_flush_early(10_000, long_ago));
        // …but never twice inside the minimum gap, which is INV-076's twenty a second.
        assert!(!should_flush_early(10_000, Duration::ZERO));
        assert!(!should_flush_early(
            10_000,
            GODOT_OUTPUT_MIN_GAP - Duration::from_millis(1)
        ));
        assert!(should_flush_early(GODOT_OUTPUT_BATCH, GODOT_OUTPUT_MIN_GAP));
        assert!(
            GODOT_OUTPUT_MIN_GAP.as_millis() >= 50,
            "a gap below 50 ms would let the pump exceed 20 events a second"
        );
    }

    #[test]
    fn a_project_is_busy_only_while_its_process_is_alive() {
        let mut sessions = GodotSessions::new();
        let session = sessions.entry("C:/games/demo");
        assert!(!session.is_busy(), "a fresh session runs nothing");

        let (handle, _signal) = crate::godot::stop_channel();
        session.running = Some(super::RunningProcess {
            handle: handle.clone(),
            started_at: std::time::Instant::now(),
            kind: GodotRunKind::Run,
        });
        assert!(session.is_busy());
        assert_eq!(session.running_kind(), Some(GodotRunKind::Run));
        // A second run — a playtest, say — must be refused while the game is up.
        assert!(session
            .running_kind()
            .map(GodotRunKind::busy_message)
            .unwrap_or_default()
            .contains("already running"));

        handle.kill();
        assert!(
            !session.is_busy(),
            "a stopped handle is stale bookkeeping, not a busy project"
        );
        assert_eq!(sessions.project_count(), 1);
    }

    #[test]
    fn the_editor_has_its_own_slot_and_does_not_block_a_run() {
        let store: super::GodotSessionStore =
            std::sync::Arc::new(std::sync::Mutex::new(GodotSessions::new()));
        let key = "C:/games/demo";

        let (editor, _editor_signal) = crate::godot::stop_channel();
        super::claim_slot(&store, key, GodotRunKind::Editor, editor).expect("the editor opens");
        // The editor never ends on its own, so counting it as *the* process would lock the
        // toolbar out for as long as Godot stayed open.
        let (run, _run_signal) = crate::godot::stop_channel();
        super::claim_slot(&store, key, GodotRunKind::Run, run).expect("Play still works");

        // Two runs, though, is exactly what the slot is for.
        let (second, _second_signal) = crate::godot::stop_channel();
        let refused = super::claim_slot(&store, key, GodotRunKind::Playtest, second)
            .expect_err("a playtest waits for the game to stop");
        assert!(refused.message.contains("already running"));
        assert!(refused.hint.is_some());

        // And a second editor is refused with its own words rather than the run's.
        let (twin, _twin_signal) = crate::godot::stop_channel();
        let refused = super::claim_slot(&store, key, GodotRunKind::Editor, twin)
            .expect_err("one editor per project");
        assert!(refused.message.contains("editor"));

        super::release_slot(&store, key, GodotRunKind::Run);
        let (third, _third_signal) = crate::godot::stop_channel();
        super::claim_slot(&store, key, GodotRunKind::Playtest, third)
            .expect("the slot frees when the run ends");
    }

    #[test]
    fn sessions_are_per_project_and_do_not_share_output() {
        let mut sessions = GodotSessions::new();
        sessions.entry("C:/games/a").push_line(line("from a"));
        sessions.entry("C:/games/b").push_line(line("from b"));
        assert_eq!(sessions.project_count(), 2);
        assert_eq!(
            sessions
                .get("C:/games/a")
                .map(|session| session.output.len()),
            Some(1)
        );
        assert!(sessions.get("C:/games/c").is_none());
    }

    // ── the stderr parser, on lines a real Godot 4 printed ────────────────────────

    #[test]
    fn a_parse_error_with_its_location_on_the_next_line_is_one_fault() {
        let output = "SCRIPT ERROR: Parse Error: Expected end of statement after expression, \
                      found \"Identifier\" instead.\n          \
                      at: GDScript::reload (res://scripts/player.gd:12)\n";
        let faults = parse_script_faults(output);
        assert_eq!(faults.len(), 1, "{faults:?}");
        assert_eq!(faults[0].file, "scripts/player.gd");
        assert_eq!(faults[0].line, 12);
        assert!(faults[0]
            .message
            .starts_with("Parse Error: Expected end of statement"));
        assert_eq!(
            faults[0].to_message(),
            "scripts/player.gd:12: Parse Error: Expected end of statement after expression, \
             found \"Identifier\" instead."
        );
    }

    #[test]
    fn a_location_first_line_is_read_without_a_continuation() {
        let faults = parse_script_faults(
            "res://scripts/player.gd:7 - Parse Error: Identifier \"speedd\" not declared in the \
             current scope.\n",
        );
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0].file, "scripts/player.gd");
        assert_eq!(faults[0].line, 7);
        assert!(faults[0].message.contains("speedd"));
    }

    #[test]
    fn the_error_prefixed_form_and_godots_own_cpp_frame_are_told_apart() {
        let output = "ERROR: res://scripts/enemy.gd:3 - Parse Error: Expected statement, found \
                      \":\" instead.\n   at: (core/object/script_language.cpp:87)\n";
        let faults = parse_script_faults(output);
        assert_eq!(
            faults.len(),
            1,
            "the C++ frame is not the user's fault: {faults:?}"
        );
        assert_eq!(faults[0].file, "scripts/enemy.gd");
        assert_eq!(faults[0].line, 3);
    }

    #[test]
    fn a_failed_load_names_the_file_even_with_no_line() {
        let faults = parse_script_faults(
            "ERROR: Failed to load script \"res://scripts/boss.gd\" with error \"Parse error\".\n",
        );
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0].file, "scripts/boss.gd");
        assert_eq!(faults[0].line, 0);
        assert_eq!(
            faults[0].to_message(),
            "scripts/boss.gd: Failed to load script \"res://scripts/boss.gd\" with error \
             \"Parse error\"."
        );
    }

    #[test]
    fn a_compile_error_whose_message_is_only_a_header_still_locates() {
        let output = "SCRIPT ERROR: Compile Error:\n          \
                      at: GDScript::reload (res://scripts/hud.gd:20)\n";
        let faults = parse_script_faults(output);
        assert_eq!(faults.len(), 1);
        assert_eq!(faults[0].file, "scripts/hud.gd");
        assert_eq!(faults[0].line, 20);
        assert_eq!(faults[0].message, "Compile Error");
    }

    #[test]
    fn ordinary_output_is_never_mistaken_for_a_fault() {
        let output = "Godot Engine v4.7.1.stable.official.a13da4feb - https://godotengine.org\n\
                      Loading resource: res://scenes/main.tscn:1\n\
                      player ready at res://scripts/player.gd\n";
        assert!(parse_script_faults(output).is_empty());
    }

    #[test]
    fn the_reported_fault_prefers_the_script_that_was_being_checked() {
        let output = "res://scripts/other.gd:2 - Parse Error: one\n\
                      res://scripts/player.gd:9 - Parse Error: two\n";
        let chosen = first_script_fault(output, "scripts/player.gd").expect("a fault");
        assert_eq!(chosen.line, 9);
        // With no match it still reports something rather than swallowing the failure.
        let fallback = first_script_fault(output, "scripts/nothing.gd").expect("a fault");
        assert_eq!(fallback.file, "scripts/other.gd");
        assert!(first_script_fault("all fine\n", "scripts/player.gd").is_none());
    }

    // ── small pure helpers ────────────────────────────────────────────────────────

    #[test]
    fn a_game_name_becomes_a_folder_name_that_is_safe_everywhere() {
        assert_eq!(project_slug("My First Game"), "my-first-game");
        assert_eq!(project_slug("  Racer 2!!  "), "racer-2");
        assert_eq!(project_slug("../../etc"), "etc");
        assert_eq!(project_slug("!!!"), "godot-game");
        assert!(!project_slug("a/b").contains('/'));
    }

    #[test]
    fn a_batch_names_its_scene_only_when_it_touched_exactly_one() {
        assert_eq!(
            single_scene(&["scenes/main.tscn".to_owned(), "scripts/a.gd".to_owned()]),
            Some("scenes/main.tscn".to_owned())
        );
        assert_eq!(
            single_scene(&["scenes/a.tscn".to_owned(), "scenes/b.tscn".to_owned()]),
            None
        );
        assert_eq!(single_scene(&["project.godot".to_owned()]), None);
    }

    #[test]
    fn the_default_playtest_script_is_valid_and_finishes_inside_the_default_frames() {
        let inputs = default_playtest_inputs();
        inputs.validate().expect("the default script is valid");
        let last = inputs
            .steps
            .iter()
            .map(|step| step.frame)
            .max()
            .unwrap_or_default();
        assert!(
            last < super::DEFAULT_PLAYTEST_FRAMES,
            "every step must fire before the run ends: {last}"
        );
        // Both halves of "walk and jump" are released again; a key left down is a bug that
        // only shows up as a drifting position three tests later.
        assert!(inputs
            .steps
            .iter()
            .any(|step| step.action.as_deref() == Some("jump") && step.pressed));
        assert!(inputs
            .steps
            .iter()
            .any(|step| step.action.as_deref() == Some("jump") && !step.pressed));
    }
}
