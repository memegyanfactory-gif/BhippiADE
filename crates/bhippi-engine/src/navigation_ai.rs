//! Bounded navigation-query and deterministic gameplay-AI contracts.
//!
//! No navmesh backend is selected here. An unconfigured backend reports unsupported rather
//! than fabricating reachability.

use crate::error::{EngineError, Result};
use crate::gameplay_contract::BASIS_POINTS_MAX;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const GAMEPLAY_AI_FORMAT: &str = "bhippi-gameplay-ai@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
pub struct NavPointMm(pub [i32; 3]);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum NavigationBackend {
    Unconfigured,
    Registered {
        capability_id: String,
        backend_version: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct NavigationArea {
    pub id: String,
    pub traversal_cost_milli: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct OffMeshLink {
    pub id: String,
    pub from: NavPointMm,
    pub to: NavPointMm,
    pub area: String,
    pub bidirectional: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct NavigationQueryLimits {
    pub max_visited_nodes: u32,
    pub max_waypoints: u32,
    pub max_total_cost_milli: u64,
}

impl NavigationQueryLimits {
    pub fn validate(self) -> Result<()> {
        if self.max_visited_nodes == 0 || self.max_waypoints < 2 || self.max_total_cost_milli == 0 {
            return Err(ai_error(
                "navigation query limits must be finite and positive".to_owned(),
                "Allow at least two waypoints and positive node/cost bounds.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PathQuery {
    pub request_id: String,
    pub start: NavPointMm,
    pub goal: NavPointMm,
    pub allowed_areas: Vec<String>,
    pub blocked_obstacle_ids: Vec<String>,
    pub allow_partial: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PathStatus {
    Complete,
    Partial,
    Unreachable,
    UnsupportedBackend,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PathResult {
    pub request_id: String,
    pub status: PathStatus,
    pub waypoints: Vec<NavPointMm>,
    pub total_cost_milli: u64,
    pub visited_nodes: u32,
    pub reason: String,
}

impl PathResult {
    pub fn validate(&self, query: &PathQuery, limits: NavigationQueryLimits) -> Result<()> {
        limits.validate()?;
        if self.request_id != query.request_id {
            return Err(ai_error(
                "path result request id does not match its query".to_owned(),
                "Return the exact request id from the bounded path query.",
            ));
        }
        if self.waypoints.len() > limits.max_waypoints as usize
            || self.visited_nodes > limits.max_visited_nodes
            || self.total_cost_milli > limits.max_total_cost_milli
        {
            return Err(ai_error(
                "path result exceeds its declared query budget".to_owned(),
                "Return a bounded partial or unreachable result.",
            ));
        }
        match self.status {
            PathStatus::Complete => {
                if self.waypoints.first() != Some(&query.start)
                    || self.waypoints.last() != Some(&query.goal)
                {
                    return Err(ai_error(
                        "complete path does not connect the requested endpoints".to_owned(),
                        "A complete result must begin at start and end at goal.",
                    ));
                }
            }
            PathStatus::Partial => {
                if !query.allow_partial
                    || self.waypoints.first() != Some(&query.start)
                    || self.waypoints.len() < 2
                {
                    return Err(ai_error(
                        "partial path is not permitted or grounded at the start".to_owned(),
                        "Return unreachable when partial paths are disabled.",
                    ));
                }
            }
            PathStatus::Unreachable | PathStatus::UnsupportedBackend => {
                if !self.waypoints.is_empty() || self.reason.trim().is_empty() {
                    return Err(ai_error(
                        "failed path result must be empty and explain why".to_owned(),
                        "Clear waypoints and record the stable failure reason.",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct NavigationContract {
    pub backend: NavigationBackend,
    pub limits: NavigationQueryLimits,
    pub areas: Vec<NavigationArea>,
    #[serde(default)]
    pub off_mesh_links: Vec<OffMeshLink>,
}

impl NavigationContract {
    pub fn validate(&self) -> Result<()> {
        self.limits.validate()?;
        let mut areas = BTreeSet::new();
        for area in &self.areas {
            require_text(&area.id, "navigation area")?;
            if area.traversal_cost_milli == 0 || !areas.insert(area.id.as_str()) {
                return Err(ai_error(
                    format!(
                        "navigation area {:?} is duplicate or has zero cost",
                        area.id
                    ),
                    "Give every area one id and a positive traversal cost.",
                ));
            }
        }
        if self.areas.is_empty() {
            return Err(ai_error(
                "navigation contract has no areas".to_owned(),
                "Declare at least one traversable area contract.",
            ));
        }
        let mut links = BTreeSet::new();
        for link in &self.off_mesh_links {
            require_text(&link.id, "off-mesh link id")?;
            if !links.insert(link.id.as_str()) || !areas.contains(link.area.as_str()) {
                return Err(ai_error(
                    format!(
                        "off-mesh link {:?} is duplicate or uses an unknown area",
                        link.id
                    ),
                    "Use a unique link id and a declared area.",
                ));
            }
        }
        if let NavigationBackend::Registered {
            capability_id,
            backend_version,
        } = &self.backend
        {
            require_text(capability_id, "navigation backend capability")?;
            require_text(backend_version, "navigation backend version")?;
        }
        Ok(())
    }

    pub fn unsupported_result(&self, query: &PathQuery) -> Result<PathResult> {
        self.validate()?;
        require_text(&query.request_id, "path request id")?;
        if query.allowed_areas.is_empty()
            || query
                .allowed_areas
                .iter()
                .any(|area| !self.areas.iter().any(|declared| declared.id == *area))
        {
            return Err(ai_error(
                "path query has no areas or references an undeclared area".to_owned(),
                "Select at least one area from the navigation contract.",
            ));
        }
        match self.backend {
            NavigationBackend::Unconfigured => Ok(PathResult {
                request_id: query.request_id.clone(),
                status: PathStatus::UnsupportedBackend,
                waypoints: Vec::new(),
                total_cost_milli: 0,
                visited_nodes: 0,
                reason: "No registered navigation backend is configured.".to_owned(),
            }),
            NavigationBackend::Registered { .. } => Err(ai_error(
                "registered navigation backend must answer through its bounded adapter".to_owned(),
                "Dispatch to the backend and validate its PathResult.",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BlackboardKind {
    Boolean,
    Integer,
    Entity,
    Point,
    Text,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum BlackboardValue {
    Boolean(bool),
    Integer(i64),
    Entity(String),
    Point(NavPointMm),
    Text(String),
}

impl BlackboardValue {
    #[must_use]
    pub const fn kind(&self) -> BlackboardKind {
        match self {
            Self::Boolean(_) => BlackboardKind::Boolean,
            Self::Integer(_) => BlackboardKind::Integer,
            Self::Entity(_) => BlackboardKind::Entity,
            Self::Point(_) => BlackboardKind::Point,
            Self::Text(_) => BlackboardKind::Text,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct BlackboardField {
    pub key: String,
    pub kind: BlackboardKind,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct Blackboard {
    pub values: BTreeMap<String, BlackboardValue>,
}

impl Blackboard {
    pub fn validate(&self, schema: &[BlackboardField]) -> Result<()> {
        validate_blackboard_schema(schema)?;
        let fields = schema
            .iter()
            .map(|field| (field.key.as_str(), field.kind))
            .collect::<BTreeMap<_, _>>();
        for (key, value) in &self.values {
            let expected = fields.get(key.as_str()).ok_or_else(|| {
                ai_error(
                    format!("blackboard contains undeclared key {key:?}"),
                    "Declare the key in the blackboard schema.",
                )
            })?;
            if *expected != value.kind() {
                return Err(ai_error(
                    format!("blackboard key {key:?} has the wrong value kind"),
                    "Write a value matching the declared blackboard kind.",
                ));
            }
        }
        Ok(())
    }

    pub fn set(
        &mut self,
        schema: &[BlackboardField],
        key: &str,
        value: BlackboardValue,
    ) -> Result<()> {
        let field = schema
            .iter()
            .find(|field| field.key == key)
            .ok_or_else(|| {
                ai_error(
                    format!("blackboard key {key:?} is not declared"),
                    "Add the key to the schema before writing it.",
                )
            })?;
        if field.kind != value.kind() {
            return Err(ai_error(
                format!("blackboard key {key:?} rejects that value kind"),
                "Use the declared blackboard value type.",
            ));
        }
        self.values.insert(key.to_owned(), value);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ValueComparison {
    Equal,
    NotEqual,
    GreaterOrEqual,
    LessOrEqual,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct BlackboardCondition {
    pub key: String,
    pub comparison: ValueComparison,
    pub expected: BlackboardValue,
}

impl BlackboardCondition {
    pub fn evaluate(&self, blackboard: &Blackboard) -> Result<bool> {
        let actual = blackboard.values.get(&self.key).ok_or_else(|| {
            ai_error(
                format!("condition reads missing blackboard key {:?}", self.key),
                "Initialise the key before evaluating AI behavior.",
            )
        })?;
        if actual.kind() != self.expected.kind() {
            return Err(ai_error(
                format!(
                    "condition for {:?} compares different value kinds",
                    self.key
                ),
                "Compare values of the same blackboard kind.",
            ));
        }
        match self.comparison {
            ValueComparison::Equal => Ok(actual == &self.expected),
            ValueComparison::NotEqual => Ok(actual != &self.expected),
            ValueComparison::GreaterOrEqual | ValueComparison::LessOrEqual => {
                let (BlackboardValue::Integer(actual), BlackboardValue::Integer(expected)) =
                    (actual, &self.expected)
                else {
                    return Err(ai_error(
                        "ordered blackboard comparison requires integers".to_owned(),
                        "Use equal/not_equal for non-integer values.",
                    ));
                };
                Ok(match self.comparison {
                    ValueComparison::GreaterOrEqual => actual >= expected,
                    ValueComparison::LessOrEqual => actual <= expected,
                    ValueComparison::Equal | ValueComparison::NotEqual => false,
                })
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct StateTransition {
    pub target: String,
    pub priority: u16,
    pub conditions: Vec<BlackboardCondition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct AiState {
    pub id: String,
    pub transitions: Vec<StateTransition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct StateMachine {
    pub initial: String,
    pub states: Vec<AiState>,
}

impl StateMachine {
    pub fn validate(&self) -> Result<()> {
        let ids = self
            .states
            .iter()
            .map(|state| state.id.as_str())
            .collect::<BTreeSet<_>>();
        if ids.len() != self.states.len() || !ids.contains(self.initial.as_str()) {
            return Err(ai_error(
                "AI state machine has duplicate states or a missing initial state".to_owned(),
                "Use unique state ids and point initial at one of them.",
            ));
        }
        for state in &self.states {
            require_text(&state.id, "AI state id")?;
            let mut priorities = BTreeSet::new();
            for transition in &state.transitions {
                if !ids.contains(transition.target.as_str())
                    || transition.conditions.is_empty()
                    || !priorities.insert(transition.priority)
                {
                    return Err(ai_error(
                        format!(
                            "state {:?} has an ambiguous or dangling transition",
                            state.id
                        ),
                        "Use unique priorities, a real target and at least one condition.",
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn step(&self, current: &str, blackboard: &Blackboard) -> Result<String> {
        self.validate()?;
        let state = self
            .states
            .iter()
            .find(|state| state.id == current)
            .ok_or_else(|| {
                ai_error(
                    format!("current AI state {current:?} is not declared"),
                    "Reset to the state machine initial state.",
                )
            })?;
        let mut transitions = state.transitions.iter().collect::<Vec<_>>();
        transitions.sort_by_key(|transition| transition.priority);
        for transition in transitions {
            let mut passes = true;
            for condition in &transition.conditions {
                if !condition.evaluate(blackboard)? {
                    passes = false;
                    break;
                }
            }
            if passes {
                return Ok(transition.target.clone());
            }
        }
        Ok(current.to_owned())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionKind {
    Sight,
    Hearing,
    Damage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PerceptionObservation {
    pub id: String,
    pub kind: PerceptionKind,
    pub subject: String,
    pub position: NavPointMm,
    pub strength_bps: u32,
    pub observed_tick: u64,
    pub expires_tick: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PerceptionLimits {
    pub max_observations: u32,
    pub max_lifetime_ticks: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PerceptionMemory {
    pub observations: Vec<PerceptionObservation>,
}

impl PerceptionMemory {
    pub fn observe(
        &mut self,
        observation: PerceptionObservation,
        now_tick: u64,
        limits: PerceptionLimits,
    ) -> Result<()> {
        if limits.max_observations == 0 || limits.max_lifetime_ticks == 0 {
            return Err(ai_error(
                "perception limits must be positive".to_owned(),
                "Set bounded observation count and lifetime.",
            ));
        }
        require_text(&observation.id, "perception observation id")?;
        require_text(&observation.subject, "perception subject")?;
        if observation.strength_bps > BASIS_POINTS_MAX
            || observation.observed_tick > now_tick
            || observation.expires_tick <= now_tick
            || observation.expires_tick - observation.observed_tick > limits.max_lifetime_ticks
        {
            return Err(ai_error(
                format!(
                    "perception observation {:?} exceeds its bounds",
                    observation.id
                ),
                "Use current time, bounded strength and a bounded future expiry.",
            ));
        }
        self.expire(now_tick);
        if let Some(existing) = self
            .observations
            .iter_mut()
            .find(|existing| existing.id == observation.id)
        {
            *existing = observation;
        } else {
            if self.observations.len() >= limits.max_observations as usize {
                return Err(ai_error(
                    "perception memory reached its observation cap".to_owned(),
                    "Expire or replace evidence before adding another observation.",
                ));
            }
            self.observations.push(observation);
        }
        self.observations.sort_by(|left, right| {
            right
                .strength_bps
                .cmp(&left.strength_bps)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(())
    }

    pub fn expire(&mut self, now_tick: u64) {
        self.observations
            .retain(|observation| observation.expires_tick > now_tick);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AiAction {
    Idle,
    Patrol,
    Chase,
    TakeCover,
    Combat,
    Flee,
    Investigate,
    Squad,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BehaviorNodeKind {
    Sequence { children: Vec<String> },
    Selector { children: Vec<String> },
    Condition { condition: BlackboardCondition },
    Action { action: AiAction },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct BehaviorNode {
    pub id: String,
    #[serde(flatten)]
    pub node: BehaviorNodeKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct BehaviorLimits {
    pub max_nodes: u32,
    pub max_depth: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct BehaviorTree {
    pub root: String,
    pub nodes: Vec<BehaviorNode>,
    pub limits: BehaviorLimits,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct BehaviorDecision {
    pub action: Option<AiAction>,
    pub visited_nodes: u32,
}

impl BehaviorTree {
    pub fn validate(&self) -> Result<()> {
        if self.limits.max_nodes == 0
            || self.limits.max_depth == 0
            || self.nodes.len() > self.limits.max_nodes as usize
        {
            return Err(ai_error(
                "behavior tree exceeds its positive node/depth bounds".to_owned(),
                "Reduce the graph or raise the explicit bounded contract.",
            ));
        }
        let ids = self
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<BTreeSet<_>>();
        if ids.len() != self.nodes.len() || !ids.contains(self.root.as_str()) {
            return Err(ai_error(
                "behavior tree has duplicate nodes or a missing root".to_owned(),
                "Use unique node ids and a declared root.",
            ));
        }
        for node in &self.nodes {
            require_text(&node.id, "behavior node id")?;
            if let BehaviorNodeKind::Sequence { children }
            | BehaviorNodeKind::Selector { children } = &node.node
            {
                if children.is_empty() || children.iter().any(|child| !ids.contains(child.as_str()))
                {
                    return Err(ai_error(
                        format!("behavior node {:?} has empty or dangling children", node.id),
                        "Point every composite at declared child nodes.",
                    ));
                }
            }
        }
        let by_id = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        validate_behavior_graph(&self.root, &by_id, self.limits)
    }

    pub fn decide(&self, blackboard: &Blackboard) -> Result<BehaviorDecision> {
        self.validate()?;
        let by_id = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let mut visited = 0;
        let outcome =
            evaluate_behavior(&self.root, &by_id, blackboard, &mut visited, 1, self.limits)?;
        Ok(BehaviorDecision {
            action: match outcome {
                EvalOutcome::Action(action) => Some(action),
                EvalOutcome::Pass | EvalOutcome::Fail => None,
            },
            visited_nodes: visited,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct UtilityConsideration {
    pub key: String,
    pub minimum: i64,
    pub maximum: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct UtilityOption {
    pub id: String,
    pub action: AiAction,
    pub considerations: Vec<UtilityConsideration>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct UtilityDecision {
    pub option: String,
    pub action: AiAction,
    pub score_bps: u32,
}

pub fn decide_utility(
    options: &[UtilityOption],
    blackboard: &Blackboard,
) -> Result<UtilityDecision> {
    if options.is_empty() {
        return Err(ai_error(
            "utility AI has no options".to_owned(),
            "Declare at least one bounded action option.",
        ));
    }
    let mut scored = Vec::new();
    for option in options {
        require_text(&option.id, "utility option id")?;
        if option.considerations.is_empty() {
            return Err(ai_error(
                format!("utility option {:?} has no considerations", option.id),
                "Ground each utility action in blackboard evidence.",
            ));
        }
        let mut total = 0_u64;
        for consideration in &option.considerations {
            if consideration.maximum <= consideration.minimum {
                return Err(ai_error(
                    format!(
                        "utility consideration {:?} has an empty range",
                        consideration.key
                    ),
                    "Set maximum above minimum.",
                ));
            }
            let Some(BlackboardValue::Integer(value)) = blackboard.values.get(&consideration.key)
            else {
                return Err(ai_error(
                    format!(
                        "utility consideration reads missing integer {:?}",
                        consideration.key
                    ),
                    "Initialise the integer blackboard key.",
                ));
            };
            let clamped = (*value).clamp(consideration.minimum, consideration.maximum);
            let numerator = u64::try_from(clamped - consideration.minimum).unwrap_or(0);
            let denominator =
                u64::try_from(consideration.maximum - consideration.minimum).unwrap_or(1);
            total = total.saturating_add(
                numerator.saturating_mul(u64::from(BASIS_POINTS_MAX)) / denominator,
            );
        }
        let count = u64::try_from(option.considerations.len()).unwrap_or(1);
        scored.push((
            u32::try_from(total / count).unwrap_or(BASIS_POINTS_MAX),
            option.id.as_str(),
            option,
        ));
    }
    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(right.1)));
    let (score_bps, _, option) = scored[0];
    Ok(UtilityDecision {
        option: option.id.clone(),
        action: option.action,
        score_bps,
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GameplayAiDocument {
    pub format: String,
    pub navigation: NavigationContract,
    pub blackboard_schema: Vec<BlackboardField>,
    pub state_machine: StateMachine,
    pub behavior_tree: BehaviorTree,
    pub perception_limits: PerceptionLimits,
    #[serde(default)]
    pub utility_options: Vec<UtilityOption>,
}

impl GameplayAiDocument {
    pub fn parse(text: &str) -> Result<Self> {
        let document: Self = serde_json::from_str(text).map_err(|error| {
            ai_error(
                format!("invalid gameplay AI contract: {error}"),
                "Fix the JSON and restore the supported format marker.",
            )
        })?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != GAMEPLAY_AI_FORMAT {
            return Err(ai_error(
                format!("unsupported gameplay AI format {:?}", self.format),
                &format!("Use {GAMEPLAY_AI_FORMAT}; unknown major versions block."),
            ));
        }
        self.navigation.validate()?;
        validate_blackboard_schema(&self.blackboard_schema)?;
        self.state_machine.validate()?;
        self.behavior_tree.validate()?;
        validate_ai_bindings(
            &self.blackboard_schema,
            &self.state_machine,
            &self.behavior_tree,
            &self.utility_options,
        )?;
        if self.perception_limits.max_observations == 0
            || self.perception_limits.max_lifetime_ticks == 0
        {
            return Err(ai_error(
                "perception limits must be positive".to_owned(),
                "Set bounded memory count and lifetime.",
            ));
        }
        let mut utility_ids = BTreeSet::new();
        for option in &self.utility_options {
            if !utility_ids.insert(option.id.as_str()) {
                return Err(ai_error(
                    format!("duplicate utility option {:?}", option.id),
                    "Give every utility option one stable id.",
                ));
            }
        }
        Ok(())
    }
}

fn validate_blackboard_schema(schema: &[BlackboardField]) -> Result<()> {
    let mut keys = BTreeSet::new();
    for field in schema {
        require_text(&field.key, "blackboard field")?;
        if !keys.insert(field.key.as_str()) {
            return Err(ai_error(
                format!("duplicate blackboard field {:?}", field.key),
                "Declare each typed key once.",
            ));
        }
    }
    Ok(())
}

fn validate_ai_bindings(
    schema: &[BlackboardField],
    state_machine: &StateMachine,
    behavior_tree: &BehaviorTree,
    utility_options: &[UtilityOption],
) -> Result<()> {
    let fields = schema
        .iter()
        .map(|field| (field.key.as_str(), field.kind))
        .collect::<BTreeMap<_, _>>();
    let conditions = state_machine
        .states
        .iter()
        .flat_map(|state| &state.transitions)
        .flat_map(|transition| &transition.conditions)
        .chain(
            behavior_tree
                .nodes
                .iter()
                .filter_map(|node| match &node.node {
                    BehaviorNodeKind::Condition { condition } => Some(condition),
                    BehaviorNodeKind::Sequence { .. }
                    | BehaviorNodeKind::Selector { .. }
                    | BehaviorNodeKind::Action { .. } => None,
                }),
        );
    for condition in conditions {
        if fields
            .get(condition.key.as_str())
            .is_none_or(|kind| *kind != condition.expected.kind())
        {
            return Err(ai_error(
                format!(
                    "AI condition uses undeclared or mistyped key {:?}",
                    condition.key
                ),
                "Bind every condition to the versioned blackboard schema.",
            ));
        }
    }
    for consideration in utility_options
        .iter()
        .flat_map(|option| &option.considerations)
    {
        if fields.get(consideration.key.as_str()) != Some(&BlackboardKind::Integer) {
            return Err(ai_error(
                format!(
                    "utility AI uses non-integer or unknown key {:?}",
                    consideration.key
                ),
                "Bind utility ranges to declared integer blackboard fields.",
            ));
        }
    }
    Ok(())
}

fn validate_behavior_graph(
    root: &str,
    nodes: &BTreeMap<&str, &BehaviorNode>,
    limits: BehaviorLimits,
) -> Result<()> {
    fn visit<'a>(
        id: &'a str,
        nodes: &BTreeMap<&'a str, &'a BehaviorNode>,
        active: &mut BTreeSet<&'a str>,
        complete: &mut BTreeSet<&'a str>,
        depth: u32,
        limits: BehaviorLimits,
    ) -> Result<()> {
        if depth > limits.max_depth {
            return Err(ai_error(
                "behavior tree exceeds its depth budget".to_owned(),
                "Flatten the tree or raise the explicit depth.",
            ));
        }
        if complete.contains(id) {
            return Ok(());
        }
        if !active.insert(id) {
            return Err(ai_error(
                format!("behavior tree cycle reaches {id:?}"),
                "Remove the cycle.",
            ));
        }
        let node = nodes.get(id).ok_or_else(|| {
            ai_error(
                format!("behavior node {id:?} is missing"),
                "Restore the referenced node.",
            )
        })?;
        if let BehaviorNodeKind::Sequence { children } | BehaviorNodeKind::Selector { children } =
            &node.node
        {
            for child in children {
                visit(child, nodes, active, complete, depth + 1, limits)?;
            }
        }
        active.remove(id);
        complete.insert(id);
        Ok(())
    }

    visit(
        root,
        nodes,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        1,
        limits,
    )
}

#[derive(Clone, Copy)]
enum EvalOutcome {
    Pass,
    Fail,
    Action(AiAction),
}

fn evaluate_behavior(
    id: &str,
    nodes: &BTreeMap<&str, &BehaviorNode>,
    blackboard: &Blackboard,
    visited: &mut u32,
    depth: u32,
    limits: BehaviorLimits,
) -> Result<EvalOutcome> {
    *visited = visited.saturating_add(1);
    if *visited > limits.max_nodes || depth > limits.max_depth {
        return Err(ai_error(
            "behavior evaluation exceeded its node/depth budget".to_owned(),
            "Return a behavior fault instead of continuing.",
        ));
    }
    let node = nodes.get(id).ok_or_else(|| {
        ai_error(
            format!("behavior node {id:?} is missing"),
            "Restore the referenced node.",
        )
    })?;
    match &node.node {
        BehaviorNodeKind::Condition { condition } => Ok(if condition.evaluate(blackboard)? {
            EvalOutcome::Pass
        } else {
            EvalOutcome::Fail
        }),
        BehaviorNodeKind::Action { action } => Ok(EvalOutcome::Action(*action)),
        BehaviorNodeKind::Sequence { children } => {
            for child in children {
                match evaluate_behavior(child, nodes, blackboard, visited, depth + 1, limits)? {
                    EvalOutcome::Fail => return Ok(EvalOutcome::Fail),
                    EvalOutcome::Action(action) => return Ok(EvalOutcome::Action(action)),
                    EvalOutcome::Pass => {}
                }
            }
            Ok(EvalOutcome::Pass)
        }
        BehaviorNodeKind::Selector { children } => {
            for child in children {
                match evaluate_behavior(child, nodes, blackboard, visited, depth + 1, limits)? {
                    EvalOutcome::Fail => {}
                    EvalOutcome::Action(action) => return Ok(EvalOutcome::Action(action)),
                    EvalOutcome::Pass => return Ok(EvalOutcome::Pass),
                }
            }
            Ok(EvalOutcome::Fail)
        }
    }
}

fn require_text(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(ai_error(
            format!("{label} must not be empty"),
            &format!("Provide a stable {label}."),
        ))
    } else {
        Ok(())
    }
}

fn ai_error(message: String, hint: &str) -> EngineError {
    EngineError::Schema(message, Some(hint.to_owned()))
}
