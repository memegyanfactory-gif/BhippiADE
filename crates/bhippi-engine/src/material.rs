//! The material document (`bhippi-material@1`, ENG-120) and the file-based shader
//! (`bhippi-shader@1`, ENG-121).
//!
//! `assets/materials/*.mat.json` and `assets/shaders/*.shader.json` have been referenced by
//! the Inspector, by `chat-engine.md` and by the scaffold since the engine track began,
//! while nothing has ever parsed them. That is why the AI could only ever *reference*
//! materials: it had no shape to write and no validator to be wrong against, so
//! `chat-engine.md` correctly forbade it from inventing one.
//!
//! Both formats are deterministic sorted-key JSON — diffable, hand-editable, and readable
//! by a model. Neither carries a node graph; ADR-0020 excluded visual shader graphs and that
//! decision stands until its own ADR replaces it.

use crate::error::{EngineError, Result};
use bhippi_types::AssetId;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

pub const MATERIAL_FORMAT: &str = "bhippi-material@1";
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

/// Which pass a shader is drawn in.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ShaderStage {
    #[default]
    Surface,
    PostProcess,
    Sky,
    Ui,
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AlphaMode, MaterialDocument, MaterialParams, ShaderDocument, ShaderStage, MAP_SLOTS,
    };

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
    fn a_shader_document_requires_wgsl_source() {
        let shader = ShaderDocument::new("water", "assets/shaders/water.wgsl");
        assert_eq!(shader.stage, ShaderStage::Surface);
        let text = shader.dump().expect("dump");
        assert_eq!(ShaderDocument::parse(&text).expect("parse"), shader);

        let bad = ShaderDocument::new("water", "assets/shaders/water.glsl");
        let error = bad.validate().expect_err("glsl cannot compile here");
        assert!(error.hint().is_some_and(|hint| hint.contains("wgsl")));
    }
}
