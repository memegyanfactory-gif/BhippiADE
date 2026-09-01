use crate::error::{EngineError, Result};
use bhippi_types::{AssetId, SceneId};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// What an imported asset is, used by the drawer's type badges and the schema's
/// `asset:<kind>` field level.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Mesh,
    Skeleton,
    Texture,
    Material,
    Audio,
    Animation,
    Scene,
    Script,
    Prefab,
    Ui,
    Font,
    Shader,
    Other,
}

impl fmt::Display for AssetKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Mesh => "mesh",
            Self::Skeleton => "skeleton",
            Self::Texture => "texture",
            Self::Material => "material",
            Self::Audio => "audio",
            Self::Animation => "animation",
            Self::Scene => "scene",
            Self::Script => "script",
            Self::Prefab => "prefab",
            Self::Ui => "ui",
            Self::Font => "font",
            Self::Shader => "shader",
            Self::Other => "other",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    Glb,
    Gltf,
    Obj,
    Fbx,
    Png,
    Jpeg,
    Tga,
    Exr,
    Hdr,
    Ktx2,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AssetUpAxis {
    X,
    #[default]
    Y,
    Z,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AssetColorSpace {
    Linear,
    #[default]
    Srgb,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetImportSettings {
    pub unit_scale: f32,
    #[serde(default)]
    pub up_axis: AssetUpAxis,
    #[serde(default)]
    pub color_space: AssetColorSpace,
    pub generate_mips: bool,
}

impl Default for AssetImportSettings {
    fn default() -> Self {
        Self {
            unit_scale: 1.0,
            up_axis: AssetUpAxis::Y,
            color_space: AssetColorSpace::Srgb,
            generate_mips: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportDisposition {
    CopyNative,
    NeedsConverter { capability: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetImportReport {
    pub format: ImportFormat,
    pub kind: AssetKind,
    pub unit_scale: f32,
    pub up_axis: AssetUpAxis,
    pub color_space: AssetColorSpace,
    pub disposition: ImportDisposition,
    pub warnings: Vec<String>,
}

/// Deterministic plan for import or reimport. It is evidence and a cache key, not a claim
/// that conversion happened; formats needing a converter say so explicitly.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetImportPlan {
    pub id: AssetId,
    pub source_name: String,
    pub source_hash: String,
    pub cache_key: String,
    pub destination_folder: String,
    pub license: LicenseState,
    pub reimport: bool,
    pub settings: AssetImportSettings,
    pub report: AssetImportReport,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetMovePlan {
    pub id: AssetId,
    pub from: String,
    pub to: String,
    pub sidecar_from: String,
    pub sidecar_to: String,
    pub expected_hash: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetDeletePlan {
    pub id: AssetId,
    pub path: String,
    pub sidecar: String,
    pub expected_hash: String,
}

pub fn plan_import(
    source_name: &str,
    bytes: &[u8],
    settings: AssetImportSettings,
    license: LicenseState,
    existing_id: Option<AssetId>,
) -> Result<AssetImportPlan> {
    if source_name.trim().is_empty() || bytes.is_empty() {
        return Err(EngineError::Asset(
            "import source name and bytes must not be empty".to_owned(),
            Some("Choose a non-empty source file.".to_owned()),
        ));
    }
    if !settings.unit_scale.is_finite() || settings.unit_scale <= 0.0 {
        return Err(EngineError::Asset(
            format!(
                "import unit_scale must be positive, got {}",
                settings.unit_scale
            ),
            Some("Use 1.0 to preserve source units.".to_owned()),
        ));
    }
    let format = import_format(source_name)?;
    let (kind, destination_folder, disposition) = match format {
        ImportFormat::Glb => (
            AssetKind::Mesh,
            "assets/models",
            ImportDisposition::CopyNative,
        ),
        ImportFormat::Gltf => (
            AssetKind::Mesh,
            "assets/models",
            ImportDisposition::NeedsConverter {
                capability: "gltf_bundle_to_glb".to_owned(),
            },
        ),
        ImportFormat::Obj => (
            AssetKind::Mesh,
            "assets/models",
            ImportDisposition::NeedsConverter {
                capability: "obj_to_glb".to_owned(),
            },
        ),
        ImportFormat::Fbx => (
            AssetKind::Mesh,
            "assets/models",
            ImportDisposition::NeedsConverter {
                capability: "fbx_to_glb".to_owned(),
            },
        ),
        ImportFormat::Png
        | ImportFormat::Jpeg
        | ImportFormat::Tga
        | ImportFormat::Exr
        | ImportFormat::Hdr
        | ImportFormat::Ktx2 => (
            AssetKind::Texture,
            "assets/textures",
            ImportDisposition::CopyNative,
        ),
    };
    let source_hash = blake3::hash(bytes).to_hex().to_string();
    let settings_bytes = serde_json::to_vec(&settings).map_err(|error| {
        EngineError::Asset(
            format!("cannot serialise import settings: {error}"),
            Some("Report this as an engine bug.".to_owned()),
        )
    })?;
    let mut cache = blake3::Hasher::new();
    cache.update(b"bhippi-import-plan@1");
    cache.update(source_hash.as_bytes());
    cache.update(&settings_bytes);
    cache.update(format!("{format:?}").as_bytes());
    let warnings = match &disposition {
        ImportDisposition::CopyNative => Vec::new(),
        ImportDisposition::NeedsConverter { capability } => vec![format!(
            "requires unavailable conversion capability {capability}; no output may be claimed"
        )],
    };
    Ok(AssetImportPlan {
        id: existing_id.unwrap_or_default(),
        source_name: source_name.to_owned(),
        source_hash,
        cache_key: cache.finalize().to_hex().to_string(),
        destination_folder: destination_folder.to_owned(),
        license,
        reimport: existing_id.is_some(),
        settings: settings.clone(),
        report: AssetImportReport {
            format,
            kind,
            unit_scale: settings.unit_scale,
            up_axis: settings.up_axis,
            color_space: settings.color_space,
            disposition,
            warnings,
        },
    })
}

fn import_format(source_name: &str) -> Result<ImportFormat> {
    let extension = Path::new(source_name)
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "glb" => Ok(ImportFormat::Glb),
        "gltf" => Ok(ImportFormat::Gltf),
        "obj" => Ok(ImportFormat::Obj),
        "fbx" => Ok(ImportFormat::Fbx),
        "png" => Ok(ImportFormat::Png),
        "jpg" | "jpeg" => Ok(ImportFormat::Jpeg),
        "tga" => Ok(ImportFormat::Tga),
        "exr" => Ok(ImportFormat::Exr),
        "hdr" => Ok(ImportFormat::Hdr),
        "ktx2" => Ok(ImportFormat::Ktx2),
        _ => Err(EngineError::Asset(
            format!("unsupported import format for {source_name:?}"),
            Some("Supported mesh/texture sources: glb, gltf, obj, fbx, png, jpg, tga, exr, hdr, ktx2.".to_owned()),
        )),
    }
}

/// Licence state of an imported asset (plan §11.2). `Unknown` blocks Release builds,
/// Debug builds warn-list (INV-074).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(untagged)]
pub enum LicenseState {
    Unknown,
    Known(String),
}

impl fmt::Display for LicenseState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown => formatter.write_str("unknown"),
            Self::Known(spdx) => formatter.write_str(spdx),
        }
    }
}

/// One record of the asset index (ULID ⇄ relative path ⇄ blake3 hash).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetRecord {
    pub id: AssetId,
    pub path_rel: String,
    pub kind: AssetKind,
    pub hash: String,
    pub license: LicenseState,
    pub size_bytes: u64,
    /// Scenes that reference the asset (from the scene documents). Cross-referencing lets
    /// "find things to edit" become a lookup instead of a search.
    #[serde(default)]
    pub used_by_scenes: Vec<SceneId>,
}

/// The in-memory asset index, persisted to `.bhippi/engine/asset-index.json`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetIndex {
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub assets: BTreeMap<AssetId, AssetRecord>,
}

impl AssetIndex {
    /// Stable ULID→record lookup.
    #[must_use]
    pub fn get(&self, id: AssetId) -> Option<&AssetRecord> {
        self.assets.get(&id)
    }

    #[must_use]
    pub fn by_path(&self, path_rel: &str) -> Option<&AssetRecord> {
        self.assets
            .values()
            .find(|record| record.path_rel == path_rel)
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.assets.len()
    }

    /// Assets not referenced by the loaded scene set. The name is intentionally precise:
    /// material/prefab dependencies must join the index before this can become a global
    /// "unused" delete command.
    #[must_use]
    pub fn unused_by_loaded_scenes(&self) -> Vec<&AssetRecord> {
        self.assets
            .values()
            .filter(|record| record.used_by_scenes.is_empty())
            .collect()
    }

    /// Prepare a recoverable move. Referenced assets are refused until dependency rewrite
    /// exists; silently moving one would orphan authored references.
    pub fn plan_move(&self, id: AssetId, destination: &str) -> Result<AssetMovePlan> {
        let record = self.get(id).ok_or_else(|| {
            EngineError::Asset(
                format!("asset {id} is not in the index"),
                Some("Refresh the Content Browser and retry.".to_owned()),
            )
        })?;
        let destination = safe_asset_path(destination)?;
        if destination == record.path_rel {
            return Err(EngineError::Asset(
                "asset is already at that path".to_owned(),
                Some("Choose a different name or folder.".to_owned()),
            ));
        }
        if !record.used_by_scenes.is_empty() {
            return Err(EngineError::Asset(
                format!(
                    "cannot move {}: dependency rewrite is required for {} loaded scene(s)",
                    record.path_rel,
                    record.used_by_scenes.len()
                ),
                Some("Keep the asset in place until dependency rewrite is available.".to_owned()),
            ));
        }
        if self.by_path(&destination).is_some() {
            return Err(EngineError::Asset(
                format!("an indexed asset already exists at {destination}"),
                Some("Choose a free destination path.".to_owned()),
            ));
        }
        if kind_from_path(Path::new(&destination)) != record.kind {
            return Err(EngineError::Asset(
                format!("moving to {destination:?} would change the asset kind"),
                Some("Keep the original file extension.".to_owned()),
            ));
        }
        Ok(AssetMovePlan {
            id,
            from: record.path_rel.clone(),
            to: destination.clone(),
            sidecar_from: format!("{}.meta.json", record.path_rel),
            sidecar_to: format!("{destination}.meta.json"),
            expected_hash: record.hash.clone(),
        })
    }

    /// Prepare a hash-guarded delete only when the index proves no loaded scene uses it.
    pub fn plan_delete(&self, id: AssetId) -> Result<AssetDeletePlan> {
        let record = self.get(id).ok_or_else(|| {
            EngineError::Asset(
                format!("asset {id} is not in the index"),
                Some("Refresh the Content Browser and retry.".to_owned()),
            )
        })?;
        if !record.used_by_scenes.is_empty() {
            return Err(EngineError::Asset(
                format!(
                    "cannot delete {}: it is used by {} loaded scene(s)",
                    record.path_rel,
                    record.used_by_scenes.len()
                ),
                Some("Remove or replace every dependency first.".to_owned()),
            ));
        }
        Ok(AssetDeletePlan {
            id,
            path: record.path_rel.clone(),
            sidecar: format!("{}.meta.json", record.path_rel),
            expected_hash: record.hash.clone(),
        })
    }

    /// Build a fresh index from a project's `assets/` tree. Files are hashed (blake3);
    /// known sidecar metadata (`.meta.json`) is honoured; unknowns get `license =
    /// unknown` and a kind guessed from the extension. Renames on disk keep their ULID
    /// because the ULID lives in the sidecar, never in the path (plan §9.1).
    pub fn scan(project_root: &Path) -> Result<Self> {
        let assets_root = project_root.join("assets");
        let mut index = Self::default();
        if !assets_root.is_dir() {
            return Ok(index);
        }
        let mut pending: Vec<PathBuf> = vec![assets_root.clone()];
        let mut visited = 0usize;
        while let Some(dir) = pending.pop() {
            let entries = std::fs::read_dir(&dir).map_err(|error| EngineError::Io {
                operation: "scan",
                path: dir.display().to_string(),
                reason: error.to_string(),
                hint: Some("Check the assets/ folder is readable.".to_owned()),
            })?;
            for entry in entries.filter_map(std::result::Result::ok) {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().into_owned();
                if file_name.starts_with('.') {
                    continue; // dotfiles and .git are never assets
                }
                let relative = path
                    .strip_prefix(project_root)
                    .map_err(|_| EngineError::Io {
                        operation: "strip",
                        path: path.display().to_string(),
                        reason: "path escaped project root".to_owned(),
                        hint: None,
                    })?
                    .to_string_lossy()
                    .into_owned()
                    .replace('\\', "/");
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                // Skip imported copies and generated caches.
                if file_name.ends_with(".meta.json") {
                    continue;
                }
                let kind = kind_from_path(&path);
                let hash =
                    blake3::hash(&std::fs::read(&path).map_err(|error| EngineError::Io {
                        operation: "hash",
                        path: path.display().to_string(),
                        reason: error.to_string(),
                        hint: None,
                    })?)
                    .to_hex()
                    .to_string();
                let meta_sidecar = PathBuf::from(format!("{}.meta.json", path.display()));
                let (id, license) = match load_meta(&meta_sidecar) {
                    Ok(Some(meta)) => (meta.id, meta.license),
                    Ok(None) => (AssetId::new(), LicenseState::Unknown),
                    Err(_) => (AssetId::new(), LicenseState::Unknown),
                };
                let size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
                if let Some(record) = index.assets.get_mut(&id) {
                    // Same ULID sidecar re-imported under a new path: keep the first.
                    record.path_rel = relative;
                    record.hash = hash;
                    record.size_bytes = size;
                    record.license = license;
                } else {
                    index.assets.insert(
                        id,
                        AssetRecord {
                            id,
                            path_rel: relative,
                            kind,
                            hash,
                            license,
                            size_bytes: size,
                            used_by_scenes: Vec::new(),
                        },
                    );
                }
                visited += 1;
                if visited > 50_000 {
                    return Err(EngineError::Asset(
                        "asset scan exceeds 50 000 files".to_owned(),
                        Some("Too many files under assets/; split the pack.".to_owned()),
                    ));
                }
            }
        }
        Ok(index)
    }

    /// Recompute `used_by_scenes` from a set of loaded scene documents — called after scene
    /// open/save; keeps the cross-reference honest without a full rescan.
    pub fn refresh_usage(&mut self, scenes: &[&crate::document::SceneDocument]) {
        use std::str::FromStr;
        for record in self.assets.values_mut() {
            record.used_by_scenes.clear();
        }
        // Map asset:xxxx → owning scene. ULIDs not in the index (deleted assets) are
        // dropped silently, gaining a future tombstone.
        let mut by_id: std::collections::HashMap<AssetId, SceneId> =
            std::collections::HashMap::new();
        for scene in scenes {
            for entity in &scene.entities {
                for payload in entity.components.values() {
                    collect_asset_refs(payload, &mut |text: String| {
                        if let Some(id) = text
                            .strip_prefix("asset:")
                            .and_then(|id| AssetId::from_str(id).ok())
                        {
                            by_id.entry(id).or_insert(scene.id);
                        }
                    });
                }
            }
        }
        for (asset_id, scene_id) in by_id {
            if let Some(record) = self.assets.get_mut(&asset_id) {
                if !record.used_by_scenes.contains(&scene_id) {
                    record.used_by_scenes.push(scene_id);
                }
            }
        }
    }
}

fn safe_asset_path(path: &str) -> Result<String> {
    let normalized = path.trim().replace('\\', "/");
    if normalized.is_empty()
        || !normalized.starts_with("assets/")
        || normalized.contains("../")
        || normalized.ends_with('/')
        || Path::new(&normalized).is_absolute()
    {
        return Err(EngineError::Asset(
            format!("asset path {path:?} leaves the assets folder"),
            Some("Choose a file path under assets/.".to_owned()),
        ));
    }
    Ok(normalized)
}

fn collect_asset_refs(value: &serde_json::Value, sink: &mut dyn FnMut(String)) {
    match value {
        serde_json::Value::String(text) => {
            if text.starts_with("asset:") {
                sink(text.clone());
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_asset_refs(item, sink);
            }
        }
        serde_json::Value::Object(map) => {
            for value in map.values() {
                collect_asset_refs(value, sink);
            }
        }
        _ => {}
    }
}

/// The `.meta.json` sidecar written on import (Unity-style, plan §9.1).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AssetMeta {
    pub id: AssetId,
    pub license: LicenseState,
    #[serde(default)]
    pub importer: String,
    #[serde(default)]
    pub imported_at: String,
}

fn load_meta(path: &Path) -> Result<Option<AssetMeta>> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path).map_err(|error| EngineError::Io {
        operation: "read meta",
        path: path.display().to_string(),
        reason: error.to_string(),
        hint: None,
    })?;
    serde_json::from_str(&text).map(Some).map_err(|error| {
        EngineError::Asset(
            format!("corrupt meta sidecar {}: {error}", path.display()),
            Some("Delete the .meta.json to re-import fresh.".to_owned()),
        )
    })
}

/// Guess the asset kind from its extension.
#[must_use]
pub fn kind_from_path(path: &Path) -> AssetKind {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if file_name.ends_with(".mat.json") {
        return AssetKind::Material;
    }
    if file_name.ends_with(".shader.json") {
        return AssetKind::Shader;
    }
    if file_name.ends_with(".bscn.json") {
        return AssetKind::Scene;
    }
    if file_name.ends_with(".prefab.json") {
        return AssetKind::Prefab;
    }
    if file_name.ends_with(".hud.json") {
        return AssetKind::Ui;
    }
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "glb" | "gltf" | "fbx" | "obj" | "mesh" => AssetKind::Mesh,
        "skel" | "skeleton" | "bgskeleton" => AssetKind::Skeleton,
        "png" | "jpg" | "jpeg" | "tga" | "exr" | "hdr" | "ktx2" => AssetKind::Texture,
        "wav" | "ogg" | "mp3" | "flac" => AssetKind::Audio,
        "anim" => AssetKind::Animation,
        "bscn" => AssetKind::Scene,
        "rhai" | "rs" => AssetKind::Script,
        "bprefab" => AssetKind::Prefab,
        "ui" => AssetKind::Ui,
        "ttf" | "otf" => AssetKind::Font,
        "shader" | "wgsl" | "glsl" => AssetKind::Shader,
        "mat" | "bmat" => AssetKind::Material,
        _ => AssetKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        kind_from_path, plan_import, AssetColorSpace, AssetImportSettings, AssetIndex, AssetKind,
        AssetMeta, AssetRecord, AssetUpAxis, ImportDisposition, LicenseState,
    };
    use bhippi_types::{AssetId, SceneId};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    fn temp_root(test_name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bhippi-engine-asset-{test_name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("assets/scenes")).expect("create root");
        dir
    }

    #[test]
    fn scan_indexes_files_and_honours_sidecar_ids() {
        let root = temp_root("scan");
        let id = AssetId::new();
        fs::write(
            root.join("assets/scenes").join("level_01.bscn.json"),
            r#"{}"#,
        )
        .expect("scene file");
        let meta = AssetMeta {
            id,
            license: LicenseState::Known("CC0".to_owned()),
            importer: "test".to_owned(),
            imported_at: "now".to_owned(),
        };
        fs::write(
            root.join("assets/scenes/level_01.bscn.json.meta.json"),
            serde_json::to_string(&meta).expect("meta"),
        )
        .expect("meta file");
        fs::write(root.join("assets").join("crate.glb"), vec![0u8; 32]).expect("glb");

        let index = AssetIndex::scan(&root).expect("scan");
        assert!(index.assets.len() == 2, "expected 2 assets");

        let glb = index
            .assets
            .values()
            .find(|record| record.path_rel == "assets/crate.glb")
            .expect("glb indexed");
        assert_eq!(kind_from_path(&root.join("assets/crate.glb")), glb.kind);
        assert_eq!(glb.license, LicenseState::Unknown);

        let scene = index
            .assets
            .values()
            .find(|record| record.path_rel.ends_with("level_01.bscn.json"))
            .expect("scene indexed");
        assert_eq!(scene.id, id, "sidecar ULID survives the scan");
        assert_eq!(scene.license, LicenseState::Known("CC0".to_owned()));
    }

    #[test]
    fn scan_is_deterministic_across_runs_and_skips_hidden() {
        let root = temp_root("hidden");
        fs::write(root.join("assets/scenes").join(".hidden.glb"), vec![0u8; 4])
            .expect("hidden file");
        let first = AssetIndex::scan(&root).expect("scan 1");
        let second = AssetIndex::scan(&root).expect("scan 2");
        assert_eq!(first.assets, second.assets);
        assert_eq!(first.revision, second.revision);
    }

    #[test]
    fn kind_guessing_covers_the_import_matrix() {
        for (name, expected) in [
            ("x.glb", "mesh"),
            ("x.skel", "skeleton"),
            ("x.png", "texture"),
            ("x.bmat", "material"),
            ("x.ogg", "audio"),
            ("x.bscn", "scene"),
            ("x.rhai", "script"),
            ("x.bprefab", "prefab"),
            ("x.ttf", "font"),
            ("x.mat.json", "material"),
            ("x.shader.json", "shader"),
            ("x.bscn.json", "scene"),
            ("x.prefab.json", "prefab"),
            ("x.hud.json", "ui"),
        ] {
            assert_eq!(
                kind_from_path(std::path::Path::new(name)).to_string(),
                expected
            );
        }
    }

    #[test]
    fn import_and_reimport_plans_are_deterministic_and_preserve_provenance() {
        let settings = AssetImportSettings {
            unit_scale: 0.01,
            up_axis: AssetUpAxis::Z,
            color_space: AssetColorSpace::Linear,
            generate_mips: false,
        };
        let license = LicenseState::Known("CC0-1.0".to_owned());
        let first = plan_import(
            "robot.glb",
            b"stable source",
            settings.clone(),
            license.clone(),
            None,
        )
        .expect("plan import");
        let second = plan_import(
            "robot.glb",
            b"stable source",
            settings.clone(),
            license.clone(),
            Some(first.id),
        )
        .expect("plan reimport");

        assert_eq!(first.source_hash, second.source_hash);
        assert_eq!(first.cache_key, second.cache_key);
        assert_eq!(second.id, first.id);
        assert!(second.reimport);
        assert_eq!(second.license, license);
        assert_eq!(second.settings, settings);
    }

    #[test]
    fn converter_dependent_imports_never_claim_conversion() {
        let plan = plan_import(
            "legacy.fbx",
            b"fbx bytes",
            AssetImportSettings::default(),
            LicenseState::Unknown,
            None,
        )
        .expect("plan");
        assert!(matches!(
            plan.report.disposition,
            ImportDisposition::NeedsConverter { .. }
        ));
        assert!(plan
            .report
            .warnings
            .iter()
            .any(|warning| warning.contains("no output may be claimed")));
        assert_eq!(plan.license, LicenseState::Unknown);
    }

    fn operation_index(referenced: bool) -> (AssetIndex, AssetId) {
        let id = AssetId::new();
        let used_by_scenes = if referenced {
            vec![SceneId::new()]
        } else {
            Vec::new()
        };
        let record = AssetRecord {
            id,
            path_rel: "assets/materials/paint.mat.json".to_owned(),
            kind: AssetKind::Material,
            hash: "expected-hash".to_owned(),
            license: LicenseState::Known("CC0-1.0".to_owned()),
            size_bytes: 42,
            used_by_scenes,
        };
        (
            AssetIndex {
                revision: 1,
                assets: BTreeMap::from([(id, record)]),
            },
            id,
        )
    }

    #[test]
    fn safe_asset_operations_are_hash_guarded_and_dependency_aware() {
        let (index, id) = operation_index(false);
        let movement = index
            .plan_move(id, "assets/materials/painted.mat.json")
            .expect("safe move plan");
        assert_eq!(movement.expected_hash, "expected-hash");
        assert_eq!(
            movement.sidecar_to,
            "assets/materials/painted.mat.json.meta.json"
        );
        assert_eq!(
            index.plan_delete(id).expect("safe delete").expected_hash,
            "expected-hash"
        );
        assert_eq!(index.unused_by_loaded_scenes().len(), 1);

        assert!(index
            .plan_move(id, "assets/../paint.mat.json")
            .expect_err("path escape")
            .hint()
            .is_some());
        assert!(index
            .plan_move(id, "assets/materials/paint.png")
            .expect_err("kind change")
            .hint()
            .is_some());
    }

    #[test]
    fn referenced_assets_refuse_move_and_delete_without_dependency_rewrite() {
        let (index, id) = operation_index(true);
        assert!(index
            .plan_move(id, "assets/materials/moved.mat.json")
            .expect_err("referenced move")
            .to_string()
            .contains("dependency rewrite"));
        assert!(index
            .plan_delete(id)
            .expect_err("referenced delete")
            .to_string()
            .contains("used by"));
        assert!(index.unused_by_loaded_scenes().is_empty());
    }
}
