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
