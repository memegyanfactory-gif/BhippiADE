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
pub const MANDATORY_SMOKE_SCENARIO: &str = "engine_smoke";

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
        if self.scenarios.is_empty() {
            return Err(schema_error(
                "an authored game test plan must contain at least one scenario",
                "Remove the empty document to use the engine smoke scenario, or add a scenario.",
            ));
        }

        let mut scenario_names = BTreeSet::new();
        for scenario in &self.scenarios {
            require_name(&scenario.name, "scenario name")?;
            require_name(&scenario.initial_level, "scenario initial_level")?;
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
            validate_schedule(scenario)?;
        }
        Ok(())
    }

    pub fn dump(&self) -> Result<String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| {
            schema_error(
                &format!("cannot serialise game test plan: {error}"),
                "Report this as an engine bug.",
            )
        })
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

fn schema_error(message: &str, hint: &str) -> EngineError {
    EngineError::Schema(message.to_owned(), Some(hint.to_owned()))
}
