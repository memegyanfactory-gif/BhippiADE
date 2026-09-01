//! Versioned game intent and registry-first composition planning.
//!
//! These contracts stop at an approved, reviewable DAG. They do not execute actions, edit
//! source, or treat conversational memory as project state.

use crate::error::{EngineError, Result};
use crate::registry::{
    CapabilityCard, CapabilityKind, CapabilityRegistry, CapabilitySearch, CostClass,
    MaturityRequirement, RelationKind, MAX_SEARCH_LIMIT,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const GAME_SPEC_FORMAT: &str = "bhippi-game-spec@1";
pub const COMPOSITION_PLAN_FORMAT: &str = "bhippi-composition-plan@1";
pub const CONFIDENCE_BPS_MAX: u16 = 10_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FactCertainty {
    Certain,
    Assumed,
    Ambiguous,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct SpecFact {
    pub value: String,
    /// Integer basis points avoid platform-dependent float formatting: 10_000 means 100%.
    pub confidence_bps: u16,
    pub certainty: FactCertainty,
    #[serde(default)]
    pub alternatives: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GameRequirement {
    pub id: String,
    pub statement: String,
    pub confidence_bps: u16,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub preferred_capabilities: Vec<String>,
    #[serde(default)]
    pub maturity: MaturityRequirement,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum QuestionImpact {
    High,
    Critical,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct OpenQuestion {
    pub id: String,
    pub question: String,
    pub impact: QuestionImpact,
    pub affects_requirements: Vec<String>,
    pub options: Vec<String>,
    #[serde(default)]
    pub resolved: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct MechanicContract {
    pub id: String,
    pub promise: String,
    pub setup: Vec<String>,
    pub requirement_ids: Vec<String>,
    pub deterministic_probes: Vec<String>,
    pub expected_evidence: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PlanningBudgets {
    pub turn_tokens: u32,
    #[serde(default)]
    pub frame_time_micros: Option<u32>,
    #[serde(default)]
    pub max_memory_mb: Option<u32>,
    #[serde(default)]
    pub max_content_mb: Option<u32>,
    #[serde(default)]
    pub max_capability_cost: Option<CostClass>,
    pub max_new_extensions: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GameConstraints {
    pub platforms: Vec<String>,
    #[serde(default)]
    pub quality: Vec<SpecFact>,
    pub budgets: PlanningBudgets,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GameSpec {
    pub format: String,
    pub title: String,
    pub genre: SpecFact,
    pub player_loop: Vec<SpecFact>,
    #[serde(default)]
    pub mechanics: Vec<GameRequirement>,
    #[serde(default)]
    pub world: Vec<GameRequirement>,
    #[serde(default)]
    pub actors: Vec<GameRequirement>,
    #[serde(default)]
    pub ui: Vec<GameRequirement>,
    pub constraints: GameConstraints,
    pub acceptance_mechanics: Vec<MechanicContract>,
    #[serde(default)]
    pub open_questions: Vec<OpenQuestion>,
}

impl GameSpec {
    pub fn parse(text: &str) -> Result<Self> {
        let spec: Self = serde_json::from_str(text).map_err(|error| {
            spec_error(
                format!("invalid game spec: {error}"),
                format!("Fix the JSON and keep format {GAME_SPEC_FORMAT}."),
            )
        })?;
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != GAME_SPEC_FORMAT {
            return Err(spec_error(
                format!("unsupported game spec format {:?}", self.format),
                format!("Use {GAME_SPEC_FORMAT}; unknown major versions block."),
            ));
        }
        require_text(&self.title, "game title")?;
        validate_fact(&self.genre, "genre")?;
        if self.player_loop.is_empty() {
            return Err(spec_error(
                "game spec has no player loop".to_owned(),
                "Describe the repeated player actions and feedback loop.".to_owned(),
            ));
        }
        for fact in &self.player_loop {
            validate_fact(fact, "player-loop fact")?;
        }
        for fact in &self.constraints.quality {
            validate_fact(fact, "quality constraint")?;
        }
        validate_budgets(&self.constraints.budgets)?;
        if self.constraints.platforms.is_empty()
            || self
                .constraints
                .platforms
                .iter()
                .any(|platform| platform.trim().is_empty())
        {
            return Err(spec_error(
                "game spec needs at least one non-empty target platform".to_owned(),
                "Name the platforms the compatibility solver must satisfy.".to_owned(),
            ));
        }
        if self
            .constraints
            .platforms
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != self.constraints.platforms.len()
        {
            return Err(spec_error(
                "game spec has duplicate target platforms".to_owned(),
                "List every target platform once.".to_owned(),
            ));
        }

        let requirements = self.requirements();
        if requirements.is_empty() {
            return Err(spec_error(
                "game spec has no mechanics, world, actor or UI requirements".to_owned(),
                "Add at least one stable requirement for the planner to resolve.".to_owned(),
            ));
        }
        let mut requirement_ids = BTreeSet::new();
        for requirement in &requirements {
            validate_id(&requirement.id, "requirement")?;
            require_text(&requirement.statement, "requirement statement")?;
            validate_confidence(requirement.confidence_bps, "requirement")?;
            if !requirement_ids.insert(requirement.id.as_str()) {
                return Err(spec_error(
                    format!("duplicate requirement id {:?}", requirement.id),
                    "Give every requirement one stable id across revisions.".to_owned(),
                ));
            }
        }
        for requirement in &requirements {
            for dependency in &requirement.depends_on {
                if !requirement_ids.contains(dependency.as_str()) {
                    return Err(spec_error(
                        format!(
                            "requirement {:?} depends on unknown {:?}",
                            requirement.id, dependency
                        ),
                        "Point depends_on at another requirement id in this spec.".to_owned(),
                    ));
                }
            }
        }
        reject_requirement_cycles(&requirements)?;

        if self.acceptance_mechanics.is_empty() {
            return Err(spec_error(
                "game spec has no acceptance mechanics".to_owned(),
                "Map each gameplay promise to deterministic setup, probes and expected evidence."
                    .to_owned(),
            ));
        }
        let mut contract_ids = BTreeSet::new();
        for contract in &self.acceptance_mechanics {
            validate_id(&contract.id, "mechanic contract")?;
            if !contract_ids.insert(contract.id.as_str()) {
                return Err(spec_error(
                    format!("duplicate mechanic contract id {:?}", contract.id),
                    "Give every mechanic contract one stable id.".to_owned(),
                ));
            }
            require_text(&contract.promise, "mechanic promise")?;
            if contract.setup.is_empty()
                || contract.requirement_ids.is_empty()
                || contract.deterministic_probes.is_empty()
                || contract.expected_evidence.is_empty()
                || contract
                    .setup
                    .iter()
                    .chain(&contract.deterministic_probes)
                    .chain(&contract.expected_evidence)
                    .any(|value| value.trim().is_empty())
            {
                return Err(spec_error(
                    format!("mechanic contract {:?} is not testable", contract.id),
                    "Provide setup, deterministic probes and expected evidence.".to_owned(),
                ));
            }
            for id in &contract.requirement_ids {
                if !requirement_ids.contains(id.as_str()) {
                    return Err(spec_error(
                        format!("mechanic contract {:?} cites unknown {id:?}", contract.id),
                        "Map the contract only to requirements in this spec.".to_owned(),
                    ));
                }
            }
        }
        validate_questions(&self.open_questions, &requirement_ids)?;
        Ok(())
    }

    pub fn dump(&self) -> Result<String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| {
            spec_error(
                format!("cannot serialise game spec: {error}"),
                "Report this as an engine serialisation bug.".to_owned(),
            )
        })
    }

    #[must_use]
    pub fn requirements(&self) -> Vec<&GameRequirement> {
        self.mechanics
            .iter()
            .chain(&self.world)
            .chain(&self.actors)
            .chain(&self.ui)
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompositionDecision {
    Build,
    Integrate,
    Wrap,
    Adapt,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RejectedCapability {
    pub capability_id: String,
    pub cost: CostClass,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct DecisionRecord {
    pub strategy: CompositionDecision,
    pub rationale: String,
    #[serde(default)]
    pub rejected_lower_cost: Vec<RejectedCapability>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RegistryQueryRecord {
    pub requirement_id: String,
    pub intent: String,
    pub returned_card_ids: Vec<String>,
    pub estimated_tokens: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanNodePayload {
    BudgetGuard { budgets: PlanningBudgets },
    CapabilitySelection { capability_ids: Vec<String> },
    Configuration { capability_id: String },
    Document { format: String, path: String },
    ActionBatch { label: String },
    TestScenario { mechanic_contract_id: String },
    ProjectExtension { extension_id: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PlanCard {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub capability_ids: Vec<String>,
    pub decision: Option<CompositionDecision>,
    pub estimated_tokens: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CompositionPlanNode {
    pub id: String,
    pub requirement_id: Option<String>,
    pub depends_on: Vec<String>,
    pub payload: PlanNodePayload,
    pub decision: Option<DecisionRecord>,
    pub card: PlanCard,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ProjectStateDelta {
    pub spec_hash: String,
    pub registry_hash: String,
    pub selected_capability_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct TaskCheckpointDelta {
    pub planned_node_ids: Vec<String>,
    pub unresolved_question_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CompositionPlan {
    pub format: String,
    pub spec_hash: String,
    pub registry_hash: String,
    pub nodes: Vec<CompositionPlanNode>,
    pub queries: Vec<RegistryQueryRecord>,
    pub estimated_card_tokens: usize,
    pub project_state_delta: ProjectStateDelta,
    pub checkpoint_delta: TaskCheckpointDelta,
}

impl CompositionPlan {
    pub fn validate(&self, token_budget: u32) -> std::result::Result<(), CompositionPlanError> {
        if self.format != COMPOSITION_PLAN_FORMAT {
            return Err(plan_error(
                "unsupported composition plan format",
                "Rebuild the plan with the current planner.",
                Vec::new(),
            ));
        }
        if self.estimated_card_tokens > token_budget as usize {
            return Err(plan_error(
                "composition plan cards exceed the turn-token budget",
                "Reduce requirements or split the plan into smaller approved slices.",
                Vec::new(),
            ));
        }
        let recomputed_tokens = self
            .nodes
            .iter()
            .map(|node| estimate_plan_card(&node.card))
            .sum::<usize>();
        if self
            .nodes
            .iter()
            .any(|node| node.card.estimated_tokens != estimate_plan_card(&node.card))
            || self.estimated_card_tokens != recomputed_tokens
        {
            return Err(plan_error(
                "composition plan token estimate is not derived from its cards",
                "Rebuild the plan instead of editing token totals.",
                Vec::new(),
            ));
        }
        let ids = self
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<BTreeSet<_>>();
        if ids.len() != self.nodes.len() {
            return Err(plan_error(
                "composition plan has duplicate node ids",
                "Give every plan node a stable unique id.",
                Vec::new(),
            ));
        }
        for node in &self.nodes {
            if node.depends_on.iter().any(|id| !ids.contains(id.as_str())) {
                return Err(plan_error(
                    "composition plan has a dangling dependency",
                    "Point every dependency at another node in the same plan.",
                    Vec::new(),
                ));
            }
        }
        reject_plan_cycles(&self.nodes)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{message}")]
pub struct CompositionPlanError {
    pub message: String,
    pub hint: String,
    pub alternatives: Vec<CapabilityCard>,
}

/// Deterministic and registry-first. No source discovery or model-authored implementation is
/// consulted by this function.
pub fn compose_plan(
    spec: &GameSpec,
    registry: &CapabilityRegistry,
) -> std::result::Result<CompositionPlan, CompositionPlanError> {
    spec.validate().map_err(|error| {
        plan_error(
            &error.to_string(),
            error.hint().unwrap_or("Fix the GameSpec before planning."),
            Vec::new(),
        )
    })?;
    let unresolved = spec
        .open_questions
        .iter()
        .filter(|question| question.resolved.is_none())
        .collect::<Vec<_>>();
    if !unresolved.is_empty() {
        return Err(plan_error(
            "material GameSpec questions are unresolved",
            "Resolve the high-impact choices before selecting capabilities.",
            Vec::new(),
        ));
    }

    let spec_bytes = serde_json::to_vec(spec).map_err(|error| {
        plan_error(
            "GameSpec could not be hashed",
            &format!("Fix the schema value: {error}"),
            Vec::new(),
        )
    })?;
    let spec_hash = blake3::hash(&spec_bytes).to_hex().to_string();
    let budget_id = "budget.project".to_owned();
    let budget_card = card(
        &budget_id,
        "Project budgets",
        "Block generation that exceeds the declared frame, memory, content or turn budget.",
        Vec::new(),
        None,
    );
    let mut nodes = vec![CompositionPlanNode {
        id: budget_id.clone(),
        requirement_id: None,
        depends_on: Vec::new(),
        payload: PlanNodePayload::BudgetGuard {
            budgets: spec.constraints.budgets.clone(),
        },
        decision: None,
        card: budget_card,
    }];
    let mut queries = Vec::new();
    let mut selected_all = BTreeSet::new();
    let mut extension_count = 0_u16;

    let mut requirements = spec.requirements();
    requirements.sort_by(|left, right| left.id.cmp(&right.id));
    for requirement in requirements {
        let resolution = resolve_requirement(spec, registry, requirement)?;
        if resolution.decision.strategy == CompositionDecision::Build {
            extension_count = extension_count.saturating_add(1);
            if extension_count > spec.constraints.budgets.max_new_extensions {
                return Err(plan_error(
                    "composition plan exceeds the bounded new-extension budget",
                    "Narrow the game scope or reuse/adapt a registered capability.",
                    resolution.alternatives,
                ));
            }
        }
        selected_all.extend(resolution.capability_ids.iter().cloned());
        queries.push(resolution.query);
        let node_id = format!("resolve.{}", requirement.id);
        let dependencies = requirement
            .depends_on
            .iter()
            .map(|id| format!("resolve.{id}"))
            .chain(std::iter::once(budget_id.clone()))
            .collect::<Vec<_>>();
        let payload = if resolution.decision.strategy == CompositionDecision::Build {
            PlanNodePayload::ProjectExtension {
                extension_id: format!("extension.project.{}", requirement.id),
            }
        } else {
            PlanNodePayload::CapabilitySelection {
                capability_ids: resolution.capability_ids.clone(),
            }
        };
        nodes.push(CompositionPlanNode {
            id: node_id.clone(),
            requirement_id: Some(requirement.id.clone()),
            depends_on: dependencies,
            payload,
            card: card(
                &node_id,
                &requirement.id,
                &requirement.statement,
                resolution.capability_ids.clone(),
                Some(resolution.decision.strategy),
            ),
            decision: Some(resolution.decision),
        });
    }

    let mut contracts = spec.acceptance_mechanics.iter().collect::<Vec<_>>();
    contracts.sort_by(|left, right| left.id.cmp(&right.id));
    for contract in contracts {
        let node_id = format!("test.{}", contract.id);
        let depends_on = contract
            .requirement_ids
            .iter()
            .map(|id| format!("resolve.{id}"))
            .collect::<Vec<_>>();
        nodes.push(CompositionPlanNode {
            id: node_id.clone(),
            requirement_id: None,
            depends_on,
            payload: PlanNodePayload::TestScenario {
                mechanic_contract_id: contract.id.clone(),
            },
            decision: None,
            card: card(&node_id, &contract.id, &contract.promise, Vec::new(), None),
        });
    }
    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    queries.sort_by(|left, right| left.requirement_id.cmp(&right.requirement_id));
    let estimated_card_tokens = nodes.iter().map(|node| node.card.estimated_tokens).sum();
    let selected_capability_ids = selected_all.into_iter().collect::<Vec<_>>();
    let planned_node_ids = nodes.iter().map(|node| node.id.clone()).collect::<Vec<_>>();
    let plan = CompositionPlan {
        format: COMPOSITION_PLAN_FORMAT.to_owned(),
        spec_hash: spec_hash.clone(),
        registry_hash: registry.hash.clone(),
        nodes,
        queries,
        estimated_card_tokens,
        project_state_delta: ProjectStateDelta {
            spec_hash,
            registry_hash: registry.hash.clone(),
            selected_capability_ids,
        },
        checkpoint_delta: TaskCheckpointDelta {
            planned_node_ids,
            unresolved_question_ids: Vec::new(),
        },
    };
    plan.validate(spec.constraints.budgets.turn_tokens)?;
    Ok(plan)
}

struct Resolution {
    capability_ids: Vec<String>,
    decision: DecisionRecord,
    query: RegistryQueryRecord,
    alternatives: Vec<CapabilityCard>,
}

fn resolve_requirement(
    spec: &GameSpec,
    registry: &CapabilityRegistry,
    requirement: &GameRequirement,
) -> std::result::Result<Resolution, CompositionPlanError> {
    let query = CapabilitySearch {
        intent: requirement.statement.clone(),
        max_cost: spec.constraints.budgets.max_capability_cost,
        maturity: requirement.maturity.clone(),
        limit: Some(MAX_SEARCH_LIMIT),
        ..CapabilitySearch::default()
    };
    let strict = registry.search(&query);
    let compatible_cards = strict
        .cards
        .iter()
        .filter(|card| {
            registry.describe(&card.id).is_some_and(|entry| {
                spec.constraints
                    .platforms
                    .iter()
                    .all(|platform| entry.platforms.contains(platform))
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut query_record = RegistryQueryRecord {
        requirement_id: requirement.id.clone(),
        intent: requirement.statement.clone(),
        returned_card_ids: strict.cards.iter().map(|card| card.id.clone()).collect(),
        estimated_tokens: strict.estimated_tokens,
    };

    let (mut selected, strategy, rationale, alternatives) = if !requirement
        .preferred_capabilities
        .is_empty()
    {
        let selected = requirement.preferred_capabilities.clone();
        if selected.iter().any(|id| {
            registry
                .describe(id)
                .is_none_or(|entry| !entry_satisfies(entry, spec, requirement, true))
        }) {
            return Err(plan_error(
                "preferred capabilities are incompatible with the GameSpec",
                "Choose a returned registry alternative or revise the platform/dependency constraint.",
                strict.cards.clone(),
            ));
        }
        (
            selected,
            if requirement.preferred_capabilities.len() == 1 {
                CompositionDecision::Integrate
            } else {
                CompositionDecision::Adapt
            },
            "Used the explicit registered capability selection after compatibility validation."
                .to_owned(),
            strict.cards,
        )
    } else if let Some(best) = compatible_cards.first() {
        let entry = registry.describe(&best.id).ok_or_else(|| {
            plan_error(
                "registry search returned an unknown capability",
                "Rebuild the registry and retry.",
                compatible_cards.clone(),
            )
        })?;
        (
            vec![best.id.clone()],
            if entry.kind == CapabilityKind::Extension {
                CompositionDecision::Wrap
            } else {
                CompositionDecision::Integrate
            },
            if entry.kind == CapabilityKind::Extension {
                "Selected a registered extension through its declared wrapper contract.".to_owned()
            } else {
                "Selected the highest-ranked compatible registered capability.".to_owned()
            },
            compatible_cards.iter().skip(1).cloned().collect(),
        )
    } else {
        let relaxed = registry.search(&CapabilitySearch {
            intent: requirement.statement.clone(),
            max_cost: spec.constraints.budgets.max_capability_cost,
            limit: Some(MAX_SEARCH_LIMIT),
            ..CapabilitySearch::default()
        });
        query_record.returned_card_ids = relaxed.cards.iter().map(|card| card.id.clone()).collect();
        query_record.estimated_tokens = relaxed.estimated_tokens;
        let partial = relaxed.cards.iter().find(|card| {
            registry
                .describe(&card.id)
                .is_some_and(|entry| entry_satisfies(entry, spec, requirement, false))
        });
        if let Some(partial) = partial {
            (
                vec![partial.id.clone()],
                CompositionDecision::Adapt,
                "No capability met the requested maturity; adapt the closest registered contract without editing engine source."
                    .to_owned(),
                relaxed.cards,
            )
        } else {
            (
                Vec::new(),
                CompositionDecision::Build,
                "No compatible registered capability matched; scaffold one bounded project extension."
                    .to_owned(),
                relaxed.cards,
            )
        }
    };

    if strategy != CompositionDecision::Build {
        expand_required_capabilities(registry, &mut selected)?;
        let invalid_platform_or_cost = selected.iter().any(|id| {
            registry.describe(id).is_none_or(|entry| {
                !entry.available
                    || spec
                        .constraints
                        .platforms
                        .iter()
                        .any(|platform| !entry.platforms.contains(platform))
                    || spec
                        .constraints
                        .budgets
                        .max_capability_cost
                        .is_some_and(|cost| entry.cost > cost)
            })
        });
        let graph_invalid = spec
            .constraints
            .platforms
            .iter()
            .any(|platform| !registry.validate_selection(&selected, Some(platform)).valid);
        if invalid_platform_or_cost || graph_invalid {
            return Err(plan_error(
                "selected capability graph is incompatible",
                "Use a compatible registry alternative before any project write.",
                alternatives,
            ));
        }
    }
    selected.sort();
    selected.dedup();
    let selected_cost = selected
        .iter()
        .filter_map(|id| registry.describe(id))
        .map(|entry| entry.cost)
        .max();
    let mut rejected_lower_cost = alternatives
        .iter()
        .filter_map(|card| registry.describe(&card.id))
        .filter(|entry| selected_cost.is_some_and(|cost| entry.cost < cost))
        .map(|entry| RejectedCapability {
            capability_id: entry.id.clone(),
            cost: entry.cost,
            reason: "Ranked lower for intent fit or failed the complete selection contract."
                .to_owned(),
        })
        .collect::<Vec<_>>();
    rejected_lower_cost.sort_by(|left, right| left.capability_id.cmp(&right.capability_id));
    Ok(Resolution {
        capability_ids: selected,
        decision: DecisionRecord {
            strategy,
            rationale,
            rejected_lower_cost,
        },
        query: query_record,
        alternatives,
    })
}

fn entry_satisfies(
    entry: &crate::registry::CapabilityEntry,
    spec: &GameSpec,
    requirement: &GameRequirement,
    require_maturity: bool,
) -> bool {
    entry.available
        && spec
            .constraints
            .platforms
            .iter()
            .all(|platform| entry.platforms.contains(platform))
        && spec
            .constraints
            .budgets
            .max_capability_cost
            .is_none_or(|cost| entry.cost <= cost)
        && (!require_maturity || entry.maturity.satisfies(&requirement.maturity))
}

fn expand_required_capabilities(
    registry: &CapabilityRegistry,
    selected: &mut Vec<String>,
) -> std::result::Result<(), CompositionPlanError> {
    let mut index = 0;
    while index < selected.len() {
        let id = selected[index].clone();
        let entry = registry.describe(&id).ok_or_else(|| {
            plan_error(
                &format!("selected capability {id:?} is not registered"),
                "Select an exact id returned by registry search.",
                Vec::new(),
            )
        })?;
        for relation in &entry.relations {
            if relation.kind == RelationKind::Requires && !selected.contains(&relation.target) {
                selected.push(relation.target.clone());
            }
        }
        index += 1;
    }
    Ok(())
}

fn card(
    id: &str,
    title: &str,
    summary: &str,
    capability_ids: Vec<String>,
    decision: Option<CompositionDecision>,
) -> PlanCard {
    let mut card = PlanCard {
        id: id.to_owned(),
        title: title.to_owned(),
        summary: summary.to_owned(),
        capability_ids,
        decision,
        estimated_tokens: 0,
    };
    card.estimated_tokens = estimate_plan_card(&card);
    card
}

fn estimate_plan_card(card: &PlanCard) -> usize {
    let mut estimate_keywords = card.capability_ids.clone();
    if let Some(decision) = card.decision {
        estimate_keywords.push(format!("{decision:?}"));
    }
    CapabilityCard {
        id: card.id.clone(),
        name: card.title.clone(),
        purpose: card.summary.clone(),
        keywords: estimate_keywords,
        registry_hash: String::new(),
    }
    .estimated_tokens()
}

fn validate_fact(fact: &SpecFact, label: &str) -> Result<()> {
    require_text(&fact.value, label)?;
    validate_confidence(fact.confidence_bps, label)?;
    match fact.certainty {
        FactCertainty::Ambiguous if fact.alternatives.len() < 2 => Err(spec_error(
            format!("ambiguous {label} needs at least two alternatives"),
            "Record the real options or mark the fact assumed/certain.".to_owned(),
        )),
        FactCertainty::Certain | FactCertainty::Assumed if !fact.alternatives.is_empty() => {
            Err(spec_error(
                format!("non-ambiguous {label} carries unused alternatives"),
                "Clear alternatives or mark the fact ambiguous.".to_owned(),
            ))
        }
        _ if fact
            .alternatives
            .iter()
            .any(|value| value.trim().is_empty())
            || fact.alternatives.iter().collect::<BTreeSet<_>>().len()
                != fact.alternatives.len() =>
        {
            Err(spec_error(
                format!("{label} alternatives must be unique and non-empty"),
                "Keep only distinct concrete alternatives.".to_owned(),
            ))
        }
        _ => Ok(()),
    }
}

fn validate_questions(questions: &[OpenQuestion], requirements: &BTreeSet<&str>) -> Result<()> {
    let mut ids = BTreeSet::new();
    for question in questions {
        validate_id(&question.id, "open question")?;
        require_text(&question.question, "open question")?;
        if !ids.insert(question.id.as_str()) {
            return Err(spec_error(
                format!("duplicate open-question id {:?}", question.id),
                "Give every material decision one stable id.".to_owned(),
            ));
        }
        if question.affects_requirements.is_empty()
            || question.options.len() < 2
            || question
                .options
                .iter()
                .any(|option| option.trim().is_empty())
            || question.options.iter().collect::<BTreeSet<_>>().len() != question.options.len()
            || question
                .affects_requirements
                .iter()
                .any(|id| !requirements.contains(id.as_str()))
        {
            return Err(spec_error(
                format!("open question {:?} is not materially grounded", question.id),
                "Name affected requirements and at least two concrete options.".to_owned(),
            ));
        }
        if let Some(answer) = &question.resolved {
            if !question.options.contains(answer) {
                return Err(spec_error(
                    format!("question {:?} resolved to an unknown option", question.id),
                    "Choose one of the recorded material options.".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_budgets(budgets: &PlanningBudgets) -> Result<()> {
    if budgets.turn_tokens == 0 {
        return Err(spec_error(
            "turn-token budget must be positive".to_owned(),
            "Set the maximum review-card tokens available for this plan.".to_owned(),
        ));
    }
    if [
        budgets.frame_time_micros,
        budgets.max_memory_mb,
        budgets.max_content_mb,
    ]
    .into_iter()
    .flatten()
    .any(|value| value == 0)
    {
        return Err(spec_error(
            "declared frame, memory and content budgets must be positive".to_owned(),
            "Use a positive bound or omit the budget when it is intentionally unspecified."
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_confidence(value: u16, label: &str) -> Result<()> {
    if value > CONFIDENCE_BPS_MAX {
        Err(spec_error(
            format!("{label} confidence {value} exceeds {CONFIDENCE_BPS_MAX} basis points"),
            format!("Use 0 through {CONFIDENCE_BPS_MAX} basis points."),
        ))
    } else {
        Ok(())
    }
}

fn require_text(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(spec_error(
            format!("{label} must not be empty"),
            format!("Provide a concrete {label}."),
        ))
    } else {
        Ok(())
    }
}

fn validate_id(id: &str, label: &str) -> Result<()> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_');
    if valid {
        Ok(())
    } else {
        Err(spec_error(
            format!("{label} id {id:?} is not canonical"),
            "Use lowercase letters, digits and underscores.".to_owned(),
        ))
    }
}

fn reject_requirement_cycles(requirements: &[&GameRequirement]) -> Result<()> {
    let edges = requirements
        .iter()
        .map(|requirement| (requirement.id.as_str(), requirement.depends_on.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let mut active = BTreeSet::new();
    let mut complete = BTreeSet::new();
    for id in edges.keys().copied() {
        visit_requirement(id, &edges, &mut active, &mut complete)?;
    }
    Ok(())
}

fn visit_requirement<'a>(
    id: &'a str,
    edges: &BTreeMap<&'a str, &'a [String]>,
    active: &mut BTreeSet<&'a str>,
    complete: &mut BTreeSet<&'a str>,
) -> Result<()> {
    if complete.contains(id) {
        return Ok(());
    }
    if !active.insert(id) {
        return Err(spec_error(
            format!("GameSpec requirement dependency cycle reaches {id:?}"),
            "Remove the cycle so the composition plan is a DAG.".to_owned(),
        ));
    }
    if let Some(next) = edges.get(id) {
        for target in *next {
            visit_requirement(target, edges, active, complete)?;
        }
    }
    active.remove(id);
    complete.insert(id);
    Ok(())
}

fn reject_plan_cycles(
    nodes: &[CompositionPlanNode],
) -> std::result::Result<(), CompositionPlanError> {
    let edges = nodes
        .iter()
        .map(|node| (node.id.as_str(), node.depends_on.as_slice()))
        .collect::<BTreeMap<_, _>>();
    let mut active = BTreeSet::new();
    let mut complete = BTreeSet::new();
    for id in edges.keys().copied() {
        visit_plan(id, &edges, &mut active, &mut complete)?;
    }
    Ok(())
}

fn visit_plan<'a>(
    id: &'a str,
    edges: &BTreeMap<&'a str, &'a [String]>,
    active: &mut BTreeSet<&'a str>,
    complete: &mut BTreeSet<&'a str>,
) -> std::result::Result<(), CompositionPlanError> {
    if complete.contains(id) {
        return Ok(());
    }
    if !active.insert(id) {
        return Err(plan_error(
            "composition plan dependency cycle detected",
            "Remove the cycle before approval.",
            Vec::new(),
        ));
    }
    if let Some(next) = edges.get(id) {
        for target in *next {
            visit_plan(target, edges, active, complete)?;
        }
    }
    active.remove(id);
    complete.insert(id);
    Ok(())
}

fn spec_error(message: String, hint: String) -> EngineError {
    EngineError::Schema(message, Some(hint))
}

fn plan_error(
    message: &str,
    hint: &str,
    alternatives: Vec<CapabilityCard>,
) -> CompositionPlanError {
    CompositionPlanError {
        message: message.to_owned(),
        hint: hint.to_owned(),
        alternatives,
    }
}
