//! Versioned runtime-kernel subsystem contracts (ADR-0036).
//!
//! These types describe and validate integration boundaries. They do not claim a scheduler,
//! worker, resource manager, backend, hot reload service or profiler exists.

use crate::error::{EngineError, Result};
use crate::registry::CapabilityRegistry;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const SUBSYSTEM_FORMAT: &str = "bhippi-subsystem@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOwner {
    RustKernel,
    ModuleWorker,
    WebviewRenderer,
    HostBroker,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Registered,
    Loading,
    Ready,
    Running,
    Quiescing,
    Stopped,
    Faulted,
}

impl LifecycleState {
    #[must_use]
    pub const fn allows(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Registered,
                Self::Loading | Self::Stopped | Self::Faulted
            ) | (Self::Loading, Self::Ready | Self::Stopped | Self::Faulted)
                | (Self::Ready, Self::Running | Self::Stopped | Self::Faulted)
                | (Self::Running, Self::Quiescing | Self::Faulted)
                | (Self::Quiescing, Self::Ready | Self::Stopped | Self::Faulted)
                | (Self::Faulted, Self::Stopped)
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SchedulePhase {
    Input,
    PrePhysics,
    Physics,
    PostPhysics,
    Animation,
    Gameplay,
    Audio,
    RenderExtract,
    Diagnostics,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct FixedStepContract {
    /// Versioned configuration value; the contract validator only requires it to be non-zero.
    pub step_micros: u64,
    pub phase: SchedulePhase,
    #[serde(default)]
    pub after: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionLane {
    Deterministic,
    ParallelBestEffort,
    Io,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct JobContract {
    pub lane: ExecutionLane,
    pub cancellable: bool,
    pub result_ordered_at_safe_point: bool,
}

/// Opaque resource identity. Paths never enter the runtime/frame vocabulary.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
pub struct RuntimeResourceHandle {
    pub id: u64,
    pub generation: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ResourceLoadRequest {
    pub request_id: u64,
    pub resource: RuntimeResourceHandle,
    pub kind: String,
    pub priority: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ResourceLoadState {
    Pending,
    Ready,
    Cancelled,
    Faulted,
}

#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Type,
)]
pub struct RuntimeEntityHandle {
    pub id: u64,
    pub generation: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum RuntimeWorldQuery {
    Exists {
        entity: RuntimeEntityHandle,
    },
    Component {
        entity: RuntimeEntityHandle,
        name: String,
    },
    FindByTag {
        tag: String,
    },
}

/// Commands mutate only the disposable runtime clone, never an authored document.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum RuntimeWorldCommand {
    SpawnRuntime {
        preset_capability: String,
    },
    Despawn {
        entity: RuntimeEntityHandle,
    },
    PatchRuntimeComponent {
        entity: RuntimeEntityHandle,
        component: String,
        value: serde_json::Value,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum QueueBackpressure {
    RejectProducer,
    DropOldest,
    CoalesceLatest,
    StopRuntime,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum EventOrdering {
    GlobalSequence,
    PerProducer,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct EventContract {
    pub id: String,
    pub schema: String,
    pub capacity: u32,
    pub ordering: EventOrdering,
    pub backpressure: QueueBackpressure,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct RuntimeBudgetContract {
    pub cpu_micros_per_tick: u64,
    pub resident_bytes: u64,
    pub emitted_events_per_tick: u32,
    pub queued_jobs: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReloadSafePoint {
    BeforeTick,
    AfterTick,
    StoppedOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct HotReloadContract {
    pub enabled: bool,
    pub safe_point: ReloadSafePoint,
    pub validate_before_swap: bool,
    pub rollback_on_fault: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePlatform {
    Windows,
    Macos,
    Linux,
    Web,
}

impl RuntimePlatform {
    pub const ALL: [Self; 4] = [Self::Windows, Self::Macos, Self::Linux, Self::Web];
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PlatformSupportLevel {
    Supported,
    Experimental,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PlatformSupport {
    pub platform: RuntimePlatform,
    pub level: PlatformSupportLevel,
    pub evidence: Option<String>,
    pub limitation: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FaultContainment {
    StopSubsystem,
    StopRuntime,
    QuarantineResource,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct FaultContract {
    pub containment: FaultContainment,
    pub rollback_to_safe_point: bool,
    pub preserve_diagnostic: bool,
    pub restartable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct TelemetryField {
    pub name: String,
    pub unit: String,
    pub cumulative: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct SubsystemContract {
    pub id: String,
    pub version: String,
    pub owner: RuntimeOwner,
    #[serde(default)]
    pub capability_ids: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub schedule: FixedStepContract,
    #[serde(default)]
    pub resource_kinds: Vec<String>,
    #[serde(default)]
    pub consumes_events: Vec<EventContract>,
    #[serde(default)]
    pub emits_events: Vec<EventContract>,
    #[serde(default)]
    pub jobs: Vec<JobContract>,
    pub budgets: RuntimeBudgetContract,
    pub hot_reload: HotReloadContract,
    pub telemetry: Vec<TelemetryField>,
    pub fault: FaultContract,
    pub platforms: Vec<PlatformSupport>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct SubsystemContractSet {
    pub format: String,
    pub capability_registry_hash: String,
    pub hash: String,
    pub contracts: Vec<SubsystemContract>,
}

impl SubsystemContractSet {
    pub fn build(
        mut contracts: Vec<SubsystemContract>,
        capabilities: &CapabilityRegistry,
    ) -> Result<Self> {
        contracts.sort_by(|left, right| left.id.cmp(&right.id));
        for contract in &mut contracts {
            sort_dedup(&mut contract.capability_ids);
            sort_dedup(&mut contract.dependencies);
            sort_dedup(&mut contract.schedule.after);
            sort_dedup(&mut contract.resource_kinds);
            contract.platforms.sort_by_key(|item| item.platform);
            contract
                .telemetry
                .sort_by(|left, right| left.name.cmp(&right.name));
        }
        validate_contracts(&contracts, capabilities)?;
        let bytes = serde_json::to_vec(&contracts).map_err(|error| {
            contract_error(
                format!("subsystem contracts could not be serialized: {error}"),
                "Fix the invalid contract metadata and rebuild.",
            )
        })?;
        Ok(Self {
            format: SUBSYSTEM_FORMAT.to_owned(),
            capability_registry_hash: capabilities.hash.clone(),
            hash: blake3::hash(&bytes).to_hex().to_string(),
            contracts,
        })
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&SubsystemContract> {
        self.contracts
            .binary_search_by_key(&id, |contract| contract.id.as_str())
            .ok()
            .and_then(|index| self.contracts.get(index))
    }
}

fn validate_contracts(
    contracts: &[SubsystemContract],
    capabilities: &CapabilityRegistry,
) -> Result<()> {
    let mut ids = BTreeSet::new();
    for contract in contracts {
        validate_id(&contract.id)?;
        validate_version(&contract.version)?;
        if !ids.insert(contract.id.as_str()) {
            return Err(contract_error(
                format!("duplicate subsystem id `{}`", contract.id),
                "Use one stable id for each runtime subsystem.",
            ));
        }
        for capability in &contract.capability_ids {
            if capabilities.describe(capability).is_none() {
                return Err(contract_error(
                    format!(
                        "subsystem `{}` names unknown capability `{capability}`",
                        contract.id
                    ),
                    "Register the capability before binding a runtime subsystem.",
                ));
            }
        }
        validate_local_contract(contract)?;
    }
    for contract in contracts {
        for dependency in contract.dependencies.iter().chain(&contract.schedule.after) {
            if !ids.contains(dependency.as_str()) {
                return Err(contract_error(
                    format!(
                        "subsystem `{}` depends on unknown `{dependency}`",
                        contract.id
                    ),
                    "Register the dependency or remove the stale ordering edge.",
                ));
            }
            if dependency == &contract.id {
                return Err(contract_error(
                    format!("subsystem `{}` depends on itself", contract.id),
                    "Remove the self dependency.",
                ));
            }
        }
    }
    reject_cycles(contracts)
}

fn validate_local_contract(contract: &SubsystemContract) -> Result<()> {
    if contract.schedule.step_micros == 0 {
        return Err(contract_error(
            format!("subsystem `{}` has a zero fixed step", contract.id),
            "Declare a non-zero step from versioned runtime configuration.",
        ));
    }
    if !contract
        .jobs
        .iter()
        .any(|job| job.lane == ExecutionLane::Deterministic)
    {
        return Err(contract_error(
            format!(
                "subsystem `{}` has no deterministic execution lane",
                contract.id
            ),
            "Keep a deterministic lane even when parallel work is supported.",
        ));
    }
    let budgets = &contract.budgets;
    if budgets.cpu_micros_per_tick == 0
        || budgets.resident_bytes == 0
        || budgets.emitted_events_per_tick == 0
        || budgets.queued_jobs == 0
    {
        return Err(contract_error(
            format!("subsystem `{}` has an unbounded/zero budget", contract.id),
            "Declare non-zero CPU, memory, event and job limits.",
        ));
    }
    for event in contract
        .consumes_events
        .iter()
        .chain(&contract.emits_events)
    {
        validate_id(&event.id)?;
        if event.schema.trim().is_empty() || event.capacity == 0 {
            return Err(contract_error(
                format!(
                    "subsystem `{}` has an unbounded event `{}`",
                    contract.id, event.id
                ),
                "Declare a typed schema and non-zero bounded capacity.",
            ));
        }
    }
    if contract.hot_reload.enabled
        && (!contract.hot_reload.validate_before_swap || !contract.hot_reload.rollback_on_fault)
    {
        return Err(contract_error(
            format!(
                "subsystem `{}` hot reload cannot roll back safely",
                contract.id
            ),
            "Validate before swapping and roll back a failed replacement.",
        ));
    }
    validate_platforms(contract)?;
    let telemetry = contract
        .telemetry
        .iter()
        .map(|field| field.name.as_str())
        .collect::<BTreeSet<_>>();
    if !telemetry.contains("cpu_micros") || !telemetry.contains("resident_bytes") {
        return Err(contract_error(
            format!(
                "subsystem `{}` lacks standard profiler counters",
                contract.id
            ),
            "Expose cpu_micros and resident_bytes in the shared telemetry schema.",
        ));
    }
    if !contract.fault.preserve_diagnostic {
        return Err(contract_error(
            format!("subsystem `{}` discards fault diagnostics", contract.id),
            "Preserve a typed diagnostic before containment or restart.",
        ));
    }
    Ok(())
}

fn validate_platforms(contract: &SubsystemContract) -> Result<()> {
    let mut seen = BTreeSet::new();
    for platform in &contract.platforms {
        if !seen.insert(platform.platform) {
            return Err(contract_error(
                format!("subsystem `{}` repeats a platform", contract.id),
                "Declare each runtime platform exactly once.",
            ));
        }
        match platform.level {
            PlatformSupportLevel::Supported
                if platform.evidence.as_deref().is_none_or(str::is_empty) =>
            {
                return Err(contract_error(
                    format!(
                        "subsystem `{}` claims support without evidence",
                        contract.id
                    ),
                    "Name the passing platform fixture or release evidence.",
                ));
            }
            PlatformSupportLevel::Experimental | PlatformSupportLevel::Unsupported
                if platform.limitation.as_deref().is_none_or(str::is_empty) =>
            {
                return Err(contract_error(
                    format!("subsystem `{}` omits a platform limitation", contract.id),
                    "State why the platform is experimental or unavailable.",
                ));
            }
            _ => {}
        }
    }
    if RuntimePlatform::ALL
        .into_iter()
        .any(|platform| !seen.contains(&platform))
    {
        return Err(contract_error(
            format!("subsystem `{}` has incomplete platform truth", contract.id),
            "Declare Windows, macOS, Linux and Web explicitly.",
        ));
    }
    Ok(())
}

fn reject_cycles(contracts: &[SubsystemContract]) -> Result<()> {
    let edges = contracts
        .iter()
        .map(|contract| {
            let mut dependencies = contract
                .dependencies
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            dependencies.extend(contract.schedule.after.iter().map(String::as_str));
            dependencies.sort();
            dependencies.dedup();
            (contract.id.as_str(), dependencies)
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
        return Err(contract_error(
            format!("runtime subsystem dependency cycle reaches `{id}`"),
            "Break the lifecycle/schedule dependency cycle.",
        ));
    }
    if let Some(next) = edges.get(id) {
        for dependency in next {
            visit(dependency, edges, active, complete)?;
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
        contract_error(
            format!("`{id}` is not a canonical subsystem/event id"),
            "Use lowercase dotted segments.",
        )
    })
}

fn validate_version(version: &str) -> Result<()> {
    let parts = version.split('.').collect::<Vec<_>>();
    (parts.len() == 3 && parts[0] == "1" && parts.iter().all(|part| part.parse::<u32>().is_ok()))
        .then_some(())
        .ok_or_else(|| {
            contract_error(
                format!("subsystem version `{version}` is incompatible with {SUBSYSTEM_FORMAT}"),
                "Use numeric 1.x.y or upgrade the reader deliberately.",
            )
        })
}

fn sort_dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}
fn contract_error(message: String, hint: &str) -> EngineError {
    EngineError::Schema(message, Some(hint.to_owned()))
}
