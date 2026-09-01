#![allow(clippy::expect_used, clippy::unwrap_used)]

use bhippi_engine::registry::CapabilityRegistry;
use bhippi_engine::runtime_contract::{
    EventContract, EventOrdering, ExecutionLane, FaultContainment, FaultContract,
    FixedStepContract, HotReloadContract, JobContract, LifecycleState, PlatformSupport,
    PlatformSupportLevel, QueueBackpressure, ReloadSafePoint, RuntimeBudgetContract,
    RuntimeEntityHandle, RuntimeOwner, RuntimePlatform, RuntimeResourceHandle, RuntimeWorldCommand,
    RuntimeWorldQuery, SchedulePhase, SubsystemContract, SubsystemContractSet, TelemetryField,
    SUBSYSTEM_FORMAT,
};

fn platforms() -> Vec<PlatformSupport> {
    vec![
        PlatformSupport {
            platform: RuntimePlatform::Windows,
            level: PlatformSupportLevel::Supported,
            evidence: Some("fixture:windows".to_owned()),
            limitation: None,
        },
        PlatformSupport {
            platform: RuntimePlatform::Macos,
            level: PlatformSupportLevel::Experimental,
            evidence: None,
            limitation: Some("release golden pending".to_owned()),
        },
        PlatformSupport {
            platform: RuntimePlatform::Linux,
            level: PlatformSupportLevel::Experimental,
            evidence: None,
            limitation: Some("release golden pending".to_owned()),
        },
        PlatformSupport {
            platform: RuntimePlatform::Web,
            level: PlatformSupportLevel::Unsupported,
            evidence: None,
            limitation: Some("backend not integrated".to_owned()),
        },
    ]
}

fn contract(id: &str, capability: &str, phase: SchedulePhase) -> SubsystemContract {
    SubsystemContract {
        id: id.to_owned(),
        version: "1.0.0".to_owned(),
        owner: RuntimeOwner::ModuleWorker,
        capability_ids: vec![capability.to_owned()],
        dependencies: Vec::new(),
        schedule: FixedStepContract {
            step_micros: 16_667,
            phase,
            after: Vec::new(),
        },
        resource_kinds: vec!["scene_snapshot".to_owned()],
        consumes_events: vec![EventContract {
            id: "runtime.input".to_owned(),
            schema: "bhippi-runtime-input@1".to_owned(),
            capacity: 64,
            ordering: EventOrdering::GlobalSequence,
            backpressure: QueueBackpressure::StopRuntime,
        }],
        emits_events: vec![EventContract {
            id: "runtime.frame".to_owned(),
            schema: "bhippi-runtime-frame@1".to_owned(),
            capacity: 64,
            ordering: EventOrdering::GlobalSequence,
            backpressure: QueueBackpressure::CoalesceLatest,
        }],
        jobs: vec![JobContract {
            lane: ExecutionLane::Deterministic,
            cancellable: true,
            result_ordered_at_safe_point: true,
        }],
        budgets: RuntimeBudgetContract {
            cpu_micros_per_tick: 2_000,
            resident_bytes: 8 * 1024 * 1024,
            emitted_events_per_tick: 64,
            queued_jobs: 8,
        },
        hot_reload: HotReloadContract {
            enabled: true,
            safe_point: ReloadSafePoint::AfterTick,
            validate_before_swap: true,
            rollback_on_fault: true,
        },
        telemetry: vec![
            TelemetryField {
                name: "resident_bytes".to_owned(),
                unit: "bytes".to_owned(),
                cumulative: false,
            },
            TelemetryField {
                name: "cpu_micros".to_owned(),
                unit: "microseconds".to_owned(),
                cumulative: true,
            },
        ],
        fault: FaultContract {
            containment: FaultContainment::StopSubsystem,
            rollback_to_safe_point: true,
            preserve_diagnostic: true,
            restartable: true,
        },
        platforms: platforms(),
    }
}

#[test]
fn valid_contracts_are_sorted_hashed_and_bound_to_capability_truth() {
    let capabilities = CapabilityRegistry::core().expect("capability registry");
    let input = contract("runtime.input", "component.transform", SchedulePhase::Input);
    let mut gameplay = contract(
        "runtime.gameplay",
        "component.script_ref",
        SchedulePhase::Gameplay,
    );
    gameplay.dependencies = vec![input.id.clone()];
    gameplay.schedule.after = vec![input.id.clone()];

    let first = SubsystemContractSet::build(vec![gameplay.clone(), input.clone()], &capabilities)
        .expect("valid subsystem set");
    let second = SubsystemContractSet::build(vec![input, gameplay], &capabilities)
        .expect("same set in another order");

    assert_eq!(first, second);
    assert_eq!(first.format, SUBSYSTEM_FORMAT);
    assert_eq!(first.capability_registry_hash, capabilities.hash);
    assert_eq!(first.hash.len(), 64);
    assert_eq!(first.contracts[0].id, "runtime.gameplay");
    assert!(first.get("runtime.input").is_some());
}

#[test]
fn lifecycle_is_explicit_and_restart_never_skips_quiescence() {
    assert!(LifecycleState::Registered.allows(LifecycleState::Loading));
    assert!(LifecycleState::Loading.allows(LifecycleState::Ready));
    assert!(LifecycleState::Ready.allows(LifecycleState::Running));
    assert!(LifecycleState::Running.allows(LifecycleState::Quiescing));
    assert!(LifecycleState::Quiescing.allows(LifecycleState::Ready));
    assert!(LifecycleState::Faulted.allows(LifecycleState::Stopped));
    assert!(!LifecycleState::Running.allows(LifecycleState::Ready));
    assert!(!LifecycleState::Faulted.allows(LifecycleState::Running));
}

#[test]
fn runtime_world_and_resources_have_handles_not_paths_or_authored_writes() {
    let resource = RuntimeResourceHandle {
        id: 7,
        generation: 2,
    };
    let entity = RuntimeEntityHandle {
        id: 9,
        generation: 1,
    };
    let query = RuntimeWorldQuery::Component {
        entity,
        name: "Transform".to_owned(),
    };
    let command = RuntimeWorldCommand::PatchRuntimeComponent {
        entity,
        component: "Transform".to_owned(),
        value: serde_json::json!({"pos": [1, 2, 3]}),
    };
    let wire = serde_json::to_string(&(resource, query, command)).expect("serializes");
    assert!(!wire.contains("path"));
    assert!(!wire.contains("authored"));
    assert!(!wire.contains("transaction"));
}

#[test]
fn unknown_capability_dangling_dependency_and_cycle_fail_closed() {
    let capabilities = CapabilityRegistry::core().expect("capability registry");
    let mut unknown = contract(
        "runtime.unknown",
        "missing.capability",
        SchedulePhase::Gameplay,
    );
    assert!(SubsystemContractSet::build(vec![unknown.clone()], &capabilities).is_err());

    unknown.capability_ids = vec!["component.transform".to_owned()];
    unknown.dependencies = vec!["runtime.missing".to_owned()];
    assert!(SubsystemContractSet::build(vec![unknown], &capabilities).is_err());

    let mut left = contract(
        "runtime.left",
        "component.transform",
        SchedulePhase::PrePhysics,
    );
    let mut right = contract(
        "runtime.right",
        "component.rigid_body",
        SchedulePhase::Physics,
    );
    left.dependencies = vec![right.id.clone()];
    right.schedule.after = vec![left.id.clone()];
    assert!(SubsystemContractSet::build(vec![left, right], &capabilities).is_err());
}

#[test]
fn budgets_queues_deterministic_lane_and_hot_reload_are_hard_gates() {
    let capabilities = CapabilityRegistry::core().expect("capability registry");
    let base = contract(
        "runtime.test",
        "component.transform",
        SchedulePhase::Gameplay,
    );

    let mut invalid = base.clone();
    invalid.budgets.cpu_micros_per_tick = 0;
    assert!(SubsystemContractSet::build(vec![invalid], &capabilities).is_err());

    let mut invalid = base.clone();
    invalid.emits_events[0].capacity = 0;
    assert!(SubsystemContractSet::build(vec![invalid], &capabilities).is_err());

    let mut invalid = base.clone();
    invalid.jobs[0].lane = ExecutionLane::ParallelBestEffort;
    assert!(SubsystemContractSet::build(vec![invalid], &capabilities).is_err());

    let mut invalid = base;
    invalid.hot_reload.rollback_on_fault = false;
    assert!(SubsystemContractSet::build(vec![invalid], &capabilities).is_err());
}

#[test]
fn platform_profiler_and_fault_claims_require_evidence() {
    let capabilities = CapabilityRegistry::core().expect("capability registry");
    let base = contract(
        "runtime.test",
        "component.transform",
        SchedulePhase::Gameplay,
    );

    let mut invalid = base.clone();
    invalid.platforms.pop();
    assert!(SubsystemContractSet::build(vec![invalid], &capabilities).is_err());

    let mut invalid = base.clone();
    invalid.platforms[0].evidence = None;
    assert!(SubsystemContractSet::build(vec![invalid], &capabilities).is_err());

    let mut invalid = base.clone();
    invalid.telemetry.retain(|field| field.name != "cpu_micros");
    assert!(SubsystemContractSet::build(vec![invalid], &capabilities).is_err());

    let mut invalid = base;
    invalid.fault.preserve_diagnostic = false;
    assert!(SubsystemContractSet::build(vec![invalid], &capabilities).is_err());
}
