#![allow(clippy::expect_used)]

use bhippi_engine::profiler_contract::{
    CrashBundleManifest, ProfilerContractError, RuntimeTrace, TraceEvent, TraceLimits,
    CRASH_BUNDLE_FORMAT, TRACE_FORMAT,
};
use std::collections::BTreeMap;

fn trace() -> RuntimeTrace {
    RuntimeTrace {
        format: TRACE_FORMAT.to_owned(),
        capture_id: "capture-1".to_owned(),
        session_nonce_hash: "nonce-hash".to_owned(),
        platform: "windows-x86_64".to_owned(),
        build_id: "build-1".to_owned(),
        started_micros: 100,
        ended_micros: 1_000,
        events: vec![
            TraceEvent::CpuSpan {
                system: "physics".to_owned(),
                label: "step".to_owned(),
                start_micros: 120,
                end_micros: 170,
                thread: 1,
            },
            TraceEvent::GpuPass {
                pass: "main".to_owned(),
                start_micros: 200,
                end_micros: 260,
                draw_calls: 14,
            },
            TraceEvent::Memory {
                at_micros: 300,
                subsystem: "assets".to_owned(),
                resident_bytes: 4_096,
                gpu_bytes: Some(2_048),
            },
        ],
        dropped_events: 0,
        counters: BTreeMap::new(),
    }
}

#[test]
fn trace_validates_and_projects_a_compact_deterministic_summary() {
    let trace = trace();
    trace
        .validate(&TraceLimits::default())
        .expect("fixture valid");
    let summary = trace.summary();
    assert_eq!(summary.total_cpu_micros_by_system["physics"], 50);
    assert_eq!(summary.total_gpu_micros_by_pass["main"], 60);
    assert_eq!(summary.peak_resident_bytes, 4_096);
    assert_eq!(summary.peak_gpu_bytes, Some(2_048));
}

#[test]
fn invalid_time_non_finite_counter_and_event_flood_fail_closed() {
    let mut outside = trace();
    outside.events.push(TraceEvent::Fault {
        at_micros: 2_000,
        subsystem: "runtime".to_owned(),
        code: "timeout".to_owned(),
        message: "bounded".to_owned(),
    });
    assert_eq!(
        outside.validate(&TraceLimits::default()),
        Err(ProfilerContractError::EventOutsideCapture)
    );

    let mut non_finite = trace();
    non_finite.counters.insert("fps".to_owned(), f64::NAN);
    assert_eq!(
        non_finite.validate(&TraceLimits::default()),
        Err(ProfilerContractError::NonFinite)
    );

    let limits = TraceLimits {
        events: 1,
        ..TraceLimits::default()
    };
    assert!(matches!(
        trace().validate(&limits),
        Err(ProfilerContractError::Limit {
            resource: "events",
            ..
        })
    ));
}

#[test]
fn crash_bundle_accepts_only_relative_traversal_free_evidence_paths() {
    let fixture = CrashBundleManifest {
        format: CRASH_BUNDLE_FORMAT.to_owned(),
        bundle_id: "crash-1".to_owned(),
        build_id: "build-1".to_owned(),
        authored_tree_hash: "tree-hash".to_owned(),
        trace_relative_path: "diagnostics/trace.json".to_owned(),
        replay_relative_path: Some("diagnostics/replay.json".to_owned()),
        game_debug_report_relative_path: None,
        symbol_ids: vec!["app.pdb:abc".to_owned()],
        redaction_version: "redaction@1".to_owned(),
    };
    assert_eq!(fixture.validate(), Ok(()));

    let escaped = CrashBundleManifest {
        trace_relative_path: "../secret.txt".to_owned(),
        ..fixture
    };
    assert!(matches!(
        escaped.validate(),
        Err(ProfilerContractError::UnsafePath(_))
    ));
}
