//! The IPC surface over the splash screen (GAD-161).
//!
//! Five things, one feature: read what this game's splash is, turn a written brief into a
//! spec, build that spec into the game, keep a logo, and take the result out again — as a
//! favourite the studio remembers, or as files on disk the author can use anywhere.
//!
//! Everything a build writes goes through the same path as every other Godot edit —
//! `lower` → `apply_changeset` → `--check-only` → the journal — so a splash is undoable, and
//! a generated script that does not compile leaves the project exactly as it found it. There
//! is no second writer here.

use crate::commands::AppError;
use crate::godot_commands::{
    apply_batch_for, engine_error, normalise_actor, resolve_project, GodotApplyHost,
};
use bhippi_engine::godot::splash::{
    self, SplashBuildOptions, SplashLibraryView, SplashProjectState, SplashSpec,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// The most favourites the panel ever lists.
const FAVOURITE_LIMIT: i64 = 200;

/// The largest logo accepted, in bytes. A splash logo is a piece of UI art, not a texture
/// atlas, and a 200 MB PNG in a game's boot path is a bug rather than a choice.
const MAX_LOGO_BYTES: u64 = 12 * 1024 * 1024;

/// Image extensions Godot imports as a texture without further setup.
const LOGO_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "svg"];

/// What one splash build did.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SplashApplyResult {
    /// Project-relative files the build wrote.
    pub files: Vec<String>,
    /// The scene the splash now hands over to.
    pub next_scene_res: String,
    /// The journal revision the change landed on, when the ledger was available.
    pub revision: Option<i64>,
    /// True when this replaced a splash the project already had.
    pub replaced: bool,
}

/// A logo, once it is in the project.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SplashLogo {
    /// The `res://` path to put in the spec.
    pub res_path: String,
    /// Project-relative, for the file list.
    pub rel_path: String,
    /// The licence recorded beside it.
    pub licence: String,
}

/// One saved splash, as the panel lists it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SplashFavourite {
    pub id: String,
    pub name: String,
    pub spec: SplashSpec,
    /// The project it was saved from. Empty when unknown.
    pub origin: String,
    pub created_at: String,
}

/// What an export wrote.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct SplashExport {
    /// The folder everything was written into.
    pub folder: String,
    /// Absolute paths of the files written, in the order they were written.
    pub files: Vec<String>,
}

/// The motions, backdrops and limits the panel draws its controls from.
///
/// Decided in Rust so the panel never invents a bound of its own (INV-051): the hold the
/// slider offers and the hold the gate accepts are the same two numbers.
#[tauri::command]
#[specta::specta]
#[must_use]
pub fn splash_library() -> SplashLibraryView {
    splash::library()
}

/// What this project's splash currently is.
#[tauri::command]
#[specta::specta]
pub async fn splash_project_state(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
) -> Result<SplashProjectState, AppError> {
    let root = resolve_project(&state, &project).await?;
    Ok(splash::project_state(&root))
}

/// Turn a written brief into a complete spec.
///
/// Deterministic and offline: the panel previews the result as the user types, and a preview
/// that needed a provider, a key and a network round trip would not be a preview. The brief
/// steers the palette, the motion and the hold; everything it does not mention takes a
/// legible default, and the result is always gate-clean.
#[tauri::command]
#[specta::specta]
#[must_use]
pub fn splash_generate(
    brief: String,
    title: String,
    tagline: String,
    logo_res_path: Option<String>,
) -> SplashSpec {
    splash::synthesize(
        &brief,
        &title,
        &tagline,
        logo_res_path.as_deref().filter(|path| !path.is_empty()),
    )
}

/// Build a spec into the game, and make the game boot into it.
///
/// The handover target is decided here rather than by the panel: it is whatever the project
/// boots into *now*, unless a splash is already installed, in which case it is what that
/// splash was built to hand over to. Reading it from the project each time is what stops a
/// rebuild from pointing the splash at itself.
#[tauri::command]
#[specta::specta]
pub async fn splash_apply(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    spec: SplashSpec,
    actor: String,
) -> Result<SplashApplyResult, AppError> {
    let root = resolve_project(&state, &project).await?;
    let current = splash::project_state(&root);

    if current.next_scene_res.trim().is_empty() {
        return Err(AppError {
            message: "This game has no main scene for the splash to hand over to.".to_owned(),
            hint: Some(
                "Set the game's first screen as its main scene, then build the splash in \
                 front of it."
                    .to_owned(),
            ),
        });
    }

    let options = SplashBuildOptions::for_project(&root, spec.clone(), &current.next_scene_res);
    let replaced = options.replace_scene;
    let built = splash::build(&options).map_err(engine_error)?;

    let outcome = apply_batch_for(
        GodotApplyHost { app: Some(&app) },
        &root,
        &built.batch,
        normalise_actor(&actor)?.as_str(),
    )
    .await
    .map_err(|failure| failure.error)?;

    // Only once the batch is on disk. A spec recorded for a build that was rolled back would
    // reopen the panel on a splash the game does not have.
    write_spec(&root, &spec)?;

    Ok(SplashApplyResult {
        files: built.files,
        next_scene_res: built.next_scene_res,
        revision: outcome.revision,
        replaced,
    })
}

/// Copy a logo into the game and record its licence.
///
/// The licence is required and is written into a `.meta.json` beside the file, the same way
/// every other imported asset carries one (INV-074). An unlicensed logo in a game's boot
/// screen is exactly the asset that ends up in a store listing.
#[tauri::command]
#[specta::specta]
pub async fn splash_import_logo(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    source: String,
    licence: String,
) -> Result<SplashLogo, AppError> {
    let root = resolve_project(&state, &project).await?;
    let licence = licence.trim();
    if licence.is_empty() {
        return Err(AppError {
            message: "A logo needs a licence before it can ship in a game.".to_owned(),
            hint: Some(
                "Name the terms you hold it under — for example CC0-1.0, or \"own artwork\"."
                    .to_owned(),
            ),
        });
    }

    let source_path = PathBuf::from(source.trim());
    let extension = source_path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !LOGO_EXTENSIONS.contains(&extension.as_str()) {
        return Err(AppError {
            message: format!("Godot does not import `.{extension}` as a texture."),
            hint: Some(format!("Use one of: {}.", LOGO_EXTENSIONS.join(", "))),
        });
    }

    let meta = tokio::fs::metadata(&source_path)
        .await
        .map_err(|error| AppError {
            message: format!("Cannot read `{}`: {error}", source_path.display()),
            hint: Some("Pick the file again.".to_owned()),
        })?;
    if !meta.is_file() {
        return Err(AppError {
            message: format!("`{}` is not a file.", source_path.display()),
            hint: None,
        });
    }
    if meta.len() > MAX_LOGO_BYTES {
        return Err(AppError {
            message: format!(
                "That logo is {:.1} MB; the limit is {} MB.",
                meta.len() as f64 / (1024.0 * 1024.0),
                MAX_LOGO_BYTES / (1024 * 1024)
            ),
            hint: Some("Export it at the size it is shown, around 512 px wide.".to_owned()),
        });
    }

    let directory = root.join(splash::SPLASH_LOGO_DIR);
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError {
            message: format!("Cannot create `{}`: {error}", directory.display()),
            hint: None,
        })?;

    let file_name = format!("logo.{extension}");
    let destination = directory.join(&file_name);
    tokio::fs::copy(&source_path, &destination)
        .await
        .map_err(|error| AppError {
            message: format!("Cannot copy the logo in: {error}"),
            hint: None,
        })?;

    let sidecar = serde_json::json!({
        "license": licence,
        "importer": "bhippi.splash",
        "imported_at": chrono::Utc::now().to_rfc3339(),
        "source": source_path.display().to_string().replace('\\', "/"),
    });
    let sidecar_text = serde_json::to_string_pretty(&sidecar).map_err(|error| AppError {
        message: format!("Cannot record the logo's licence: {error}"),
        hint: None,
    })?;
    tokio::fs::write(
        directory.join(format!("{file_name}.meta.json")),
        sidecar_text,
    )
    .await
    .map_err(|error| AppError {
        message: format!("Cannot record the logo's licence: {error}"),
        hint: None,
    })?;

    let rel_path = format!("{}/{file_name}", splash::SPLASH_LOGO_DIR);
    Ok(SplashLogo {
        res_path: format!("res://{rel_path}"),
        rel_path,
        licence: licence.to_owned(),
    })
}

/// Remember a splash, so the next game can start from it.
#[tauri::command]
#[specta::specta]
pub async fn splash_favourite(
    state: tauri::State<'_, crate::Runtime>,
    name: String,
    spec: SplashSpec,
    origin: String,
) -> Result<SplashFavourite, AppError> {
    let database = favourites_db(&state)?;
    let name = {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            spec.title.trim().to_owned()
        } else {
            trimmed.to_owned()
        }
    };
    let name = if name.is_empty() {
        "Untitled splash".to_owned()
    } else {
        name
    };

    let spec_json = serde_json::to_string(&spec).map_err(|error| AppError {
        message: format!("Cannot save that splash: {error}"),
        hint: None,
    })?;
    let now = chrono::Utc::now();
    let entry = bhippi_db::NewSplashFavourite {
        id: ulid::Ulid::new().to_string(),
        name: name.clone(),
        spec_json,
        origin: origin.replace('\\', "/"),
    };
    database
        .splash()
        .save(&entry, &now)
        .await
        .map_err(|error| AppError {
            message: format!("Cannot save that splash: {error}"),
            hint: Some("Run `bhippi doctor` and try again.".to_owned()),
        })?;

    Ok(SplashFavourite {
        id: entry.id,
        name,
        spec,
        origin: entry.origin,
        created_at: now.to_rfc3339(),
    })
}

/// Every saved splash, newest first.
///
/// A row whose stored spec no longer parses is skipped rather than fatal: one favourite
/// saved by an older build must not empty the whole list.
#[tauri::command]
#[specta::specta]
pub async fn splash_favourites(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<SplashFavourite>, AppError> {
    let database = favourites_db(&state)?;
    let rows = database
        .splash()
        .list(FAVOURITE_LIMIT)
        .await
        .map_err(|error| AppError {
            message: format!("Cannot read the saved splashes: {error}"),
            hint: Some("Run `bhippi doctor` and try again.".to_owned()),
        })?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let spec = serde_json::from_str::<SplashSpec>(&row.spec_json).ok()?;
            Some(SplashFavourite {
                id: row.id,
                name: row.name,
                spec,
                origin: row.origin,
                created_at: row.created_at,
            })
        })
        .collect())
}

/// Forget one saved splash.
#[tauri::command]
#[specta::specta]
pub async fn splash_forget_favourite(
    state: tauri::State<'_, crate::Runtime>,
    id: String,
) -> Result<bool, AppError> {
    let database = favourites_db(&state)?;
    database
        .splash()
        .remove(id.trim())
        .await
        .map_err(|error| AppError {
            message: format!("Cannot forget that splash: {error}"),
            hint: None,
        })
}

/// Write the splash out as files the author can use anywhere.
///
/// Four things land in the folder: the splash as an SVG that any design tool opens, the spec
/// as JSON so Bhippi can rebuild it, and the generated scene and script when they exist. The
/// logo is copied beside them. Nothing here runs Godot — an export that needed a working
/// engine install would fail exactly when someone wanted the file for a store page.
#[tauri::command]
#[specta::specta]
pub async fn splash_export(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    destination: String,
    spec: SplashSpec,
) -> Result<SplashExport, AppError> {
    let root = resolve_project(&state, &project).await?;
    let folder = PathBuf::from(destination.trim());
    if folder.as_os_str().is_empty() {
        return Err(AppError {
            message: "No folder was chosen for the export.".to_owned(),
            hint: Some("Pick where the splash should be written.".to_owned()),
        });
    }
    tokio::fs::create_dir_all(&folder)
        .await
        .map_err(|error| AppError {
            message: format!("Cannot create `{}`: {error}", folder.display()),
            hint: None,
        })?;

    let stem = file_stem_for(&spec.title);
    let mut files: Vec<String> = Vec::new();

    let svg_path = folder.join(format!("{stem}.svg"));
    write_file(&svg_path, splash::svg(&spec).as_bytes()).await?;
    files.push(display_of(&svg_path));

    let spec_json = serde_json::to_string_pretty(&spec).map_err(|error| AppError {
        message: format!("Cannot write the splash spec: {error}"),
        hint: None,
    })?;
    let spec_path = folder.join(format!("{stem}.splash.json"));
    write_file(&spec_path, spec_json.as_bytes()).await?;
    files.push(display_of(&spec_path));

    for rel in [splash::SPLASH_SCENE_REL, splash::SPLASH_SCRIPT_REL] {
        let source = root.join(rel);
        if !source.is_file() {
            continue;
        }
        let Some(name) = Path::new(rel).file_name() else {
            continue;
        };
        let destination = folder.join(name);
        if let Ok(bytes) = tokio::fs::read(&source).await {
            write_file(&destination, &bytes).await?;
            files.push(display_of(&destination));
        }
    }

    if let Some(logo) = &spec.logo_res_path {
        let rel = logo.trim_start_matches("res://");
        let source = root.join(rel);
        if source.is_file() {
            if let Some(name) = source.file_name() {
                let destination = folder.join(name);
                if let Ok(bytes) = tokio::fs::read(&source).await {
                    write_file(&destination, &bytes).await?;
                    files.push(display_of(&destination));
                }
            }
        }
    }

    Ok(SplashExport {
        folder: display_of(&folder),
        files,
    })
}

fn display_of(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

async fn write_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    tokio::fs::write(path, bytes)
        .await
        .map_err(|error| AppError {
            message: format!("Cannot write `{}`: {error}", path.display()),
            hint: Some("Choose a folder you can write to.".to_owned()),
        })
}

/// A file name from a game's title: lowercase, words joined by `-`, nothing a filesystem
/// will argue with.
fn file_stem_for(title: &str) -> String {
    let mut stem = String::new();
    let mut pending_dash = false;
    for ch in title.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !stem.is_empty() {
                stem.push('-');
            }
            pending_dash = false;
            stem.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if stem.is_empty() {
        "splash".to_owned()
    } else {
        stem
    }
}

/// Record the spec beside the project so the panel reopens on what was built.
fn write_spec(root: &Path, spec: &SplashSpec) -> Result<(), AppError> {
    let path = root.join(splash::SPLASH_SPEC_REL);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| AppError {
            message: format!("Cannot create `{}`: {error}", parent.display()),
            hint: None,
        })?;
    }
    let text = serde_json::to_string_pretty(spec).map_err(|error| AppError {
        message: format!("Cannot record the splash: {error}"),
        hint: None,
    })?;
    std::fs::write(&path, text).map_err(|error| AppError {
        message: format!("Cannot record the splash: {error}"),
        hint: None,
    })
}

fn favourites_db<'a>(
    state: &'a tauri::State<'a, crate::Runtime>,
) -> Result<&'a bhippi_db::Database, AppError> {
    state.brain_db.as_ref().as_ref().ok_or_else(|| AppError {
        message: "The studio database is unavailable, so splashes cannot be saved.".to_owned(),
        hint: Some("Run `bhippi doctor` and restart Bhippi.".to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_becomes_a_file_name_a_filesystem_accepts() {
        assert_eq!(file_stem_for("Greenwood"), "greenwood");
        assert_eq!(file_stem_for("Green & Wood II"), "green-wood-ii");
        assert_eq!(file_stem_for("  spaced  out  "), "spaced-out");
        assert_eq!(file_stem_for("///"), "splash", "never an empty name");
        assert_eq!(file_stem_for(""), "splash");
    }

    #[test]
    fn every_logo_extension_offered_is_one_godot_imports() {
        // The list the error message prints and the list the check uses are the same list,
        // so an accepted file can never be one the message said was unsupported.
        for extension in LOGO_EXTENSIONS {
            assert!(
                !extension.starts_with('.'),
                "extensions are compared without their dot"
            );
            assert_eq!(*extension, extension.to_ascii_lowercase());
        }
    }
}
