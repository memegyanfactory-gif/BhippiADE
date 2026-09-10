//! The Inspector's IPC surface, and the gate between *recommend* and *execute* (ADR-0056).
//!
//! `bhippi-engine::inspect` finds things and describes repairs; it cannot perform one. This
//! module is where a repair becomes real, and the whole point of it is the narrowness of
//! that path:
//!
//! ```text
//! inspector_scan          reads the project, writes nothing
//! inspector_preview_fix   shows the person the exact actions — and only now mints a token
//! inspector_apply_fix     needs that token, and refuses without it
//!      └── godot_commands::apply_batch_for → lower → apply_changeset → --check-only → journal
//! ```
//!
//! There is no other way in. `inspector_apply_fix` does not accept a batch from its caller;
//! it looks the fix up by the token the preview issued, so "the UI sent slightly different
//! actions than it showed" is not a thing that can happen (INV-096). And because the last
//! hop is the agent's own apply path, an inspector fix is journalled, undoable and — for a
//! script — check-compiled, exactly like every other change Bhippi makes.
//!
//! The one thing this module writes that the engine does not is the **ledger**: what has been
//! ignored and what has been resolved, under the project's `.bhippi/` state directory. That
//! is Bhippi's own memory of the project, never a change to the project itself.

use crate::commands::AppError;
use crate::godot_commands::{apply_batch_for, resolve_project, GodotApplyHost, GodotBatchResult};
use bhippi_engine::godot::action::GodotActionBatch;
use bhippi_engine::inspect::{
    self, Changes, Finding, FindingLedger, InspectCommand, InspectScope, InspectionReport,
    InspectorCard, PerformanceEvidence, ProposedFix,
};
use bhippi_types::{FixRisk, InspectorId, Severity};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Where a project's inspector memory lives. Inside `.bhippi/`, which every project walker
/// in the codebase skips, so the Inspector's own notes never become a finding.
const LEDGER_DIR: &str = ".bhippi";
const LEDGER_FILE: &str = "inspector-ledger.json";

/// One project's live inspector state.
#[derive(Default)]
pub struct InspectorSession {
    /// The most recent scan, so *Open*, *Fix* and *Ask* can find a finding by id without
    /// rescanning the project underneath the user.
    pub report: Option<InspectionReport>,
    /// Findings the user has hidden, kept beside the report they were hidden from.
    pub ignored: Vec<Finding>,
    pub ledger: FindingLedger,
    pub changes: Changes,
    /// Fix tokens this session has actually shown to a person, and what they showed.
    approvals: HashMap<String, ApprovedFix>,
}

#[derive(Clone)]
struct ApprovedFix {
    finding_id: String,
    /// The finding's check code, which is half of what the token is derived from.
    code: String,
    label: String,
    fix: ProposedFix,
}

#[derive(Default)]
pub struct InspectorSessions {
    by_project: HashMap<String, InspectorSession>,
}

impl InspectorSessions {
    fn entry(&mut self, project: &str) -> &mut InspectorSession {
        self.by_project.entry(project.to_owned()).or_default()
    }

    #[must_use]
    pub fn get(&self, project: &str) -> Option<&InspectorSession> {
        self.by_project.get(project)
    }
}

/// The managed handle. A plain `std::sync::Mutex`: every critical section is a map lookup.
pub type InspectorStore = Arc<Mutex<InspectorSessions>>;

fn lock(store: &InspectorStore) -> Result<std::sync::MutexGuard<'_, InspectorSessions>, AppError> {
    store.lock().map_err(|_| AppError {
        message: "The Inspector session store is poisoned.".to_owned(),
        hint: Some("Restart the app; an earlier Inspector call panicked.".to_owned()),
    })
}

// ── what crosses IPC ─────────────────────────────────────────────────────────────────

/// What the user asked to inspect. The webview names a scope; Rust resolves it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Type)]
pub struct InspectRequest {
    /// `"project"` (the default), `"level"`, `"selection"` or `"changes"`.
    #[serde(default)]
    pub scope: Option<String>,
    /// For `"level"`: the scene, project-relative or `res://`.
    #[serde(default)]
    pub scene: Option<String>,
    /// For `"selection"`.
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub asset: Option<String>,
    /// For `"changes"`: the files the caller determined changed.
    #[serde(default)]
    pub files: Vec<String>,
    /// Empty runs every inspector.
    #[serde(default)]
    pub inspectors: Vec<InspectorId>,
    /// `"critical"` or `"high"`.
    #[serde(default)]
    pub min_severity: Option<Severity>,
}

/// One scan's result, as the drawer draws it.
#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct InspectorScanResult {
    pub report: InspectionReport,
    /// New, returned and resolved since the previous scan of this project.
    pub changes: Changes,
    /// Findings the user has hidden. Not scored, not shown unless asked for.
    pub ignored: Vec<Finding>,
    /// True when the ledger could not be read or written; the scan still stands, but
    /// "resolved on 10 Sep" will not survive a restart. Never silently false.
    pub memory_unavailable: bool,
}

/// The card the user reads before anything is written (§17).
#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct FixPreview {
    pub finding_id: String,
    pub title: String,
    pub summary: String,
    pub risk: FixRisk,
    /// One line per action, in order.
    pub steps: Vec<String>,
    pub files: Vec<String>,
    /// Hand this back to `inspector_apply_fix`. It is only valid for this exact preview.
    pub token: String,
}

// ── commands ─────────────────────────────────────────────────────────────────────────

/// The Inspector rail (§11). Static: the UI draws this and invents no row.
#[tauri::command]
#[specta::specta]
#[must_use]
pub fn inspector_rail() -> Vec<InspectorCard> {
    inspect::registry()
}

/// Run an inspection and remember what it found.
#[tauri::command]
#[specta::specta]
pub async fn inspector_scan(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, InspectorStore>,
    project: String,
    request: InspectRequest,
) -> Result<InspectorScanResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let command = build_command(&request)?;
    let now = chrono::Utc::now().to_rfc3339();

    // The scan is file IO and CPU. Neither belongs on the async runtime (R6).
    let scan_root = root.clone();
    let scan_command = command.clone();
    let scan_now = now.clone();
    let (report, ledger, memory_unavailable) = tokio::task::spawn_blocking(move || {
        let (ledger, readable) = read_ledger(&scan_root);
        let report = inspect::run(&scan_root, &scan_command, None, &scan_now);
        (report, ledger, !readable)
    })
    .await
    .map_err(|error| AppError {
        message: format!("The inspection did not finish: {error}"),
        hint: Some("Try the scan again.".to_owned()),
    })?;

    let reconciled = inspect::reconcile(&ledger, report.findings.clone(), &now);
    let written = write_ledger(&root, &reconciled.ledger);

    // The score is over what is actually reported, so hiding a finding hides its penalty.
    let mut report = report;
    report.findings = reconciled.findings.clone();
    report.health = inspect::HealthReport::compute(
        &report.findings,
        &report
            .health
            .dimensions
            .iter()
            .map(|dimension| (dimension.inspector, dimension.coverage.clone()))
            .collect::<Vec<_>>(),
    );

    let result = InspectorScanResult {
        report: report.clone(),
        changes: reconciled.changes.clone(),
        ignored: reconciled.ignored.clone(),
        memory_unavailable: memory_unavailable || !written,
    };

    let mut sessions = lock(&store)?;
    let session = sessions.entry(&key(&root));
    session.report = Some(report);
    session.ignored = reconciled.ignored;
    session.ledger = reconciled.ledger;
    session.changes = reconciled.changes;
    // A new scan invalidates every outstanding approval: the project has been re-read, and
    // a token minted against the old bytes must not apply to the new ones.
    session.approvals.clear();

    Ok(result)
}

/// The most recent scan, without running another one.
#[tauri::command]
#[specta::specta]
pub async fn inspector_last_report(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, InspectorStore>,
    project: String,
) -> Result<Option<InspectionReport>, AppError> {
    let root = resolve_project(&state, &project).await?;
    let sessions = lock(&store)?;
    Ok(sessions
        .get(&key(&root))
        .and_then(|session| session.report.clone()))
}

/// Show the exact change a fix would make. **Mints the approval token.**
///
/// This is the only place a token is created, which is what makes "the user saw it" a
/// precondition of applying rather than a convention.
#[tauri::command]
#[specta::specta]
pub async fn inspector_preview_fix(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, InspectorStore>,
    project: String,
    finding_id: String,
) -> Result<FixPreview, AppError> {
    let root = resolve_project(&state, &project).await?;
    let mut sessions = lock(&store)?;
    let session = sessions.entry(&key(&root));
    let finding = session
        .report
        .as_ref()
        .and_then(|report| report.finding(&finding_id))
        .cloned()
        .ok_or_else(|| AppError {
            message: format!("Finding {finding_id} is not in the last scan."),
            hint: Some("Run the inspection again and open the finding from the drawer.".to_owned()),
        })?;
    let fix = finding.fix.clone().ok_or_else(|| AppError {
        message: format!("{} has no automatic fix.", finding.code),
        hint: Some(
            "Follow the recommendation by hand, or send the finding to an agent as a task."
                .to_owned(),
        ),
    })?;

    session.approvals.insert(
        fix.token.clone(),
        ApprovedFix {
            finding_id: finding.id.clone(),
            code: finding.code.clone(),
            label: format!("Inspector fix · {}", finding.code),
            fix: fix.clone(),
        },
    );

    Ok(FixPreview {
        finding_id: finding.id,
        title: finding.title,
        summary: fix.summary,
        risk: fix.risk,
        steps: fix.steps,
        files: fix.files,
        token: fix.token,
    })
}

/// Apply a fix the user has approved.
///
/// Refuses a token this session did not issue, and a token issued for a different finding.
/// The batch is the one the preview showed — the caller cannot supply actions.
#[tauri::command]
#[specta::specta]
pub async fn inspector_apply_fix(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, InspectorStore>,
    project: String,
    finding_id: String,
    token: String,
) -> Result<GodotBatchResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let approved = {
        let sessions = lock(&store)?;
        sessions
            .get(&key(&root))
            .and_then(|session| session.approvals.get(&token).cloned())
    };
    let approved = approved.ok_or_else(|| unapproved(&finding_id))?;
    if approved.finding_id != finding_id {
        return Err(unapproved(&finding_id));
    }
    // Belt and braces: the token is content-addressed, so recomputing it from the stored
    // code and actions must reproduce it exactly. A mismatch means the stored fix is not
    // the one the token was minted for, and nothing is written.
    if inspect::fix_token(&approved.code, &approved.fix.actions) != token {
        return Err(unapproved(&finding_id));
    }

    let batch = GodotActionBatch::new(approved.label.clone(), approved.fix.actions.clone());
    let result = apply_batch_for(
        GodotApplyHost { app: Some(&app) },
        &root,
        &batch,
        // The user pressed Apply. This is a person's change, and the project's `[agent]`
        // policy is there to bind the agent, not the person it protects.
        "user",
    )
    .await
    .map_err(|failure| failure.error)?;

    // An applied fix is spent: the same token must not apply twice.
    if let Ok(mut sessions) = lock(&store) {
        sessions.entry(&key(&root)).approvals.remove(&token);
    }
    Ok(result)
}

/// Stop (or resume) reporting one finding.
#[tauri::command]
#[specta::specta]
pub async fn inspector_set_ignored(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, InspectorStore>,
    project: String,
    finding_id: String,
    ignored: bool,
) -> Result<bool, AppError> {
    let root = resolve_project(&state, &project).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut sessions = lock(&store)?;
    let session = sessions.entry(&key(&root));

    let code = session
        .report
        .as_ref()
        .and_then(|report| report.finding(&finding_id))
        .map(|finding| finding.code.clone())
        .or_else(|| {
            session
                .ignored
                .iter()
                .find(|finding| finding.id == finding_id)
                .map(|finding| finding.code.clone())
        })
        .unwrap_or_default();

    if ignored {
        session.ledger.ignore(&finding_id, &code, &now);
    } else {
        session.ledger.unignore(&finding_id);
    }
    let written = write_ledger(&root, &session.ledger);
    Ok(written)
}

/// The task text a *Send to agent* button hands to a normal Bhippi agent (§18).
#[tauri::command]
#[specta::specta]
pub async fn inspector_agent_task(
    state: tauri::State<'_, crate::Runtime>,
    store: tauri::State<'_, InspectorStore>,
    project: String,
    finding_id: String,
) -> Result<String, AppError> {
    let root = resolve_project(&state, &project).await?;
    let sessions = lock(&store)?;
    let finding = sessions
        .get(&key(&root))
        .and_then(|session| session.report.as_ref())
        .and_then(|report| report.finding(&finding_id))
        .ok_or_else(|| AppError {
            message: format!("Finding {finding_id} is not in the last scan."),
            hint: Some("Run the inspection again first.".to_owned()),
        })?;
    Ok(inspect::agent_task(finding))
}

// ── plumbing ─────────────────────────────────────────────────────────────────────────

fn unapproved(finding_id: &str) -> AppError {
    AppError {
        message: format!("Nothing has been approved for {finding_id}."),
        hint: Some(
            "Open the fix preview and press Apply fix there. A fix is only applied from the \
             preview that showed it."
                .to_owned(),
        ),
    }
}

fn key(root: &Path) -> String {
    root.to_string_lossy().replace('\\', "/").to_lowercase()
}

/// Turn the webview's request into the engine's command.
pub fn build_command(request: &InspectRequest) -> Result<InspectCommand, AppError> {
    let scope = match request.scope.as_deref().unwrap_or("project") {
        "project" => InspectScope::Project,
        "level" => {
            let scene = request.scene.clone().ok_or_else(|| AppError {
                message: "A level scan needs a scene.".to_owned(),
                hint: Some("Open a scene in the viewport first.".to_owned()),
            })?;
            InspectScope::Level {
                scene: bhippi_engine::godot::res_to_rel(&scene),
            }
        }
        "selection" => {
            if request.scene.is_none()
                && request.file.is_none()
                && request.asset.is_none()
                && request.node.is_none()
            {
                return Err(AppError {
                    message: "Nothing is selected.".to_owned(),
                    hint: Some(
                        "Select a node, a script or an asset, or inspect the whole project."
                            .to_owned(),
                    ),
                });
            }
            InspectScope::Selection {
                scene: request
                    .scene
                    .as_deref()
                    .map(bhippi_engine::godot::res_to_rel),
                node: request.node.clone(),
                file: request
                    .file
                    .as_deref()
                    .map(bhippi_engine::godot::res_to_rel),
                asset: request
                    .asset
                    .as_deref()
                    .map(bhippi_engine::godot::res_to_rel),
            }
        }
        "changes" => {
            if request.files.is_empty() {
                return Err(AppError {
                    message: "Nothing has changed.".to_owned(),
                    hint: Some("There is nothing to compare against.".to_owned()),
                });
            }
            InspectScope::Changes {
                files: request
                    .files
                    .iter()
                    .map(|file| bhippi_engine::godot::res_to_rel(file))
                    .collect(),
            }
        }
        other => {
            return Err(AppError {
                message: format!("{other} is not an inspection scope."),
                hint: Some("Use project, level, selection or changes.".to_owned()),
            })
        }
    };
    Ok(InspectCommand {
        scope,
        inspectors: request.inspectors.clone(),
        min_severity: request.min_severity,
    })
}

fn ledger_path(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR).join(LEDGER_FILE)
}

/// `(ledger, was readable)`. An unreadable or foreign-schema ledger starts empty and says
/// so — a scan that silently forgot every "ignore" would be worse than one that admits it.
fn read_ledger(root: &Path) -> (FindingLedger, bool) {
    let path = ledger_path(root);
    if !path.exists() {
        return (FindingLedger::default(), true);
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<FindingLedger>(&text) {
            Ok(ledger) if ledger.schema == inspect::memory::LEDGER_SCHEMA => (ledger, true),
            Ok(ledger) => {
                tracing::warn!(schema = %ledger.schema, "inspector ledger has a schema this build does not read");
                (FindingLedger::default(), false)
            }
            Err(error) => {
                tracing::warn!(%error, "inspector ledger did not parse");
                (FindingLedger::default(), false)
            }
        },
        Err(error) => {
            tracing::warn!(%error, "inspector ledger could not be read");
            (FindingLedger::default(), false)
        }
    }
}

/// True when it landed. A project on a read-only volume is inspectable; it just cannot
/// remember, and the scan result says so rather than pretending.
fn write_ledger(root: &Path, ledger: &FindingLedger) -> bool {
    let path = ledger_path(root);
    let Some(parent) = path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    match serde_json::to_string_pretty(ledger) {
        Ok(text) => std::fs::write(&path, text).is_ok(),
        Err(error) => {
            tracing::warn!(%error, "inspector ledger could not be serialised");
            false
        }
    }
}

// ── the chat command (§30) ───────────────────────────────────────────────────────────

/// Run `/inspect …` and render the answer as markdown.
///
/// Deliberately independent of the session store: a slash command is a question, answered
/// from files, with **no provider and no tokens** — it works offline and with no AI
/// configured at all, which is the same promise `/gamedebug` makes.
///
/// # Errors
/// When the command does not parse, or names a scope the studio cannot satisfy.
pub fn run_chat_command(
    project_root: &Path,
    text: &str,
    context: &bhippi_engine::inspect::CommandContext,
    now: &str,
) -> Result<String, AppError> {
    let command =
        bhippi_engine::inspect::parse_command(text, context).map_err(|error| AppError {
            hint: error.hint().map(str::to_owned),
            message: error.to_string(),
        })?;
    let report = inspect::run(project_root, &command, None, now);
    Ok(render_report(&report))
}

/// One inspection, as the transcript prints it.
///
/// Compact on purpose: a wall of findings in a chat transcript is unreadable and un-actionable
/// — the drawer is where a person works through them. This is the summary plus the worst few,
/// and it says how many it did not print.
#[must_use]
pub fn render_report(report: &InspectionReport) -> String {
    const PRINTED: usize = 12;
    let counts = report.health.counts;
    let newline = "\n";
    let mut out = String::new();
    out.push_str(&format!(
        "### Inspector · {}{newline}{newline}",
        report.scope.label()
    ));

    match report.health.score {
        Some(score) => out.push_str(&format!(
            "**Project health {score} / 100**{newline}{newline}"
        )),
        None => out.push_str(&format!(
            "**Project health — not enough was scanned to score.**{newline}{newline}"
        )),
    }
    if let Some(line) = &report.health.incomplete_line {
        out.push_str(&format!("{line}{newline}{newline}"));
    }

    out.push_str(&format!(
        "{} finding(s) · Critical {} · High {} · Medium {} · Low {} · Suggestion {}{newline}{newline}",
        counts.total, counts.critical, counts.high, counts.medium, counts.low, counts.suggestion
    ));

    if report.findings.is_empty() {
        out.push_str(&format!(
            "Nothing found. That is a real answer, not an empty screen.{newline}"
        ));
        return out;
    }

    out.push_str(&format!(
        "| Severity | Finding | Where | Confidence |{newline}|---|---|---|---|{newline}"
    ));
    for finding in report.findings.iter().take(PRINTED) {
        // A pipe inside a title would end the cell early. `&#124;` rather than a backslash
        // escape: the transcript renders markdown through a sanitiser, and the entity
        // survives both it and a table cell.
        let title = finding.title.replace('|', "&#124;");
        out.push_str(&format!(
            "| {} | {title} | `{}` | {}% |{newline}",
            finding.severity.label(),
            finding.where_label,
            finding.confidence
        ));
    }
    if report.findings.len() > PRINTED {
        out.push_str(&format!(
            "{newline}{} more in the Inspector drawer.{newline}",
            report.findings.len() - PRINTED
        ));
    }
    for note in &report.truncated {
        out.push_str(&format!("{newline}_{note}_{newline}"));
    }
    out.push_str(&format!(
        "{newline}Nothing was changed. Open the Inspector drawer to read a finding in full, \
         or to preview a fix.{newline}"
    ));
    out
}

/// A measurement from a playtest, in the shape the Performance Inspector accepts.
///
/// Kept here rather than in the engine because "what counts as a run" is the app's
/// knowledge: the engine is handed the numbers, never asked to produce them.
#[must_use]
pub fn evidence_from_playtest(
    frames: Option<u64>,
    elapsed_ms: u64,
    scene: Option<String>,
    captured_at: &str,
) -> PerformanceEvidence {
    let mut evidence = PerformanceEvidence::new("headless playtest", captured_at);
    if let Some(frames) = frames {
        evidence = evidence.with_frames(frames, elapsed_ms);
    }
    if let Some(scene) = scene {
        evidence = evidence.in_scene(scene);
    }
    evidence
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhippi_engine::inspect::Location;

    fn temp_root(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("bhippi_inspector_cmd_{name}_{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&dir).expect("the temp dir is created");
        dir
    }

    fn finding_with_fix() -> Finding {
        let fix = ProposedFix::new(
            "BHP-INS-803",
            "Turn monitoring on for `Trigger`",
            FixRisk::Low,
            vec![bhippi_engine::godot::action::GodotAction::SetProperty {
                scene: "scenes/main.tscn".to_owned(),
                path: "Trigger".to_owned(),
                property: "monitoring".to_owned(),
                value: bhippi_engine::godot::tscn::TscnValue::Bool(true),
            }],
        )
        .expect("the fix is well formed");
        Finding::draft(
            InspectorId::Physics,
            "BHP-INS-803",
            Severity::Medium,
            100,
            "`Trigger` is not monitoring",
            Location::node("scenes/main.tscn", "Trigger"),
        )
        .cause("monitoring = false")
        .impact("the area detects nothing")
        .recommend("set monitoring = true")
        .fix(fix)
        .build()
        .expect("the draft is complete")
    }

    #[test]
    fn a_scope_the_request_cannot_satisfy_is_refused_rather_than_widened() {
        let level = build_command(&InspectRequest {
            scope: Some("level".to_owned()),
            ..InspectRequest::default()
        });
        assert!(level.is_err(), "a level scan with no scene is refused");

        let selection = build_command(&InspectRequest {
            scope: Some("selection".to_owned()),
            ..InspectRequest::default()
        });
        assert!(
            selection.is_err(),
            "a selection scan with nothing selected is refused"
        );

        let changes = build_command(&InspectRequest {
            scope: Some("changes".to_owned()),
            ..InspectRequest::default()
        });
        assert!(
            changes.is_err(),
            "a changes scan with no changes is refused"
        );

        let nonsense = build_command(&InspectRequest {
            scope: Some("sideways".to_owned()),
            ..InspectRequest::default()
        });
        assert!(nonsense.is_err());
    }

    #[test]
    fn a_res_path_and_a_relative_path_resolve_to_the_same_scope() {
        let from_res = build_command(&InspectRequest {
            scope: Some("level".to_owned()),
            scene: Some("res://scenes/main.tscn".to_owned()),
            ..InspectRequest::default()
        })
        .expect("it builds");
        let from_rel = build_command(&InspectRequest {
            scope: Some("level".to_owned()),
            scene: Some("scenes/main.tscn".to_owned()),
            ..InspectRequest::default()
        })
        .expect("it builds");
        assert_eq!(from_res.scope, from_rel.scope);
    }

    #[test]
    fn a_default_request_is_a_whole_project_scan_with_every_inspector() {
        let command = build_command(&InspectRequest::default()).expect("it builds");
        assert_eq!(command.scope, InspectScope::Project);
        assert!(command.inspectors.is_empty());
        assert_eq!(command.min_severity, None);
    }

    #[test]
    fn the_ledger_round_trips_through_the_project_state_directory() {
        let root = temp_root("ledger");
        let (empty, readable) = read_ledger(&root);
        assert!(readable, "a project with no ledger yet is not an error");
        assert!(empty.entries.is_empty());

        let mut ledger = FindingLedger::default();
        ledger.ignore("f_abc", "BHP-INS-203", "2026-09-10T00:00:00Z");
        assert!(write_ledger(&root, &ledger));

        let (read_back, readable) = read_ledger(&root);
        assert!(readable);
        assert_eq!(read_back, ledger);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_ledger_from_a_schema_this_build_does_not_read_starts_empty_and_reports_it() {
        let root = temp_root("foreign");
        std::fs::create_dir_all(root.join(LEDGER_DIR)).expect("the state dir is created");
        std::fs::write(
            ledger_path(&root),
            r#"{"schema":"bhippi-inspect-ledger@99","entries":[]}"#,
        )
        .expect("the fixture is written");

        let (ledger, readable) = read_ledger(&root);
        assert!(ledger.entries.is_empty());
        assert!(!readable, "an unreadable ledger is reported, not hidden");

        std::fs::remove_dir_all(&root).ok();
    }

    /// The gate, stated as a test: a token is only ever created by a preview.
    #[test]
    fn a_fix_is_only_applicable_through_the_token_its_preview_issued() {
        let mut session = InspectorSession::default();
        let finding = finding_with_fix();
        let fix = finding.fix.clone().expect("the finding carries a fix");

        // Nothing approved yet: no token in the session at all.
        assert!(session.approvals.is_empty());
        assert!(!session.approvals.contains_key(&fix.token));

        // The preview is what registers it.
        session.approvals.insert(
            fix.token.clone(),
            ApprovedFix {
                finding_id: finding.id.clone(),
                code: finding.code.clone(),
                label: "Inspector fix".to_owned(),
                fix: fix.clone(),
            },
        );
        let approved = session
            .approvals
            .get(&fix.token)
            .expect("the preview registered it");
        assert_eq!(approved.finding_id, finding.id);
        assert_eq!(approved.fix.actions, fix.actions);

        // A token nobody issued is not in the map, whatever it claims to be for.
        assert!(!session
            .approvals
            .contains_key("fix_deadbeefdeadbeefdeadbeef"));
    }

    #[test]
    fn a_token_is_bound_to_its_own_actions_so_a_changed_fix_needs_a_new_preview() {
        let finding = finding_with_fix();
        let fix = finding.fix.clone().expect("the finding carries a fix");
        let elsewhere = ProposedFix::new(
            "BHP-INS-803",
            "Turn monitoring on for `OtherTrigger`",
            FixRisk::Low,
            vec![bhippi_engine::godot::action::GodotAction::SetProperty {
                scene: "scenes/main.tscn".to_owned(),
                path: "OtherTrigger".to_owned(),
                property: "monitoring".to_owned(),
                value: bhippi_engine::godot::tscn::TscnValue::Bool(true),
            }],
        )
        .expect("the fix is well formed");
        assert_ne!(fix.token, elsewhere.token);
    }

    #[test]
    fn the_token_recompute_the_apply_path_runs_reproduces_the_preview_token() {
        let finding = finding_with_fix();
        let fix = finding.fix.clone().expect("the finding carries a fix");
        assert_eq!(
            bhippi_engine::inspect::fix_token(&finding.code, &fix.actions),
            fix.token,
            "apply recomputes the token from (code, actions); it must match the preview's"
        );
    }

    #[test]
    fn the_error_a_missing_approval_produces_tells_the_user_what_to_press() {
        let error = unapproved("f_abc");
        assert!(error.message.contains("f_abc"));
        let hint = error.hint.unwrap_or_default();
        assert!(hint.contains("Apply fix"));
    }

    #[test]
    fn playtest_numbers_become_evidence_and_a_run_that_counted_nothing_stays_empty() {
        let measured = evidence_from_playtest(
            Some(600),
            12_000,
            Some("scenes/main.tscn".to_owned()),
            "2026-09-10T00:00:00Z",
        );
        assert_eq!(measured.frame_ms(), Some(20.0));
        assert_eq!(measured.scene.as_deref(), Some("scenes/main.tscn"));

        let unmeasured = evidence_from_playtest(None, 12_000, None, "2026-09-10T00:00:00Z");
        assert_eq!(unmeasured.frame_ms(), None);
    }
}
