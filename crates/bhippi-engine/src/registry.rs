//! Versioned engine capability discovery (ADR-0035).
//!
//! This is separate from [`crate::capability`]: that module controls whether an agent may
//! act, while this module describes what the engine has. Core entries are projected from
//! their Rust owners rather than copied into a second catalogue.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const REGISTRY_FORMAT: &str = "bhippi-capability@1";
pub const ENTRY_VERSION: &str = "1.0.0";
pub const DEFAULT_SEARCH_LIMIT: usize = 8;
pub const MAX_SEARCH_LIMIT: usize = 20;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Component,
    Preset,
    HudWidget,
    Weather,
    ScriptHost,
    BuildTarget,
    Extension,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CostClass {
    Trivial,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Requires,
    Conflicts,
    ComposesWith,
    Supersedes,
    Provides,
    Consumes,
    TestWith,
    EditorFor,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
pub struct CapabilityRelation {
    pub kind: RelationKind,
    pub target: String,
}

/// The seven truth dimensions are independent; `available` is never a maturity shortcut.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilityMaturity {
    pub documented: bool,
    pub implemented: bool,
    pub tested: bool,
    pub editor_accessible: bool,
    pub ai_accessible: bool,
    pub runtime_proven: bool,
    pub production_ready: bool,
    #[serde(default)]
    pub proven_platforms: Vec<String>,
    #[serde(default)]
    pub budget_evidence: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct MaturityRequirement {
    pub documented: bool,
    pub implemented: bool,
    pub tested: bool,
    pub editor_accessible: bool,
    pub ai_accessible: bool,
    pub runtime_proven: bool,
    pub production_ready: bool,
}

impl CapabilityMaturity {
    #[must_use]
    pub fn satisfies(&self, needed: &MaturityRequirement) -> bool {
        (!needed.documented || self.documented)
            && (!needed.implemented || self.implemented)
            && (!needed.tested || self.tested)
            && (!needed.editor_accessible || self.editor_accessible)
            && (!needed.ai_accessible || self.ai_accessible)
            && (!needed.runtime_proven || self.runtime_proven)
            && (!needed.production_ready || self.production_ready)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ContractField {
    pub name: String,
    pub type_name: String,
    pub required: bool,
    pub description: String,
}

/// Deep contract returned only after a card has been selected.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilityEntry {
    pub id: String,
    pub name: String,
    pub kind: CapabilityKind,
    pub category: String,
    pub version: String,
    pub purpose: String,
    pub owner: String,
    #[serde(default)]
    pub inputs: Vec<ContractField>,
    #[serde(default)]
    pub outputs: Vec<ContractField>,
    #[serde(default)]
    pub properties: Vec<ContractField>,
    #[serde(default)]
    pub operations: Vec<String>,
    #[serde(default)]
    pub relations: Vec<CapabilityRelation>,
    #[serde(default)]
    pub runtime_requirements: Vec<String>,
    pub cost: CostClass,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub editor_route: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub compatible_components: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub limitations: Vec<String>,
    #[serde(default)]
    pub extension_points: Vec<String>,
    #[serde(default)]
    pub verification: Vec<String>,
    #[serde(default)]
    pub validators: Vec<String>,
    #[serde(default)]
    pub debuggers: Vec<String>,
    pub maturity: CapabilityMaturity,
    pub licence: String,
    pub provenance: String,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilityCard {
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub keywords: Vec<String>,
    pub registry_hash: String,
}

impl CapabilityCard {
    #[must_use]
    pub fn estimated_tokens(&self) -> usize {
        let bytes = self.id.len()
            + self.name.len()
            + self.purpose.len()
            + self.keywords.iter().map(String::len).sum::<usize>()
            + self.registry_hash.len();
        bytes.div_ceil(4)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilitySearch {
    pub intent: String,
    pub category: Option<String>,
    pub compatible_component: Option<String>,
    pub platform: Option<String>,
    pub max_cost: Option<CostClass>,
    pub maturity: MaturityRequirement,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilitySearchResult {
    pub cards: Vec<CapabilityCard>,
    pub registry_hash: String,
    pub estimated_tokens: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct SelectionValidation {
    pub valid: bool,
    pub missing: Vec<String>,
    pub conflicts: Vec<String>,
    pub unavailable: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilityRegistry {
    pub format: String,
    pub hash: String,
    pub entries: Vec<CapabilityEntry>,
}

impl CapabilityRegistry {
    /// Build a catalogue from component, template, HUD, weather, script and target owners.
    pub fn core() -> Result<Self> {
        let mut entries = component_entries();
        entries.extend(template_entries());
        entries.extend(hud_entries());
        entries.extend(weather_entries());
        entries.extend(script_host_entries());
        entries.extend(build_target_entries());
        Self::build(entries)
    }

    pub fn build(mut entries: Vec<CapabilityEntry>) -> Result<Self> {
        entries.sort_by(|left, right| left.id.cmp(&right.id));
        for entry in &mut entries {
            entry.keywords.sort();
            entry.keywords.dedup();
            entry.platforms.sort();
            entry.platforms.dedup();
            entry.relations.sort();
            entry.relations.dedup();
        }
        validate_entries(&entries)?;
        let bytes = serde_json::to_vec(&entries).map_err(|error| {
            schema_error(
                format!("capability registry could not be serialised: {error}"),
                "Fix the invalid capability metadata and rebuild the registry.",
            )
        })?;
        Ok(Self {
            format: REGISTRY_FORMAT.to_owned(),
            hash: blake3::hash(&bytes).to_hex().to_string(),
            entries,
        })
    }

    #[must_use]
    pub fn describe(&self, id: &str) -> Option<&CapabilityEntry> {
        self.entries
            .binary_search_by_key(&id, |entry| entry.id.as_str())
            .ok()
            .and_then(|index| self.entries.get(index))
    }

    #[must_use]
    pub fn search(&self, query: &CapabilitySearch) -> CapabilitySearchResult {
        let query_terms = terms(&query.intent);
        let mut ranked = self
            .entries
            .iter()
            .filter(|entry| search_filters(entry, query))
            .filter_map(|entry| {
                let score = lexical_score(entry, &query_terms);
                (query_terms.is_empty() || score > 0).then_some((score, entry))
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| left.id.cmp(&right.id))
        });
        let limit = query
            .limit
            .unwrap_or(DEFAULT_SEARCH_LIMIT)
            .clamp(1, MAX_SEARCH_LIMIT);
        let cards = ranked
            .into_iter()
            .take(limit)
            .map(|(_, entry)| CapabilityCard {
                id: entry.id.clone(),
                name: entry.name.clone(),
                purpose: entry.purpose.clone(),
                keywords: entry.keywords.clone(),
                registry_hash: self.hash.clone(),
            })
            .collect::<Vec<_>>();
        CapabilitySearchResult {
            estimated_tokens: cards.iter().map(CapabilityCard::estimated_tokens).sum(),
            cards,
            registry_hash: self.hash.clone(),
        }
    }

    #[must_use]
    pub fn validate_selection(
        &self,
        ids: &[String],
        platform: Option<&str>,
    ) -> SelectionValidation {
        let selected = ids.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let mut missing = Vec::new();
        let mut conflicts = Vec::new();
        let mut unavailable = Vec::new();
        for id in ids {
            let Some(entry) = self.describe(id) else {
                missing.push(id.clone());
                continue;
            };
            if !entry.available
                || platform.is_some_and(|value| !entry.platforms.iter().any(|p| p == value))
            {
                unavailable.push(id.clone());
            }
            for relation in &entry.relations {
                match relation.kind {
                    RelationKind::Requires if !selected.contains(relation.target.as_str()) => {
                        missing.push(relation.target.clone());
                    }
                    RelationKind::Conflicts if selected.contains(relation.target.as_str()) => {
                        conflicts.push(format!("{} conflicts with {}", entry.id, relation.target));
                    }
                    _ => {}
                }
            }
        }
        sort_dedup(&mut missing);
        sort_dedup(&mut conflicts);
        sort_dedup(&mut unavailable);
        SelectionValidation {
            valid: missing.is_empty() && conflicts.is_empty() && unavailable.is_empty(),
            missing,
            conflicts,
            unavailable,
        }
    }

    /// Fail closed with bounded alternatives and a project-extension route.
    pub fn require(&self, id: &str) -> Result<&CapabilityEntry> {
        self.describe(id).ok_or_else(|| {
            let alternatives = self.search(&CapabilitySearch {
                intent: id.to_owned(),
                limit: Some(3),
                ..CapabilitySearch::default()
            });
            let ids = alternatives
                .cards
                .iter()
                .map(|card| card.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            EngineError::NotFound(
                format!("capability `{id}` is not registered"),
                Some(if ids.is_empty() {
                    "Search by intent or declare a bounded project extension.".to_owned()
                } else {
                    format!("Try: {ids}; otherwise declare a bounded project extension.")
                }),
            )
        })
    }
}

/// Additive extension declaration. Unknown fields and permissions fail closed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(deny_unknown_fields)]
pub struct ExtensionManifest {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub config: Vec<ContractField>,
    pub runtime_exposed: bool,
    pub editor_exposed: bool,
    pub ai_exposed: bool,
    pub cost: CostClass,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub licence: String,
    pub provenance: String,
}

impl ExtensionManifest {
    pub fn validate(&self, registry: &CapabilityRegistry) -> Result<()> {
        validate_id(&self.id)?;
        validate_version(&self.version)?;
        for permission in &self.permissions {
            if crate::capability::Capability::from_name(permission).is_none() {
                return Err(schema_error(
                    format!(
                        "extension `{}` requests unknown permission `{permission}`",
                        self.id
                    ),
                    "Use one of the seven project agent permissions.",
                ));
            }
        }
        for dependency in &self.dependencies {
            if registry.describe(dependency).is_none() {
                return Err(schema_error(
                    format!(
                        "extension `{}` has unknown dependency `{dependency}`",
                        self.id
                    ),
                    "Register the dependency first or correct its stable id.",
                ));
            }
        }
        if self.licence.trim().is_empty() || self.provenance.trim().is_empty() {
            return Err(schema_error(
                format!("extension `{}` has incomplete provenance", self.id),
                "Declare both licence and provenance.",
            ));
        }
        Ok(())
    }
}

fn component_entries() -> Vec<CapabilityEntry> {
    crate::schema::registry()
        .into_iter()
        .map(|schema| {
            entry(
                format!("component.{}", canonical(schema.name)),
                schema.name,
                CapabilityKind::Component,
                component_category(schema.name),
                schema.doc,
                "bhippi-engine::schema",
                terms(&format!("{} {}", schema.name, schema.doc)),
                schema
                    .fields
                    .iter()
                    .map(|field| ContractField {
                        name: field.name.to_owned(),
                        type_name: field.kind.to_string(),
                        required: true,
                        description: field.doc.to_owned(),
                    })
                    .collect(),
                vec![format!("schema::validate_component({})", schema.name)],
            )
        })
        .collect()
}

fn template_entries() -> Vec<CapabilityEntry> {
    crate::scaffold::templates()
        .into_iter()
        .map(|template| {
            let mut value = entry(
                format!("preset.template.{}", canonical(&template.name)),
                template.label.clone(),
                CapabilityKind::Preset,
                "placement",
                format!("Place the editable `{}` engine template.", template.name),
                "bhippi-engine::scaffold",
                terms(&format!(
                    "{} {} template prefab",
                    template.name, template.label
                )),
                Vec::new(),
                vec![format!("scaffold::template({})", template.name)],
            );
            value.operations = vec!["spawn".to_owned()];
            value.relations = template
                .components
                .iter()
                .map(|(name, _)| CapabilityRelation {
                    kind: RelationKind::Provides,
                    target: format!("component.{}", canonical(name)),
                })
                .collect();
            value.compatible_components = template
                .components
                .iter()
                .map(|(name, _)| format!("component.{}", canonical(name)))
                .collect();
            value.examples = vec![format!("spawn template={}", template.name)];
            value.editor_route = Some("engine.content.add".to_owned());
            value
        })
        .collect()
}

fn hud_entries() -> Vec<CapabilityEntry> {
    crate::hud::WidgetKind::all()
        .into_iter()
        .map(|kind| {
            let name = format!("HUD {}", kind.as_str().replace('_', " "));
            let mut value = entry(
                format!("hud.widget.{}", kind.as_str()),
                name,
                CapabilityKind::HudWidget,
                "hud",
                format!("Add and configure a `{}` HUD widget.", kind.as_str()),
                "bhippi-engine::hud",
                terms(&format!("hud ui widget {}", kind.as_str())),
                crate::hud::widget_schema(kind)
                    .iter()
                    .map(|prop| ContractField {
                        name: prop.name.to_owned(),
                        type_name: format!("{:?}", prop.kind),
                        required: false,
                        description: prop.doc.to_owned(),
                    })
                    .collect(),
                vec![format!("hud::widget_schema({})", kind.as_str())],
            );
            value.runtime_requirements = vec![crate::hud::HUD_FORMAT.to_owned()];
            value.operations = vec!["hud add_widget".to_owned(), "hud set_property".to_owned()];
            value.editor_route = Some("engine.hud".to_owned());
            value
        })
        .collect()
}

fn weather_entries() -> Vec<CapabilityEntry> {
    crate::weather::presets()
        .into_iter()
        .map(|preset| {
            let mut value = entry(
                format!("weather.{}", preset.id),
                preset.label,
                CapabilityKind::Weather,
                "environment",
                format!(
                    "Apply the `{}` lighting, fog and precipitation preset.",
                    preset.id
                ),
                "bhippi-engine::weather",
                terms(&format!(
                    "weather environment fog sky {} {}",
                    preset.id, preset.precip
                )),
                Vec::new(),
                vec![format!("weather::preset({})", preset.id)],
            );
            value.operations = vec!["set_weather".to_owned()];
            value.relations = vec![CapabilityRelation {
                kind: RelationKind::ComposesWith,
                target: "component.weather_volume".to_owned(),
            }];
            value.compatible_components = vec!["component.weather_volume".to_owned()];
            value.editor_route = Some("engine.environment.weather".to_owned());
            value
        })
        .collect()
}

fn script_host_entries() -> Vec<CapabilityEntry> {
    crate::script::HOST_FNS
        .iter()
        .map(|host| {
            let mut value = entry(
                format!("script.host.{}", host.name),
                host.name,
                CapabilityKind::ScriptHost,
                "scripting",
                host.doc,
                "bhippi-engine::script",
                terms(&format!("script host {} {}", host.name, host.doc)),
                (0..host.arity)
                    .map(|index| ContractField {
                        name: format!("arg{index}"),
                        type_name: "script value".to_owned(),
                        required: true,
                        description: "Bounded VM value.".to_owned(),
                    })
                    .collect(),
                vec![format!("script::host_fn({})", host.name)],
            );
            value.runtime_requirements = vec!["compiled bhippi script VM".to_owned()];
            value.compatible_components = vec!["component.script_ref".to_owned()];
            value.limitations = vec![format!("exactly {} arguments", host.arity)];
            value.editor_route = Some("engine.script".to_owned());
            value
        })
        .collect()
}

fn build_target_entries() -> Vec<CapabilityEntry> {
    ["android", "ios", "linux", "macos", "web", "windows"]
        .into_iter()
        .map(|target| {
            let mut value = entry(
                format!("build.target.{target}"),
                format!("{target} build"),
                CapabilityKind::BuildTarget,
                "build",
                format!("Validate and package the project for {target}."),
                "bhippi-engine::manifest + bhippi-engine-build",
                terms(&format!("build package export {target}")),
                Vec::new(),
                vec!["manifest::GameManifest::enabled_targets".to_owned()],
            );
            value.cost = CostClass::High;
            value.platforms = vec![target.to_owned()];
            value.operations = vec!["build".to_owned()];
            value.runtime_requirements = vec![format!("{target} host toolchain")];
            value.limitations =
                vec!["Host toolchain availability is checked at build time.".to_owned()];
            value.maturity.runtime_proven = false;
            value.editor_route = Some("engine.build".to_owned());
            value
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn entry(
    id: String,
    name: impl Into<String>,
    kind: CapabilityKind,
    category: &str,
    purpose: impl Into<String>,
    owner: &str,
    keywords: Vec<String>,
    properties: Vec<ContractField>,
    validators: Vec<String>,
) -> CapabilityEntry {
    CapabilityEntry {
        id,
        name: name.into(),
        kind,
        category: category.to_owned(),
        version: ENTRY_VERSION.to_owned(),
        purpose: purpose.into(),
        owner: owner.to_owned(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        properties,
        operations: Vec::new(),
        relations: Vec::new(),
        runtime_requirements: Vec::new(),
        cost: CostClass::Low,
        platforms: all_platforms(),
        editor_route: Some("engine.details".to_owned()),
        keywords,
        compatible_components: Vec::new(),
        examples: Vec::new(),
        limitations: Vec::new(),
        extension_points: Vec::new(),
        verification: vec!["deterministic owner validation".to_owned()],
        validators,
        debuggers: vec!["/gamedebug".to_owned()],
        maturity: CapabilityMaturity {
            documented: true,
            implemented: true,
            tested: true,
            editor_accessible: true,
            ai_accessible: true,
            runtime_proven: true,
            production_ready: false,
            proven_platforms: Vec::new(),
            budget_evidence: Vec::new(),
        },
        licence: "project licence".to_owned(),
        provenance: format!("generated from {owner}"),
        available: true,
        unavailable_reason: None,
    }
}

fn validate_entries(entries: &[CapabilityEntry]) -> Result<()> {
    let mut ids = BTreeSet::new();
    for entry in entries {
        validate_id(&entry.id)?;
        validate_version(&entry.version)?;
        if !ids.insert(entry.id.as_str()) {
            return Err(schema_error(
                format!("duplicate capability id `{}`", entry.id),
                "Use one stable id.",
            ));
        }
        if entry.validators.is_empty() {
            return Err(schema_error(
                format!("capability `{}` has no validator", entry.id),
                "Name a Rust validator or probe.",
            ));
        }
        if !entry.available && entry.unavailable_reason.is_none() {
            return Err(schema_error(
                format!("capability `{}` is unavailable without a reason", entry.id),
                "Record the missing dependency.",
            ));
        }
    }
    for entry in entries {
        for relation in &entry.relations {
            if !ids.contains(relation.target.as_str()) {
                return Err(schema_error(
                    format!(
                        "capability `{}` points at unknown `{}`",
                        entry.id, relation.target
                    ),
                    "Register the target or remove the relation.",
                ));
            }
        }
    }
    reject_cycles(entries)
}

fn reject_cycles(entries: &[CapabilityEntry]) -> Result<()> {
    let edges = entries
        .iter()
        .map(|entry| {
            (
                entry.id.as_str(),
                entry
                    .relations
                    .iter()
                    .filter(|relation| {
                        matches!(
                            relation.kind,
                            RelationKind::Requires | RelationKind::Supersedes
                        )
                    })
                    .map(|relation| relation.target.as_str())
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut active = BTreeSet::new();
    let mut complete = BTreeSet::new();
    for id in edges.keys().copied() {
        visit(id, &edges, &mut active, &mut complete)?;
    }
    Ok(())
}

fn visit<'a>(
    id: &'a str,
    edges: &BTreeMap<&'a str, Vec<&'a str>>,
    active: &mut BTreeSet<&'a str>,
    complete: &mut BTreeSet<&'a str>,
) -> Result<()> {
    if complete.contains(id) {
        return Ok(());
    }
    if !active.insert(id) {
        return Err(schema_error(
            format!("capability dependency cycle reaches `{id}`"),
            "Remove the requires/supersedes cycle.",
        ));
    }
    if let Some(next) = edges.get(id) {
        for target in next {
            visit(target, edges, active, complete)?;
        }
    }
    active.remove(id);
    complete.insert(id);
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
        schema_error(
            format!("`{id}` is not a canonical capability id"),
            "Use lowercase dotted segments.",
        )
    })
}

fn validate_version(version: &str) -> Result<()> {
    let parts = version.split('.').collect::<Vec<_>>();
    (parts.len() == 3 && parts[0] == "1" && parts.iter().all(|part| part.parse::<u32>().is_ok()))
        .then_some(())
        .ok_or_else(|| {
            schema_error(
                format!("version `{version}` is incompatible with {REGISTRY_FORMAT}"),
                "Use numeric 1.x.y.",
            )
        })
}

fn search_filters(entry: &CapabilityEntry, query: &CapabilitySearch) -> bool {
    entry.available
        && query
            .category
            .as_ref()
            .is_none_or(|category| entry.category.eq_ignore_ascii_case(category))
        && query.compatible_component.as_ref().is_none_or(|component| {
            entry.id == *component || entry.compatible_components.contains(component)
        })
        && query
            .platform
            .as_ref()
            .is_none_or(|platform| entry.platforms.contains(platform))
        && query.max_cost.is_none_or(|cost| entry.cost <= cost)
        && entry.maturity.satisfies(&query.maturity)
}

fn lexical_score(entry: &CapabilityEntry, query: &[String]) -> usize {
    let ids = terms(&entry.id);
    let names = terms(&entry.name);
    let keywords = entry
        .keywords
        .iter()
        .flat_map(|value| terms(value))
        .collect::<BTreeSet<_>>();
    let purpose = terms(&entry.purpose).into_iter().collect::<BTreeSet<_>>();
    query
        .iter()
        .map(|term| {
            usize::from(ids.contains(term)) * 8
                + usize::from(names.contains(term)) * 6
                + usize::from(keywords.contains(term)) * 4
                + usize::from(purpose.contains(term)) * 2
        })
        .sum()
}

fn terms(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn canonical(value: &str) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if ch.is_ascii_uppercase() && index > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn component_category(name: &str) -> &'static str {
    match name {
        "RigidBody" | "Collider" | "CharacterController" => "physics",
        "Camera" => "camera",
        "Light" | "WeatherVolume" => "environment",
        "MeshRenderer" | "MaterialOverride" => "rendering",
        "AudioSource" => "audio",
        "ScriptRef" => "scripting",
        "UiDocument" => "hud",
        _ => "scene",
    }
}

fn all_platforms() -> Vec<String> {
    ["android", "ios", "linux", "macos", "web", "windows"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn sort_dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}
fn schema_error(message: String, hint: &str) -> EngineError {
    EngineError::Schema(message, Some(hint.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> CapabilityRegistry {
        CapabilityRegistry::core().expect("core registry builds")
    }

    #[test]
    fn core_registry_is_deterministic_and_covers_authoritative_families() {
        let first = registry();
        assert_eq!(first, registry());
        assert_eq!(first.format, REGISTRY_FORMAT);
        assert_eq!(first.hash.len(), 64);
        for schema in crate::schema::registry() {
            assert!(first
                .describe(&format!("component.{}", canonical(schema.name)))
                .is_some());
        }
        for template in crate::scaffold::templates() {
            assert!(first
                .describe(&format!("preset.template.{}", template.name))
                .is_some());
        }
        for kind in crate::hud::WidgetKind::all() {
            assert!(first
                .describe(&format!("hud.widget.{}", kind.as_str()))
                .is_some());
        }
        for preset in crate::weather::presets() {
            assert!(first.describe(&format!("weather.{}", preset.id)).is_some());
        }
        for host in crate::script::HOST_FNS {
            assert!(first
                .describe(&format!("script.host.{}", host.name))
                .is_some());
        }
    }

    #[test]
    fn survival_search_is_bounded_relevant_and_hash_bound() {
        let registry = registry();
        let result = registry.search(&CapabilitySearch {
            intent: "third person player survival camera physics character controller".to_owned(),
            limit: Some(6),
            ..CapabilitySearch::default()
        });
        assert!(!result.cards.is_empty());
        assert!(result.cards.len() <= 6);
        assert!(result
            .cards
            .iter()
            .any(|card| card.id == "component.character_controller"));
        assert!(result
            .cards
            .iter()
            .all(|card| card.registry_hash == registry.hash));
        assert!(result.estimated_tokens > 0);
    }

    #[test]
    fn hallucinated_capability_is_rejected_with_extension_guidance() {
        let error = registry()
            .require("physics.quantum_grapple")
            .expect_err("unknown fails");
        assert!(error.to_string().contains("not registered"));
        assert!(error.hint().is_some_and(|hint| hint.contains("extension")));
    }

    #[test]
    fn malformed_registry_graphs_fail_closed() {
        let base = registry().entries[0].clone();
        assert!(CapabilityRegistry::build(vec![base.clone(), base.clone()]).is_err());

        let mut dangling = base.clone();
        dangling.relations.push(CapabilityRelation {
            kind: RelationKind::Requires,
            target: "missing.capability".to_owned(),
        });
        assert!(CapabilityRegistry::build(vec![dangling]).is_err());

        let mut no_validator = base.clone();
        no_validator.validators.clear();
        assert!(CapabilityRegistry::build(vec![no_validator]).is_err());

        let mut left = base.clone();
        left.id = "test.left".to_owned();
        left.relations = vec![CapabilityRelation {
            kind: RelationKind::Requires,
            target: "test.right".to_owned(),
        }];
        let mut right = base;
        right.id = "test.right".to_owned();
        right.relations = vec![CapabilityRelation {
            kind: RelationKind::Requires,
            target: "test.left".to_owned(),
        }];
        assert!(CapabilityRegistry::build(vec![left, right]).is_err());
    }

    #[test]
    fn search_filters_and_selection_validation_are_deterministic() {
        let registry = registry();
        let query = CapabilitySearch {
            intent: "character physics".to_owned(),
            category: Some("physics".to_owned()),
            compatible_component: None,
            platform: Some("windows".to_owned()),
            max_cost: Some(CostClass::Low),
            maturity: MaturityRequirement {
                implemented: true,
                ai_accessible: true,
                ..MaturityRequirement::default()
            },
            limit: Some(10),
        };
        assert_eq!(registry.search(&query), registry.search(&query));
        assert!(registry.search(&query).cards.iter().all(|card| registry
            .describe(&card.id)
            .is_some_and(|entry| entry.category == "physics")));

        let base = registry.entries[0].clone();
        let mut left = base.clone();
        left.id = "test.left".to_owned();
        left.platforms = vec!["windows".to_owned()];
        left.relations = vec![
            CapabilityRelation {
                kind: RelationKind::Requires,
                target: "test.required".to_owned(),
            },
            CapabilityRelation {
                kind: RelationKind::Conflicts,
                target: "test.rival".to_owned(),
            },
        ];
        let mut required = base.clone();
        required.id = "test.required".to_owned();
        required.relations.clear();
        required.platforms = all_platforms();
        let mut rival = base;
        rival.id = "test.rival".to_owned();
        rival.relations.clear();
        rival.platforms = all_platforms();
        let test_registry =
            CapabilityRegistry::build(vec![left, required, rival]).expect("valid graph");
        let result = test_registry.validate_selection(
            &["test.left".to_owned(), "test.rival".to_owned()],
            Some("web"),
        );
        assert!(!result.valid);
        assert_eq!(result.missing, vec!["test.required"]);
        assert_eq!(result.unavailable, vec!["test.left"]);
        assert_eq!(
            result.conflicts,
            vec!["test.left conflicts with test.rival"]
        );
    }

    #[test]
    fn extension_manifest_blocks_unknown_authority_and_provenance_gaps() {
        let registry = registry();
        let mut manifest = ExtensionManifest {
            id: "extension.survival_pack".to_owned(),
            version: ENTRY_VERSION.to_owned(),
            dependencies: vec!["component.character_controller".to_owned()],
            permissions: vec!["create_content".to_owned()],
            config: Vec::new(),
            runtime_exposed: true,
            editor_exposed: true,
            ai_exposed: true,
            cost: CostClass::Medium,
            platforms: vec!["windows".to_owned()],
            licence: "MIT".to_owned(),
            provenance: "local extension manifest".to_owned(),
        };
        manifest.validate(&registry).expect("known manifest");
        manifest.permissions = vec!["root_access".to_owned()];
        assert!(manifest.validate(&registry).is_err());
        manifest.permissions = vec!["create_content".to_owned()];
        manifest.dependencies = vec!["missing.capability".to_owned()];
        assert!(manifest.validate(&registry).is_err());
        manifest.dependencies.clear();
        manifest.licence.clear();
        assert!(manifest.validate(&registry).is_err());
    }

    #[test]
    fn maturity_is_not_one_available_flag() {
        let maturity = CapabilityMaturity {
            implemented: true,
            ai_accessible: true,
            ..CapabilityMaturity::default()
        };
        assert!(maturity.satisfies(&MaturityRequirement {
            implemented: true,
            ..MaturityRequirement::default()
        }));
        assert!(!maturity.satisfies(&MaturityRequirement {
            runtime_proven: true,
            ..MaturityRequirement::default()
        }));
    }
}
