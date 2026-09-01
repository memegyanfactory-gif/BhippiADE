//! Versioned, deterministic gameplay scenarios used by `/gamedebug`.
//!
//! The document is deliberately data-only. A model may author extra scenarios, but the
//! runtime receives this validated schedule and assertion vocabulary rather than prose.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::collections::BTreeSet;

pub const GAME_TEST_PLAN_FORMAT: &str = "bhippi-game-test-plan@1";
pub const GAME_TEST_BATCH_FORMAT: &str = "bhippi-game-test-batch@1";
/// One discoverable authored-plan location. A model cannot choose a friendlier plan at run time.
pub const GAME_TEST_PLAN_FILE: &str = "tests/game-test-plan.json";
pub const MANDATORY_SMOKE_SCENARIO: &str = "engine_smoke";
/// Largest integer that JSON/JavaScript workers preserve without rounding.
pub const MAX_EXACT_WORKER_SEED: u64 = 9_007_199_254_740_991;
pub const MAX_GAME_TEST_PLAN_BYTES: usize = 1_048_576;
pub const MAX_GAME_TEST_SCENARIOS: usize = 32;
pub const MAX_GAME_TEST_INPUT_STEPS: usize = 4_096;
pub const MAX_GAME_TEST_CHECKPOINTS: usize = 1_024;
pub const MAX_GAME_TEST_ASSERTIONS: usize = 8_192;
/// Total authored simulation time across every scenario. Deterministic simulation does not
/// sleep, but it still consumes instruction, heap and outer observation budgets.
pub const MAX_GAME_TEST_SIMULATION_MILLIS: u64 = 300_000;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GameTestPlan {
    pub format: String,
    pub scenarios: Vec<GameTestScenario>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GameTestScenario {
    pub name: String,
    pub initial_level: String,
    pub seed: u64,
    #[serde(default)]
    pub input: Vec<GameTestInputStep>,
    pub checkpoints: Vec<GameTestCheckpoint>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GameTestInputStep {
    pub at_ms: u64,
    #[serde(flatten)]
    pub input: GameTestInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameTestInput {
    Press { action: String },
    Release { action: String },
    Axis { axis: String, value: f32 },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GameTestCheckpoint {
    pub name: String,
    pub at_ms: u64,
    pub assertions: Vec<GameTestAssertion>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TestComparison {
    Equal,
    NotEqual,
    GreaterOrEqual,
    LessOrEqual,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GameTestAssertion {
    Variable {
        path: String,
        comparison: TestComparison,
        expected: Value,
    },
    Event {
        name: String,
        min_count: u32,
    },
    Transform {
        entity: String,
        #[serde(default)]
        translation: Option<[f32; 3]>,
        #[serde(default)]
        rotation_degrees: Option<[f32; 3]>,
        #[serde(default)]
        scale: Option<[f32; 3]>,
        tolerance: f32,
    },
    Hud {
        widget: String,
        property: String,
        comparison: TestComparison,
        expected: Value,
    },
    LevelTravel {
        level: String,
    },
}

/// Scenario-specific evidence returned by fresh disposable workers. This deliberately does not
/// union grants, traces or budgets across levels: each scenario retains the sandbox facts that
/// actually governed it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameTestBatchEvidence {
    pub format: String,
    pub plan_format: String,
    pub authored_tree_before: String,
    pub authored_tree_after: String,
    pub scenarios: Vec<GameTestScenarioEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameTestScenarioEvidence {
    pub name: String,
    pub initial_level: String,
    pub seed: u64,
    /// A one-way identity for the fresh worker session; raw nonces never enter reports.
    pub worker_session_hash: String,
    pub runtime: crate::game_debug::GameDebugRuntimeEvidence,
    pub assertions: Vec<GameTestAssertionEvidence>,
    pub completed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GameTestAssertionEvidence {
    pub checkpoint: String,
    pub assertion_index: u32,
    pub passed: bool,
    pub address: String,
    pub observed: serde_json::Value,
    pub expected: serde_json::Value,
}

impl GameTestBatchEvidence {
    pub fn parse(text: &str, plan: &GameTestPlan) -> Result<Self> {
        let evidence: Self = serde_json::from_str(text).map_err(|error| {
            schema_error(
                &format!("invalid game test batch evidence: {error}"),
                &format!("Fix the JSON and keep format {GAME_TEST_BATCH_FORMAT}."),
            )
        })?;
        evidence.validate_against(plan)?;
        Ok(evidence)
    }

    pub fn validate_against(&self, plan: &GameTestPlan) -> Result<()> {
        plan.validate()?;
        if self.format != GAME_TEST_BATCH_FORMAT || self.plan_format != GAME_TEST_PLAN_FORMAT {
            return Err(schema_error(
                "game test batch and plan formats are incompatible",
                &format!(
                    "Use {GAME_TEST_BATCH_FORMAT} evidence for {GAME_TEST_PLAN_FORMAT} plans."
                ),
            ));
        }
        if !valid_blake3(&self.authored_tree_before)
            || !valid_blake3(&self.authored_tree_after)
            || self.authored_tree_before != self.authored_tree_after
        {
            return Err(schema_error(
                "game test batch did not preserve one canonical authored tree hash",
                "Run every scenario against disposable runtime state and retain the same BLAKE3 authored-tree hash.",
            ));
        }
        if self.scenarios.len() != plan.scenarios.len() {
            return Err(schema_error(
                "game test batch does not contain exactly one result per planned scenario",
                "Return results in plan order without dropping, merging or duplicating scenarios.",
            ));
        }

        let mut worker_sessions = BTreeSet::new();
        for (scenario, expected) in self.scenarios.iter().zip(&plan.scenarios) {
            if scenario.name != expected.name
                || scenario.initial_level != expected.initial_level
                || scenario.seed != expected.seed
            {
                return Err(schema_error(
                    &format!("scenario evidence {:?} does not match the planned identity", scenario.name),
                    "Keep scenario name, initial level, seed and order byte-for-byte aligned with the validated plan.",
                ));
            }
            if !valid_sha256(&scenario.worker_session_hash)
                || !worker_sessions.insert(scenario.worker_session_hash.as_str())
            {
                return Err(schema_error(
                    "scenario evidence has an invalid or reused worker session identity",
                    "Start a fresh worker per scenario and store only its unique SHA-256 nonce hash.",
                ));
            }
            scenario
                .runtime
                .validate(crate::game_debug::GameDebugMode::Full)?;
            if scenario.runtime.authored_hash_before != scenario.runtime.authored_hash_after {
                return Err(schema_error(
                    &format!("scenario {:?} changed its runtime-authored snapshot", scenario.name),
                    "Discard runtime state after every scenario and never write it into authored documents.",
                ));
            }

            let expected_assertions = expected
                .checkpoints
                .iter()
                .flat_map(|checkpoint| {
                    checkpoint
                        .assertions
                        .iter()
                        .enumerate()
                        .map(move |(index, assertion)| (checkpoint.name.as_str(), index, assertion))
                })
                .collect::<Vec<_>>();
            if scenario.assertions.len() != expected_assertions.len() {
                return Err(schema_error(
                    &format!("scenario {:?} omitted assertion evidence", scenario.name),
                    "Return one pass/fail observation for every assertion, including assertions not reached after a fault.",
                ));
            }
            for (actual, (checkpoint, index, assertion)) in
                scenario.assertions.iter().zip(expected_assertions)
            {
                if actual.checkpoint != checkpoint
                    || usize::try_from(actual.assertion_index).ok() != Some(index)
                    || actual.address.trim().is_empty()
                    || serde_json::to_value(assertion).ok().as_ref() != Some(&actual.expected)
                {
                    return Err(schema_error(
                        &format!("scenario {:?} assertion evidence is out of order, unlocated or changes the expected value", scenario.name),
                        "Keep checkpoint order, zero-based assertion indices and expected assertion bytes from the plan, with an exact evidence address.",
                    ));
                }
            }
            let derived_completed = scenario.runtime.termination_reason == "completed"
                && scenario.runtime.fault_count == 0
                && scenario.runtime.checkpoint_hashes.len() == expected.checkpoints.len()
                && scenario.assertions.iter().all(|assertion| assertion.passed);
            if scenario.completed != derived_completed {
                return Err(schema_error(
                    &format!("scenario {:?} has a forged completed flag", scenario.name),
                    "Derive completion from clean worker termination, every checkpoint and every assertion result.",
                ));
            }
        }
        Ok(())
    }

    pub fn dump(&self, plan: &GameTestPlan) -> Result<String> {
        self.validate_against(plan)?;
        serde_json::to_string_pretty(self).map_err(|error| {
            schema_error(
                &format!("cannot serialise game test batch evidence: {error}"),
                "Report this as an engine bug.",
            )
        })
    }
}

fn valid_blake3(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(valid_blake3)
}

impl GameTestPlan {
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > MAX_GAME_TEST_PLAN_BYTES {
            return Err(schema_error(
                "game test plan exceeds its encoded byte budget",
                &format!("Keep the UTF-8 JSON at or below {MAX_GAME_TEST_PLAN_BYTES} bytes."),
            ));
        }
        let plan: Self = serde_json::from_str(text).map_err(|error| {
            EngineError::Schema(
                format!("invalid game test plan: {error}"),
                Some(format!(
                    "Fix the scenario document and keep format {GAME_TEST_PLAN_FORMAT}."
                )),
            )
        })?;
        plan.validate()?;
        Ok(plan)
    }

    /// Resolve an optional authored plan. Absence is not permission to skip exercise: the
    /// engine inserts the same fixed-seed smoke scenario for every project.
    pub fn resolve(authored: Option<Self>, default_level: &str) -> Result<Self> {
        match authored {
            Some(plan) => {
                plan.validate()?;
                Ok(plan)
            }
            None => {
                let plan = Self::mandatory_smoke(default_level)?;
                plan.validate()?;
                Ok(plan)
            }
        }
    }

    /// Load the fixed authored plan, or use the mandatory smoke plan when the file is absent.
    ///
    /// Keeping discovery here makes `/gamedebug`, CI and future dashboard runs agree about the
    /// exact bytes under test. The plan itself is authored input, never a runtime report.
    pub fn load_or_smoke(project_root: &std::path::Path, default_level: &str) -> Result<Self> {
        let path = project_root.join(GAME_TEST_PLAN_FILE);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let plan = Self::mandatory_smoke(default_level)?;
                plan.validate_project_scenes(project_root)?;
                return Ok(plan);
            }
            Err(error) => {
                return Err(EngineError::Io {
                    operation: "inspect",
                    path: path.display().to_string(),
                    reason: error.to_string(),
                    hint: Some(format!(
                        "Make {GAME_TEST_PLAN_FILE} a readable regular file."
                    )),
                });
            }
        };
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(schema_error(
                &format!("{GAME_TEST_PLAN_FILE} must be a regular non-symlink file"),
                "Replace it with an authored JSON document inside the project.",
            ));
        }
        if usize::try_from(metadata.len()).map_or(true, |length| length > MAX_GAME_TEST_PLAN_BYTES)
        {
            return Err(schema_error(
                &format!("{GAME_TEST_PLAN_FILE} exceeds its encoded byte budget"),
                &format!("Keep the UTF-8 JSON at or below {MAX_GAME_TEST_PLAN_BYTES} bytes."),
            ));
        }
        let text = std::fs::read_to_string(&path).map_err(|error| EngineError::Io {
            operation: "read",
            path: path.display().to_string(),
            reason: error.to_string(),
            hint: Some(format!("Make {GAME_TEST_PLAN_FILE} readable UTF-8 JSON.")),
        })?;
        let plan = Self::parse(&text)?;
        plan.validate_project_scenes(project_root)?;
        Ok(plan)
    }

    fn validate_project_scenes(&self, project_root: &std::path::Path) -> Result<()> {
        let canonical_root = project_root
            .canonicalize()
            .map_err(|error| EngineError::Io {
                operation: "canonicalize",
                path: project_root.display().to_string(),
                reason: error.to_string(),
                hint: Some("Open a readable local game project and retry.".to_owned()),
            })?;
        for scenario in &self.scenarios {
            validate_project_scene(
                &canonical_root,
                &scenario.initial_level,
                &format!("scenario {:?} initial_level", scenario.name),
            )?;
            for checkpoint in &scenario.checkpoints {
                for assertion in &checkpoint.assertions {
                    if let GameTestAssertion::LevelTravel { level } = assertion {
                        validate_project_scene(
                            &canonical_root,
                            level,
                            &format!(
                                "scenario {:?} checkpoint {:?} level-travel assertion",
                                scenario.name, checkpoint.name
                            ),
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn mandatory_smoke(default_level: &str) -> Result<Self> {
        if default_level.trim().is_empty() {
            return Err(schema_error(
                "the mandatory smoke scenario needs a default level",
                "Set game.default_scene to an authored scene before running the test plan.",
            ));
        }
        Ok(Self {
            format: GAME_TEST_PLAN_FORMAT.to_owned(),
            scenarios: vec![GameTestScenario {
                name: MANDATORY_SMOKE_SCENARIO.to_owned(),
                initial_level: default_level.to_owned(),
                seed: 0,
                input: Vec::new(),
                checkpoints: vec![GameTestCheckpoint {
                    name: "initial_level_loaded".to_owned(),
                    at_ms: 0,
                    assertions: vec![GameTestAssertion::LevelTravel {
                        level: default_level.to_owned(),
                    }],
                }],
            }],
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != GAME_TEST_PLAN_FORMAT {
            return Err(schema_error(
                &format!("unsupported game test plan format {:?}", self.format),
                &format!("Set format to {GAME_TEST_PLAN_FORMAT}; unknown major versions block."),
            ));
        }
        if self.scenarios.is_empty() || self.scenarios.len() > MAX_GAME_TEST_SCENARIOS {
            return Err(schema_error(
                &format!(
                    "an authored game test plan must contain 1 to {MAX_GAME_TEST_SCENARIOS} scenarios"
                ),
                "Remove the empty document to use the engine smoke scenario, or split an oversized suite.",
            ));
        }

        let mut scenario_names = BTreeSet::new();
        let mut total_inputs = 0_usize;
        let mut total_checkpoints = 0_usize;
        let mut total_assertions = 0_usize;
        let mut total_simulation_millis = 0_u64;
        for scenario in &self.scenarios {
            require_name(&scenario.name, "scenario name")?;
            require_name(&scenario.initial_level, "scenario initial_level")?;
            if scenario.seed > MAX_EXACT_WORKER_SEED {
                return Err(schema_error(
                    &format!(
                        "scenario {:?} seed {} cannot cross the JSON worker boundary exactly",
                        scenario.name, scenario.seed
                    ),
                    &format!("Use an integer seed from 0 to {MAX_EXACT_WORKER_SEED}."),
                ));
            }
            if !scenario_names.insert(scenario.name.as_str()) {
                return Err(schema_error(
                    &format!("duplicate game test scenario {:?}", scenario.name),
                    "Give every scenario a unique stable name.",
                ));
            }
            if scenario.checkpoints.is_empty() {
                return Err(schema_error(
                    &format!("scenario {:?} has no checkpoints", scenario.name),
                    "Add at least one checkpoint with a concrete assertion.",
                ));
            }
            total_inputs = total_inputs.saturating_add(scenario.input.len());
            total_checkpoints = total_checkpoints.saturating_add(scenario.checkpoints.len());
            total_assertions = total_assertions.saturating_add(
                scenario
                    .checkpoints
                    .iter()
                    .map(|checkpoint| checkpoint.assertions.len())
                    .sum::<usize>(),
            );
            total_simulation_millis = total_simulation_millis.saturating_add(
                scenario
                    .checkpoints
                    .last()
                    .map_or(0, |checkpoint| checkpoint.at_ms),
            );
            validate_schedule(scenario)?;
        }
        if total_inputs > MAX_GAME_TEST_INPUT_STEPS
            || total_checkpoints > MAX_GAME_TEST_CHECKPOINTS
            || total_assertions > MAX_GAME_TEST_ASSERTIONS
            || total_simulation_millis > MAX_GAME_TEST_SIMULATION_MILLIS
        {
            return Err(schema_error(
                "game test plan exceeds its deterministic execution budget",
                &format!(
                    "Keep totals at or below {MAX_GAME_TEST_INPUT_STEPS} input steps, {MAX_GAME_TEST_CHECKPOINTS} checkpoints, {MAX_GAME_TEST_ASSERTIONS} assertions and {MAX_GAME_TEST_SIMULATION_MILLIS} simulated milliseconds."
                ),
            ));
        }
        Ok(())
    }

    pub fn dump(&self) -> Result<String> {
        self.validate()?;
        let text = serde_json::to_string_pretty(self).map_err(|error| {
            schema_error(
                &format!("cannot serialise game test plan: {error}"),
                "Report this as an engine bug.",
            )
        })?;
        if text.len() > MAX_GAME_TEST_PLAN_BYTES {
            return Err(schema_error(
                "serialised game test plan exceeds its encoded byte budget",
                "Split the plan into a smaller deterministic suite.",
            ));
        }
        Ok(text)
    }
}

fn validate_schedule(scenario: &GameTestScenario) -> Result<()> {
    let mut previous_step = 0;
    for (index, step) in scenario.input.iter().enumerate() {
        if index > 0 && step.at_ms < previous_step {
            return Err(schema_error(
                &format!(
                    "scenario {:?} input steps are not time-ordered",
                    scenario.name
                ),
                "Sort input steps by at_ms so replay order is deterministic.",
            ));
        }
        previous_step = step.at_ms;
        match &step.input {
            GameTestInput::Press { action } | GameTestInput::Release { action } => {
                require_name(action, "input action")?;
            }
            GameTestInput::Axis { axis, value } => {
                require_name(axis, "input axis")?;
                if !value.is_finite() || !(-1.0..=1.0).contains(value) {
                    return Err(schema_error(
                        &format!("axis {axis:?} value {value} is outside -1..=1"),
                        "Use a finite normalised axis value from -1 to 1.",
                    ));
                }
            }
        }
    }

    let mut checkpoint_names = BTreeSet::new();
    let mut previous_checkpoint = 0;
    for (index, checkpoint) in scenario.checkpoints.iter().enumerate() {
        require_name(&checkpoint.name, "checkpoint name")?;
        if !checkpoint_names.insert(checkpoint.name.as_str()) {
            return Err(schema_error(
                &format!(
                    "scenario {:?} has duplicate checkpoint {:?}",
                    scenario.name, checkpoint.name
                ),
                "Give every checkpoint in a scenario a unique stable name.",
            ));
        }
        if index > 0 && checkpoint.at_ms < previous_checkpoint {
            return Err(schema_error(
                &format!(
                    "scenario {:?} checkpoints are not time-ordered",
                    scenario.name
                ),
                "Sort checkpoints by at_ms so replay order is deterministic.",
            ));
        }
        previous_checkpoint = checkpoint.at_ms;
        if checkpoint.assertions.is_empty() {
            return Err(schema_error(
                &format!("checkpoint {:?} has no assertions", checkpoint.name),
                "Add a variable, event, transform, HUD or level-travel assertion.",
            ));
        }
        for assertion in &checkpoint.assertions {
            validate_assertion(assertion)?;
        }
    }
    if scenario.input.last().is_some_and(|step| {
        scenario
            .checkpoints
            .last()
            .is_some_and(|checkpoint| step.at_ms > checkpoint.at_ms)
    }) {
        return Err(schema_error(
            &format!(
                "scenario {:?} has input after its final checkpoint",
                scenario.name
            ),
            "Move the final checkpoint after every input transition, or remove unused input.",
        ));
    }
    Ok(())
}

fn validate_assertion(assertion: &GameTestAssertion) -> Result<()> {
    match assertion {
        GameTestAssertion::Variable { path, .. } => require_name(path, "variable path"),
        GameTestAssertion::Event { name, min_count } => {
            require_name(name, "event name")?;
            if *min_count == 0 {
                return Err(schema_error(
                    "an event assertion with min_count 0 proves nothing",
                    "Set min_count to at least 1.",
                ));
            }
            Ok(())
        }
        GameTestAssertion::Transform {
            entity,
            translation,
            rotation_degrees,
            scale,
            tolerance,
        } => {
            require_name(entity, "transform entity")?;
            if translation.is_none() && rotation_degrees.is_none() && scale.is_none() {
                return Err(schema_error(
                    "a transform assertion must name translation, rotation_degrees or scale",
                    "Add at least one expected transform component.",
                ));
            }
            if !tolerance.is_finite() || *tolerance < 0.0 {
                return Err(schema_error(
                    "transform tolerance must be finite and non-negative",
                    "Use 0 for exact comparison or a finite positive tolerance.",
                ));
            }
            for vector in [
                translation.as_ref(),
                rotation_degrees.as_ref(),
                scale.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                if vector.iter().any(|value| !value.is_finite()) {
                    return Err(schema_error(
                        "transform expectations must contain finite values",
                        "Replace NaN or infinity with concrete coordinates.",
                    ));
                }
            }
            Ok(())
        }
        GameTestAssertion::Hud {
            widget, property, ..
        } => {
            require_name(widget, "HUD widget")?;
            require_name(property, "HUD property")
        }
        GameTestAssertion::LevelTravel { level } => require_name(level, "travelled level"),
    }
}

fn require_name(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(schema_error(
            &format!("{label} must not be empty"),
            &format!("Give the {label} a stable non-empty value."),
        ))
    } else {
        Ok(())
    }
}

fn validate_project_scene(
    canonical_root: &std::path::Path,
    relative: &str,
    label: &str,
) -> Result<()> {
    let relative_path = std::path::Path::new(relative);
    let safe_shape = !relative.contains('\\')
        && relative.starts_with("assets/scenes/")
        && relative.ends_with(".bscn.json")
        && relative_path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
    if !safe_shape {
        return Err(schema_error(
            &format!("{label} is not a safe authored scene path: {relative:?}"),
            "Use an assets/scenes/*.bscn.json project-relative path without traversal.",
        ));
    }
    let joined = canonical_root.join(relative_path);
    let metadata = std::fs::symlink_metadata(&joined).map_err(|error| EngineError::Io {
        operation: "inspect",
        path: joined.display().to_string(),
        reason: error.to_string(),
        hint: Some(format!(
            "Create the scene referenced by {label}, or correct the plan."
        )),
    })?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(schema_error(
            &format!("{label} must reference a regular non-symlink scene"),
            "Keep game-test scenes as authored files inside assets/scenes/.",
        ));
    }
    let canonical = joined.canonicalize().map_err(|error| EngineError::Io {
        operation: "canonicalize",
        path: joined.display().to_string(),
        reason: error.to_string(),
        hint: Some("Repair the scene path and retry.".to_owned()),
    })?;
    if !canonical.starts_with(canonical_root) {
        return Err(schema_error(
            &format!("{label} resolves outside the game project"),
            "Keep every scenario and level-travel target inside this project.",
        ));
    }
    Ok(())
}

fn schema_error(message: &str, hint: &str) -> EngineError {
    EngineError::Schema(message.to_owned(), Some(hint.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{
        GameTestInput, GameTestInputStep, GameTestPlan, GAME_TEST_PLAN_FILE,
        MANDATORY_SMOKE_SCENARIO, MAX_EXACT_WORKER_SEED, MAX_GAME_TEST_SIMULATION_MILLIS,
    };

    #[test]
    fn fixed_project_plan_is_loaded_and_absence_uses_smoke() {
        let root = std::env::temp_dir().join(format!(
            "bhippi-game-test-plan-load-{}",
            bhippi_types::TransactionId::new()
        ));
        std::fs::create_dir_all(root.join("tests")).expect("test folder");
        std::fs::create_dir_all(root.join("assets/scenes")).expect("scene folder");
        std::fs::write(root.join("assets/scenes/main.bscn.json"), b"{}").expect("smoke scene");
        std::fs::write(root.join("assets/scenes/level_01.bscn.json"), b"{}")
            .expect("authored scene");
        let fallback = GameTestPlan::load_or_smoke(&root, "assets/scenes/main.bscn.json")
            .expect("missing plan uses smoke");
        assert_eq!(fallback.scenarios[0].name, MANDATORY_SMOKE_SCENARIO);

        let authored = serde_json::json!({
            "format": "bhippi-game-test-plan@1",
            "scenarios": [{
                "name": "authored_boot",
                "initial_level": "assets/scenes/level_01.bscn.json",
                "seed": 7,
                "input": [],
                "checkpoints": [{
                    "name": "booted",
                    "at_ms": 0,
                    "assertions": [{
                        "kind": "level_travel",
                        "level": "assets/scenes/level_01.bscn.json"
                    }]
                }]
            }]
        });
        std::fs::write(
            root.join(GAME_TEST_PLAN_FILE),
            serde_json::to_vec_pretty(&authored).expect("json"),
        )
        .expect("plan writes");
        let loaded = GameTestPlan::load_or_smoke(&root, "assets/scenes/main.bscn.json")
            .expect("authored plan loads");
        assert_eq!(loaded.scenarios[0].name, "authored_boot");
        let _ignored = std::fs::remove_dir_all(root);
    }

    #[test]
    fn worker_seed_must_survive_json_without_rounding() {
        let mut plan =
            GameTestPlan::mandatory_smoke("assets/scenes/main.bscn.json").expect("smoke plan");
        plan.scenarios[0].seed = MAX_EXACT_WORKER_SEED + 1;
        let error = plan.validate().expect_err("inexact seed must block");
        assert!(error.to_string().contains("JSON worker boundary"));
    }

    #[test]
    fn plan_timeline_is_bounded_and_every_input_is_observed() {
        let mut oversized =
            GameTestPlan::mandatory_smoke("assets/scenes/main.bscn.json").expect("smoke plan");
        oversized.scenarios[0].checkpoints[0].at_ms = MAX_GAME_TEST_SIMULATION_MILLIS + 1;
        assert!(oversized
            .validate()
            .expect_err("oversized timeline must block")
            .to_string()
            .contains("execution budget"));

        let mut unobserved =
            GameTestPlan::mandatory_smoke("assets/scenes/main.bscn.json").expect("smoke plan");
        unobserved.scenarios[0].input.push(GameTestInputStep {
            at_ms: 1,
            input: GameTestInput::Press {
                action: "jump".to_owned(),
            },
        });
        assert!(unobserved
            .validate()
            .expect_err("input after final checkpoint must block")
            .to_string()
            .contains("after its final checkpoint"));
    }
}
