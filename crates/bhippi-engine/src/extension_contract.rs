//! Nested-prefab evolution and plugin/capability-pack contracts (Phase 22).
//!
//! This module validates documents and staged lifecycle plans. It does not propagate nested
//! instances, migrate files, install plugins, load SDK code or create editor panels/backends.

use crate::error::{EngineError, Result};
use crate::prefab::PrefabDocument;
use crate::registry::{CapabilityEntry, CapabilityRegistry, ExtensionManifest};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const PREFAB_EVOLUTION_FORMAT: &str = "bhippi-prefab-evolution@1";
pub const PLUGIN_FORMAT: &str = "bhippi-plugin@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExposedParameterType {
    Bool,
    Number,
    String,
    Vec3,
    Asset,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ExposedParameterContract {
    pub id: String,
    pub value_type: ExposedParameterType,
    pub default: serde_json::Value,
    pub target_node: String,
    pub component: String,
    pub property_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct NestedPrefabContract {
    pub mount_node: String,
    pub prefab_id: String,
    pub required_version: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PrefabOverrideContract {
    pub target_node: String,
    pub component: String,
    pub property_path: String,
    pub expected_base_hash: String,
    pub value: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PrefabVariantContract {
    pub id: String,
    pub parent_variant: Option<String>,
    pub parameter_values: BTreeMap<String, serde_json::Value>,
    pub overrides: Vec<PrefabOverrideContract>,
    pub replicated: bool,
    pub authority: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum PrefabMigrationOperation {
    RenameNode { from: String, to: String },
    RenameParameter { from: String, to: String },
    ReplaceNestedPrefab { from: String, to: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PrefabMigrationContract {
    pub from_version: String,
    pub to_version: String,
    pub operations: Vec<PrefabMigrationOperation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PrefabEvolutionContract {
    pub format: String,
    pub prefab_id: String,
    pub version: String,
    pub nested: Vec<NestedPrefabContract>,
    pub exposed_parameters: Vec<ExposedParameterContract>,
    pub variants: Vec<PrefabVariantContract>,
    pub migrations: Vec<PrefabMigrationContract>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PrefabConflict {
    pub variant: String,
    pub target: String,
    pub expected_base_hash: String,
    pub actual_base_hash: String,
}

impl PrefabEvolutionContract {
    pub fn validate(
        &self,
        prefab: &PrefabDocument,
        catalogue: &BTreeMap<String, String>,
    ) -> Result<()> {
        prefab.validate()?;
        if self.format != PREFAB_EVOLUTION_FORMAT || self.prefab_id != prefab.id.to_string() {
            return Err(error(
                "prefab evolution format/id does not match its prefab",
                "Use bhippi-prefab-evolution@1 and the source prefab id.",
            ));
        }
        validate_version(&self.version)?;
        let nodes = prefab
            .nodes
            .iter()
            .map(|node| node.local_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut nested_ids = BTreeSet::new();
        for nested in &self.nested {
            if !nodes.contains(nested.mount_node.as_str())
                || nested.prefab_id == self.prefab_id
                || !catalogue.contains_key(&nested.prefab_id)
                || !nested_ids.insert(nested.prefab_id.as_str())
            {
                return Err(error(
                    "nested prefab is cyclic, duplicate or dangling",
                    "Mount a different declared prefab on a real local node.",
                ));
            }
            validate_version(&nested.required_version)?;
        }
        let mut parameters = BTreeMap::new();
        for parameter in &self.exposed_parameters {
            validate_id(&parameter.id)?;
            if parameters
                .insert(parameter.id.as_str(), parameter)
                .is_some()
                || !nodes.contains(parameter.target_node.as_str())
                || parameter.component.trim().is_empty()
                || parameter.property_path.trim().is_empty()
                || !value_matches(parameter.value_type, &parameter.default)
            {
                return Err(error(
                    "exposed prefab parameter is duplicate, dangling or mistyped",
                    "Bind a typed default to a real node/component/property.",
                ));
            }
        }
        let variants = self
            .variants
            .iter()
            .map(|variant| (variant.id.as_str(), variant))
            .collect::<BTreeMap<_, _>>();
        if variants.len() != self.variants.len() {
            return Err(error(
                "duplicate prefab variant id",
                "Use one stable variant id.",
            ));
        }
        for variant in &self.variants {
            validate_id(&variant.id)?;
            if variant
                .parent_variant
                .as_ref()
                .is_some_and(|parent| !variants.contains_key(parent.as_str()))
            {
                return Err(error(
                    "variant parent is missing",
                    "Choose a declared parent variant.",
                ));
            }
            if variant.replicated && variant.authority.as_deref().is_none_or(str::is_empty) {
                return Err(error(
                    "replicated prefab variant lacks authority metadata",
                    "Declare the authority model explicitly.",
                ));
            }
            for (parameter, value) in &variant.parameter_values {
                let Some(schema) = parameters.get(parameter.as_str()) else {
                    return Err(error(
                        "variant sets an unknown exposed parameter",
                        "Choose a declared parameter.",
                    ));
                };
                if !value_matches(schema.value_type, value) {
                    return Err(error(
                        "variant parameter has the wrong type",
                        "Match the exposed parameter type.",
                    ));
                }
            }
            for item in &variant.overrides {
                if !nodes.contains(item.target_node.as_str())
                    || item.component.trim().is_empty()
                    || item.property_path.trim().is_empty()
                    || item.expected_base_hash.len() != 64
                {
                    return Err(error(
                        "variant override is dangling or lacks a base hash",
                        "Target a real node/property and record a 64-character base hash.",
                    ));
                }
            }
            reject_variant_cycle(variant.id.as_str(), &variants)?;
        }
        validate_migrations(&self.migrations, &self.version)
    }

    #[must_use]
    pub fn conflicts(&self, actual_base_hashes: &BTreeMap<String, String>) -> Vec<PrefabConflict> {
        let mut conflicts = Vec::new();
        for variant in &self.variants {
            for item in &variant.overrides {
                let target = format!(
                    "{}:{}:{}",
                    item.target_node, item.component, item.property_path
                );
                if let Some(actual) = actual_base_hashes.get(&target) {
                    if actual != &item.expected_base_hash {
                        conflicts.push(PrefabConflict {
                            variant: variant.id.clone(),
                            target,
                            expected_base_hash: item.expected_base_hash.clone(),
                            actual_base_hash: actual.clone(),
                        });
                    }
                }
            }
        }
        conflicts.sort_by(|left, right| {
            left.variant
                .cmp(&right.variant)
                .then_with(|| left.target.cmp(&right.target))
        });
        conflicts
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PluginExposure {
    Component,
    Importer,
    EditorPanel,
    RenderFeature,
    PhysicsFeature,
    CapabilityPack,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PluginDependencyContract {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CapabilityPackContract {
    pub entries: Vec<CapabilityEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(deny_unknown_fields)]
pub struct PluginManifestContract {
    pub format: String,
    pub id: String,
    pub version: String,
    pub extension: ExtensionManifest,
    pub dependencies: Vec<PluginDependencyContract>,
    pub exposures: Vec<PluginExposure>,
    pub pack: Option<CapabilityPackContract>,
}

impl PluginManifestContract {
    pub fn validate(
        &self,
        registry: &CapabilityRegistry,
        installed_plugins: &BTreeMap<String, String>,
    ) -> Result<Option<CapabilityRegistry>> {
        if self.format != PLUGIN_FORMAT || self.id != self.extension.id {
            return Err(error(
                "plugin format/id does not match its extension",
                "Use bhippi-plugin@1 and one stable plugin id.",
            ));
        }
        validate_id(&self.id)?;
        validate_version(&self.version)?;
        self.extension.validate(registry)?;
        let exposures = self.exposures.iter().copied().collect::<BTreeSet<_>>();
        if exposures.len() != self.exposures.len() {
            return Err(error(
                "plugin repeats an exposure",
                "List each exposure once.",
            ));
        }
        for dependency in &self.dependencies {
            validate_id(&dependency.id)?;
            validate_version(&dependency.version)?;
            if installed_plugins.get(&dependency.id) != Some(&dependency.version) {
                return Err(error(
                    "plugin dependency is missing or version-mismatched",
                    "Install the exact declared dependency first.",
                ));
            }
        }
        match (
            &self.pack,
            exposures.contains(&PluginExposure::CapabilityPack),
        ) {
            (Some(_), false) | (None, true) => {
                return Err(error(
                    "plugin capability-pack exposure and payload disagree",
                    "Declare both the exposure and its validated pack, or neither.",
                ));
            }
            _ => {}
        }
        let Some(pack) = &self.pack else {
            return Ok(None);
        };
        if pack.entries.iter().any(|entry| {
            entry.licence.trim().is_empty()
                || entry.provenance.trim().is_empty()
                || entry.owner != self.id
        }) {
            return Err(error(
                "capability pack entry lacks plugin ownership/licence/provenance",
                "Bind every entry to this plugin and declare its origin/licence.",
            ));
        }
        let mut entries = registry.entries.clone();
        entries.extend(pack.entries.clone());
        CapabilityRegistry::build(entries).map(Some)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PluginLifecycleState {
    Staged,
    Validated,
    Installed,
    Active,
    Disabled,
    Removing,
    Removed,
    Faulted,
}

impl PluginLifecycleState {
    #[must_use]
    pub const fn allows(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Staged,
                Self::Validated | Self::Removed | Self::Faulted
            ) | (
                Self::Validated,
                Self::Installed | Self::Removed | Self::Faulted
            ) | (
                Self::Installed,
                Self::Active | Self::Disabled | Self::Removing | Self::Faulted
            ) | (Self::Active, Self::Disabled | Self::Faulted)
                | (
                    Self::Disabled,
                    Self::Active | Self::Removing | Self::Faulted
                )
                | (Self::Removing, Self::Removed | Self::Faulted)
                | (Self::Faulted, Self::Disabled | Self::Removing)
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PluginRecoveryContract {
    pub staged_manifest_hash: String,
    pub previous_manifest_hash: Option<String>,
    pub rollback_required_on_fault: bool,
    pub preserve_diagnostic: bool,
}

impl PluginRecoveryContract {
    pub fn validate(&self) -> Result<()> {
        if self.staged_manifest_hash.len() != 64
            || self
                .previous_manifest_hash
                .as_ref()
                .is_some_and(|hash| hash.len() != 64)
            || !self.rollback_required_on_fault
            || !self.preserve_diagnostic
        {
            return Err(error(
                "plugin recovery contract cannot restore safely",
                "Record 64-character hashes, mandatory rollback and diagnostics.",
            ));
        }
        Ok(())
    }
}

fn reject_variant_cycle<'a>(
    start: &'a str,
    variants: &BTreeMap<&'a str, &'a PrefabVariantContract>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut cursor = Some(start);
    while let Some(id) = cursor {
        if !seen.insert(id) {
            return Err(error(
                "prefab variant inheritance contains a cycle",
                "Break the parent-variant cycle.",
            ));
        }
        cursor = variants
            .get(id)
            .and_then(|variant| variant.parent_variant.as_deref());
    }
    Ok(())
}

fn validate_migrations(migrations: &[PrefabMigrationContract], current: &str) -> Result<()> {
    let mut from_versions = BTreeSet::new();
    for migration in migrations {
        validate_version(&migration.from_version)?;
        validate_version(&migration.to_version)?;
        if migration.from_version == migration.to_version
            || migration.to_version != current
            || migration.operations.is_empty()
            || !from_versions.insert(migration.from_version.as_str())
        {
            return Err(error(
                "prefab migration is duplicate, empty or not targeted at current version",
                "Declare one non-empty path from each old version to the current version.",
            ));
        }
    }
    Ok(())
}

fn value_matches(kind: ExposedParameterType, value: &serde_json::Value) -> bool {
    match kind {
        ExposedParameterType::Bool => value.is_boolean(),
        ExposedParameterType::Number => value.as_f64().is_some_and(f64::is_finite),
        ExposedParameterType::String | ExposedParameterType::Asset => value.is_string(),
        ExposedParameterType::Vec3 => value.as_array().is_some_and(|items| {
            items.len() == 3
                && items
                    .iter()
                    .all(|item| item.as_f64().is_some_and(f64::is_finite))
        }),
    }
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
            &format!("`{id}` is not a canonical extension id"),
            "Use lowercase dotted segments.",
        )
    })
}

fn validate_version(version: &str) -> Result<()> {
    let parts = version.split('.').collect::<Vec<_>>();
    (parts.len() == 3 && parts.iter().all(|part| part.parse::<u32>().is_ok()))
        .then_some(())
        .ok_or_else(|| error("extension version is not numeric semver", "Use x.y.z."))
}

fn error(message: &str, hint: &str) -> EngineError {
    EngineError::Schema(message.to_owned(), Some(hint.to_owned()))
}
