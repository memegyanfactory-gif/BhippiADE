//! Deterministic export-doctor contracts for Phase 23.
//!
//! A clear report proves only that supplied host/toolchain/package evidence is internally valid.
//! It never claims an export, signature, install, launch, upgrade or rollback was executed.

use crate::BuildMode;
use bhippi_engine::asset::LicenseState;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

pub const EXPORT_DOCTOR_FORMAT: &str = "bhippi-export-doctor@1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExportTarget {
    Windows,
    Macos,
    Linux,
    Web,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExportHost {
    Windows,
    Macos,
    Linux,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ToolchainEvidence {
    pub id: String,
    pub available: bool,
    #[serde(default)]
    pub version: Option<String>,
    /// A log/fixture identifier, never a secret or environment-variable value.
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PackageFileContract {
    pub path: String,
    pub content_hash: String,
    pub size_bytes: u64,
    pub license: LicenseState,
    pub executable: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct DependencyInventoryEntry {
    pub name: String,
    pub version: String,
    pub license: LicenseState,
    pub source_hash: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct SigningContract {
    pub required: bool,
    /// Public label or fingerprint only. Private keys and credentials are forbidden here.
    #[serde(default)]
    pub identity_label: Option<String>,
    #[serde(default)]
    pub timestamp_authority: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct CrashSymbolContract {
    pub required: bool,
    #[serde(default)]
    pub symbol_toolchain: Option<String>,
    #[serde(default)]
    pub output_path: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SmokeLane {
    Install,
    Launch,
    Upgrade,
    Rollback,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct SmokeLaneEvidence {
    pub lane: SmokeLane,
    pub passed: bool,
    pub artifact_hash: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ExportDoctorInput {
    pub format: String,
    pub target: ExportTarget,
    pub host: ExportHost,
    pub mode: BuildMode,
    pub build_hash: String,
    pub source_date_epoch: u64,
    #[serde(default)]
    pub required_toolchains: Vec<String>,
    #[serde(default)]
    pub toolchains: Vec<ToolchainEvidence>,
    pub signing: SigningContract,
    pub crash_symbols: CrashSymbolContract,
    #[serde(default)]
    pub files: Vec<PackageFileContract>,
    #[serde(default)]
    pub dependencies: Vec<DependencyInventoryEntry>,
    pub require_smoke_lanes: bool,
    #[serde(default)]
    pub smoke_lanes: Vec<SmokeLaneEvidence>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ExportDoctorReport {
    pub format: String,
    pub target: ExportTarget,
    pub host: ExportHost,
    pub input_hash: String,
    pub supported: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    /// Always false: this doctor validates evidence and never invokes an exporter.
    pub execution_performed: bool,
}

pub fn run_export_doctor(
    input: &ExportDoctorInput,
) -> Result<ExportDoctorReport, ExportDoctorError> {
    validate_input(input)?;
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    if !host_can_export(input.host, input.target) {
        blockers.push(format!(
            "host {:?} cannot prove target {:?}; use a target-native export host",
            input.host, input.target
        ));
    }

    let toolchains = input
        .toolchains
        .iter()
        .map(|toolchain| (toolchain.id.as_str(), toolchain))
        .collect::<BTreeMap<_, _>>();
    for required in &input.required_toolchains {
        match toolchains.get(required.as_str()) {
            Some(toolchain)
                if toolchain.available
                    && toolchain
                        .version
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
                    && toolchain
                        .evidence
                        .as_deref()
                        .is_some_and(|value| !value.is_empty()) => {}
            _ => blockers.push(format!(
                "required toolchain {required:?} lacks available versioned evidence"
            )),
        }
    }

    if input.signing.required
        && input
            .signing
            .identity_label
            .as_deref()
            .is_none_or(str::is_empty)
    {
        blockers.push("required signing identity label is missing".to_owned());
    }
    if input.crash_symbols.required {
        let Some(symbol_toolchain) = input.crash_symbols.symbol_toolchain.as_deref() else {
            blockers.push("required crash-symbol toolchain is not declared".to_owned());
            return finish_report(input, blockers, warnings);
        };
        if !toolchains
            .get(symbol_toolchain)
            .is_some_and(|toolchain| toolchain.available)
        {
            blockers.push(format!(
                "crash-symbol toolchain {symbol_toolchain:?} is unavailable"
            ));
        }
        if input
            .crash_symbols
            .output_path
            .as_deref()
            .is_none_or(|path| !safe_package_path(path))
        {
            blockers.push("crash-symbol output path is missing or unsafe".to_owned());
        }
    }

    for file in &input.files {
        if input.mode.is_release() && file.license == LicenseState::Unknown {
            blockers.push(format!("package file {:?} has unknown license", file.path));
        }
    }
    for dependency in &input.dependencies {
        if input.mode.is_release() && dependency.license == LicenseState::Unknown {
            blockers.push(format!(
                "dependency {:?} has unknown license",
                dependency.name
            ));
        }
    }
    if input.mode == BuildMode::Debug
        && input
            .files
            .iter()
            .any(|file| file.license == LicenseState::Unknown)
    {
        warnings.push("debug package contains unknown-license files".to_owned());
    }

    if input.require_smoke_lanes {
        let evidence = input
            .smoke_lanes
            .iter()
            .map(|lane| (lane.lane, lane))
            .collect::<BTreeMap<_, _>>();
        for required in [
            SmokeLane::Install,
            SmokeLane::Launch,
            SmokeLane::Upgrade,
            SmokeLane::Rollback,
        ] {
            match evidence.get(&required) {
                Some(lane)
                    if lane.passed
                        && lane.artifact_hash == input.build_hash
                        && !lane.evidence.trim().is_empty() => {}
                _ => blockers.push(format!(
                    "smoke lane {required:?} lacks passing build-bound evidence"
                )),
            }
        }
    } else {
        warnings.push("install/launch/upgrade/rollback smoke evidence was not required".to_owned());
    }
    finish_report(input, blockers, warnings)
}

fn finish_report(
    input: &ExportDoctorInput,
    mut blockers: Vec<String>,
    mut warnings: Vec<String>,
) -> Result<ExportDoctorReport, ExportDoctorError> {
    blockers.sort();
    blockers.dedup();
    warnings.sort();
    warnings.dedup();
    let input_hash = canonical_input_hash(input)?;
    Ok(ExportDoctorReport {
        format: EXPORT_DOCTOR_FORMAT.to_owned(),
        target: input.target,
        host: input.host,
        input_hash,
        supported: blockers.is_empty(),
        blockers,
        warnings,
        execution_performed: false,
    })
}

fn validate_input(input: &ExportDoctorInput) -> Result<(), ExportDoctorError> {
    if input.format != EXPORT_DOCTOR_FORMAT {
        return Err(ExportDoctorError::Invalid(format!(
            "expected {EXPORT_DOCTOR_FORMAT}, got {:?}",
            input.format
        )));
    }
    if input.build_hash.trim().is_empty() || input.source_date_epoch == 0 {
        return Err(ExportDoctorError::Invalid(
            "build hash and reproducible source_date_epoch are required".to_owned(),
        ));
    }
    if input.files.is_empty() || input.dependencies.is_empty() {
        return Err(ExportDoctorError::Invalid(
            "package files and dependency inventory must not be empty".to_owned(),
        ));
    }
    unique_non_empty(&input.required_toolchains, "required toolchain")?;
    let toolchain_ids = input
        .toolchains
        .iter()
        .map(|toolchain| toolchain.id.clone())
        .collect::<Vec<_>>();
    unique_non_empty(&toolchain_ids, "toolchain evidence")?;
    let mut paths = BTreeSet::new();
    for file in &input.files {
        if !safe_package_path(&file.path)
            || file.content_hash.trim().is_empty()
            || file.size_bytes == 0
            || !paths.insert(file.path.as_str())
        {
            return Err(ExportDoctorError::Invalid(format!(
                "package file {:?} is unsafe, empty or duplicated",
                file.path
            )));
        }
    }
    let mut dependencies = BTreeSet::new();
    for dependency in &input.dependencies {
        if dependency.name.trim().is_empty()
            || dependency.version.trim().is_empty()
            || dependency.source_hash.trim().is_empty()
            || !dependencies.insert(dependency.name.as_str())
        {
            return Err(ExportDoctorError::Invalid(
                "dependency inventory has empty or duplicate entries".to_owned(),
            ));
        }
    }
    let lanes = input
        .smoke_lanes
        .iter()
        .map(|lane| lane.lane)
        .collect::<BTreeSet<_>>();
    if lanes.len() != input.smoke_lanes.len() {
        return Err(ExportDoctorError::Invalid(
            "smoke lanes must be unique".to_owned(),
        ));
    }
    Ok(())
}

fn canonical_input_hash(input: &ExportDoctorInput) -> Result<String, ExportDoctorError> {
    let mut canonical = input.clone();
    canonical.required_toolchains.sort();
    canonical
        .toolchains
        .sort_by(|left, right| left.id.cmp(&right.id));
    canonical
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    canonical
        .dependencies
        .sort_by(|left, right| left.name.cmp(&right.name));
    canonical.smoke_lanes.sort_by_key(|lane| lane.lane);
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|error| ExportDoctorError::Encoding(error.to_string()))?;
    Ok(blake3::hash(&bytes).to_hex().to_string())
}

fn unique_non_empty(values: &[String], label: &str) -> Result<(), ExportDoctorError> {
    let unique = values.iter().collect::<BTreeSet<_>>();
    if values.iter().any(|value| value.trim().is_empty()) || unique.len() != values.len() {
        return Err(ExportDoctorError::Invalid(format!(
            "{label} ids must be non-empty and unique"
        )));
    }
    Ok(())
}

fn host_can_export(host: ExportHost, target: ExportTarget) -> bool {
    match target {
        ExportTarget::Windows => host == ExportHost::Windows,
        ExportTarget::Macos => host == ExportHost::Macos,
        ExportTarget::Linux => host == ExportHost::Linux,
        ExportTarget::Web => true,
    }
}

fn safe_package_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    !normalized.trim().is_empty()
        && !normalized.starts_with('/')
        && !normalized.contains("../")
        && !normalized.contains(":/")
        && !normalized.ends_with('/')
}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum ExportDoctorError {
    #[error("invalid export-doctor input: {0}")]
    Invalid(String),
    #[error("cannot encode export-doctor input: {0}")]
    Encoding(String),
}

#[cfg(test)]
mod tests {
    use super::{
        run_export_doctor, CrashSymbolContract, DependencyInventoryEntry, ExportDoctorInput,
        ExportHost, ExportTarget, PackageFileContract, SigningContract, SmokeLane,
        SmokeLaneEvidence, ToolchainEvidence, EXPORT_DOCTOR_FORMAT,
    };
    use crate::BuildMode;
    use bhippi_engine::asset::LicenseState;

    fn input() -> ExportDoctorInput {
        let build_hash = "build-hash-1".to_owned();
        ExportDoctorInput {
            format: EXPORT_DOCTOR_FORMAT.to_owned(),
            target: ExportTarget::Windows,
            host: ExportHost::Windows,
            mode: BuildMode::Release,
            build_hash: build_hash.clone(),
            source_date_epoch: 1_800_000_000,
            required_toolchains: vec!["rust-msvc".to_owned(), "pdb".to_owned()],
            toolchains: vec![
                ToolchainEvidence {
                    id: "rust-msvc".to_owned(),
                    available: true,
                    version: Some("1.85".to_owned()),
                    evidence: Some("doctor/rust.txt".to_owned()),
                },
                ToolchainEvidence {
                    id: "pdb".to_owned(),
                    available: true,
                    version: Some("14.4".to_owned()),
                    evidence: Some("doctor/pdb.txt".to_owned()),
                },
            ],
            signing: SigningContract {
                required: true,
                identity_label: Some("public-fingerprint".to_owned()),
                timestamp_authority: Some("configured-authority-name".to_owned()),
            },
            crash_symbols: CrashSymbolContract {
                required: true,
                symbol_toolchain: Some("pdb".to_owned()),
                output_path: Some("symbols/game.pdb".to_owned()),
            },
            files: vec![PackageFileContract {
                path: "bin/game.exe".to_owned(),
                content_hash: "file-hash".to_owned(),
                size_bytes: 1_024,
                license: LicenseState::Known("AGPL-3.0-only".to_owned()),
                executable: true,
            }],
            dependencies: vec![DependencyInventoryEntry {
                name: "runtime".to_owned(),
                version: "0.1.0".to_owned(),
                license: LicenseState::Known("AGPL-3.0-only".to_owned()),
                source_hash: "source-hash".to_owned(),
            }],
            require_smoke_lanes: true,
            smoke_lanes: [
                SmokeLane::Install,
                SmokeLane::Launch,
                SmokeLane::Upgrade,
                SmokeLane::Rollback,
            ]
            .into_iter()
            .map(|lane| SmokeLaneEvidence {
                lane,
                passed: true,
                artifact_hash: build_hash.clone(),
                evidence: format!("smoke/{lane:?}.json"),
            })
            .collect(),
        }
    }

    #[test]
    fn complete_evidence_is_clear_but_never_claims_execution() {
        let report = run_export_doctor(&input()).expect("doctor");
        assert!(report.supported);
        assert!(report.blockers.is_empty());
        assert!(!report.execution_performed);
    }

    #[test]
    fn wrong_host_missing_evidence_and_unknown_licenses_block_release() {
        let mut value = input();
        value.target = ExportTarget::Macos;
        value.files[0].license = LicenseState::Unknown;
        value.signing.identity_label = None;
        value.smoke_lanes.pop();
        let report = run_export_doctor(&value).expect("doctor report");
        assert!(!report.supported);
        assert!(report.blockers.iter().any(|line| line.contains("host")));
        assert!(report
            .blockers
            .iter()
            .any(|line| line.contains("unknown license")));
        assert!(report.blockers.iter().any(|line| line.contains("signing")));
        assert!(report.blockers.iter().any(|line| line.contains("Rollback")));
    }

    #[test]
    fn doctor_hash_is_order_independent_and_unsafe_package_paths_fail_closed() {
        let first = input();
        let mut reordered = first.clone();
        reordered.required_toolchains.reverse();
        reordered.toolchains.reverse();
        reordered.smoke_lanes.reverse();
        assert_eq!(
            run_export_doctor(&first).expect("first").input_hash,
            run_export_doctor(&reordered).expect("reordered").input_hash
        );

        let mut unsafe_input = first;
        unsafe_input.files[0].path = "../game.exe".to_owned();
        assert!(run_export_doctor(&unsafe_input).is_err());
    }
}
