//! The Bhippi playtest probe: the GDScript autoload, and the Rust types on both sides of it.
//!
//! A headless Godot run is only useful if something comes back from it. The probe is that
//! something: an autoload that replays a scripted input file frame by frame and appends one
//! JSON line per sample to a telemetry file. Bhippi writes the input file, runs
//! [`playtest_command`](super::command::playtest_command), and reads the telemetry back
//! with [`TelemetryReport::from_jsonl`].
//!
//! The GDScript itself lives in `probe.gd` next to this file rather than inside a Rust
//! string literal, because GDScript is indentation-sensitive and tabs inside a `r#"…"#`
//! literal are exactly the kind of thing an editor silently converts.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::collections::BTreeMap;

/// Where the probe script lives inside a project.
pub const PROBE_RES_PATH: &str = "res://bhippi/probe.gd";
/// The same path, project-relative.
pub const PROBE_REL_PATH: &str = "bhippi/probe.gd";
/// The autoload name scripts call (`BhippiProbe.set_var(…)`).
pub const PROBE_AUTOLOAD_NAME: &str = "BhippiProbe";
/// The version this crate writes into an inputs file.
pub const PLAYTEST_INPUTS_VERSION: u32 = 1;
/// The probe's default sampling interval, in frames. Must match `probe.gd`.
pub const DEFAULT_SAMPLE_EVERY: u32 = 6;
/// The most input steps one playtest may carry.
pub const MAX_PLAYTEST_STEPS: usize = 512;
/// The most telemetry lines a report keeps. Past this the report is marked `truncated`
/// rather than growing without bound — a game stuck in a loop can emit forever.
pub const MAX_TELEMETRY_LINES: usize = 5_000;
/// The prefix every key name in an inputs file carries.
pub const KEY_PREFIX: &str = "KEY_";

/// The GDScript autoload, verbatim.
#[must_use]
pub fn probe_source() -> &'static str {
    include_str!("probe.gd")
}

// ── scripted input ───────────────────────────────────────────────────────────────────

/// One frame's worth of injected input. Exactly one of `action` / `key` must be set.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PlaytestStep {
    pub frame: u32,
    /// An input action name from `project.godot`'s `[input]` section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// A `KEY_*` name, resolved by the probe through `OS.find_keycode_from_string`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub pressed: bool,
}

impl PlaytestStep {
    #[must_use]
    pub fn action(frame: u32, action: &str, pressed: bool) -> Self {
        Self {
            frame,
            action: Some(action.to_owned()),
            key: None,
            pressed,
        }
    }

    #[must_use]
    pub fn key(frame: u32, key: &str, pressed: bool) -> Self {
        Self {
            frame,
            action: None,
            key: Some(key.to_owned()),
            pressed,
        }
    }
}

/// The file the probe reads.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PlaytestInputs {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_every: Option<u32>,
    pub steps: Vec<PlaytestStep>,
}

impl PlaytestInputs {
    #[must_use]
    pub fn new(steps: Vec<PlaytestStep>) -> Self {
        Self {
            version: PLAYTEST_INPUTS_VERSION,
            sample_every: None,
            steps,
        }
    }

    /// Reject anything the probe would silently mis-replay.
    ///
    /// Frames must not go backwards, because the probe walks the list once and never looks
    /// back: an out-of-order step would simply never fire, and the playtest would "pass"
    /// having tested nothing.
    pub fn validate(&self) -> Result<()> {
        if self.version != PLAYTEST_INPUTS_VERSION {
            return Err(EngineError::Action(
                format!(
                    "playtest inputs version {} is not {PLAYTEST_INPUTS_VERSION}",
                    self.version
                ),
                Some("Regenerate the input file with this build of Bhippi.".to_owned()),
            ));
        }
        if self.steps.is_empty() {
            return Err(EngineError::Action(
                "a playtest input file has no steps".to_owned(),
                Some(
                    "Add at least one step, or run the playtest without an input file.".to_owned(),
                ),
            ));
        }
        if self.steps.len() > MAX_PLAYTEST_STEPS {
            return Err(EngineError::Action(
                format!(
                    "{} playtest steps is past the {MAX_PLAYTEST_STEPS} cap",
                    self.steps.len()
                ),
                Some("Split the run into several shorter playtests.".to_owned()),
            ));
        }
        if let Some(sample_every) = self.sample_every {
            if sample_every == 0 {
                return Err(EngineError::Action(
                    "sample_every must be at least 1".to_owned(),
                    Some(format!(
                        "Leave it out to use the default of {DEFAULT_SAMPLE_EVERY}."
                    )),
                ));
            }
        }
        let mut previous = 0u32;
        for (index, step) in self.steps.iter().enumerate() {
            if step.frame < previous {
                return Err(EngineError::Action(
                    format!(
                        "step {index} is at frame {} after frame {previous}",
                        step.frame
                    ),
                    Some("Sort the steps by frame; the probe replays them in order.".to_owned()),
                ));
            }
            previous = step.frame;
            match (step.action.as_deref(), step.key.as_deref()) {
                (Some(action), None) => {
                    if action.trim().is_empty() {
                        return Err(EngineError::Action(
                            format!("step {index} has an empty action name"),
                            Some("Name an action from project.godot's [input] section.".to_owned()),
                        ));
                    }
                }
                (None, Some(key)) => {
                    if keycode_for(key).is_none() {
                        return Err(EngineError::Action(
                            format!("step {index} names an unknown key `{key}`"),
                            Some(format!(
                                "Keys look like {KEY_PREFIX}W, {KEY_PREFIX}SPACE or {KEY_PREFIX}F1."
                            )),
                        ));
                    }
                }
                _ => {
                    return Err(EngineError::Action(
                        format!("step {index} must set exactly one of `action` or `key`"),
                        Some("An action fires by name; a key fires by keycode.".to_owned()),
                    ))
                }
            }
        }
        Ok(())
    }

    /// The JSON the probe reads. Validated first, so an invalid file is never written.
    pub fn to_json(&self) -> Result<String> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| {
            EngineError::Action(
                format!("could not serialise playtest inputs: {error}"),
                None,
            )
        })
    }

    pub fn parse(text: &str) -> Result<Self> {
        let parsed: Self = serde_json::from_str(text).map_err(|error| {
            EngineError::Action(
                format!("playtest inputs are not valid JSON: {error}"),
                Some("Expected {\"version\": 1, \"steps\": [...]}.".to_owned()),
            )
        })?;
        parsed.validate()?;
        Ok(parsed)
    }
}

/// The Godot 4 keycode for a `KEY_*` name, or `None` when the probe could not resolve it.
///
/// The values are Godot's own — confirmed against `OS.find_keycode_from_string` on 4.7.1 —
/// and the set is deliberately limited to names that function accepts, because the probe
/// resolves keys through it at runtime. A name this table does not know is a name the
/// playtest would silently skip.
#[must_use]
pub fn keycode_for(name: &str) -> Option<u32> {
    /// Godot's `Key::SPECIAL` bit.
    const SPECIAL: u32 = 0x0040_0000;
    let bare = name.strip_prefix(KEY_PREFIX)?.to_ascii_uppercase();
    if bare.len() == 1 {
        let character = bare.chars().next()?;
        if character.is_ascii_uppercase() || character.is_ascii_digit() {
            return Some(character as u32);
        }
    }
    if let Some(number) = bare.strip_prefix('F') {
        if let Ok(index) = number.parse::<u32>() {
            if (1..=12).contains(&index) {
                return Some(SPECIAL | (0x1C + index - 1));
            }
        }
    }
    let offset = match bare.as_str() {
        "SPACE" => return Some(32),
        "ESCAPE" => 0x01,
        "TAB" => 0x02,
        "BACKTAB" => 0x03,
        "BACKSPACE" => 0x04,
        "ENTER" => 0x05,
        "INSERT" => 0x07,
        "DELETE" => 0x08,
        "HOME" => 0x0D,
        "END" => 0x0E,
        "LEFT" => 0x0F,
        "UP" => 0x10,
        "RIGHT" => 0x11,
        "DOWN" => 0x12,
        "PAGEUP" => 0x13,
        "PAGEDOWN" => 0x14,
        "SHIFT" => 0x15,
        "CTRL" => 0x16,
        "ALT" => 0x18,
        "CAPSLOCK" => 0x19,
        _ => return None,
    };
    Some(SPECIAL | offset)
}

// ── telemetry ────────────────────────────────────────────────────────────────────────

/// One node in the `bhippi_track` group, as the probe sampled it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct TrackedNode {
    pub path: String,
    /// `[x, y, z]` for a `Node3D`, `[x, y]` for a `Node2D`.
    #[serde(default)]
    pub pos: Option<Vec<f64>>,
    /// Present for `CharacterBody2D`/`CharacterBody3D`.
    #[serde(default)]
    pub vel: Option<Vec<f64>>,
}

/// A moment a script marked with `BhippiProbe.emit_event`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TelemetryEvent {
    #[serde(default)]
    pub frame: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// One sample line.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct TelemetryLine {
    pub frame: u32,
    /// Milliseconds since the process started (`Time.get_ticks_msec`).
    #[serde(default)]
    pub time: i64,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub scene: String,
    #[serde(default)]
    pub node_count: u32,
    #[serde(default)]
    pub tracked: Vec<TrackedNode>,
    #[serde(default)]
    pub vars: BTreeMap<String, Value>,
    #[serde(default)]
    pub events: Vec<TelemetryEvent>,
}

/// Everything one playtest produced.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct TelemetryReport {
    pub samples: Vec<TelemetryLine>,
    /// True when the final `{"done": true}` line arrived — the only proof the game shut
    /// down cleanly rather than being killed mid-frame.
    pub done: bool,
    /// The frame count the game reported on the way out.
    pub frames: Option<u32>,
    /// True when the file held more than [`MAX_TELEMETRY_LINES`] lines.
    pub truncated: bool,
    /// Lines that were not JSON — usually a Godot error printed into the same file.
    pub malformed_lines: usize,
    /// The last position seen for each tracked node.
    pub last_positions: BTreeMap<String, Vec<f64>>,
    /// The variables as of the last sample.
    pub vars: BTreeMap<String, Value>,
    /// Every event, in order.
    pub events: Vec<TelemetryEvent>,
}

impl TelemetryReport {
    /// Read a telemetry file. Never fails: a run that crashed halfway still produced
    /// evidence, and throwing it away because the last line is half-written would discard
    /// exactly the run worth looking at.
    #[must_use]
    pub fn from_jsonl(text: &str) -> Self {
        let mut report = Self::default();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if index >= MAX_TELEMETRY_LINES {
                report.truncated = true;
                break;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                report.malformed_lines += 1;
                continue;
            };
            if value.get("done").and_then(Value::as_bool) == Some(true) {
                report.done = true;
                report.frames = value
                    .get("frames")
                    .and_then(Value::as_u64)
                    .and_then(|frames| u32::try_from(frames).ok());
                continue;
            }
            let Ok(sample) = serde_json::from_value::<TelemetryLine>(value) else {
                report.malformed_lines += 1;
                continue;
            };
            for node in &sample.tracked {
                if let Some(position) = &node.pos {
                    report
                        .last_positions
                        .insert(node.path.clone(), position.clone());
                }
            }
            report.vars = sample.vars.clone();
            report.events.extend(sample.events.iter().cloned());
            report.samples.push(sample);
        }
        report
    }

    #[must_use]
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Every sampled position of one tracked node, in order.
    #[must_use]
    pub fn positions(&self, path: &str) -> Vec<Vec<f64>> {
        self.samples
            .iter()
            .filter_map(|sample| {
                sample
                    .tracked
                    .iter()
                    .find(|node| node.path == path)
                    .and_then(|node| node.pos.clone())
            })
            .collect()
    }

    /// One axis of one tracked node over time — `axis` 1 is Y in both 2D and 3D.
    #[must_use]
    pub fn axis_series(&self, path: &str, axis: usize) -> Vec<f64> {
        self.positions(path)
            .into_iter()
            .filter_map(|position| position.get(axis).copied())
            .collect()
    }

    /// The names of every event recorded, in order.
    #[must_use]
    pub fn event_names(&self) -> Vec<String> {
        self.events.iter().map(|event| event.name.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        keycode_for, probe_source, PlaytestInputs, PlaytestStep, TelemetryReport,
        DEFAULT_SAMPLE_EVERY, MAX_PLAYTEST_STEPS, PROBE_AUTOLOAD_NAME, PROBE_RES_PATH,
    };

    #[test]
    fn the_autoload_defines_the_api_the_rust_side_and_the_game_both_rely_on() {
        let source = probe_source();
        for needle in [
            "extends Node",
            "func _ready() -> void:",
            "func set_var(key: String, value: Variant) -> void:",
            "func emit_event(event_name: String, data: Variant = null) -> void:",
            "func _process(_delta: float) -> void:",
            "func _inject(step: Dictionary) -> void:",
            "func _sample() -> Dictionary:",
            "func _notification(what: int) -> void:",
            "func _exit_tree() -> void:",
            "OS.get_cmdline_user_args()",
            "Input.parse_input_event",
            "InputEventAction.new()",
            "InputEventKey.new()",
            "OS.find_keycode_from_string",
            "bhippi_track",
            "--bhippi-inputs=",
            "--bhippi-telemetry=",
            "NOTIFICATION_WM_CLOSE_REQUEST",
            "\"done\": true",
        ] {
            assert!(source.contains(needle), "probe.gd must contain `{needle}`");
        }
        assert_eq!(PROBE_RES_PATH, "res://bhippi/probe.gd");
        assert_eq!(PROBE_AUTOLOAD_NAME, "BhippiProbe");
    }

    /// GDScript refuses a file that mixes tabs and spaces for indentation, and a probe that
    /// will not parse is a playtest that reports nothing at all.
    #[test]
    fn the_autoload_is_indented_with_tabs_only() {
        for (number, line) in probe_source().lines().enumerate() {
            assert!(
                !line.starts_with(' '),
                "probe.gd line {} is space-indented: {line:?}",
                number + 1
            );
            assert!(
                !line.contains('\r'),
                "probe.gd line {} carries a CR",
                number + 1
            );
        }
        assert!(probe_source().contains('\t'));
        assert!(probe_source().contains(&format!("DEFAULT_SAMPLE_EVERY := {DEFAULT_SAMPLE_EVERY}")));
    }

    #[test]
    fn keycodes_match_the_values_godot_resolves() {
        // Confirmed against OS.find_keycode_from_string on Godot 4.7.1.
        assert_eq!(keycode_for("KEY_A"), Some(65));
        assert_eq!(keycode_for("KEY_Z"), Some(90));
        assert_eq!(keycode_for("KEY_0"), Some(48));
        assert_eq!(keycode_for("KEY_SPACE"), Some(32));
        assert_eq!(keycode_for("KEY_ESCAPE"), Some(4_194_305));
        assert_eq!(keycode_for("KEY_ENTER"), Some(4_194_309));
        assert_eq!(keycode_for("KEY_LEFT"), Some(4_194_319));
        assert_eq!(keycode_for("KEY_DOWN"), Some(4_194_322));
        assert_eq!(keycode_for("KEY_F1"), Some(4_194_332));
        assert_eq!(keycode_for("KEY_F12"), Some(4_194_343));
        assert_eq!(keycode_for("KEY_NOPE"), None);
        assert_eq!(keycode_for("KEY_F13"), None);
        // The prefix is mandatory: a bare `W` would be ambiguous with an action name.
        assert_eq!(keycode_for("W"), None);
    }

    #[test]
    fn a_valid_input_file_round_trips_through_json() {
        let inputs = PlaytestInputs::new(vec![
            PlaytestStep::action(0, "jump", true),
            PlaytestStep::action(2, "jump", false),
            PlaytestStep::key(10, "KEY_W", true),
        ]);
        let json = inputs.to_json().expect("valid inputs serialise");
        assert!(json.contains("\"version\":1"));
        assert!(!json.contains("\"key\":null"), "absent fields stay absent");
        assert_eq!(PlaytestInputs::parse(&json).expect("parses"), inputs);
    }

    #[test]
    fn inputs_the_probe_would_mis_replay_are_refused_with_a_hint() {
        let cases: Vec<PlaytestInputs> = vec![
            PlaytestInputs::new(Vec::new()),
            PlaytestInputs::new(vec![
                PlaytestStep::action(10, "jump", true),
                PlaytestStep::action(2, "jump", false),
            ]),
            PlaytestInputs::new(vec![PlaytestStep::key(0, "KEY_NOPE", true)]),
            PlaytestInputs::new(vec![PlaytestStep::action(0, "  ", true)]),
            PlaytestInputs::new(vec![PlaytestStep {
                frame: 0,
                action: Some("jump".to_owned()),
                key: Some("KEY_W".to_owned()),
                pressed: true,
            }]),
            PlaytestInputs::new(vec![PlaytestStep {
                frame: 0,
                action: None,
                key: None,
                pressed: true,
            }]),
            PlaytestInputs {
                version: 2,
                sample_every: None,
                steps: vec![PlaytestStep::action(0, "jump", true)],
            },
            PlaytestInputs {
                version: 1,
                sample_every: Some(0),
                steps: vec![PlaytestStep::action(0, "jump", true)],
            },
            PlaytestInputs::new(
                (0..=MAX_PLAYTEST_STEPS)
                    .map(|frame| {
                        PlaytestStep::action(u32::try_from(frame).unwrap_or(0), "jump", true)
                    })
                    .collect(),
            ),
        ];
        for inputs in cases {
            let error = inputs.validate().expect_err("must be refused");
            assert!(error.hint().is_some(), "{error} needs a hint");
        }
    }

    const SAMPLE_LOG: &str = concat!(
        r#"{"frame":0,"time":16,"fps":60,"scene":"Main","node_count":9,"tracked":[{"path":"/root/Main/Player","pos":[0,1,0],"vel":[0,0,0]}],"vars":{},"events":[]}"#,
        "\n",
        r#"{"frame":6,"time":116,"fps":60,"scene":"Main","node_count":9,"tracked":[{"path":"/root/Main/Player","pos":[0,1.8,0],"vel":[0,3.2,0]}],"vars":{"player_y":1.8},"events":[{"frame":6,"name":"jumped","data":null}]}"#,
        "\n",
        "Godot error: something printed into the same file\n",
        r#"{"frame":12,"time":216,"fps":60,"scene":"Main","node_count":9,"tracked":[{"path":"/root/Main/Player","pos":[0,2.4,0]}],"vars":{"player_y":2.4},"events":[]}"#,
        "\n",
        r#"{"done":true,"frames":120}"#,
        "\n",
    );

    #[test]
    fn a_telemetry_file_reads_back_into_a_report() {
        let report = TelemetryReport::from_jsonl(SAMPLE_LOG);
        assert_eq!(report.sample_count(), 3);
        assert!(report.done);
        assert_eq!(report.frames, Some(120));
        assert_eq!(report.malformed_lines, 1);
        assert!(!report.truncated);
        assert_eq!(
            report.last_positions.get("/root/Main/Player"),
            Some(&vec![0.0, 2.4, 0.0])
        );
        assert_eq!(report.event_names(), vec!["jumped"]);
        let ys = report.axis_series("/root/Main/Player", 1);
        assert_eq!(ys, vec![1.0, 1.8, 2.4]);
        assert!(ys.last() > ys.first(), "the player rose after the jump");
        assert!(report.vars.contains_key("player_y"));
    }

    #[test]
    fn a_run_that_never_stopped_is_truncated_and_says_so() {
        let mut text = String::new();
        for frame in 0..(super::MAX_TELEMETRY_LINES + 10) {
            text.push_str(&format!("{{\"frame\":{frame},\"tracked\":[]}}\n"));
        }
        let report = TelemetryReport::from_jsonl(&text);
        assert!(report.truncated);
        assert!(!report.done);
        assert_eq!(report.sample_count(), super::MAX_TELEMETRY_LINES);
    }

    #[test]
    fn a_crashed_run_still_yields_what_it_managed_to_write() {
        let report =
            TelemetryReport::from_jsonl("{\"frame\":0,\"tracked\":[]}\n{\"frame\":6,\"tracke");
        assert_eq!(report.sample_count(), 1);
        assert_eq!(report.malformed_lines, 1);
        assert!(
            !report.done,
            "no done line means the game did not shut down"
        );
    }
}
