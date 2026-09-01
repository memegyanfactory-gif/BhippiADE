//! Hand-editable runtime input map (`assets/input.json`).
//!
//! The webview runtime consumes this validated document. Keeping the vocabulary here makes
//! input names stable for HUD glyphs, scripts and AI edits without embedding key choices in
//! the renderer.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const INPUT_FORMAT: &str = "bhippi-input@1";
pub const DEFAULT_INPUT_PATH: &str = "assets/input.json";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, specta::Type)]
pub struct InputDocument {
    pub format: String,
    #[serde(default)]
    pub actions: Vec<ActionBinding>,
    #[serde(default)]
    pub axes: Vec<AxisBinding>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, specta::Type)]
pub struct ActionBinding {
    pub name: String,
    pub keys: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, specta::Type)]
pub struct AxisBinding {
    pub name: String,
    pub positive: Vec<String>,
    pub negative: Vec<String>,
}

impl Default for InputDocument {
    fn default() -> Self {
        Self {
            format: INPUT_FORMAT.to_owned(),
            actions: vec![
                ActionBinding {
                    name: "jump".to_owned(),
                    keys: vec!["Space".to_owned(), "Gamepad0".to_owned()],
                },
                ActionBinding {
                    name: "pause".to_owned(),
                    keys: vec!["Escape".to_owned(), "Gamepad9".to_owned()],
                },
                ActionBinding {
                    name: "fire".to_owned(),
                    keys: vec!["Mouse0".to_owned(), "Gamepad5".to_owned()],
                },
            ],
            axes: vec![
                AxisBinding {
                    name: "move_x".to_owned(),
                    positive: vec!["KeyD".to_owned(), "ArrowRight".to_owned()],
                    negative: vec!["KeyA".to_owned(), "ArrowLeft".to_owned()],
                },
                AxisBinding {
                    name: "move_z".to_owned(),
                    positive: vec!["KeyS".to_owned(), "ArrowDown".to_owned()],
                    negative: vec!["KeyW".to_owned(), "ArrowUp".to_owned()],
                },
            ],
        }
    }
}

impl InputDocument {
    pub fn parse(text: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(text).map_err(|error| {
            EngineError::Scene(
                format!("invalid input map: {error}"),
                Some("Fix assets/input.json or restore the generated default.".to_owned()),
            )
        })?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != INPUT_FORMAT {
            return Err(EngineError::Scene(
                format!("unsupported input format {:?}", self.format),
                Some(format!("Set format to {INPUT_FORMAT}.")),
            ));
        }
        let mut names = BTreeSet::new();
        for (name, keys) in self
            .actions
            .iter()
            .map(|binding| (&binding.name, binding.keys.as_slice()))
            .chain(self.axes.iter().flat_map(|binding| {
                [
                    (&binding.name, binding.positive.as_slice()),
                    (&binding.name, binding.negative.as_slice()),
                ]
            }))
        {
            if name.trim().is_empty() || keys.iter().any(|key| key.trim().is_empty()) {
                return Err(EngineError::Scene(
                    "input names and key codes must not be empty".to_owned(),
                    Some("Use DOM KeyboardEvent.code names such as KeyW or Space.".to_owned()),
                ));
            }
        }
        for name in self
            .actions
            .iter()
            .map(|binding| binding.name.as_str())
            .chain(self.axes.iter().map(|binding| binding.name.as_str()))
        {
            if !names.insert(name) {
                return Err(EngineError::Scene(
                    format!("duplicate input binding {name:?}"),
                    Some("Give every action and axis a unique name.".to_owned()),
                ));
            }
        }
        Ok(())
    }

    pub fn dump(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            EngineError::Scene(
                format!("cannot serialise input map: {error}"),
                Some("Report this as an engine bug.".to_owned()),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{InputDocument, INPUT_FORMAT};

    #[test]
    fn defaults_round_trip_and_expose_named_gameplay_inputs() {
        let input = InputDocument::default();
        let text = input.dump().expect("dump");
        assert_eq!(InputDocument::parse(&text).expect("parse"), input);
        assert!(text.contains("move_x"));
        assert!(text.contains("jump"));
    }

    #[test]
    fn invalid_or_duplicate_bindings_block() {
        let mut input = InputDocument {
            format: "future".to_owned(),
            ..InputDocument::default()
        };
        assert!(input.validate().is_err());

        input.format = INPUT_FORMAT.to_owned();
        input.axes[0].name = input.actions[0].name.clone();
        let error = input.validate().expect_err("duplicate must block");
        assert!(error.hint().is_some());
    }
}
