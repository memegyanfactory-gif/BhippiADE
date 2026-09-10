//! Godot export-template detection and management (ADR-0043 §5, GAD-121).
//!
//! Export templates contain the stripped, precompiled engine binaries that Godot links
//! games with for Web and Desktop exports. This module finds installed templates,
//! verifies their version matches the pinned Godot release (`4.7.1-stable`), and provides
//! an explicit install recipe with official SHA-256 pins. Templates are never fetched
//! silently (INV-003, INV-087).

use super::detect::{GODOT_PINNED_TAG, GODOT_PINNED_VERSION};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// The official export templates TPZ archive filename.
pub const TEMPLATES_ARCHIVE_NAME: &str = "Godot_v4.7.1-stable_export_templates.tpz";
/// Official release URL for the pinned export templates.
pub const TEMPLATES_DOWNLOAD_URL: &str =
    "https://github.com/godotengine/godot/releases/download/4.7.1-stable/Godot_v4.7.1-stable_export_templates.tpz";
/// The expected SHA-256 checksum of the official export templates archive.
pub const TEMPLATES_SHA256: &str =
    "e9d82136e0539c3f5693c4e9dfd00d235eaec651ef34a87a7ba74ea759a1036f";

/// Essential template files for Web export.
pub const WEB_TEMPLATE_FILES: &[&str] = &["web_release.zip", "web_debug.zip"];
/// Essential template files for Windows Desktop export.
pub const WINDOWS_TEMPLATE_FILES: &[&str] =
    &["windows_release_x86_64.exe", "windows_debug_x86_64.exe"];

/// Status of installed export templates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct ExportTemplatesStatus {
    pub is_installed: bool,
    pub version: String,
    pub path: PathBuf,
    pub has_web: bool,
    pub has_windows: bool,
    pub missing_files: Vec<String>,
}

/// Information offered to the user to download and install export templates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct TemplateInstallOffer {
    pub version: String,
    pub download_url: String,
    pub archive_name: String,
    pub expected_sha256: String,
    pub target_directory: PathBuf,
    pub instructions: String,
}

/// Locate the expected export templates directory for the current platform and pinned version.
#[must_use]
pub fn templates_install_dir() -> PathBuf {
    let folder_name = format!("{}.stable", GODOT_PINNED_VERSION);

    if cfg!(windows) {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata)
                .join("Godot")
                .join("export_templates")
                .join(folder_name);
        }
    } else if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Godot")
                .join("export_templates")
                .join(folder_name);
        }
    } else {
        // Linux / BSD XDG
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(data_home)
                .join("godot")
                .join("export_templates")
                .join(folder_name);
        } else if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("godot")
                .join("export_templates")
                .join(folder_name);
        }
    }

    PathBuf::from("export_templates").join(folder_name)
}

/// Inspect export templates at the standard platform location or an explicit override.
#[must_use]
pub fn check_export_templates(custom_dir: Option<&Path>) -> ExportTemplatesStatus {
    let dir = custom_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(templates_install_dir);

    if !dir.is_dir() {
        let mut missing = Vec::new();
        missing.extend(WEB_TEMPLATE_FILES.iter().map(|&s| s.to_owned()));
        missing.extend(WINDOWS_TEMPLATE_FILES.iter().map(|&s| s.to_owned()));
        return ExportTemplatesStatus {
            is_installed: false,
            version: GODOT_PINNED_TAG.to_owned(),
            path: dir,
            has_web: false,
            has_windows: false,
            missing_files: missing,
        };
    }

    let mut missing_files = Vec::new();
    let mut has_web = true;
    for file in WEB_TEMPLATE_FILES {
        if !dir.join(file).is_file() {
            has_web = false;
            missing_files.push((*file).to_owned());
        }
    }

    let mut has_windows = true;
    for file in WINDOWS_TEMPLATE_FILES {
        if !dir.join(file).is_file() {
            has_windows = false;
            missing_files.push((*file).to_owned());
        }
    }

    let is_installed = has_web || has_windows;

    ExportTemplatesStatus {
        is_installed,
        version: GODOT_PINNED_TAG.to_owned(),
        path: dir,
        has_web,
        has_windows,
        missing_files,
    }
}

/// Provide the download and installation recipe without executing any network calls.
#[must_use]
pub fn describe_template_offer() -> TemplateInstallOffer {
    let target = templates_install_dir();
    TemplateInstallOffer {
        version: GODOT_PINNED_TAG.to_owned(),
        download_url: TEMPLATES_DOWNLOAD_URL.to_owned(),
        archive_name: TEMPLATES_ARCHIVE_NAME.to_owned(),
        expected_sha256: TEMPLATES_SHA256.to_owned(),
        target_directory: target.clone(),
        instructions: format!(
            "Download {} from GitHub releases, verify SHA-256, and extract the 'templates' contents into {}",
            TEMPLATES_ARCHIVE_NAME,
            target.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_offer_is_pinned_and_has_sha256() {
        let offer = describe_template_offer();
        assert_eq!(offer.version, "4.7.1-stable");
        assert!(offer.download_url.contains("4.7.1-stable"));
        assert_eq!(offer.expected_sha256.len(), 64);
        assert!(offer.instructions.contains("SHA-256"));
    }

    #[test]
    fn checking_nonexistent_directory_reports_uninstalled() {
        let temp_dir = std::env::temp_dir().join(format!("bhippi_tpl_test_{}", ulid::Ulid::new()));
        let status = check_export_templates(Some(&temp_dir));
        assert!(!status.is_installed);
        assert!(!status.has_web);
        assert!(!status.has_windows);
        assert!(!status.missing_files.is_empty());
    }

    #[test]
    fn checking_populated_directory_reports_installed_targets() {
        let temp_dir = std::env::temp_dir().join(format!("bhippi_tpl_test2_{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Create web templates only
        for file in WEB_TEMPLATE_FILES {
            std::fs::write(temp_dir.join(file), b"PK\x03\x04mock_zip").unwrap();
        }

        let status = check_export_templates(Some(&temp_dir));
        assert!(status.is_installed);
        assert!(status.has_web);
        assert!(!status.has_windows);

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
