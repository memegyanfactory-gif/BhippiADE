//! Versioned typed behavior-graph and safe compiled-action contracts (Phase 22).
//!
//! Compilation produces deterministic inert instructions. This module does not execute them,
//! host a node editor, run breakpoints or expose a runtime debugger.

use crate::error::{EngineError, Result};
use crate::registry::CapabilityRegistry;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const BEHAVIOR_GRAPH_FORMAT: &str = "bhippi-behavior-graph@1";
pub const BEHAVIOR_BYTECODE_FORMAT: &str = "bhippi-behavior-bytecode@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GraphValueType {
    Flow,
    Bool,
    Number,
    String,
    Vec3,
    Entity,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum GraphValue {
    Bool(bool),
    Number(f64),
    String(String),
    Vec3([f32; 3]),
    Entity(String),
}

impl GraphValue {
    #[must_use]
    pub const fn value_type(&self) -> GraphValueType {
        match self {
            Self::Bool(_) => GraphValueType::Bool,
            Self::Number(_) => GraphValueType::Number,
            Self::String(_) => GraphValueType::String,
            Self::Vec3(_) => GraphValueType::Vec3,
            Self::Entity(_) => GraphValueType::Entity,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GraphPortContract {
    pub id: String,
    pub value_type: GraphValueType,
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum BehaviorNodeKind {
    Event {
        event: String,
    },
    Constant {
        output: String,
    },
    Branch,
    Sequence,
    ReadVariable {
        name: String,
    },
    WriteVariable {
        name: String,
    },
    DispatchAction {
        capability_id: String,
        action_kind: String,
    },
    EmitEvent {
        event: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct BehaviorNodeContract {
    pub id: String,
    pub kind: BehaviorNodeKind,
    #[serde(default)]
    pub inputs: Vec<GraphPortContract>,
    #[serde(default)]
    pub outputs: Vec<GraphPortContract>,
    #[serde(default)]
    pub literals: BTreeMap<String, GraphValue>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GraphEdgeContract {
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GraphDebugContract {
    #[serde(default)]
    pub breakpoints: Vec<String>,
    #[serde(default)]
    pub watch_values: Vec<String>,
    pub trace_capacity: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct BehaviorGraphDocument {
    pub format: String,
    pub id: String,
    pub capability_registry_hash: String,
    pub variables: BTreeMap<String, GraphValueType>,
    pub nodes: Vec<BehaviorNodeContract>,
    pub edges: Vec<GraphEdgeContract>,
    pub debug: GraphDebugContract,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "opcode", content = "data", rename_all = "snake_case")]
pub enum GraphInstruction {
    ReceiveEvent {
        node: String,
        event: String,
    },
    LoadConstant {
        node: String,
        port: String,
    },
    Branch {
        node: String,
    },
    Sequence {
        node: String,
    },
    ReadVariable {
        node: String,
        name: String,
    },
    WriteVariable {
        node: String,
        name: String,
    },
    DispatchAction {
        node: String,
        capability_id: String,
        action_kind: String,
    },
    EmitEvent {
        node: String,
        event: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CompiledBehaviorGraph {
    pub format: String,
    pub graph_id: String,
    pub source_hash: String,
    pub capability_registry_hash: String,
    pub instructions: Vec<GraphInstruction>,
    pub breakpoints: Vec<usize>,
    pub watched_variables: Vec<String>,
}

impl BehaviorGraphDocument {
    pub fn compile(&self, registry: &CapabilityRegistry) -> Result<CompiledBehaviorGraph> {
        let order = self.validate(registry)?;
        let nodes = self
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect::<BTreeMap<_, _>>();
        let mut instructions = Vec::with_capacity(order.len());
        let mut instruction_by_node = BTreeMap::new();
        for node_id in order {
            let Some(node) = nodes.get(node_id) else {
                return Err(error(
                    "validated graph lost a node during compilation",
                    "Report this as an engine bug.",
                ));
            };
            let instruction = instruction_for(node)?;
            instruction_by_node.insert(node.id.as_str(), instructions.len());
            instructions.push(instruction);
        }
        let breakpoints = self
            .debug
            .breakpoints
            .iter()
            .filter_map(|id| instruction_by_node.get(id.as_str()).copied())
            .collect();
        let bytes = serde_json::to_vec(self).map_err(|failure| {
            error(
                &format!("behavior graph could not be hashed: {failure}"),
                "Fix the graph document and compile again.",
            )
        })?;
        Ok(CompiledBehaviorGraph {
            format: BEHAVIOR_BYTECODE_FORMAT.to_owned(),
            graph_id: self.id.clone(),
            source_hash: blake3::hash(&bytes).to_hex().to_string(),
            capability_registry_hash: registry.hash.clone(),
            instructions,
            breakpoints,
            watched_variables: self.debug.watch_values.clone(),
        })
    }

    fn validate<'a>(&'a self, registry: &CapabilityRegistry) -> Result<Vec<&'a str>> {
        if self.format != BEHAVIOR_GRAPH_FORMAT || self.capability_registry_hash != registry.hash {
            return Err(error(
                "behavior graph format or registry hash is stale",
                "Use bhippi-behavior-graph@1 and rebuild against the active registry.",
            ));
        }
        validate_id(&self.id)?;
        if self.debug.trace_capacity == 0 {
            return Err(error(
                "behavior trace is unbounded/disabled",
                "Declare a non-zero bounded trace capacity.",
            ));
        }
        let mut nodes = BTreeMap::new();
        for node in &self.nodes {
            validate_node(node, &self.variables, registry)?;
            if nodes.insert(node.id.as_str(), node).is_some() {
                return Err(error(
                    "duplicate behavior node id",
                    "Use one stable node id.",
                ));
            }
        }
        for id in self
            .debug
            .breakpoints
            .iter()
            .chain(&self.debug.watch_values)
        {
            let exists = nodes.contains_key(id.as_str()) || self.variables.contains_key(id);
            if !exists {
                return Err(error(
                    "debug breakpoint/watch is dangling",
                    "Watch a declared node or variable.",
                ));
            }
        }
        validate_edges(&nodes, &self.edges)?;
        topological_order(&nodes, &self.edges)
    }
}

fn validate_node(
    node: &BehaviorNodeContract,
    variables: &BTreeMap<String, GraphValueType>,
    registry: &CapabilityRegistry,
) -> Result<()> {
    validate_id(&node.id)?;
    let inputs = node
        .inputs
        .iter()
        .map(|port| port.id.as_str())
        .collect::<BTreeSet<_>>();
    let outputs = node
        .outputs
        .iter()
        .map(|port| port.id.as_str())
        .collect::<BTreeSet<_>>();
    if inputs.len() != node.inputs.len() || outputs.len() != node.outputs.len() {
        return Err(error(
            "node repeats a port id",
            "Use unique input/output port ids.",
        ));
    }
    for (port, literal) in &node.literals {
        let Some(schema) = node.inputs.iter().find(|candidate| candidate.id == *port) else {
            return Err(error(
                "node literal targets an unknown input",
                "Choose a declared input port.",
            ));
        };
        if literal.value_type() != schema.value_type || !finite_value(literal) {
            return Err(error(
                "node literal type/value is invalid",
                "Match the port type and use finite values.",
            ));
        }
    }
    match &node.kind {
        BehaviorNodeKind::Event { event } | BehaviorNodeKind::EmitEvent { event }
            if event.trim().is_empty() =>
        {
            Err(error("graph event name is empty", "Use a stable event id."))
        }
        BehaviorNodeKind::ReadVariable { name } | BehaviorNodeKind::WriteVariable { name }
            if !variables.contains_key(name) =>
        {
            Err(error(
                "graph node names an unknown variable",
                "Declare the variable first.",
            ))
        }
        BehaviorNodeKind::DispatchAction {
            capability_id,
            action_kind,
        } if registry.describe(capability_id).is_none() || action_kind.trim().is_empty() => {
            Err(error(
                "graph action is not a registered capability/action",
                "Choose a registered capability and a typed action kind.",
            ))
        }
        BehaviorNodeKind::Constant { output } if !outputs.contains(output.as_str()) => Err(error(
            "constant node output is missing",
            "Name one declared output port.",
        )),
        _ => Ok(()),
    }
}

fn validate_edges(
    nodes: &BTreeMap<&str, &BehaviorNodeContract>,
    edges: &[GraphEdgeContract],
) -> Result<()> {
    let mut targets = BTreeSet::new();
    let mut exact = BTreeSet::new();
    for edge in edges {
        let (Some(from), Some(to)) = (
            nodes.get(edge.from_node.as_str()),
            nodes.get(edge.to_node.as_str()),
        ) else {
            return Err(error(
                "graph edge references a missing node",
                "Connect declared nodes.",
            ));
        };
        let Some(output) = from.outputs.iter().find(|port| port.id == edge.from_port) else {
            return Err(error(
                "graph edge references a missing output",
                "Choose a declared output port.",
            ));
        };
        let Some(input) = to.inputs.iter().find(|port| port.id == edge.to_port) else {
            return Err(error(
                "graph edge references a missing input",
                "Choose a declared input port.",
            ));
        };
        if output.value_type != input.value_type {
            return Err(error(
                "graph edge type mismatch",
                "Connect ports with the same value type.",
            ));
        }
        if !targets.insert((edge.to_node.as_str(), edge.to_port.as_str()))
            || !exact.insert((
                edge.from_node.as_str(),
                edge.from_port.as_str(),
                edge.to_node.as_str(),
                edge.to_port.as_str(),
            ))
        {
            return Err(error(
                "graph input has duplicate writers",
                "Connect each input once.",
            ));
        }
    }
    for node in nodes.values() {
        for input in node.inputs.iter().filter(|port| port.required) {
            let connected = targets.contains(&(node.id.as_str(), input.id.as_str()));
            let literal = node.literals.contains_key(&input.id);
            if !connected && !literal {
                return Err(error(
                    "required graph input is unbound",
                    "Connect it or provide a typed literal.",
                ));
            }
        }
    }
    Ok(())
}

fn topological_order<'a>(
    nodes: &BTreeMap<&'a str, &BehaviorNodeContract>,
    edges: &'a [GraphEdgeContract],
) -> Result<Vec<&'a str>> {
    let mut incoming = nodes
        .keys()
        .map(|id| (*id, 0_usize))
        .collect::<BTreeMap<_, _>>();
    let mut outgoing: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in edges {
        *incoming.entry(edge.to_node.as_str()).or_default() += 1;
        outgoing
            .entry(edge.from_node.as_str())
            .or_default()
            .push(edge.to_node.as_str());
    }
    let mut ready = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect::<BTreeSet<_>>();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(id) = ready.pop_first() {
        order.push(id);
        if let Some(next) = outgoing.get(id) {
            for target in next {
                if let Some(count) = incoming.get_mut(target) {
                    *count -= 1;
                    if *count == 0 {
                        ready.insert(target);
                    }
                }
            }
        }
    }
    (order.len() == nodes.len())
        .then_some(order)
        .ok_or_else(|| {
            error(
                "behavior graph contains an execution/data cycle",
                "Break the cycle or express bounded repetition as a runtime action.",
            )
        })
}

fn instruction_for(node: &BehaviorNodeContract) -> Result<GraphInstruction> {
    Ok(match &node.kind {
        BehaviorNodeKind::Event { event } => GraphInstruction::ReceiveEvent {
            node: node.id.clone(),
            event: event.clone(),
        },
        BehaviorNodeKind::Constant { output } => GraphInstruction::LoadConstant {
            node: node.id.clone(),
            port: output.clone(),
        },
        BehaviorNodeKind::Branch => GraphInstruction::Branch {
            node: node.id.clone(),
        },
        BehaviorNodeKind::Sequence => GraphInstruction::Sequence {
            node: node.id.clone(),
        },
        BehaviorNodeKind::ReadVariable { name } => GraphInstruction::ReadVariable {
            node: node.id.clone(),
            name: name.clone(),
        },
        BehaviorNodeKind::WriteVariable { name } => GraphInstruction::WriteVariable {
            node: node.id.clone(),
            name: name.clone(),
        },
        BehaviorNodeKind::DispatchAction {
            capability_id,
            action_kind,
        } => GraphInstruction::DispatchAction {
            node: node.id.clone(),
            capability_id: capability_id.clone(),
            action_kind: action_kind.clone(),
        },
        BehaviorNodeKind::EmitEvent { event } => GraphInstruction::EmitEvent {
            node: node.id.clone(),
            event: event.clone(),
        },
    })
}

fn finite_value(value: &GraphValue) -> bool {
    match value {
        GraphValue::Number(value) => value.is_finite(),
        GraphValue::Vec3(value) => value.iter().all(|item| item.is_finite()),
        _ => true,
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
            &format!("`{id}` is not a canonical graph id"),
            "Use lowercase dotted segments.",
        )
    })
}

fn error(message: &str, hint: &str) -> EngineError {
    EngineError::Schema(message.to_owned(), Some(hint.to_owned()))
}
