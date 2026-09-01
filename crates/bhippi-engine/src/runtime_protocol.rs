//! Versioned messages for the disposable gameplay worker (ADR-0033).
//!
//! This module owns wire truth only. It does not claim a worker is a security boundary and it
//! deliberately exposes no filesystem, network, DOM, provider or generic IPC operation.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;

pub const RUNTIME_PROTOCOL_FORMAT: &str = "bhippi-runtime-protocol@1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RuntimeEnvelope<T> {
    pub format: String,
    pub session_nonce: String,
    pub sequence: u64,
    pub payload: T,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum RuntimeRequest {
    Start {
        snapshot_json: String,
        programs: Vec<RuntimeProgram>,
        capabilities: Vec<RuntimeCapability>,
        seed: u64,
        budgets: RuntimeBudgets,
    },
    Tick {
        delta_millis: u32,
        input: Vec<RuntimeInputState>,
    },
    HostResult {
        call_id: u64,
        result: RuntimeHostResult,
    },
    Stop,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum RuntimeResponse {
    Started,
    Frame {
        frame: u64,
        checkpoint_hash: String,
        events_json: String,
        consumed: RuntimeBudgetUsage,
    },
    HostCall(RuntimeHostCall),
    Stopped {
        reason: RuntimeStopReason,
        consumed: RuntimeBudgetUsage,
    },
    Fault(RuntimeFault),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RuntimeProgram {
    pub entity_id: String,
    pub program_json: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCapability {
    EntityRead,
    EntityWriteRuntime,
    InputRead,
    HudAction,
    LevelTravel,
    AudioEvent,
    DeterministicTimer,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RuntimeBudgets {
    pub instructions_per_tick: u64,
    pub instructions_total: u64,
    pub call_depth: u32,
    pub spawned_entities: u32,
    pub emitted_events: u32,
    pub log_bytes: u64,
    pub message_bytes: u64,
    pub messages_per_tick: u32,
    pub timers: u32,
    pub heap_estimate_bytes: u64,
    pub wall_clock_millis: u64,
}

/// Application-owned ceilings for one disposable runtime. A project may request lower limits,
/// but it cannot raise these ceilings through a scene, script or worker message.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RuntimeBudgetPolicy {
    pub maximum: RuntimeBudgets,
}

impl Default for RuntimeBudgetPolicy {
    fn default() -> Self {
        Self {
            maximum: RuntimeBudgets {
                instructions_per_tick: 200_000,
                instructions_total: 20_000_000,
                call_depth: 64,
                spawned_entities: 4_096,
                emitted_events: 16_384,
                log_bytes: 1_048_576,
                message_bytes: 1_048_576,
                messages_per_tick: 4_096,
                timers: 4_096,
                heap_estimate_bytes: 67_108_864,
                wall_clock_millis: 300_000,
            },
        }
    }
}

impl RuntimeBudgets {
    /// Reject disabled or project-controlled resource limits and requests above the application
    /// policy. This must run before a worker is created.
    pub fn validate(&self, policy: &RuntimeBudgetPolicy) -> Result<(), RuntimeFault> {
        let checks = [
            (
                "instructions_per_tick",
                self.instructions_per_tick,
                policy.maximum.instructions_per_tick,
            ),
            (
                "instructions_total",
                self.instructions_total,
                policy.maximum.instructions_total,
            ),
            (
                "call_depth",
                u64::from(self.call_depth),
                u64::from(policy.maximum.call_depth),
            ),
            (
                "spawned_entities",
                u64::from(self.spawned_entities),
                u64::from(policy.maximum.spawned_entities),
            ),
            (
                "emitted_events",
                u64::from(self.emitted_events),
                u64::from(policy.maximum.emitted_events),
            ),
            ("log_bytes", self.log_bytes, policy.maximum.log_bytes),
            (
                "message_bytes",
                self.message_bytes,
                policy.maximum.message_bytes,
            ),
            (
                "messages_per_tick",
                u64::from(self.messages_per_tick),
                u64::from(policy.maximum.messages_per_tick),
            ),
            (
                "timers",
                u64::from(self.timers),
                u64::from(policy.maximum.timers),
            ),
            (
                "heap_estimate_bytes",
                self.heap_estimate_bytes,
                policy.maximum.heap_estimate_bytes,
            ),
            (
                "wall_clock_millis",
                self.wall_clock_millis,
                policy.maximum.wall_clock_millis,
            ),
        ];
        for (name, requested, maximum) in checks {
            if requested == 0 || requested > maximum {
                return Err(RuntimeFault::new(
                    RuntimeFaultCode::InvalidBudget,
                    format!("runtime budget `{name}` must be within 1..={maximum}; requested {requested}"),
                ));
            }
        }
        if self.instructions_per_tick > self.instructions_total {
            return Err(RuntimeFault::new(
                RuntimeFaultCode::InvalidBudget,
                "runtime `instructions_per_tick` cannot exceed `instructions_total`".to_owned(),
            ));
        }
        Ok(())
    }

    /// Report the first exhausted cumulative budget with a stable machine-readable resource id.
    pub fn check_usage(&self, usage: &RuntimeBudgetUsage) -> Result<(), RuntimeBudgetExceeded> {
        let checks = [
            (
                "instructions_total",
                usage.instructions,
                self.instructions_total,
            ),
            (
                "spawned_entities",
                u64::from(usage.spawned_entities),
                u64::from(self.spawned_entities),
            ),
            (
                "emitted_events",
                u64::from(usage.emitted_events),
                u64::from(self.emitted_events),
            ),
            ("log_bytes", usage.log_bytes, self.log_bytes),
            ("timers", u64::from(usage.timers), u64::from(self.timers)),
            (
                "heap_estimate_bytes",
                usage.heap_estimate_bytes,
                self.heap_estimate_bytes,
            ),
            (
                "wall_clock_millis",
                usage.wall_clock_millis,
                self.wall_clock_millis,
            ),
        ];
        for (resource, consumed, limit) in checks {
            if consumed > limit {
                return Err(RuntimeBudgetExceeded {
                    resource: resource.to_owned(),
                    consumed,
                    limit,
                });
            }
        }
        Ok(())
    }

    pub fn check_tick(
        &self,
        instructions: u64,
        messages: u32,
    ) -> Result<(), RuntimeBudgetExceeded> {
        for (resource, consumed, limit) in [
            (
                "instructions_per_tick",
                instructions,
                self.instructions_per_tick,
            ),
            (
                "messages_per_tick",
                u64::from(messages),
                u64::from(self.messages_per_tick),
            ),
        ] {
            if consumed > limit {
                return Err(RuntimeBudgetExceeded {
                    resource: resource.to_owned(),
                    consumed,
                    limit,
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RuntimeBudgetExceeded {
    pub resource: String,
    pub consumed: u64,
    pub limit: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct RuntimeBudgetUsage {
    pub instructions: u64,
    pub spawned_entities: u32,
    pub emitted_events: u32,
    pub log_bytes: u64,
    pub messages: u64,
    pub timers: u32,
    pub heap_estimate_bytes: u64,
    pub wall_clock_millis: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RuntimeInputState {
    pub action: String,
    pub value: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RuntimeHostCall {
    pub call_id: u64,
    pub capability: RuntimeCapability,
    pub operation: RuntimeHostOperation,
}

impl RuntimeHostCall {
    pub fn validate_declared(
        &self,
        declared: &BTreeSet<RuntimeCapability>,
    ) -> Result<(), RuntimeFault> {
        let required = self.operation.required_capability();
        if self.capability != required {
            return Err(RuntimeFault::new(
                RuntimeFaultCode::InvalidHostCall,
                format!(
                    "host operation requires `{required:?}` but the call claimed `{:?}`",
                    self.capability
                ),
            ));
        }
        if !declared.contains(&required) {
            return Err(RuntimeFault::new(
                RuntimeFaultCode::UndeclaredCapability,
                format!("runtime capability `{required:?}` was not declared for this run"),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum RuntimeHostOperation {
    ReadEntity {
        entity_id: String,
    },
    PatchRuntimeEntity {
        entity_id: String,
        patch_json: String,
    },
    ReadInput {
        action: String,
    },
    DispatchHudAction {
        action_json: String,
    },
    TravelLevel {
        level: String,
    },
    EmitAudio {
        event: String,
    },
    ScheduleTimer {
        timer_id: String,
        delay_millis: u64,
    },
}

impl RuntimeHostOperation {
    #[must_use]
    pub const fn required_capability(&self) -> RuntimeCapability {
        match self {
            Self::ReadEntity { .. } => RuntimeCapability::EntityRead,
            Self::PatchRuntimeEntity { .. } => RuntimeCapability::EntityWriteRuntime,
            Self::ReadInput { .. } => RuntimeCapability::InputRead,
            Self::DispatchHudAction { .. } => RuntimeCapability::HudAction,
            Self::TravelLevel { .. } => RuntimeCapability::LevelTravel,
            Self::EmitAudio { .. } => RuntimeCapability::AudioEvent,
            Self::ScheduleTimer { .. } => RuntimeCapability::DeterministicTimer,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum RuntimeHostResult {
    Ok { value_json: String },
    Rejected { fault: RuntimeFault },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStopReason {
    Requested,
    Completed,
    Replaced,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeFaultCode {
    InvalidFormat,
    InvalidNonce,
    OutOfOrder,
    PayloadTooLarge,
    MalformedMessage,
    UndeclaredCapability,
    InvalidHostCall,
    InvalidBudget,
    BudgetExhausted,
    InvalidBytecode,
    WorkerExited,
    WatchdogTimeout,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RuntimeFault {
    pub code: RuntimeFaultCode,
    pub message: String,
    pub script: Option<String>,
    pub line: Option<u32>,
    pub instruction: Option<u64>,
}

impl RuntimeFault {
    fn new(code: RuntimeFaultCode, message: String) -> Self {
        Self {
            code,
            message,
            script: None,
            line: None,
            instruction: None,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RuntimeProtocolError {
    #[error("runtime protocol session nonce cannot be empty")]
    EmptyNonce,
    #[error("runtime message exceeded the configured payload cap ({actual} > {maximum} bytes)")]
    PayloadTooLarge { actual: usize, maximum: usize },
    #[error("runtime message is malformed: {0}")]
    Malformed(String),
    #[error("unknown runtime protocol format `{0}`")]
    InvalidFormat(String),
    #[error("runtime message nonce does not match this disposable session")]
    InvalidNonce,
    #[error("runtime message sequence is out of order (expected {expected}, got {actual})")]
    OutOfOrder { expected: u64, actual: u64 },
    #[error("runtime protocol sequence overflowed")]
    SequenceOverflow,
}

/// Stateful validation at one side of one disposable runtime session.
///
/// The caller supplies the encoded-message cap from versioned runtime configuration. A message
/// is returned only after size, schema, format, nonce and exact sequence have all passed.
#[derive(Clone, Debug)]
pub struct RuntimeProtocolGuard {
    session_nonce: String,
    next_sequence: u64,
    maximum_payload_bytes: usize,
}

impl RuntimeProtocolGuard {
    pub fn new(
        session_nonce: impl Into<String>,
        maximum_payload_bytes: usize,
    ) -> Result<Self, RuntimeProtocolError> {
        let session_nonce = session_nonce.into();
        if session_nonce.trim().is_empty() {
            return Err(RuntimeProtocolError::EmptyNonce);
        }
        Ok(Self {
            session_nonce,
            next_sequence: 0,
            maximum_payload_bytes,
        })
    }

    pub fn accept_request(
        &mut self,
        encoded: &[u8],
    ) -> Result<RuntimeEnvelope<RuntimeRequest>, RuntimeProtocolError> {
        self.accept(encoded)
    }

    pub fn accept_response(
        &mut self,
        encoded: &[u8],
    ) -> Result<RuntimeEnvelope<RuntimeResponse>, RuntimeProtocolError> {
        self.accept(encoded)
    }

    fn accept<T: DeserializeOwned>(
        &mut self,
        encoded: &[u8],
    ) -> Result<RuntimeEnvelope<T>, RuntimeProtocolError> {
        if encoded.len() > self.maximum_payload_bytes {
            return Err(RuntimeProtocolError::PayloadTooLarge {
                actual: encoded.len(),
                maximum: self.maximum_payload_bytes,
            });
        }
        let envelope: RuntimeEnvelope<T> = serde_json::from_slice(encoded)
            .map_err(|error| RuntimeProtocolError::Malformed(error.to_string()))?;
        if envelope.format != RUNTIME_PROTOCOL_FORMAT {
            return Err(RuntimeProtocolError::InvalidFormat(envelope.format));
        }
        if envelope.session_nonce != self.session_nonce {
            return Err(RuntimeProtocolError::InvalidNonce);
        }
        if envelope.sequence != self.next_sequence {
            return Err(RuntimeProtocolError::OutOfOrder {
                expected: self.next_sequence,
                actual: envelope.sequence,
            });
        }
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(RuntimeProtocolError::SequenceOverflow)?;
        Ok(envelope)
    }

    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RuntimeBudgetPolicy, RuntimeBudgetUsage, RuntimeBudgets, RuntimeCapability,
        RuntimeEnvelope, RuntimeHostCall, RuntimeHostOperation, RuntimeProtocolError,
        RuntimeProtocolGuard, RuntimeRequest, RuntimeResponse, RUNTIME_PROTOCOL_FORMAT,
    };

    fn encoded(nonce: &str, sequence: u64, payload: RuntimeRequest) -> Vec<u8> {
        serde_json::to_vec(&RuntimeEnvelope {
            format: RUNTIME_PROTOCOL_FORMAT.to_owned(),
            session_nonce: nonce.to_owned(),
            sequence,
            payload,
        })
        .expect("fixture serializes")
    }

    #[test]
    fn accepts_only_the_exact_monotonic_session() {
        let mut guard = RuntimeProtocolGuard::new("run-01", 4096).expect("valid guard");
        let first = encoded("run-01", 0, RuntimeRequest::Stop);
        assert_eq!(
            guard
                .accept_request(&first)
                .expect("first accepted")
                .sequence,
            0
        );
        assert_eq!(guard.next_sequence(), 1);

        let replay = guard.accept_request(&first).expect_err("replay rejected");
        assert_eq!(
            replay,
            RuntimeProtocolError::OutOfOrder {
                expected: 1,
                actual: 0
            }
        );

        let wrong_nonce = encoded("run-02", 1, RuntimeRequest::Stop);
        assert_eq!(
            guard
                .accept_request(&wrong_nonce)
                .expect_err("nonce rejected"),
            RuntimeProtocolError::InvalidNonce,
        );

        let second = encoded("run-01", 1, RuntimeRequest::Stop);
        assert_eq!(
            guard
                .accept_request(&second)
                .expect("second accepted")
                .sequence,
            1
        );
    }

    #[test]
    fn rejects_oversized_unknown_and_malformed_messages() {
        assert_eq!(
            RuntimeProtocolGuard::new(" ", 20).expect_err("empty nonce rejected"),
            RuntimeProtocolError::EmptyNonce,
        );

        let message = encoded("run", 0, RuntimeRequest::Stop);
        let mut tiny = RuntimeProtocolGuard::new("run", message.len() - 1).expect("guard");
        assert!(matches!(
            tiny.accept_request(&message),
            Err(RuntimeProtocolError::PayloadTooLarge { .. })
        ));

        let mut malformed = RuntimeProtocolGuard::new("run", 100).expect("guard");
        assert!(matches!(
            malformed.accept_request(br#"{"format":"future"}"#),
            Err(RuntimeProtocolError::Malformed(_))
        ));

        let unknown = serde_json::json!({
            "format": "bhippi-runtime-protocol@2",
            "session_nonce": "run",
            "sequence": 0,
            "payload": { "kind": "stop" }
        });
        let unknown = serde_json::to_vec(&unknown).expect("fixture serializes");
        let mut guard = RuntimeProtocolGuard::new("run", 4096).expect("guard");
        assert_eq!(
            guard.accept_request(&unknown).expect_err("format rejected"),
            RuntimeProtocolError::InvalidFormat("bhippi-runtime-protocol@2".to_owned()),
        );
    }

    #[test]
    fn unknown_payload_variants_fail_closed() {
        let unknown = serde_json::json!({
            "format": RUNTIME_PROTOCOL_FORMAT,
            "session_nonce": "run",
            "sequence": 0,
            "payload": { "kind": "open_socket", "data": { "url": "https://example.com" } }
        });
        let encoded = serde_json::to_vec(&unknown).expect("fixture serializes");
        let mut guard = RuntimeProtocolGuard::new("run", 4096).expect("guard");
        assert!(matches!(
            guard.accept_request(&encoded),
            Err(RuntimeProtocolError::Malformed(_))
        ));
    }

    #[test]
    fn responses_use_the_same_nonce_sequence_and_size_gate() {
        let encoded = serde_json::to_vec(&RuntimeEnvelope {
            format: RUNTIME_PROTOCOL_FORMAT.to_owned(),
            session_nonce: "worker-run".to_owned(),
            sequence: 0,
            payload: RuntimeResponse::Started,
        })
        .expect("fixture serializes");
        let mut guard = RuntimeProtocolGuard::new("worker-run", 4096).expect("guard");
        assert_eq!(
            guard
                .accept_response(&encoded)
                .expect("response accepted")
                .payload,
            RuntimeResponse::Started,
        );
        assert_eq!(guard.next_sequence(), 1);
    }

    #[test]
    fn host_calls_require_the_exact_declared_capability() {
        let call = RuntimeHostCall {
            call_id: 7,
            capability: RuntimeCapability::LevelTravel,
            operation: RuntimeHostOperation::TravelLevel {
                level: "Level2".to_owned(),
            },
        };
        let declared = std::collections::BTreeSet::from([RuntimeCapability::LevelTravel]);
        assert_eq!(call.validate_declared(&declared), Ok(()));

        let denied = call
            .validate_declared(&std::collections::BTreeSet::new())
            .expect_err("undeclared call blocked");
        assert_eq!(denied.code, super::RuntimeFaultCode::UndeclaredCapability);

        let mismatched = RuntimeHostCall {
            capability: RuntimeCapability::AudioEvent,
            ..call
        }
        .validate_declared(&declared)
        .expect_err("mismatched declaration blocked");
        assert_eq!(mismatched.code, super::RuntimeFaultCode::InvalidHostCall);
    }

    #[test]
    fn budgets_are_non_zero_application_bounded_and_internally_consistent() {
        let policy = RuntimeBudgetPolicy::default();
        let mut requested = policy.maximum.clone();
        assert_eq!(requested.validate(&policy), Ok(()));

        requested.instructions_per_tick = requested.instructions_total + 1;
        assert_eq!(
            requested
                .validate(&policy)
                .expect_err("inconsistent cap blocked")
                .code,
            super::RuntimeFaultCode::InvalidBudget
        );

        let mut disabled = policy.maximum.clone();
        disabled.wall_clock_millis = 0;
        assert_eq!(
            disabled
                .validate(&policy)
                .expect_err("disabled watchdog blocked")
                .code,
            super::RuntimeFaultCode::InvalidBudget
        );

        let mut excessive = policy.maximum.clone();
        excessive.call_depth += 1;
        assert_eq!(
            excessive
                .validate(&policy)
                .expect_err("project cannot raise cap")
                .code,
            super::RuntimeFaultCode::InvalidBudget
        );
    }

    #[test]
    fn cumulative_usage_reports_the_first_stable_exhausted_resource() {
        let budgets = RuntimeBudgets {
            instructions_per_tick: 10,
            instructions_total: 100,
            call_depth: 4,
            spawned_entities: 3,
            emitted_events: 4,
            log_bytes: 20,
            message_bytes: 128,
            messages_per_tick: 5,
            timers: 2,
            heap_estimate_bytes: 1_024,
            wall_clock_millis: 50,
        };
        let usage = RuntimeBudgetUsage {
            instructions: 101,
            ..RuntimeBudgetUsage::default()
        };
        let exceeded = budgets
            .check_usage(&usage)
            .expect_err("instruction cap enforced");
        assert_eq!(exceeded.resource, "instructions_total");
        assert_eq!(exceeded.consumed, 101);
        assert_eq!(exceeded.limit, 100);

        let tick = budgets
            .check_tick(10, 6)
            .expect_err("message burst blocked");
        assert_eq!(tick.resource, "messages_per_tick");
    }
}
