//! Unified, bounded profiler and crash-evidence contracts for Phase 24.
//!
//! The schema does not claim capture backends exist. It gives every future subsystem one stable,
//! redacted evidence shape and gives AI queries a compact deterministic projection.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const TRACE_FORMAT: &str = "bhippi-runtime-trace@1";
pub const CRASH_BUNDLE_FORMAT: &str = "bhippi-crash-bundle@1";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct RuntimeTrace {
    pub format: String,
    pub capture_id: String,
    pub session_nonce_hash: String,
    pub platform: String,
    pub build_id: String,
    pub started_micros: u64,
    pub ended_micros: u64,
    pub events: Vec<TraceEvent>,
    pub dropped_events: u64,
    pub counters: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum TraceEvent {
    CpuSpan {
        system: String,
        label: String,
        start_micros: u64,
        end_micros: u64,
        thread: u32,
    },
    GpuPass {
        pass: String,
        start_micros: u64,
        end_micros: u64,
        draw_calls: u32,
    },
    Memory {
        at_micros: u64,
        subsystem: String,
        resident_bytes: u64,
        gpu_bytes: Option<u64>,
    },
    Counter {
        at_micros: u64,
        subsystem: String,
        name: String,
        value: f64,
        unit: String,
    },
    Fault {
        at_micros: u64,
        subsystem: String,
        code: String,
        message: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct TraceLimits {
    pub events: usize,
    pub encoded_bytes: usize,
    pub text_bytes: usize,
    pub duration_micros: u64,
}

impl Default for TraceLimits {
    fn default() -> Self {
        Self {
            events: 1_000_000,
            encoded_bytes: 64 * 1_024 * 1_024,
            text_bytes: 4 * 1_024 * 1_024,
            duration_micros: 30 * 60 * 1_000_000,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TraceSummary {
    pub capture_id: String,
    pub duration_micros: u64,
    pub event_count: usize,
    pub dropped_events: u64,
    pub total_cpu_micros_by_system: BTreeMap<String, u64>,
    pub total_gpu_micros_by_pass: BTreeMap<String, u64>,
    pub peak_resident_bytes: u64,
    pub peak_gpu_bytes: Option<u64>,
    pub fault_codes: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CrashBundleManifest {
    pub format: String,
    pub bundle_id: String,
    pub build_id: String,
    pub authored_tree_hash: String,
    pub trace_relative_path: String,
    pub replay_relative_path: Option<String>,
    pub game_debug_report_relative_path: Option<String>,
    pub symbol_ids: Vec<String>,
    pub redaction_version: String,
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum ProfilerContractError {
    #[error("unsupported profiler contract format `{0}`")]
    UnsupportedFormat(String),
    #[error("profiler field `{0}` cannot be empty")]
    EmptyField(&'static str),
    #[error("profiler capture limit `{resource}` exceeded ({actual} > {limit})")]
    Limit {
        resource: &'static str,
        actual: u64,
        limit: u64,
    },
    #[error("profiler event lies outside the capture interval")]
    EventOutsideCapture,
    #[error("profiler value is not finite")]
    NonFinite,
    #[error("crash bundle path must be relative and traversal-free: `{0}`")]
    UnsafePath(String),
    #[error("profiler contract could not be encoded: {0}")]
    Encoding(String),
}

impl RuntimeTrace {
    pub fn validate(&self, limits: &TraceLimits) -> Result<(), ProfilerContractError> {
        if self.format != TRACE_FORMAT {
            return Err(ProfilerContractError::UnsupportedFormat(
                self.format.clone(),
            ));
        }
        for (field, value) in [
            ("capture_id", self.capture_id.as_str()),
            ("session_nonce_hash", self.session_nonce_hash.as_str()),
            ("platform", self.platform.as_str()),
            ("build_id", self.build_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(ProfilerContractError::EmptyField(field));
            }
        }
        if self.ended_micros < self.started_micros {
            return Err(ProfilerContractError::EventOutsideCapture);
        }
        let duration = self.ended_micros - self.started_micros;
        if duration > limits.duration_micros {
            return Err(ProfilerContractError::Limit {
                resource: "duration_micros",
                actual: duration,
                limit: limits.duration_micros,
            });
        }
        if self.events.len() > limits.events {
            return Err(ProfilerContractError::Limit {
                resource: "events",
                actual: self.events.len() as u64,
                limit: limits.events as u64,
            });
        }
        let mut text_bytes = 0_usize;
        for event in &self.events {
            let (start, end, text) = event_bounds_and_text(event)?;
            if start < self.started_micros || end > self.ended_micros || end < start {
                return Err(ProfilerContractError::EventOutsideCapture);
            }
            text_bytes = text_bytes.saturating_add(text);
        }
        if self.counters.values().any(|value| !value.is_finite()) {
            return Err(ProfilerContractError::NonFinite);
        }
        if text_bytes > limits.text_bytes {
            return Err(ProfilerContractError::Limit {
                resource: "text_bytes",
                actual: text_bytes as u64,
                limit: limits.text_bytes as u64,
            });
        }
        let encoded = serde_json::to_vec(self)
            .map_err(|error| ProfilerContractError::Encoding(error.to_string()))?;
        if encoded.len() > limits.encoded_bytes {
            return Err(ProfilerContractError::Limit {
                resource: "encoded_bytes",
                actual: encoded.len() as u64,
                limit: limits.encoded_bytes as u64,
            });
        }
        Ok(())
    }

    #[must_use]
    pub fn summary(&self) -> TraceSummary {
        let mut cpu = BTreeMap::<String, u64>::new();
        let mut gpu = BTreeMap::<String, u64>::new();
        let mut faults = BTreeMap::<String, u64>::new();
        let mut peak_resident = 0_u64;
        let mut peak_gpu = None::<u64>;
        for event in &self.events {
            match event {
                TraceEvent::CpuSpan {
                    system,
                    start_micros,
                    end_micros,
                    ..
                } => {
                    *cpu.entry(system.clone()).or_default() +=
                        end_micros.saturating_sub(*start_micros);
                }
                TraceEvent::GpuPass {
                    pass,
                    start_micros,
                    end_micros,
                    ..
                } => {
                    *gpu.entry(pass.clone()).or_default() +=
                        end_micros.saturating_sub(*start_micros);
                }
                TraceEvent::Memory {
                    resident_bytes,
                    gpu_bytes,
                    ..
                } => {
                    peak_resident = peak_resident.max(*resident_bytes);
                    if let Some(bytes) = gpu_bytes {
                        peak_gpu = Some(peak_gpu.unwrap_or_default().max(*bytes));
                    }
                }
                TraceEvent::Fault { code, .. } => {
                    *faults.entry(code.clone()).or_default() += 1;
                }
                TraceEvent::Counter { .. } => {}
            }
        }
        TraceSummary {
            capture_id: self.capture_id.clone(),
            duration_micros: self.ended_micros.saturating_sub(self.started_micros),
            event_count: self.events.len(),
            dropped_events: self.dropped_events,
            total_cpu_micros_by_system: cpu,
            total_gpu_micros_by_pass: gpu,
            peak_resident_bytes: peak_resident,
            peak_gpu_bytes: peak_gpu,
            fault_codes: faults,
        }
    }
}

impl CrashBundleManifest {
    pub fn validate(&self) -> Result<(), ProfilerContractError> {
        if self.format != CRASH_BUNDLE_FORMAT {
            return Err(ProfilerContractError::UnsupportedFormat(
                self.format.clone(),
            ));
        }
        for path in [
            Some(self.trace_relative_path.as_str()),
            self.replay_relative_path.as_deref(),
            self.game_debug_report_relative_path.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_relative_path(path)?;
        }
        let unique = self.symbol_ids.iter().collect::<BTreeSet<_>>();
        if unique.len() != self.symbol_ids.len() {
            return Err(ProfilerContractError::EmptyField("duplicate_symbol_id"));
        }
        Ok(())
    }
}

fn event_bounds_and_text(event: &TraceEvent) -> Result<(u64, u64, usize), ProfilerContractError> {
    match event {
        TraceEvent::CpuSpan {
            system,
            label,
            start_micros,
            end_micros,
            ..
        } => Ok((*start_micros, *end_micros, system.len() + label.len())),
        TraceEvent::GpuPass {
            pass,
            start_micros,
            end_micros,
            ..
        } => Ok((*start_micros, *end_micros, pass.len())),
        TraceEvent::Memory {
            at_micros,
            subsystem,
            ..
        } => Ok((*at_micros, *at_micros, subsystem.len())),
        TraceEvent::Counter {
            at_micros,
            subsystem,
            name,
            value,
            unit,
        } => {
            if !value.is_finite() {
                return Err(ProfilerContractError::NonFinite);
            }
            Ok((
                *at_micros,
                *at_micros,
                subsystem.len() + name.len() + unit.len(),
            ))
        }
        TraceEvent::Fault {
            at_micros,
            subsystem,
            code,
            message,
        } => Ok((
            *at_micros,
            *at_micros,
            subsystem.len() + code.len() + message.len(),
        )),
    }
}

fn validate_relative_path(path: &str) -> Result<(), ProfilerContractError> {
    let normalized = path.replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(":/")
        || normalized
            .split('/')
            .any(|part| part == ".." || part.is_empty())
    {
        return Err(ProfilerContractError::UnsafePath(path.to_owned()));
    }
    Ok(())
}
