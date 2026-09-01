//! Input, character-controller preset and camera-rig contracts (Phase 18).
//!
//! These are backend/device/UI-neutral validated documents. They do not read devices, move a
//! character, collide a camera or render an editor.

use crate::error::{EngineError, Result};
use crate::input::InputDocument;
use crate::registry::CapabilityRegistry;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;

pub const CONTROL_CONTRACT_FORMAT: &str = "bhippi-control-contract@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum InputDeviceClass {
    KeyboardMouse,
    Gamepad,
    Touch,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum InputTrigger {
    Pressed,
    Released,
    Held,
    Axis,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ContextBinding {
    pub action: String,
    pub trigger: InputTrigger,
    /// Simultaneous stable input codes. One code is an ordinary binding; two or more is a chord.
    pub chord: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct InputContextContract {
    pub id: String,
    pub priority: i32,
    pub enabled_by_default: bool,
    #[serde(default)]
    pub blocks_lower_contexts: bool,
    pub bindings: Vec<ContextBinding>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RebindConflictPolicy {
    Reject,
    UnbindExisting,
    AllowShared,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RebindingContract {
    pub enabled: bool,
    pub conflict_policy: RebindConflictPolicy,
    #[serde(default)]
    pub reserved_codes: Vec<String>,
    pub persist_profile: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AccessibilityInputContract {
    pub hold_to_toggle_supported: bool,
    pub simultaneous_chord_alternative_required: bool,
    pub stick_deadzone: f32,
    pub pointer_sensitivity: f32,
    pub vibration_scale: f32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct DeviceSupportContract {
    pub device: InputDeviceClass,
    pub supported: bool,
    pub evidence: Option<String>,
    pub limitation: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct InputControlContract {
    pub contexts: Vec<InputContextContract>,
    pub rebinding: RebindingContract,
    pub accessibility: AccessibilityInputContract,
    pub devices: Vec<DeviceSupportContract>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CharacterPresetKind {
    FirstPerson,
    ThirdPerson,
    Platformer,
    TopDown,
    Flying,
}

impl CharacterPresetKind {
    pub const ALL: [Self; 5] = [
        Self::FirstPerson,
        Self::ThirdPerson,
        Self::Platformer,
        Self::TopDown,
        Self::Flying,
    ];
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CharacterMovementContract {
    pub move_speed: f32,
    pub acceleration: f32,
    pub braking: f32,
    pub jump_speed: f32,
    pub air_control: f32,
    pub max_slope_radians: f32,
    pub step_height: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CharacterModesContract {
    pub crouch_height: Option<f32>,
    pub slide_speed: Option<f32>,
    pub swim_speed: Option<f32>,
    pub climb_speed: Option<f32>,
    pub ladder_speed: Option<f32>,
    pub mantle_height: Option<f32>,
    pub root_motion: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CharacterPresetContract {
    pub id: String,
    pub kind: CharacterPresetKind,
    pub movement: CharacterMovementContract,
    pub modes: CharacterModesContract,
    pub input_context: String,
    pub camera_rig: String,
    pub capability_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CameraRigKind {
    FirstPerson,
    ThirdPerson,
    Platformer,
    TopDown,
    Racing,
    Cinematic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CameraBlendCurve {
    Linear,
    SmoothStep,
    EaseIn,
    EaseOut,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CameraBlendContract {
    pub duration_millis: u32,
    pub curve: CameraBlendCurve,
    pub preserve_target: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CameraCollisionContract {
    pub enabled: bool,
    pub probe_radius: f32,
    pub minimum_distance: f32,
    pub restore_speed: f32,
    pub occlusion_fade: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CameraModifierContract {
    pub position_damping: f32,
    pub rotation_damping: f32,
    pub maximum_shake: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CameraRigContract {
    pub id: String,
    pub kind: CameraRigKind,
    pub offset: [f32; 3],
    pub fov_radians: f32,
    pub tracking_strength: f32,
    pub blend: CameraBlendContract,
    pub collision: CameraCollisionContract,
    pub modifiers: CameraModifierContract,
    pub capability_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ControlContractSet {
    pub format: String,
    pub capability_registry_hash: String,
    pub input: InputControlContract,
    pub characters: Vec<CharacterPresetContract>,
    pub cameras: Vec<CameraRigContract>,
}

impl ControlContractSet {
    pub fn validate(
        &self,
        base_input: &InputDocument,
        registry: &CapabilityRegistry,
    ) -> Result<()> {
        if self.format != CONTROL_CONTRACT_FORMAT {
            return Err(error(
                "unsupported control contract format",
                "Use bhippi-control-contract@1.",
            ));
        }
        if self.capability_registry_hash != registry.hash {
            return Err(error(
                "control contract registry hash is stale",
                "Rebuild it against the active capability registry.",
            ));
        }
        base_input.validate()?;
        self.input.validate(base_input)?;

        let contexts = self
            .input
            .contexts
            .iter()
            .map(|item| item.id.as_str())
            .collect::<BTreeSet<_>>();
        let cameras = self
            .cameras
            .iter()
            .map(|item| item.id.as_str())
            .collect::<BTreeSet<_>>();
        if contexts.len() != self.input.contexts.len() || cameras.len() != self.cameras.len() {
            return Err(error(
                "control context/camera ids are duplicated",
                "Use one stable id per context and camera rig.",
            ));
        }
        let mut character_ids = BTreeSet::new();
        for character in &self.characters {
            character.validate(registry)?;
            if !character_ids.insert(character.id.as_str())
                || !contexts.contains(character.input_context.as_str())
                || !cameras.contains(character.camera_rig.as_str())
            {
                return Err(error(
                    "character preset has duplicate or dangling control references",
                    "Choose declared context and camera ids.",
                ));
            }
        }
        for camera in &self.cameras {
            camera.validate(registry)?;
        }
        Ok(())
    }
}

impl InputControlContract {
    pub fn validate(&self, base: &InputDocument) -> Result<()> {
        let names = base
            .actions
            .iter()
            .map(|item| item.name.as_str())
            .chain(base.axes.iter().map(|item| item.name.as_str()))
            .collect::<BTreeSet<_>>();
        let reserved = self
            .rebinding
            .reserved_codes
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if reserved.len() != self.rebinding.reserved_codes.len() {
            return Err(error(
                "reserved input codes repeat",
                "List each reserved code once.",
            ));
        }
        let mut contexts = BTreeSet::new();
        for context in &self.contexts {
            validate_id(&context.id)?;
            if !contexts.insert(context.id.as_str()) || context.bindings.is_empty() {
                return Err(error(
                    "input context is duplicate or empty",
                    "Use a unique id and at least one binding.",
                ));
            }
            let mut chords = BTreeSet::new();
            for binding in &context.bindings {
                if !names.contains(binding.action.as_str())
                    || binding.chord.is_empty()
                    || binding.chord.iter().any(|code| code.trim().is_empty())
                {
                    return Err(error(
                        "context binding names an unknown action or empty chord",
                        "Use a base input action/axis and non-empty stable codes.",
                    ));
                }
                let signature = format!(
                    "{}:{:?}:{}",
                    binding.action,
                    binding.trigger,
                    binding.chord.join("+")
                );
                if !chords.insert(signature) {
                    return Err(error(
                        "context repeats a binding",
                        "Remove the duplicate action/chord.",
                    ));
                }
            }
        }
        let accessibility = &self.accessibility;
        if !unit(accessibility.stick_deadzone)
            || !positive(accessibility.pointer_sensitivity)
            || !unit(accessibility.vibration_scale)
        {
            return Err(error(
                "input accessibility values are out of range",
                "Use deadzone/vibration in 0..1 and positive sensitivity.",
            ));
        }
        let devices = self
            .devices
            .iter()
            .map(|item| item.device)
            .collect::<BTreeSet<_>>();
        if devices.len() != self.devices.len()
            || InputDeviceClass::ALL
                .into_iter()
                .any(|device| !devices.contains(&device))
        {
            return Err(error(
                "input device matrix is incomplete or duplicated",
                "Declare keyboard/mouse, gamepad and touch exactly once.",
            ));
        }
        for device in &self.devices {
            if device.supported && device.evidence.as_deref().is_none_or(str::is_empty) {
                return Err(error(
                    "input device support lacks evidence",
                    "Name the passing device fixture.",
                ));
            }
            if !device.supported && device.limitation.as_deref().is_none_or(str::is_empty) {
                return Err(error(
                    "unsupported input device lacks a limitation",
                    "State why the device is unavailable.",
                ));
            }
        }
        Ok(())
    }
}

impl InputDeviceClass {
    pub const ALL: [Self; 3] = [Self::KeyboardMouse, Self::Gamepad, Self::Touch];
}

impl CharacterPresetContract {
    pub fn validate(&self, registry: &CapabilityRegistry) -> Result<()> {
        validate_id(&self.id)?;
        validate_capabilities(&self.capability_ids, registry)?;
        let movement = &self.movement;
        if !positive(movement.move_speed)
            || !positive(movement.acceleration)
            || !non_negative(movement.braking)
            || !non_negative(movement.jump_speed)
            || !unit(movement.air_control)
            || !positive(movement.max_slope_radians)
            || !non_negative(movement.step_height)
        {
            return Err(error(
                "character movement parameters are invalid",
                "Use finite positive movement values and air control in 0..1.",
            ));
        }
        for value in [
            self.modes.crouch_height,
            self.modes.slide_speed,
            self.modes.swim_speed,
            self.modes.climb_speed,
            self.modes.ladder_speed,
            self.modes.mantle_height,
        ]
        .into_iter()
        .flatten()
        {
            if !positive(value) {
                return Err(error(
                    "character mode parameter is invalid",
                    "Enabled modes require positive finite values.",
                ));
            }
        }
        Ok(())
    }
}

impl CameraRigContract {
    pub fn validate(&self, registry: &CapabilityRegistry) -> Result<()> {
        validate_id(&self.id)?;
        validate_capabilities(&self.capability_ids, registry)?;
        if !self.offset.iter().all(|value| value.is_finite())
            || !self.fov_radians.is_finite()
            || !(0.0..std::f32::consts::PI).contains(&self.fov_radians)
            || !unit(self.tracking_strength)
            || self.blend.duration_millis == 0
            || !non_negative(self.collision.probe_radius)
            || !non_negative(self.collision.minimum_distance)
            || !positive(self.collision.restore_speed)
            || !non_negative(self.modifiers.position_damping)
            || !non_negative(self.modifiers.rotation_damping)
            || !non_negative(self.modifiers.maximum_shake)
        {
            return Err(error(
                "camera rig contains invalid blend/collision/modifier values",
                "Use finite values, FOV in 0..PI, tracking in 0..1 and non-zero blend time.",
            ));
        }
        if self.collision.enabled && self.collision.probe_radius == 0.0 {
            return Err(error(
                "camera collision has no probe radius",
                "Use a positive radius or disable collision.",
            ));
        }
        Ok(())
    }
}

fn validate_capabilities(ids: &[String], registry: &CapabilityRegistry) -> Result<()> {
    for id in ids {
        if registry.describe(id).is_none() {
            return Err(error(
                &format!("control contract names unknown capability `{id}`"),
                "Register it before binding the preset.",
            ));
        }
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<()> {
    let valid = !id.is_empty()
        && id.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        });
    valid.then_some(()).ok_or_else(|| {
        error(
            &format!("`{id}` is not a canonical control id"),
            "Use lowercase dotted segments.",
        )
    })
}

fn positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}
fn non_negative(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}
fn unit(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}
fn error(message: &str, hint: &str) -> EngineError {
    EngineError::Schema(message.to_owned(), Some(hint.to_owned()))
}
