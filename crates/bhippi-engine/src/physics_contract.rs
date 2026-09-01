//! Backend-neutral physics domain contracts (Phase 18).
//!
//! These values describe validated bodies, colliders, queries, constraints and deterministic
//! tolerances. They do not integrate or simulate a physics backend.

use crate::error::{EngineError, Result};
use crate::registry::CapabilityRegistry;
use crate::runtime_contract::{RuntimeEntityHandle, RuntimeResourceHandle};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;

pub const PHYSICS_CONTRACT_FORMAT: &str = "bhippi-physics-contract@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    Static,
    Dynamic,
    Kinematic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CollisionDetection {
    Discrete,
    Continuous,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PhysicsMaterialContract {
    pub id: String,
    pub friction: f32,
    pub restitution: f32,
    pub density: f32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CollisionLayerContract {
    pub id: String,
    pub bit: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ColliderShapeContract {
    Cuboid {
        half_extents: [f32; 3],
    },
    Sphere {
        radius: f32,
    },
    Capsule {
        radius: f32,
        half_height: f32,
    },
    Convex {
        mesh: RuntimeResourceHandle,
    },
    Heightfield {
        field: RuntimeResourceHandle,
    },
    Compound {
        children: Vec<ColliderShapeContract>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ColliderContract {
    pub id: String,
    pub shape: ColliderShapeContract,
    pub material: String,
    pub layer: String,
    #[serde(default)]
    pub collides_with: Vec<String>,
    pub sensor: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct BodyContract {
    pub entity: RuntimeEntityHandle,
    pub kind: BodyKind,
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub gravity_scale: f32,
    pub collision_detection: CollisionDetection,
    #[serde(default)]
    pub colliders: Vec<ColliderContract>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PhysicsQueryFilter {
    #[serde(default)]
    pub layers: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<RuntimeEntityHandle>,
    pub include_sensors: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum PhysicsQueryContract {
    Raycast {
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        filter: PhysicsQueryFilter,
    },
    ShapeCast {
        shape: ColliderShapeContract,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        filter: PhysicsQueryFilter,
    },
    Overlap {
        shape: ColliderShapeContract,
        origin: [f32; 3],
        filter: PhysicsQueryFilter,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum PhysicsCommandContract {
    AddForce {
        entity: RuntimeEntityHandle,
        force: [f32; 3],
    },
    AddImpulse {
        entity: RuntimeEntityHandle,
        impulse: [f32; 3],
    },
    AddTorque {
        entity: RuntimeEntityHandle,
        torque: [f32; 3],
    },
    SetVelocity {
        entity: RuntimeEntityHandle,
        linear: [f32; 3],
        angular: [f32; 3],
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ConstraintKindContract {
    Fixed,
    Hinge {
        axis: [f32; 3],
        limits: Option<[f32; 2]>,
    },
    Slider {
        axis: [f32; 3],
        limits: Option<[f32; 2]>,
    },
    Spring {
        stiffness: f32,
        damping: f32,
        rest_length: f32,
    },
    Rope {
        maximum_length: f32,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ConstraintContract {
    pub id: String,
    pub first: RuntimeEntityHandle,
    pub second: RuntimeEntityHandle,
    pub kind: ConstraintKindContract,
    pub break_force: Option<f32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PhysicsLaneContract {
    pub format: String,
    pub capability_ids: Vec<String>,
    pub step_micros: u64,
    pub maximum_substeps: u32,
    pub position_tolerance: f32,
    pub velocity_tolerance: f32,
    pub cpu_micros_per_step: u64,
    pub resident_bytes: u64,
    pub materials: Vec<PhysicsMaterialContract>,
    pub layers: Vec<CollisionLayerContract>,
}

impl PhysicsLaneContract {
    pub fn validate(&self, registry: &CapabilityRegistry) -> Result<()> {
        if self.format != PHYSICS_CONTRACT_FORMAT {
            return Err(error(
                "unsupported physics contract format",
                "Use bhippi-physics-contract@1.",
            ));
        }
        for capability in &self.capability_ids {
            if registry.describe(capability).is_none() {
                return Err(error(
                    &format!("physics contract names unknown capability `{capability}`"),
                    "Register the capability before binding it to physics.",
                ));
            }
        }
        if self.step_micros == 0
            || self.maximum_substeps == 0
            || self.cpu_micros_per_step == 0
            || self.resident_bytes == 0
            || !positive(self.position_tolerance)
            || !positive(self.velocity_tolerance)
        {
            return Err(error(
                "physics lane has an invalid tolerance or budget",
                "Declare non-zero fixed-step, tolerance, CPU and memory limits.",
            ));
        }
        let mut material_ids = BTreeSet::new();
        for material in &self.materials {
            validate_id(&material.id)?;
            if !material_ids.insert(material.id.as_str())
                || !unit(material.friction)
                || !unit(material.restitution)
                || !positive(material.density)
            {
                return Err(error(
                    "physics material is duplicate or outside its validated range",
                    "Use unique ids, friction/restitution in 0..1 and positive density.",
                ));
            }
        }
        let mut layer_ids = BTreeSet::new();
        let mut layer_bits = BTreeSet::new();
        for layer in &self.layers {
            validate_id(&layer.id)?;
            if !layer_ids.insert(layer.id.as_str()) || !layer_bits.insert(layer.bit) {
                return Err(error(
                    "collision layers repeat an id or bit",
                    "Assign every collision layer one unique id and bit.",
                ));
            }
        }
        Ok(())
    }

    pub fn validate_body(&self, body: &BodyContract) -> Result<()> {
        if body.kind == BodyKind::Dynamic && !positive(body.mass) {
            return Err(error(
                "dynamic body mass must be positive",
                "Set a positive mass.",
            ));
        }
        if !non_negative(body.linear_damping)
            || !non_negative(body.angular_damping)
            || !body.gravity_scale.is_finite()
        {
            return Err(error(
                "body damping or gravity is invalid",
                "Use finite non-negative damping.",
            ));
        }
        let materials = self
            .materials
            .iter()
            .map(|item| item.id.as_str())
            .collect::<BTreeSet<_>>();
        let layers = self
            .layers
            .iter()
            .map(|item| item.id.as_str())
            .collect::<BTreeSet<_>>();
        for collider in &body.colliders {
            validate_id(&collider.id)?;
            validate_shape(&collider.shape)?;
            if !materials.contains(collider.material.as_str())
                || !layers.contains(collider.layer.as_str())
                || collider
                    .collides_with
                    .iter()
                    .any(|layer| !layers.contains(layer.as_str()))
            {
                return Err(error(
                    "collider references an unknown material or layer",
                    "Choose ids declared by the physics lane contract.",
                ));
            }
        }
        Ok(())
    }
}

impl PhysicsQueryContract {
    pub fn validate(&self, lane: &PhysicsLaneContract) -> Result<()> {
        match self {
            Self::Raycast {
                direction,
                max_distance,
                filter,
                ..
            }
            | Self::ShapeCast {
                direction,
                max_distance,
                filter,
                ..
            } => {
                if !positive(*max_distance) || !non_zero_vector(*direction) {
                    return Err(error(
                        "cast direction/distance is invalid",
                        "Use a non-zero direction and positive distance.",
                    ));
                }
                validate_filter(filter, lane)?;
                if let Self::ShapeCast { shape, .. } = self {
                    validate_shape(shape)?;
                }
            }
            Self::Overlap { shape, filter, .. } => {
                validate_shape(shape)?;
                validate_filter(filter, lane)?;
            }
        }
        Ok(())
    }
}

impl ConstraintContract {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        if self.first == self.second {
            return Err(error(
                "constraint endpoints are identical",
                "Connect two different runtime entities.",
            ));
        }
        if self.break_force.is_some_and(|value| !positive(value)) {
            return Err(error(
                "constraint break force is invalid",
                "Use a positive break force or omit it.",
            ));
        }
        match &self.kind {
            ConstraintKindContract::Hinge { axis, limits }
            | ConstraintKindContract::Slider { axis, limits } => {
                if !non_zero_vector(*axis) || limits.is_some_and(|range| range[0] > range[1]) {
                    return Err(error(
                        "constraint axis/limits are invalid",
                        "Use a non-zero axis and ordered limits.",
                    ));
                }
            }
            ConstraintKindContract::Spring {
                stiffness,
                damping,
                rest_length,
            } => {
                if !positive(*stiffness) || !non_negative(*damping) || !positive(*rest_length) {
                    return Err(error(
                        "spring parameters are invalid",
                        "Use positive stiffness/rest length and non-negative damping.",
                    ));
                }
            }
            ConstraintKindContract::Rope { maximum_length } if !positive(*maximum_length) => {
                return Err(error(
                    "rope length is invalid",
                    "Use a positive maximum length.",
                ));
            }
            _ => {}
        }
        Ok(())
    }
}

fn validate_shape(shape: &ColliderShapeContract) -> Result<()> {
    let valid = match shape {
        ColliderShapeContract::Cuboid { half_extents } => {
            half_extents.iter().all(|value| positive(*value))
        }
        ColliderShapeContract::Sphere { radius } => positive(*radius),
        ColliderShapeContract::Capsule {
            radius,
            half_height,
        } => positive(*radius) && positive(*half_height),
        ColliderShapeContract::Convex { .. } | ColliderShapeContract::Heightfield { .. } => true,
        ColliderShapeContract::Compound { children } => !children.is_empty(),
    };
    if !valid {
        return Err(error(
            "collider shape is empty or invalid",
            "Use positive dimensions and non-empty compound children.",
        ));
    }
    if let ColliderShapeContract::Compound { children } = shape {
        for child in children {
            validate_shape(child)?;
        }
    }
    Ok(())
}

fn validate_filter(filter: &PhysicsQueryFilter, lane: &PhysicsLaneContract) -> Result<()> {
    let layers = lane
        .layers
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    if filter
        .layers
        .iter()
        .any(|layer| !layers.contains(layer.as_str()))
    {
        return Err(error(
            "physics query names an unknown layer",
            "Choose a layer declared by the lane.",
        ));
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
            &format!("`{id}` is not a canonical physics id"),
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
fn non_zero_vector(value: [f32; 3]) -> bool {
    value.iter().all(|item| item.is_finite()) && value.iter().any(|item| *item != 0.0)
}
fn error(message: &str, hint: &str) -> EngineError {
    EngineError::Schema(message.to_owned(), Some(hint.to_owned()))
}
