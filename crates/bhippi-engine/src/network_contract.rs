//! Versioned network authority contract (ADR-0038).
//!
//! This module intentionally provides no socket or provider implementation. It makes unsafe or
//! unbounded network declarations unrepresentable before Phase 23 selects a real transport.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;

pub const NETWORK_CONTRACT_FORMAT: &str = "bhippi-network-contract@1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct NetworkContract {
    pub format: String,
    pub authority: AuthorityModel,
    pub identity: IdentityContract,
    pub timing: NetworkTiming,
    pub transport: TransportContract,
    pub replication: Vec<ReplicationRule>,
    pub input_messages: Vec<InputMessageContract>,
    pub server_rpcs: Vec<ServerRpcContract>,
    pub prediction: PredictionContract,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityModel {
    DedicatedServer,
    ListenServer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct IdentityContract {
    pub server_issued: bool,
    pub session_nonce_bytes: u16,
    pub entity_counter_bits: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct NetworkTiming {
    pub simulation_hz: u16,
    pub snapshot_hz: u16,
    pub interpolation_millis: u16,
    pub maximum_rollback_ticks: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct TransportContract {
    pub encryption_required: bool,
    pub peer_authentication_required: bool,
    pub reliable_ordered: bool,
    pub unreliable_sequenced: bool,
    pub maximum_payload_bytes: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ReplicationRule {
    pub id: String,
    pub component: String,
    pub fields: Vec<String>,
    pub visibility: ReplicationVisibility,
    pub frequency_hz: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationVisibility {
    AllClients,
    OwnerOnly,
    RelevantClients,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct InputMessageContract {
    pub id: String,
    pub actions: Vec<String>,
    pub maximum_payload_bytes: u16,
    pub maximum_per_second: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ServerRpcContract {
    pub id: String,
    pub permission: String,
    pub reliability: RpcReliability,
    pub maximum_payload_bytes: u16,
    pub maximum_per_second: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RpcReliability {
    ReliableOrdered,
    UnreliableSequenced,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PredictionContract {
    pub predicted_components: Vec<String>,
    pub maximum_history_ticks: u16,
    pub reconciliation: ReconciliationPolicy,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationPolicy {
    AuthoritativeSnap,
    AuthoritativeReplayInputs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct NetworkContractLimits {
    pub maximum_payload_bytes: u32,
    pub maximum_messages_per_second: u16,
    pub maximum_rollback_ticks: u16,
    pub maximum_rules: usize,
}

impl Default for NetworkContractLimits {
    fn default() -> Self {
        Self {
            maximum_payload_bytes: 64 * 1_024,
            maximum_messages_per_second: 240,
            maximum_rollback_ticks: 240,
            maximum_rules: 4_096,
        }
    }
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum NetworkContractError {
    #[error("unsupported network contract format `{0}`")]
    UnsupportedFormat(String),
    #[error("network identity must be issued by the authoritative server")]
    ClientIssuedIdentity,
    #[error("network transport cannot disable encryption or peer authentication")]
    InsecureTransport,
    #[error("network contract field `{0}` is outside application limits")]
    OutsideLimit(&'static str),
    #[error("network contract contains duplicate id `{0}`")]
    DuplicateId(String),
    #[error("network contract field `{0}` cannot be empty")]
    EmptyField(&'static str),
    #[error("replication rule `{0}` has no fields")]
    EmptyReplication(String),
    #[error("predicted component `{0}` is not replicated")]
    PredictionWithoutReplication(String),
    #[error("network contract could not be encoded: {0}")]
    Encoding(String),
}

impl NetworkContract {
    pub fn validate(&self, limits: &NetworkContractLimits) -> Result<(), NetworkContractError> {
        if self.format != NETWORK_CONTRACT_FORMAT {
            return Err(NetworkContractError::UnsupportedFormat(self.format.clone()));
        }
        if !self.identity.server_issued {
            return Err(NetworkContractError::ClientIssuedIdentity);
        }
        if self.identity.session_nonce_bytes < 16 || self.identity.entity_counter_bits < 32 {
            return Err(NetworkContractError::OutsideLimit("identity"));
        }
        if !self.transport.encryption_required || !self.transport.peer_authentication_required {
            return Err(NetworkContractError::InsecureTransport);
        }
        if self.transport.maximum_payload_bytes == 0
            || self.transport.maximum_payload_bytes > limits.maximum_payload_bytes
        {
            return Err(NetworkContractError::OutsideLimit("maximum_payload_bytes"));
        }
        if self.timing.simulation_hz == 0
            || self.timing.snapshot_hz == 0
            || self.timing.snapshot_hz > self.timing.simulation_hz
            || self.timing.maximum_rollback_ticks > limits.maximum_rollback_ticks
            || self.prediction.maximum_history_ticks > self.timing.maximum_rollback_ticks
        {
            return Err(NetworkContractError::OutsideLimit("timing"));
        }
        let total_rules =
            self.replication.len() + self.input_messages.len() + self.server_rpcs.len();
        if total_rules > limits.maximum_rules {
            return Err(NetworkContractError::OutsideLimit("rules"));
        }

        let mut ids = BTreeSet::new();
        let mut replicated_components = BTreeSet::new();
        for rule in &self.replication {
            validate_id(&rule.id, "replication.id")?;
            validate_id(&rule.component, "replication.component")?;
            if !ids.insert(rule.id.clone()) {
                return Err(NetworkContractError::DuplicateId(rule.id.clone()));
            }
            if rule.fields.is_empty() {
                return Err(NetworkContractError::EmptyReplication(rule.id.clone()));
            }
            if rule.frequency_hz == 0 || rule.frequency_hz > self.timing.simulation_hz {
                return Err(NetworkContractError::OutsideLimit(
                    "replication.frequency_hz",
                ));
            }
            replicated_components.insert(rule.component.as_str());
        }
        for message in &self.input_messages {
            validate_id(&message.id, "input_message.id")?;
            if !ids.insert(message.id.clone()) {
                return Err(NetworkContractError::DuplicateId(message.id.clone()));
            }
            validate_rate_and_payload(
                message.maximum_per_second,
                u32::from(message.maximum_payload_bytes),
                limits,
            )?;
            if message.actions.is_empty() {
                return Err(NetworkContractError::EmptyField("input_message.actions"));
            }
        }
        for rpc in &self.server_rpcs {
            validate_id(&rpc.id, "server_rpc.id")?;
            validate_id(&rpc.permission, "server_rpc.permission")?;
            if !ids.insert(rpc.id.clone()) {
                return Err(NetworkContractError::DuplicateId(rpc.id.clone()));
            }
            validate_rate_and_payload(
                rpc.maximum_per_second,
                u32::from(rpc.maximum_payload_bytes),
                limits,
            )?;
        }
        for component in &self.prediction.predicted_components {
            if !replicated_components.contains(component.as_str()) {
                return Err(NetworkContractError::PredictionWithoutReplication(
                    component.clone(),
                ));
            }
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<String, NetworkContractError> {
        let mut canonical = self.clone();
        canonical
            .replication
            .sort_by(|left, right| left.id.cmp(&right.id));
        canonical
            .input_messages
            .sort_by(|left, right| left.id.cmp(&right.id));
        canonical
            .server_rpcs
            .sort_by(|left, right| left.id.cmp(&right.id));
        canonical.prediction.predicted_components.sort();
        let bytes = serde_json::to_vec(&canonical)
            .map_err(|error| NetworkContractError::Encoding(error.to_string()))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

fn validate_id(value: &str, field: &'static str) -> Result<(), NetworkContractError> {
    if value.trim().is_empty() {
        return Err(NetworkContractError::EmptyField(field));
    }
    Ok(())
}

fn validate_rate_and_payload(
    rate: u16,
    payload: u32,
    limits: &NetworkContractLimits,
) -> Result<(), NetworkContractError> {
    if rate == 0 || rate > limits.maximum_messages_per_second {
        return Err(NetworkContractError::OutsideLimit("message_rate"));
    }
    if payload == 0 || payload > limits.maximum_payload_bytes {
        return Err(NetworkContractError::OutsideLimit("message_payload"));
    }
    Ok(())
}
