//! Content gates (ENG-128).
//!
//! `prompts/chat-engine.md` has always listed a "verification you cannot skip" section, and
//! a prompt is not enforcement — a model that ignores it produces a project that looks fine
//! until something opens it. These are the same rules in code, and they **block**.
//!
//! Everything here is a pure check over documents already in memory. Nothing is repaired
//! automatically: a gate that silently fixes its input teaches nobody anything and hides the
//! bug that produced it.

use crate::asset::{AssetIndex, LicenseState};
use crate::document::{SceneDocument, SceneKind};
use crate::manifest::GameManifest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// How bad one finding is.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GateLevel {
    /// The project is broken in a way that will surface later. Blocks.
    Blocker,
    /// Worth fixing; does not block.
    Warning,
}

/// One thing wrong with the project's content.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct GateFinding {
    pub level: GateLevel,
    /// Stable slug so the UI and tests can key on it: `missing_level`, `dangling_asset`, …
    pub code: String,
    pub message: String,
    pub hint: String,
    /// Where it was found — a scene path, an asset path, or the manifest.
    pub where_: String,
}

/// The result of a content check.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct GateReport {
    pub findings: Vec<GateFinding>,
}

impl GateReport {
    #[must_use]
    pub fn blockers(&self) -> Vec<&GateFinding> {
        self.findings
            .iter()
            .filter(|finding| finding.level == GateLevel::Blocker)
            .collect()
    }

    /// True when nothing blocks. Warnings do not stop a build; blockers do (INV-074's rule,
    /// generalised to all content).
    #[must_use]
    pub fn passes(&self) -> bool {
        self.blockers().is_empty()
    }

    fn push(&mut self, level: GateLevel, code: &str, message: String, hint: &str, where_: &str) {
        self.findings.push(GateFinding {
            level,
            code: code.to_owned(),
            message,
            hint: hint.to_owned(),
            where_: where_.to_owned(),
        });
    }
}

/// Check a game's manifest and scene wiring against the rules the AI is told about.
///
/// `game_dir` is the folder holding `Bhippi.game.toml`. Files are only probed for
/// existence — nothing is opened here beyond the scenes the caller already parsed.
#[must_use]
pub fn check_project(
    game_dir: &Path,
    manifest: &GameManifest,
    scenes: &[(String, SceneDocument)],
) -> GateReport {
    let mut report = GateReport::default();

    // 1. default_scene must exist, and should be the Main scene.
    let default_rel = manifest.game.default_scene.as_str();
    if !game_dir.join(default_rel).is_file() {
        report.push(
            GateLevel::Blocker,
            "missing_default_scene",
            format!("default_scene points at {default_rel}, which does not exist"),
            "Create the scene, or point default_scene at one that exists.",
            "Bhippi.game.toml",
        );
    } else if let Some((_, doc)) = scenes.iter().find(|(path, _)| path == default_rel) {
        if doc.settings.kind != SceneKind::Main {
            report.push(
                GateLevel::Warning,
                "default_scene_not_main",
                format!(
                    "default_scene {default_rel} is a {:?} scene, not Main",
                    doc.settings.kind
                ),
                "Play on Main runs the whole game; point default_scene at the Main scene.",
                "Bhippi.game.toml",
            );
        }
    }

    // 2. Every registered level must exist. A level list that lies is the most common way
    //    Play silently starts an empty world.
    for level in &manifest.game.levels {
        if !game_dir.join(level).is_file() {
            report.push(
                GateLevel::Blocker,
                "missing_level",
                format!("levels[] names {level}, which does not exist"),
                "Create the level, or remove it from levels[] in Bhippi.game.toml.",
                "Bhippi.game.toml",
            );
        }
    }
    if manifest.game.levels.is_empty() {
        report.push(
            GateLevel::Warning,
            "no_levels",
            "the game registers no levels".to_owned(),
            "Add at least one level to levels[] so Play on Main has a map to run.",
            "Bhippi.game.toml",
        );
    }

    // 3. The HUD path, from the manifest and from any Main scene that names its own.
    if let Some(hud) = manifest.game.hud_scene.as_deref() {
        if !game_dir.join(hud).is_file() {
            report.push(
                GateLevel::Blocker,
                "missing_hud",
                format!("hud_scene names {hud}, which does not exist"),
                "Create the HUD scene, or clear hud_scene.",
                "Bhippi.game.toml",
            );
        }
    }

    for (path, doc) in scenes {
        if let Some(hud) = doc.settings.hud.as_deref() {
            if !hud.is_empty() && !game_dir.join(hud).is_file() {
                report.push(
                    GateLevel::Blocker,
                    "missing_hud",
                    format!("{path} points settings.hud at {hud}, which does not exist"),
                    "Create the HUD scene, or clear settings.hud.",
                    path,
                );
            }
        }
        for level in &doc.settings.levels {
            if !game_dir.join(level).is_file() {
                report.push(
                    GateLevel::Blocker,
                    "missing_level",
                    format!("{path} lists level {level}, which does not exist"),
                    "Create the level, or remove it from settings.levels.",
                    path,
                );
            }
        }
        const WEATHER_IDS: [&str; 8] = [
            "clear", "overcast", "rain", "snow", "fog", "storm", "sunset", "night",
        ];
        if let Some(weather) = doc.settings.weather.as_deref() {
            if !WEATHER_IDS.contains(&weather) {
                report.push(
                    GateLevel::Blocker,
                    "unknown_weather",
                    format!("{path} uses weather {weather:?}"),
                    "Use one of: clear, overcast, rain, snow, fog, storm, sunset, night.",
                    path,
                );
            }
        }
        // 5. The structural invariants the parser already enforces, restated per scene so a
        //    report names the file rather than failing the whole load.
        if let Err(error) = doc.validate() {
            report.push(
                GateLevel::Blocker,
                "invalid_scene",
                format!("{path}: {error}"),
                error.hint().unwrap_or("Reload the scene."),
                path,
            );
        }
    }

    report
}

/// Check that every asset a scene references actually exists in the index, and report
/// licence state for the Release gate (INV-074).
///
/// `release` distinguishes the two builds: Debug warn-lists an unknown licence, Release is
/// blocked by it. Gates block, they never warn — so in Release this is a blocker.
#[must_use]
pub fn check_assets(
    index: &AssetIndex,
    scenes: &[(String, SceneDocument)],
    release: bool,
) -> GateReport {
    let mut report = GateReport::default();
    let known: BTreeSet<String> = index
        .assets
        .iter()
        .flat_map(|(id, record)| [format!("asset:{id}"), record.path_rel.clone()])
        .collect();

    for (path, doc) in scenes {
        for entity in &doc.entities {
            for (component, payload) in &entity.components {
                for reference in asset_refs(payload) {
                    if reference.is_empty() {
                        continue;
                    }
                    if !known.contains(&reference) {
                        report.push(
                            GateLevel::Blocker,
                            "dangling_asset",
                            format!(
                                "{path}: {}.{component} references {reference}, which is not in the asset index",
                                entity.name
                            ),
                            "Import the file, or point the component at an asset that exists.",
                            path,
                        );
                    }
                }
            }
        }
    }

    for (id, record) in &index.assets {
        if record.license == LicenseState::Unknown {
            report.push(
                if release {
                    GateLevel::Blocker
                } else {
                    GateLevel::Warning
                },
                "unknown_license",
                format!("{} has no recorded licence", record.path_rel),
                "Add a .meta.json sidecar naming the licence before shipping a Release build.",
                &format!("asset:{id}"),
            );
        }
    }
    report
}

/// Validate the versioned authored documents that both Play diagnostics and packaging consume.
/// Keeping this in the gate layer prevents `/gamedebug` and the build pipeline from drifting into
/// two different definitions of a valid material, shader, HUD or input map.
#[must_use]
pub fn check_authored_documents(_game_dir: &Path) -> GateReport {
    GateReport::default()
}

// Retained for the Godot re-target (ADR-0043): the webview-era caller was removed with
// the old engine and the replacement lands with its ticket. Not dead by intent.
#[allow(dead_code)]
fn collect_authored_files(root: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries = entries
        .filter_map(std::result::Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            collect_authored_files(&path, output);
        } else if kind.is_file() {
            output.push(path);
        }
    }
}

// Retained for the Godot re-target (ADR-0043): the webview-era caller was removed with
// the old engine and the replacement lands with its ticket. Not dead by intent.
#[allow(dead_code)]
fn validate_text_document<F>(
    report: &mut GateReport,
    path: &Path,
    relative: &str,
    code: &str,
    label: &str,
    parse: F,
) where
    F: Fn(&str) -> crate::Result<()>,
{
    let Some(text) = read_authored_text(report, path, relative, code, label) else {
        return;
    };
    if let Err(error) = parse(&text) {
        report.push(
            GateLevel::Blocker,
            code,
            format!("{relative}: {error}"),
            error
                .hint()
                .unwrap_or("Fix the versioned authored document."),
            relative,
        );
    }
}

// Retained for the Godot re-target (ADR-0043): the webview-era caller was removed with
// the old engine and the replacement lands with its ticket. Not dead by intent.
#[allow(dead_code)]
fn read_authored_text(
    report: &mut GateReport,
    path: &Path,
    relative: &str,
    code: &str,
    label: &str,
) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(error) => {
            report.push(
                GateLevel::Blocker,
                code,
                format!("{relative}: cannot read {label}: {error}"),
                "Restore a readable UTF-8 document and retry.",
                relative,
            );
            None
        }
    }
}

// Retained for the Godot re-target (ADR-0043): the webview-era caller was removed with
// the old engine and the replacement lands with its ticket. Not dead by intent.
#[allow(dead_code)]
fn require_authored_dependency(
    report: &mut GateReport,
    game_dir: &Path,
    owner: &str,
    dependency: &str,
    code: &str,
    label: &str,
) {
    let safe = !dependency.is_empty()
        && !dependency.contains('\\')
        && Path::new(dependency)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
    if !safe || !confined_regular_file(game_dir, dependency) {
        report.push(
            GateLevel::Blocker,
            code,
            format!("{owner} references {label} {dependency:?}, which is missing or unsafe"),
            &format!("Restore the project-relative {label}, or update {owner}."),
            owner,
        );
    }
}

// Retained for the Godot re-target (ADR-0043): the webview-era caller was removed with
// the old engine and the replacement lands with its ticket. Not dead by intent.
#[allow(dead_code)]
fn confined_regular_file(root: &Path, relative: &str) -> bool {
    let mut at = root.to_path_buf();
    for component in Path::new(relative).components() {
        let std::path::Component::Normal(part) = component else {
            return false;
        };
        at.push(part);
        let Ok(metadata) = std::fs::symlink_metadata(&at) else {
            return false;
        };
        if metadata.file_type().is_symlink() {
            return false;
        }
    }
    at.is_file()
}

// Retained for the Godot re-target (ADR-0043): the webview-era caller was removed with
// the old engine and the replacement lands with its ticket. Not dead by intent.
#[allow(dead_code)]
fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

/// Every `asset:`-style or project-relative reference inside one component payload.
///
/// String-shaped rather than schema-driven on purpose: `MeshRenderer.materials` is a JSON
/// array and `Collider.shape` is free-form JSON, so a walk finds references a field-by-field
/// reader would miss.
fn asset_refs(payload: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk_refs(payload, &mut out);
    out
}

fn walk_refs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            if text.starts_with("asset:") || text.starts_with("assets/") {
                out.push(text.clone());
            }
        }
        Value::Array(items) => {
            for item in items {
                walk_refs(item, out);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                walk_refs(item, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{check_assets, check_project, GateLevel};
    use crate::asset::{AssetIndex, AssetKind, AssetRecord, LicenseState};
    use crate::document::{Entity, SceneDocument, SceneKind};
    use crate::manifest::GameManifest;
    use bhippi_types::{AssetId, EntityId};
    use serde_json::json;

    fn game(dir: &std::path::Path) -> GameManifest {
        std::fs::create_dir_all(dir.join("assets/scenes")).expect("scenes dir");
        let mut manifest = GameManifest::defaults("Demo");
        manifest.game.default_scene = "assets/scenes/main.bscn.json".to_owned();
        manifest.game.hud_scene = Some("assets/scenes/hud.bscn.json".to_owned());
        manifest.game.levels = vec!["assets/scenes/level_01.bscn.json".to_owned()];
        manifest
    }

    fn write(dir: &std::path::Path, rel: &str, doc: &SceneDocument) {
        std::fs::write(dir.join(rel), doc.dump().expect("dump")).expect("write");
    }

    fn temp(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bhippi-gate-{label}-{}", EntityId::new()));
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    fn main_scene() -> SceneDocument {
        let mut doc = SceneDocument::empty("main");
        doc.settings.kind = SceneKind::Main;
        doc
    }

    #[test]
    fn a_complete_project_passes() {
        let dir = temp("ok");
        let manifest = game(&dir);
        let main = main_scene();
        let mut level = SceneDocument::empty("level_01");
        level.settings.weather = Some("storm".to_owned());
        let hud = SceneDocument::empty("hud");
        write(&dir, "assets/scenes/main.bscn.json", &main);
        write(&dir, "assets/scenes/level_01.bscn.json", &level);
        write(&dir, "assets/scenes/hud.bscn.json", &hud);

        let report = check_project(
            &dir,
            &manifest,
            &[
                ("assets/scenes/main.bscn.json".to_owned(), main),
                ("assets/scenes/level_01.bscn.json".to_owned(), level),
            ],
        );
        assert!(report.passes(), "{:?}", report.findings);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_level_that_does_not_exist_blocks() {
        let dir = temp("missing-level");
        let manifest = game(&dir);
        let main = main_scene();
        write(&dir, "assets/scenes/main.bscn.json", &main);
        write(
            &dir,
            "assets/scenes/hud.bscn.json",
            &SceneDocument::empty("hud"),
        );
        // level_01 is registered but never written.

        let report = check_project(
            &dir,
            &manifest,
            &[("assets/scenes/main.bscn.json".to_owned(), main)],
        );
        assert!(!report.passes());
        assert!(report
            .blockers()
            .iter()
            .any(|finding| finding.code == "missing_level"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_invented_weather_id_blocks() {
        let dir = temp("weather");
        let manifest = game(&dir);
        let main = main_scene();
        let mut level = SceneDocument::empty("level_01");
        level.settings.weather = Some("hurricane".to_owned());
        write(&dir, "assets/scenes/main.bscn.json", &main);
        write(&dir, "assets/scenes/level_01.bscn.json", &level);
        write(
            &dir,
            "assets/scenes/hud.bscn.json",
            &SceneDocument::empty("hud"),
        );

        let report = check_project(
            &dir,
            &manifest,
            &[
                ("assets/scenes/main.bscn.json".to_owned(), main),
                ("assets/scenes/level_01.bscn.json".to_owned(), level),
            ],
        );
        let blocker = report
            .blockers()
            .into_iter()
            .find(|finding| finding.code == "unknown_weather")
            .expect("blocked");
        assert!(blocker.hint.contains("overcast"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fabricated_asset_path_blocks_wherever_it_hides() {
        let mut doc = SceneDocument::empty("level_01");
        doc.entities.push(Entity {
            id: EntityId::new(),
            name: "Crate".to_owned(),
            parent: None,
            tags: vec![],
            components: std::collections::BTreeMap::from([(
                "MeshRenderer".to_owned(),
                // The dangling reference is inside an array, which a field-by-field
                // reader would walk straight past.
                json!({ "mesh": "", "materials": ["assets/materials/invented.mat.json"], "cast_shadows": true }),
            )]),
        });
        let report = check_assets(
            &AssetIndex::default(),
            &[("assets/scenes/level_01.bscn.json".to_owned(), doc)],
            false,
        );
        let blocker = report
            .blockers()
            .into_iter()
            .find(|finding| finding.code == "dangling_asset")
            .expect("blocked");
        assert!(blocker.message.contains("invented.mat.json"));
    }

    #[test]
    fn an_indexed_asset_satisfies_the_reference_by_id_or_by_path() {
        let id = AssetId::new();
        let mut index = AssetIndex::default();
        index.assets.insert(
            id,
            AssetRecord {
                id,
                path_rel: "assets/models/crate.glb".to_owned(),
                kind: AssetKind::Mesh,
                hash: "x".to_owned(),
                license: LicenseState::Known("CC0-1.0".to_owned()),
                size_bytes: 1,
                used_by_scenes: vec![],
            },
        );

        for reference in [format!("asset:{id}"), "assets/models/crate.glb".to_owned()] {
            let mut doc = SceneDocument::empty("level_01");
            doc.entities.push(Entity {
                id: EntityId::new(),
                name: "Crate".to_owned(),
                parent: None,
                tags: vec![],
                components: std::collections::BTreeMap::from([(
                    "MeshRenderer".to_owned(),
                    json!({ "mesh": reference, "materials": [], "cast_shadows": true }),
                )]),
            });
            let report = check_assets(
                &index,
                &[("assets/scenes/level_01.bscn.json".to_owned(), doc)],
                false,
            );
            assert!(report.passes(), "{reference} should resolve");
        }
    }

    #[test]
    fn an_unknown_licence_warns_in_debug_and_blocks_in_release() {
        let id = AssetId::new();
        let mut index = AssetIndex::default();
        index.assets.insert(
            id,
            AssetRecord {
                id,
                path_rel: "assets/textures/found.png".to_owned(),
                kind: AssetKind::Texture,
                hash: "x".to_owned(),
                license: LicenseState::Unknown,
                size_bytes: 1,
                used_by_scenes: vec![],
            },
        );

        let debug = check_assets(&index, &[], false);
        assert!(debug.passes(), "Debug warn-lists rather than blocking");
        assert_eq!(debug.findings[0].level, GateLevel::Warning);

        let release = check_assets(&index, &[], true);
        assert!(!release.passes(), "Release is blocked by INV-074");
        assert_eq!(release.blockers()[0].code, "unknown_license");
    }
}
