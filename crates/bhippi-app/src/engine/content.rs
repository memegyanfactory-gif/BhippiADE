//! Content authoring on the transaction path (ENG-122, ENG-123).
//!
//! Creating a material is a *write*, so it obeys the same rule every other write does: it
//! rides in a transaction, it is journaled, and it is undoable (INV-070/071). The reason
//! this matters is the common case — "make a wet concrete material and put it on the floor"
//! is one change to a user, so creating the file and assigning it must be one Ctrl+Z, not
//! two unrelated events one of which cannot be reversed.
//!
//! Each content action produces a [`FileChange`] carrying the bytes it wrote *and the bytes
//! that were there before*, which is what makes the inverse exact rather than approximate.

use crate::commands::AppError;
use bhippi_engine::asset::{AssetMeta, LicenseState};
use bhippi_engine::document::SceneDocument;
use bhippi_engine::material::{MaterialDocument, ShaderDocument, ShaderStage};
use bhippi_engine::prefab::PrefabDocument;
use bhippi_engine::transaction::Op;
use bhippi_types::EntityId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// One file this transaction wrote, with enough to put it back.
///
/// `prior` is `None` when the file did not exist, which is how undo knows to delete rather
/// than restore. The sidecar rides along in the same change so a rollback never leaves an
/// orphaned `.meta.json` pointing at a file that is gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub rel_path: String,
    pub wrote: String,
    pub prior: Option<String>,
    pub sidecar: Option<(String, Option<String>, String)>,
}

impl FileChange {
    /// Write the file (and its sidecar) into the game folder.
    pub fn apply(&self, game_dir: &Path) -> Result<(), AppError> {
        write_file(game_dir, &self.rel_path, &self.wrote)?;
        if let Some((path, _, contents)) = &self.sidecar {
            write_file(game_dir, path, contents)?;
        }
        Ok(())
    }

    /// Put the file back the way it was: restore prior bytes, or delete what we created.
    pub fn revert(&self, game_dir: &Path) -> Result<(), AppError> {
        if let Some((path, prior, _)) = &self.sidecar {
            match prior {
                Some(bytes) => write_file(game_dir, path, bytes)?,
                None => remove_file(game_dir, path),
            }
        }
        match &self.prior {
            Some(bytes) => write_file(game_dir, &self.rel_path, bytes),
            None => {
                remove_file(game_dir, &self.rel_path);
                Ok(())
            }
        }
    }
}

fn write_file(game_dir: &Path, rel: &str, contents: &str) -> Result<(), AppError> {
    let path = game_dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| AppError {
            message: format!("Could not create {}: {error}", parent.display()),
            hint: Some("Check the project is writable.".to_owned()),
        })?;
    }
    std::fs::write(&path, contents).map_err(|error| AppError {
        message: format!("Could not write {rel}: {error}"),
        hint: Some("Check the file is writable.".to_owned()),
    })
}

/// Deleting is best-effort on purpose: a rollback that fails because someone already removed
/// the file has still achieved what it wanted.
fn remove_file(game_dir: &Path, rel: &str) {
    let _ignored = std::fs::remove_file(game_dir.join(rel));
}

fn read_prior(game_dir: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(game_dir.join(rel)).ok()
}

/// A project-relative path that cannot escape the game folder.
fn safe_rel(rel: &str) -> Result<String, AppError> {
    let normalised = rel.trim().replace('\\', "/");
    if normalised.is_empty() {
        return Err(AppError::plain("No asset path was given."));
    }
    if normalised.contains("..") || Path::new(&normalised).is_absolute() {
        return Err(AppError::plain("That asset path leaves the game folder."));
    }
    Ok(normalised)
}

/// The content half of the action vocabulary: actions that write asset *files* rather than
/// scene entities. They travel in the same batch as scene actions and commit together.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ContentAction {
    /// Write `assets/materials/<name>.mat.json`.
    CreateMaterial {
        name: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        shader: Option<String>,
        /// Map slot → texture reference.
        #[serde(default)]
        maps: std::collections::BTreeMap<String, Option<String>>,
        /// Partial parameter overrides, merged onto the defaults.
        #[serde(default)]
        params: Option<Value>,
    },
    /// Write `assets/shaders/<name>.shader.json`.
    CreateShader {
        name: String,
        source: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        stage: Option<ShaderStage>,
    },
    /// Write `assets/scripts/<name>.rhai`, **after** it compiles (ADR-0030).
    ///
    /// The compiler runs before the file lands, so a script the AI wrote wrong comes back as
    /// a located fault in the same turn rather than as a file that fails at Play.
    CreateScript {
        name: String,
        source: String,
        #[serde(default)]
        path: Option<String>,
        /// Attach the script to this entity as well, in the same transaction.
        #[serde(default)]
        entity: Option<EntityId>,
    },
    /// Capture an entity subtree into `assets/prefabs/<name>.prefab.json`.
    CreatePrefab {
        name: String,
        entity: EntityId,
        #[serde(default)]
        path: Option<String>,
    },
    /// Record an asset's licence in its `.meta.json` sidecar — the difference between a
    /// Release build that ships and one that is blocked (INV-074).
    SetAssetLicense { path: String, license: String },
}

/// What a content action produced: the file it wrote, and any scene ops it implies.
#[derive(Debug)]
pub struct ContentOutcome {
    pub file: FileChange,
    pub ops: Vec<Op>,
    pub label: String,
    /// The reference a following action can use to point at what was just created.
    pub asset_ref: String,
}

impl ContentAction {
    /// The one-line label for the Activity Dock and the undo entry.
    #[must_use]
    pub fn to_label(&self) -> String {
        match self {
            Self::CreateMaterial { name, .. } => format!("create material {name}"),
            Self::CreateShader { name, .. } => format!("create shader {name}"),
            Self::CreateScript { name, .. } => format!("create script {name}"),
            Self::CreatePrefab { name, .. } => format!("create prefab {name}"),
            Self::SetAssetLicense { path, .. } => format!("license {path}"),
        }
    }

    /// Build the file change (and any ops) without writing anything yet.
    pub fn prepare(
        &self,
        game_dir: &Path,
        scene: &SceneDocument,
    ) -> Result<ContentOutcome, AppError> {
        match self {
            Self::CreateMaterial {
                name,
                path,
                shader,
                maps,
                params,
            } => {
                let rel = safe_rel(
                    path.as_deref()
                        .unwrap_or(&format!("assets/materials/{}.mat.json", slug(name))),
                )?;
                let mut material = MaterialDocument::new(name.clone());
                material.shader.clone_from(shader);
                material.maps.clone_from(maps);
                if let Some(overrides) = params {
                    // Merge onto the defaults so a caller naming only `roughness` still gets
                    // a complete, valid document.
                    let mut base = serde_json::to_value(&material.params).map_err(json_error)?;
                    merge(&mut base, overrides);
                    material.params = serde_json::from_value(base).map_err(|error| AppError {
                        message: format!("Those material parameters are not valid: {error}"),
                        hint: Some(
                            "Fields: base_color, roughness, metallic, emissive, emissive_strength, \
                             normal_strength, tiling, offset, alpha_mode, alpha_cutoff, double_sided."
                                .to_owned(),
                        ),
                    })?;
                }
                material.validate().map_err(engine_error)?;
                let asset_ref = rel.clone();
                Ok(ContentOutcome {
                    file: file_with_sidecar(
                        game_dir,
                        &rel,
                        material.dump().map_err(engine_error)?,
                        material.id,
                    ),
                    ops: Vec::new(),
                    label: self.to_label(),
                    asset_ref,
                })
            }
            Self::CreateShader {
                name,
                source,
                path,
                stage,
            } => {
                let rel = safe_rel(
                    path.as_deref()
                        .unwrap_or(&format!("assets/shaders/{}.shader.json", slug(name))),
                )?;
                let mut shader = ShaderDocument::new(name.clone(), source.clone());
                if let Some(stage) = stage {
                    shader.stage = *stage;
                }
                shader.validate().map_err(engine_error)?;
                let asset_ref = rel.clone();
                Ok(ContentOutcome {
                    file: file_with_sidecar(
                        game_dir,
                        &rel,
                        shader.dump().map_err(engine_error)?,
                        shader.id,
                    ),
                    ops: Vec::new(),
                    label: self.to_label(),
                    asset_ref,
                })
            }
            Self::CreateScript {
                name,
                source,
                path,
                entity,
            } => {
                let rel = safe_rel(
                    path.as_deref()
                        .unwrap_or(&format!("assets/scripts/{}.rhai", slug(name))),
                )?;
                // Compile first. Writing a script that cannot run and reporting success is
                // exactly the "fake breadth" this phase exists to remove.
                let program =
                    bhippi_engine::script::compile(&rel, source).map_err(|fault| AppError {
                        message: format!(
                            "{}:{}:{} {}",
                            fault.file, fault.line, fault.column, fault.message
                        ),
                        hint: fault.hint.clone(),
                    })?;
                let mut ops = Vec::new();
                if let Some(target) = entity {
                    let mut payload = serde_json::Map::new();
                    payload.insert("script".to_owned(), Value::String(rel.clone()));
                    payload.insert(
                        "hooks".to_owned(),
                        Value::Array(
                            program
                                .hook_names()
                                .into_iter()
                                .map(|hook| Value::String(hook.to_owned()))
                                .collect(),
                        ),
                    );
                    ops.push(Op::AddComponent {
                        entity: *target,
                        component: "ScriptRef".to_owned(),
                        value: Value::Object(payload),
                    });
                }
                let asset_ref = rel.clone();
                Ok(ContentOutcome {
                    file: FileChange {
                        rel_path: rel.clone(),
                        wrote: source.clone(),
                        prior: read_prior(game_dir, &rel),
                        sidecar: None,
                    },
                    ops,
                    label: self.to_label(),
                    asset_ref,
                })
            }
            Self::CreatePrefab { name, entity, path } => {
                let rel = safe_rel(
                    path.as_deref()
                        .unwrap_or(&format!("assets/prefabs/{}.prefab.json", slug(name))),
                )?;
                let prefab =
                    PrefabDocument::from_subtree(scene, *entity, name).map_err(engine_error)?;
                let asset_ref = rel.clone();
                Ok(ContentOutcome {
                    file: file_with_sidecar(
                        game_dir,
                        &rel,
                        prefab.dump().map_err(engine_error)?,
                        prefab.id,
                    ),
                    ops: Vec::new(),
                    label: self.to_label(),
                    asset_ref,
                })
            }
            Self::SetAssetLicense { path, license } => {
                let rel = safe_rel(path)?;
                if !game_dir.join(&rel).is_file() {
                    return Err(AppError {
                        message: format!("{rel} does not exist."),
                        hint: Some("Import the asset before recording its licence.".to_owned()),
                    });
                }
                let sidecar_rel = format!("{rel}.meta.json");
                let prior = read_prior(game_dir, &sidecar_rel);
                // Keep the existing id if there is one: an asset's ULID is its identity
                // across renames, and minting a new one would orphan every reference.
                let id = prior
                    .as_deref()
                    .and_then(|text| serde_json::from_str::<AssetMeta>(text).ok())
                    .map(|meta| meta.id)
                    // `AssetId::default()` mints a fresh ULID (see define_id!), so this is a new id,
                    // not the nil one.
                    .unwrap_or_default();
                let meta = AssetMeta {
                    id,
                    license: if license.trim().is_empty() {
                        LicenseState::Unknown
                    } else {
                        LicenseState::Known(license.trim().to_owned())
                    },
                    importer: "bhippi".to_owned(),
                    imported_at: chrono::Utc::now().to_rfc3339(),
                };
                let contents = serde_json::to_string_pretty(&meta).map_err(json_error)?;
                Ok(ContentOutcome {
                    file: FileChange {
                        rel_path: sidecar_rel,
                        wrote: contents,
                        prior,
                        sidecar: None,
                    },
                    ops: Vec::new(),
                    label: self.to_label(),
                    asset_ref: rel,
                })
            }
        }
    }
}

/// A generated asset knows its own provenance, so it is never `license = unknown` and never
/// blocks a Release build for a reason nobody can act on.
fn file_with_sidecar(
    game_dir: &Path,
    rel: &str,
    contents: String,
    id: bhippi_types::AssetId,
) -> FileChange {
    let sidecar_rel = format!("{rel}.meta.json");
    let meta = AssetMeta {
        id,
        license: LicenseState::Known("generated".to_owned()),
        importer: "bhippi-engine".to_owned(),
        imported_at: chrono::Utc::now().to_rfc3339(),
    };
    FileChange {
        rel_path: rel.to_owned(),
        wrote: contents,
        prior: read_prior(game_dir, rel),
        sidecar: Some((
            sidecar_rel.clone(),
            read_prior(game_dir, &sidecar_rel),
            serde_json::to_string_pretty(&meta).unwrap_or_default(),
        )),
    }
}

fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(base), Value::Object(patch)) => {
            for (key, value) in patch {
                merge(base.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

fn slug(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_').to_owned();
    if trimmed.is_empty() {
        "asset".to_owned()
    } else {
        trimmed
    }
}

fn engine_error(error: bhippi_engine::EngineError) -> AppError {
    AppError {
        message: error.to_string(),
        hint: error.hint().map(str::to_owned),
    }
}

fn json_error(error: serde_json::Error) -> AppError {
    AppError {
        message: format!("Could not serialise the asset: {error}"),
        hint: Some("Report this as an engine bug.".to_owned()),
    }
}

/// Import a file from anywhere on disk into the project, writing a sidecar so the asset has
/// an id and a recorded licence from the moment it lands (ENG-123).
///
/// The mesh **conversion** half of ENG-124 (OBJ/FBX → GLB, unit and axis correction) is not
/// here: it needs `tobj`/`ufbx` and a normalisation pass, and copying a file while claiming
/// to have converted it would be worse than not offering it. A `.obj` imported today is
/// indexed and referenced correctly; the renderer's ability to draw it is Phase 5.
pub fn import_file(
    game_dir: &Path,
    source: &Path,
    dest_rel: &str,
    license: Option<&str>,
) -> Result<FileChange, AppError> {
    let rel = safe_rel(dest_rel)?;
    let bytes = std::fs::read(source).map_err(|error| AppError {
        message: format!("Could not read {}: {error}", source.display()),
        hint: Some("Check the file still exists.".to_owned()),
    })?;
    // Binary assets are copied directly; only the sidecar rides in the FileChange, because
    // `FileChange` carries text and a `.glb` is not text.
    let dest = game_dir.join(&rel);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|error| AppError {
            message: format!("Could not create {}: {error}", parent.display()),
            hint: Some("Check the project is writable.".to_owned()),
        })?;
    }
    std::fs::write(&dest, &bytes).map_err(|error| AppError {
        message: format!("Could not write {rel}: {error}"),
        hint: Some("Check the destination is writable.".to_owned()),
    })?;

    let sidecar_rel = format!("{rel}.meta.json");
    let meta = AssetMeta {
        id: bhippi_types::AssetId::new(),
        license: match license.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => LicenseState::Known(value.to_owned()),
            // An import with no stated licence is honestly unknown, and INV-074 will stop a
            // Release build until someone says what it is. That is the gate working.
            None => LicenseState::Unknown,
        },
        importer: "bhippi-import".to_owned(),
        imported_at: chrono::Utc::now().to_rfc3339(),
    };
    let contents = serde_json::to_string_pretty(&meta).map_err(json_error)?;
    write_file(game_dir, &sidecar_rel, &contents)?;
    Ok(FileChange {
        rel_path: sidecar_rel,
        wrote: contents,
        prior: None,
        sidecar: None,
    })
}

/// The two folders that hold what an import produces, by extension.
#[must_use]
pub fn import_folder(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "tga" | "exr" | "hdr" | "ktx2" => "assets/textures",
        "wav" | "ogg" | "mp3" | "flac" => "assets/audio",
        "ttf" | "otf" => "assets/fonts",
        _ => "assets/models",
    }
}

/// Try to read a content action out of a raw batch payload. `None` means it is a scene
/// action and should be lowered the usual way.
#[must_use]
pub fn parse_content_action(raw: &Value) -> Option<ContentAction> {
    let kind = raw.get("kind").and_then(Value::as_str)?;
    if !matches!(
        kind,
        "create_material"
            | "create_shader"
            | "create_script"
            | "create_prefab"
            | "set_asset_license"
    ) {
        return None;
    }
    serde_json::from_value(raw.clone()).ok()
}

/// The same list, for the error message when a payload names one of these but is malformed.
pub const CONTENT_KINDS: [&str; 5] = [
    "create_material",
    "create_shader",
    "create_script",
    "create_prefab",
    "set_asset_license",
];

#[cfg(test)]
mod tests {
    use super::{import_folder, parse_content_action, slug, ContentAction, FileChange};
    use bhippi_engine::document::SceneDocument;
    use bhippi_engine::material::MaterialDocument;
    use bhippi_types::EntityId;
    use serde_json::json;

    fn temp(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("bhippi-content-{label}-{}", EntityId::new()));
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    #[test]
    fn creating_a_material_writes_a_valid_document_and_a_sidecar() {
        let dir = temp("material");
        let action = ContentAction::CreateMaterial {
            name: "Wet Concrete".to_owned(),
            path: None,
            shader: None,
            maps: std::collections::BTreeMap::from([(
                "albedo".to_owned(),
                Some("assets/textures/concrete.png".to_owned()),
            )]),
            params: Some(json!({ "roughness": 0.2, "metallic": 0.0 })),
        };
        let outcome = action
            .prepare(&dir, &SceneDocument::empty("level_01"))
            .expect("prepares");
        assert_eq!(
            outcome.file.rel_path,
            "assets/materials/wet_concrete.mat.json"
        );
        outcome.file.apply(&dir).expect("writes");

        let text = std::fs::read_to_string(dir.join(&outcome.file.rel_path)).expect("read");
        let material = MaterialDocument::parse(&text).expect("the written document validates");
        assert_eq!(material.params.roughness, 0.2);
        // Unnamed parameters keep their defaults rather than becoming null.
        assert_eq!(material.params.base_color, [0.8, 0.8, 0.8]);
        assert_eq!(material.name, "Wet Concrete");

        // The sidecar means a generated asset never blocks a Release build for an unknown
        // licence — it knows where it came from.
        let sidecar =
            std::fs::read_to_string(dir.join(format!("{}.meta.json", outcome.file.rel_path)))
                .expect("sidecar");
        assert!(sidecar.contains("generated"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_script_that_compiles_is_written_and_attached_in_the_same_transaction() {
        let dir = temp("script-ok");
        let mut scene = SceneDocument::empty("level_01");
        let entity = EntityId::new();
        scene.entities.push(bhippi_engine::document::Entity {
            id: entity,
            name: "Door".to_owned(),
            parent: None,
            tags: Vec::new(),
            components: Default::default(),
        });
        let action = ContentAction::CreateScript {
            name: "Sliding Door".to_owned(),
            source: "fn on_update(dt) { translate(self_id(), 0.0, dt, 0.0); }
"
            .to_owned(),
            path: None,
            entity: Some(entity),
        };

        let outcome = action.prepare(&dir, &scene).expect("prepares");
        assert_eq!(outcome.file.rel_path, "assets/scripts/sliding_door.rhai");
        assert_eq!(outcome.ops.len(), 1, "the attach travels in the same batch");
        outcome.file.apply(&dir).expect("writes");
        let text = std::fs::read_to_string(dir.join(&outcome.file.rel_path)).expect("read");
        assert!(text.contains("on_update"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_script_that_does_not_compile_is_refused_with_its_line() {
        let dir = temp("script-bad");
        let action = ContentAction::CreateScript {
            name: "Broken".to_owned(),
            // `for` is outside the subset (ADR-0030), on line 2.
            source: "fn on_start() {
  for i in 0..3 { log(\"x\"); }
}
"
            .to_owned(),
            path: None,
            entity: None,
        };

        let error = action
            .prepare(&dir, &SceneDocument::empty("level_01"))
            .expect_err("a script that cannot run must not be written");
        assert!(error.message.contains(":2:"), "{}", error.message);
        assert!(error.message.contains("for"));
        assert!(
            !dir.join("assets/scripts/broken.rhai").exists(),
            "nothing may be written when the compile fails"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_script_is_recognised_as_a_content_action() {
        let parsed = parse_content_action(&json!({
            "kind": "create_script",
            "name": "Coin",
            "source": "fn on_start() { log(\"hi\"); }"
        }));
        assert!(matches!(parsed, Some(ContentAction::CreateScript { .. })));
    }

    #[test]
    fn reverting_removes_both_the_asset_and_its_sidecar() {
        let dir = temp("revert");
        let action = ContentAction::CreateMaterial {
            name: "Rust".to_owned(),
            path: None,
            shader: None,
            maps: Default::default(),
            params: None,
        };
        let outcome = action
            .prepare(&dir, &SceneDocument::empty("level_01"))
            .expect("prepares");
        outcome.file.apply(&dir).expect("writes");
        assert!(dir.join(&outcome.file.rel_path).is_file());

        outcome.file.revert(&dir).expect("reverts");
        assert!(!dir.join(&outcome.file.rel_path).is_file(), "asset removed");
        assert!(
            !dir.join(format!("{}.meta.json", outcome.file.rel_path))
                .is_file(),
            "no orphaned sidecar left behind"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwriting_an_existing_material_restores_the_old_bytes_on_revert() {
        let dir = temp("overwrite");
        std::fs::create_dir_all(dir.join("assets/materials")).expect("dir");
        let original = MaterialDocument::new("Original").dump().expect("dump");
        std::fs::write(dir.join("assets/materials/wood.mat.json"), &original).expect("seed");

        let action = ContentAction::CreateMaterial {
            name: "Wood".to_owned(),
            path: Some("assets/materials/wood.mat.json".to_owned()),
            shader: None,
            maps: Default::default(),
            params: Some(json!({ "roughness": 0.9 })),
        };
        let outcome = action
            .prepare(&dir, &SceneDocument::empty("level_01"))
            .expect("prepares");
        outcome.file.apply(&dir).expect("writes");
        let after =
            std::fs::read_to_string(dir.join("assets/materials/wood.mat.json")).expect("read");
        assert!(after.contains("0.9"));

        outcome.file.revert(&dir).expect("reverts");
        let restored =
            std::fs::read_to_string(dir.join("assets/materials/wood.mat.json")).expect("read");
        assert_eq!(restored, original, "the prior bytes come back exactly");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_material_parameters_are_refused_before_anything_is_written() {
        let dir = temp("invalid");
        let action = ContentAction::CreateMaterial {
            name: "Broken".to_owned(),
            path: None,
            shader: None,
            maps: Default::default(),
            params: Some(json!({ "roughness": 4.0 })),
        };
        let error = action
            .prepare(&dir, &SceneDocument::empty("level_01"))
            .expect_err("out of range");
        assert!(error.message.contains("roughness"));
        assert!(
            !dir.join("assets/materials/broken.mat.json").exists(),
            "nothing is written when preparation fails"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_asset_path_cannot_escape_the_game_folder() {
        let dir = temp("escape");
        let action = ContentAction::CreateMaterial {
            name: "Evil".to_owned(),
            path: Some("../../outside.mat.json".to_owned()),
            shader: None,
            maps: Default::default(),
            params: None,
        };
        let error = action
            .prepare(&dir, &SceneDocument::empty("level_01"))
            .expect_err("path traversal");
        assert!(error.message.contains("leaves the game folder"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn setting_a_licence_keeps_the_assets_existing_id() {
        let dir = temp("license");
        std::fs::create_dir_all(dir.join("assets/textures")).expect("dir");
        std::fs::write(dir.join("assets/textures/wall.png"), b"png").expect("asset");

        let first = ContentAction::SetAssetLicense {
            path: "assets/textures/wall.png".to_owned(),
            license: "CC0-1.0".to_owned(),
        }
        .prepare(&dir, &SceneDocument::empty("l"))
        .expect("prepares");
        first.file.apply(&dir).expect("writes");
        let id_before: String = serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(dir.join("assets/textures/wall.png.meta.json")).expect("read"),
        )
        .expect("json")["id"]
            .as_str()
            .expect("id")
            .to_owned();

        let second = ContentAction::SetAssetLicense {
            path: "assets/textures/wall.png".to_owned(),
            license: "CC-BY-4.0".to_owned(),
        }
        .prepare(&dir, &SceneDocument::empty("l"))
        .expect("prepares");
        second.file.apply(&dir).expect("writes");
        let text =
            std::fs::read_to_string(dir.join("assets/textures/wall.png.meta.json")).expect("read");
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(
            parsed["id"].as_str().expect("id"),
            id_before,
            "an asset's ULID is its identity across licence edits"
        );
        assert_eq!(parsed["license"].as_str(), Some("CC-BY-4.0"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn licensing_a_file_that_does_not_exist_is_refused() {
        let dir = temp("no-file");
        let error = ContentAction::SetAssetLicense {
            path: "assets/textures/ghost.png".to_owned(),
            license: "CC0-1.0".to_owned(),
        }
        .prepare(&dir, &SceneDocument::empty("l"))
        .expect_err("nothing to license");
        assert!(error.hint.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_content_kinds_parse_as_content_actions() {
        assert!(parse_content_action(&json!({ "kind": "spawn", "template": "cube" })).is_none());
        assert!(parse_content_action(&json!({
            "kind": "create_material", "name": "Wood"
        }))
        .is_some());
        // A content kind with a broken payload is not silently treated as a scene action.
        assert!(parse_content_action(&json!({ "kind": "create_shader" })).is_none());
    }

    #[test]
    fn names_become_safe_slugs_and_files_land_by_type() {
        assert_eq!(slug("Wet Concrete #2"), "wet_concrete__2");
        assert_eq!(slug("   "), "asset");
        assert_eq!(import_folder("png"), "assets/textures");
        assert_eq!(import_folder("glb"), "assets/models");
        assert_eq!(import_folder("wav"), "assets/audio");
    }

    #[test]
    fn a_file_change_with_no_prior_deletes_on_revert_even_if_already_gone() {
        let dir = temp("idempotent");
        let change = FileChange {
            rel_path: "assets/materials/x.mat.json".to_owned(),
            wrote: "{}".to_owned(),
            prior: None,
            sidecar: None,
        };
        // Reverting something never written must not error — rollback runs on failure paths.
        change.revert(&dir).expect("revert is idempotent");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
