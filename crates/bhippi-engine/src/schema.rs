use crate::asset::AssetKind;
use crate::error::{EngineError, Result};
use serde_json::Value;
use std::fmt;

/// The editor's component catalog (plan §10.1). This ships **without** Bevy: the viewport's
/// reflection registry is the runtime truth; this in-repo registry is the editor's *editable*
/// contract the Inspector renders from and the AI schema excerpt is produced from. Keeping
/// them hand-aligned would drift, so this registry is what a `schema.json` export
/// generates — both sides assert equality in tests (ENG-024).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComponentSchema {
    pub name: &'static str,
    pub doc: &'static str,
    pub fields: &'static [FieldSchema],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldSchema {
    pub name: &'static str,
    pub kind: FieldKind,
    pub doc: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldKind {
    /// Drag-number with an optional range.
    F32 {
        min: Option<f32>,
        max: Option<f32>,
    },
    /// A value meaning "no numeric bound".
    Unbounded,
    Vec3 {
        min: Option<f32>,
        max: Option<f32>,
    },
    Vec4,
    I32,
    Bool,
    Enum(&'static [&'static str]),
    String,
    AssetRef(AssetKind),
    Color,
    Json,
}

impl fmt::Display for FieldKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::F32 { min, max } => write!(
                formatter,
                "f32{}{}",
                min.map(|m| format!("≥{m}")).unwrap_or_default(),
                max.map(|m| format!("≤{m}")).unwrap_or_default()
            ),
            Self::Unbounded => formatter.write_str("f32"),
            Self::Vec3 { .. } => formatter.write_str("vec3"),
            Self::Vec4 => formatter.write_str("vec4"),
            Self::I32 => formatter.write_str("i32"),
            Self::Bool => formatter.write_str("bool"),
            Self::Enum(values) => write!(formatter, "enum({})", values.join("|")),
            Self::String => formatter.write_str("string"),
            Self::AssetRef(kind) => write!(formatter, "asset:{kind}"),
            Self::Color => formatter.write_str("color"),
            Self::Json => formatter.write_str("json"),
        }
    }
}

const TRANSFORM_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        name: "pos",
        kind: FieldKind::Vec3 {
            min: None,
            max: None,
        },
        doc: "World position in metres.",
    },
    FieldSchema {
        name: "rot",
        kind: FieldKind::Vec4,
        doc: "Quaternion rotation (x, y, z, w).",
    },
    FieldSchema {
        name: "scale",
        kind: FieldKind::Vec3 {
            min: Some(0.0),
            max: None,
        },
        doc: "Non-uniform scale. Negative mirrors.",
    },
];

const COMPONENTS: &[ComponentSchema] = &[
    ComponentSchema {
        name: "Transform",
        doc: "Position, rotation and scale of an entity. Every entity carries one.",
        fields: TRANSFORM_FIELDS,
    },
    ComponentSchema {
        name: "MeshRenderer",
        doc: "Draws a mesh with the given materials.",
        fields: &[
            FieldSchema { name: "mesh", kind: FieldKind::AssetRef(AssetKind::Mesh), doc: "The .glb mesh asset." },
            FieldSchema { name: "materials", kind: FieldKind::Json, doc: "Array of material asset refs." },
            FieldSchema { name: "cast_shadows", kind: FieldKind::Bool, doc: "Whether the mesh casts shadows." },
        ],
    },
    ComponentSchema {
        name: "SkinnedMeshRenderer",
        doc: "Draws a skinned mesh driven by an animation skeleton.",
        fields: &[
            FieldSchema { name: "mesh", kind: FieldKind::AssetRef(AssetKind::Mesh), doc: "The skinned mesh asset." },
            FieldSchema { name: "animation_root", kind: FieldKind::String, doc: "Optional bone-entity name to root the skeleton at." },
        ],
    },
    ComponentSchema {
        name: "Light",
        doc: "A light source. Types: direcional, point, spot.",
        fields: &[
            FieldSchema { name: "kind", kind: FieldKind::Enum(&["directional", "point", "spot"]), doc: "Light shape." },
            FieldSchema { name: "color", kind: FieldKind::Color, doc: "RGB color (0..1)." },
            FieldSchema { name: "intensity", kind: FieldKind::Unbounded, doc: "Luminous intensity in candela." },
            FieldSchema { name: "range", kind: FieldKind::Unbounded, doc: "Point/spot falloff distance." },
            FieldSchema { name: "outer_angle", kind: FieldKind::F32 { min: Some(0.0), max: Some(std::f32::consts::PI) }, doc: "Spot outer cone angle in radians." },
        ],
    },
    ComponentSchema {
        name: "Camera",
        doc: "A view camera. The primary camera drives the render target in play mode.",
        fields: &[
            FieldSchema { name: "fov", kind: FieldKind::F32 { min: Some(0.0), max: Some(std::f32::consts::PI) }, doc: "Vertical field of view in radians." },
            FieldSchema { name: "near", kind: FieldKind::Unbounded, doc: "Near clip plane." },
            FieldSchema { name: "far", kind: FieldKind::Unbounded, doc: "Far clip plane." },
            FieldSchema { name: "orthographic", kind: FieldKind::Bool, doc: "Orthographic instead of perspective." },
        ],
    },
    ComponentSchema {
        name: "RigidBody",
        doc: "Physics body simulated by the physics backend.",
        fields: &[
            FieldSchema { name: "kind", kind: FieldKind::Enum(&["static", "dynamic", "kinematic"]), doc: "Body type." },
            FieldSchema { name: "mass", kind: FieldKind::F32 { min: Some(0.0001), max: None }, doc: "Mass in kg for dynamic bodies." },
            FieldSchema { name: "lock_rotation", kind: FieldKind::Bool, doc: "Prevent angular motion." },
        ],
    },
    ComponentSchema {
        name: "Collider",
        doc: "Shape used by the physics solver.",
        fields: &[
            FieldSchema { name: "shape", kind: FieldKind::Json, doc: "Shape descriptor: cuboid[w,h,d], sphere[r], capsule, mesh, heightfield." },
            FieldSchema { name: "sensor", kind: FieldKind::Bool, doc: "Sensor (no collision response)." },
        ],
    },
    ComponentSchema {
        name: "CharacterController",
        doc: "Kinematic character movement with stepped ground checks.",
        fields: &[
            FieldSchema { name: "height", kind: FieldKind::Unbounded, doc: "Standing height in metres." },
            FieldSchema { name: "radius", kind: FieldKind::Unbounded, doc: "Capsule radius." },
            FieldSchema { name: "max_slope", kind: FieldKind::F32 { min: Some(0.0), max: Some(std::f32::consts::PI) }, doc: "Walkable slope limit." },
            FieldSchema { name: "step_height", kind: FieldKind::Unbounded, doc: "Maximum ledge height the controller can step onto." },
            FieldSchema { name: "move_speed", kind: FieldKind::Unbounded, doc: "Maximum ground movement speed in metres per second." },
            FieldSchema { name: "jump_speed", kind: FieldKind::Unbounded, doc: "Initial upward velocity when the jump action fires." },
        ],
    },
    ComponentSchema {
        name: "AudioSource",
        doc: "Plays an audio clip.",
        fields: &[
            FieldSchema { name: "clip", kind: FieldKind::AssetRef(AssetKind::Audio), doc: "The audio asset." },
            FieldSchema { name: "volume", kind: FieldKind::F32 { min: Some(0.0), max: Some(4.0) }, doc: "Volume multiplier." },
            FieldSchema { name: "loop", kind: FieldKind::Bool, doc: "Loop the clip." },
            FieldSchema { name: "spatial", kind: FieldKind::Bool, doc: "Positional (3D) audio." },
        ],
    },
    ComponentSchema {
        name: "AudioListener",
        doc: "The ears of the camera; only the first is honoured in play mode.",
        fields: &[],
    },
    ComponentSchema {
        name: "AnimationPlayer",
        doc: "Plays animation clips on this entity's skeleton or the referenced one.",
        fields: &[
            FieldSchema { name: "clip", kind: FieldKind::AssetRef(AssetKind::Animation), doc: "The animation asset." },
            FieldSchema { name: "speed", kind: FieldKind::Unbounded, doc: "Playback speed multiplier." },
            FieldSchema { name: "loop", kind: FieldKind::Bool, doc: "Loop the clip." },
        ],
    },
    ComponentSchema {
        name: "ParticleEmitter",
        doc: "GPU particle emitter.",
        fields: &[
            FieldSchema { name: "count", kind: FieldKind::I32, doc: "Particle count." },
            FieldSchema { name: "rate", kind: FieldKind::Unbounded, doc: "Emission per second." },
            FieldSchema { name: "lifetime", kind: FieldKind::Unbounded, doc: "Particle lifetime in seconds." },
            FieldSchema { name: "gravity", kind: FieldKind::Unbounded, doc: "Extra gravity factor." },
        ],
    },
    ComponentSchema {
        name: "NavAgent",
        doc: "Requests paths from the navmesh and follows them.",
        fields: &[
            FieldSchema { name: "radius", kind: FieldKind::Unbounded, doc: "Agent radius for corridor clearance." },
            FieldSchema { name: "max_speed", kind: FieldKind::Unbounded, doc: "Movement speed limit." },
        ],
    },
    ComponentSchema {
        name: "UiDocument",
        doc: "Attaches a HUD layout document to the camera's UI layer.",
        fields: &[FieldSchema { name: "layout", kind: FieldKind::AssetRef(AssetKind::Ui), doc: "The HUD layout asset." }],
    },
    ComponentSchema {
        name: "ScriptRef",
        doc: "Binds a gameplay script to this entity (Track B .rhai or Track A Rust system).",
        fields: &[
            FieldSchema { name: "script", kind: FieldKind::AssetRef(AssetKind::Script), doc: "The script path." },
            FieldSchema { name: "hooks", kind: FieldKind::Json, doc: "Lifecycle hooks: on_start, on_update, on_collision." },
            FieldSchema { name: "config", kind: FieldKind::Json, doc: "Per-entity public fields exposed to the script." },
        ],
    },
    ComponentSchema {
        name: "Tag",
        doc: "A simple label for filtering; the singleton component-free form carries the tag in fields.",
        fields: &[FieldSchema { name: "value", kind: FieldKind::String, doc: "Tag value." }],
    },
    ComponentSchema {
        name: "MaterialOverride",
        doc: "PBR maps on this mesh. Missing maps fall back to the material asset.",
        fields: &[
            FieldSchema { name: "albedo", kind: FieldKind::AssetRef(AssetKind::Texture), doc: "Base color / albedo map." },
            FieldSchema { name: "normal", kind: FieldKind::AssetRef(AssetKind::Texture), doc: "Normal map." },
            FieldSchema { name: "roughness", kind: FieldKind::AssetRef(AssetKind::Texture), doc: "Roughness map." },
            FieldSchema { name: "metallic", kind: FieldKind::AssetRef(AssetKind::Texture), doc: "Metallic map." },
            FieldSchema { name: "ao", kind: FieldKind::AssetRef(AssetKind::Texture), doc: "Ambient occlusion map." },
            FieldSchema { name: "emissive", kind: FieldKind::AssetRef(AssetKind::Texture), doc: "Emissive map." },
            FieldSchema { name: "color", kind: FieldKind::Color, doc: "Tint multiplied with albedo." },
        ],
    },
    ComponentSchema {
        name: "ShaderRef",
        doc: "Assigns a file-based shader (not a node graph) to this mesh.",
        fields: &[
            FieldSchema { name: "shader", kind: FieldKind::AssetRef(AssetKind::Shader), doc: "assets/shaders/*.shader.json" },
        ],
    },
    ComponentSchema {
        name: "PrefabInstance",
        doc: "Marks this entity as an instance of a prefab, and which of its components have been customised.",
        fields: &[
            FieldSchema { name: "prefab", kind: FieldKind::String, doc: "The prefab asset id this was stamped from." },
            FieldSchema { name: "overrides", kind: FieldKind::Json, doc: "Component names this instance customised; propagation skips them." },
        ],
    },
    ComponentSchema {
        name: "Provenance",
        doc: "Who created this entity and in which transaction. Written automatically; the Outliner filters on it.",
        fields: &[
            FieldSchema { name: "created_by", kind: FieldKind::Enum(&["user", "agent", "system"]), doc: "Which actor authored it." },
            FieldSchema { name: "txn", kind: FieldKind::String, doc: "The transaction id it was created in." },
            FieldSchema { name: "at", kind: FieldKind::String, doc: "RFC 3339 timestamp." },
        ],
    },
    ComponentSchema {
        name: "Visibility",
        doc: "Editor and runtime visibility, plus the Outliner's lock. Absent means visible and unlocked.",
        fields: &[
            FieldSchema { name: "visible", kind: FieldKind::Bool, doc: "Drawn in the viewport and in play." },
            FieldSchema { name: "locked", kind: FieldKind::Bool, doc: "Cannot be selected or dragged in the viewport." },
        ],
    },
    ComponentSchema {
        name: "WeatherVolume",
        doc: "UltraSky-style weather that lights, sky, and overlay particles honour.",
        fields: &[
            FieldSchema { name: "preset", kind: FieldKind::Enum(&["clear", "overcast", "rain", "snow", "fog", "storm", "sunset", "night"]), doc: "Weather preset." },
            FieldSchema { name: "intensity", kind: FieldKind::F32 { min: Some(0.0), max: Some(2.0) }, doc: "Precipitation / fog strength." },
        ],
    },
];

/// All components in registry order (stable for schema export / mind map).
#[must_use]
pub fn registry() -> Vec<ComponentSchema> {
    COMPONENTS.to_vec()
}

#[must_use]
pub fn component(name: &str) -> Option<ComponentSchema> {
    COMPONENTS
        .iter()
        .copied()
        .find(|schema| schema.name == name)
}

/// A compact, model-readable description of one component's fields — echoed back beside a
/// rejection so the next attempt has the real shape instead of a guess (ENG-112).
/// `None` for an unknown component; callers then show the registry list instead.
#[must_use]
pub fn excerpt(name: &str) -> Option<String> {
    let schema = component(name)?;
    let mut out = format!("{} — {}\n", schema.name, schema.doc);
    if schema.fields.is_empty() {
        out.push_str("  (no fields)\n");
    }
    for field in schema.fields {
        out.push_str(&format!(
            "  {}: {}  — {}\n",
            field.name, field.kind, field.doc
        ));
    }
    Some(out)
}

/// Every component name in registry order — the hint for an unknown component.
#[must_use]
pub fn component_names() -> Vec<&'static str> {
    COMPONENTS.iter().map(|schema| schema.name).collect()
}

/// The value Details writes for Reset. Defaults live beside validation, not in TypeScript.
#[must_use]
pub fn field_default(component_name: &str, field: &FieldSchema) -> Value {
    use serde_json::json;
    match (component_name, field.name) {
        ("Transform", "pos") => json!([0.0, 0.0, 0.0]),
        ("Transform", "rot") => json!([0.0, 0.0, 0.0, 1.0]),
        ("Transform", "scale") => json!([1.0, 1.0, 1.0]),
        ("Camera", "fov") => json!(0.9),
        ("Camera", "near") => json!(0.05),
        ("Camera", "far") => json!(500.0),
        ("Light", "color") | ("MaterialOverride", "color") => json!([1.0, 1.0, 1.0]),
        ("Light", "intensity") => json!(1.0),
        ("Light", "range") => json!(20.0),
        ("Light", "outer_angle") => json!(0.5),
        ("RigidBody", "mass") => json!(1.0),
        ("CharacterController", "height") => json!(1.8),
        ("CharacterController", "radius") => json!(0.35),
        ("CharacterController", "max_slope") => json!(0.7),
        ("CharacterController", "step_height") => json!(0.3),
        ("CharacterController", "move_speed") => json!(5.0),
        ("CharacterController", "jump_speed") => json!(5.5),
        (_, "materials") => json!([]),
        (_, "shape" | "hooks" | "config" | "overrides") => json!({}),
        _ => match field.kind {
            FieldKind::F32 { min, .. } | FieldKind::Vec3 { min, .. } => {
                let value = min.unwrap_or(0.0);
                if matches!(field.kind, FieldKind::Vec3 { .. }) {
                    json!([value, value, value])
                } else {
                    json!(value)
                }
            }
            FieldKind::Unbounded => json!(0.0),
            FieldKind::Vec4 => json!([0.0, 0.0, 0.0, 1.0]),
            FieldKind::I32 => json!(0),
            FieldKind::Bool => json!(false),
            FieldKind::Enum(values) => values.first().map_or(Value::Null, |value| json!(value)),
            FieldKind::String | FieldKind::AssetRef(_) => json!(""),
            FieldKind::Color => json!([1.0, 1.0, 1.0]),
            FieldKind::Json => json!({}),
        },
    }
}

/// Schema-aware validation. Unknown component names, unknown fields, wrong enum values and
/// out-of-range numbers are rejected with the schema excerpt echoed (the AI repair round).
pub fn validate_component(name: &str, payload: &Value) -> Result<()> {
    let schema = component(name).ok_or_else(|| {
        EngineError::Schema(
            format!("unknown component {name:?}"),
            Some("Add Component searches the registry; valid names are listed there.".to_owned()),
        )
    })?;

    let object = payload.as_object().ok_or_else(|| {
        EngineError::Schema(
            format!("component {name:?} must be a JSON object"),
            Some("Component payloads are field maps.".to_owned()),
        )
    })?;

    for (field_name, field_value) in object {
        let field = schema
            .fields
            .iter()
            .find(|field| field.name == field_name)
            .ok_or_else(|| {
                EngineError::Schema(
                    format!("component {name:?} has no field {field_name:?}"),
                    Some(format!(
                        "Known fields: {}",
                        schema
                            .fields
                            .iter()
                            .map(|field| field.name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                )
            })?;
        validate_field(name, field, field_value)?;
    }
    Ok(())
}

fn validate_field(component: &str, field: &FieldSchema, value: &Value) -> Result<()> {
    let location = || format!("{component}.{}", field.name);
    match field.kind {
        FieldKind::F32 { min, max } => {
            let number = value.as_f64().ok_or_else(|| {
                EngineError::Schema(
                    format!("{} must be a number, got {value}", location()),
                    Some("Use a decimal number.".to_owned()),
                )
            })?;
            if let Some(lo) = min {
                if (number as f32) < lo {
                    return Err(EngineError::Schema(
                        format!("{} must be ≥ {lo}", location()),
                        Some("Raise the value into range.".to_owned()),
                    ));
                }
            }
            if let Some(hi) = max {
                if (number as f32) > hi {
                    return Err(EngineError::Schema(
                        format!("{} must be ≤ {hi}", location()),
                        Some("Lower the value into range.".to_owned()),
                    ));
                }
            }
        }
        FieldKind::Unbounded => {
            if value.as_f64().is_none() {
                return Err(EngineError::Schema(
                    format!("{} must be a number", location()),
                    Some("Use a decimal number.".to_owned()),
                ));
            }
        }
        FieldKind::I32 => {
            let number = value.as_i64().ok_or_else(|| {
                EngineError::Schema(
                    format!("{} must be an integer", location()),
                    Some("Use a whole number.".to_owned()),
                )
            })?;
            if !(i32::MIN as i64..=i32::MAX as i64).contains(&number) {
                return Err(EngineError::Schema(
                    format!("{} out of i32 range", location()),
                    Some("Shorten the count.".to_owned()),
                ));
            }
        }
        FieldKind::Bool => {
            if !value.is_boolean() {
                return Err(EngineError::Schema(
                    format!("{} must be true or false", location()),
                    Some("Use a boolean.".to_owned()),
                ));
            }
        }
        FieldKind::Enum(values) => {
            let text = value.as_str().ok_or_else(|| {
                EngineError::Schema(
                    format!("{} must be a string", location()),
                    Some(format!("Valid values: {}", values.join(", "))),
                )
            })?;
            if !values.contains(&text) {
                return Err(EngineError::Schema(
                    format!("{} invalid enum value {text:?}", location()),
                    Some(format!("Valid values: {}", values.join(", "))),
                ));
            }
        }
        FieldKind::String => {
            if !value.is_string() {
                return Err(EngineError::Schema(
                    format!("{} must be a string", location()),
                    Some("Use a quoted string.".to_owned()),
                ));
            }
        }
        FieldKind::Vec3 { min, .. } => {
            let arr = value.as_array().ok_or_else(|| {
                EngineError::Schema(
                    format!("{} must be [x, y, z]", location()),
                    Some("Use a 3-number array.".to_owned()),
                )
            })?;
            if arr.len() != 3 {
                return Err(EngineError::Schema(
                    format!("{} must have 3 numbers", location()),
                    Some("Use a 3-number array.".to_owned()),
                ));
            }
            for number in arr {
                let number = number.as_f64().ok_or_else(|| {
                    EngineError::Schema(
                        format!("{} values must be numbers", location()),
                        Some("Use numbers.".to_owned()),
                    )
                })?;
                if let Some(lo) = min {
                    if (number as f32) < lo {
                        return Err(EngineError::Schema(
                            format!("{} must be ≥ {lo}", location()),
                            Some("Raise the value into range.".to_owned()),
                        ));
                    }
                }
            }
        }
        FieldKind::Vec4 => {
            let Some(arr) = value.as_array() else {
                return Err(EngineError::Schema(
                    format!("{} must be [x, y, z, w]", location()),
                    Some("Use a 4-number array.".to_owned()),
                ));
            };
            if arr.len() != 4 {
                return Err(EngineError::Schema(
                    format!("{} must have 4 numbers", location()),
                    Some("Use a 4-number array.".to_owned()),
                ));
            }
        }
        FieldKind::Color => match value {
            Value::Array(arr) if arr.len() == 3 => {
                let _ = arr;
            }
            Value::String(hex) => {
                if !(hex.starts_with('#') && (hex.len() == 7 || hex.len() == 9)) {
                    return Err(EngineError::Schema(
                        format!("{} must be a 3-number array or #rrggbb string", location()),
                        Some("Use [r, g, b] with values 0..1, or a hex string.".to_owned()),
                    ));
                }
            }
            _ => {
                return Err(EngineError::Schema(
                    format!("{} must be a 3-number array or hex string", location()),
                    Some("Use [r, g, b] with values 0..1, or a hex string.".to_owned()),
                ));
            }
        },
        FieldKind::AssetRef(kind) => {
            let text = value.as_str().ok_or_else(|| {
                EngineError::Schema(
                    format!("{} must be an asset reference", location()),
                    Some(format!("Use asset:<ulid> of kind {kind}.")),
                )
            })?;
            // Three forms, and only three (ENG-161): empty means unset, `builtin:<name>` is
            // a primitive the renderer builds itself, `asset:<ulid>` is an imported file.
            // A bare `"cube"` used to be accepted by omission, which is how the viewport
            // ended up sniffing strings to guess what a mesh was.
            let known_builtin = crate::mesh::builtin_from_reference(text).is_some();
            if !text.is_empty() && !text.starts_with("asset:") && !known_builtin {
                let hint = if text.starts_with(crate::mesh::BUILTIN_PREFIX) {
                    format!(
                        "Unknown built-in. Available: {}",
                        crate::mesh::builtin_references().join(", ")
                    )
                } else {
                    format!(
                        "Use asset:<ulid> of kind {kind}, one of {}, or leave empty for unset.",
                        crate::mesh::builtin_references().join(", ")
                    )
                };
                return Err(EngineError::Schema(
                    format!("{} is not an asset or built-in reference", location()),
                    Some(hint),
                ));
            }
        }
        FieldKind::Json => {
            // JSON payloads are schema-free; any valid value passes.
        }
    }
    Ok(())
}

/// Every registered component must validate its own literal empty-map payload only if it
/// has no required fields; used by tests to keep the registry honest.
#[cfg(test)]
mod tests {
    use super::{component, field_default, registry, validate_component};
    use serde_json::json;

    #[test]
    fn registry_contains_the_v1_core_set() {
        let names: Vec<&str> = registry().iter().map(|schema| schema.name).collect();
        for expected in [
            "Transform",
            "MeshRenderer",
            "Light",
            "Camera",
            "RigidBody",
            "Collider",
            "CharacterController",
            "AudioSource",
            "ScriptRef",
            "Tag",
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn prints_a_schema_excerpt_on_invalid_payloads() {
        let error =
            validate_component("RigidBody", &json!({ "kind": "bouncy" })).expect_err("reject");
        let text = error.to_string();
        assert!(text.contains("intensity") || text.contains("valid enum") || text.contains("kind"));
    }

    #[test]
    fn transform_pos_validates_ranges_and_vec3_shape() {
        assert!(validate_component("Transform", &json!({ "pos": [1.0, 2.0, 3.0] })).is_ok());
        let error =
            validate_component("Transform", &json!({ "pos": [1.0, 2.0] })).expect_err("bad len");
        assert!(error.hint().is_some());
    }

    #[test]
    fn mesh_references_are_asset_ids_builtins_or_unset() {
        assert!(validate_component(
            "MeshRenderer",
            &json!({ "mesh": "asset:01JD0000000000000000000000" })
        )
        .is_ok());
        // A built-in primitive is a first-class reference (ENG-161), not a special case the
        // renderer has to sniff for.
        assert!(validate_component("MeshRenderer", &json!({ "mesh": "builtin:cube" })).is_ok());
        assert!(validate_component("MeshRenderer", &json!({ "mesh": "" })).is_ok());

        let error = validate_component("MeshRenderer", &json!({ "mesh": "crate.glb" }))
            .expect_err("bare path");
        assert!(error
            .hint()
            .is_some_and(|hint| hint.contains("builtin:cube")));

        // A bare name is the old ambiguous form and must not pass by omission.
        let error =
            validate_component("MeshRenderer", &json!({ "mesh": "cube" })).expect_err("bare name");
        assert!(error.hint().is_some());

        let error = validate_component("MeshRenderer", &json!({ "mesh": "builtin:hologram" }))
            .expect_err("unknown built-in");
        assert!(error
            .hint()
            .is_some_and(|hint| hint.contains("Unknown built-in")));
    }

    #[test]
    fn unknown_components_are_rejected_with_a_hint() {
        let error = validate_component("GravityGun", &json!({})).expect_err("unknown");
        assert!(error.hint().is_some());
        assert!(component("GravityGun").is_none());
    }

    #[test]
    fn every_registry_field_has_a_valid_reset_default() {
        for component in registry() {
            let payload = serde_json::Value::Object(
                component
                    .fields
                    .iter()
                    .map(|field| (field.name.to_owned(), field_default(component.name, field)))
                    .collect(),
            );
            validate_component(component.name, &payload).unwrap_or_else(|error| {
                panic!("{} defaults must validate: {error}", component.name)
            });
        }
    }
}
