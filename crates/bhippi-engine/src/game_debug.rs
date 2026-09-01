//! Fixed, engine-owned game diagnostic pipeline (`/gamedebug`).
//!
//! This is intentionally separate from the general workspace debugger. A compiler-clean
//! repository is not proof that its game manifest, scenes, assets or gameplay scripts form a
//! valid game. Every report contains the same ordered stage ids so callers and future AI repair
//! turns cannot skip work or turn an unavailable runtime check into a pass.

use crate::asset::AssetIndex;
use crate::document::SceneDocument;
use crate::error::{EngineError, Result};
use crate::gates::{self, GateLevel};
use crate::manifest::load_manifest;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const REPORT_SCHEMA: &str = "bhippi-game-debug@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GameDebugMode {
    Quick,
    Full,
    Release,
}

impl GameDebugMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Full => "full",
            Self::Release => "release",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Passed,
    Failed,
    Skipped,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GameDebugStage {
    pub id: String,
    pub label: String,
    pub status: StageStatus,
    pub summary: String,
    /// Monotonic wall-clock time spent in this stage. Skipped stages are zero.
    #[serde(default)]
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GameDebugFinding {
    pub code: String,
    pub severity: String,
    pub stage: String,
    pub address: String,
    pub message: String,
    pub evidence: String,
    pub reproduction: String,
    pub repair: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EvaluationStatus {
    pub status: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GameDebugReport {
    pub schema: String,
    pub run_id: String,
    pub mode: GameDebugMode,
    pub project: String,
    pub started_at: String,
    pub authored_tree_before: String,
    pub authored_tree_after: String,
    pub stages: Vec<GameDebugStage>,
    pub findings: Vec<GameDebugFinding>,
    pub quality: EvaluationStatus,
    pub sandbox: EvaluationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<GameDebugRuntimeEvidence>,
    pub artifacts: Vec<String>,
    pub repair_batch_id: Option<String>,
    pub outcome: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameDebugRuntimeEvidence {
    pub protocol: String,
    pub execution: String,
    pub capabilities: Vec<crate::runtime_protocol::RuntimeCapability>,
    pub budgets: GameDebugWorkerBudgets,
    pub termination_reason: String,
    pub authored_hash_before: String,
    pub authored_hash_after: String,
    pub frames: u64,
    pub checkpoint_hashes: Vec<String>,
    pub fault_count: usize,
    pub trace: GameDebugRuntimeTrace,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameDebugRuntimeTrace {
    pub entries: Vec<GameDebugRuntimeTraceEntry>,
    pub truncated: bool,
    pub redactions: u64,
    pub usage: GameDebugRuntimeUsage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameDebugRuntimeTraceEntry {
    pub kind: String,
    pub capability: Option<crate::runtime_protocol::RuntimeCapability>,
    pub decision: Option<String>,
    pub subject: Option<String>,
    pub line: Option<u64>,
    pub instruction: Option<u64>,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameDebugRuntimeUsage {
    pub instructions: u64,
    pub messages: u64,
    pub spawned_entities: u64,
    pub emitted_events: u64,
    pub log_bytes: u64,
    pub timers: u64,
    pub heap_estimate_bytes: u64,
    pub wall_clock_millis: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameDebugWorkerBudgets {
    pub instructions_per_tick: u64,
    pub instructions_total: u64,
    pub call_depth: u64,
    pub timers: u64,
    pub heap_estimate_bytes: u64,
    pub wall_clock_millis: u64,
    pub message_bytes: u64,
    pub messages_per_tick: u64,
    pub spawned_entities: u64,
    pub emitted_events: u64,
    pub log_bytes: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerPlaytestEvidence {
    authored_unchanged: bool,
    authored_hash_before: String,
    authored_hash_after: String,
    completed: bool,
    frames: u64,
    samples: Vec<WorkerCheckpointEvidence>,
    faults: Vec<serde_json::Value>,
    sandbox: WorkerSandboxEvidence,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerCheckpointEvidence {
    checkpoint_hash: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerSandboxEvidence {
    protocol: String,
    execution: String,
    capabilities: Vec<crate::runtime_protocol::RuntimeCapability>,
    budgets: WorkerBudgetEvidence,
    termination_reason: String,
    trace: WorkerTraceEvidence,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerTraceEvidence {
    entries: Vec<WorkerTraceEntryEvidence>,
    truncated: bool,
    redactions: u64,
    usage: WorkerUsageEvidence,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerTraceEntryEvidence {
    kind: String,
    capability: Option<crate::runtime_protocol::RuntimeCapability>,
    decision: Option<String>,
    subject: Option<String>,
    line: Option<u64>,
    instruction: Option<u64>,
    message: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerUsageEvidence {
    instructions: u64,
    messages: u64,
    spawned_entities: u64,
    emitted_events: u64,
    log_bytes: u64,
    timers: u64,
    heap_estimate_bytes: u64,
    wall_clock_millis: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkerBudgetEvidence {
    instructions_per_tick: u64,
    instructions_total: u64,
    call_depth: u64,
    timers: u64,
    heap_estimate_bytes: u64,
    wall_clock_millis: u64,
    message_bytes: u64,
    messages_per_tick: u64,
    spawned_entities: u64,
    emitted_events: u64,
    log_bytes: u64,
}

impl GameDebugReport {
    #[must_use]
    pub fn authored_tree_unchanged(&self) -> bool {
        self.authored_tree_before == self.authored_tree_after
    }

    pub fn parse(text: &str) -> Result<Self> {
        let report: Self = serde_json::from_str(text).map_err(|error| {
            report_error(
                &format!("invalid game-debug report: {error}"),
                &format!("Fix the JSON and keep schema {REPORT_SCHEMA}."),
            )
        })?;
        report.validate()?;
        Ok(report)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema != REPORT_SCHEMA {
            return Err(report_error(
                &format!("unsupported game-debug schema {:?}", self.schema),
                &format!("Use schema {REPORT_SCHEMA}."),
            ));
        }
        if self.run_id.parse::<ulid::Ulid>().is_err() {
            return Err(report_error(
                "game-debug run_id is not a ULID",
                "Use the immutable ULID allocated when the run starts.",
            ));
        }
        if chrono::DateTime::parse_from_rfc3339(&self.started_at).is_err() {
            return Err(report_error(
                "game-debug started_at is not RFC 3339",
                "Store the UTC run start as an RFC 3339 timestamp.",
            ));
        }
        let expected_ids = STAGES.map(|(id, _)| id);
        let actual_ids = self
            .stages
            .iter()
            .map(|stage| stage.id.as_str())
            .collect::<Vec<_>>();
        if actual_ids != expected_ids {
            return Err(report_error(
                "game-debug stages are missing, duplicated or out of canonical order",
                "Emit every fixed stage exactly once in registry order.",
            ));
        }
        for (stage, (_, label)) in self.stages.iter().zip(STAGES) {
            if stage.label != label || stage.summary.trim().is_empty() {
                return Err(report_error(
                    &format!("game-debug stage {} has invalid display data", stage.id),
                    "Use the registry label and a non-empty evidence summary.",
                ));
            }
        }
        for finding in &self.findings {
            if !matches!(finding.severity.as_str(), "blocker" | "warning" | "info")
                || finding.code.trim().is_empty()
                || finding.address.trim().is_empty()
                || finding.message.trim().is_empty()
                || finding.evidence.trim().is_empty()
                || finding.reproduction.trim().is_empty()
                || finding.repair.trim().is_empty()
                || !expected_ids.contains(&finding.stage.as_str())
            {
                return Err(report_error(
                    "a game-debug finding has invalid severity, stage or evidence fields",
                    "Use a stable code, canonical stage, severity and complete evidence/reproduction/repair text.",
                ));
            }
        }
        if let Some(runtime) = &self.runtime {
            runtime.validate(self.mode)?;
        }
        let mut sorted = self.findings.clone();
        sort_findings(&mut sorted);
        if sorted != self.findings {
            return Err(report_error(
                "game-debug findings are not in canonical order",
                "Sort findings by severity, stage, address and code before serialising.",
            ));
        }
        let expected_outcome = report_outcome(self);
        if self.outcome != expected_outcome {
            return Err(report_error(
                &format!(
                    "game-debug outcome {:?} contradicts evidence; expected {expected_outcome:?}",
                    self.outcome
                ),
                "Recompute the outcome from stage status, blockers and authored-tree hashes.",
            ));
        }
        Ok(())
    }

    pub fn dump(&self) -> Result<String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| {
            report_error(
                &format!("cannot serialise game-debug report: {error}"),
                "Report this as an engine bug.",
            )
        })
    }
}

const STAGES: [(&str, &str); 9] = [
    ("01_discover", "Discover game"),
    ("02_validate", "Validate authored content"),
    ("03_compile", "Compile gameplay scripts"),
    ("04_sandbox", "Verify runtime sandbox"),
    ("05_exercise", "Exercise gameplay"),
    ("06_inspect", "Inspect game state"),
    ("07_observe", "Capture runtime evidence"),
    ("08_score", "Score generation quality"),
    ("09_report", "Build canonical report"),
];

/// Run the immutable portion of the fixed game-debug pipeline.
///
/// `quick` implements discovery, content validation, script compilation and structural
/// inspection. Runtime exercise, independent quality scoring and hostile sandbox execution
/// remain explicit `unsupported` stages for `full`/`release`; they are never reported as a
/// pass until the later phase supplies real evidence.
#[must_use]
pub fn run(project_root: &Path, mode: GameDebugMode) -> GameDebugReport {
    let started_at = chrono::Utc::now().to_rfc3339();
    let before = authored_tree_hash(project_root);
    let mut stages = STAGES
        .iter()
        .map(|(id, label)| GameDebugStage {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
            status: StageStatus::Skipped,
            summary: "not reached".to_owned(),
            duration_ms: 0,
        })
        .collect::<Vec<_>>();
    let mut findings = Vec::new();

    let stage_started = Instant::now();
    let manifest = match load_manifest(project_root) {
        Ok(Some(manifest)) => {
            set_stage(
                &mut stages,
                "01_discover",
                StageStatus::Passed,
                "Bhippi.game.toml loaded",
                elapsed_ms(stage_started),
            );
            Some(manifest)
        }
        Ok(None) => {
            set_stage(
                &mut stages,
                "01_discover",
                StageStatus::Failed,
                "Bhippi.game.toml is missing",
                elapsed_ms(stage_started),
            );
            findings.push(finding(
                "BHP-GD-001",
                "blocker",
                "01_discover",
                "Bhippi.game.toml",
                "This folder is not a Bhippi game project.",
                "The project root has no Bhippi.game.toml file.",
                "Run `/gamedebug quick` from the same folder.",
                "Open a game project or create one with New Game.",
            ));
            None
        }
        Err(error) => {
            set_stage(
                &mut stages,
                "01_discover",
                StageStatus::Failed,
                "Bhippi.game.toml could not be parsed",
                elapsed_ms(stage_started),
            );
            findings.push(finding(
                "BHP-GD-002",
                "blocker",
                "01_discover",
                "Bhippi.game.toml",
                "The game manifest is invalid.",
                &error.to_string(),
                "Run `/gamedebug quick` from this project.",
                error.hint().unwrap_or("Fix the manifest syntax and retry."),
            ));
            None
        }
    };

    if let Some(manifest) = manifest {
        let stage_started = Instant::now();
        let (scenes, scene_failures) = load_scenes(project_root);
        findings.extend(scene_failures);

        let mut gate_report = gates::check_project(project_root, &manifest, &scenes);
        gate_report
            .findings
            .extend(gates::check_authored_documents(project_root).findings);
        match AssetIndex::scan(project_root) {
            Ok(mut index) => {
                index.refresh_usage(&scenes.iter().map(|(_, scene)| scene).collect::<Vec<_>>());
                gate_report.findings.extend(
                    gates::check_assets(&index, &scenes, mode == GameDebugMode::Release).findings,
                );
            }
            Err(error) => findings.push(finding(
                "BHP-GD-120",
                "blocker",
                "02_validate",
                "assets/",
                "The asset index could not be built.",
                &error.to_string(),
                "Run `/gamedebug quick` from this project.",
                error
                    .hint()
                    .unwrap_or("Repair the unreadable asset or sidecar."),
            )),
        }

        for item in gate_report.findings {
            findings.push(finding(
                &format!("BHP-GATE-{}", item.code.to_ascii_uppercase()),
                match item.level {
                    GateLevel::Blocker => "blocker",
                    GateLevel::Warning => "warning",
                },
                "02_validate",
                &item.where_,
                &item.message,
                &item.message,
                &format!("Run `/gamedebug {}` from this project.", mode.as_str()),
                &item.hint,
            ));
        }

        let validate_failed = findings
            .iter()
            .any(|item| item.stage == "02_validate" && item.severity == "blocker");
        set_stage(
            &mut stages,
            "02_validate",
            if validate_failed {
                StageStatus::Failed
            } else {
                StageStatus::Passed
            },
            if validate_failed {
                "authored content has blocking findings"
            } else {
                "manifest, scenes and asset references passed static gates"
            },
            elapsed_ms(stage_started),
        );

        let stage_started = Instant::now();
        let compiled_scripts = compile_scripts(project_root, &mut findings);
        let compile_failed = findings
            .iter()
            .any(|item| item.stage == "03_compile" && item.severity == "blocker");
        set_stage(
            &mut stages,
            "03_compile",
            if compile_failed {
                StageStatus::Failed
            } else {
                StageStatus::Passed
            },
            if compile_failed {
                "one or more gameplay scripts did not compile"
            } else {
                "all discovered .rhai gameplay scripts compiled"
            },
            elapsed_ms(stage_started),
        );
        let stage_started = Instant::now();
        if !validate_failed && !compile_failed {
            let huds = load_hud_documents(project_root);
            let input = load_input_document(project_root);
            let input_ref = input
                .as_ref()
                .map(|(path, document)| (path.as_str(), document));
            findings.extend(
                crate::game_inspector::inspect(
                    &manifest,
                    &scenes,
                    &huds,
                    input_ref,
                    &compiled_scripts,
                )
                .into_iter()
                .map(|item| {
                    finding(
                        &item.code,
                        item.severity.as_str(),
                        "06_inspect",
                        &item.address,
                        &item.observed,
                        &format!("Observed: {} Expected: {}", item.observed, item.expected),
                        &format!("Run `/gamedebug {}` from this project.", mode.as_str()),
                        &item.repair,
                    )
                }),
            );
        }
        let inspect_failed = validate_failed
            || compile_failed
            || findings
                .iter()
                .any(|item| item.stage == "06_inspect" && item.severity == "blocker");
        set_stage(
            &mut stages,
            "06_inspect",
            if inspect_failed {
                StageStatus::Failed
            } else {
                StageStatus::Passed
            },
            if validate_failed || compile_failed {
                "semantic inspection could not prove the game graph because an earlier stage failed"
            } else if inspect_failed {
                "semantic game-graph inspection found a blocking defect"
            } else {
                "level, play entry, input, HUD, objective, dependency and script-flow inspection complete"
            },
            elapsed_ms(stage_started),
        );
    }

    let runtime_requested = mode != GameDebugMode::Quick;
    for id in ["04_sandbox", "05_exercise", "07_observe", "08_score"] {
        set_stage(
            &mut stages,
            id,
            if runtime_requested {
                StageStatus::Unsupported
            } else {
                StageStatus::Skipped
            },
            if runtime_requested {
                "runtime evidence is not implemented yet; this stage did not pass"
            } else {
                "quick mode does not select this runtime stage"
            },
            0,
        );
    }
    let stage_started = Instant::now();
    set_stage(
        &mut stages,
        "09_report",
        StageStatus::Passed,
        "canonical in-memory report built",
        elapsed_ms(stage_started),
    );

    sort_findings(&mut findings);
    let after = authored_tree_hash(project_root);
    let has_blocker = findings.iter().any(|item| item.severity == "blocker");
    let unsupported = stages
        .iter()
        .any(|stage| stage.status == StageStatus::Unsupported);
    let outcome = if has_blocker || before != after {
        "failed"
    } else if unsupported {
        "incomplete"
    } else {
        "passed"
    };

    GameDebugReport {
        schema: REPORT_SCHEMA.to_owned(),
        run_id: ulid::Ulid::new().to_string(),
        mode,
        project: project_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("game")
            .to_owned(),
        started_at,
        authored_tree_before: before,
        authored_tree_after: after,
        stages,
        findings,
        quality: EvaluationStatus {
            status: "not_evaluated".to_owned(),
            reason: "Phase 9 quality corpus/evaluator evidence is not wired yet.".to_owned(),
        },
        sandbox: EvaluationStatus {
            status: "not_evaluated".to_owned(),
            reason: "Phase 11 sandbox backend/hostile-corpus evidence is not wired yet.".to_owned(),
        },
        runtime: None,
        artifacts: Vec::new(),
        repair_batch_id: None,
        outcome: outcome.to_owned(),
    }
}

impl GameDebugRuntimeEvidence {
    fn validate(&self, mode: GameDebugMode) -> Result<()> {
        if mode == GameDebugMode::Quick {
            return Err(report_error(
                "quick game-debug report contains runtime evidence",
                "Runtime evidence belongs only to full or release mode.",
            ));
        }
        if self.protocol != crate::runtime_protocol::RUNTIME_PROTOCOL_FORMAT
            || self.execution != "application_module_worker"
            || !matches!(
                self.termination_reason.as_str(),
                "completed" | "runtime_fault"
            )
        {
            return Err(report_error(
                "game-debug runtime evidence has an unsupported sandbox identity",
                "Use the application-owned module worker and the current runtime protocol.",
            ));
        }
        let capabilities = self.capabilities.iter().copied().collect::<BTreeSet<_>>();
        if capabilities.len() != self.capabilities.len()
            || capabilities.into_iter().collect::<Vec<_>>() != self.capabilities
        {
            return Err(report_error(
                "game-debug runtime capabilities are duplicated or not canonical",
                "Store the sorted, deduplicated Rust-derived grant set.",
            ));
        }
        if [
            self.budgets.instructions_per_tick,
            self.budgets.instructions_total,
            self.budgets.call_depth,
            self.budgets.timers,
            self.budgets.heap_estimate_bytes,
            self.budgets.wall_clock_millis,
            self.budgets.message_bytes,
            self.budgets.messages_per_tick,
            self.budgets.spawned_entities,
            self.budgets.emitted_events,
            self.budgets.log_bytes,
        ]
        .contains(&0)
            || self.authored_hash_before.trim().is_empty()
            || self.authored_hash_after.trim().is_empty()
            || self
                .checkpoint_hashes
                .iter()
                .any(|hash| !hash.starts_with("fnv1a32:"))
        {
            return Err(report_error(
                "game-debug runtime evidence has invalid budgets or hashes",
                "Keep every enforced ceiling non-zero and every checkpoint hash canonical.",
            ));
        }
        self.trace.validate(&self.budgets, &self.capabilities)?;
        Ok(())
    }
}

impl GameDebugRuntimeTrace {
    fn validate(
        &self,
        budgets: &GameDebugWorkerBudgets,
        granted: &[crate::runtime_protocol::RuntimeCapability],
    ) -> Result<()> {
        if self.entries.len() > 128
            || self.usage.instructions > budgets.instructions_total
            || self.usage.spawned_entities > budgets.spawned_entities
            || self.usage.emitted_events > budgets.emitted_events
            || self.usage.log_bytes > budgets.log_bytes
            || self.usage.timers > budgets.timers
            || self.usage.heap_estimate_bytes > budgets.heap_estimate_bytes
            || self.usage.wall_clock_millis > budgets.wall_clock_millis
        {
            return Err(report_error(
                "game-debug runtime trace exceeds its declared bounds",
                "Keep trace entries and usage inside the Rust-owned sandbox ceilings.",
            ));
        }
        let mut decisions = BTreeMap::new();
        for entry in &self.entries {
            let shape_ok = match entry.kind.as_str() {
                "capability" => {
                    let decision = entry.decision.as_deref();
                    let capability = entry.capability;
                    if let (Some(capability), Some(decision)) = (capability, decision) {
                        if decisions.insert(capability, decision).is_some() {
                            return Err(report_error(
                                "game-debug runtime trace duplicates a capability decision",
                                "Record exactly one grant or denial for every closed capability.",
                            ));
                        }
                    }
                    capability.is_some()
                        && matches!(decision, Some("granted" | "denied"))
                        && entry.subject.is_none()
                        && entry.line.is_none()
                        && entry.instruction.is_none()
                        && entry.message.is_none()
                }
                "log" => {
                    entry.subject.as_deref().is_some_and(valid_trace_text)
                        && entry.message.as_deref().is_some_and(valid_trace_text)
                }
                "script_fault" => {
                    entry.subject.as_deref().is_some_and(valid_trace_text)
                        && entry.line.is_some()
                        && entry.instruction.is_some()
                        && entry.message.as_deref().is_some_and(valid_trace_text)
                }
                "runtime_fault" | "budget" => {
                    entry.message.as_deref().is_some_and(valid_trace_text)
                }
                _ => false,
            };
            if !shape_ok {
                return Err(report_error(
                    "game-debug runtime trace contains an invalid entry",
                    "Keep trace kinds closed, bounded and structurally complete.",
                ));
            }
        }
        let all_capabilities = [
            crate::runtime_protocol::RuntimeCapability::EntityRead,
            crate::runtime_protocol::RuntimeCapability::EntityWriteRuntime,
            crate::runtime_protocol::RuntimeCapability::InputRead,
            crate::runtime_protocol::RuntimeCapability::HudAction,
            crate::runtime_protocol::RuntimeCapability::LevelTravel,
            crate::runtime_protocol::RuntimeCapability::AudioEvent,
            crate::runtime_protocol::RuntimeCapability::DeterministicTimer,
        ];
        if decisions.len() != all_capabilities.len()
            || all_capabilities.iter().any(|capability| {
                decisions.get(capability).copied()
                    != Some(if granted.contains(capability) {
                        "granted"
                    } else {
                        "denied"
                    })
            })
        {
            return Err(report_error(
                "game-debug runtime trace has incomplete capability decisions",
                "Record one decision for every closed capability and match the Rust grant set.",
            ));
        }
        Ok(())
    }
}

fn valid_trace_text(value: &str) -> bool {
    if value.is_empty() || value.len() > 256 {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    let owner_path = lower.contains("/users/")
        || lower.contains("/home/")
        || value.as_bytes().windows(3).any(|part| {
            part[0].is_ascii_alphabetic() && part[1] == b':' && matches!(part[2], b'/' | b'\\')
        });
    let credential = lower.contains("bearer ") && !lower.contains("bearer [redacted]")
        || ["api_key=", "api-key=", "token=", "secret=", "password="]
            .iter()
            .any(|needle| lower.contains(needle));
    !owner_path && !credential
}

/// Merge one worker-backed playtest into stages 04 and 05 without allowing runtime evidence to
/// rewrite the static stages or authored files.
pub fn apply_runtime_evidence(
    report: &mut GameDebugReport,
    evidence_json: &str,
    duration_ms: u64,
) -> Result<()> {
    if report.mode == GameDebugMode::Quick {
        return Err(report_error(
            "quick mode does not accept runtime evidence",
            "Use /gamedebug full or release for sandbox exercise.",
        ));
    }
    let payload: WorkerPlaytestEvidence = serde_json::from_str(evidence_json).map_err(|error| {
        report_error(
            &format!("invalid worker playtest evidence: {error}"),
            "Repeat the full game-debug run with the Engine pane open.",
        )
    })?;
    let runtime = GameDebugRuntimeEvidence {
        protocol: payload.sandbox.protocol,
        execution: payload.sandbox.execution,
        capabilities: payload.sandbox.capabilities,
        budgets: GameDebugWorkerBudgets {
            instructions_per_tick: payload.sandbox.budgets.instructions_per_tick,
            instructions_total: payload.sandbox.budgets.instructions_total,
            call_depth: payload.sandbox.budgets.call_depth,
            timers: payload.sandbox.budgets.timers,
            heap_estimate_bytes: payload.sandbox.budgets.heap_estimate_bytes,
            wall_clock_millis: payload.sandbox.budgets.wall_clock_millis,
            message_bytes: payload.sandbox.budgets.message_bytes,
            messages_per_tick: payload.sandbox.budgets.messages_per_tick,
            spawned_entities: payload.sandbox.budgets.spawned_entities,
            emitted_events: payload.sandbox.budgets.emitted_events,
            log_bytes: payload.sandbox.budgets.log_bytes,
        },
        termination_reason: payload.sandbox.termination_reason,
        authored_hash_before: payload.authored_hash_before,
        authored_hash_after: payload.authored_hash_after,
        frames: payload.frames,
        checkpoint_hashes: payload
            .samples
            .into_iter()
            .map(|sample| sample.checkpoint_hash)
            .collect(),
        fault_count: payload.faults.len(),
        trace: GameDebugRuntimeTrace {
            entries: payload
                .sandbox
                .trace
                .entries
                .into_iter()
                .map(|entry| GameDebugRuntimeTraceEntry {
                    kind: entry.kind,
                    capability: entry.capability,
                    decision: entry.decision,
                    subject: entry.subject,
                    line: entry.line,
                    instruction: entry.instruction,
                    message: entry.message,
                })
                .collect(),
            truncated: payload.sandbox.trace.truncated,
            redactions: payload.sandbox.trace.redactions,
            usage: GameDebugRuntimeUsage {
                instructions: payload.sandbox.trace.usage.instructions,
                messages: payload.sandbox.trace.usage.messages,
                spawned_entities: payload.sandbox.trace.usage.spawned_entities,
                emitted_events: payload.sandbox.trace.usage.emitted_events,
                log_bytes: payload.sandbox.trace.usage.log_bytes,
                timers: payload.sandbox.trace.usage.timers,
                heap_estimate_bytes: payload.sandbox.trace.usage.heap_estimate_bytes,
                wall_clock_millis: payload.sandbox.trace.usage.wall_clock_millis,
            },
        },
    };
    runtime.validate(report.mode)?;

    let sandbox_passed =
        payload.authored_unchanged && runtime.authored_hash_before == runtime.authored_hash_after;
    set_stage(
        &mut report.stages,
        "04_sandbox",
        if sandbox_passed {
            StageStatus::Passed
        } else {
            StageStatus::Failed
        },
        &format!(
            "{} via {}; {} grants; termination {}; authored hashes {}",
            runtime.protocol,
            runtime.execution,
            runtime.capabilities.len(),
            runtime.termination_reason,
            if sandbox_passed { "match" } else { "differ" },
        ),
        duration_ms,
    );
    if !sandbox_passed {
        report.findings.push(finding(
            "BHP-GD-401",
            "blocker",
            "04_sandbox",
            "runtime://authored-snapshot",
            "The disposable runtime did not preserve the authored snapshot hash.",
            &format!(
                "worker before={} after={}",
                runtime.authored_hash_before, runtime.authored_hash_after
            ),
            &format!("Run `/gamedebug {}` again.", report.mode.as_str()),
            "Stop the runtime write escape and keep all simulation state inside the disposable clone.",
        ));
    }

    let exercise_passed = sandbox_passed
        && payload.completed
        && runtime.frames > 0
        && !runtime.checkpoint_hashes.is_empty()
        && runtime.fault_count == 0;
    set_stage(
        &mut report.stages,
        "05_exercise",
        if exercise_passed {
            StageStatus::Passed
        } else {
            StageStatus::Failed
        },
        &format!(
            "{} frames; {} deterministic checkpoints; {} runtime faults",
            runtime.frames,
            runtime.checkpoint_hashes.len(),
            runtime.fault_count
        ),
        duration_ms,
    );
    if !exercise_passed {
        report.findings.push(finding(
            "BHP-GD-501",
            "blocker",
            "05_exercise",
            "runtime://engine-smoke",
            "The deterministic engine smoke route did not complete cleanly.",
            &format!(
                "completed={} frames={} checkpoints={} faults={}",
                payload.completed,
                runtime.frames,
                runtime.checkpoint_hashes.len(),
                runtime.fault_count
            ),
            &format!("Run `/gamedebug {}` again.", report.mode.as_str()),
            "Fix the first runtime fault, then repeat the identical smoke route.",
        ));
    }
    report.sandbox = EvaluationStatus {
        status: if sandbox_passed { "verified" } else { "failed" }.to_owned(),
        reason: format!(
            "{}; grants={:?}; budgets={:?}; termination={}; authored_before={}; authored_after={}",
            runtime.protocol,
            runtime.capabilities,
            runtime.budgets,
            runtime.termination_reason,
            runtime.authored_hash_before,
            runtime.authored_hash_after
        ),
    };
    report.runtime = Some(runtime);
    finish_runtime_merge(report);
    report.validate()
}

/// Record an unavailable/failed worker as evidence, never as an implicit static pass.
pub fn apply_runtime_failure(report: &mut GameDebugReport, reason: &str, duration_ms: u64) {
    set_stage(
        &mut report.stages,
        "04_sandbox",
        StageStatus::Failed,
        "the application-owned runtime worker could not return evidence",
        duration_ms,
    );
    set_stage(
        &mut report.stages,
        "05_exercise",
        StageStatus::Skipped,
        "exercise did not run because sandbox startup or observation failed",
        0,
    );
    report.findings.push(finding(
        "BHP-GD-400",
        "blocker",
        "04_sandbox",
        "runtime://worker",
        "The sandbox observation did not complete.",
        reason,
        &format!(
            "Run `/gamedebug {}` with the Engine pane open.",
            report.mode.as_str()
        ),
        "Open the Engine pane, keep it active, and retry the same command.",
    ));
    report.sandbox = EvaluationStatus {
        status: "failed".to_owned(),
        reason: reason.to_owned(),
    };
    finish_runtime_merge(report);
}

fn finish_runtime_merge(report: &mut GameDebugReport) {
    sort_findings(&mut report.findings);
    report.outcome = report_outcome(report).to_owned();
}

fn load_scenes(project_root: &Path) -> (Vec<(String, SceneDocument)>, Vec<GameDebugFinding>) {
    let mut paths = Vec::new();
    collect_files(&project_root.join("assets"), ".bscn.json", &mut paths);
    paths.sort();
    let mut scenes = Vec::new();
    let mut failures = Vec::new();
    for path in paths {
        let relative = relative(project_root, &path);
        match std::fs::read_to_string(&path) {
            Ok(text) => match SceneDocument::parse(&text) {
                Ok(scene) => scenes.push((relative, scene)),
                Err(error) => failures.push(finding(
                    "BHP-GD-110",
                    "blocker",
                    "02_validate",
                    &relative,
                    "A scene document is invalid.",
                    &error.to_string(),
                    &format!("Run `/gamedebug quick`; it will parse {relative}."),
                    error.hint().unwrap_or("Fix the scene document and retry."),
                )),
            },
            Err(error) => failures.push(finding(
                "BHP-GD-111",
                "blocker",
                "02_validate",
                &relative,
                "A scene document could not be read.",
                &error.to_string(),
                &format!("Run `/gamedebug quick`; it will read {relative}."),
                "Restore read access to the scene file and retry.",
            )),
        }
    }
    (scenes, failures)
}

fn compile_scripts(
    project_root: &Path,
    findings: &mut Vec<GameDebugFinding>,
) -> Vec<(String, crate::script::ScriptProgram)> {
    let mut paths = Vec::new();
    collect_files(&project_root.join("assets"), ".rhai", &mut paths);
    collect_files(&project_root.join("scripts"), ".rhai", &mut paths);
    paths.sort();
    paths.dedup();
    let mut programs = Vec::new();
    for path in paths {
        let relative = relative(project_root, &path);
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                findings.push(finding(
                    "BHP-GD-210",
                    "blocker",
                    "03_compile",
                    &relative,
                    "A gameplay script could not be read.",
                    &error.to_string(),
                    &format!("Run `/gamedebug quick`; it will read {relative}."),
                    "Restore read access to the script and retry.",
                ));
                continue;
            }
        };
        match crate::script::compile(&relative, &source) {
            Ok(program) => programs.push((relative, program)),
            Err(fault) => {
                findings.push(finding(
                    "BHP-GD-211",
                    "blocker",
                    "03_compile",
                    &format!("{}:{}:{}", fault.file, fault.line, fault.column),
                    "A gameplay script did not compile.",
                    &fault.message,
                    &format!("Run `/gamedebug quick`; it will compile {relative}."),
                    fault.hint.as_deref().unwrap_or("Fix the script and retry."),
                ));
            }
        }
    }
    programs
}

fn load_hud_documents(project_root: &Path) -> Vec<(String, crate::hud::HudDocument)> {
    let mut paths = Vec::new();
    collect_files(&project_root.join("assets"), ".hud.json", &mut paths);
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let relative = relative(project_root, &path);
            let text = std::fs::read_to_string(path).ok()?;
            crate::hud::HudDocument::parse(&text)
                .ok()
                .map(|document| (relative, document))
        })
        .collect()
}

fn load_input_document(project_root: &Path) -> Option<(String, crate::input::InputDocument)> {
    let path = project_root.join(crate::input::DEFAULT_INPUT_PATH);
    let text = std::fs::read_to_string(&path).ok()?;
    crate::input::InputDocument::parse(&text)
        .ok()
        .map(|document| (relative(project_root, &path), document))
}

fn collect_files(root: &Path, suffix: &str, output: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries = entries
        .filter_map(std::result::Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect_files(&path, suffix, output);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(suffix))
        {
            output.push(path);
        }
    }
}

fn authored_tree_hash(project_root: &Path) -> String {
    let mut files = Vec::new();
    let manifest = project_root.join(crate::GAME_MANIFEST_FILE);
    if manifest.is_file() {
        files.push(manifest);
    }
    collect_authored_files(&project_root.join("assets"), &mut files);
    collect_authored_files(&project_root.join("scripts"), &mut files);
    files.sort();
    let mut hasher = blake3::Hasher::new();
    for path in files {
        let rel = relative(project_root, &path);
        hasher.update(rel.as_bytes());
        hasher.update(&[0]);
        if let Ok(bytes) = std::fs::read(path) {
            hasher.update(&bytes);
        }
        hasher.update(&[0xff]);
    }
    hasher.finalize().to_hex().to_string()
}

fn collect_authored_files(root: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect_authored_files(&path, output);
        } else if kind.is_file() {
            output.push(path);
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

fn set_stage(
    stages: &mut [GameDebugStage],
    id: &str,
    status: StageStatus,
    summary: &str,
    duration_ms: u64,
) {
    if let Some(stage) = stages.iter_mut().find(|stage| stage.id == id) {
        stage.status = status;
        stage.summary = summary.to_owned();
        stage.duration_ms = duration_ms;
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[allow(clippy::too_many_arguments)]
fn finding(
    code: &str,
    severity: &str,
    stage: &str,
    address: &str,
    message: &str,
    evidence: &str,
    reproduction: &str,
    repair: &str,
) -> GameDebugFinding {
    GameDebugFinding {
        code: code.to_owned(),
        severity: severity.to_owned(),
        stage: stage.to_owned(),
        address: address.to_owned(),
        message: message.to_owned(),
        evidence: evidence.to_owned(),
        reproduction: reproduction.to_owned(),
        repair: repair.to_owned(),
    }
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "blocker" => 3,
        "warning" => 2,
        _ => 1,
    }
}

fn sort_findings(findings: &mut [GameDebugFinding]) {
    findings.sort_by(|left, right| {
        severity_rank(&right.severity)
            .cmp(&severity_rank(&left.severity))
            .then_with(|| left.stage.cmp(&right.stage))
            .then_with(|| left.address.cmp(&right.address))
            .then_with(|| left.code.cmp(&right.code))
    });
}

fn report_outcome(report: &GameDebugReport) -> &'static str {
    if !report.authored_tree_unchanged()
        || report
            .findings
            .iter()
            .any(|item| item.severity == "blocker")
        || report
            .stages
            .iter()
            .any(|stage| stage.status == StageStatus::Failed)
    {
        "failed"
    } else if report
        .stages
        .iter()
        .any(|stage| stage.status == StageStatus::Unsupported)
    {
        "incomplete"
    } else {
        "passed"
    }
}

fn report_error(message: &str, hint: &str) -> EngineError {
    EngineError::Schema(message.to_owned(), Some(hint.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_runtime_evidence, apply_runtime_failure, run, GameDebugMode, StageStatus, STAGES,
    };
    use crate::scaffold::write_project;

    fn game(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("bhippi-game-debug-{label}-{}", ulid::Ulid::new()));
        write_project(&root, "Debug Fixture", false).expect("fixture writes");
        root
    }

    #[test]
    fn every_run_has_the_same_ordered_stage_graph() {
        let root = game("ordered");
        let report = run(&root, GameDebugMode::Quick);
        let ids = report
            .stages
            .iter()
            .map(|stage| stage.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, STAGES.iter().map(|(id, _)| *id).collect::<Vec<_>>());
        assert_eq!(report.outcome, "passed");
        assert!(report.authored_tree_unchanged());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn broken_gameplay_script_is_a_located_stable_finding() {
        let root = game("bad-script");
        let script = root.join("scripts/level_01.rhai");
        std::fs::write(&script, "fn on_start() { missing_host(); }").expect("break script");
        let report = run(&root, GameDebugMode::Quick);
        let finding = report
            .findings
            .iter()
            .find(|item| item.code == "BHP-GD-211")
            .expect("script finding");
        assert!(finding.address.contains("scripts/level_01.rhai"));
        assert_eq!(finding.stage, "03_compile");
        assert_eq!(report.outcome, "failed");
        assert!(report.authored_tree_unchanged());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_shader_source_is_the_same_stage_two_gate_as_packaging() {
        let root = game("missing-shader-source");
        std::fs::remove_file(root.join("assets/shaders/lit_pbr.wgsl"))
            .expect("remove shader source");
        let report = run(&root, GameDebugMode::Quick);
        let finding = report
            .findings
            .iter()
            .find(|item| item.code == "BHP-GATE-MISSING_SHADER_SOURCE")
            .expect("shared gate finding");
        assert_eq!(finding.stage, "02_validate");
        assert_eq!(finding.address, "assets/shaders/lit_pbr.shader.json");
        assert_eq!(report.stages[1].status, StageStatus::Failed);
        assert_eq!(report.outcome, "failed");
        assert!(report.authored_tree_unchanged());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_manifest_fails_discovery_without_inventing_later_passes() {
        let root =
            std::env::temp_dir().join(format!("bhippi-game-debug-missing-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&root).expect("fixture dir");
        let report = run(&root, GameDebugMode::Quick);
        assert_eq!(report.outcome, "failed");
        assert_eq!(report.stages[0].status, StageStatus::Failed);
        assert!(report.findings.iter().any(|item| item.code == "BHP-GD-001"));
        assert!(report.authored_tree_unchanged());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn requested_runtime_stages_are_honestly_unsupported() {
        let root = game("full");
        let report = run(&root, GameDebugMode::Full);
        assert_eq!(report.outcome, "incomplete");
        assert!(report
            .stages
            .iter()
            .any(|stage| stage.status == StageStatus::Unsupported));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn worker_evidence_closes_only_sandbox_and_exercise_stages() {
        let root = game("runtime-evidence");
        let mut report = run(&root, GameDebugMode::Full);
        let evidence = serde_json::json!({
            "authoredUnchanged": true,
            "authoredHashBefore": "fnv1a32:12345678",
            "authoredHashAfter": "fnv1a32:12345678",
            "completed": true,
            "frames": 1,
            "samples": [{ "checkpointHash": "fnv1a32:abcdef01" }],
            "faults": [],
            "sandbox": {
                "protocol": "bhippi-runtime-protocol@1",
                "execution": "application_module_worker",
                "capabilities": [],
                "budgets": {
                    "instructionsPerTick": 200000,
                    "instructionsTotal": 20000000,
                    "callDepth": 64,
                    "timers": 4096,
                    "heapEstimateBytes": 67108864,
                    "wallClockMillis": 300000,
                    "messageBytes": 1024,
                    "messagesPerTick": 8,
                    "spawnedEntities": 8,
                    "emittedEvents": 8,
                    "logBytes": 1024
                },
                "terminationReason": "completed",
                "trace": {
                    "entries": [
                        { "kind": "capability", "capability": "entity_read", "decision": "denied" },
                        { "kind": "capability", "capability": "entity_write_runtime", "decision": "denied" },
                        { "kind": "capability", "capability": "input_read", "decision": "denied" },
                        { "kind": "capability", "capability": "hud_action", "decision": "denied" },
                        { "kind": "capability", "capability": "level_travel", "decision": "denied" },
                        { "kind": "capability", "capability": "audio_event", "decision": "denied" },
                        { "kind": "capability", "capability": "deterministic_timer", "decision": "denied" }
                    ],
                    "truncated": false,
                    "redactions": 0,
                    "usage": {
                        "instructions": 0,
                        "messages": 2,
                        "spawnedEntities": 0,
                        "emittedEvents": 0,
                        "logBytes": 0,
                        "timers": 0,
                        "heapEstimateBytes": 512,
                        "wallClockMillis": 1
                    }
                }
            }
        });
        apply_runtime_evidence(&mut report, &evidence.to_string(), 7).expect("evidence merges");
        assert_eq!(report.stages[3].status, StageStatus::Passed);
        assert_eq!(report.stages[4].status, StageStatus::Passed);
        assert_eq!(report.stages[6].status, StageStatus::Unsupported);
        assert_eq!(report.outcome, "incomplete");
        assert_eq!(report.sandbox.status, "verified");
        assert_eq!(report.runtime.as_ref().map(|item| item.frames), Some(1));
        report.validate().expect("merged report validates");

        let mut leaked = evidence.clone();
        leaked["sandbox"]["trace"]["entries"]
            .as_array_mut()
            .expect("trace entries")
            .push(serde_json::json!({
                "kind": "log",
                "subject": "player",
                "message": "token=live-secret"
            }));
        let mut leaked_report = run(&root, GameDebugMode::Full);
        assert!(apply_runtime_evidence(&mut leaked_report, &leaked.to_string(), 1).is_err());

        let mut incomplete = evidence;
        incomplete["sandbox"]["trace"]["entries"]
            .as_array_mut()
            .expect("trace entries")
            .pop();
        let mut incomplete_report = run(&root, GameDebugMode::Full);
        assert!(
            apply_runtime_evidence(&mut incomplete_report, &incomplete.to_string(), 1).is_err()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unavailable_worker_is_a_stable_blocking_finding() {
        let root = game("runtime-failure");
        let mut report = run(&root, GameDebugMode::Full);
        apply_runtime_failure(&mut report, "Engine pane was not open", 12);
        assert_eq!(report.stages[3].status, StageStatus::Failed);
        assert_eq!(report.stages[4].status, StageStatus::Skipped);
        assert!(report.findings.iter().any(|item| item.code == "BHP-GD-400"));
        assert_eq!(report.outcome, "failed");
        report.validate().expect("failure report validates");
        let _ = std::fs::remove_dir_all(root);
    }
}
