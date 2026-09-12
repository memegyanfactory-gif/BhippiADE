//! Workbench filesystem and preview commands for the in-app editor and browser.
//!
//! Every path here is resolved against the **active project held in Rust state** and
//! canonicalized before use, then checked to still sit inside that root (ADR-0013).
//! A relative path the frontend supplies can therefore never reach a sibling project,
//! a parent directory, or a home-directory file — including through a symlink, because
//! the check runs on the canonical target rather than on the string.
//!
//! Filesystem and process behavior stays in Rust (R3): the TypeScript side receives
//! already-decided rows and never joins, walks, or probes a path itself.

use crate::commands::{required_project_path, AppError};
use base64::Engine;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Component, Path, PathBuf};

/// Files larger than this are reported, not loaded — an editor that freezes on a
/// 40 MB log is worse than one that says the file is too large to open.
const MAX_EDITABLE_BYTES: u64 = 1_048_576;

/// Directories the tree never descends into. Build output and dependency trees are
/// tens of thousands of entries that nobody navigates by hand.
const SKIPPED_DIRECTORIES: [&str; 10] = [
    "node_modules",
    "target",
    ".git",
    "dist",
    "build",
    ".next",
    ".svelte-kit",
    ".turbo",
    "__pycache__",
    ".venv",
];

/// One row in the workbench file tree.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct WorkspaceEntry {
    /// Display name — the final path segment.
    pub name: String,
    /// Project-relative path with `/` separators, stable across platforms.
    pub path: String,
    pub is_directory: bool,
    /// Byte length for files; `0` for directories, which are not measured.
    pub size: u64,
    /// True when the directory holds at least one visible entry, so the tree can draw
    /// a disclosure chevron without listing the children first.
    pub has_children: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FilePreviewKind {
    Text,
    Image,
    Model,
    Binary,
}

pub(crate) fn preview_type(extension: &str) -> (FilePreviewKind, Option<&'static str>) {
    let mime = match extension.to_ascii_lowercase().as_str() {
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        "avif" => Some("image/avif"),
        _ => None,
    };
    if mime.is_some() {
        return (FilePreviewKind::Image, mime);
    }
    if matches!(
        extension.to_ascii_lowercase().as_str(),
        "glb" | "gltf" | "obj" | "fbx" | "blend" | "stl" | "ply" | "dae"
    ) {
        return (FilePreviewKind::Model, None);
    }
    (FilePreviewKind::Text, None)
}

/// A file the editor has opened.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct WorkspaceFile {
    pub preview_kind: FilePreviewKind,
    pub preview_mime: Option<String>,
    pub path: String,
    pub name: String,
    pub text: String,
    pub bytes: u64,
    /// True when the file was too large to load; saving is refused for these.
    pub truncated: bool,
    /// Lowercase extension, or an empty string — the highlighter's only input.
    pub language: String,
    /// False when the file is too large or not text; the editor then opens read-only.
    pub editable: bool,
    /// Base64-encoded content for binary files (images, etc.). `None` for text files.
    pub content_base64: Option<String>,
}

/// A localhost address the in-app browser can try.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PreviewTarget {
    pub url: String,
    /// Where the guess came from — a framework default, plus the project's own dev
    /// script when it has one. Never an invented address.
    pub label: String,
    /// True only when a TCP connection to that port actually succeeded just now.
    pub reachable: bool,
}

/// Ports worth probing: the defaults of the dev servers this stack tends to run.
const CANDIDATE_PORTS: [(u16, &str); 9] = [
    (5173, "Vite"),
    (3000, "Next.js / Node"),
    (4321, "Astro"),
    (8080, "HTTP server"),
    (1420, "Tauri dev"),
    (5000, "Flask / Node"),
    (8000, "Python / Django"),
    (4200, "Angular"),
    (3001, "Node"),
];

fn to_relative_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Rejects a relative path outright when it carries traversal or an absolute root.
///
/// This runs *before* the filesystem is touched, so an obviously hostile string never
/// becomes a `canonicalize` call. It is not the only defence: [`resolve`] still checks
/// the canonical result against the project root.
fn sanitize_relative(relative: &str) -> Result<PathBuf, AppError> {
    let trimmed = relative.trim().replace('\\', "/");
    let mut safe = PathBuf::new();
    for segment in trimmed.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                return Err(AppError {
                    message: "That path leaves the project folder.".to_owned(),
                    hint: Some(
                        "The workbench only opens files inside the open project.".to_owned(),
                    ),
                })
            }
            other => {
                if other.contains(':') || other.chars().any(char::is_control) {
                    return Err(AppError::plain("That path is not a valid project path."));
                }
                safe.push(other);
            }
        }
    }
    Ok(safe)
}

/// Resolves `relative` inside the active project and proves the result stays there.
///
/// Returns the canonical project root alongside the canonical target, because callers
/// need the root to turn the result back into a project-relative display path.
pub(crate) async fn resolve(
    state: &crate::Runtime,
    relative: &str,
) -> Result<(PathBuf, PathBuf), AppError> {
    let root = PathBuf::from(required_project_path(state).await?);
    let root = std::fs::canonicalize(&root).map_err(|error| AppError {
        message: format!("The open project is unavailable: {error}"),
        hint: Some("Reopen the project, then try again.".to_owned()),
    })?;
    let requested = sanitize_relative(relative)?;
    if requested.as_os_str().is_empty() {
        return Ok((root.clone(), root));
    }
    let joined = root.join(&requested);
    // Canonicalizing the target is what makes a symlink pointing outside the project
    // fail here rather than quietly open somebody else's file.
    let canonical = std::fs::canonicalize(&joined).map_err(|error| AppError {
        message: format!("That file is unavailable: {error}"),
        hint: Some("It may have been renamed, moved, or deleted.".to_owned()),
    })?;
    if !canonical.starts_with(&root) {
        return Err(AppError {
            message: "That path resolves outside the open project.".to_owned(),
            hint: Some("The workbench is confined to the project folder.".to_owned()),
        });
    }
    Ok((root, canonical))
}

/// Dotfiles stay visible — `.github`, `.cargo`, and `.gitignore` are all things people
/// edit — but the per-tool caches above are noise in a navigator.
fn is_skipped(name: &str, is_directory: bool) -> bool {
    is_directory && SKIPPED_DIRECTORIES.contains(&name)
}

fn directory_has_children(path: &Path) -> bool {
    let Ok(mut entries) = std::fs::read_dir(path) else {
        return false;
    };
    entries.any(|entry| {
        entry.is_ok_and(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
            !is_skipped(&name, is_directory)
        })
    })
}

/// Lists one directory of the open project, directories first then files, each A–Z.
///
/// Listing is per-directory rather than a recursive walk, so opening a large repository
/// costs one `read_dir` and a folder nobody expands is never read at all.
///
/// # Errors
/// Fails when no project is open, when the path leaves the project, or when the
/// directory cannot be read.
#[tauri::command]
#[specta::specta]
pub async fn list_workspace_dir(
    state: tauri::State<'_, crate::Runtime>,
    relative: String,
) -> Result<Vec<WorkspaceEntry>, AppError> {
    let (root, directory) = resolve(state.inner(), &relative).await?;
    if !directory.is_dir() {
        return Err(AppError::plain("That path is not a folder."));
    }
    let mut rows = Vec::new();
    let mut reader = tokio::fs::read_dir(&directory)
        .await
        .map_err(|error| AppError::plain(format!("Could not read that folder: {error}")))?;
    while let Ok(Some(entry)) = reader.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        let is_directory = metadata.is_dir();
        if is_skipped(&name, is_directory) {
            continue;
        }
        let Ok(relative_path) = entry.path().strip_prefix(&root).map(to_relative_string) else {
            continue;
        };
        rows.push(WorkspaceEntry {
            name,
            path: relative_path,
            is_directory,
            size: if is_directory { 0 } else { metadata.len() },
            has_children: is_directory && directory_has_children(&entry.path()),
        });
    }
    rows.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(rows)
}

fn language_of(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Reads one project file for the editor.
///
/// Binary content and anything over 1 MB come back flagged rather than as an error, so
/// the editor can say *why* a file will not open instead of showing a blank pane.
///
/// # Errors
/// Fails when no project is open, when the path leaves the project, or when the file
/// cannot be read.
#[tauri::command]
#[specta::specta]
pub async fn read_workspace_file(
    state: tauri::State<'_, crate::Runtime>,
    relative: String,
    expected_project: Option<String>,
) -> Result<WorkspaceFile, AppError> {
    let (root, path) = resolve(state.inner(), &relative).await?;
    if let Some(expected) = expected_project {
        let expected = tokio::fs::canonicalize(expected)
            .await
            .map_err(|error| AppError::plain(error.to_string()))?;
        if expected != root {
            return Err(AppError::plain(
                "The active project changed. Select the file again.",
            ));
        }
    }
    read_file_at(&root, &path).await
}

async fn read_file_at(root: &Path, path: &Path) -> Result<WorkspaceFile, AppError> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| AppError::plain(format!("Could not open that file: {error}")))?;
    if metadata.is_dir() {
        return Err(AppError::plain("That path is a folder, not a file."));
    }
    let bytes = metadata.len();
    let name = file_name_of(path);
    let relative_path = path
        .strip_prefix(root)
        .map(to_relative_string)
        .unwrap_or_else(|_| name.clone());
    let language = language_of(path);
    let (preview_kind, mime) = preview_type(&language);
    let preview_mime = mime.map(str::to_owned);
    if preview_kind == FilePreviewKind::Model {
        return Ok(WorkspaceFile {
            preview_kind,
            preview_mime,
            path: relative_path,
            name,
            text: String::new(),
            bytes,
            truncated: false,
            language,
            editable: false,
            content_base64: None,
        });
    }
    if preview_kind == FilePreviewKind::Image && bytes <= bhippi_types::WORKSPACE_IMAGE_MAX_BYTES {
        let raw = tokio::fs::read(path)
            .await
            .map_err(|error| AppError::plain(error.to_string()))?;
        let text = if language == "svg" {
            String::from_utf8(raw.clone())
                .map_err(|error| AppError::plain(format!("Invalid SVG text: {error}")))?
        } else {
            String::new()
        };
        return Ok(WorkspaceFile {
            preview_kind,
            preview_mime,
            path: relative_path,
            name,
            text,
            bytes,
            truncated: false,
            editable: language == "svg" && bytes <= MAX_EDITABLE_BYTES,
            language,
            content_base64: Some(base64::engine::general_purpose::STANDARD.encode(raw)),
        });
    }
    if bytes > MAX_EDITABLE_BYTES {
        return Ok(WorkspaceFile {
            preview_kind,
            preview_mime,
            path: relative_path,
            name,
            text: String::new(),
            bytes,
            truncated: true,
            language,
            editable: false,
            content_base64: None,
        });
    }
    let raw = tokio::fs::read(path)
        .await
        .map_err(|error| AppError::plain(format!("Could not read that file: {error}")))?;
    // A NUL byte in the first block is the practical binary test. Decoding a PNG into
    // replacement characters and calling it text would let someone "save" a corrupted
    // image back over the original.
    let binary = raw.iter().take(8_000).any(|byte| *byte == 0);
    match (binary, String::from_utf8(raw.clone())) {
        (false, Ok(text)) => Ok(WorkspaceFile {
            preview_kind,
            preview_mime,
            path: relative_path,
            name,
            text,
            bytes,
            truncated: false,
            language,
            editable: true,
            content_base64: None,
        }),
        _ => {
            let preview_kind = FilePreviewKind::Binary;
            let encoded = base64::engine::general_purpose::STANDARD.encode(&raw);
            Ok(WorkspaceFile {
                preview_kind,
                preview_mime,
                path: relative_path,
                name,
                text: String::new(),
                bytes,
                truncated: false,
                language,
                editable: false,
                content_base64: Some(encoded),
            })
        }
    }
}

/// Resolves a project-relative path for create/write without requiring the file to exist.
async fn resolve_for_write(
    state: &crate::Runtime,
    relative: &str,
) -> Result<(PathBuf, PathBuf), AppError> {
    let root = PathBuf::from(required_project_path(state).await?);
    let root = std::fs::canonicalize(&root).map_err(|error| AppError {
        message: format!("The open project is unavailable: {error}"),
        hint: Some("Reopen the project, then try again.".to_owned()),
    })?;
    let requested = sanitize_relative(relative)?;
    if requested.as_os_str().is_empty() {
        return Err(AppError::plain("Give the file a path inside the project."));
    }
    let joined = root.join(&requested);
    if let Some(parent) = joined.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| AppError {
                message: format!("Could not create that folder: {error}"),
                hint: Some("Check that the project folder is writable.".to_owned()),
            })?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(|error| AppError {
            message: format!("Could not resolve that folder: {error}"),
            hint: Some("Pick a folder inside the open project.".to_owned()),
        })?;
        if !canonical_parent.starts_with(&root) {
            return Err(AppError {
                message: "That path resolves outside the open project.".to_owned(),
                hint: Some("The workbench is confined to the project folder.".to_owned()),
            });
        }
    }
    Ok((root, joined))
}

/// Writes a file into the open project, creating it (and parent folders) when missing.
///
/// Pressing save **is** the permission for this write, but it is refused for a file
/// that was never fully loaded: saving a truncated buffer would silently delete the
/// rest of the file.
///
/// # Errors
/// Fails when no project is open, when the path leaves the project, when the target is
/// a directory, or when the write itself fails.
#[tauri::command]
#[specta::specta]
pub async fn write_workspace_file(
    state: tauri::State<'_, crate::Runtime>,
    relative: String,
    text: String,
) -> Result<WorkspaceFile, AppError> {
    let exists = resolve(state.inner(), &relative).await;
    let (_root, path) = match exists {
        Ok(pair) => pair,
        Err(_) => resolve_for_write(state.inner(), &relative).await?,
    };
    if tokio::fs::metadata(&path)
        .await
        .map(|meta| meta.is_dir())
        .unwrap_or(false)
    {
        return Err(AppError::plain("That path is a folder, not a file."));
    }
    if let Ok(metadata) = tokio::fs::metadata(&path).await {
        if metadata.len() > MAX_EDITABLE_BYTES {
            return Err(AppError {
                message: "That file is too large to edit in Bhippi.".to_owned(),
                hint: Some("Open it in an external editor instead.".to_owned()),
            });
        }
    }
    tokio::fs::write(&path, text.as_bytes())
        .await
        .map_err(|error| AppError::plain(format!("Could not save that file: {error}")))?;
    read_workspace_file(state, relative, None).await
}

/// Copies a user-picked file from anywhere on disk into the open project.
///
/// Used by Engine Replace Object / texture import. The destination must stay inside
/// the project; the source is the path the OS file dialog already authorised.
#[tauri::command]
#[specta::specta]
pub async fn import_workspace_file(
    state: tauri::State<'_, crate::Runtime>,
    source_absolute: String,
    dest_relative: String,
) -> Result<WorkspaceEntry, AppError> {
    let source = PathBuf::from(source_absolute.trim());
    if !source.is_file() {
        return Err(AppError {
            message: "Pick a file that exists on this computer.".to_owned(),
            hint: Some("Use the file dialog, then try again.".to_owned()),
        });
    }
    let (root, dest) = resolve_for_write(state.inner(), &dest_relative).await?;
    tokio::fs::copy(&source, &dest)
        .await
        .map_err(|error| AppError::plain(format!("Could not import that file: {error}")))?;
    let metadata = tokio::fs::metadata(&dest)
        .await
        .map_err(|error| AppError::plain(format!("Imported file is unreadable: {error}")))?;
    Ok(WorkspaceEntry {
        name: file_name_of(&dest),
        path: dest
            .strip_prefix(&root)
            .map(to_relative_string)
            .unwrap_or_else(|_| dest_relative.replace('\\', "/")),
        is_directory: false,
        size: metadata.len(),
        has_children: false,
    })
}

/// True when something is listening on `127.0.0.1:port` right now.
async fn port_is_live(port: u16) -> bool {
    let connect = tokio::net::TcpStream::connect(("127.0.0.1", port));
    matches!(
        tokio::time::timeout(std::time::Duration::from_millis(180), connect).await,
        Ok(Ok(_))
    )
}

/// Reads `package.json` for a dev script, purely so an idle port can be labelled with
/// the command that would actually start something on it.
async fn dev_script(root: &Path) -> Option<String> {
    let raw = tokio::fs::read_to_string(root.join("package.json"))
        .await
        .ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&raw).ok()?;
    let scripts = value.get("scripts")?.as_object()?;
    ["dev", "start", "serve", "preview"]
        .into_iter()
        .find(|name| scripts.contains_key(*name))
        .map(|name| format!("npm run {name}"))
}

/// Probes the usual local dev-server ports and reports what answered.
///
/// Nothing here starts a server or assumes a running one: a target is marked reachable
/// only when a TCP connection to it succeeded during this call.
///
/// # Errors
/// Fails when no project is open.
#[tauri::command]
#[specta::specta]
pub async fn preview_targets(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<PreviewTarget>, AppError> {
    let root = PathBuf::from(required_project_path(state.inner()).await?);
    let hint = dev_script(&root).await;

    let mut rows = Vec::with_capacity(CANDIDATE_PORTS.len());
    for (port, label) in CANDIDATE_PORTS {
        let reachable = port_is_live(port).await;
        rows.push(PreviewTarget {
            url: format!("http://localhost:{port}"),
            label: match (reachable, hint.as_deref()) {
                (true, _) | (false, None) => label.to_owned(),
                (false, Some(command)) => format!("{label} · try {command}"),
            },
            reachable,
        });
    }
    rows.sort_by_key(|row| !row.reachable);
    Ok(rows)
}

/// Where a project's standing agent rules live, relative to its root.
pub const RULES_RELATIVE_PATH: &str = ".bhippi/rules.md";

/// Rules the project owner wrote, plus where they are stored.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ProjectRules {
    /// Markdown as typed. Empty when the file does not exist yet.
    pub text: String,
    /// Project-relative location, shown so the file can be edited outside Bhippi too.
    pub path: String,
    /// False before the file has ever been saved.
    pub exists: bool,
}

/// Reads the open project's rules file, or reports an empty, not-yet-written one.
///
/// A missing file is the ordinary first-run state, not an error — the panel opens on an
/// empty editor rather than on a failure the user can do nothing about.
///
/// # Errors
/// Fails when no project is open, or when an existing rules file cannot be read.
#[tauri::command]
#[specta::specta]
pub async fn read_project_rules(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<ProjectRules, AppError> {
    let root = PathBuf::from(required_project_path(state.inner()).await?);
    let path = root.join(".bhippi").join("rules.md");
    match tokio::fs::read_to_string(&path).await {
        Ok(text) => Ok(ProjectRules {
            text,
            path: RULES_RELATIVE_PATH.to_owned(),
            exists: true,
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ProjectRules {
            text: String::new(),
            path: RULES_RELATIVE_PATH.to_owned(),
            exists: false,
        }),
        Err(error) => Err(AppError::plain(format!(
            "Could not read the project rules: {error}"
        ))),
    }
}

/// Writes the open project's rules file, creating `.bhippi/` on first save.
///
/// # Errors
/// Fails when no project is open, or when the file cannot be created or written.
#[tauri::command]
#[specta::specta]
pub async fn write_project_rules(
    state: tauri::State<'_, crate::Runtime>,
    text: String,
) -> Result<ProjectRules, AppError> {
    let root = PathBuf::from(required_project_path(state.inner()).await?);
    let directory = root.join(".bhippi");
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError {
            message: format!("Could not create the project rules folder: {error}"),
            hint: Some("Check that the project folder is writable.".to_owned()),
        })?;
    tokio::fs::write(directory.join("rules.md"), text.as_bytes())
        .await
        .map_err(|error| AppError::plain(format!("Could not save the project rules: {error}")))?;
    Ok(ProjectRules {
        text,
        path: RULES_RELATIVE_PATH.to_owned(),
        exists: true,
    })
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn asset_files_are_routed_without_the_text_size_limit() {
        use super::{read_file_at, FilePreviewKind};
        let folder =
            std::env::temp_dir().join(format!("bhippi-file-preview-test-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&folder).unwrap();
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"10\"/></svg>";
        std::fs::write(folder.join("icon.svg"), svg).unwrap();
        let loaded = read_file_at(&folder, &folder.join("icon.svg"))
            .await
            .unwrap();
        assert_eq!(loaded.preview_kind, FilePreviewKind::Image);
        assert_eq!(loaded.preview_mime.as_deref(), Some("image/svg+xml"));
        assert_eq!(loaded.text, svg);
        assert!(loaded.editable && loaded.content_base64.is_some());
        std::fs::write(folder.join("large.PNG"), vec![128_u8; 2_000_000]).unwrap();
        let image = read_file_at(&folder, &folder.join("large.PNG"))
            .await
            .unwrap();
        assert_eq!(image.preview_kind, FilePreviewKind::Image);
        assert!(!image.truncated && !image.editable && image.content_base64.is_some());
        for extension in ["glb", "gltf", "obj", "fbx", "blend", "stl", "ply", "dae"] {
            let path = folder.join(format!("mesh.{extension}"));
            std::fs::write(&path, vec![0_u8; 2_000_000]).unwrap();
            let model = read_file_at(&folder, &path).await.unwrap();
            assert_eq!(model.preview_kind, FilePreviewKind::Model);
            assert!(!model.editable && !model.truncated);
            assert!(model.text.is_empty() && model.content_base64.is_none());
        }
        std::fs::write(folder.join("data.bin"), [0_u8, 255, 128]).unwrap();
        let binary = read_file_at(&folder, &folder.join("data.bin"))
            .await
            .unwrap();
        assert_eq!(binary.preview_kind, FilePreviewKind::Binary);
        assert!(!binary.editable);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[tokio::test]
    async fn oversize_images_and_text_explain_why_they_cannot_open() {
        use super::read_file_at;
        let folder =
            std::env::temp_dir().join(format!("bhippi-file-size-test-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&folder).unwrap();
        for (name, bytes) in [
            ("huge.png", bhippi_types::WORKSPACE_IMAGE_MAX_BYTES + 1),
            ("huge.txt", super::MAX_EDITABLE_BYTES + 1),
        ] {
            let path = folder.join(name);
            std::fs::File::create(&path)
                .unwrap()
                .set_len(bytes)
                .unwrap();
            let loaded = read_file_at(&folder, &path).await.unwrap();
            assert!(loaded.truncated && !loaded.editable);
            assert!(loaded.content_base64.is_none());
        }
        std::fs::remove_dir_all(folder).unwrap();
    }

    use super::{file_name_of, language_of, sanitize_relative, to_relative_string};
    use std::path::{Path, PathBuf};

    #[test]
    fn traversal_is_refused_before_the_filesystem_is_touched() {
        assert!(sanitize_relative("../secrets.toml").is_err());
        assert!(sanitize_relative("src/../../other-project").is_err());
        assert!(sanitize_relative("nested/..").is_err());
    }

    #[test]
    fn drive_prefixes_and_control_characters_are_refused() {
        assert!(sanitize_relative("C:/Windows/System32").is_err());
        assert!(sanitize_relative("src\\lib\\..\\..\\etc").is_err());
        assert!(sanitize_relative("src/lib\u{0}.rs").is_err());
    }

    #[test]
    fn ordinary_relative_paths_normalise_to_platform_separators() {
        assert_eq!(
            sanitize_relative("ui/src/App.tsx").ok(),
            Some(PathBuf::from("ui").join("src").join("App.tsx"))
        );
        assert_eq!(
            sanitize_relative("./ui//src/").ok(),
            Some(PathBuf::from("ui").join("src"))
        );
        assert_eq!(sanitize_relative("").ok(), Some(PathBuf::new()));
    }

    #[test]
    fn relative_display_always_uses_forward_slashes() {
        let path = Path::new("crates").join("bhippi-app").join("src");
        assert_eq!(to_relative_string(&path), "crates/bhippi-app/src");
    }

    #[test]
    fn language_is_the_lowercased_extension() {
        assert_eq!(language_of(Path::new("App.TSX")), "tsx");
        assert_eq!(language_of(Path::new("Makefile")), "");
        assert_eq!(file_name_of(Path::new("ui/src/App.tsx")), "App.tsx");
    }
}
