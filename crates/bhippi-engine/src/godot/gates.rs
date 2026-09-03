//! Godot project gates. These **block**; they do not advise.
//!
//! Every check here is something that turns into a broken project later: a scene pointing at
//! a script that was renamed, an autoload the game calls but never registered, an export
//! preset the CLI needs and cannot find. Godot itself reports most of these as a red line in
//! its output at run time, by which point the failure is a mystery to whoever pressed Play.
//!
//! Findings carry stable `BHP-GD-4xx` codes so the UI, the tests and the agent's repair loop
//! can all key on the same thing. `release` decides severity for the two checks whose answer
//! genuinely depends on it — asset licensing and the web export preset — because a debug run
//! on your own machine is not a thing you ship (INV-074's rule, applied to Godot).

use super::export_presets::{ExportPresets, WEB_PRESET_NAME};
use super::manifest::is_godot;
use super::probe::{PROBE_AUTOLOAD_NAME, PROBE_REL_PATH, PROBE_RES_PATH};
use super::project::GodotProjectFile;
use super::{res_to_rel, ASSETS_DIR, SCENES_DIR};
use crate::manifest::load_manifest;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// The manifest is missing or unreadable.
pub const CODE_MANIFEST: &str = "BHP-GD-401";
/// The manifest is not a Godot project's.
pub const CODE_RUNTIME: &str = "BHP-GD-402";
/// `project.godot` is missing.
pub const CODE_PROJECT_FILE: &str = "BHP-GD-403";
/// `project.godot` does not parse.
pub const CODE_PROJECT_PARSE: &str = "BHP-GD-404";
/// No main scene is set.
pub const CODE_MAIN_SCENE_UNSET: &str = "BHP-GD-405";
/// The main scene file is not there.
pub const CODE_MAIN_SCENE_MISSING: &str = "BHP-GD-406";
/// A scene under `scenes/` does not parse.
pub const CODE_SCENE_PARSE: &str = "BHP-GD-407";
/// A scene references an external resource that is not on disk.
pub const CODE_DANGLING_RESOURCE: &str = "BHP-GD-408";
/// A referenced script is not on disk.
pub const CODE_MISSING_SCRIPT: &str = "BHP-GD-409";
/// The probe autoload is not registered.
pub const CODE_PROBE_AUTOLOAD: &str = "BHP-GD-410";
/// The probe script is not on disk.
pub const CODE_PROBE_SCRIPT: &str = "BHP-GD-411";
/// `export_presets.cfg` has no Web preset (or is missing entirely).
pub const CODE_WEB_PRESET: &str = "BHP-GD-412";
/// An asset has no licence sidecar.
pub const CODE_LICENSE_MISSING: &str = "BHP-GD-413";
/// An asset's licence is `unknown`.
pub const CODE_LICENSE_UNKNOWN: &str = "BHP-GD-414";
/// The manifest's `[godot].main_scene` disagrees with `project.godot`.
pub const CODE_MAIN_SCENE_DRIFT: &str = "BHP-GD-415";

/// How many files the walkers will look at before giving up. A project this large is not
/// one a gate should stall the UI over.
pub const MAX_SCANNED_FILES: usize = 20_000;
/// How deep the walkers descend.
pub const MAX_SCAN_DEPTH: usize = 12;
/// The sidecar suffix that states an asset's licence.
pub const LICENSE_SIDECAR_SUFFIX: &str = ".meta.json";
/// The licence value that means "nobody has said".
pub const LICENSE_UNKNOWN: &str = "unknown";

/// One thing wrong with the project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct Finding {
    /// A stable `BHP-GD-4xx` code.
    pub code: String,
    pub message: String,
    pub hint: String,
    /// The file (or setting) the finding is about, project-relative.
    pub where_: String,
}

/// What the check found. Blockers stop a build; warnings do not.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GateReport {
    pub blockers: Vec<Finding>,
    pub warnings: Vec<Finding>,
}

impl GateReport {
    /// True when nothing blocks.
    #[must_use]
    pub fn passes(&self) -> bool {
        self.blockers.is_empty()
    }

    #[must_use]
    pub fn codes(&self) -> Vec<String> {
        self.blockers
            .iter()
            .chain(self.warnings.iter())
            .map(|finding| finding.code.clone())
            .collect()
    }

    #[must_use]
    pub fn has(&self, code: &str) -> bool {
        self.blockers
            .iter()
            .chain(self.warnings.iter())
            .any(|finding| finding.code == code)
    }

    fn block(&mut self, code: &str, message: String, hint: &str, where_: &str) {
        self.blockers.push(Finding {
            code: code.to_owned(),
            message,
            hint: hint.to_owned(),
            where_: where_.to_owned(),
        });
    }

    fn warn(&mut self, code: &str, message: String, hint: &str, where_: &str) {
        self.warnings.push(Finding {
            code: code.to_owned(),
            message,
            hint: hint.to_owned(),
            where_: where_.to_owned(),
        });
    }

    /// A finding whose severity depends on whether this is a release.
    fn gate(&mut self, release: bool, code: &str, message: String, hint: &str, where_: &str) {
        if release {
            self.block(code, message, hint, where_);
        } else {
            self.warn(code, message, hint, where_);
        }
    }
}

/// Check a Godot project. Pure apart from reading files; nothing is repaired.
#[must_use]
pub fn check_project(root: &Path, release: bool) -> GateReport {
    let mut report = GateReport::default();

    // 1. The manifest, and that it says this is a Godot project at all.
    let manifest = match load_manifest(root) {
        Ok(Some(manifest)) => Some(manifest),
        Ok(None) => {
            report.block(
                CODE_MANIFEST,
                format!("{} has no Bhippi.game.toml", root.display()),
                "Create the project with New Godot Project, or add the manifest by hand.",
                crate::GAME_MANIFEST_FILE,
            );
            None
        }
        Err(error) => {
            report.block(
                CODE_MANIFEST,
                format!("Bhippi.game.toml does not parse: {error}"),
                error
                    .hint()
                    .unwrap_or("Fix the manifest syntax and try again."),
                crate::GAME_MANIFEST_FILE,
            );
            None
        }
    };
    if let Some(manifest) = &manifest {
        if !is_godot(manifest) {
            report.block(
                CODE_RUNTIME,
                "the manifest does not say runtime = \"godot\"".to_owned(),
                "Add `runtime = \"godot\"` above the first [table] in Bhippi.game.toml.",
                crate::GAME_MANIFEST_FILE,
            );
        }
    }

    // 2. project.godot must be there and must parse; without it nothing else is meaningful.
    let project_path = root.join(super::action::PROJECT_FILE);
    let Some(project_text) = std::fs::read_to_string(&project_path).ok() else {
        report.block(
            CODE_PROJECT_FILE,
            "project.godot is missing".to_owned(),
            "Godot identifies a project by this file; restore it or re-scaffold.",
            super::action::PROJECT_FILE,
        );
        return report;
    };
    let project = match GodotProjectFile::parse(&project_text) {
        Ok(project) => project,
        Err(error) => {
            report.block(
                CODE_PROJECT_PARSE,
                format!("project.godot does not parse: {error}"),
                error
                    .hint()
                    .unwrap_or("Open the project in Godot once to have it rewrite the file."),
                super::action::PROJECT_FILE,
            );
            return report;
        }
    };

    // 3. The main scene: set, present, and agreeing with the manifest.
    match project.main_scene() {
        None => report.block(
            CODE_MAIN_SCENE_UNSET,
            "no run/main_scene is set".to_owned(),
            "Set the main scene in Project Settings, or with the set_main_scene action.",
            super::action::PROJECT_FILE,
        ),
        Some(main_scene) => {
            let rel = res_to_rel(&main_scene);
            if !root.join(&rel).is_file() {
                report.block(
                    CODE_MAIN_SCENE_MISSING,
                    format!("run/main_scene points at {rel}, which is not there"),
                    "Create the scene, or point run/main_scene at one that exists.",
                    &rel,
                );
            }
            if let Some(godot) = manifest
                .as_ref()
                .and_then(|manifest| manifest.godot.as_ref())
            {
                if res_to_rel(&godot.main_scene) != rel {
                    report.warn(
                        CODE_MAIN_SCENE_DRIFT,
                        format!(
                            "Bhippi.game.toml says {} but project.godot says {rel}",
                            godot.main_scene
                        ),
                        "Point [godot].main_scene at the same scene as run/main_scene.",
                        crate::GAME_MANIFEST_FILE,
                    );
                }
            }
        }
    }

    // 4. Every scene under scenes/ parses, and everything it references is on disk.
    for scene_rel in scene_files(root) {
        let Ok(text) = std::fs::read_to_string(root.join(&scene_rel)) else {
            report.block(
                CODE_SCENE_PARSE,
                format!("{scene_rel} could not be read"),
                "Check the file is readable and is text (.tscn), not binary (.scn).",
                &scene_rel,
            );
            continue;
        };
        let document = match super::tscn::parse(&text) {
            Ok(document) => document,
            Err(error) => {
                report.block(
                    CODE_SCENE_PARSE,
                    format!("{scene_rel} does not parse: {error}"),
                    error
                        .hint()
                        .unwrap_or("Open the scene in Godot and save it again."),
                    &scene_rel,
                );
                continue;
            }
        };
        for resource in &document.ext_resources {
            let target = res_to_rel(&resource.path);
            if root.join(&target).is_file() {
                continue;
            }
            if resource.type_ == "Script" {
                report.block(
                    CODE_MISSING_SCRIPT,
                    format!("{scene_rel} attaches {}, which is not there", resource.path),
                    "Write the script, or detach it from the node that carries it.",
                    &scene_rel,
                );
            } else {
                report.block(
                    CODE_DANGLING_RESOURCE,
                    format!(
                        "{scene_rel} references {} ({}), which is not there",
                        resource.path, resource.type_
                    ),
                    "Restore the file, or remove the node that references it.",
                    &scene_rel,
                );
            }
        }
    }

    // 5. The probe: registered as an autoload, and present on disk.
    let autoloads = project.autoloads();
    match autoloads
        .iter()
        .find(|autoload| autoload.name == PROBE_AUTOLOAD_NAME)
    {
        None => report.block(
            CODE_PROBE_AUTOLOAD,
            format!("{PROBE_AUTOLOAD_NAME} is not registered as an autoload"),
            "Playtests report nothing without it. Add it under [autoload] in project.godot.",
            super::action::PROJECT_FILE,
        ),
        Some(autoload) => {
            let rel = res_to_rel(&autoload.path);
            if !root.join(&rel).is_file() {
                report.block(
                    CODE_PROBE_SCRIPT,
                    format!("{PROBE_AUTOLOAD_NAME} points at {rel}, which is not there"),
                    "Restore bhippi/probe.gd, or re-scaffold the probe.",
                    &rel,
                );
            }
        }
    }
    // Every other autoload has to resolve too — a missing one stops the game at start-up.
    for autoload in &autoloads {
        if autoload.name == PROBE_AUTOLOAD_NAME {
            continue;
        }
        let rel = res_to_rel(&autoload.path);
        if !root.join(&rel).is_file() {
            report.block(
                CODE_MISSING_SCRIPT,
                format!(
                    "autoload {} points at {rel}, which is not there",
                    autoload.name
                ),
                "Write the script, or remove the autoload from project.godot.",
                super::action::PROJECT_FILE,
            );
        }
    }

    // 6. The web preset, without which `--export-release Web` has nothing to name.
    let presets_path = root.join(super::action::EXPORT_PRESETS_FILE);
    match std::fs::read_to_string(&presets_path)
        .ok()
        .and_then(|text| ExportPresets::parse(&text).ok())
    {
        Some(presets) if presets.has_preset(WEB_PRESET_NAME) => {}
        Some(_) => report.gate(
            release,
            CODE_WEB_PRESET,
            format!("export_presets.cfg has no `{WEB_PRESET_NAME}` preset"),
            "CLI export names a preset; add the Web preset before exporting.",
            super::action::EXPORT_PRESETS_FILE,
        ),
        None => report.gate(
            release,
            CODE_WEB_PRESET,
            "export_presets.cfg is missing or unreadable".to_owned(),
            "Re-scaffold the export presets; the CLI export needs this file.",
            super::action::EXPORT_PRESETS_FILE,
        ),
    }

    // 7. Licensing. Nothing unlicensed ships (INV-074).
    check_licences(root, release, &mut report);
    report
}

/// Every `.tscn` under `scenes/`, project-relative with forward slashes.
#[must_use]
pub fn scene_files(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    walk(&root.join(SCENES_DIR), root, 0, &mut found, &|path| {
        path.extension()
            .map(|extension| extension.eq_ignore_ascii_case("tscn"))
            .unwrap_or(false)
    });
    found.sort();
    found
}

fn check_licences(root: &Path, release: bool, report: &mut GateReport) {
    let mut assets = Vec::new();
    walk(&root.join(ASSETS_DIR), root, 0, &mut assets, &|path| {
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            return false;
        };
        // Sidecars, Godot's import stubs and hidden files are not themselves assets.
        !name.ends_with(LICENSE_SIDECAR_SUFFIX)
            && !name.ends_with(".import")
            && !name.starts_with('.')
    });
    assets.sort();
    for asset in assets {
        let sidecar = format!("{asset}{LICENSE_SIDECAR_SUFFIX}");
        let Ok(text) = std::fs::read_to_string(root.join(&sidecar)) else {
            report.gate(
                release,
                CODE_LICENSE_MISSING,
                format!("{asset} has no {LICENSE_SIDECAR_SUFFIX} stating its licence"),
                "Add the sidecar with the licence you have for the file; unlicensed assets do not ship.",
                &asset,
            );
            continue;
        };
        if !states_a_licence(&text) {
            report.gate(
                release,
                CODE_LICENSE_UNKNOWN,
                format!("{sidecar} says the licence is {LICENSE_UNKNOWN}"),
                "Record the actual licence, or replace the asset with one you can ship.",
                &asset,
            );
        }
    }
}

/// True when a sidecar names a real licence.
///
/// `LicenseState` is `#[serde(untagged)]`, so `Unknown` is written as JSON `null` and a
/// known licence as a plain string — and a hand-written sidecar may say `"unknown"` in
/// words. All three mean the same thing here.
fn states_a_licence(sidecar_text: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(sidecar_text) else {
        return false;
    };
    let Some(license) = value.get("license") else {
        return false;
    };
    match license.as_str() {
        Some(text) => !text.trim().is_empty() && !text.trim().eq_ignore_ascii_case(LICENSE_UNKNOWN),
        None => false,
    }
}

/// Depth- and count-limited directory walk collecting project-relative paths.
fn walk(
    directory: &Path,
    root: &Path,
    depth: usize,
    found: &mut Vec<String>,
    keep: &dyn Fn(&Path) -> bool,
) {
    if depth > MAX_SCAN_DEPTH || found.len() >= MAX_SCANNED_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if found.len() >= MAX_SCANNED_FILES {
            return;
        }
        if path.is_dir() {
            walk(&path, root, depth + 1, found, keep);
            continue;
        }
        if !keep(&path) {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            found.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// The probe's canonical location, for a caller repairing [`CODE_PROBE_SCRIPT`].
#[must_use]
pub fn probe_locations() -> (&'static str, &'static str) {
    (PROBE_REL_PATH, PROBE_RES_PATH)
}

#[cfg(test)]
mod tests {
    use super::{
        check_project, states_a_licence, CODE_DANGLING_RESOURCE, CODE_LICENSE_MISSING,
        CODE_LICENSE_UNKNOWN, CODE_MAIN_SCENE_MISSING, CODE_MANIFEST, CODE_MISSING_SCRIPT,
        CODE_PROBE_AUTOLOAD, CODE_PROJECT_FILE, CODE_RUNTIME, CODE_WEB_PRESET,
    };
    use crate::godot::scaffold::{write_project, ProjectTemplate};
    use std::path::{Path, PathBuf};

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn scaffolded(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("bhippi-godot-gates-{name}"));
            let _ = std::fs::remove_dir_all(&root);
            write_project(&root, "Gate Demo", ProjectTemplate::ThirdPerson3D, true)
                .expect("scaffold");
            Self(root)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_scaffolded_project_passes_in_debug_and_in_release() {
        let root = TempRoot::scaffolded("clean");
        for release in [false, true] {
            let report = check_project(root.path(), release);
            assert!(
                report.passes(),
                "release={release} blockers: {:?}",
                report.blockers
            );
            assert!(
                report.warnings.is_empty(),
                "release={release} warnings: {:?}",
                report.warnings
            );
        }
    }

    #[test]
    fn a_dangling_resource_blocks_and_names_the_scene() {
        let root = TempRoot::scaffolded("dangling");
        std::fs::remove_file(root.path().join("scripts/player.gd")).expect("remove script");
        let report = check_project(root.path(), false);
        assert!(!report.passes());
        assert!(report.has(CODE_MISSING_SCRIPT));
        let finding = report
            .blockers
            .iter()
            .find(|finding| finding.code == CODE_MISSING_SCRIPT)
            .expect("the finding");
        assert_eq!(finding.where_, "scenes/main.tscn");
        assert!(finding.message.contains("res://scripts/player.gd"));
        assert!(!finding.hint.is_empty());
    }

    #[test]
    fn a_missing_non_script_resource_gets_its_own_code() {
        let root = TempRoot::scaffolded("packed");
        let scene = root.path().join("scenes/main.tscn");
        let text = std::fs::read_to_string(&scene).expect("scene");
        let patched = text.replace(
            "[node name=\"Camera3D\"",
            "[ext_resource type=\"PackedScene\" path=\"res://scenes/gone.tscn\" id=\"9_gone\"]\n\n[node name=\"Camera3D\"",
        );
        std::fs::write(&scene, patched).expect("write");
        let report = check_project(root.path(), false);
        assert!(report.has(CODE_DANGLING_RESOURCE));
        assert!(!report.passes());
    }

    #[test]
    fn removing_the_probe_autoload_blocks_playtesting() {
        let root = TempRoot::scaffolded("probe");
        let project = root.path().join("project.godot");
        let text = std::fs::read_to_string(&project).expect("project");
        let patched: String = text
            .lines()
            .filter(|line| !line.starts_with("BhippiProbe="))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&project, patched).expect("write");
        let report = check_project(root.path(), false);
        assert!(report.has(CODE_PROBE_AUTOLOAD));
    }

    #[test]
    fn a_missing_main_scene_or_project_file_blocks() {
        let root = TempRoot::scaffolded("mainscene");
        std::fs::remove_file(root.path().join("scenes/main.tscn")).expect("remove scene");
        assert!(check_project(root.path(), false).has(CODE_MAIN_SCENE_MISSING));

        std::fs::remove_file(root.path().join("project.godot")).expect("remove project");
        let report = check_project(root.path(), false);
        assert!(report.has(CODE_PROJECT_FILE));
        assert!(!report.passes());
    }

    #[test]
    fn a_folder_that_is_not_a_bhippi_godot_project_says_so() {
        let root = std::env::temp_dir().join("bhippi-godot-gates-empty");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        let report = check_project(&root, false);
        assert!(report.has(CODE_MANIFEST));
        assert!(report.has(CODE_PROJECT_FILE));

        // A Bhippi (non-Godot) project in the same folder is refused for its runtime.
        crate::scaffold::write_project(&root, "Legacy", true).expect("bhippi scaffold");
        let report = check_project(&root, false);
        assert!(report.has(CODE_RUNTIME));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unlicensed_assets_warn_in_debug_and_block_in_release() {
        let root = TempRoot::scaffolded("licence");
        let assets = root.path().join("assets/textures");
        std::fs::create_dir_all(&assets).expect("assets dir");
        std::fs::write(assets.join("brick.png"), [0u8, 1, 2]).expect("asset");

        let debug = check_project(root.path(), false);
        assert!(debug.passes(), "an unlicensed asset only warns in debug");
        assert!(debug.has(CODE_LICENSE_MISSING));

        let release = check_project(root.path(), true);
        assert!(!release.passes());
        assert!(release.has(CODE_LICENSE_MISSING));

        // A sidecar that says nothing is no better than no sidecar.
        std::fs::write(
            assets.join("brick.png.meta.json"),
            r#"{"id":"01JC7B0KZ0TCVZVWY5YE2H3ZZQ","license":null}"#,
        )
        .expect("sidecar");
        assert!(check_project(root.path(), true).has(CODE_LICENSE_UNKNOWN));

        std::fs::write(
            assets.join("brick.png.meta.json"),
            r#"{"id":"01JC7B0KZ0TCVZVWY5YE2H3ZZQ","license":"CC0-1.0"}"#,
        )
        .expect("sidecar");
        let licensed = check_project(root.path(), true);
        assert!(licensed.passes(), "blockers: {:?}", licensed.blockers);
    }

    #[test]
    fn a_project_without_a_web_preset_warns_then_blocks_a_release() {
        let root = TempRoot::scaffolded("preset");
        std::fs::remove_file(root.path().join("export_presets.cfg")).expect("remove presets");
        assert!(check_project(root.path(), false).passes());
        assert!(check_project(root.path(), false).has(CODE_WEB_PRESET));
        assert!(!check_project(root.path(), true).passes());
    }

    #[test]
    fn licence_sidecars_are_read_the_way_both_writers_write_them() {
        assert!(states_a_licence(r#"{"license":"MIT"}"#));
        assert!(states_a_licence(r#"{"license":"project-authored"}"#));
        assert!(!states_a_licence(r#"{"license":null}"#));
        assert!(!states_a_licence(r#"{"license":"unknown"}"#));
        assert!(!states_a_licence(r#"{"license":"  "}"#));
        assert!(!states_a_licence(r#"{}"#));
        assert!(!states_a_licence("not json"));
    }
}
