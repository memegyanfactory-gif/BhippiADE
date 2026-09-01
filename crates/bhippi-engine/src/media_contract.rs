//! Typed VFX and audio domain contracts (Phase 19).
//!
//! The contracts are deterministic data and validation only. No GPU, audio device, importer,
//! runtime mixer, streaming service or editor is implemented here.

use crate::error::{EngineError, Result};
use crate::registry::CapabilityRegistry;
use crate::runtime_contract::{RuntimeEntityHandle, RuntimeResourceHandle};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const MEDIA_CONTRACT_FORMAT: &str = "bhippi-media-contract@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum VfxExecutionClass {
    Cpu,
    Gpu,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CurvePoint {
    pub time: f32,
    pub value: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum VfxNodeKind {
    Emitter {
        rate: f32,
        burst: u32,
    },
    InitialVelocity {
        minimum: [f32; 3],
        maximum: [f32; 3],
    },
    Gravity {
        acceleration: [f32; 3],
    },
    SizeCurve {
        points: Vec<CurvePoint>,
    },
    ColorCurve {
        red: Vec<CurvePoint>,
        green: Vec<CurvePoint>,
        blue: Vec<CurvePoint>,
        alpha: Vec<CurvePoint>,
    },
    Collision {
        layer: String,
        restitution: f32,
    },
    Ribbon {
        width: f32,
    },
    Beam {
        width: f32,
    },
    Decal {
        material: RuntimeResourceHandle,
    },
    Light {
        intensity: f32,
        range: f32,
    },
    Event {
        event: String,
    },
    SubEmitter {
        graph: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct VfxNodeContract {
    pub id: String,
    pub execution: VfxExecutionClass,
    pub kind: VfxNodeKind,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct VfxEdgeContract {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct VfxLodContract {
    pub distance: f32,
    pub spawn_scale: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct VfxBudgetContract {
    pub maximum_live_particles: u32,
    pub maximum_emitters: u32,
    pub pool_bytes: u64,
    pub cpu_micros_per_frame: u64,
    pub gpu_micros_per_frame: u64,
    pub maximum_overdraw: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct VfxGraphContract {
    pub id: String,
    pub nodes: Vec<VfxNodeContract>,
    pub edges: Vec<VfxEdgeContract>,
    pub lod: Vec<VfxLodContract>,
    pub budgets: VfxBudgetContract,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AudioDeviceState {
    Unavailable,
    Opening,
    Ready,
    Suspended,
    Lost,
    Closed,
}

impl AudioDeviceState {
    #[must_use]
    pub const fn allows(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Unavailable, Self::Opening | Self::Closed)
                | (Self::Opening, Self::Ready | Self::Lost | Self::Closed)
                | (Self::Ready, Self::Suspended | Self::Lost | Self::Closed)
                | (Self::Suspended, Self::Ready | Self::Lost | Self::Closed)
                | (Self::Lost, Self::Opening | Self::Closed)
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AudioClipContract {
    pub id: String,
    pub resource: RuntimeResourceHandle,
    pub duration_seconds: f32,
    pub channels: u8,
    pub sample_rate: u32,
    pub streaming: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AttenuationContract {
    pub minimum_distance: f32,
    pub maximum_distance: f32,
    pub rolloff: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SpatialAudioContract {
    pub enabled: bool,
    pub attenuation: AttenuationContract,
    pub occlusion: f32,
    pub reverb_send: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ReverbZoneContract {
    pub id: String,
    pub center: [f32; 3],
    pub half_extents: [f32; 3],
    pub wet: f32,
    pub decay_seconds: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AudioEffectKind {
    Gain,
    LowPass,
    HighPass,
    Compressor,
    Reverb,
    Delay,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AudioEffectContract {
    pub kind: AudioEffectKind,
    pub enabled: bool,
    pub amount: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MixerBusContract {
    pub id: String,
    pub parent: Option<String>,
    pub gain: f32,
    pub effects: Vec<AudioEffectContract>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum AudioEventAction {
    Play {
        clip: String,
        bus: String,
        looped: bool,
    },
    Stop {
        event: String,
    },
    SetBusGain {
        bus: String,
        gain: f32,
    },
    SetParameter {
        name: String,
        value: f32,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AudioEventContract {
    pub id: String,
    pub priority: u8,
    pub spatial: SpatialAudioContract,
    pub actions: Vec<AudioEventAction>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct AudioBudgetContract {
    pub maximum_voices: u32,
    pub streaming_bytes: u64,
    pub resident_bytes: u64,
    pub cpu_micros_per_frame: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AudioContract {
    pub clips: Vec<AudioClipContract>,
    pub buses: Vec<MixerBusContract>,
    pub events: Vec<AudioEventContract>,
    pub zones: Vec<ReverbZoneContract>,
    pub listener: Option<RuntimeEntityHandle>,
    pub budgets: AudioBudgetContract,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MediaContractSet {
    pub format: String,
    pub capability_registry_hash: String,
    pub capability_ids: Vec<String>,
    pub vfx: Vec<VfxGraphContract>,
    pub audio: AudioContract,
}

impl MediaContractSet {
    pub fn validate(&self, registry: &CapabilityRegistry) -> Result<()> {
        if self.format != MEDIA_CONTRACT_FORMAT || self.capability_registry_hash != registry.hash {
            return Err(error(
                "media format or registry hash is stale",
                "Use bhippi-media-contract@1 and rebuild against the active registry.",
            ));
        }
        for capability in &self.capability_ids {
            if registry.describe(capability).is_none() {
                return Err(error(
                    &format!("media contract names unknown capability `{capability}`"),
                    "Register it before binding media.",
                ));
            }
        }
        let graph_ids = self
            .vfx
            .iter()
            .map(|graph| graph.id.as_str())
            .collect::<BTreeSet<_>>();
        if graph_ids.len() != self.vfx.len() {
            return Err(error("duplicate VFX graph id", "Use one stable graph id."));
        }
        for graph in &self.vfx {
            graph.validate(&graph_ids)?;
        }
        self.audio.validate()
    }
}

impl VfxGraphContract {
    fn validate(&self, graphs: &BTreeSet<&str>) -> Result<()> {
        validate_id(&self.id)?;
        let budget = &self.budgets;
        if budget.maximum_live_particles == 0
            || budget.maximum_emitters == 0
            || budget.pool_bytes == 0
            || budget.cpu_micros_per_frame == 0
            || budget.gpu_micros_per_frame == 0
            || !positive(budget.maximum_overdraw)
        {
            return Err(error(
                "VFX budgets are zero/unbounded",
                "Declare non-zero particle, pool, CPU/GPU and overdraw budgets.",
            ));
        }
        let mut nodes = BTreeSet::new();
        for node in &self.nodes {
            validate_id(&node.id)?;
            if !nodes.insert(node.id.as_str()) {
                return Err(error("duplicate VFX node", "Use one stable node id."));
            }
            validate_vfx_node(&node.kind, graphs, &self.id)?;
        }
        let mut edges = BTreeSet::new();
        for edge in &self.edges {
            if !nodes.contains(edge.from.as_str())
                || !nodes.contains(edge.to.as_str())
                || !edges.insert((edge.from.as_str(), edge.to.as_str()))
            {
                return Err(error(
                    "VFX edge is duplicate or dangling",
                    "Connect declared nodes once.",
                ));
            }
        }
        reject_vfx_cycles(&nodes, &self.edges)?;
        let mut previous = -1.0_f32;
        for lod in &self.lod {
            if !non_negative(lod.distance) || lod.distance <= previous || !unit(lod.spawn_scale) {
                return Err(error(
                    "VFX LOD is invalid or unordered",
                    "Use increasing distances and spawn scale in 0..1.",
                ));
            }
            previous = lod.distance;
        }
        Ok(())
    }
}

impl AudioContract {
    fn validate(&self) -> Result<()> {
        if self.budgets.maximum_voices == 0
            || self.budgets.streaming_bytes == 0
            || self.budgets.resident_bytes == 0
            || self.budgets.cpu_micros_per_frame == 0
        {
            return Err(error(
                "audio budgets are zero/unbounded",
                "Declare non-zero voice, stream, memory and CPU budgets.",
            ));
        }
        let mut clips = BTreeSet::new();
        for clip in &self.clips {
            validate_id(&clip.id)?;
            if !clips.insert(clip.id.as_str())
                || !positive(clip.duration_seconds)
                || clip.channels == 0
                || clip.sample_rate == 0
            {
                return Err(error(
                    "audio clip is duplicate or invalid",
                    "Use unique ids, positive duration/channels/sample rate.",
                ));
            }
        }
        let mut buses = BTreeSet::new();
        for bus in &self.buses {
            validate_id(&bus.id)?;
            if !buses.insert(bus.id.as_str())
                || !non_negative(bus.gain)
                || bus.effects.iter().any(|effect| !unit(effect.amount))
            {
                return Err(error(
                    "mixer bus is duplicate or invalid",
                    "Use unique ids, non-negative gain and effect amount in 0..1.",
                ));
            }
        }
        if self.buses.iter().filter(|bus| bus.parent.is_none()).count() != 1 {
            return Err(error(
                "mixer must have exactly one root bus",
                "Route every bus under one master.",
            ));
        }
        validate_bus_tree(&self.buses, &buses)?;
        let mut events = BTreeSet::new();
        for event in &self.events {
            validate_id(&event.id)?;
            if !events.insert(event.id.as_str()) || event.actions.is_empty() {
                return Err(error(
                    "audio event is duplicate or empty",
                    "Use a unique id and at least one action.",
                ));
            }
            validate_spatial(&event.spatial)?;
            for action in &event.actions {
                match action {
                    AudioEventAction::Play { clip, bus, .. }
                        if !clips.contains(clip.as_str()) || !buses.contains(bus.as_str()) =>
                    {
                        return Err(error(
                            "audio play action is dangling",
                            "Choose a declared clip and bus.",
                        ));
                    }
                    AudioEventAction::SetBusGain { bus, gain }
                        if !buses.contains(bus.as_str()) || !non_negative(*gain) =>
                    {
                        return Err(error(
                            "audio bus action is invalid",
                            "Choose a declared bus and non-negative gain.",
                        ));
                    }
                    AudioEventAction::SetParameter { name, value }
                        if name.trim().is_empty() || !value.is_finite() =>
                    {
                        return Err(error(
                            "audio parameter action is invalid",
                            "Use a named finite parameter.",
                        ));
                    }
                    _ => {}
                }
            }
        }
        for zone in &self.zones {
            validate_id(&zone.id)?;
            if !zone.center.iter().all(|value| value.is_finite())
                || !zone.half_extents.iter().all(|value| positive(*value))
                || !unit(zone.wet)
                || !positive(zone.decay_seconds)
            {
                return Err(error(
                    "reverb zone is invalid",
                    "Use finite center, positive bounds/decay and wet in 0..1.",
                ));
            }
        }
        Ok(())
    }
}

fn validate_vfx_node(kind: &VfxNodeKind, graphs: &BTreeSet<&str>, own_graph: &str) -> Result<()> {
    let valid = match kind {
        VfxNodeKind::Emitter { rate, .. } => non_negative(*rate),
        VfxNodeKind::InitialVelocity { minimum, maximum } => {
            minimum.iter().chain(maximum).all(|value| value.is_finite())
        }
        VfxNodeKind::Gravity { acceleration } => acceleration.iter().all(|value| value.is_finite()),
        VfxNodeKind::SizeCurve { points } => valid_curve(points),
        VfxNodeKind::ColorCurve {
            red,
            green,
            blue,
            alpha,
        } => [red, green, blue, alpha]
            .into_iter()
            .all(|curve| valid_curve(curve)),
        VfxNodeKind::Collision { layer, restitution } => {
            !layer.trim().is_empty() && unit(*restitution)
        }
        VfxNodeKind::Ribbon { width } | VfxNodeKind::Beam { width } => positive(*width),
        VfxNodeKind::Decal { .. } => true,
        VfxNodeKind::Light { intensity, range } => non_negative(*intensity) && positive(*range),
        VfxNodeKind::Event { event } => !event.trim().is_empty(),
        VfxNodeKind::SubEmitter { graph } => graph != own_graph && graphs.contains(graph.as_str()),
    };
    valid.then_some(()).ok_or_else(|| {
        error(
            "VFX node parameters are invalid/dangling",
            "Use finite ranges, valid curves and a different declared sub-emitter graph.",
        )
    })
}

fn valid_curve(points: &[CurvePoint]) -> bool {
    points.len() >= 2
        && points
            .iter()
            .all(|point| point.time.is_finite() && point.value.is_finite())
        && points.windows(2).all(|pair| pair[0].time < pair[1].time)
}

fn reject_vfx_cycles(nodes: &BTreeSet<&str>, edges: &[VfxEdgeContract]) -> Result<()> {
    let mut incoming = nodes
        .iter()
        .map(|id| (*id, 0_usize))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in edges {
        *incoming.entry(edge.to.as_str()).or_default() += 1;
        outgoing
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    let mut ready = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect::<Vec<_>>();
    let mut visited = 0;
    while let Some(id) = ready.pop() {
        visited += 1;
        if let Some(next) = outgoing.get(id) {
            for target in next {
                if let Some(count) = incoming.get_mut(target) {
                    *count -= 1;
                    if *count == 0 {
                        ready.push(target);
                    }
                }
            }
        }
    }
    (visited == nodes.len())
        .then_some(())
        .ok_or_else(|| error("VFX graph contains a cycle", "Break the execution cycle."))
}

fn validate_bus_tree(buses: &[MixerBusContract], ids: &BTreeSet<&str>) -> Result<()> {
    for bus in buses {
        if bus
            .parent
            .as_ref()
            .is_some_and(|parent| !ids.contains(parent.as_str()))
        {
            return Err(error(
                "mixer bus parent is missing",
                "Route to a declared bus.",
            ));
        }
        let mut active = BTreeSet::new();
        let mut cursor = Some(bus.id.as_str());
        while let Some(current) = cursor {
            if !active.insert(current) {
                return Err(error(
                    "mixer bus contains a cycle",
                    "Break the parent cycle.",
                ));
            }
            cursor = buses
                .iter()
                .find(|candidate| candidate.id == current)
                .and_then(|candidate| candidate.parent.as_deref());
        }
    }
    Ok(())
}

fn validate_spatial(spatial: &SpatialAudioContract) -> Result<()> {
    let attenuation = &spatial.attenuation;
    if !non_negative(attenuation.minimum_distance)
        || !positive(attenuation.maximum_distance)
        || attenuation.minimum_distance >= attenuation.maximum_distance
        || !positive(attenuation.rolloff)
        || !unit(spatial.occlusion)
        || !unit(spatial.reverb_send)
    {
        return Err(error(
            "spatial audio parameters are invalid",
            "Use ordered attenuation distances, positive rolloff and 0..1 sends.",
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
            &format!("`{id}` is not a canonical media id"),
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
