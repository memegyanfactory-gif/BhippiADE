//! Versioned, deterministic terrain and procedural-world domain contracts (ADR-0039).
//!
//! This module describes authored truth and bounded work plans only. It deliberately contains no
//! renderer, terrain editor, water simulation, asynchronous I/O, origin shifting or HLOD backend.

use crate::error::{EngineError, Result};
use crate::procedural::{scatter, Bounds};
use bhippi_types::AssetId;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const TERRAIN_FORMAT: &str = "bhippi-terrain@1";
pub const BIOME_FORMAT: &str = "bhippi-biome@1";
pub const PROCEDURAL_GRAPH_FORMAT: &str = "bhippi-procedural-graph@1";
pub const PROCEDURAL_PROGRAM_FORMAT: &str = "bhippi-procedural-program@1";
pub const PROCEDURAL_BAKE_FORMAT: &str = "bhippi-procedural-bake-plan@1";
pub const WORLD_PARTITION_FORMAT: &str = "bhippi-world-partition@1";
pub const TERRAIN_BAKE_FORMAT: &str = "bhippi-terrain-bake-plan@1";

const MAX_TERRAIN_CHUNKS: u32 = 16_384;
const MAX_TERRAIN_LAYERS: usize = 32;
const MAX_SPLINES: usize = 1_024;
const MAX_SPLINE_POINTS: usize = 16_384;
const MAX_GRAPH_NODES: usize = 4_096;
const MAX_BIOME_RULES: usize = 256;
const MAX_SCATTER_PER_CELL: u32 = 4_096;
const MAX_STREAMING_CELLS: usize = 65_536;
const MAX_STREAMING_QUEUE: u16 = 4_096;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
pub struct TerrainChunkCoord {
    pub x: i32,
    pub z: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TerrainNormalMethod {
    HeightGradient,
    FaceWeighted,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TerrainNoiseRule {
    pub frequency: f32,
    pub amplitude_m: f32,
    pub octaves: u8,
    pub lacunarity: f32,
    pub persistence: f32,
    pub seed_offset: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct LandscapeLayer {
    pub name: String,
    pub material: String,
    #[serde(default)]
    pub mask: Option<String>,
    pub weight: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TerrainSplineKind {
    Road,
    RiverPath,
}

/// An authored path only. `RiverPath` does not imply water rendering or simulation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TerrainSpline {
    pub name: String,
    pub kind: TerrainSplineKind,
    pub points: Vec<[f32; 3]>,
    pub width_m: f32,
    #[serde(default)]
    pub material: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TerrainHeightOverride {
    pub sample_x: u32,
    pub sample_z: u32,
    pub height_m: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TerrainDocument {
    pub format: String,
    pub id: AssetId,
    pub name: String,
    pub seed: u64,
    /// Samples per side, including the shared edge. Must be `2^n + 1`.
    pub chunk_samples: u16,
    pub chunks_x: u16,
    pub chunks_z: u16,
    pub sample_spacing_m: f32,
    pub height_scale_m: f32,
    pub lod_levels: u8,
    pub collision_requested: bool,
    pub normal_method: TerrainNormalMethod,
    #[serde(default)]
    pub noise: Vec<TerrainNoiseRule>,
    #[serde(default)]
    pub layers: Vec<LandscapeLayer>,
    #[serde(default)]
    pub splines: Vec<TerrainSpline>,
    /// Sparse authored values applied after deterministic generation.
    #[serde(default)]
    pub manual_overrides: Vec<TerrainHeightOverride>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TerrainChunkBakeContract {
    pub coord: TerrainChunkCoord,
    pub output_path: String,
    pub contract_hash: String,
    pub lod_levels: u8,
    pub collision_requested: bool,
    pub normal_method: TerrainNormalMethod,
}

/// A deterministic request for a backend, not evidence that height/collision/normal data exists.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TerrainBakePlan {
    pub format: String,
    pub terrain_id: AssetId,
    pub source_hash: String,
    pub algorithm: String,
    pub seed: u64,
    pub manual_overrides_hash: String,
    pub chunks: Vec<TerrainChunkBakeContract>,
}

impl TerrainDocument {
    pub fn parse(text: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(text).map_err(|error| {
            world_error(
                format!("invalid terrain document: {error}"),
                format!("Terrain documents use {TERRAIN_FORMAT} JSON."),
            )
        })?;
        document.validate()?;
        Ok(document)
    }

    pub fn dump(&self) -> Result<String> {
        canonical_json(self, "terrain")
    }

    pub fn validate(&self) -> Result<()> {
        require_format(&self.format, TERRAIN_FORMAT, "terrain")?;
        require_name(&self.name, "terrain")?;
        let intervals = self.chunk_samples.saturating_sub(1);
        if self.chunk_samples < 3 || !intervals.is_power_of_two() {
            return Err(world_error(
                format!("chunk_samples {} is not 2^n + 1", self.chunk_samples),
                "Use 65, 129, 257 or another 2^n + 1 sample count.",
            ));
        }
        let chunk_count = u32::from(self.chunks_x).saturating_mul(u32::from(self.chunks_z));
        if chunk_count == 0 || chunk_count > MAX_TERRAIN_CHUNKS {
            return Err(world_error(
                format!("terrain requests {chunk_count} chunks"),
                format!("Use 1..={MAX_TERRAIN_CHUNKS} chunks."),
            ));
        }
        positive("sample_spacing_m", self.sample_spacing_m)?;
        positive("height_scale_m", self.height_scale_m)?;
        if self.lod_levels == 0 || self.lod_levels > 12 {
            return Err(world_error(
                format!("lod_levels {} is outside 1..=12", self.lod_levels),
                "Choose a bounded LOD count.",
            ));
        }
        if self.noise.len() > 16 {
            return Err(world_error(
                "terrain has more than 16 noise rules",
                "Bake or combine noise rules before adding more.",
            ));
        }
        for rule in &self.noise {
            positive("noise frequency", rule.frequency)?;
            non_negative("noise amplitude", rule.amplitude_m)?;
            if !(1..=12).contains(&rule.octaves) {
                return Err(world_error(
                    format!("noise octaves {} is outside 1..=12", rule.octaves),
                    "Use a bounded octave count.",
                ));
            }
            if !(1.0..=8.0).contains(&rule.lacunarity) || !rule.lacunarity.is_finite() {
                return Err(world_error(
                    "noise lacunarity must be finite in 1..=8",
                    "Use a value such as 2.0.",
                ));
            }
            unit("noise persistence", rule.persistence)?;
        }
        if self.layers.len() > MAX_TERRAIN_LAYERS {
            return Err(world_error(
                format!("terrain has more than {MAX_TERRAIN_LAYERS} layers"),
                "Combine landscape layers before adding more.",
            ));
        }
        let mut layer_names = BTreeSet::new();
        for layer in &self.layers {
            require_name(&layer.name, "landscape layer")?;
            if !layer_names.insert(layer.name.as_str()) {
                return Err(world_error(
                    format!("duplicate landscape layer {:?}", layer.name),
                    "Give every layer a unique name.",
                ));
            }
            require_suffix(&layer.material, ".mat.json", "landscape material")?;
            if let Some(mask) = layer.mask.as_deref() {
                require_asset_reference(mask, "landscape mask")?;
            }
            unit("landscape layer weight", layer.weight)?;
        }
        if self.splines.len() > MAX_SPLINES {
            return Err(world_error(
                format!("terrain has more than {MAX_SPLINES} splines"),
                "Split paths across terrain documents.",
            ));
        }
        let mut spline_names = BTreeSet::new();
        for spline in &self.splines {
            require_name(&spline.name, "terrain spline")?;
            if !spline_names.insert(spline.name.as_str()) {
                return Err(world_error(
                    format!("duplicate terrain spline {:?}", spline.name),
                    "Give every spline a unique name.",
                ));
            }
            if !(2..=MAX_SPLINE_POINTS).contains(&spline.points.len())
                || spline.points.iter().any(|point| !finite3(*point))
            {
                return Err(world_error(
                    format!("terrain spline {:?} has invalid points", spline.name),
                    format!("Use 2..={MAX_SPLINE_POINTS} finite control points."),
                ));
            }
            positive("terrain spline width", spline.width_m)?;
            if let Some(material) = spline.material.as_deref() {
                require_suffix(material, ".mat.json", "spline material")?;
            }
        }
        let max_x = u32::from(self.chunks_x)
            .saturating_mul(u32::from(intervals))
            .saturating_add(1);
        let max_z = u32::from(self.chunks_z)
            .saturating_mul(u32::from(intervals))
            .saturating_add(1);
        let mut override_coords = BTreeSet::new();
        for authored in &self.manual_overrides {
            if authored.sample_x >= max_x
                || authored.sample_z >= max_z
                || !authored.height_m.is_finite()
                || !override_coords.insert((authored.sample_x, authored.sample_z))
            {
                return Err(world_error(
                    "terrain manual override is duplicate, non-finite or outside the heightfield",
                    "Keep one finite override per valid sample coordinate.",
                ));
            }
        }
        Ok(())
    }

    pub fn bake_plan(&self) -> Result<TerrainBakePlan> {
        self.validate()?;
        let source = self.dump()?;
        let source_hash = blake3::hash(source.as_bytes()).to_hex().to_string();
        let mut overrides = self.manual_overrides.clone();
        overrides.sort_by_key(|authored| (authored.sample_x, authored.sample_z));
        let overrides = canonical_json(&overrides, "terrain overrides")?;
        let manual_overrides_hash = blake3::hash(overrides.as_bytes()).to_hex().to_string();
        let mut chunks =
            Vec::with_capacity(usize::from(self.chunks_x) * usize::from(self.chunks_z));
        for z in 0..self.chunks_z {
            for x in 0..self.chunks_x {
                let coord = TerrainChunkCoord {
                    x: i32::from(x),
                    z: i32::from(z),
                };
                let mut hasher = blake3::Hasher::new();
                hasher.update(b"bhippi-terrain-chunk-contract@1");
                hasher.update(source_hash.as_bytes());
                hasher.update(&coord.x.to_le_bytes());
                hasher.update(&coord.z.to_le_bytes());
                chunks.push(TerrainChunkBakeContract {
                    coord,
                    output_path: format!(
                        "assets/generated/terrain/{}/{}_{}.height.bin",
                        self.id, coord.x, coord.z
                    ),
                    contract_hash: hasher.finalize().to_hex().to_string(),
                    lod_levels: self.lod_levels,
                    collision_requested: self.collision_requested,
                    normal_method: self.normal_method,
                });
            }
        }
        Ok(TerrainBakePlan {
            format: TERRAIN_BAKE_FORMAT.to_owned(),
            terrain_id: self.id,
            source_hash,
            algorithm: "heightfield-contract-v1".to_owned(),
            seed: self.seed,
            manual_overrides_hash,
            chunks,
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct BiomeScatterRule {
    pub name: String,
    pub asset: String,
    pub density_per_square_km: f32,
    pub altitude_m: [f32; 2],
    pub slope_degrees: [f32; 2],
    pub min_distance_m: f32,
    pub max_per_cell: u32,
    pub seed_offset: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct BiomeDocument {
    pub format: String,
    pub id: AssetId,
    pub name: String,
    pub seed: u64,
    pub terrain: String,
    #[serde(default)]
    pub rules: Vec<BiomeScatterRule>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct BiomeScatterCandidates {
    pub rule: String,
    pub asset: String,
    pub cell: TerrainChunkCoord,
    pub seed: u64,
    /// Y remains zero until a terrain backend projects the candidates onto a proven surface.
    pub candidates: Vec<[f32; 3]>,
    pub requires_surface_projection: bool,
}

impl BiomeDocument {
    pub fn parse(text: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(text).map_err(|error| {
            world_error(
                format!("invalid biome document: {error}"),
                format!("Biome documents use {BIOME_FORMAT} JSON."),
            )
        })?;
        document.validate()?;
        Ok(document)
    }

    pub fn dump(&self) -> Result<String> {
        canonical_json(self, "biome")
    }

    pub fn validate(&self) -> Result<()> {
        require_format(&self.format, BIOME_FORMAT, "biome")?;
        require_name(&self.name, "biome")?;
        require_suffix(&self.terrain, ".terrain.json", "biome terrain")?;
        if self.rules.len() > MAX_BIOME_RULES {
            return Err(world_error(
                format!("biome has more than {MAX_BIOME_RULES} scatter rules"),
                "Split the biome into smaller documents.",
            ));
        }
        let mut names = BTreeSet::new();
        for rule in &self.rules {
            require_name(&rule.name, "biome rule")?;
            require_asset_reference(&rule.asset, "biome scatter asset")?;
            if !names.insert(rule.name.as_str()) {
                return Err(world_error(
                    format!("duplicate biome rule {:?}", rule.name),
                    "Give every biome rule a unique name.",
                ));
            }
            non_negative("biome density", rule.density_per_square_km)?;
            ordered_range("altitude_m", rule.altitude_m, None)?;
            ordered_range("slope_degrees", rule.slope_degrees, Some([0.0, 90.0]))?;
            non_negative("biome minimum distance", rule.min_distance_m)?;
            if rule.max_per_cell == 0 || rule.max_per_cell > MAX_SCATTER_PER_CELL {
                return Err(world_error(
                    format!("biome max_per_cell {} is outside bounds", rule.max_per_cell),
                    format!("Use 1..={MAX_SCATTER_PER_CELL}."),
                ));
            }
        }
        Ok(())
    }

    pub fn plan_cell_scatter(
        &self,
        terrain: &TerrainDocument,
        cell: TerrainChunkCoord,
    ) -> Result<Vec<BiomeScatterCandidates>> {
        self.validate()?;
        terrain.validate()?;
        if cell.x < 0
            || cell.z < 0
            || cell.x >= i32::from(terrain.chunks_x)
            || cell.z >= i32::from(terrain.chunks_z)
        {
            return Err(world_error(
                format!("terrain cell {},{} does not exist", cell.x, cell.z),
                "Choose a coordinate inside the terrain chunk grid.",
            ));
        }
        let extent = f32::from(terrain.chunk_samples - 1) * terrain.sample_spacing_m;
        let min_x = cell.x as f32 * extent;
        let min_z = cell.z as f32 * extent;
        let bounds = Bounds::new([min_x, 0.0, min_z], [min_x + extent, 0.0, min_z + extent])?;
        let area_square_km = extent * extent / 1_000_000.0;
        let mut plans = Vec::new();
        for rule in &self.rules {
            let requested = (rule.density_per_square_km * area_square_km).round() as u32;
            let count = requested.min(rule.max_per_cell).min(MAX_SCATTER_PER_CELL);
            let seed = mix_seed(self.seed ^ terrain.seed, rule.seed_offset, cell);
            let candidates = if count == 0 {
                Vec::new()
            } else {
                scatter(bounds, count, rule.min_distance_m, seed)?
            };
            plans.push(BiomeScatterCandidates {
                rule: rule.name.clone(),
                asset: rule.asset.clone(),
                cell,
                seed,
                candidates,
                requires_surface_projection: true,
            });
        }
        Ok(plans)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProceduralValueType {
    ScalarField,
    Curve,
    Points,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProceduralNodeKind {
    NoiseField {
        frequency: f32,
        octaves: u8,
        seed_offset: u64,
    },
    Spline {
        points: Vec<[f32; 3]>,
    },
    Grid {
        origin: [f32; 3],
        columns: u32,
        rows: u32,
        spacing: [f32; 2],
    },
    Scatter {
        min: [f32; 3],
        max: [f32; 3],
        count: u32,
        min_distance: f32,
        seed_offset: u64,
    },
    AlongSpline {
        spline: String,
        spacing_m: f32,
    },
    ProjectToField {
        points: String,
        field: String,
    },
    MergePoints {
        inputs: Vec<String>,
    },
}

impl ProceduralNodeKind {
    fn output_type(&self) -> ProceduralValueType {
        match self {
            Self::NoiseField { .. } => ProceduralValueType::ScalarField,
            Self::Spline { .. } => ProceduralValueType::Curve,
            Self::Grid { .. }
            | Self::Scatter { .. }
            | Self::AlongSpline { .. }
            | Self::ProjectToField { .. }
            | Self::MergePoints { .. } => ProceduralValueType::Points,
        }
    }

    fn inputs(&self) -> Vec<(&'static str, &str, ProceduralValueType)> {
        match self {
            Self::AlongSpline { spline, .. } => {
                vec![("spline", spline.as_str(), ProceduralValueType::Curve)]
            }
            Self::ProjectToField { points, field } => vec![
                ("points", points.as_str(), ProceduralValueType::Points),
                ("field", field.as_str(), ProceduralValueType::ScalarField),
            ],
            Self::MergePoints { inputs } => inputs
                .iter()
                .map(|input| ("input", input.as_str(), ProceduralValueType::Points))
                .collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProceduralNode {
    pub id: String,
    #[serde(flatten)]
    pub node: ProceduralNodeKind,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProceduralGraphDocument {
    pub format: String,
    pub id: AssetId,
    pub name: String,
    pub seed: u64,
    pub output: String,
    #[serde(default)]
    pub nodes: Vec<ProceduralNode>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProceduralProgram {
    pub format: String,
    pub graph_id: AssetId,
    pub source_hash: String,
    pub seed: u64,
    pub output: String,
    pub output_type: ProceduralValueType,
    pub operations: Vec<ProceduralNode>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProceduralArtifactContract {
    pub path: String,
    pub contract_hash: String,
}

/// Provenance-bound output requests. Hashes bind the graph and requested path, not output bytes.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProceduralBakePlan {
    pub format: String,
    pub graph_id: AssetId,
    pub source_hash: String,
    pub algorithm: String,
    pub seed: u64,
    pub artifacts: Vec<ProceduralArtifactContract>,
}

impl ProceduralGraphDocument {
    pub fn parse(text: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(text).map_err(|error| {
            world_error(
                format!("invalid procedural graph: {error}"),
                format!("Procedural graphs use {PROCEDURAL_GRAPH_FORMAT} JSON."),
            )
        })?;
        document.validate()?;
        Ok(document)
    }

    pub fn dump(&self) -> Result<String> {
        canonical_json(self, "procedural graph")
    }

    pub fn validate(&self) -> Result<()> {
        self.compile().map(|_| ())
    }

    pub fn compile(&self) -> Result<ProceduralProgram> {
        require_format(&self.format, PROCEDURAL_GRAPH_FORMAT, "procedural graph")?;
        require_name(&self.name, "procedural graph")?;
        if self.nodes.is_empty() || self.nodes.len() > MAX_GRAPH_NODES {
            return Err(world_error(
                format!(
                    "procedural graph node count {} is outside bounds",
                    self.nodes.len()
                ),
                format!("Use 1..={MAX_GRAPH_NODES} nodes."),
            ));
        }
        let mut nodes = BTreeMap::new();
        for node in &self.nodes {
            require_name(&node.id, "procedural node")?;
            if nodes.insert(node.id.as_str(), node).is_some() {
                return Err(world_error(
                    format!("duplicate procedural node id {:?}", node.id),
                    "Give every node a unique id.",
                ));
            }
            validate_procedural_node(node)?;
        }
        let output = nodes.get(self.output.as_str()).ok_or_else(|| {
            world_error(
                format!("procedural graph output {:?} does not exist", self.output),
                "Point output at an existing node.",
            )
        })?;
        let mut states = BTreeMap::new();
        let mut order = Vec::new();
        visit_procedural(self.output.as_str(), &nodes, &mut states, &mut order)?;
        if order.len() != nodes.len() {
            let reachable = order.iter().copied().collect::<BTreeSet<_>>();
            let unused = nodes
                .keys()
                .filter(|id| !reachable.contains(**id))
                .copied()
                .collect::<Vec<_>>();
            return Err(world_error(
                format!(
                    "procedural graph has unreachable nodes: {}",
                    unused.join(", ")
                ),
                "Connect or remove every node before compiling.",
            ));
        }
        let source = self.dump()?;
        Ok(ProceduralProgram {
            format: PROCEDURAL_PROGRAM_FORMAT.to_owned(),
            graph_id: self.id,
            source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            seed: self.seed,
            output: self.output.clone(),
            output_type: output.node.output_type(),
            operations: order
                .into_iter()
                .filter_map(|id| nodes.get(id).map(|node| (*node).clone()))
                .collect(),
        })
    }

    pub fn bake_plan(&self, output_paths: &[String]) -> Result<ProceduralBakePlan> {
        let program = self.compile()?;
        if output_paths.is_empty() || output_paths.len() > 256 {
            return Err(world_error(
                format!(
                    "procedural bake output count {} is outside bounds",
                    output_paths.len()
                ),
                "Request between 1 and 256 generated artifacts.",
            ));
        }
        let mut unique = BTreeSet::new();
        let mut artifacts = Vec::with_capacity(output_paths.len());
        for path in output_paths {
            let normalized = path.replace('\\', "/");
            if !normalized.starts_with("assets/generated/")
                || normalized.contains("../")
                || normalized.ends_with('/')
                || !unique.insert(normalized.clone())
            {
                return Err(world_error(
                    format!("procedural bake output path {path:?} is unsafe or duplicated"),
                    "Use unique file paths under assets/generated/.",
                ));
            }
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"bhippi-procedural-artifact-contract@1");
            hasher.update(program.source_hash.as_bytes());
            hasher.update(normalized.as_bytes());
            artifacts.push(ProceduralArtifactContract {
                path: normalized,
                contract_hash: hasher.finalize().to_hex().to_string(),
            });
        }
        Ok(ProceduralBakePlan {
            format: PROCEDURAL_BAKE_FORMAT.to_owned(),
            graph_id: self.id,
            source_hash: program.source_hash,
            algorithm: "procedural-graph-contract-v1".to_owned(),
            seed: self.seed,
            artifacts,
        })
    }
}

fn validate_procedural_node(node: &ProceduralNode) -> Result<()> {
    match &node.node {
        ProceduralNodeKind::NoiseField {
            frequency, octaves, ..
        } => {
            positive("procedural noise frequency", *frequency)?;
            if !(1..=12).contains(octaves) {
                return Err(world_error(
                    format!("node {:?} has invalid octave count", node.id),
                    "Use 1..=12 octaves.",
                ));
            }
        }
        ProceduralNodeKind::Spline { points } => {
            if !(2..=MAX_SPLINE_POINTS).contains(&points.len())
                || points.iter().any(|point| !finite3(*point))
            {
                return Err(world_error(
                    format!("node {:?} has an invalid spline", node.id),
                    format!("Use 2..={MAX_SPLINE_POINTS} finite points."),
                ));
            }
        }
        ProceduralNodeKind::Grid {
            origin,
            columns,
            rows,
            spacing,
        } => {
            if !finite3(*origin)
                || *columns == 0
                || *rows == 0
                || columns.saturating_mul(*rows) > 10_000
            {
                return Err(world_error(
                    format!("node {:?} has invalid grid bounds", node.id),
                    "Use a finite origin and 1..=10,000 grid points.",
                ));
            }
            positive("grid X spacing", spacing[0])?;
            positive("grid Z spacing", spacing[1])?;
        }
        ProceduralNodeKind::Scatter {
            min,
            max,
            count,
            min_distance,
            ..
        } => {
            Bounds::new(*min, *max)?;
            if *count == 0 || *count > 10_000 {
                return Err(world_error(
                    format!("node {:?} has invalid scatter count", node.id),
                    "Use 1..=10,000 points.",
                ));
            }
            non_negative("scatter minimum distance", *min_distance)?;
        }
        ProceduralNodeKind::AlongSpline { spacing_m, .. } => {
            positive("along-spline spacing", *spacing_m)?;
        }
        ProceduralNodeKind::MergePoints { inputs } => {
            let unique = inputs.iter().collect::<BTreeSet<_>>();
            if inputs.len() < 2 || inputs.len() > 64 || unique.len() != inputs.len() {
                return Err(world_error(
                    format!(
                        "node {:?} merge inputs are duplicate or outside 2..=64",
                        node.id
                    ),
                    "Connect between 2 and 64 unique point sources.",
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

fn visit_procedural<'a>(
    id: &'a str,
    nodes: &BTreeMap<&'a str, &'a ProceduralNode>,
    states: &mut BTreeMap<&'a str, u8>,
    order: &mut Vec<&'a str>,
) -> Result<()> {
    match states.get(id).copied() {
        Some(1) => {
            return Err(world_error(
                format!("procedural graph contains a cycle at {id:?}"),
                "Break the cycle before compiling.",
            ));
        }
        Some(2) => return Ok(()),
        _ => {}
    }
    let node = nodes.get(id).ok_or_else(|| {
        world_error(
            format!("procedural node {id:?} does not exist"),
            "Reconnect the missing node.",
        )
    })?;
    states.insert(id, 1);
    for (socket, input, expected) in node.node.inputs() {
        let source = nodes.get(input).ok_or_else(|| {
            world_error(
                format!("node {id:?} input {socket:?} references missing node {input:?}"),
                "Reconnect the missing input.",
            )
        })?;
        let actual = source.node.output_type();
        if actual != expected {
            return Err(world_error(
                format!(
                    "node {id:?} input {socket:?} needs {expected:?}, got {actual:?} from {input:?}"
                ),
                "Connect a node with the required value type.",
            ));
        }
        visit_procedural(input, nodes, states, order)?;
    }
    states.insert(id, 2);
    order.push(id);
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
pub struct StreamingCellCoord {
    pub x: i32,
    pub z: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorldOriginStrategy {
    Fixed,
    Floating { rebase_threshold_m: f32 },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct StreamingCell {
    pub coord: StreamingCellCoord,
    pub sub_scene: String,
    #[serde(default)]
    pub terrain_chunks: Vec<TerrainChunkCoord>,
    #[serde(default)]
    pub dependencies: Vec<StreamingCellCoord>,
    pub estimated_memory_mb: u32,
    #[serde(default)]
    pub hlod_asset: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct WorldPartitionDocument {
    pub format: String,
    pub id: AssetId,
    pub name: String,
    pub cell_size_m: f32,
    pub origin_strategy: WorldOriginStrategy,
    #[serde(default)]
    pub cells: Vec<StreamingCell>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct StreamingBudget {
    pub max_concurrent_loads: u8,
    pub max_resident_cells: u16,
    pub max_resident_memory_mb: u32,
    pub max_queue: u16,
    pub request_timeout_ms: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum StreamingOperation {
    Load,
    Unload,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct StreamingRequest {
    pub request_id: String,
    pub partition_hash: String,
    pub operation: StreamingOperation,
    pub cell: StreamingCellCoord,
    pub priority: u32,
    pub estimated_memory_mb: u32,
    pub timeout_ms: u32,
    pub cancellation_token: String,
}

/// A bounded queue for a future async backend. It is not evidence that any I/O ran.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct StreamingPlan {
    pub partition_hash: String,
    pub max_concurrent_loads: u8,
    pub loads: Vec<StreamingRequest>,
    pub unloads: Vec<StreamingRequest>,
    pub projected_resident_cells: u16,
    pub projected_resident_memory_mb: u32,
    pub cancellation_supported: bool,
}

impl WorldPartitionDocument {
    pub fn parse(text: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(text).map_err(|error| {
            world_error(
                format!("invalid world partition document: {error}"),
                format!("World partitions use {WORLD_PARTITION_FORMAT} JSON."),
            )
        })?;
        document.validate()?;
        Ok(document)
    }

    pub fn dump(&self) -> Result<String> {
        canonical_json(self, "world partition")
    }

    pub fn hash(&self) -> Result<String> {
        self.validate()?;
        Ok(blake3::hash(self.dump()?.as_bytes()).to_hex().to_string())
    }

    pub fn validate(&self) -> Result<()> {
        require_format(&self.format, WORLD_PARTITION_FORMAT, "world partition")?;
        require_name(&self.name, "world partition")?;
        positive("world partition cell size", self.cell_size_m)?;
        if let WorldOriginStrategy::Floating { rebase_threshold_m } = self.origin_strategy {
            positive("origin rebase threshold", rebase_threshold_m)?;
            if rebase_threshold_m < self.cell_size_m {
                return Err(world_error(
                    "origin rebase threshold is smaller than one streaming cell",
                    "Use a threshold at least as large as cell_size_m.",
                ));
            }
        }
        if self.cells.is_empty() || self.cells.len() > MAX_STREAMING_CELLS {
            return Err(world_error(
                format!(
                    "streaming cell count {} is outside bounds",
                    self.cells.len()
                ),
                format!("Use 1..={MAX_STREAMING_CELLS} cells."),
            ));
        }
        let mut cells = BTreeMap::new();
        for cell in &self.cells {
            require_suffix(&cell.sub_scene, ".bscn.json", "streaming sub-scene")?;
            if cell.estimated_memory_mb == 0 || cell.estimated_memory_mb > 65_536 {
                return Err(world_error(
                    format!("cell {:?} has invalid memory estimate", cell.coord),
                    "Use a non-zero estimate no larger than 65,536 MiB.",
                ));
            }
            if let Some(hlod) = cell.hlod_asset.as_deref() {
                require_asset_reference(hlod, "HLOD asset")?;
            }
            if cells.insert(cell.coord, cell).is_some() {
                return Err(world_error(
                    format!("duplicate streaming cell {:?}", cell.coord),
                    "Give every cell one unique coordinate.",
                ));
            }
        }
        for cell in &self.cells {
            let mut unique = BTreeSet::new();
            for dependency in &cell.dependencies {
                if *dependency == cell.coord
                    || !cells.contains_key(dependency)
                    || !unique.insert(*dependency)
                {
                    return Err(world_error(
                        format!("cell {:?} has an invalid dependency", cell.coord),
                        "Dependencies must be unique existing cells other than the owner.",
                    ));
                }
            }
        }
        let mut states = BTreeMap::new();
        for coord in cells.keys().copied() {
            visit_cell(coord, &cells, &mut states)?;
        }
        Ok(())
    }

    pub fn plan_streaming(
        &self,
        focus: StreamingCellCoord,
        resident: &BTreeSet<StreamingCellCoord>,
        desired: &BTreeSet<StreamingCellCoord>,
        budget: StreamingBudget,
    ) -> Result<StreamingPlan> {
        self.validate()?;
        budget.validate()?;
        let cells = self
            .cells
            .iter()
            .map(|cell| (cell.coord, cell))
            .collect::<BTreeMap<_, _>>();
        for coord in resident.union(desired) {
            if !cells.contains_key(coord) {
                return Err(world_error(
                    format!("streaming set references unknown cell {coord:?}"),
                    "Refresh against the current partition document.",
                ));
            }
        }
        for coord in desired {
            for dependency in &cells[coord].dependencies {
                if !desired.contains(dependency) {
                    return Err(world_error(
                        format!("desired cell {coord:?} omits required dependency {dependency:?}"),
                        "Make the desired resident set dependency-closed.",
                    ));
                }
            }
        }
        if desired.len() > usize::from(budget.max_resident_cells) {
            return Err(world_error(
                format!("desired set has {} cells", desired.len()),
                format!(
                    "Budget allows {} resident cells.",
                    budget.max_resident_cells
                ),
            ));
        }
        let projected_memory = desired.iter().try_fold(0u32, |total, coord| {
            total
                .checked_add(cells[coord].estimated_memory_mb)
                .ok_or_else(|| {
                    world_error(
                        "streaming memory estimate overflow",
                        "Reduce the desired set.",
                    )
                })
        })?;
        if projected_memory > budget.max_resident_memory_mb {
            return Err(world_error(
                format!("desired set needs {projected_memory} MiB"),
                format!("Budget allows {} MiB.", budget.max_resident_memory_mb),
            ));
        }
        let queue_len = resident.symmetric_difference(desired).count();
        if queue_len > usize::from(budget.max_queue) {
            return Err(world_error(
                format!("streaming queue needs {queue_len} requests"),
                format!("Budget allows {} queued requests.", budget.max_queue),
            ));
        }
        let partition_hash = self.hash()?;
        let mut loads = desired
            .difference(resident)
            .copied()
            .collect::<Vec<StreamingCellCoord>>();
        loads.sort_by_key(|coord| (cell_distance(*coord, focus), *coord));
        let unloads = resident
            .difference(desired)
            .copied()
            .collect::<Vec<StreamingCellCoord>>();
        let loads = loads
            .into_iter()
            .enumerate()
            .map(|(priority, coord)| {
                streaming_request(
                    &partition_hash,
                    StreamingOperation::Load,
                    coord,
                    u32::try_from(priority).unwrap_or(u32::MAX),
                    cells[&coord].estimated_memory_mb,
                    budget.request_timeout_ms,
                )
            })
            .collect();
        let unloads = unloads
            .into_iter()
            .enumerate()
            .map(|(priority, coord)| {
                streaming_request(
                    &partition_hash,
                    StreamingOperation::Unload,
                    coord,
                    u32::try_from(priority).unwrap_or(u32::MAX),
                    cells[&coord].estimated_memory_mb,
                    budget.request_timeout_ms,
                )
            })
            .collect();
        Ok(StreamingPlan {
            partition_hash,
            max_concurrent_loads: budget.max_concurrent_loads,
            loads,
            unloads,
            projected_resident_cells: u16::try_from(desired.len()).unwrap_or(u16::MAX),
            projected_resident_memory_mb: projected_memory,
            cancellation_supported: true,
        })
    }

    /// Validate a queued request against current authored truth before an async backend uses it.
    pub fn validate_request(&self, request: &StreamingRequest) -> Result<()> {
        self.validate()?;
        let current_hash = self.hash()?;
        if request.partition_hash != current_hash {
            return Err(world_error(
                "streaming request was planned from a stale world partition",
                "Discard it and plan again from the current partition hash.",
            ));
        }
        let cell = self
            .cells
            .iter()
            .find(|cell| cell.coord == request.cell)
            .ok_or_else(|| {
                world_error(
                    format!(
                        "streaming request references unknown cell {:?}",
                        request.cell
                    ),
                    "Discard it and plan again.",
                )
            })?;
        let expected = streaming_request(
            &current_hash,
            request.operation,
            request.cell,
            request.priority,
            cell.estimated_memory_mb,
            request.timeout_ms,
        );
        if request.request_id != expected.request_id
            || request.cancellation_token != expected.cancellation_token
            || request.estimated_memory_mb != cell.estimated_memory_mb
            || !(100..=120_000).contains(&request.timeout_ms)
        {
            return Err(world_error(
                "streaming request does not match its partition-bound contract",
                "Discard the altered request and plan again.",
            ));
        }
        Ok(())
    }
}

impl StreamingBudget {
    pub fn validate(self) -> Result<()> {
        if self.max_concurrent_loads == 0
            || self.max_concurrent_loads > 32
            || self.max_resident_cells == 0
            || self.max_resident_memory_mb == 0
            || self.max_queue == 0
            || self.max_queue > MAX_STREAMING_QUEUE
            || !(100..=120_000).contains(&self.request_timeout_ms)
        {
            return Err(world_error(
                "streaming budget is zero or outside engine bounds",
                "Use 1..=32 concurrent loads, bounded resident/queue values and a 100..=120,000 ms timeout.",
            ));
        }
        Ok(())
    }
}

fn visit_cell(
    coord: StreamingCellCoord,
    cells: &BTreeMap<StreamingCellCoord, &StreamingCell>,
    states: &mut BTreeMap<StreamingCellCoord, u8>,
) -> Result<()> {
    match states.get(&coord).copied() {
        Some(1) => {
            return Err(world_error(
                format!("streaming-cell dependency cycle reaches {coord:?}"),
                "Break the cell dependency cycle.",
            ));
        }
        Some(2) => return Ok(()),
        _ => {}
    }
    states.insert(coord, 1);
    for dependency in &cells[&coord].dependencies {
        visit_cell(*dependency, cells, states)?;
    }
    states.insert(coord, 2);
    Ok(())
}

fn streaming_request(
    partition_hash: &str,
    operation: StreamingOperation,
    cell: StreamingCellCoord,
    priority: u32,
    estimated_memory_mb: u32,
    timeout_ms: u32,
) -> StreamingRequest {
    let operation_name = match operation {
        StreamingOperation::Load => "load",
        StreamingOperation::Unload => "unload",
    };
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"bhippi-stream-request@1");
    hasher.update(partition_hash.as_bytes());
    hasher.update(operation_name.as_bytes());
    hasher.update(&cell.x.to_le_bytes());
    hasher.update(&cell.z.to_le_bytes());
    let request_id = hasher.finalize().to_hex().to_string();
    StreamingRequest {
        cancellation_token: format!("cancel:{request_id}"),
        request_id,
        partition_hash: partition_hash.to_owned(),
        operation,
        cell,
        priority,
        estimated_memory_mb,
        timeout_ms,
    }
}

fn cell_distance(left: StreamingCellCoord, right: StreamingCellCoord) -> u32 {
    left.x
        .abs_diff(right.x)
        .saturating_add(left.z.abs_diff(right.z))
}

fn mix_seed(base: u64, offset: u64, coord: TerrainChunkCoord) -> u64 {
    let x_bits = u64::from(u32::from_le_bytes(coord.x.to_le_bytes()));
    let z_bits = u64::from(u32::from_le_bytes(coord.z.to_le_bytes()));
    base ^ offset.rotate_left(17)
        ^ x_bits.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ z_bits.wrapping_mul(0xBF58_476D_1CE4_E5B9)
}

fn canonical_json<T: Serialize>(value: &T, label: &str) -> Result<String> {
    serde_json::to_string_pretty(value).map_err(|error| {
        world_error(
            format!("cannot serialise {label}: {error}"),
            "Report this as an engine bug.",
        )
    })
}

fn require_format(actual: &str, expected: &str, label: &str) -> Result<()> {
    if actual != expected {
        return Err(world_error(
            format!("unsupported {label} format {actual:?}"),
            format!("Expected {expected}."),
        ));
    }
    Ok(())
}

fn require_name(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(world_error(
            format!("{label} name must not be empty"),
            format!("Give the {label} a name."),
        ));
    }
    Ok(())
}

fn require_suffix(value: &str, suffix: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || !value.ends_with(suffix) || value.contains("../") {
        return Err(world_error(
            format!("{label} reference {value:?} is invalid"),
            format!("Use a project-relative {suffix} reference."),
        ));
    }
    Ok(())
}

fn require_asset_reference(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty()
        || value.contains("../")
        || (!value.starts_with("assets/") && !value.starts_with("asset:"))
    {
        return Err(world_error(
            format!("{label} reference {value:?} is invalid"),
            "Use an asset:<id> or project-relative assets/ reference.",
        ));
    }
    Ok(())
}

fn positive(field: &str, value: f32) -> Result<()> {
    if !value.is_finite() || value <= 0.0 {
        return Err(world_error(
            format!("{field} must be finite and positive, got {value}"),
            "Use a positive finite value.",
        ));
    }
    Ok(())
}

fn non_negative(field: &str, value: f32) -> Result<()> {
    if !value.is_finite() || value < 0.0 {
        return Err(world_error(
            format!("{field} must be finite and non-negative, got {value}"),
            "Use a non-negative finite value.",
        ));
    }
    Ok(())
}

fn unit(field: &str, value: f32) -> Result<()> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(world_error(
            format!("{field} must be in 0..=1, got {value}"),
            "Use a finite normalized value.",
        ));
    }
    Ok(())
}

fn ordered_range(field: &str, values: [f32; 2], bounds: Option<[f32; 2]>) -> Result<()> {
    let in_bounds = bounds.is_none_or(|bounds| values[0] >= bounds[0] && values[1] <= bounds[1]);
    if !values[0].is_finite() || !values[1].is_finite() || values[0] > values[1] || !in_bounds {
        return Err(world_error(
            format!("{field} is not a finite ordered range"),
            "Put the lower bound first and stay inside the documented range.",
        ));
    }
    Ok(())
}

fn finite3(value: [f32; 3]) -> bool {
    value.iter().all(|component| component.is_finite())
}

fn world_error(message: impl Into<String>, hint: impl Into<String>) -> EngineError {
    EngineError::Asset(message.into(), Some(hint.into()))
}
