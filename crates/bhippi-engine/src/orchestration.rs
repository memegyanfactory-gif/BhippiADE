//! Capability-first routing and bounded source-fallback contracts (ADR-0040).
//! This module makes decisions only; live provider routing, source access and execution remain
//! with their existing owners.

use crate::registry::{
    CapabilityCard, CapabilityRegistry, CapabilitySearch, CostClass, MaturityRequirement,
};
use serde::{Deserialize, Serialize};
use specta::Type;

pub const ROUTER_DECISION_FORMAT: &str = "bhippi-capability-route@1";
pub const TOOL_DESCRIPTOR_FORMAT: &str = "bhippi-generic-engine-tools@1";
pub const GENERIC_ENGINE_TOOL_NAMES: [&str; 5] = [
    "engine.action",
    "engine.capabilities.describe",
    "engine.capabilities.search",
    "engine.playtest",
    "engine.query",
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilityRouteRequest {
    pub intent: String,
    pub category: Option<String>,
    pub compatible_component: Option<String>,
    pub platform: Option<String>,
    pub max_cost: Option<CostClass>,
    pub maturity: MaturityRequirement,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RankedCapability {
    pub card: CapabilityCard,
    pub compatible: bool,
    pub cost: CostClass,
    pub maturity_satisfied: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CapabilityRouteDecision {
    pub format: String,
    pub registry_hash: String,
    pub ranked: Vec<RankedCapability>,
    pub source_access: SourceAccessDecision,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SourceAccessDecision {
    DeniedUntilClassified,
    DeniedKnownFix {
        limitation: SourceFallbackLimitation,
    },
    BoundedExtensionAllowed {
        proof: EngineLimitationProof,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SourceFallbackLimitation {
    BadParameters,
    MissingDependency,
    InvalidOrder,
    IncompatibleComposition,
    UnsupportedCapability,
    ActualEngineLimitation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct EngineLimitationProof {
    pub requested_capability: String,
    pub searched_intents: Vec<String>,
    pub registry_hash: String,
    pub alternatives_considered: Vec<String>,
    pub limitation_evidence: Vec<String>,
}

impl EngineLimitationProof {
    pub fn validate(&self, registry: &CapabilityRegistry) -> std::result::Result<(), String> {
        if self.requested_capability.trim().is_empty()
            || self.searched_intents.is_empty()
            || self.alternatives_considered.is_empty()
            || self.limitation_evidence.is_empty()
        {
            return Err(
                "engine limitation proof requires a request, search record, considered alternative and evidence"
                    .to_owned(),
            );
        }
        if self.registry_hash != registry.hash {
            return Err("engine limitation proof is stale for the active registry".to_owned());
        }
        if self
            .alternatives_considered
            .iter()
            .any(|id| registry.describe(id).is_none())
        {
            return Err("engine limitation proof names an unknown alternative".to_owned());
        }
        Ok(())
    }
}

impl CapabilityRouteDecision {
    pub fn source_extension_authorized(&self) -> bool {
        matches!(
            self.source_access,
            SourceAccessDecision::BoundedExtensionAllowed { .. }
        )
    }
}

pub fn route_capabilities(
    registry: &CapabilityRegistry,
    request: &CapabilityRouteRequest,
) -> CapabilityRouteDecision {
    let search = registry.search(&CapabilitySearch {
        intent: request.intent.clone(),
        category: request.category.clone(),
        compatible_component: request.compatible_component.clone(),
        platform: request.platform.clone(),
        max_cost: request.max_cost,
        maturity: request.maturity.clone(),
        limit: request.limit,
    });
    let ranked = search
        .cards
        .into_iter()
        .filter_map(|card| {
            registry.describe(&card.id).map(|entry| RankedCapability {
                compatible: request
                    .compatible_component
                    .as_ref()
                    .is_none_or(|component| {
                        entry
                            .compatible_components
                            .iter()
                            .any(|item| item == component)
                    }),
                cost: entry.cost,
                maturity_satisfied: entry.maturity.satisfies(&request.maturity),
                card,
            })
        })
        .collect();
    CapabilityRouteDecision {
        format: ROUTER_DECISION_FORMAT.to_owned(),
        registry_hash: registry.hash.clone(),
        ranked,
        source_access: SourceAccessDecision::DeniedUntilClassified,
    }
}

pub fn classify_known_failure(
    registry: &CapabilityRegistry,
    selected_ids: &[String],
    platform: Option<&str>,
    bad_parameters: bool,
    invalid_order: bool,
) -> SourceFallbackLimitation {
    if bad_parameters {
        return SourceFallbackLimitation::BadParameters;
    }
    if invalid_order {
        return SourceFallbackLimitation::InvalidOrder;
    }
    let validation = registry.validate_selection(selected_ids, platform);
    if !validation.missing.is_empty() {
        SourceFallbackLimitation::MissingDependency
    } else if !validation.conflicts.is_empty() {
        SourceFallbackLimitation::IncompatibleComposition
    } else {
        SourceFallbackLimitation::UnsupportedCapability
    }
}

pub fn authorize_bounded_extension(
    registry: &CapabilityRegistry,
    mut decision: CapabilityRouteDecision,
    proof: EngineLimitationProof,
) -> std::result::Result<CapabilityRouteDecision, String> {
    if decision.format != ROUTER_DECISION_FORMAT || decision.registry_hash != registry.hash {
        return Err("capability route decision is stale for the active registry".to_owned());
    }
    proof.validate(registry)?;
    decision.source_access = SourceAccessDecision::BoundedExtensionAllowed { proof };
    Ok(decision)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GenericToolDescriptor {
    pub name: String,
    pub purpose: String,
    pub request_discriminators: Vec<String>,
    pub response_discriminators: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GenericToolManifest {
    pub format: String,
    pub tools: Vec<GenericToolDescriptor>,
}

impl GenericToolManifest {
    pub fn engine_default() -> Self {
        Self {
            format: TOOL_DESCRIPTOR_FORMAT.to_owned(),
            tools: vec![
                tool(
                    "engine.capabilities.search",
                    "Find ranked engine capability cards",
                    &["search"],
                    &["cards"],
                ),
                tool(
                    "engine.capabilities.describe",
                    "Load one selected capability contract",
                    &["capability_id"],
                    &["contract"],
                ),
                tool(
                    "engine.query",
                    "Read typed project and scene projections",
                    &["project", "scene", "entity", "component"],
                    &[
                        "project_state",
                        "scene_state",
                        "entity_state",
                        "component_state",
                    ],
                ),
                tool(
                    "engine.action",
                    "Submit a typed validated engine action batch",
                    &["validate", "apply"],
                    &["validation", "transaction"],
                ),
                tool(
                    "engine.playtest",
                    "Run typed test plans and return evidence",
                    &["plan", "execute", "report"],
                    &["test_plan", "test_run", "evidence_report"],
                ),
            ],
        }
    }

    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.format != TOOL_DESCRIPTOR_FORMAT {
            return Err("unsupported generic tool descriptor format".to_owned());
        }
        if self.tools.len() != 5 {
            return Err("generic engine surface requires exactly five stable tools".to_owned());
        }
        let mut names = self
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        if names != GENERIC_ENGINE_TOOL_NAMES
            || names.len() != self.tools.len()
            || self.tools.iter().any(|tool| {
                tool.request_discriminators.is_empty() || tool.response_discriminators.is_empty()
            })
        {
            return Err(
                "generic tools require unique names and typed request/response discriminators"
                    .to_owned(),
            );
        }
        Ok(())
    }

    pub fn estimated_schema_tokens(&self) -> usize {
        self.tools
            .iter()
            .map(|tool| {
                tool.name.len()
                    + tool.purpose.len()
                    + tool
                        .request_discriminators
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
                    + tool
                        .response_discriminators
                        .iter()
                        .map(String::len)
                        .sum::<usize>()
            })
            .sum::<usize>()
            .div_ceil(4)
    }
}

fn tool(name: &str, purpose: &str, requests: &[&str], responses: &[&str]) -> GenericToolDescriptor {
    GenericToolDescriptor {
        name: name.to_owned(),
        purpose: purpose.to_owned(),
        request_discriminators: requests.iter().map(|value| (*value).to_owned()).collect(),
        response_discriminators: responses.iter().map(|value| (*value).to_owned()).collect(),
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelTaskClass {
    ParameterEdit,
    CapabilityComposition,
    Architecture,
    VisualJudgement,
    SourceExtension,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ModelRouteRecommendation {
    pub task_class: ModelTaskClass,
    pub user_model_choice: Option<String>,
    pub route_reason: String,
    pub fallback_class: Option<String>,
    pub cost_class: CostClass,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_is_registry_first_and_denies_source_by_default() {
        let registry = CapabilityRegistry::core().expect("registry");
        let decision = route_capabilities(
            &registry,
            &CapabilityRouteRequest {
                intent: "weather rain".to_owned(),
                category: None,
                compatible_component: None,
                platform: None,
                max_cost: None,
                maturity: MaturityRequirement::default(),
                limit: Some(3),
            },
        );
        assert!(!decision.ranked.is_empty());
        assert_eq!(decision.registry_hash, registry.hash);
        assert!(!decision.source_extension_authorized());
    }

    #[test]
    fn only_fresh_complete_limitation_proof_authorizes_extension() {
        let registry = CapabilityRegistry::core().expect("registry");
        let alternative = registry.entries[0].id.clone();
        let decision = route_capabilities(
            &registry,
            &CapabilityRouteRequest {
                intent: "unknown mechanic".to_owned(),
                category: None,
                compatible_component: None,
                platform: None,
                max_cost: None,
                maturity: MaturityRequirement::default(),
                limit: Some(3),
            },
        );
        let incomplete = EngineLimitationProof {
            requested_capability: "portal".to_owned(),
            searched_intents: Vec::new(),
            registry_hash: registry.hash.clone(),
            alternatives_considered: Vec::new(),
            limitation_evidence: vec!["search returned no compatible implementation".to_owned()],
        };
        assert!(authorize_bounded_extension(&registry, decision.clone(), incomplete).is_err());
        let proof = EngineLimitationProof {
            requested_capability: "portal".to_owned(),
            searched_intents: vec!["portal".to_owned(), "teleport".to_owned()],
            registry_hash: registry.hash.clone(),
            alternatives_considered: vec![alternative],
            limitation_evidence: vec![
                "registry searches produced no compatible capability".to_owned()
            ],
        };
        let authorized =
            authorize_bounded_extension(&registry, decision, proof).expect("complete proof");
        assert!(authorized.source_extension_authorized());

        let fresh_proof = match &authorized.source_access {
            SourceAccessDecision::BoundedExtensionAllowed { proof } => proof.clone(),
            _ => panic!("authorized decision carries its proof"),
        };
        let mut stale_decision = authorized;
        stale_decision.registry_hash = "stale".to_owned();
        assert!(authorize_bounded_extension(&registry, stale_decision, fresh_proof).is_err());
    }

    #[test]
    fn generic_tool_surface_stays_small_typed_and_measurable() {
        let manifest = GenericToolManifest::engine_default();
        manifest.validate().expect("tool manifest");
        assert_eq!(manifest.tools.len(), 5);
        assert!(manifest.estimated_schema_tokens() > 0);

        let mut divergent = manifest;
        divergent.tools[0].name = "engine.hidden_escape".to_owned();
        assert!(divergent.validate().is_err());
    }
}
