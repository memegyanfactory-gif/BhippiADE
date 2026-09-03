//! The user's own asset library (SPA-101…103): folders outside any project that Bhippi may
//! read from, search, and copy into `assets/` — and that the agent may draw on.
//!
//! Three rules keep it honest. A folder is *read*, never written: importing copies a file
//! into the project and touches nothing in the library. The agent never invents a path: an
//! `<asset_import>` is refused unless its source sits under a registered folder, so the model
//! can only name what the index showed it. And a copied file arrives with a sidecar, because
//! a library file is a user file and the release gate (INV-074) still blocks on `unknown` —
//! the sidecar says where it came from and what licence, if any, travelled with it.

use crate::commands::AppError;
use crate::studio_dock::{
    asset_kind, is_asset_file, licence_from_meta, ProjectAsset, ProjectAssetKind,
};
use bhippi_engine::godot::gates::{LICENSE_SIDECAR_SUFFIX, MAX_SCAN_DEPTH};
use bhippi_engine::godot::ASSETS_DIR;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The most files one folder scan lists. A bigger library is still searchable — the walk
/// stops and says so — and the index the agent sees is capped far below this anyway.
pub const MAX_LIBRARY_FILES: usize = 4_000;
/// How many paths the agent's index names per kind and folder. Retrieval, not a dump.
const INDEX_EXAMPLES_PER_KIND: usize = 24;
/// Directories a library walk never enters.
const SKIPPED: [&str; 8] = [
    ".git",
    ".godot",
    ".import",
    ".bhippi",
    "node_modules",
    "target",
    "__pycache__",
    ".cache",
];
/// Licence files a library root may carry; the first one found names the folder's licence.
const LICENCE_FILES: [&str; 6] = [
    "LICENSE",
    "LICENSE.txt",
    "LICENSE.md",
    "LICENCE",
    "LICENCE.txt",
    "License.txt",
];

// ── shapes ───────────────────────────────────────────────────────────────────────────

/// How many files of one kind a folder holds.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct KindCount {
    pub kind: ProjectAssetKind,
    pub label: String,
    pub count: u32,
}

/// One registered folder, as the Assets screen lists it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct LibraryFolder {
    /// The path as registered.
    pub path: String,
    /// Its last segment, for the chip.
    pub name: String,
    /// False when the folder is gone; the row stays so the user can remove it knowingly.
    pub exists: bool,
    pub file_count: u32,
    pub counts: Vec<KindCount>,
    /// True when the walk stopped at [`MAX_LIBRARY_FILES`].
    pub truncated: bool,
    /// What a `LICENSE` file at the root says, when there is one.
    pub licence: Option<String>,
}

/// One file in a library, as a search result and as an import source.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct LibraryAsset {
    /// Absolute — the value `<asset_import>` and the Add button hand back.
    pub path: String,
    /// The registered folder it belongs to.
    pub folder: String,
    /// Relative to that folder, forward slashes.
    pub rel: String,
    pub name: String,
    pub kind: ProjectAssetKind,
    pub kind_label: String,
    pub size_bytes: u64,
    /// A sibling sidecar's licence, else the folder's licence file, else nothing.
    pub licence: Option<String>,
}

/// The whole library, for the screen and the dock.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AssetLibraryView {
    pub folders: Vec<LibraryFolder>,
    pub total_files: u32,
}

/// `<asset_import>{"source":…,"dest":…}</asset_import>` — the agent asks for a copy.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AssetImportTag {
    pub source: String,
    #[serde(default)]
    pub dest: Option<String>,
}

/// `<asset_register>{"rel":…,"licence":…,"provenance":…}</asset_register>` — the agent
/// declares a file it (or a tool of its) wrote under `assets/`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AssetRegisterTag {
    pub rel: String,
    #[serde(default, alias = "license")]
    pub licence: Option<String>,
    #[serde(default)]
    pub provenance: Option<String>,
}

// ── scanning ─────────────────────────────────────────────────────────────────────────

struct Scanned {
    /// `(relative path with forward slashes, absolute path, size)`, sorted by relative path.
    files: Vec<(String, PathBuf, u64)>,
    /// Sidecar text by the relative path of the file it describes.
    sidecars: BTreeMap<String, String>,
    truncated: bool,
}

fn scan_folder(root: &Path) -> Scanned {
    let mut scanned = Scanned {
        files: Vec::new(),
        sidecars: BTreeMap::new(),
        truncated: false,
    };
    walk(root, root, 0, &mut scanned);
    scanned.files.sort_by(|left, right| left.0.cmp(&right.0));
    scanned
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Scanned) {
    if depth > MAX_SCAN_DEPTH || out.truncated {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        if out.files.len() >= MAX_LIBRARY_FILES {
            out.truncated = true;
            return;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if SKIPPED.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            walk(root, &path, depth + 1, out);
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        let rel = relative_forward(root, &path);
        if name.ends_with(LICENSE_SIDECAR_SUFFIX) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let described = rel.trim_end_matches(LICENSE_SIDECAR_SUFFIX).to_owned();
                out.sidecars.insert(described, text);
            }
            continue;
        }
        if !is_asset_file(&rel) || !is_library_asset(&name) {
            continue;
        }
        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        out.files.push((rel, path, size));
    }
}

/// A library folder is a download, not a project: it carries licences, readmes, archives
/// and installers beside the assets. The project rule (`is_asset_file`) keeps those; the
/// library rule drops what no game would ever import. Never a guess about what *is* an
/// asset — only about what plainly is not.
fn is_library_asset(name: &str) -> bool {
    if LICENCE_FILES
        .iter()
        .any(|licence| licence.eq_ignore_ascii_case(name))
    {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("readme") || lower.starts_with("changelog") {
        return false;
    }
    let Some((_, ext)) = lower.rsplit_once('.') else {
        return false;
    };
    !matches!(
        ext,
        "txt"
            | "md"
            | "pdf"
            | "html"
            | "htm"
            | "url"
            | "ini"
            | "log"
            | "zip"
            | "rar"
            | "7z"
            | "exe"
            | "msi"
            | "dll"
            | "bat"
            | "ps1"
            | "sh"
            | "py"
            | "js"
            | "ts"
            | "cs"
            | "csv"
            | "xml"
            | "yml"
            | "yaml"
    )
}

/// `30 models`, `1 texture`, `4 audio files` — the agent's index counts in plain words.
fn plural(kind: ProjectAssetKind, count: usize) -> String {
    let word = match (kind, count == 1) {
        (ProjectAssetKind::Model, true) => "model",
        (ProjectAssetKind::Model, false) => "models",
        (ProjectAssetKind::Texture, true) => "texture",
        (ProjectAssetKind::Texture, false) => "textures",
        (ProjectAssetKind::Audio, true) => "audio file",
        (ProjectAssetKind::Audio, false) => "audio files",
        (ProjectAssetKind::Scene, true) => "scene",
        (ProjectAssetKind::Scene, false) => "scenes",
        (ProjectAssetKind::Material, true) => "material",
        (ProjectAssetKind::Material, false) => "materials",
        (ProjectAssetKind::Shader, true) => "shader",
        (ProjectAssetKind::Shader, false) => "shaders",
        (ProjectAssetKind::Other, true) => "other file",
        (ProjectAssetKind::Other, false) => "other files",
    };
    format!("{count} {word}")
}

fn relative_forward(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// What a `LICENSE` file at the folder root says, reduced to a short identifier the sidecar
/// can carry. Never a guess: an unrecognised text is `see LICENSE`, which the release gate
/// still treats as stated.
fn folder_licence(root: &Path) -> Option<String> {
    let path = LICENCE_FILES
        .iter()
        .map(|name| root.join(name))
        .find(|candidate| candidate.is_file())?;
    let text = std::fs::read_to_string(&path).ok()?;
    let head: String = text
        .chars()
        .take(600)
        .collect::<String>()
        .to_ascii_lowercase();
    let id = if head.contains("cc0") || head.contains("creative commons zero") {
        "CC0-1.0"
    } else if head.contains("mit license") || head.contains("permission is hereby granted") {
        "MIT"
    } else if head.contains("apache license") {
        "Apache-2.0"
    } else if head.contains("attribution 4.0") || head.contains("cc-by 4.0") {
        "CC-BY-4.0"
    } else if head.contains("attribution-sharealike") {
        "CC-BY-SA-4.0"
    } else {
        "see LICENSE"
    };
    Some(id.to_owned())
}

fn kind_counts(files: &[(String, PathBuf, u64)]) -> Vec<KindCount> {
    let mut counts: BTreeMap<u8, (ProjectAssetKind, u32)> = BTreeMap::new();
    for (rel, _, _) in files {
        let kind = asset_kind(rel);
        let slot = counts.entry(kind_rank(kind)).or_insert((kind, 0));
        slot.1 = slot.1.saturating_add(1);
    }
    counts
        .into_values()
        .map(|(kind, count)| KindCount {
            kind,
            label: kind.label().to_owned(),
            count,
        })
        .collect()
}

/// Display order for kinds: the ones a game reaches for first come first.
const fn kind_rank(kind: ProjectAssetKind) -> u8 {
    match kind {
        ProjectAssetKind::Model => 0,
        ProjectAssetKind::Texture => 1,
        ProjectAssetKind::Audio => 2,
        ProjectAssetKind::Scene => 3,
        ProjectAssetKind::Material => 4,
        ProjectAssetKind::Shader => 5,
        ProjectAssetKind::Other => 6,
    }
}

/// The `assets/<folder>` a kind files under when the caller names no destination.
const fn kind_folder(kind: ProjectAssetKind) -> &'static str {
    match kind {
        ProjectAssetKind::Model => "models",
        ProjectAssetKind::Texture => "textures",
        ProjectAssetKind::Audio => "audio",
        ProjectAssetKind::Scene => "scenes",
        ProjectAssetKind::Material => "materials",
        ProjectAssetKind::Shader => "shaders",
        ProjectAssetKind::Other => "misc",
    }
}

fn describe_folder(registered: &str) -> LibraryFolder {
    let root = PathBuf::from(registered);
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| registered.to_owned());
    if !root.is_dir() {
        return LibraryFolder {
            path: registered.to_owned(),
            name,
            exists: false,
            file_count: 0,
            counts: Vec::new(),
            truncated: false,
            licence: None,
        };
    }
    let scanned = scan_folder(&root);
    LibraryFolder {
        path: registered.to_owned(),
        name,
        exists: true,
        file_count: u32::try_from(scanned.files.len()).unwrap_or(u32::MAX),
        counts: kind_counts(&scanned.files),
        truncated: scanned.truncated,
        licence: folder_licence(&root),
    }
}

/// The pure half of `asset_library_list`: every registered folder, described.
#[must_use]
pub fn library_view(dirs: &[String]) -> AssetLibraryView {
    let folders: Vec<LibraryFolder> = dirs.iter().map(|dir| describe_folder(dir)).collect();
    let total_files = folders
        .iter()
        .fold(0u32, |sum, folder| sum.saturating_add(folder.file_count));
    AssetLibraryView {
        folders,
        total_files,
    }
}

fn assets_of(registered: &str) -> Vec<LibraryAsset> {
    let root = PathBuf::from(registered);
    if !root.is_dir() {
        return Vec::new();
    }
    let scanned = scan_folder(&root);
    let fallback = folder_licence(&root);
    scanned
        .files
        .iter()
        .map(|(rel, path, size)| {
            let kind = asset_kind(rel);
            let licence = scanned
                .sidecars
                .get(rel)
                .and_then(|text| licence_from_meta(text))
                .or_else(|| fallback.clone());
            LibraryAsset {
                path: path.to_string_lossy().into_owned(),
                folder: registered.to_owned(),
                rel: rel.clone(),
                name: rel.rsplit('/').next().unwrap_or(rel).to_owned(),
                kind,
                kind_label: kind.label().to_owned(),
                size_bytes: *size,
                licence,
            }
        })
        .collect()
}

/// The pure half of `asset_library_search`: every folder, filtered, capped at `limit`.
#[must_use]
pub fn search(
    dirs: &[String],
    query: Option<&str>,
    kind: Option<ProjectAssetKind>,
    limit: usize,
) -> Vec<LibraryAsset> {
    let needle = query
        .map(|q| q.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let mut out = Vec::new();
    for dir in dirs {
        for asset in assets_of(dir) {
            if kind.is_some_and(|wanted| wanted != asset.kind) {
                continue;
            }
            if !needle.is_empty()
                && !asset.rel.to_ascii_lowercase().contains(&needle)
                && !asset.kind_label.to_ascii_lowercase().contains(&needle)
            {
                continue;
            }
            out.push(asset);
            if out.len() >= limit {
                return out;
            }
        }
    }
    out
}

// ── the agent's view ─────────────────────────────────────────────────────────────────

/// The block the engine context carries when folders are registered (SPA-102): every
/// folder with its counts and licence, then a handful of example paths per kind. The
/// agent imports by naming one of these paths verbatim; anything else is refused.
#[must_use]
pub fn library_context(dirs: &[String]) -> String {
    if dirs.is_empty() {
        return String::new();
    }
    let mut out = String::from("## Asset library (the user's folders)\n\n");
    let mut any = false;
    for dir in dirs {
        let folder = describe_folder(dir);
        if !folder.exists {
            out.push_str(&format!(
                "- `{}` — folder is missing right now.\n",
                folder.path
            ));
            continue;
        }
        any = true;
        let counts = folder
            .counts
            .iter()
            .map(|count| {
                plural(
                    count.kind,
                    usize::try_from(count.count).unwrap_or(usize::MAX),
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "- `{}` — {} files ({}); licence {}{}\n",
            folder.path,
            folder.file_count,
            if counts.is_empty() {
                "nothing recognised".to_owned()
            } else {
                counts
            },
            folder.licence.as_deref().unwrap_or("not stated"),
            if folder.truncated {
                "; listing capped"
            } else {
                ""
            }
        ));
        let assets = assets_of(dir);
        let mut by_kind: BTreeMap<u8, Vec<&LibraryAsset>> = BTreeMap::new();
        for asset in &assets {
            by_kind
                .entry(kind_rank(asset.kind))
                .or_default()
                .push(asset);
        }
        for group in by_kind.values() {
            let Some(first) = group.first() else {
                continue;
            };
            let shown: Vec<String> = group
                .iter()
                .take(INDEX_EXAMPLES_PER_KIND)
                .map(|asset| asset.rel.clone())
                .collect();
            let more = group.len().saturating_sub(shown.len());
            out.push_str(&format!(
                "  {}: {}{}\n",
                plural(first.kind, group.len()),
                shown.join(", "),
                if more > 0 {
                    format!(" … and {more} more (ask the user to search)")
                } else {
                    String::new()
                }
            ));
        }
    }
    if !any {
        out.push_str("\nNo registered folder is reachable; build or generate instead.\n");
    }
    out
}

// ── importing ────────────────────────────────────────────────────────────────────────

fn canonical(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// True when `source` sits under one of the registered folders. Both sides canonical, so
/// `..`, symlinks and case cannot smuggle a path in from outside.
#[must_use]
pub fn is_under_library(dirs: &[String], source: &Path) -> bool {
    let Some(source) = canonical(source) else {
        return false;
    };
    dirs.iter()
        .filter_map(|dir| canonical(Path::new(dir)))
        .any(|root| source.starts_with(&root))
}

/// A project-relative destination under `assets/`, or the reason it is refused.
fn sanitize_dest(dest: &str) -> Result<PathBuf, String> {
    let trimmed = dest.trim().trim_matches('"').replace('\\', "/");
    let trimmed = trimmed.strip_prefix("res://").unwrap_or(&trimmed);
    let mut safe = PathBuf::new();
    for segment in trimmed.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return Err(format!("`{dest}` climbs out of the project.")),
            other if other.contains(':') || other.chars().any(char::is_control) => {
                return Err(format!("`{dest}` is not a project-relative path."));
            }
            other => safe.push(other),
        }
    }
    let first = safe
        .components()
        .next()
        .map(|component| component.as_os_str().to_string_lossy().into_owned());
    if first.as_deref() != Some(ASSETS_DIR) || safe.components().count() < 2 {
        return Err(format!(
            "`{dest}` must be a file under `{ASSETS_DIR}/` (for example `{ASSETS_DIR}/models/crate.glb`)."
        ));
    }
    Ok(safe)
}

/// A free file name beside `target`: `crate.glb`, then `crate-2.glb`, `crate-3.glb`…
fn unique_target(target: &Path) -> PathBuf {
    if !target.exists() {
        return target.to_path_buf();
    }
    let stem = target
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "asset".to_owned());
    let ext = target
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    let parent = target.parent().map(Path::to_path_buf).unwrap_or_default();
    (2..1000)
        .map(|n| parent.join(format!("{stem}-{n}{ext}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| parent.join(format!("{stem}-{}{ext}", chrono::Utc::now().timestamp())))
}

fn sidecar_json(licence: Option<&str>, provenance: serde_json::Value) -> String {
    let body = serde_json::json!({
        "license": licence.unwrap_or("unknown"),
        "provenance": provenance,
        "imported_at": chrono::Utc::now().to_rfc3339(),
    });
    serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".to_owned())
}

fn asset_row(rel: &Path, size_bytes: u64, licence: Option<String>) -> ProjectAsset {
    let rel_text = rel.to_string_lossy().replace('\\', "/");
    let name = rel
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel_text.clone());
    let folder = rel
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .filter(|folder| !folder.is_empty())
        .unwrap_or_else(|| ASSETS_DIR.to_owned());
    let kind = asset_kind(&rel_text);
    ProjectAsset {
        rel: rel_text,
        name,
        folder,
        kind,
        kind_label: kind.label().to_owned(),
        size_bytes,
        licence,
        provenance: Some("user".to_owned()),
    }
}

/// Copies one library file into the project, with a sidecar. The source must sit under a
/// registered folder; the destination must sit under `assets/`. Never overwrites: a name
/// that is taken gets a numbered sibling, and the reply names the path that was actually
/// written.
pub fn import_file(
    project_root: &Path,
    dirs: &[String],
    source: &str,
    dest: Option<&str>,
) -> Result<ProjectAsset, String> {
    let source_path = PathBuf::from(source.trim().trim_matches('"'));
    if !source_path.is_file() {
        return Err(format!("`{source}` is not a file."));
    }
    if !is_under_library(dirs, &source_path) {
        return Err(format!(
            "`{source}` is not inside a registered library folder; only paths Bhippi listed can be imported."
        ));
    }
    let name = source_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| format!("`{source}` has no file name."))?;
    let rel = match dest {
        Some(dest) => sanitize_dest(dest)?,
        None => PathBuf::from(ASSETS_DIR)
            .join(kind_folder(asset_kind(&name)))
            .join(&name),
    };
    let target = unique_target(&project_root.join(&rel));
    let rel = target
        .strip_prefix(project_root)
        .map(Path::to_path_buf)
        .unwrap_or(rel);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create `{}`: {error}", parent.display()))?;
    }
    std::fs::copy(&source_path, &target)
        .map_err(|error| format!("could not copy `{source}`: {error}"))?;
    let size_bytes = std::fs::metadata(&target)
        .map(|meta| meta.len())
        .unwrap_or(0);

    // The licence travels: a sibling sidecar first, else the folder's LICENSE file.
    let sibling = format!("{}{LICENSE_SIDECAR_SUFFIX}", source_path.display());
    let licence = std::fs::read_to_string(&sibling)
        .ok()
        .and_then(|text| licence_from_meta(&text))
        .or_else(|| {
            dirs.iter()
                .filter_map(|dir| canonical(Path::new(dir)))
                .find(|root| canonical(&source_path).is_some_and(|s| s.starts_with(root)))
                .and_then(|root| folder_licence(&root))
        });
    let library_root = dirs
        .iter()
        .find(|dir| {
            canonical(Path::new(dir))
                .zip(canonical(&source_path))
                .is_some_and(|(root, s)| s.starts_with(&root))
        })
        .cloned()
        .unwrap_or_default();
    let sidecar = sidecar_json(
        licence.as_deref(),
        serde_json::json!({
            "source": "user_library",
            "library": library_root,
            "origin": source_path.to_string_lossy(),
        }),
    );
    let sidecar_path = PathBuf::from(format!("{}{LICENSE_SIDECAR_SUFFIX}", target.display()));
    std::fs::write(&sidecar_path, sidecar)
        .map_err(|error| format!("copied `{name}` but could not write its sidecar: {error}"))?;
    Ok(asset_row(&rel, size_bytes, licence))
}

/// Writes the sidecar for a file the agent (or a tool of its) put under `assets/` (SPA-203).
/// The file must already exist; the path must be under `assets/`; nothing else is touched.
pub fn register_sidecar(
    project_root: &Path,
    tag: &AssetRegisterTag,
) -> Result<ProjectAsset, String> {
    let rel = sanitize_dest(&tag.rel)?;
    let target = project_root.join(&rel);
    if !target.is_file() {
        return Err(format!(
            "`{}` does not exist yet; write the file first, then register it.",
            rel.to_string_lossy().replace('\\', "/")
        ));
    }
    let licence = tag
        .licence
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let provenance = tag
        .provenance
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("procedural");
    let sidecar = sidecar_json(
        licence.as_deref(),
        serde_json::json!({ "source": provenance, "registered_by": "agent" }),
    );
    let sidecar_path = PathBuf::from(format!("{}{LICENSE_SIDECAR_SUFFIX}", target.display()));
    std::fs::write(&sidecar_path, sidecar)
        .map_err(|error| format!("could not write the sidecar: {error}"))?;
    let size_bytes = std::fs::metadata(&target)
        .map(|meta| meta.len())
        .unwrap_or(0);
    let mut row = asset_row(&rel, size_bytes, licence);
    row.provenance = Some(crate::studio_dock::provenance_from_meta(&format!(
        "{{\"provenance\":\"{provenance}\"}}"
    )));
    Ok(row)
}

// ── tags ─────────────────────────────────────────────────────────────────────────────

fn extract_tagged<T: serde::de::DeserializeOwned>(text: &str, tag: &str) -> Vec<T> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find(&open) {
        let body_start = cursor + start + open.len();
        let Some(end) = text[body_start..].find(&close) else {
            break;
        };
        let body = text[body_start..body_start + end].trim();
        if let Ok(parsed) = serde_json::from_str::<T>(body) {
            out.push(parsed);
        }
        cursor = body_start + end + close.len();
    }
    out
}

fn strip_tagged(text: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut clean = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find(&open) {
        let absolute = cursor + start;
        clean.push_str(&text[cursor..absolute]);
        let body_start = absolute + open.len();
        let Some(end) = text[body_start..].find(&close) else {
            return clean;
        };
        cursor = body_start + end + close.len();
    }
    clean.push_str(&text[cursor..]);
    clean
}

#[must_use]
pub fn extract_asset_import_tags(text: &str) -> Vec<AssetImportTag> {
    extract_tagged(text, "asset_import")
}

#[must_use]
pub fn extract_asset_register_tags(text: &str) -> Vec<AssetRegisterTag> {
    extract_tagged(text, "asset_register")
}

/// The visible answer with both asset tags removed — protocol, not prose.
#[must_use]
pub fn strip_asset_tags(text: &str) -> String {
    strip_tagged(&strip_tagged(text, "asset_import"), "asset_register")
}

#[must_use]
pub fn has_asset_tags(text: &str) -> bool {
    text.contains("<asset_import>") || text.contains("<asset_register>")
}

// ── commands ─────────────────────────────────────────────────────────────────────────

async fn dirs_of(state: &crate::Runtime) -> Result<Vec<String>, AppError> {
    let config = state.config.load().await.map_err(AppError::from)?;
    Ok(config.assets.library_dirs)
}

async fn view_for(dirs: Vec<String>) -> Result<AssetLibraryView, AppError> {
    tokio::task::spawn_blocking(move || library_view(&dirs))
        .await
        .map_err(|error| AppError {
            message: format!("The library scan did not finish: {error}"),
            hint: Some("Open the Assets screen again to retry.".to_owned()),
        })
}

/// Every registered folder, described.
#[tauri::command]
#[specta::specta]
pub async fn asset_library_list(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<AssetLibraryView, AppError> {
    let dirs = dirs_of(&state).await?;
    view_for(dirs).await
}

/// Registers a folder. The user's pick in the native dialog **is** the permission.
#[tauri::command]
#[specta::specta]
pub async fn asset_library_add(
    state: tauri::State<'_, crate::Runtime>,
    path: String,
) -> Result<AssetLibraryView, AppError> {
    let canonical_dir = crate::workspace::canonical_directory(&path)?;
    let display = crate::workspace::display_path(&canonical_dir);
    let mut config = state.config.load().await.map_err(AppError::from)?;
    if !config
        .assets
        .library_dirs
        .iter()
        .any(|dir| crate::workspace::paths_match(dir, &display))
    {
        config.assets.library_dirs.push(display);
    }
    state.config.save(&config).await.map_err(AppError::from)?;
    view_for(config.assets.library_dirs).await
}

/// Forgets a folder. The folder itself is untouched — the library never writes to it.
#[tauri::command]
#[specta::specta]
pub async fn asset_library_remove(
    state: tauri::State<'_, crate::Runtime>,
    path: String,
) -> Result<AssetLibraryView, AppError> {
    let mut config = state.config.load().await.map_err(AppError::from)?;
    config
        .assets
        .library_dirs
        .retain(|dir| !crate::workspace::paths_match(dir, path.trim()));
    state.config.save(&config).await.map_err(AppError::from)?;
    view_for(config.assets.library_dirs).await
}

/// Files across every folder, filtered by text and kind, capped.
#[tauri::command]
#[specta::specta]
pub async fn asset_library_search(
    state: tauri::State<'_, crate::Runtime>,
    query: Option<String>,
    kind: Option<ProjectAssetKind>,
    limit: Option<u32>,
) -> Result<Vec<LibraryAsset>, AppError> {
    let dirs = dirs_of(&state).await?;
    let limit = usize::try_from(limit.unwrap_or(200))
        .unwrap_or(200)
        .clamp(1, 2_000);
    tokio::task::spawn_blocking(move || search(&dirs, query.as_deref(), kind, limit))
        .await
        .map_err(|error| AppError {
            message: format!("The library search did not finish: {error}"),
            hint: None,
        })
}

/// Copies one library file into the open project's `assets/`, with its sidecar.
#[tauri::command]
#[specta::specta]
pub async fn asset_library_import(
    state: tauri::State<'_, crate::Runtime>,
    project: String,
    source: String,
    dest: Option<String>,
) -> Result<ProjectAsset, AppError> {
    let root = crate::godot_commands::resolve_project(&state, &project).await?;
    let dirs = dirs_of(&state).await?;
    tokio::task::spawn_blocking(move || import_file(&root, &dirs, &source, dest.as_deref()))
        .await
        .map_err(|error| AppError {
            message: format!("The import did not finish: {error}"),
            hint: None,
        })?
        .map_err(|message| AppError {
            message,
            hint: Some("Add the folder under Assets › Library folders first.".to_owned()),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bhippi-asset-library-{name}-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("temp dir: {error}"));
        dir
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|error| panic!("mkdir: {error}"));
        }
        std::fs::write(path, bytes).unwrap_or_else(|error| panic!("write: {error}"));
    }

    #[test]
    fn a_folder_is_described_by_kind_and_its_licence_file() {
        let lib = temp_dir("describe");
        write(&lib.join("props/crate.glb"), b"glb");
        write(
            &lib.join("props/crate.glb.meta.json"),
            br#"{"license":"CC-BY-4.0"}"#,
        );
        write(&lib.join("tex/wood.png"), b"png");
        write(&lib.join("sfx/jump.wav"), b"wav");
        write(&lib.join("README.md"), b"not an asset");
        write(&lib.join("LICENSE"), b"CC0 1.0 Universal");
        let view = library_view(&[lib.to_string_lossy().into_owned()]);
        let folder = &view.folders[0];
        assert!(folder.exists);
        assert_eq!(folder.file_count, 3, "the README is not an asset");
        assert_eq!(folder.licence.as_deref(), Some("CC0-1.0"));
        let labels: Vec<(String, u32)> = folder
            .counts
            .iter()
            .map(|count| (count.label.clone(), count.count))
            .collect();
        assert_eq!(
            labels,
            vec![
                ("Model".to_owned(), 1),
                ("Texture".to_owned(), 1),
                ("Audio".to_owned(), 1)
            ]
        );
        let hits = search(
            &[lib.to_string_lossy().into_owned()],
            Some("crate"),
            None,
            10,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].licence.as_deref(),
            Some("CC-BY-4.0"),
            "the sidecar wins"
        );
        let wood = search(
            &[lib.to_string_lossy().into_owned()],
            None,
            Some(ProjectAssetKind::Texture),
            10,
        );
        assert_eq!(
            wood[0].licence.as_deref(),
            Some("CC0-1.0"),
            "else the folder's LICENSE"
        );
    }

    #[test]
    fn a_missing_folder_stays_listed_but_says_so() {
        let view = library_view(&["C:/definitely/not/here".to_owned()]);
        assert!(!view.folders[0].exists);
        assert_eq!(view.total_files, 0);
        assert!(library_context(&["C:/definitely/not/here".to_owned()]).contains("missing"));
    }

    #[test]
    fn importing_copies_with_a_sidecar_and_refuses_paths_outside_the_library() {
        let lib = temp_dir("lib");
        let project = temp_dir("project");
        write(&lib.join("props/barrel.glb"), b"barrel");
        write(
            &lib.join("LICENSE"),
            b"MIT License\n\nPermission is hereby granted",
        );
        let elsewhere = temp_dir("elsewhere");
        write(&elsewhere.join("secret.glb"), b"no");
        let dirs = vec![lib.to_string_lossy().into_owned()];

        let imported = import_file(
            &project,
            &dirs,
            &lib.join("props/barrel.glb").to_string_lossy(),
            None,
        )
        .unwrap_or_else(|error| panic!("import: {error}"));
        assert_eq!(imported.rel, "assets/models/barrel.glb");
        assert_eq!(imported.licence.as_deref(), Some("MIT"));
        assert_eq!(imported.provenance.as_deref(), Some("user"));
        let sidecar = std::fs::read_to_string(project.join("assets/models/barrel.glb.meta.json"))
            .unwrap_or_else(|error| panic!("sidecar: {error}"));
        assert!(sidecar.contains("\"user_library\""));
        assert!(sidecar.contains("\"MIT\""));

        // A second import of the same name does not overwrite the first.
        let again = import_file(
            &project,
            &dirs,
            &lib.join("props/barrel.glb").to_string_lossy(),
            None,
        )
        .unwrap_or_else(|error| panic!("import: {error}"));
        assert_eq!(again.rel, "assets/models/barrel-2.glb");

        let refused = import_file(
            &project,
            &dirs,
            &elsewhere.join("secret.glb").to_string_lossy(),
            None,
        );
        assert!(refused.is_err(), "a path outside the library is refused");
        let climbed = import_file(
            &project,
            &dirs,
            &lib.join("props/barrel.glb").to_string_lossy(),
            Some("assets/../project.godot"),
        );
        assert!(climbed.is_err(), "a destination cannot leave assets/");
        let outside = import_file(
            &project,
            &dirs,
            &lib.join("props/barrel.glb").to_string_lossy(),
            Some("scenes/barrel.glb"),
        );
        assert!(outside.is_err(), "a destination must be under assets/");
    }

    #[test]
    fn registering_writes_the_sidecar_for_a_file_that_exists_under_assets() {
        let project = temp_dir("register");
        write(&project.join("assets/models/lamp.glb"), b"lamp");
        let row = register_sidecar(
            &project,
            &AssetRegisterTag {
                rel: "res://assets/models/lamp.glb".to_owned(),
                licence: Some("project".to_owned()),
                provenance: Some("procedural".to_owned()),
            },
        )
        .unwrap_or_else(|error| panic!("register: {error}"));
        assert_eq!(row.rel, "assets/models/lamp.glb");
        assert_eq!(row.licence.as_deref(), Some("project"));
        assert_eq!(row.provenance.as_deref(), Some("procedural"));
        let missing = register_sidecar(
            &project,
            &AssetRegisterTag {
                rel: "assets/models/ghost.glb".to_owned(),
                licence: None,
                provenance: None,
            },
        );
        assert!(
            missing.is_err(),
            "a file that does not exist cannot be registered"
        );
    }

    #[test]
    fn tags_parse_and_strip() {
        let text = "Done.\n<asset_import>{\"source\":\"C:\\\\lib\\\\a.glb\",\"dest\":\"assets/models/a.glb\"}</asset_import>\nand\n<asset_register>{\"rel\":\"assets/models/b.glb\",\"license\":\"project\"}</asset_register>";
        let imports = extract_asset_import_tags(text);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].dest.as_deref(), Some("assets/models/a.glb"));
        let registers = extract_asset_register_tags(text);
        assert_eq!(
            registers[0].licence.as_deref(),
            Some("project"),
            "`license` is accepted too"
        );
        assert_eq!(strip_asset_tags(text).trim(), "Done.\n\nand");
        assert!(has_asset_tags(text));
        assert!(!has_asset_tags("plain prose"));
    }

    #[test]
    fn the_agent_index_names_paths_verbatim_and_caps_them() {
        let lib = temp_dir("index");
        for n in 0..30 {
            write(&lib.join(format!("m/prop{n:02}.glb")), b"x");
        }
        let context = library_context(&[lib.to_string_lossy().into_owned()]);
        assert!(context.contains("m/prop00.glb"));
        assert!(context.contains("and 6 more"), "{context}");
        assert!(context.contains("30 models"));
    }
}
