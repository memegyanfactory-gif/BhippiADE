//! The material document (`bhippi-material@1`, ENG-120) and the file-based shader
//! (`bhippi-shader@1`, ENG-121).
//!
//! `assets/materials/*.mat.json` and `assets/shaders/*.shader.json` have been referenced by
//! the Inspector, by `chat-engine.md` and by the scaffold since the engine track began,
//! while nothing has ever parsed them. That is why the AI could only ever *reference*
//! materials: it had no shape to write and no validator to be wrong against, so
//! `chat-engine.md` correctly forbade it from inventing one.
//!
//! All formats are deterministic sorted-key JSON — diffable, hand-editable, and readable
//! by a model. ADR-0037 admits a closed, typed material graph as domain data while keeping
//! GPU shader compilation and a visual graph editor outside this crate.

use crate::error::{EngineError, Result};
use bhippi_types::AssetId;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const MATERIAL_FORMAT: &str = "bhippi-material@1";
pub const MATERIAL_INSTANCE_FORMAT: &str = "bhippi-material-instance@1";
pub const MATERIAL_GRAPH_FORMAT: &str = "bhippi-material-graph@1";
pub const MATERIAL_PROGRAM_FORMAT: &str = "bhippi-material-program@1";
pub const SHADER_FORMAT: &str = "bhippi-shader@1";

/// The PBR map slots a material can bind. Fixed, because the renderer, the Inspector and
/// the schema registry's `MaterialOverride` all have to agree on this list — a free-form
/// map set would mean three places guessing.
pub const MAP_SLOTS: [&str; 6] = [
    "albedo",
    "normal",
    "roughness",
    "metallic",
    "ao",
    "emissive",
];

/// How a material's transparency is resolved.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AlphaMode {
    #[default]
    Opaque,
    /// Cut out at `alpha_cutoff`; no sorting cost.
    Mask,
    /// Sorted and blended.
    Blend,
}

/// Scalar and colour parameters. Everything here has a defensible default so a material
/// written with only an albedo map is still a complete, renderable document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialParams {
    /// Linear RGB tint multiplied with the albedo map.
    pub base_color: [f32; 3],
    pub roughness: f32,
    pub metallic: f32,
    pub emissive: [f32; 3],
    pub emissive_strength: f32,
    pub normal_strength: f32,
    pub tiling: [f32; 2],
    pub offset: [f32; 2],
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub double_sided: bool,
}

impl Default for MaterialParams {
    fn default() -> Self {
        Self {
            base_color: [0.8, 0.8, 0.8],
            roughness: 0.5,
            metallic: 0.0,
            emissive: [0.0, 0.0, 0.0],
            emissive_strength: 0.0,
            normal_strength: 1.0,
            tiling: [1.0, 1.0],
            offset: [0.0, 0.0],
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            double_sided: false,
        }
    }
}

/// Optional production lobes. Zero-valued defaults preserve the existing PBR material.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialLobes {
    pub clearcoat: f32,
    pub clearcoat_roughness: f32,
    pub sheen: f32,
    pub transmission: f32,
    pub anisotropy: f32,
    pub index_of_refraction: f32,
}

impl Default for MaterialLobes {
    fn default() -> Self {
        Self {
            clearcoat: 0.0,
            clearcoat_roughness: 0.0,
            sheen: 0.0,
            transmission: 0.0,
            anisotropy: 0.0,
            index_of_refraction: 1.5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MaterialLayerBlend {
    #[default]
    Mix,
    Add,
    Multiply,
}

/// One material layered over the base. The renderer contract receives this list in order;
/// no layer is silently flattened or reordered.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialLayer {
    pub material: String,
    pub weight: f32,
    #[serde(default)]
    pub blend: MaterialLayerBlend,
}

/// One `assets/materials/*.mat.json` document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialDocument {
    pub format: String,
    pub id: AssetId,
    pub name: String,
    /// Optional `assets/shaders/*.shader.json` this material is drawn with. Empty or absent
    /// means the standard PBR shader.
    #[serde(default)]
    pub shader: Option<String>,
    /// Map slot → texture reference (`asset:<ulid>` or a project-relative path). A slot may
    /// be absent or null; only the six known slots are accepted.
    #[serde(default)]
    pub maps: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub params: MaterialParams,
    #[serde(default)]
    pub lobes: MaterialLobes,
    #[serde(default)]
    pub layers: Vec<MaterialLayer>,
}

impl MaterialDocument {
    /// A new material with defaults and no maps bound.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            format: MATERIAL_FORMAT.to_owned(),
            id: AssetId::new(),
            name: name.into(),
            shader: None,
            maps: BTreeMap::new(),
            params: MaterialParams::default(),
            lobes: MaterialLobes::default(),
            layers: Vec::new(),
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let doc: Self = serde_json::from_str(text).map_err(|error| {
            EngineError::Asset(
                format!("invalid material document: {error}"),
                Some(
                    "Materials are bhippi-material@1 JSON; re-create it from the Inspector."
                        .to_owned(),
                ),
            )
        })?;
        doc.validate()?;
        Ok(doc)
    }

    /// Deterministic serialisation — `BTreeMap` keys, stable field order.
    pub fn dump(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            EngineError::Asset(
                format!("cannot serialise material: {error}"),
                Some("Report this as an engine bug.".to_owned()),
            )
        })
    }

    /// Reject anything the renderer or the Inspector could not honour. Ranges are clamped
    /// by refusal rather than silently, because a material quietly rewritten under the user
    /// is worse than one that says what is wrong.
    pub fn validate(&self) -> Result<()> {
        if self.format != MATERIAL_FORMAT {
            return Err(EngineError::Asset(
                format!("unsupported material format {:?}", self.format),
                Some(format!("Expected {MATERIAL_FORMAT}.")),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(EngineError::Asset(
                "material name must not be empty".to_owned(),
                Some("Give the material a name.".to_owned()),
            ));
        }
        for slot in self.maps.keys() {
            if !MAP_SLOTS.contains(&slot.as_str()) {
                return Err(EngineError::Asset(
                    format!("unknown material map slot {slot:?}"),
                    Some(format!("Valid slots: {}", MAP_SLOTS.join(", "))),
                ));
            }
        }
        unit("roughness", self.params.roughness)?;
        unit("metallic", self.params.metallic)?;
        unit("alpha_cutoff", self.params.alpha_cutoff)?;
        non_negative("emissive_strength", self.params.emissive_strength)?;
        non_negative("normal_strength", self.params.normal_strength)?;
        for (index, channel) in self.params.base_color.iter().enumerate() {
            unit(&format!("base_color[{index}]"), *channel)?;
        }
        for (index, channel) in self.params.emissive.iter().enumerate() {
            non_negative(&format!("emissive[{index}]"), *channel)?;
        }
        if self.params.tiling.contains(&0.0) {
            return Err(EngineError::Asset(
                "tiling must not be zero on either axis".to_owned(),
                Some("Use 1.0 for no tiling.".to_owned()),
            ));
        }
        if let Some(shader) = self.shader.as_deref() {
            if !shader.is_empty() && !shader.ends_with(".shader.json") {
                return Err(EngineError::Asset(
                    format!("shader reference {shader:?} is not a .shader.json file"),
                    Some("Point at assets/shaders/<name>.shader.json.".to_owned()),
                ));
            }
        }
        validate_lobes(&self.lobes)?;
        for layer in &self.layers {
            if !layer.material.ends_with(".mat.json") || layer.material.trim().is_empty() {
                return Err(EngineError::Asset(
                    format!(
                        "material layer {:?} is not a .mat.json reference",
                        layer.material
                    ),
                    Some("Point the layer at assets/materials/<name>.mat.json.".to_owned()),
                ));
            }
            unit("material layer weight", layer.weight)?;
        }
        Ok(())
    }

    /// Every texture this material binds, for the dependency gate (ENG-128).
    #[must_use]
    pub fn texture_refs(&self) -> Vec<&str> {
        self.maps
            .values()
            .filter_map(|value| value.as_deref())
            .filter(|value| !value.is_empty())
            .collect()
    }
}

fn validate_lobes(lobes: &MaterialLobes) -> Result<()> {
    for (name, value) in [
        ("clearcoat", lobes.clearcoat),
        ("clearcoat_roughness", lobes.clearcoat_roughness),
        ("sheen", lobes.sheen),
        ("transmission", lobes.transmission),
        ("anisotropy", lobes.anisotropy),
    ] {
        unit(name, value)?;
    }
    if !(1.0..=2.5).contains(&lobes.index_of_refraction) || !lobes.index_of_refraction.is_finite() {
        return Err(EngineError::Asset(
            format!(
                "index_of_refraction must be between 1 and 2.5, got {}",
                lobes.index_of_refraction
            ),
            Some("Use a physical dielectric index such as 1.5 for glass.".to_owned()),
        ));
    }
    Ok(())
}

/// Sparse, typed overrides for a material instance. `None` means inherit from the parent.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialParamOverrides {
    pub base_color: Option<[f32; 3]>,
    pub roughness: Option<f32>,
    pub metallic: Option<f32>,
    pub emissive: Option<[f32; 3]>,
    pub emissive_strength: Option<f32>,
    pub normal_strength: Option<f32>,
    pub tiling: Option<[f32; 2]>,
    pub offset: Option<[f32; 2]>,
    pub alpha_mode: Option<AlphaMode>,
    pub alpha_cutoff: Option<f32>,
    pub double_sided: Option<bool>,
}

impl MaterialParamOverrides {
    fn apply_to(&self, params: &mut MaterialParams) {
        if let Some(value) = self.base_color {
            params.base_color = value;
        }
        if let Some(value) = self.roughness {
            params.roughness = value;
        }
        if let Some(value) = self.metallic {
            params.metallic = value;
        }
        if let Some(value) = self.emissive {
            params.emissive = value;
        }
        if let Some(value) = self.emissive_strength {
            params.emissive_strength = value;
        }
        if let Some(value) = self.normal_strength {
            params.normal_strength = value;
        }
        if let Some(value) = self.tiling {
            params.tiling = value;
        }
        if let Some(value) = self.offset {
            params.offset = value;
        }
        if let Some(value) = self.alpha_mode {
            params.alpha_mode = value;
        }
        if let Some(value) = self.alpha_cutoff {
            params.alpha_cutoff = value;
        }
        if let Some(value) = self.double_sided {
            params.double_sided = value;
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialLobeOverrides {
    pub clearcoat: Option<f32>,
    pub clearcoat_roughness: Option<f32>,
    pub sheen: Option<f32>,
    pub transmission: Option<f32>,
    pub anisotropy: Option<f32>,
    pub index_of_refraction: Option<f32>,
}

impl MaterialLobeOverrides {
    fn apply_to(&self, lobes: &mut MaterialLobes) {
        if let Some(value) = self.clearcoat {
            lobes.clearcoat = value;
        }
        if let Some(value) = self.clearcoat_roughness {
            lobes.clearcoat_roughness = value;
        }
        if let Some(value) = self.sheen {
            lobes.sheen = value;
        }
        if let Some(value) = self.transmission {
            lobes.transmission = value;
        }
        if let Some(value) = self.anisotropy {
            lobes.anisotropy = value;
        }
        if let Some(value) = self.index_of_refraction {
            lobes.index_of_refraction = value;
        }
    }
}

/// A reusable child of a material. The parent remains an explicit dependency; resolution
/// takes the already-validated parent document and produces an immutable runtime value.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialInstanceDocument {
    pub format: String,
    pub id: AssetId,
    pub name: String,
    pub parent: String,
    #[serde(default)]
    pub params: MaterialParamOverrides,
    #[serde(default)]
    pub lobes: MaterialLobeOverrides,
    #[serde(default)]
    pub maps: BTreeMap<String, Option<String>>,
    /// `None` inherits; `Some(None)` selects the standard PBR shader.
    #[serde(default)]
    pub shader: Option<Option<String>>,
    /// `None` inherits the parent's layers; `Some` replaces them as one explicit list.
    #[serde(default)]
    pub layers: Option<Vec<MaterialLayer>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ResolvedMaterial {
    pub id: AssetId,
    pub name: String,
    pub parent: String,
    pub parent_id: AssetId,
    pub shader: Option<String>,
    pub maps: BTreeMap<String, Option<String>>,
    pub params: MaterialParams,
    pub lobes: MaterialLobes,
    pub layers: Vec<MaterialLayer>,
}

impl MaterialInstanceDocument {
    #[must_use]
    pub fn new(name: impl Into<String>, parent: impl Into<String>) -> Self {
        Self {
            format: MATERIAL_INSTANCE_FORMAT.to_owned(),
            id: AssetId::new(),
            name: name.into(),
            parent: parent.into(),
            params: MaterialParamOverrides::default(),
            lobes: MaterialLobeOverrides::default(),
            maps: BTreeMap::new(),
            shader: None,
            layers: None,
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(text).map_err(|error| {
            EngineError::Asset(
                format!("invalid material instance document: {error}"),
                Some(format!(
                    "Material instances use {MATERIAL_INSTANCE_FORMAT} JSON."
                )),
            )
        })?;
        document.validate()?;
        Ok(document)
    }

    pub fn dump(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            EngineError::Asset(
                format!("cannot serialise material instance: {error}"),
                Some("Report this as an engine bug.".to_owned()),
            )
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != MATERIAL_INSTANCE_FORMAT {
            return Err(EngineError::Asset(
                format!("unsupported material instance format {:?}", self.format),
                Some(format!("Expected {MATERIAL_INSTANCE_FORMAT}.")),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(EngineError::Asset(
                "material instance name must not be empty".to_owned(),
                Some("Give the instance a name.".to_owned()),
            ));
        }
        if !self.parent.ends_with(".mat.json") || self.parent.trim().is_empty() {
            return Err(EngineError::Asset(
                format!(
                    "material instance parent {:?} is not a .mat.json file",
                    self.parent
                ),
                Some("Point at assets/materials/<name>.mat.json.".to_owned()),
            ));
        }
        for slot in self.maps.keys() {
            if !MAP_SLOTS.contains(&slot.as_str()) {
                return Err(EngineError::Asset(
                    format!("unknown material instance map slot {slot:?}"),
                    Some(format!("Valid slots: {}", MAP_SLOTS.join(", "))),
                ));
            }
        }
        // Validate sparse values without needing to resolve the parent. Starting from the
        // engine defaults makes each supplied override observable to the normal material
        // validator while omitted fields retain valid values.
        let mut candidate = MaterialDocument::new(&self.name);
        self.params.apply_to(&mut candidate.params);
        self.lobes.apply_to(&mut candidate.lobes);
        candidate.maps.clone_from(&self.maps);
        if let Some(shader) = &self.shader {
            candidate.shader.clone_from(shader);
        }
        if let Some(layers) = &self.layers {
            candidate.layers.clone_from(layers);
        }
        candidate.validate()
    }

    pub fn resolve(&self, parent: &MaterialDocument) -> Result<ResolvedMaterial> {
        self.validate()?;
        parent.validate()?;
        let mut params = parent.params.clone();
        self.params.apply_to(&mut params);
        let mut lobes = parent.lobes.clone();
        self.lobes.apply_to(&mut lobes);
        let mut maps = parent.maps.clone();
        for (slot, value) in &self.maps {
            maps.insert(slot.clone(), value.clone());
        }
        let candidate = MaterialDocument {
            format: MATERIAL_FORMAT.to_owned(),
            id: self.id,
            name: self.name.clone(),
            shader: self.shader.clone().unwrap_or_else(|| parent.shader.clone()),
            maps: maps.clone(),
            params: params.clone(),
            lobes: lobes.clone(),
            layers: self.layers.clone().unwrap_or_else(|| parent.layers.clone()),
        };
        candidate.validate()?;
        Ok(ResolvedMaterial {
            id: self.id,
            name: self.name.clone(),
            parent: self.parent.clone(),
            parent_id: parent.id,
            shader: candidate.shader,
            maps,
            params,
            lobes,
            layers: candidate.layers,
        })
    }
}

fn unit(field: &str, value: f32) -> Result<()> {
    if !(0.0..=1.0).contains(&value) || !value.is_finite() {
        return Err(EngineError::Asset(
            format!("{field} must be between 0 and 1, got {value}"),
            Some("Use a value in the 0..1 range.".to_owned()),
        ));
    }
    Ok(())
}

fn non_negative(field: &str, value: f32) -> Result<()> {
    if value < 0.0 || !value.is_finite() {
        return Err(EngineError::Asset(
            format!("{field} must be zero or more, got {value}"),
            Some("Use a non-negative number.".to_owned()),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MaterialValueType {
    Scalar,
    Color,
    Normal,
    Surface,
}

/// Closed, typed graph vocabulary. Inputs name other node ids; arbitrary shader text is
/// deliberately outside this document and stays in validated `ShaderDocument` assets.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MaterialGraphNodeKind {
    Scalar {
        value: f32,
    },
    Color {
        value: [f32; 3],
    },
    ParameterScalar {
        name: String,
        default: f32,
    },
    ParameterColor {
        name: String,
        default: [f32; 3],
    },
    TextureColor {
        texture: String,
    },
    TextureNormal {
        texture: String,
    },
    AddScalar {
        left: String,
        right: String,
    },
    MultiplyScalar {
        left: String,
        right: String,
    },
    MultiplyColor {
        left: String,
        right: String,
    },
    PbrOutput {
        base_color: String,
        roughness: String,
        metallic: String,
        #[serde(default)]
        normal: Option<String>,
        #[serde(default)]
        emissive: Option<String>,
    },
}

impl MaterialGraphNodeKind {
    fn output_type(&self) -> MaterialValueType {
        match self {
            Self::Scalar { .. }
            | Self::ParameterScalar { .. }
            | Self::AddScalar { .. }
            | Self::MultiplyScalar { .. } => MaterialValueType::Scalar,
            Self::Color { .. }
            | Self::ParameterColor { .. }
            | Self::TextureColor { .. }
            | Self::MultiplyColor { .. } => MaterialValueType::Color,
            Self::TextureNormal { .. } => MaterialValueType::Normal,
            Self::PbrOutput { .. } => MaterialValueType::Surface,
        }
    }

    fn inputs(&self) -> Vec<(&'static str, &str, MaterialValueType)> {
        match self {
            Self::AddScalar { left, right } | Self::MultiplyScalar { left, right } => vec![
                ("left", left.as_str(), MaterialValueType::Scalar),
                ("right", right.as_str(), MaterialValueType::Scalar),
            ],
            Self::MultiplyColor { left, right } => vec![
                ("left", left.as_str(), MaterialValueType::Color),
                ("right", right.as_str(), MaterialValueType::Color),
            ],
            Self::PbrOutput {
                base_color,
                roughness,
                metallic,
                normal,
                emissive,
            } => {
                let mut inputs = vec![
                    ("base_color", base_color.as_str(), MaterialValueType::Color),
                    ("roughness", roughness.as_str(), MaterialValueType::Scalar),
                    ("metallic", metallic.as_str(), MaterialValueType::Scalar),
                ];
                if let Some(normal) = normal {
                    inputs.push(("normal", normal.as_str(), MaterialValueType::Normal));
                }
                if let Some(emissive) = emissive {
                    inputs.push(("emissive", emissive.as_str(), MaterialValueType::Color));
                }
                inputs
            }
            _ => Vec::new(),
        }
    }

    fn parameter(&self) -> Option<(&str, MaterialValueType)> {
        match self {
            Self::ParameterScalar { name, .. } => Some((name, MaterialValueType::Scalar)),
            Self::ParameterColor { name, .. } => Some((name, MaterialValueType::Color)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialGraphNode {
    pub id: String,
    #[serde(flatten)]
    pub node: MaterialGraphNodeKind,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialGraphDocument {
    pub format: String,
    pub id: AssetId,
    pub name: String,
    pub output: String,
    #[serde(default)]
    pub nodes: Vec<MaterialGraphNode>,
}

/// Validated, dependency-ordered graph consumed by a future renderer backend. This is a
/// real compile target, but not GPU bytecode and does not claim shader compilation.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct MaterialGraphProgram {
    pub format: String,
    pub graph_id: AssetId,
    pub source_hash: String,
    pub output: String,
    pub parameters: BTreeMap<String, MaterialValueType>,
    pub operations: Vec<MaterialGraphNode>,
}

impl MaterialGraphDocument {
    pub fn parse(text: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(text).map_err(|error| {
            EngineError::Asset(
                format!("invalid material graph document: {error}"),
                Some(format!("Material graphs use {MATERIAL_GRAPH_FORMAT} JSON.")),
            )
        })?;
        document.validate()?;
        Ok(document)
    }

    pub fn dump(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            EngineError::Asset(
                format!("cannot serialise material graph: {error}"),
                Some("Report this as an engine bug.".to_owned()),
            )
        })
    }

    pub fn validate(&self) -> Result<()> {
        self.compile().map(|_| ())
    }

    pub fn compile(&self) -> Result<MaterialGraphProgram> {
        if self.format != MATERIAL_GRAPH_FORMAT {
            return Err(EngineError::Asset(
                format!("unsupported material graph format {:?}", self.format),
                Some(format!("Expected {MATERIAL_GRAPH_FORMAT}.")),
            ));
        }
        if self.name.trim().is_empty() || self.output.trim().is_empty() {
            return Err(EngineError::Asset(
                "material graph name and output must not be empty".to_owned(),
                Some("Name the graph and point `output` at its pbr_output node.".to_owned()),
            ));
        }
        let mut nodes = BTreeMap::new();
        for node in &self.nodes {
            if node.id.trim().is_empty() || nodes.insert(node.id.as_str(), node).is_some() {
                return Err(EngineError::Asset(
                    format!(
                        "material graph has an empty or duplicate node id {:?}",
                        node.id
                    ),
                    Some("Give every graph node a unique id.".to_owned()),
                ));
            }
            match &node.node {
                MaterialGraphNodeKind::TextureColor { texture }
                | MaterialGraphNodeKind::TextureNormal { texture }
                    if texture.trim().is_empty() =>
                {
                    return Err(EngineError::Asset(
                        format!("texture node {:?} has no asset reference", node.id),
                        Some("Choose a texture asset.".to_owned()),
                    ));
                }
                MaterialGraphNodeKind::ParameterScalar { name, default } => {
                    if name.trim().is_empty() || !default.is_finite() {
                        return Err(EngineError::Asset(
                            format!("scalar parameter on {:?} is invalid", node.id),
                            Some("Name the parameter and use a finite default.".to_owned()),
                        ));
                    }
                }
                MaterialGraphNodeKind::ParameterColor { name, default } => {
                    if name.trim().is_empty() || default.iter().any(|value| !value.is_finite()) {
                        return Err(EngineError::Asset(
                            format!("color parameter on {:?} is invalid", node.id),
                            Some("Name the parameter and use finite channels.".to_owned()),
                        ));
                    }
                }
                MaterialGraphNodeKind::Scalar { value } if !value.is_finite() => {
                    return Err(EngineError::Asset(
                        format!("scalar node {:?} is not finite", node.id),
                        Some("Use a finite scalar.".to_owned()),
                    ));
                }
                MaterialGraphNodeKind::Color { value }
                    if value.iter().any(|channel| !channel.is_finite()) =>
                {
                    return Err(EngineError::Asset(
                        format!("color node {:?} is not finite", node.id),
                        Some("Use finite color channels.".to_owned()),
                    ));
                }
                _ => {}
            }
        }
        let output = nodes.get(self.output.as_str()).ok_or_else(|| {
            EngineError::Asset(
                format!("material graph output {:?} does not exist", self.output),
                Some("Point `output` at a pbr_output node.".to_owned()),
            )
        })?;
        if output.node.output_type() != MaterialValueType::Surface {
            return Err(EngineError::Asset(
                format!("material graph output {:?} is not a surface", self.output),
                Some("Use a pbr_output node as the graph output.".to_owned()),
            ));
        }

        let mut state = BTreeMap::new();
        let mut order = Vec::new();
        visit_material_node(self.output.as_str(), &nodes, &mut state, &mut order)?;
        if order.len() != self.nodes.len() {
            let reachable = order.iter().copied().collect::<BTreeSet<_>>();
            let unused = self
                .nodes
                .iter()
                .filter(|node| !reachable.contains(node.id.as_str()))
                .map(|node| node.id.clone())
                .collect::<Vec<_>>();
            return Err(EngineError::Asset(
                format!(
                    "material graph has unreachable nodes: {}",
                    unused.join(", ")
                ),
                Some("Connect or remove every node before compiling.".to_owned()),
            ));
        }
        let mut parameters = BTreeMap::new();
        for node in &self.nodes {
            if let Some((name, value_type)) = node.node.parameter() {
                if parameters.insert(name.to_owned(), value_type).is_some() {
                    return Err(EngineError::Asset(
                        format!("material parameter {name:?} is declared more than once"),
                        Some("Give every exposed parameter one unique name.".to_owned()),
                    ));
                }
            }
        }
        let source = self.dump()?;
        Ok(MaterialGraphProgram {
            format: MATERIAL_PROGRAM_FORMAT.to_owned(),
            graph_id: self.id,
            source_hash: blake3::hash(source.as_bytes()).to_hex().to_string(),
            output: self.output.clone(),
            parameters,
            operations: order
                .into_iter()
                .filter_map(|id| nodes.get(id).map(|node| (*node).clone()))
                .collect(),
        })
    }
}

fn visit_material_node<'a>(
    id: &'a str,
    nodes: &BTreeMap<&'a str, &'a MaterialGraphNode>,
    state: &mut BTreeMap<&'a str, u8>,
    order: &mut Vec<&'a str>,
) -> Result<()> {
    match state.get(id).copied() {
        Some(1) => {
            return Err(EngineError::Asset(
                format!("material graph contains a cycle at {id:?}"),
                Some("Break the cycle before compiling.".to_owned()),
            ));
        }
        Some(2) => return Ok(()),
        _ => {}
    }
    let node = nodes.get(id).ok_or_else(|| {
        EngineError::Asset(
            format!("material graph node {id:?} does not exist"),
            Some("Reconnect the missing input.".to_owned()),
        )
    })?;
    state.insert(id, 1);
    for (socket, input, expected) in node.node.inputs() {
        let source = nodes.get(input).ok_or_else(|| {
            EngineError::Asset(
                format!("node {id:?} input {socket:?} references missing node {input:?}"),
                Some("Reconnect the missing input.".to_owned()),
            )
        })?;
        let actual = source.node.output_type();
        if actual != expected {
            return Err(EngineError::Asset(
                format!(
                    "node {id:?} input {socket:?} needs {expected:?}, got {actual:?} from {input:?}"
                ),
                Some("Connect a node with the required value type.".to_owned()),
            ));
        }
        visit_material_node(input, nodes, state, order)?;
    }
    state.insert(id, 2);
    order.push(id);
    Ok(())
}

/// Which pass a shader is drawn in.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ShaderStage {
    #[default]
    Surface,
    PostProcess,
    Sky,
    Ui,
    Compute,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ShaderCapability {
    Compute,
    StorageBuffer,
    StorageTexture,
    Float16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ShaderBindingKind {
    UniformFloat,
    UniformVec2,
    UniformVec3,
    UniformVec4,
    Texture2d,
    Sampler,
    StorageBuffer,
    StorageTexture,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ShaderCompileContract {
    pub descriptor_hash: String,
    pub stage: ShaderStage,
    pub source: String,
    pub entry_point: String,
    pub includes: Vec<String>,
    pub variants: BTreeMap<String, Vec<String>>,
    pub bindings: BTreeMap<String, ShaderBindingKind>,
    pub required_capabilities: BTreeSet<ShaderCapability>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ShaderPlatformCapabilities {
    pub platform: String,
    #[serde(default)]
    pub supported: BTreeSet<ShaderCapability>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ShaderSupportReport {
    pub platform: String,
    pub supported: bool,
    pub missing: Vec<ShaderCapability>,
}

fn default_shader_entry_point() -> String {
    "main".to_owned()
}

/// One `assets/shaders/*.shader.json` document — a **file-based** shader, assignable to a
/// mesh via `ShaderRef`. Not a node graph (ADR-0020).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ShaderDocument {
    pub format: String,
    pub id: AssetId,
    pub name: String,
    pub stage: ShaderStage,
    /// The WGSL source file this shader compiles, relative to the project.
    pub source: String,
    #[serde(default = "default_shader_entry_point")]
    pub entry_point: String,
    /// Project-relative WGSL snippets included by the backend before compilation.
    #[serde(default)]
    pub includes: Vec<String>,
    /// Compile-time axes. Values remain strings so the backend owns their WGSL mapping.
    #[serde(default)]
    pub variants: BTreeMap<String, Vec<String>>,
    /// Declared reflection surface; backend reflection must match this contract exactly.
    #[serde(default)]
    pub bindings: BTreeMap<String, ShaderBindingKind>,
    #[serde(default)]
    pub required_capabilities: BTreeSet<ShaderCapability>,
    /// Named parameters the material may override, with their default values.
    #[serde(default)]
    pub params: BTreeMap<String, serde_json::Value>,
}

impl ShaderDocument {
    #[must_use]
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            format: SHADER_FORMAT.to_owned(),
            id: AssetId::new(),
            name: name.into(),
            stage: ShaderStage::Surface,
            source: source.into(),
            entry_point: default_shader_entry_point(),
            includes: Vec::new(),
            variants: BTreeMap::new(),
            bindings: BTreeMap::new(),
            required_capabilities: BTreeSet::new(),
            params: BTreeMap::new(),
        }
    }

    pub fn parse(text: &str) -> Result<Self> {
        let doc: Self = serde_json::from_str(text).map_err(|error| {
            EngineError::Asset(
                format!("invalid shader document: {error}"),
                Some("Shaders are bhippi-shader@1 JSON.".to_owned()),
            )
        })?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn dump(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|error| {
            EngineError::Asset(
                format!("cannot serialise shader: {error}"),
                Some("Report this as an engine bug.".to_owned()),
            )
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != SHADER_FORMAT {
            return Err(EngineError::Asset(
                format!("unsupported shader format {:?}", self.format),
                Some(format!("Expected {SHADER_FORMAT}.")),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(EngineError::Asset(
                "shader name must not be empty".to_owned(),
                Some("Give the shader a name.".to_owned()),
            ));
        }
        // The renderer compiles WGSL. Accepting a `.glsl` here would mean a shader that
        // validates and then fails at draw time, which is the worst place to find out.
        if !self.source.ends_with(".wgsl") {
            return Err(EngineError::Asset(
                format!("shader source {:?} must be a .wgsl file", self.source),
                Some("Point `source` at a .wgsl file under assets/shaders/.".to_owned()),
            ));
        }
        validate_shader_path("source", &self.source)?;
        if self.entry_point.trim().is_empty()
            || !self
                .entry_point
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(EngineError::Asset(
                format!("shader entry point {:?} is invalid", self.entry_point),
                Some("Use a WGSL identifier such as main or cs_main.".to_owned()),
            ));
        }
        let mut includes = BTreeSet::new();
        for include in &self.includes {
            validate_shader_path("include", include)?;
            if !include.ends_with(".wgsl") || !includes.insert(include.as_str()) {
                return Err(EngineError::Asset(
                    format!("shader include {include:?} is duplicated or not WGSL"),
                    Some("Use each project-relative .wgsl include once.".to_owned()),
                ));
            }
        }
        for (axis, values) in &self.variants {
            if axis.trim().is_empty()
                || values.is_empty()
                || values.iter().any(|value| value.trim().is_empty())
            {
                return Err(EngineError::Asset(
                    format!("shader variant axis {axis:?} has no usable values"),
                    Some("Name the axis and give it at least one non-empty value.".to_owned()),
                ));
            }
            let unique = values.iter().collect::<BTreeSet<_>>();
            if unique.len() != values.len() {
                return Err(EngineError::Asset(
                    format!("shader variant axis {axis:?} repeats a value"),
                    Some("Remove the duplicate permutation value.".to_owned()),
                ));
            }
        }
        for name in self.bindings.keys() {
            if name.trim().is_empty() {
                return Err(EngineError::Asset(
                    "shader binding names must not be empty".to_owned(),
                    Some("Name every reflected binding.".to_owned()),
                ));
            }
        }
        if self.stage == ShaderStage::Compute
            && !self
                .required_capabilities
                .contains(&ShaderCapability::Compute)
        {
            return Err(EngineError::Asset(
                "compute shader does not declare the compute capability".to_owned(),
                Some("Add `compute` to required_capabilities.".to_owned()),
            ));
        }
        Ok(())
    }

    pub fn compile_contract(&self) -> Result<ShaderCompileContract> {
        self.validate()?;
        let descriptor = self.dump()?;
        Ok(ShaderCompileContract {
            descriptor_hash: blake3::hash(descriptor.as_bytes()).to_hex().to_string(),
            stage: self.stage,
            source: self.source.clone(),
            entry_point: self.entry_point.clone(),
            includes: self.includes.clone(),
            variants: self.variants.clone(),
            bindings: self.bindings.clone(),
            required_capabilities: self.required_capabilities.clone(),
        })
    }

    pub fn support_report(
        &self,
        platform: &ShaderPlatformCapabilities,
    ) -> Result<ShaderSupportReport> {
        self.validate()?;
        let missing = self
            .required_capabilities
            .difference(&platform.supported)
            .copied()
            .collect::<Vec<_>>();
        Ok(ShaderSupportReport {
            platform: platform.platform.clone(),
            supported: missing.is_empty(),
            missing,
        })
    }
}

fn validate_shader_path(field: &str, path: &str) -> Result<()> {
    let normalized = path.replace('\\', "/");
    if normalized.starts_with('/')
        || normalized.contains("../")
        || normalized.contains(":/")
        || !normalized.starts_with("assets/shaders/")
    {
        return Err(EngineError::Asset(
            format!("shader {field} path {path:?} leaves assets/shaders"),
            Some("Use a project-relative path under assets/shaders/.".to_owned()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AlphaMode, MaterialDocument, MaterialGraphDocument, MaterialGraphNode,
        MaterialGraphNodeKind, MaterialInstanceDocument, MaterialParams, MaterialValueType,
        ShaderCapability, ShaderDocument, ShaderPlatformCapabilities, ShaderStage, MAP_SLOTS,
        MATERIAL_GRAPH_FORMAT,
    };
    use bhippi_types::AssetId;
    use std::collections::BTreeSet;

    #[test]
    fn a_default_material_round_trips_deterministically() {
        let material = MaterialDocument::new("crate_wood");
        let first = material.dump().expect("dump");
        let second = material.dump().expect("dump");
        assert_eq!(first, second);
        let reparsed = MaterialDocument::parse(&first).expect("parse");
        assert_eq!(reparsed, material);
    }

    #[test]
    fn maps_are_restricted_to_the_known_pbr_slots() {
        let mut material = MaterialDocument::new("wood");
        for slot in MAP_SLOTS {
            material
                .maps
                .insert(slot.to_owned(), Some(format!("assets/textures/{slot}.png")));
        }
        material.validate().expect("every known slot is allowed");
        assert_eq!(material.texture_refs().len(), 6);

        material
            .maps
            .insert("shininess".to_owned(), Some("x.png".to_owned()));
        let error = material.validate().expect_err("unknown slot");
        assert!(error.hint().is_some_and(|hint| hint.contains("albedo")));
    }

    #[test]
    fn out_of_range_scalars_are_refused_not_clamped() {
        let mut material = MaterialDocument::new("wood");
        material.params.roughness = 1.4;
        let error = material.validate().expect_err("roughness > 1");
        assert!(error.to_string().contains("roughness"));

        material.params = MaterialParams {
            metallic: -0.2,
            ..MaterialParams::default()
        };
        assert!(material.validate().is_err());

        material.params = MaterialParams {
            tiling: [0.0, 1.0],
            ..MaterialParams::default()
        };
        let error = material.validate().expect_err("zero tiling");
        assert!(error.hint().is_some());
    }

    #[test]
    fn emissive_may_exceed_one_because_it_is_radiance_not_albedo() {
        let mut material = MaterialDocument::new("lamp");
        material.params.emissive = [4.0, 3.5, 1.0];
        material.params.emissive_strength = 12.0;
        material
            .validate()
            .expect("emissive is not clamped to the 0..1 albedo range");
    }

    #[test]
    fn a_shader_reference_must_point_at_a_shader_document() {
        let mut material = MaterialDocument::new("wood");
        material.shader = Some("assets/shaders/pbr.wgsl".to_owned());
        let error = material.validate().expect_err("wgsl is not the document");
        assert!(error.hint().is_some());

        material.shader = Some("assets/shaders/pbr.shader.json".to_owned());
        material.validate().expect("a shader document is fine");
    }

    #[test]
    fn a_wrong_format_marker_is_rejected_with_the_expected_one() {
        let mut material = MaterialDocument::new("wood");
        material.format = "bhippi-material@2".to_owned();
        let text = serde_json::to_string(&material).expect("serialise");
        let error = MaterialDocument::parse(&text).expect_err("future format");
        assert!(error.hint().is_some_and(|hint| hint.contains("@1")));
    }

    #[test]
    fn alpha_mode_round_trips_through_snake_case() {
        let mut material = MaterialDocument::new("glass");
        material.params.alpha_mode = AlphaMode::Blend;
        let text = material.dump().expect("dump");
        assert!(text.contains("\"blend\""));
        assert_eq!(
            MaterialDocument::parse(&text)
                .expect("parse")
                .params
                .alpha_mode,
            AlphaMode::Blend
        );
    }

    #[test]
    fn material_instances_inherit_and_apply_only_explicit_overrides() {
        let mut parent = MaterialDocument::new("paint");
        parent.params.roughness = 0.8;
        parent.params.metallic = 0.2;
        parent.lobes.clearcoat = 0.3;
        parent.maps.insert(
            "albedo".to_owned(),
            Some("assets/textures/paint.png".to_owned()),
        );

        let mut instance =
            MaterialInstanceDocument::new("wet_paint", "assets/materials/paint.mat.json");
        instance.params.roughness = Some(0.1);
        instance.lobes.clearcoat = Some(1.0);
        instance.maps.insert("albedo".to_owned(), None);
        let resolved = instance.resolve(&parent).expect("resolve instance");

        assert_eq!(resolved.parent_id, parent.id);
        assert_eq!(resolved.params.roughness, 0.1);
        assert_eq!(resolved.params.metallic, 0.2, "unspecified value inherits");
        assert_eq!(resolved.lobes.clearcoat, 1.0);
        assert_eq!(resolved.maps.get("albedo"), Some(&None));
        assert_eq!(
            MaterialInstanceDocument::parse(&instance.dump().expect("dump")).expect("parse"),
            instance
        );
    }

    #[test]
    fn material_instance_invalid_overrides_fail_before_parent_resolution() {
        let mut instance = MaterialInstanceDocument::new("bad", "assets/materials/paint.mat.json");
        instance.params.roughness = Some(1.5);
        let error = instance.validate().expect_err("invalid sparse override");
        assert!(error.to_string().contains("roughness"));
    }

    fn valid_graph() -> MaterialGraphDocument {
        MaterialGraphDocument {
            format: MATERIAL_GRAPH_FORMAT.to_owned(),
            id: AssetId::new(),
            name: "paint".to_owned(),
            output: "surface".to_owned(),
            nodes: vec![
                MaterialGraphNode {
                    id: "surface".to_owned(),
                    node: MaterialGraphNodeKind::PbrOutput {
                        base_color: "tint".to_owned(),
                        roughness: "roughness".to_owned(),
                        metallic: "metallic".to_owned(),
                        normal: None,
                        emissive: None,
                    },
                },
                MaterialGraphNode {
                    id: "tint".to_owned(),
                    node: MaterialGraphNodeKind::ParameterColor {
                        name: "tint".to_owned(),
                        default: [0.8, 0.2, 0.1],
                    },
                },
                MaterialGraphNode {
                    id: "roughness".to_owned(),
                    node: MaterialGraphNodeKind::ParameterScalar {
                        name: "roughness".to_owned(),
                        default: 0.6,
                    },
                },
                MaterialGraphNode {
                    id: "metallic".to_owned(),
                    node: MaterialGraphNodeKind::Scalar { value: 0.0 },
                },
            ],
        }
    }

    #[test]
    fn typed_material_graph_compiles_to_dependency_ordered_program() {
        let graph = valid_graph();
        let first = graph.compile().expect("compile");
        let second = graph.compile().expect("deterministic recompile");
        assert_eq!(first, second);
        assert_eq!(
            first.parameters.get("tint"),
            Some(&MaterialValueType::Color)
        );
        assert_eq!(
            first.operations.last().map(|node| node.id.as_str()),
            Some("surface")
        );
        assert!(first
            .operations
            .iter()
            .position(|node| node.id == "roughness")
            .is_some_and(|position| position < first.operations.len() - 1));
    }

    #[test]
    fn typed_material_graph_refuses_wrong_socket_types_and_unreachable_nodes() {
        let mut wrong_type = valid_graph();
        let surface = wrong_type
            .nodes
            .iter_mut()
            .find(|node| node.id == "surface")
            .expect("surface");
        if let MaterialGraphNodeKind::PbrOutput { roughness, .. } = &mut surface.node {
            *roughness = "tint".to_owned();
        }
        let error = wrong_type.compile().expect_err("color into scalar socket");
        assert!(error.to_string().contains("needs Scalar"));

        let mut unreachable = valid_graph();
        unreachable.nodes.push(MaterialGraphNode {
            id: "orphan".to_owned(),
            node: MaterialGraphNodeKind::Scalar { value: 0.5 },
        });
        let error = unreachable.compile().expect_err("orphan must be explicit");
        assert!(error.to_string().contains("unreachable"));
    }

    #[test]
    fn a_shader_document_requires_wgsl_source() {
        let shader = ShaderDocument::new("water", "assets/shaders/water.wgsl");
        assert_eq!(shader.stage, ShaderStage::Surface);
        let text = shader.dump().expect("dump");
        assert_eq!(ShaderDocument::parse(&text).expect("parse"), shader);

        let bad = ShaderDocument::new("water", "assets/shaders/water.glsl");
        let error = bad.validate().expect_err("glsl cannot compile here");
        assert!(error.hint().is_some_and(|hint| hint.contains("wgsl")));
    }

    #[test]
    fn compute_shader_contract_is_deterministic_and_platform_gated() {
        let mut shader = ShaderDocument::new("particles", "assets/shaders/particles.wgsl");
        shader.stage = ShaderStage::Compute;
        shader.entry_point = "cs_main".to_owned();
        shader.required_capabilities =
            BTreeSet::from([ShaderCapability::Compute, ShaderCapability::StorageBuffer]);
        let first = shader.compile_contract().expect("contract");
        let second = shader.compile_contract().expect("stable contract");
        assert_eq!(first, second);

        let report = shader
            .support_report(&ShaderPlatformCapabilities {
                platform: "web".to_owned(),
                supported: BTreeSet::from([ShaderCapability::Compute]),
            })
            .expect("support report");
        assert!(!report.supported);
        assert_eq!(report.missing, vec![ShaderCapability::StorageBuffer]);
    }

    #[test]
    fn shader_contract_refuses_escaped_paths_and_undeclared_compute() {
        let escaped = ShaderDocument::new("bad", "assets/shaders/../secret.wgsl");
        assert!(escaped
            .validate()
            .expect_err("path escape")
            .hint()
            .is_some());

        let mut compute = ShaderDocument::new("compute", "assets/shaders/compute.wgsl");
        compute.stage = ShaderStage::Compute;
        let error = compute.validate().expect_err("capability must be explicit");
        assert!(error.to_string().contains("compute capability"));
    }
}
