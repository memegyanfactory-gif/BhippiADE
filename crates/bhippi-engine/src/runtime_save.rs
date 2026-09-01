//! Versioned runtime save/checkpoint truth for Phase 23.
//!
//! This is deliberately a domain contract, not persistence I/O. The application layer must use
//! atomic replacement and recovery around these validated bytes; providers receive the opaque
//! encrypted/encoded payload only through a separately approved extension.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const RUNTIME_SAVE_FORMAT: &str = "bhippi-runtime-save@1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RuntimeSave {
    pub format: String,
    pub game_id: String,
    pub build_id: String,
    pub save_id: String,
    pub tick: u64,
    pub seed: u64,
    pub active_level: String,
    pub entities: Vec<PersistedEntity>,
    pub globals: BTreeMap<String, PersistedValue>,
    pub checkpoint_hash: String,
    pub parent_checkpoint_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PersistedEntity {
    pub stable_id: String,
    pub source_scene: String,
    pub source_prefab: Option<String>,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub state: BTreeMap<String, PersistedValue>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PersistedValue {
    Bool(bool),
    Integer(i64),
    Number(f64),
    Text(String),
    Entity(String),
    List(Vec<PersistedValue>),
    Record(BTreeMap<String, PersistedValue>),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RuntimeSaveLimits {
    pub encoded_bytes: usize,
    pub entities: usize,
    pub values: usize,
    pub nesting_depth: usize,
    pub text_bytes: usize,
}

impl Default for RuntimeSaveLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: 16 * 1_024 * 1_024,
            entities: 100_000,
            values: 1_000_000,
            nesting_depth: 16,
            text_bytes: 1_048_576,
        }
    }
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum RuntimeSaveError {
    #[error("unsupported runtime-save format `{0}`")]
    UnsupportedFormat(String),
    #[error("runtime save field `{field}` cannot be empty")]
    EmptyIdentity { field: &'static str },
    #[error("runtime save contains duplicate stable entity id `{0}`")]
    DuplicateEntity(String),
    #[error("runtime save exceeds `{resource}` limit ({actual} > {limit})")]
    Limit {
        resource: &'static str,
        actual: usize,
        limit: usize,
    },
    #[error("runtime save contains a non-finite number at `{0}`")]
    NonFinite(String),
    #[error("runtime save checkpoint hash does not match its canonical payload")]
    HashMismatch,
    #[error("runtime save could not be encoded: {0}")]
    Encoding(String),
}

#[derive(Default)]
struct ValueMeasure {
    values: usize,
    text_bytes: usize,
}

impl RuntimeSave {
    pub fn validate(&self, limits: &RuntimeSaveLimits) -> Result<(), RuntimeSaveError> {
        if self.format != RUNTIME_SAVE_FORMAT {
            return Err(RuntimeSaveError::UnsupportedFormat(self.format.clone()));
        }
        for (field, value) in [
            ("game_id", self.game_id.as_str()),
            ("build_id", self.build_id.as_str()),
            ("save_id", self.save_id.as_str()),
            ("active_level", self.active_level.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(RuntimeSaveError::EmptyIdentity { field });
            }
        }
        if self.entities.len() > limits.entities {
            return Err(RuntimeSaveError::Limit {
                resource: "entities",
                actual: self.entities.len(),
                limit: limits.entities,
            });
        }

        let mut ids = BTreeSet::new();
        let mut measure = ValueMeasure::default();
        for entity in &self.entities {
            if entity.stable_id.trim().is_empty() {
                return Err(RuntimeSaveError::EmptyIdentity { field: "stable_id" });
            }
            if !ids.insert(entity.stable_id.clone()) {
                return Err(RuntimeSaveError::DuplicateEntity(entity.stable_id.clone()));
            }
            validate_vector(
                &entity.position,
                &format!("entity:{}:position", entity.stable_id),
            )?;
            validate_vector(
                &entity.rotation,
                &format!("entity:{}:rotation", entity.stable_id),
            )?;
            validate_vector(&entity.scale, &format!("entity:{}:scale", entity.stable_id))?;
            measure_map(&entity.state, 1, limits, &mut measure)?;
        }
        measure_map(&self.globals, 1, limits, &mut measure)?;

        let bytes = serde_json::to_vec(self)
            .map_err(|error| RuntimeSaveError::Encoding(error.to_string()))?;
        if bytes.len() > limits.encoded_bytes {
            return Err(RuntimeSaveError::Limit {
                resource: "encoded_bytes",
                actual: bytes.len(),
                limit: limits.encoded_bytes,
            });
        }
        if self.checkpoint_hash != self.canonical_checkpoint_hash()? {
            return Err(RuntimeSaveError::HashMismatch);
        }
        Ok(())
    }

    pub fn canonical_checkpoint_hash(&self) -> Result<String, RuntimeSaveError> {
        let mut canonical = self.clone();
        canonical.checkpoint_hash.clear();
        canonical
            .entities
            .sort_by(|left, right| left.stable_id.cmp(&right.stable_id));
        let bytes = serde_json::to_vec(&canonical)
            .map_err(|error| RuntimeSaveError::Encoding(error.to_string()))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    pub fn seal(mut self) -> Result<Self, RuntimeSaveError> {
        self.checkpoint_hash = self.canonical_checkpoint_hash()?;
        Ok(self)
    }
}

fn validate_vector(vector: &[f32; 3], path: &str) -> Result<(), RuntimeSaveError> {
    if vector.iter().any(|value| !value.is_finite()) {
        return Err(RuntimeSaveError::NonFinite(path.to_owned()));
    }
    Ok(())
}

fn measure_map(
    map: &BTreeMap<String, PersistedValue>,
    depth: usize,
    limits: &RuntimeSaveLimits,
    measure: &mut ValueMeasure,
) -> Result<(), RuntimeSaveError> {
    for (key, value) in map {
        measure.text_bytes = measure.text_bytes.saturating_add(key.len());
        measure_value(value, depth, limits, measure)?;
    }
    Ok(())
}

fn measure_value(
    value: &PersistedValue,
    depth: usize,
    limits: &RuntimeSaveLimits,
    measure: &mut ValueMeasure,
) -> Result<(), RuntimeSaveError> {
    if depth > limits.nesting_depth {
        return Err(RuntimeSaveError::Limit {
            resource: "nesting_depth",
            actual: depth,
            limit: limits.nesting_depth,
        });
    }
    measure.values = measure.values.saturating_add(1);
    if measure.values > limits.values {
        return Err(RuntimeSaveError::Limit {
            resource: "values",
            actual: measure.values,
            limit: limits.values,
        });
    }
    match value {
        PersistedValue::Number(number) if !number.is_finite() => {
            return Err(RuntimeSaveError::NonFinite("persisted_value".to_owned()));
        }
        PersistedValue::Text(text) | PersistedValue::Entity(text) => {
            measure.text_bytes = measure.text_bytes.saturating_add(text.len());
        }
        PersistedValue::List(values) => {
            for item in values {
                measure_value(item, depth + 1, limits, measure)?;
            }
        }
        PersistedValue::Record(values) => {
            measure_map(values, depth + 1, limits, measure)?;
        }
        PersistedValue::Bool(_) | PersistedValue::Integer(_) | PersistedValue::Number(_) => {}
    }
    if measure.text_bytes > limits.text_bytes {
        return Err(RuntimeSaveError::Limit {
            resource: "text_bytes",
            actual: measure.text_bytes,
            limit: limits.text_bytes,
        });
    }
    Ok(())
}
