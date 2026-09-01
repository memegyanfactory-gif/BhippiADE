//! Versioned animation, skeleton and rigging contracts (Phase 19).
//!
//! This module validates domain documents. It does not import assets, evaluate a pose, skin a
//! mesh, run an IK solver, cache poses or expose an editor/runtime backend.

use crate::error::{EngineError, Result};
use crate::registry::CapabilityRegistry;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const ANIMATION_CONTRACT_FORMAT: &str = "bhippi-animation-contract@1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct BoneContract {
    pub id: String,
    pub index: u32,
    pub parent: Option<String>,
    pub inverse_bind: [f32; 16],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SkeletonContract {
    pub id: String,
    pub bones: Vec<BoneContract>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TransformKey {
    pub time_seconds: f32,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct BoneTrackContract {
    pub bone: String,
    pub keys: Vec<TransformKey>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AnimationEventContract {
    pub id: String,
    pub time_seconds: f32,
    pub payload_schema: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CompressionContract {
    pub translation_error: f32,
    pub rotation_error_radians: f32,
    pub scale_error: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AnimationClipContract {
    pub id: String,
    pub skeleton: String,
    pub duration_seconds: f32,
    pub looping: bool,
    pub root_motion_bone: Option<String>,
    pub tracks: Vec<BoneTrackContract>,
    pub events: Vec<AnimationEventContract>,
    pub compression: CompressionContract,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum AnimationNodeKind {
    Clip {
        clip: String,
    },
    State {
        source: String,
    },
    Blend1d {
        parameter: String,
        points: Vec<(f32, String)>,
    },
    Blend2d {
        parameters: [String; 2],
        points: Vec<([f32; 2], String)>,
    },
    Additive {
        base: String,
        additive: String,
        weight_parameter: String,
    },
    Layer {
        base: String,
        overlay: String,
        bone_mask: Vec<String>,
    },
    Montage {
        clips: Vec<String>,
    },
    PoseCache {
        source: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AnimationNodeContract {
    pub id: String,
    pub kind: AnimationNodeKind,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AnimationTransitionContract {
    pub from: String,
    pub to: String,
    pub parameter: String,
    pub threshold: f32,
    pub blend_seconds: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AnimationLayerContract {
    pub id: String,
    pub entry_node: String,
    pub weight: f32,
    pub additive: bool,
    pub bone_mask: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AnimationGraphContract {
    pub id: String,
    pub parameters: BTreeMap<String, f32>,
    pub nodes: Vec<AnimationNodeContract>,
    pub transitions: Vec<AnimationTransitionContract>,
    pub layers: Vec<AnimationLayerContract>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum IkSolverKind {
    ForwardKinematics,
    TwoBone,
    Ccd,
    Fabrik,
    Foot,
    Hand,
    LookAt,
    Aim,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct IkConstraintContract {
    pub id: String,
    pub solver: IkSolverKind,
    pub chain: Vec<String>,
    pub target: [f32; 3],
    pub pole_target: Option<[f32; 3]>,
    pub weight: f32,
    pub iterations: u32,
    pub tolerance: f32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RetargetContract {
    pub source_skeleton: String,
    pub target_skeleton: String,
    pub bone_map: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AnimationBudgetContract {
    pub maximum_characters: u32,
    pub cpu_micros_per_frame: u64,
    pub pose_cache_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AnimationContractSet {
    pub format: String,
    pub capability_registry_hash: String,
    pub capability_ids: Vec<String>,
    pub skeletons: Vec<SkeletonContract>,
    pub clips: Vec<AnimationClipContract>,
    pub graphs: Vec<AnimationGraphContract>,
    pub constraints: Vec<IkConstraintContract>,
    pub retargets: Vec<RetargetContract>,
    pub budgets: AnimationBudgetContract,
}

impl AnimationContractSet {
    pub fn validate(&self, registry: &CapabilityRegistry) -> Result<()> {
        if self.format != ANIMATION_CONTRACT_FORMAT
            || self.capability_registry_hash != registry.hash
        {
            return Err(error(
                "animation format or registry hash is stale",
                "Use bhippi-animation-contract@1 and rebuild against the active registry.",
            ));
        }
        for id in &self.capability_ids {
            if registry.describe(id).is_none() {
                return Err(error(
                    &format!("animation contract names unknown capability `{id}`"),
                    "Register it before binding animation.",
                ));
            }
        }
        if self.budgets.maximum_characters == 0
            || self.budgets.cpu_micros_per_frame == 0
            || self.budgets.pose_cache_bytes == 0
        {
            return Err(error(
                "animation budgets are zero/unbounded",
                "Declare non-zero character, CPU and pose-cache limits.",
            ));
        }
        let mut skeletons = BTreeMap::new();
        for skeleton in &self.skeletons {
            skeleton.validate()?;
            if skeletons.insert(skeleton.id.as_str(), skeleton).is_some() {
                return Err(error(
                    "duplicate skeleton id",
                    "Use one stable skeleton id.",
                ));
            }
        }
        let mut clips = BTreeMap::new();
        for clip in &self.clips {
            let Some(skeleton) = skeletons.get(clip.skeleton.as_str()) else {
                return Err(error(
                    "animation clip names an unknown skeleton",
                    "Choose a declared skeleton.",
                ));
            };
            clip.validate(skeleton)?;
            if clips.insert(clip.id.as_str(), clip).is_some() {
                return Err(error(
                    "duplicate animation clip id",
                    "Use one stable clip id.",
                ));
            }
        }
        let mut graph_ids = BTreeSet::new();
        for graph in &self.graphs {
            if !graph_ids.insert(graph.id.as_str()) {
                return Err(error(
                    "duplicate animation graph id",
                    "Use one stable graph id.",
                ));
            }
            graph.validate(&clips, &skeletons)?;
        }
        let bones = self
            .skeletons
            .iter()
            .flat_map(|skeleton| skeleton.bones.iter().map(|bone| bone.id.as_str()))
            .collect::<BTreeSet<_>>();
        for constraint in &self.constraints {
            constraint.validate(&bones)?;
        }
        for retarget in &self.retargets {
            retarget.validate(&skeletons)?;
        }
        Ok(())
    }
}

impl SkeletonContract {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        if self.bones.is_empty() {
            return Err(error(
                "skeleton has no bones",
                "Declare at least one root bone.",
            ));
        }
        let mut ids = BTreeSet::new();
        let mut indices = BTreeSet::new();
        for bone in &self.bones {
            validate_id(&bone.id)?;
            if !ids.insert(bone.id.as_str())
                || !indices.insert(bone.index)
                || !bone.inverse_bind.iter().all(|value| value.is_finite())
            {
                return Err(error(
                    "skeleton has duplicate or invalid bones",
                    "Use unique ids/indices and finite bind matrices.",
                ));
            }
        }
        if self
            .bones
            .iter()
            .filter(|bone| bone.parent.is_none())
            .count()
            != 1
        {
            return Err(error(
                "skeleton must have exactly one root",
                "Join the hierarchy under one root bone.",
            ));
        }
        for bone in &self.bones {
            if bone
                .parent
                .as_ref()
                .is_some_and(|parent| !ids.contains(parent.as_str()))
            {
                return Err(error(
                    "bone parent is missing",
                    "Choose a bone in the same skeleton.",
                ));
            }
            let mut active = BTreeSet::new();
            let mut cursor = Some(bone.id.as_str());
            while let Some(current) = cursor {
                if !active.insert(current) {
                    return Err(error(
                        "bone hierarchy contains a cycle",
                        "Break the parent cycle.",
                    ));
                }
                cursor = self
                    .bones
                    .iter()
                    .find(|candidate| candidate.id == current)
                    .and_then(|candidate| candidate.parent.as_deref());
            }
        }
        Ok(())
    }
}

impl AnimationClipContract {
    pub fn validate(&self, skeleton: &SkeletonContract) -> Result<()> {
        validate_id(&self.id)?;
        if !positive(self.duration_seconds)
            || !non_negative(self.compression.translation_error)
            || !non_negative(self.compression.rotation_error_radians)
            || !non_negative(self.compression.scale_error)
        {
            return Err(error(
                "clip duration/compression is invalid",
                "Use positive duration and non-negative tolerances.",
            ));
        }
        let bones = skeleton
            .bones
            .iter()
            .map(|bone| bone.id.as_str())
            .collect::<BTreeSet<_>>();
        if self
            .root_motion_bone
            .as_ref()
            .is_some_and(|bone| !bones.contains(bone.as_str()))
        {
            return Err(error(
                "root-motion bone is unknown",
                "Choose a bone from the clip skeleton.",
            ));
        }
        let mut tracks = BTreeSet::new();
        for track in &self.tracks {
            if !bones.contains(track.bone.as_str())
                || !tracks.insert(track.bone.as_str())
                || track.keys.is_empty()
            {
                return Err(error(
                    "clip track is duplicate, empty or references an unknown bone",
                    "Use one non-empty track per skeleton bone.",
                ));
            }
            let mut previous = -1.0_f32;
            for key in &track.keys {
                if !finite_key(key)
                    || key.time_seconds <= previous
                    || key.time_seconds > self.duration_seconds
                {
                    return Err(error(
                        "clip keys are invalid or unordered",
                        "Use finite transforms and strictly increasing times within duration.",
                    ));
                }
                previous = key.time_seconds;
            }
        }
        let mut event_ids = BTreeSet::new();
        for event in &self.events {
            validate_id(&event.id)?;
            if !event_ids.insert(event.id.as_str())
                || !event.time_seconds.is_finite()
                || !(0.0..=self.duration_seconds).contains(&event.time_seconds)
            {
                return Err(error(
                    "animation event is duplicate or outside the clip",
                    "Use unique events inside clip duration.",
                ));
            }
        }
        Ok(())
    }
}

impl AnimationGraphContract {
    fn validate(
        &self,
        clips: &BTreeMap<&str, &AnimationClipContract>,
        skeletons: &BTreeMap<&str, &SkeletonContract>,
    ) -> Result<()> {
        validate_id(&self.id)?;
        let mut nodes = BTreeSet::new();
        for node in &self.nodes {
            validate_id(&node.id)?;
            if !nodes.insert(node.id.as_str()) {
                return Err(error("duplicate animation node", "Use one stable node id."));
            }
        }
        let skeleton_bones = skeletons
            .values()
            .flat_map(|skeleton| skeleton.bones.iter().map(|bone| bone.id.as_str()))
            .collect::<BTreeSet<_>>();
        for node in &self.nodes {
            validate_node(&node.kind, &nodes, clips, &self.parameters, &skeleton_bones)?;
        }
        for transition in &self.transitions {
            if !nodes.contains(transition.from.as_str())
                || !nodes.contains(transition.to.as_str())
                || !self.parameters.contains_key(&transition.parameter)
                || !transition.threshold.is_finite()
                || !non_negative(transition.blend_seconds)
            {
                return Err(error(
                    "animation transition is invalid",
                    "Use declared nodes/parameters and finite blend values.",
                ));
            }
        }
        for layer in &self.layers {
            validate_id(&layer.id)?;
            if !nodes.contains(layer.entry_node.as_str())
                || !unit(layer.weight)
                || layer
                    .bone_mask
                    .iter()
                    .any(|bone| !skeleton_bones.contains(bone.as_str()))
            {
                return Err(error(
                    "animation layer has invalid entry/weight/mask",
                    "Use a declared node, 0..1 weight and real bones.",
                ));
            }
        }
        Ok(())
    }
}

impl IkConstraintContract {
    fn validate(&self, bones: &BTreeSet<&str>) -> Result<()> {
        validate_id(&self.id)?;
        if self.chain.is_empty()
            || self.chain.iter().any(|bone| !bones.contains(bone.as_str()))
            || !self.target.iter().all(|value| value.is_finite())
            || self
                .pole_target
                .is_some_and(|target| !target.iter().all(|value| value.is_finite()))
            || !unit(self.weight)
            || self.iterations == 0
            || !positive(self.tolerance)
        {
            return Err(error(
                "IK constraint has invalid chain/target/budget",
                "Use real bones, finite targets, positive iterations/tolerance and 0..1 weight.",
            ));
        }
        if self.solver == IkSolverKind::TwoBone && self.chain.len() != 3 {
            return Err(error(
                "two-bone IK requires a three-joint chain",
                "Declare root, middle and end bones.",
            ));
        }
        Ok(())
    }
}

impl RetargetContract {
    fn validate(&self, skeletons: &BTreeMap<&str, &SkeletonContract>) -> Result<()> {
        let (Some(source), Some(target)) = (
            skeletons.get(self.source_skeleton.as_str()),
            skeletons.get(self.target_skeleton.as_str()),
        ) else {
            return Err(error(
                "retarget skeleton is missing",
                "Choose declared source and target skeletons.",
            ));
        };
        let source_bones = source
            .bones
            .iter()
            .map(|bone| bone.id.as_str())
            .collect::<BTreeSet<_>>();
        let target_bones = target
            .bones
            .iter()
            .map(|bone| bone.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut mapped_targets = BTreeSet::new();
        for (from, to) in &self.bone_map {
            if !source_bones.contains(from.as_str())
                || !target_bones.contains(to.as_str())
                || !mapped_targets.insert(to.as_str())
            {
                return Err(error(
                    "retarget map is dangling or many-to-one",
                    "Map real source bones to unique target bones.",
                ));
            }
        }
        Ok(())
    }
}

fn validate_node(
    kind: &AnimationNodeKind,
    nodes: &BTreeSet<&str>,
    clips: &BTreeMap<&str, &AnimationClipContract>,
    parameters: &BTreeMap<String, f32>,
    bones: &BTreeSet<&str>,
) -> Result<()> {
    let source_ok = |source: &str| nodes.contains(source) || clips.contains_key(source);
    let valid = match kind {
        AnimationNodeKind::Clip { clip } => clips.contains_key(clip.as_str()),
        AnimationNodeKind::State { source } | AnimationNodeKind::PoseCache { source } => {
            source_ok(source)
        }
        AnimationNodeKind::Blend1d { parameter, points } => {
            parameters.contains_key(parameter)
                && points.len() >= 2
                && points.windows(2).all(|pair| pair[0].0 < pair[1].0)
                && points.iter().all(|(_, source)| source_ok(source))
        }
        AnimationNodeKind::Blend2d {
            parameters: names,
            points,
        } => {
            names.iter().all(|name| parameters.contains_key(name))
                && points.len() >= 3
                && points.iter().all(|(point, source)| {
                    point.iter().all(|value| value.is_finite()) && source_ok(source)
                })
        }
        AnimationNodeKind::Additive {
            base,
            additive,
            weight_parameter,
        } => source_ok(base) && source_ok(additive) && parameters.contains_key(weight_parameter),
        AnimationNodeKind::Layer {
            base,
            overlay,
            bone_mask,
        } => {
            source_ok(base)
                && source_ok(overlay)
                && bone_mask.iter().all(|bone| bones.contains(bone.as_str()))
        }
        AnimationNodeKind::Montage { clips: sequence } => {
            !sequence.is_empty()
                && sequence
                    .iter()
                    .all(|clip| clips.contains_key(clip.as_str()))
        }
    };
    valid.then_some(()).ok_or_else(|| {
        error(
            "animation node has a dangling/invalid contract",
            "Use declared clips, nodes, parameters and bones.",
        )
    })
}

fn finite_key(key: &TransformKey) -> bool {
    key.time_seconds.is_finite()
        && key.translation.iter().all(|value| value.is_finite())
        && key.rotation.iter().all(|value| value.is_finite())
        && key
            .scale
            .iter()
            .all(|value| value.is_finite() && *value > 0.0)
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
            &format!("`{id}` is not a canonical animation id"),
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
