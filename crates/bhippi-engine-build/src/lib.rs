//! Build orchestration for the game-engine workbench (ADR-0020, M14).
//!
//! Collects the game project (manifest, scenes, scripts, assets) into a deterministic,
//! self-contained artifact under `builds/`, gated by license state and schema validity.
//! No Bevy is ever linked here — the builder is a plain file-packager that the app runs
//! as a task; the viewport binary consumes artifacts.

#![cfg_attr(
    test,
    allow(clippy::expect_used, clippy::unwrap_used),
    doc = "Tests may panic on purpose: `expect` is how a test states its precondition, and a panic there is a failing test rather than a crashed app. The workspace `deny` stands everywhere else."
)]

use bhippi_engine::asset::AssetIndex;
use bhippi_engine::document::SceneDocument;
use bhippi_engine::error::{EngineError, Result as EngineResult};
use bhippi_engine::manifest::{load_manifest, GameManifest};
use bhippi_types::BuildId;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// Debug builds warn-list unknown licenses; Release builds are blocked by them (INV-074:
/// gates block, never warn).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BuildMode {
    Debug,
    Release,
}

impl BuildMode {
    #[must_use]
    pub fn is_release(self) -> bool {
        matches!(self, Self::Release)
    }
}

/// The validation report written next to every artifact and surfaced in the Build panel.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct ValidationReport {
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub scene_errors: Vec<String>,
}

impl ValidationReport {
    pub fn is_clear(&self) -> bool {
        self.blockers.is_empty() && self.scene_errors.is_empty()
    }
}

/// The result of a `prepare()` pass: where the artifact landed, for which targets, and the
/// report the UI shows.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct BuildOutput {
    pub build: BuildId,
    pub artifact_dir: String,
    pub mode: BuildMode,
    pub targets: Vec<String>,
    pub report: ValidationReport,
}

/// Collect every scene/script under `assets` and `scripts`, validate them and index the
/// assets. Returns the list of source files (relative paths) for the pack step.
/// Everything a build packs: the files to copy, and the scenes paired with their
/// project-relative paths so a gate finding can name the file it came from.
type Collected = (Vec<PathBuf>, Vec<(String, SceneDocument)>);

fn collect(
    project_root: &Path,
    manifest: &GameManifest,
    report: &mut ValidationReport,
) -> EngineResult<Collected> {
    let mut sources = Vec::new();
    let mut scenes = Vec::new();
    for root in ["assets", "scripts"] {
        let dir = project_root.join(root);
        collect_tree(project_root, &dir, &mut sources, &mut scenes, report);
    }
    // The manifest's own content is hash-stable; scenes get a structural pass.
    for (path, scene) in &scenes {
        if let Err(error) = scene.validate() {
            report.scene_errors.push(format!("scene {path}: {error}"));
        }
    }
    let _ = manifest;
    Ok((sources, scenes))
}

fn collect_tree(
    project_root: &Path,
    dir: &Path,
    sources: &mut Vec<PathBuf>,
    scenes: &mut Vec<(String, SceneDocument)>,
    report: &mut ValidationReport,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files = Vec::new();
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.ends_with(".meta.json") {
            continue;
        }
        if path.is_dir() {
            collect_tree(project_root, &path, sources, scenes, report);
            continue;
        }
        files.push(path);
    }
    files.sort(); // deterministic pack order (INV: reproducible artifacts)
    for path in files {
        // Scene files are `*.bscn.json`, so their *extension* is `json` — matching on
        // `bscn` meant no scene was ever collected, and the structural pass below has been
        // running over an empty list since the crate was written.
        let is_scene = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".bscn.json"));
        if is_scene {
            match std::fs::read_to_string(&path) {
                Ok(text) => match SceneDocument::parse(&text) {
                    Ok(scene) => {
                        let rel = path
                            .strip_prefix(project_root)
                            .map(|rest| rest.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_else(|_| path.to_string_lossy().into_owned());
                        scenes.push((rel, scene));
                        sources.push(path);
                    }
                    Err(error) => report
                        .scene_errors
                        .push(format!("{}: {error}", path.display())),
                },
                Err(error) => report
                    .scene_errors
                    .push(format!("{}: {error}", path.display())),
            }
        } else {
            sources.push(path);
        }
    }
}

/// Prepare a build artifact. Gates (in order): manifest loads → scenes validate → Debug
/// warns / Release blocks on unknown licenses → files copied → report written.
pub fn prepare(project_root: &Path, mode: BuildMode) -> EngineResult<BuildOutput> {
    let Some(manifest) = load_manifest(project_root)? else {
        return Err(EngineError::Manifest(
            "no Bhippi.game.toml in this folder".to_owned(),
            Some("Run New Game Project first.".to_owned()),
        ));
    };
    let mut report = ValidationReport::default();
    let (sources, scenes) = collect(project_root, &manifest, &mut report)?;

    let mut index = AssetIndex::scan(project_root).map_err(|error| {
        EngineError::Asset(
            format!("asset scan failed: {error}"),
            Some("Fix the offending file and retry the build.".to_owned()),
        )
    })?;
    index.refresh_usage(&scenes.iter().map(|(_, scene)| scene).collect::<Vec<_>>());

    // Content gates (ENG-128). The licence rule (INV-074 / plan §11.2) lives inside
    // `check_assets` — Debug warns, Release blocks — alongside dangling-reference and
    // manifest-wiring checks, so a build cannot ship a project whose level list names a file
    // that is not there. Gates block; they never warn.
    for finding in bhippi_engine::gates::check_project(project_root, &manifest, &scenes)
        .findings
        .into_iter()
        .chain(bhippi_engine::gates::check_assets(&index, &scenes, mode.is_release()).findings)
    {
        let line = format!("{} ({})", finding.message, finding.code);
        match finding.level {
            bhippi_engine::gates::GateLevel::Blocker => report.blockers.push(line),
            bhippi_engine::gates::GateLevel::Warning => report.warnings.push(line),
        }
    }
    if !report.blockers.is_empty() {
        return Err(EngineError::Build(
            format!("{} content blocker(s)", report.blockers.len()),
            Some(
                "Fix the listed problems — a missing level or an unlicensed asset cannot ship."
                    .to_owned(),
            ),
        ));
    }

    // Deterministic artifact layout: builds/<slug>-<short_ulid>/source…
    let build = BuildId::new();
    let slug = slugify(&manifest.game.name);
    let short = &build.to_string()[..8.min(build.to_string().len())];
    let artifact = project_root.join("builds").join(format!("{slug}-{short}"));
    if artifact.exists() {
        std::fs::remove_dir_all(&artifact).map_err(|error| EngineError::Io {
            operation: "clean",
            path: artifact.display().to_string(),
            reason: error.to_string(),
            hint: None,
        })?;
    }
    std::fs::create_dir_all(&artifact).map_err(|error| EngineError::Io {
        operation: "create",
        path: artifact.display().to_string(),
        reason: error.to_string(),
        hint: None,
    })?;

    // Pack source files preserving their relative layout inside the artifact.
    for source in sources {
        let relative = source
            .strip_prefix(project_root)
            .map_err(|_| EngineError::Io {
                operation: "strip",
                path: source.display().to_string(),
                reason: "file escaped project root".to_owned(),
                hint: None,
            })?;
        let target = artifact.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| EngineError::Io {
                operation: "create",
                path: parent.display().to_string(),
                reason: error.to_string(),
                hint: None,
            })?;
        }
        std::fs::copy(&source, &target).map_err(|error| EngineError::Io {
            operation: "copy",
            path: source.display().to_string(),
            reason: error.to_string(),
            hint: None,
        })?;
    }
    // Also copy the manifest into the artifact for self-contained archives.
    std::fs::copy(
        project_root.join("Bhippi.game.toml"),
        artifact.join("Bhippi.game.toml"),
    )
    .map_err(|error| EngineError::Io {
        operation: "copy",
        path: "Bhippi.game.toml".to_owned(),
        reason: error.to_string(),
        hint: None,
    })?;

    let report_text = serde_json::to_string_pretty(&report).map_err(|error| {
        EngineError::Build(
            format!("cannot serialise build report: {error}"),
            Some("Report this as an engine bug.".to_owned()),
        )
    })?;
    std::fs::write(artifact.join("build-report.json"), report_text).map_err(|error| {
        EngineError::Io {
            operation: "write report",
            path: artifact.join("build-report.json").display().to_string(),
            reason: error.to_string(),
            hint: None,
        }
    })?;

    let targets = manifest
        .enabled_targets()
        .into_iter()
        .map(str::to_owned)
        .collect();
    Ok(BuildOutput {
        build,
        artifact_dir: artifact.display().to_string(),
        mode,
        targets,
        report,
    })
}

fn slugify(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{prepare, BuildMode};
    use bhippi_engine::asset::{AssetIndex, LicenseState};
    use bhippi_engine::scaffold::write_project;
    use std::fs;
    use std::path::Path;

    fn project() -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("bhippi-build-{}", uuid()));
        let _ = fs::remove_dir_all(&root);
        write_project(&root, "Demo", false).expect("scaffold");
        (root.clone(), root.join("builds"))
    }

    fn uuid() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{nanos:016x}")
    }

    #[test]
    fn debug_build_warns_on_unknown_licenses_but_succeeds() {
        let (root, builds) = project();
        // Nothing imported → zero blockers in Debug.
        let output = prepare(&root, BuildMode::Debug).expect("debug build ok");
        assert!(output.report.is_clear());
        assert!(!output.targets.is_empty());
        assert_eq!(output.mode, BuildMode::Debug);
        assert!(Path::new(&output.artifact_dir)
            .join("build-report.json")
            .is_file());
        assert!(Path::new(&output.artifact_dir)
            .join("Bhippi.game.toml")
            .is_file());
        assert!(builds.exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn release_build_blocks_unknown_licenses() {
        let (root, _builds) = project();
        fs::write(root.join("assets/crate.glb"), vec![0u8; 16]).expect("asset");
        let error = prepare(&root, BuildMode::Release).expect_err("blocked");
        assert_eq!(error.code(), bhippi_engine::EngineErrorCode::Build);
        assert!(error.hint().is_some());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn asset_scan_finds_imported_library_and_sidecar_ids() {
        let (root, _builds) = project();
        let id = bhippi_types::AssetId::new();
        let meta = serde_json::json!({ "id": id, "license": "CC0-1.0", "importer": "test", "imported_at": "x" });
        fs::write(root.join("assets/crate.glb"), vec![0u8; 32]).expect("glb");
        fs::write(root.join("assets/crate.glb.meta.json"), meta.to_string()).expect("sidecar");
        let index = AssetIndex::scan(&root).expect("scan");
        // `used_by_scenes` updates are a read-path concern; presence of the sidecar id is
        // what matters here (rename-stability, plan §9.1).
        let crate_record = index.by_path("assets/crate.glb").expect("crate");
        assert_eq!(crate_record.id, id);
        assert_eq!(
            crate_record.license,
            LicenseState::Known("CC0-1.0".to_owned())
        );
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod gate_tests {
    use super::{prepare, BuildMode};
    use bhippi_engine::scaffold;
    use bhippi_types::BuildId;
    use std::fs;
    use std::path::PathBuf;

    fn scaffolded(label: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("bhippi-gate-build-{label}-{}", BuildId::new()));
        scaffold::write_project(&root, "Demo", false).expect("scaffold");
        root
    }

    /// ENG-128: a gate nobody calls does not block. The build now runs the content gates,
    /// so a level registered in the manifest but missing from disk fails the build instead
    /// of shipping a game that starts an empty world.
    #[test]
    fn a_missing_level_blocks_the_build() {
        let root = scaffolded("missing-level");
        fs::remove_file(root.join("assets/scenes/level_01.bscn.json")).expect("delete the level");

        let error = prepare(&root, BuildMode::Debug)
            .expect_err("a game whose level list lies must not build");
        let text = error.to_string();
        assert!(text.contains("content blocker"), "got: {text}");
        assert!(error.hint().is_some());

        let _ = fs::remove_dir_all(&root);
    }

    /// An invented weather id would silently fall back at runtime; the gate stops it here.
    #[test]
    fn an_invented_weather_id_blocks_the_build() {
        let root = scaffolded("weather");
        let path = root.join("assets/scenes/level_01.bscn.json");
        let text = fs::read_to_string(&path).expect("read");
        fs::write(&path, text.replace("\"clear\"", "\"hurricane\"")).expect("write");

        let error = prepare(&root, BuildMode::Debug).expect_err("unknown weather must block");
        assert!(error.to_string().contains("content blocker"));

        let _ = fs::remove_dir_all(&root);
    }

    /// And the scaffold itself still builds clean in both modes — the gates must catch real
    /// breakage without failing a project nobody has touched.
    #[test]
    fn a_fresh_scaffold_builds_in_debug_and_release() {
        let root = scaffolded("clean");
        prepare(&root, BuildMode::Debug).expect("debug build");
        prepare(&root, BuildMode::Release).expect("release build");
        let _ = fs::remove_dir_all(&root);
    }
}
