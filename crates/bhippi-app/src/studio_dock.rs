//! What the Studio's bottom dock shows, computed in Rust (R3).
//!
//! The dock has five tabs and every one of them is a projection of something real:
//!
//! * **Assets** — the files under `assets/` and *only* those. A freshly scaffolded Godot
//!   project has none, and the dock must say so rather than presenting `project.godot`,
//!   `.gitignore` and `main.gd` as a licensed asset library. The rule for "is this an
//!   asset" is the same one the release licence gate applies
//!   ([`bhippi_engine::godot::gates`]): under `assets/`, not a `.meta.json` sidecar, not
//!   one of Godot's `.import` stubs, not hidden, not a script.
//! * **Library** — the engine capability registry (ADR-0035, amended by ADR-0043): the
//!   Godot node classes, the Bhippi presets and the proven export targets. Grouped and
//!   labelled here so the webview only draws the groups it is handed.
//! * **Code** — the project's own `.gd` scripts.
//!
//! Licence and provenance come from the `<file>.meta.json` sidecar and from nowhere else.
//! A file without one is `unknown`, never a cheerful default: an asset that looks licensed
//! because a UI guessed is exactly the one that ships by accident (INV: no unlicensed
//! asset ships).

use crate::commands::AppError;
use crate::godot_commands::resolve_project;
use bhippi_engine::godot::gates::{
    LICENSE_SIDECAR_SUFFIX, LICENSE_UNKNOWN, MAX_SCANNED_FILES, MAX_SCAN_DEPTH,
};
use bhippi_engine::godot::ASSETS_DIR;
use bhippi_engine::registry::{CapabilityKind, CapabilityRegistry};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Godot's own import stubs sit beside the file they describe and are not assets.
const IMPORT_STUB_SUFFIX: &str = ".import";

/// Directories the script walk never descends into: engine caches and version control
/// hold thousands of files nobody edits.
const SKIPPED_DIRECTORIES: [&str; 6] = [
    ".godot",
    ".git",
    ".bhippi",
    "node_modules",
    "target",
    ".import",
];

/// The most scripts the Code tab lists. A project with more than this has a problem the
/// dock is not the place to solve.
const MAX_LISTED_SCRIPTS: usize = 500;

// ── assets ───────────────────────────────────────────────────────────────────────────

/// What a file under `assets/` *is*, decided by extension rather than by which folder
/// somebody dropped it in: a texture in `assets/models/` is still a texture.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ProjectAssetKind {
    Model,
    Texture,
    Audio,
    Scene,
    Material,
    Shader,
    Other,
}

impl ProjectAssetKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Model => "Model",
            Self::Texture => "Texture",
            Self::Audio => "Audio",
            Self::Scene => "Scene",
            Self::Material => "Material",
            Self::Shader => "Shader",
            Self::Other => "Other",
        }
    }
}

/// One row of the Assets tab.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProjectAsset {
    /// Project-relative, forward slashes — always starts `assets/`.
    pub rel: String,
    /// Final path segment.
    pub name: String,
    /// The folder it groups under, e.g. `assets/models`.
    pub folder: String,
    pub kind: ProjectAssetKind,
    /// Display label for [`Self::kind`], so the webview never keeps a second table.
    pub kind_label: String,
    pub size_bytes: u64,
    /// The licence the sidecar states. `None` renders as `unknown`.
    pub licence: Option<String>,
    /// One of `procedural`, `bundled`, `external`, `user` — normalised here. `None` when
    /// there is no readable sidecar, which is *not* the same as "made by the user".
    pub provenance: Option<String>,
}

/// One chip in the Assets tab's source filter, with the count that makes it honest.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetSourceFacet {
    /// `all`, `procedural`, `bundled`, `external`, `user`, `unknown`.
    pub id: String,
    pub label: String,
    pub count: u32,
}

/// The Assets tab's whole state.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProjectAssetsView {
    /// Sorted by path. Empty for a freshly scaffolded project, and that is the truth.
    pub assets: Vec<ProjectAsset>,
    /// The filter chips, in display order, `all` first.
    pub sources: Vec<AssetSourceFacet>,
    /// False when the project has no `assets/` directory at all.
    pub has_assets_dir: bool,
    /// True when the walk stopped at [`MAX_SCANNED_FILES`].
    pub truncated: bool,
    /// How many of the listed assets have no licence recorded. The release gate blocks on
    /// these, so the dock names the number rather than making the user count.
    pub unlicensed_count: u32,
}

/// Everything under `assets/` in the project, with the licence its sidecar states.
#[tauri::command]
#[specta::specta]
pub async fn list_project_assets(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
) -> Result<ProjectAssetsView, AppError> {
    let root = resolve_project(&state, &project).await?;
    tokio::task::spawn_blocking(move || collect_assets(&root))
        .await
        .map_err(|error| AppError {
            message: format!("The asset scan did not finish: {error}"),
            hint: Some("Open the drawer again to retry.".to_owned()),
        })
}

/// The pure half: walk `assets/`, pair every file with its sidecar, classify.
#[must_use]
pub fn collect_assets(root: &Path) -> ProjectAssetsView {
    let assets_dir = root.join(ASSETS_DIR);
    let has_assets_dir = assets_dir.is_dir();

    let mut walked = Walk::default();
    walked.run(&assets_dir, root, 0);

    let mut assets = walked
        .files
        .iter()
        .filter(|(rel, _)| is_asset_file(rel))
        .map(|(rel, size)| {
            let sidecar = walked.meta.get(&format!("{rel}{LICENSE_SIDECAR_SUFFIX}"));
            let kind = asset_kind(rel);
            ProjectAsset {
                name: file_name(rel).to_owned(),
                folder: folder_of(rel),
                kind,
                kind_label: kind.label().to_owned(),
                size_bytes: *size,
                licence: sidecar.and_then(|text| licence_from_meta(text)),
                provenance: sidecar.map(|text| provenance_from_meta(text)),
                rel: rel.clone(),
            }
        })
        .collect::<Vec<_>>();
    assets.sort_by(|left, right| left.rel.cmp(&right.rel));

    let unlicensed_count = u32::try_from(
        assets
            .iter()
            .filter(|asset| asset.licence.is_none())
            .count(),
    )
    .unwrap_or(u32::MAX);

    ProjectAssetsView {
        sources: source_facets(&assets),
        unlicensed_count,
        truncated: walked.truncated,
        has_assets_dir,
        assets,
    }
}

/// The filter chips: `all` always, then each bucket that actually has something in it.
fn source_facets(assets: &[ProjectAsset]) -> Vec<AssetSourceFacet> {
    let buckets: [(&str, &str); 5] = [
        ("procedural", "Procedural"),
        ("bundled", "Bundled CC0"),
        ("external", "External AI"),
        ("user", "User"),
        ("unknown", "Unknown"),
    ];
    let mut facets = vec![AssetSourceFacet {
        id: "all".to_owned(),
        label: "All".to_owned(),
        count: u32::try_from(assets.len()).unwrap_or(u32::MAX),
    }];
    for (id, label) in buckets {
        let count = assets
            .iter()
            .filter(|asset| asset.provenance.as_deref().unwrap_or("unknown") == id)
            .count();
        if count > 0 {
            facets.push(AssetSourceFacet {
                id: id.to_owned(),
                label: label.to_owned(),
                count: u32::try_from(count).unwrap_or(u32::MAX),
            });
        }
    }
    facets
}

/// The release gate's rule, applied to a project-relative path: a sidecar, an import stub,
/// a hidden file or a script is not an asset.
#[must_use]
pub fn is_asset_file(rel: &str) -> bool {
    let name = file_name(rel);
    if name.is_empty()
        || name.starts_with('.')
        || name.ends_with(LICENSE_SIDECAR_SUFFIX)
        || name.ends_with(IMPORT_STUB_SUFFIX)
    {
        return false;
    }
    !matches!(
        extension(name).as_str(),
        "gd" | "gdshaderinc" | "cfg" | "godot" | "toml" | "lock" | "md"
    )
}

/// Extension → kind. Anything unrecognised is `Other`, never a guess.
#[must_use]
pub fn asset_kind(rel: &str) -> ProjectAssetKind {
    match extension(file_name(rel)).as_str() {
        "glb" | "gltf" | "obj" | "fbx" | "dae" | "blend" | "mesh" => ProjectAssetKind::Model,
        "png" | "jpg" | "jpeg" | "webp" | "tga" | "bmp" | "ktx2" | "hdr" | "exr" | "svg" => {
            ProjectAssetKind::Texture
        }
        "wav" | "mp3" | "ogg" | "flac" | "m4a" => ProjectAssetKind::Audio,
        "tscn" | "scn" | "escn" => ProjectAssetKind::Scene,
        "material" | "tres" => ProjectAssetKind::Material,
        "gdshader" | "shader" | "glsl" => ProjectAssetKind::Shader,
        _ => ProjectAssetKind::Other,
    }
}

/// The licence a sidecar states, or `None`.
///
/// `LicenseState` is `#[serde(untagged)]`, so an unknown licence is JSON `null`; a
/// hand-written sidecar may spell it `"unknown"`. Both mean the same thing: not stated.
#[must_use]
pub fn licence_from_meta(text: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let licence = value.get("license")?.as_str()?.trim();
    if licence.is_empty() || licence.eq_ignore_ascii_case(LICENSE_UNKNOWN) {
        return None;
    }
    Some(licence.to_owned())
}

/// The source bucket a sidecar describes. A sidecar that parses but says nothing about
/// provenance is `user`: somebody put the file there by hand.
#[must_use]
pub fn provenance_from_meta(text: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return "unknown".to_owned();
    };
    let raw = match value.get("provenance") {
        Some(serde_json::Value::String(source)) => source.clone(),
        Some(serde_json::Value::Object(map)) => map
            .get("source")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    };
    let lower = raw.to_ascii_lowercase();
    if lower.contains("procedural") {
        "procedural".to_owned()
    } else if lower.contains("bundled") || lower.contains("cc0") {
        "bundled".to_owned()
    } else if lower.contains("external")
        || lower.contains("generated")
        || lower.contains("provider")
    {
        "external".to_owned()
    } else {
        "user".to_owned()
    }
}

// ── scripts ──────────────────────────────────────────────────────────────────────────

/// One `.gd` file, for the Code tab's picker.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProjectScript {
    /// Project-relative, forward slashes.
    pub rel: String,
    pub name: String,
    pub size_bytes: u64,
}

/// Every `.gd` script in the project, sorted by path.
#[tauri::command]
#[specta::specta]
pub async fn list_project_scripts(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
) -> Result<Vec<ProjectScript>, AppError> {
    let root = resolve_project(&state, &project).await?;
    tokio::task::spawn_blocking(move || collect_scripts(&root))
        .await
        .map_err(|error| AppError {
            message: format!("The script scan did not finish: {error}"),
            hint: Some("Open the drawer again to retry.".to_owned()),
        })
}

/// The pure half of [`list_project_scripts`].
#[must_use]
pub fn collect_scripts(root: &Path) -> Vec<ProjectScript> {
    let mut walked = Walk::default();
    walked.run(root, root, 0);
    let mut scripts = walked
        .files
        .into_iter()
        .filter(|(rel, _)| extension(file_name(rel)) == "gd")
        .map(|(rel, size_bytes)| ProjectScript {
            name: file_name(&rel).to_owned(),
            size_bytes,
            rel,
        })
        .collect::<Vec<_>>();
    scripts.sort_by(|left, right| left.rel.cmp(&right.rel));
    scripts.truncate(MAX_LISTED_SCRIPTS);
    scripts
}

// ── capability library ───────────────────────────────────────────────────────────────

/// One card in the Library tab.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CapabilityItem {
    pub id: String,
    pub name: String,
    pub category: String,
    pub purpose: String,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    /// Lowercased search haystack: the webview's filter box matches on this and computes
    /// nothing of its own.
    pub search_text: String,
}

/// The capabilities of one kind, in the order the tab draws them.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CapabilityGroup {
    pub kind: CapabilityKind,
    pub label: String,
    pub items: Vec<CapabilityItem>,
}

/// The Library tab's whole state.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct CapabilityLibrary {
    pub groups: Vec<CapabilityGroup>,
    /// The registry's content hash, so the tab can say which catalogue it is showing.
    pub registry_hash: String,
    pub total: u32,
}

/// The engine capability registry: Godot node classes, Bhippi presets, export targets.
#[tauri::command]
#[specta::specta]
pub async fn list_capabilities() -> Result<CapabilityLibrary, AppError> {
    tokio::task::spawn_blocking(build_capability_library)
        .await
        .map_err(|error| AppError {
            message: format!("The capability registry did not load: {error}"),
            hint: Some("Open the Library tab again to retry.".to_owned()),
        })?
}

/// The pure half of [`list_capabilities`].
pub fn build_capability_library() -> Result<CapabilityLibrary, AppError> {
    let registry = CapabilityRegistry::core().map_err(|error| AppError {
        message: format!("The capability registry could not be built: {error}"),
        hint: Some("This is a Bhippi bug: the built-in catalogue is invalid.".to_owned()),
    })?;

    let mut grouped: BTreeMap<String, (CapabilityKind, Vec<CapabilityItem>)> = BTreeMap::new();
    for entry in &registry.entries {
        let search_text = format!(
            "{} {} {} {}",
            entry.name,
            entry.id,
            entry.category,
            entry.keywords.join(" ")
        )
        .to_ascii_lowercase();
        grouped
            .entry(capability_kind_label(entry.kind).to_owned())
            .or_insert_with(|| (entry.kind, Vec::new()))
            .1
            .push(CapabilityItem {
                id: entry.id.clone(),
                name: entry.name.clone(),
                category: entry.category.clone(),
                purpose: entry.purpose.clone(),
                available: entry.available,
                unavailable_reason: entry.unavailable_reason.clone(),
                search_text,
            });
    }

    let total = u32::try_from(registry.entries.len()).unwrap_or(u32::MAX);
    let groups = grouped
        .into_iter()
        .map(|(label, (kind, mut items))| {
            items.sort_by(|left, right| left.name.cmp(&right.name));
            CapabilityGroup { kind, label, items }
        })
        .collect::<Vec<_>>();

    Ok(CapabilityLibrary {
        groups,
        registry_hash: registry.hash,
        total,
    })
}

#[must_use]
fn capability_kind_label(kind: CapabilityKind) -> &'static str {
    match kind {
        CapabilityKind::GodotNode => "Godot nodes",
        CapabilityKind::Preset => "Presets",
        CapabilityKind::BuildTarget => "Build targets",
        CapabilityKind::Extension => "Extensions",
    }
}

// ── the walk ─────────────────────────────────────────────────────────────────────────

/// Depth- and count-limited walk collecting project-relative files and sidecar text.
#[derive(Default)]
struct Walk {
    files: Vec<(String, u64)>,
    meta: BTreeMap<String, String>,
    truncated: bool,
}

impl Walk {
    fn run(&mut self, directory: &Path, root: &Path, depth: usize) {
        if depth > MAX_SCAN_DEPTH {
            return;
        }
        if self.files.len() >= MAX_SCANNED_FILES {
            self.truncated = true;
            return;
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        let mut paths = entries
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<PathBuf>>();
        paths.sort();
        for path in paths {
            if self.files.len() >= MAX_SCANNED_FILES {
                self.truncated = true;
                return;
            }
            let name = file_name_of(&path);
            if path.is_dir() {
                if !SKIPPED_DIRECTORIES.contains(&name.as_str()) {
                    self.run(&path, root, depth + 1);
                }
                continue;
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let rel = relative.to_string_lossy().replace('\\', "/");
            if name.ends_with(LICENSE_SIDECAR_SUFFIX) {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    self.meta.insert(rel, text);
                }
                continue;
            }
            let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            self.files.push((rel, size));
        }
    }
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn file_name(rel: &str) -> &str {
    match rel.rfind('/') {
        Some(cut) => &rel[cut + 1..],
        None => rel,
    }
}

fn folder_of(rel: &str) -> String {
    match rel.rfind('/') {
        Some(cut) if cut > 0 => rel[..cut].to_owned(),
        _ => ASSETS_DIR.to_owned(),
    }
}

fn extension(name: &str) -> String {
    match name.rfind('.') {
        Some(dot) if dot > 0 => name[dot + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        asset_kind, build_capability_library, collect_assets, collect_scripts, is_asset_file,
        licence_from_meta, provenance_from_meta, ProjectAssetKind,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A throwaway project root. `tempfile` is not a dependency of this crate, so the
    /// fixtures follow the convention the rest of the crate uses.
    fn fixture_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("bhippi-dock-{name}-{}", ulid::Ulid::new()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture root");
        root
    }

    fn write(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, body).expect("write fixture");
    }

    /// A freshly scaffolded Godot project has no `assets/` at all — and therefore no
    /// assets. This is the screenshot the owner sent: eight files, all "CC0".
    #[test]
    fn scaffold_files_are_not_assets() {
        let root = fixture_root("scaffold");
        let root = root.as_path();
        write(root, ".gitignore", "target/\n");
        write(root, "project.godot", "config_version=5\n");
        write(root, "Bhippi.game.toml", "[game]\n");
        write(root, "export_presets.cfg", "[preset.0]\n");
        write(root, "scripts/main.gd", "extends Node\n");
        write(root, "scenes/main.tscn", "[gd_scene]\n");

        let view = collect_assets(root);
        assert!(!view.has_assets_dir, "there is no assets/ directory");
        assert!(view.assets.is_empty(), "scaffold files are not assets");
        assert_eq!(view.unlicensed_count, 0);
        // Only the "All" chip, and it counts zero.
        assert_eq!(view.sources.len(), 1);
        assert_eq!(view.sources[0].id, "all");
        assert_eq!(view.sources[0].count, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn only_files_under_assets_are_listed_and_licence_needs_a_sidecar() {
        let root = fixture_root("assets");
        let root = root.as_path();
        // Not assets.
        write(root, "project.godot", "config_version=5\n");
        write(root, "scripts/player.gd", "extends Node\n");
        write(root, "scenes/main.tscn", "[gd_scene]\n");
        // Assets.
        write(root, "assets/models/hero.glb", "glb");
        write(root, "assets/textures/grass.png", "png");
        write(root, "assets/audio/coin.wav", "wav");
        // Non-assets that live under assets/.
        write(root, "assets/.keep", "");
        write(root, "assets/tools/build.gd", "extends Node\n");
        write(root, "assets/textures/grass.png.import", "[remap]\n");
        // One sidecar only.
        write(
            root,
            "assets/models/hero.glb.meta.json",
            r#"{"license":"CC0-1.0","provenance":{"source":"bundled_cc0"}}"#,
        );

        let view = collect_assets(root);
        assert!(view.has_assets_dir);
        let listed = view
            .assets
            .iter()
            .map(|a| a.rel.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            listed,
            vec![
                "assets/audio/coin.wav",
                "assets/models/hero.glb",
                "assets/textures/grass.png",
            ],
            "sidecars, import stubs, dotfiles and scripts are not assets"
        );

        let hero = &view.assets[1];
        assert_eq!(hero.licence.as_deref(), Some("CC0-1.0"));
        assert_eq!(hero.provenance.as_deref(), Some("bundled"));
        assert_eq!(hero.kind, ProjectAssetKind::Model);
        assert_eq!(hero.folder, "assets/models");

        // No sidecar means unknown — never a default licence.
        assert_eq!(view.assets[0].licence, None);
        assert_eq!(view.assets[0].provenance, None);
        assert_eq!(view.assets[2].licence, None);
        assert_eq!(view.unlicensed_count, 2);

        let chips = view
            .sources
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(chips, vec!["all", "bundled", "unknown"]);
        assert_eq!(view.sources[0].count, 3);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sidecar_saying_unknown_is_not_a_licence() {
        assert_eq!(licence_from_meta(r#"{"license":"unknown"}"#), None);
        assert_eq!(licence_from_meta(r#"{"license":null}"#), None);
        assert_eq!(licence_from_meta(r#"{"license":"  "}"#), None);
        assert_eq!(licence_from_meta("not json"), None);
        assert_eq!(
            licence_from_meta(r#"{"license":"MIT"}"#).as_deref(),
            Some("MIT")
        );
    }

    #[test]
    fn provenance_buckets() {
        assert_eq!(
            provenance_from_meta(r#"{"provenance":"procedural_mesh"}"#),
            "procedural"
        );
        assert_eq!(
            provenance_from_meta(r#"{"provenance":{"source":"bundled_cc0"}}"#),
            "bundled"
        );
        assert_eq!(
            provenance_from_meta(r#"{"provenance":{"source":"provider_generated"}}"#),
            "external"
        );
        // A sidecar with no provenance still means somebody put the file there.
        assert_eq!(provenance_from_meta(r#"{"license":"MIT"}"#), "user");
        assert_eq!(provenance_from_meta("not json"), "unknown");
    }

    #[test]
    fn kinds_come_from_the_extension_not_the_folder() {
        assert_eq!(
            asset_kind("assets/models/grass.png"),
            ProjectAssetKind::Texture
        );
        assert_eq!(
            asset_kind("assets/textures/hero.glb"),
            ProjectAssetKind::Model
        );
        assert_eq!(asset_kind("assets/sfx/coin.ogg"), ProjectAssetKind::Audio);
        assert_eq!(
            asset_kind("assets/rooms/hall.tscn"),
            ProjectAssetKind::Scene
        );
        assert_eq!(
            asset_kind("assets/mat/rock.tres"),
            ProjectAssetKind::Material
        );
        assert_eq!(
            asset_kind("assets/fx/water.gdshader"),
            ProjectAssetKind::Shader
        );
        assert_eq!(asset_kind("assets/notes.txt"), ProjectAssetKind::Other);
    }

    #[test]
    fn config_and_dotfiles_are_never_assets() {
        assert!(!is_asset_file("assets/.gitkeep"));
        assert!(!is_asset_file("assets/thing.png.meta.json"));
        assert!(!is_asset_file("assets/thing.png.import"));
        assert!(!is_asset_file("assets/tool.gd"));
        assert!(!is_asset_file("assets/notes.md"));
        assert!(is_asset_file("assets/thing.png"));
    }

    #[test]
    fn scripts_are_the_projects_gd_files_only() {
        let root = fixture_root("scripts");
        let root = root.as_path();
        write(root, "scripts/player.gd", "extends Node\n");
        write(root, "scripts/enemy.gd", "extends Node\n");
        write(root, "scenes/main.tscn", "[gd_scene]\n");
        write(root, ".godot/cache/junk.gd", "extends Node\n");

        let scripts = collect_scripts(root);
        let rels = scripts.iter().map(|s| s.rel.as_str()).collect::<Vec<_>>();
        assert_eq!(rels, vec!["scripts/enemy.gd", "scripts/player.gd"]);
        assert!(scripts[0].size_bytes > 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn capability_library_groups_the_real_registry() {
        let library = build_capability_library().expect("core registry builds");
        assert!(library.total > 0, "the registry is not empty");
        assert!(!library.registry_hash.is_empty());
        assert!(!library.groups.is_empty());
        let counted: usize = library.groups.iter().map(|group| group.items.len()).sum();
        assert_eq!(
            counted, library.total as usize,
            "every entry lands in a group"
        );
        for group in &library.groups {
            assert!(!group.label.is_empty());
            for item in &group.items {
                assert!(!item.id.is_empty());
                assert_eq!(item.search_text, item.search_text.to_ascii_lowercase());
            }
        }
    }
}
