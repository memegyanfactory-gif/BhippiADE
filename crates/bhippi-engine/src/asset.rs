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
    use super::{kind_from_path, AssetIndex, AssetMeta, LicenseState};
    use bhippi_types::AssetId;
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
        ] {
            assert_eq!(
                kind_from_path(std::path::Path::new(name)).to_string(),
                expected
            );
        }
    }
}
