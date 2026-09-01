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
    pub artifacts: Vec<String>,
    pub repair_batch_id: Option<String>,
    pub outcome: String,
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

        validate_authored_formats(project_root, &mut findings);

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
        artifacts: Vec::new(),
        repair_batch_id: None,
        outcome: outcome.to_owned(),
    }
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

fn validate_authored_formats(project_root: &Path, findings: &mut Vec<GameDebugFinding>) {
    validate_files(
        project_root,
        ".hud.json",
        "BHP-GD-130",
        "HUD document",
        |text| crate::hud::HudDocument::parse(text).map(|_| ()),
        findings,
    );
    validate_files(
        project_root,
        ".mat.json",
        "BHP-GD-131",
        "material document",
        |text| crate::material::MaterialDocument::parse(text).map(|_| ()),
        findings,
    );
    validate_files(
        project_root,
        ".shader.json",
        "BHP-GD-132",
        "shader document",
        |text| crate::material::ShaderDocument::parse(text).map(|_| ()),
        findings,
    );
    let input_path = project_root.join(crate::input::DEFAULT_INPUT_PATH);
    if input_path.is_file() {
        validate_one(
            project_root,
            &input_path,
            "BHP-GD-133",
            "input document",
            |text| crate::input::InputDocument::parse(text).map(|_| ()),
            findings,
        );
    }
}

fn validate_files<F>(
    project_root: &Path,
    suffix: &str,
    code: &str,
    label: &str,
    parse: F,
    findings: &mut Vec<GameDebugFinding>,
) where
    F: Fn(&str) -> crate::Result<()>,
{
    let mut paths = Vec::new();
    collect_files(&project_root.join("assets"), suffix, &mut paths);
    paths.sort();
    for path in paths {
        validate_one(project_root, &path, code, label, &parse, findings);
    }
}

fn validate_one<F>(
    project_root: &Path,
    path: &Path,
    code: &str,
    label: &str,
    parse: F,
    findings: &mut Vec<GameDebugFinding>,
) where
    F: Fn(&str) -> crate::Result<()>,
{
    let relative = relative(project_root, path);
    let result = std::fs::read_to_string(path)
        .map_err(|error| error.to_string())
        .and_then(|text| parse(&text).map_err(|error| error.to_string()));
    if let Err(error) = result {
        findings.push(finding(
            code,
            "blocker",
            "02_validate",
            &relative,
            &format!("A {label} is invalid."),
            &error,
            &format!("Run `/gamedebug quick`; it will parse {relative}."),
            &format!("Fix the {label} using its versioned schema and retry."),
        ));
    }
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
    use super::{run, GameDebugMode, StageStatus, STAGES};
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
}
