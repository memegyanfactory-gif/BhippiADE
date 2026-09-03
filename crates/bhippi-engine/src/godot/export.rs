//! Game export, publishing, Export Doctor, and packaging (ADR-0043, GAD-120…125).
//!
//! Orchestrates Web and Desktop exports, enforces the Export Doctor's safety gates
//! (INV-074 unlicensed asset blocking, template checks, artefact health), generates
//! attribution credits, and packages deliverables for distribution.

use super::credits::{write_credits, CREDITS_FILE};
use super::export_presets::{
    ExportPresets, PresetTarget, WEB_EXPORT_PATH, WEB_PRESET_NAME, WINDOWS_EXPORT_DIR,
    WINDOWS_PRESET_NAME,
};
use super::gates::check_project;
use super::templates::check_export_templates;
use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// Supported export targets.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ExportTarget {
    Web,
    WindowsDesktop,
}

impl ExportTarget {
    #[must_use]
    pub fn preset_target(self) -> PresetTarget {
        match self {
            Self::Web => PresetTarget::Web,
            Self::WindowsDesktop => PresetTarget::Windows,
        }
    }

    #[must_use]
    pub fn preset_name(self) -> &'static str {
        match self {
            Self::Web => WEB_PRESET_NAME,
            Self::WindowsDesktop => WINDOWS_PRESET_NAME,
        }
    }

    #[must_use]
    pub fn default_rel_output(self, game_name: &str) -> String {
        match self {
            Self::Web => WEB_EXPORT_PATH.to_owned(),
            Self::WindowsDesktop => {
                let sanitized = game_name.trim().replace(' ', "_");
                let exe_name = if sanitized.is_empty() {
                    "game.exe".to_owned()
                } else {
                    format!("{sanitized}.exe")
                };
                format!("{WINDOWS_EXPORT_DIR}/{exe_name}")
            }
        }
    }

    #[must_use]
    pub fn export_directory(self) -> &'static str {
        match self {
            Self::Web => "export/web",
            Self::WindowsDesktop => WINDOWS_EXPORT_DIR,
        }
    }
}

/// Findings and verdict from the Export Doctor.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ExportDoctorReport {
    pub passed: bool,
    pub target: Option<ExportTarget>,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub artefacts_checked: Vec<String>,
}

impl ExportDoctorReport {
    #[must_use]
    pub fn new(target: ExportTarget) -> Self {
        Self {
            passed: true,
            target: Some(target),
            blockers: Vec::new(),
            warnings: Vec::new(),
            artefacts_checked: Vec::new(),
        }
    }

    pub fn block(&mut self, message: impl Into<String>) {
        self.blockers.push(message.into());
        self.passed = false;
    }

    pub fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}

/// Run pre-export verification over the project.
pub fn pre_export_doctor(
    project_root: &Path,
    target: ExportTarget,
    release: bool,
) -> ExportDoctorReport {
    let mut doctor = ExportDoctorReport::new(target);

    // 1. Verify project manifest exists and declares godot runtime
    let manifest_path = project_root.join(crate::GAME_MANIFEST_FILE);
    if !manifest_path.is_file() {
        doctor.block("Bhippi.game.toml manifest is missing");
    }

    // 2. Verify export_presets.cfg contains the requested preset
    let presets_path = project_root.join(super::action::EXPORT_PRESETS_FILE);
    match std::fs::read_to_string(&presets_path) {
        Ok(text) => match ExportPresets::parse(&text) {
            Ok(presets) => {
                if !presets.has_preset(target.preset_name()) {
                    doctor.block(format!(
                        "export_presets.cfg has no `{}` preset",
                        target.preset_name()
                    ));
                }
            }
            Err(e) => doctor.block(format!("export_presets.cfg does not parse: {e}")),
        },
        Err(_) => doctor.block("export_presets.cfg is missing"),
    }

    // 3. Project gate check (manifest, scenes, licensing INV-074)
    let gate_report = check_project(project_root, release);
    for blocker in &gate_report.blockers {
        doctor.block(format!("{}: {}", blocker.code, blocker.message));
    }
    for warning in &gate_report.warnings {
        doctor.warn(format!("{}: {}", warning.code, warning.message));
    }

    // 4. Template presence check
    let tpl_status = check_export_templates(None);
    match target {
        ExportTarget::Web if !tpl_status.has_web => {
            doctor.warn("Web export templates not found in standard directory; export may fail if Godot has no templates configured");
        }
        ExportTarget::WindowsDesktop if !tpl_status.has_windows => {
            doctor.warn("Windows export templates not found in standard directory; export may fail if Godot has no templates configured");
        }
        _ => {}
    }

    doctor
}

/// Run post-export verification inspecting the produced files.
pub fn post_export_doctor(project_root: &Path, target: ExportTarget) -> ExportDoctorReport {
    let mut doctor = ExportDoctorReport::new(target);
    let export_dir = project_root.join(target.export_directory());

    if !export_dir.is_dir() {
        doctor.block(format!(
            "export directory does not exist: {}",
            export_dir.display()
        ));
        return doctor;
    }

    // Credits must always be generated
    let credits_path = export_dir.join(CREDITS_FILE);
    if !credits_path.is_file() {
        doctor.block(format!("export is missing {CREDITS_FILE} attribution"));
    } else {
        doctor.artefacts_checked.push(CREDITS_FILE.to_owned());
    }

    match target {
        ExportTarget::Web => {
            for expected in &["index.html", "index.wasm", "index.pck"] {
                let path = export_dir.join(expected);
                if !path.is_file() {
                    doctor.block(format!("web export bundle is missing {expected}"));
                } else {
                    let Ok(meta) = std::fs::metadata(&path) else {
                        doctor.block(format!("cannot read metadata for {expected}"));
                        continue;
                    };
                    if meta.len() == 0 {
                        doctor.block(format!("exported file {expected} is 0 bytes (corrupt)"));
                    } else {
                        doctor.artefacts_checked.push((*expected).to_owned());
                    }
                }
            }
        }
        ExportTarget::WindowsDesktop => {
            let entries = std::fs::read_dir(&export_dir).ok().map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().is_file())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
            });

            let files = entries.unwrap_or_default();
            let has_exe = files.iter().any(|f| f.ends_with(".exe"));
            let has_pck = files.iter().any(|f| f.ends_with(".pck"));

            if !has_exe {
                doctor.block("desktop export missing executable binary (.exe)");
            }
            if !has_pck {
                doctor.warn("desktop export has no standalone .pck (may be embedded in .exe)");
            }
            doctor.artefacts_checked.extend(files);
        }
    }

    doctor
}

/// Write `credits.html` directly into the project's export folder before shipping.
pub fn ensure_export_credits(project_root: &Path, target: ExportTarget) -> Result<PathBuf> {
    let export_dir = project_root.join(target.export_directory());
    std::fs::create_dir_all(&export_dir).map_err(|e| EngineError::Io {
        operation: "create_dir_all",
        path: export_dir.display().to_string(),
        reason: e.to_string(),
        hint: Some("Check workspace directory permissions.".to_owned()),
    })?;

    write_credits(project_root, &export_dir)
}

/// Package an export folder into a zip archive (GAD-125).
pub fn package_export_zip(
    project_root: &Path,
    target: ExportTarget,
    output_zip: &Path,
) -> Result<PathBuf> {
    let export_dir = project_root.join(target.export_directory());
    if !export_dir.is_dir() {
        return Err(EngineError::Build(
            format!("export directory {} does not exist", export_dir.display()),
            Some("Export the game first before packaging.".to_owned()),
        ));
    }

    if let Some(parent) = output_zip.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Pure Rust zip builder (using standard zip format without external dependencies)
    let file = std::fs::File::create(output_zip).map_err(|e| EngineError::Io {
        operation: "create",
        path: output_zip.display().to_string(),
        reason: e.to_string(),
        hint: Some("Check destination file write permissions.".to_owned()),
    })?;

    let mut writer = std::io::BufWriter::new(file);
    create_zip_archive(&export_dir, &mut writer)?;

    Ok(output_zip.to_path_buf())
}

// ---------------------------------------------------------------------------
// Pure, safe, self-contained ZIP writer (PKWARE ZIP format)
// ---------------------------------------------------------------------------

use std::io::Write;

fn create_zip_archive<W: Write>(src_dir: &Path, out: &mut W) -> Result<()> {
    let mut files = Vec::new();
    collect_files_recursive(src_dir, src_dir, &mut files)?;

    let mut central_dir = Vec::new();
    let mut offset = 0u32;

    for (rel_path, abs_path) in &files {
        let data = std::fs::read(abs_path).map_err(|e| EngineError::Io {
            operation: "read",
            path: abs_path.display().to_string(),
            reason: e.to_string(),
            hint: None,
        })?;

        let crc = crc32(&data);
        let uncompressed_size = data.len() as u32;
        let compressed_size = uncompressed_size;
        let path_bytes = rel_path.as_bytes();
        let path_len = path_bytes.len() as u16;

        // Local file header (30 bytes + path)
        out.write_all(b"PK\x03\x04").map_err(io_err)?;
        out.write_all(&20u16.to_le_bytes()).map_err(io_err)?; // version needed
        out.write_all(&0u16.to_le_bytes()).map_err(io_err)?; // flags
        out.write_all(&0u16.to_le_bytes()).map_err(io_err)?; // method: 0 (store)
        out.write_all(&0u16.to_le_bytes()).map_err(io_err)?; // mod time
        out.write_all(&0u16.to_le_bytes()).map_err(io_err)?; // mod date
        out.write_all(&crc.to_le_bytes()).map_err(io_err)?;
        out.write_all(&compressed_size.to_le_bytes())
            .map_err(io_err)?;
        out.write_all(&uncompressed_size.to_le_bytes())
            .map_err(io_err)?;
        out.write_all(&path_len.to_le_bytes()).map_err(io_err)?;
        out.write_all(&0u16.to_le_bytes()).map_err(io_err)?; // extra len
        out.write_all(path_bytes).map_err(io_err)?;
        out.write_all(&data).map_err(io_err)?;

        // Central directory entry (46 bytes + path)
        central_dir.extend_from_slice(b"PK\x01\x02");
        central_dir.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central_dir.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // flags
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // method
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // mod date
        central_dir.extend_from_slice(&crc.to_le_bytes());
        central_dir.extend_from_slice(&compressed_size.to_le_bytes());
        central_dir.extend_from_slice(&uncompressed_size.to_le_bytes());
        central_dir.extend_from_slice(&path_len.to_le_bytes());
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // disk start
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // int attrs
        central_dir.extend_from_slice(&0u32.to_le_bytes()); // ext attrs
        central_dir.extend_from_slice(&offset.to_le_bytes());
        central_dir.extend_from_slice(path_bytes);

        offset += 30 + path_len as u32 + uncompressed_size;
    }

    let cd_start = offset;
    let cd_size = central_dir.len() as u32;
    let num_entries = files.len() as u16;

    out.write_all(&central_dir).map_err(io_err)?;

    // End of central directory record (22 bytes)
    out.write_all(b"PK\x05\x06").map_err(io_err)?;
    out.write_all(&0u16.to_le_bytes()).map_err(io_err)?; // disk num
    out.write_all(&0u16.to_le_bytes()).map_err(io_err)?; // cd start disk
    out.write_all(&num_entries.to_le_bytes()).map_err(io_err)?;
    out.write_all(&num_entries.to_le_bytes()).map_err(io_err)?;
    out.write_all(&cd_size.to_le_bytes()).map_err(io_err)?;
    out.write_all(&cd_start.to_le_bytes()).map_err(io_err)?;
    out.write_all(&0u16.to_le_bytes()).map_err(io_err)?; // comment len

    Ok(())
}

fn collect_files_recursive(
    dir: &Path,
    root: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| EngineError::Io {
        operation: "read_dir",
        path: dir.display().to_string(),
        reason: e.to_string(),
        hint: None,
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, root, out)?;
        } else if path.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push((rel.to_string_lossy().replace('\\', "/"), path));
            }
        }
    }
    Ok(())
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (!((crc & 1) == 0)) as u32;
            crc = (crc >> 1) ^ (0xEDB88320 & (0u32.wrapping_sub(mask)));
        }
    }
    !crc
}

fn io_err(e: std::io::Error) -> EngineError {
    EngineError::Io {
        operation: "write_zip",
        path: "in-memory-or-disk".to_owned(),
        reason: e.to_string(),
        hint: None,
    }
}

/// Project publishing metadata stored in `Bhippi.game.toml` under `[publish]`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct PublishConfig {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub itch_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_export_doctor_flags_missing_manifest_and_presets() {
        let temp_dir = std::env::temp_dir().join(format!("bhippi_doc_test_{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let report = pre_export_doctor(&temp_dir, ExportTarget::Web, false);
        assert!(!report.passed);
        assert!(report.blockers.iter().any(|b| b.contains("manifest")));
        assert!(report
            .blockers
            .iter()
            .any(|b| b.contains("export_presets.cfg")));

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn post_export_doctor_checks_web_artefacts() {
        let temp_dir = std::env::temp_dir().join(format!("bhippi_doc_test2_{}", ulid::Ulid::new()));
        let web_dir = temp_dir.join("export/web");
        std::fs::create_dir_all(&web_dir).unwrap();

        // Initially fails because files are missing
        let empty_report = post_export_doctor(&temp_dir, ExportTarget::Web);
        assert!(!empty_report.passed);

        // Populate valid dummy export files
        std::fs::write(
            web_dir.join("index.html"),
            b"<html><body>game</body></html>",
        )
        .unwrap();
        std::fs::write(web_dir.join("index.wasm"), b"\x00asm\x01\x00\x00\x00").unwrap();
        std::fs::write(web_dir.join("index.pck"), b"GDPC\x00\x00\x00\x00").unwrap();
        std::fs::write(web_dir.join("credits.html"), b"<html>credits</html>").unwrap();

        let ok_report = post_export_doctor(&temp_dir, ExportTarget::Web);
        assert!(ok_report.passed, "blockers: {:?}", ok_report.blockers);
        assert_eq!(ok_report.artefacts_checked.len(), 4);

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn zip_packaging_creates_valid_archive() {
        let temp_dir = std::env::temp_dir().join(format!("bhippi_zip_test_{}", ulid::Ulid::new()));
        let web_dir = temp_dir.join("export/web");
        std::fs::create_dir_all(&web_dir).unwrap();
        std::fs::write(web_dir.join("index.html"), b"hello game").unwrap();
        std::fs::write(web_dir.join("data.txt"), b"some data").unwrap();

        let zip_dest = temp_dir.join("game.zip");
        let res = package_export_zip(&temp_dir, ExportTarget::Web, &zip_dest).unwrap();
        assert!(res.is_file());

        let bytes = std::fs::read(&zip_dest).unwrap();
        assert_eq!(&bytes[0..4], b"PK\x03\x04");

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
