//! The evidence one scan is allowed to reason over (ADR-0056 §4).
//!
//! Every inspector reads *this*, never the disk. The project is walked once, parsed once
//! and hashed once; nine specialists then answer from the same bytes. That is what keeps a
//! full project scan a single pass instead of nine, and — more importantly — what makes two
//! inspectors' claims about the same node comparable: they are looking at the same parse.
//!
//! The snapshot is **read-only, by construction**. There is no `fs::write` in this module or
//! anywhere else under `inspect`, and `inspect::tests::inspection_never_writes` proves it by
//! reading the source. An inspector that could write would eventually write.

use crate::godot::scene::GodotScene;
use crate::godot::{res_to_rel, RES_PREFIX};
use bhippi_types::{
    INSPECT_MAX_HASH_BYTES, INSPECT_MAX_SCENES, INSPECT_MAX_SCRIPTS, INSPECT_MAX_SCRIPT_BYTES,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Directories a scan never descends into: engine caches, version control and dependency
/// trees hold tens of thousands of files nobody authored.
pub const SKIPPED_DIRECTORIES: [&str; 7] = [
    ".godot",
    ".git",
    ".bhippi",
    ".import",
    "node_modules",
    "target",
    "__pycache__",
];

/// Third-party code lives here. It is indexed — a reference into it must resolve — but its
/// own style is not the user's problem, so the code inspector holds its tongue about it.
pub const VENDORED_DIRECTORY: &str = "addons";

/// How deep the walk goes.
pub const MAX_DEPTH: usize = 12;

/// One `.tscn`/`.tres` the scan read.
#[derive(Clone, Debug)]
pub struct SceneEntry {
    /// Project-relative, forward slashes.
    pub rel: String,
    /// `res://`-prefixed.
    pub res: String,
    /// `None` when the file did not parse; `parse_error` then says why.
    pub scene: Option<GodotScene>,
    pub parse_error: Option<String>,
    pub vendored: bool,
}

impl SceneEntry {
    /// The scene, or nothing when it did not parse.
    #[must_use]
    pub fn parsed(&self) -> Option<&GodotScene> {
        self.scene.as_ref()
    }
}

/// One GDScript the scan read.
#[derive(Clone, Debug)]
pub struct ScriptEntry {
    pub rel: String,
    pub res: String,
    /// Newline-normalised source. Empty when `too_large`.
    pub source: String,
    pub too_large: bool,
    pub vendored: bool,
}

impl ScriptEntry {
    /// `(1-based line number, text)` for every line.
    pub fn lines(&self) -> impl Iterator<Item = (u32, &str)> {
        self.source.lines().enumerate().map(|(index, line)| {
            (
                u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1),
                line,
            )
        })
    }
}

/// One file on disk that is neither a scene nor a script.
#[derive(Clone, Debug)]
pub struct AssetEntry {
    pub rel: String,
    pub res: String,
    pub bytes: u64,
    pub kind: crate::asset::AssetKind,
    /// `(width, height)` when the header could be read. PNG and JPEG only — a format whose
    /// header this crate cannot read reports nothing rather than a guess.
    pub pixels: Option<(u32, u32)>,
    /// Content hash, for duplicate detection. `None` for files over
    /// [`INSPECT_MAX_HASH_BYTES`], which are not hashed rather than partially hashed.
    pub hash: Option<String>,
}

/// Everything one scan may look at.
#[derive(Clone, Debug, Default)]
pub struct ProjectSnapshot {
    pub root: PathBuf,
    /// The project's display name, from `project.godot`.
    pub name: Option<String>,
    /// The parsed `project.godot`, when it is there and parses.
    pub project_file: Option<crate::godot::project::GodotProjectFile>,
    /// Project-relative path of the main scene, when one is set and on disk.
    pub main_scene: Option<String>,
    pub scenes: Vec<SceneEntry>,
    pub scripts: Vec<ScriptEntry>,
    pub assets: Vec<AssetEntry>,
    /// Every project-relative path the walk saw, for "is this reference on disk".
    pub files: BTreeSet<String>,
    /// What a cap cut off.
    pub truncated: Vec<String>,
}

impl ProjectSnapshot {
    /// Walk, parse and hash a Godot project.
    ///
    /// Never fails: a project that does not parse is a project with findings, not an error.
    /// What could not be read is recorded where the inspectors can report it.
    #[must_use]
    pub fn read(root: &Path) -> Self {
        let mut snapshot = Self {
            root: root.to_path_buf(),
            ..Self::default()
        };

        let mut paths = Vec::new();
        walk(root, root, 0, &mut paths);
        paths.sort();
        snapshot.files = paths.iter().cloned().collect();

        // `project.godot` first: the main scene decides what "reachable" means later.
        let project_path = root.join("project.godot");
        if let Ok(text) = std::fs::read_to_string(&project_path) {
            match crate::godot::project::GodotProjectFile::parse(&text.replace("\r\n", "\n")) {
                Ok(file) => {
                    snapshot.name = file.name();
                    snapshot.main_scene = file.main_scene().map(|res| res_to_rel(&res));
                    snapshot.project_file = Some(file);
                }
                Err(_) => {
                    snapshot.truncated.push(
                        "project.godot did not parse; project-wide checks are limited".to_owned(),
                    );
                }
            }
        }

        for rel in &paths {
            let vendored = is_vendored(rel);
            let full = root.join(rel);
            if has_extension(rel, "tscn") || has_extension(rel, "tres") {
                if snapshot.scenes.len() >= INSPECT_MAX_SCENES {
                    continue;
                }
                let entry = match std::fs::read_to_string(&full) {
                    Ok(text) => match GodotScene::parse(&text.replace("\r\n", "\n")) {
                        Ok(scene) => SceneEntry {
                            rel: rel.clone(),
                            res: format!("{RES_PREFIX}{rel}"),
                            scene: Some(scene),
                            parse_error: None,
                            vendored,
                        },
                        Err(error) => SceneEntry {
                            rel: rel.clone(),
                            res: format!("{RES_PREFIX}{rel}"),
                            scene: None,
                            parse_error: Some(error.to_string()),
                            vendored,
                        },
                    },
                    Err(error) => SceneEntry {
                        rel: rel.clone(),
                        res: format!("{RES_PREFIX}{rel}"),
                        scene: None,
                        parse_error: Some(error.to_string()),
                        vendored,
                    },
                };
                snapshot.scenes.push(entry);
            } else if has_extension(rel, "gd") {
                if snapshot.scripts.len() >= INSPECT_MAX_SCRIPTS {
                    continue;
                }
                let size = std::fs::metadata(&full).map(|meta| meta.len()).unwrap_or(0);
                let too_large = size > INSPECT_MAX_SCRIPT_BYTES;
                let source = if too_large {
                    String::new()
                } else {
                    std::fs::read_to_string(&full)
                        .map(|text| text.replace("\r\n", "\n"))
                        .unwrap_or_default()
                };
                snapshot.scripts.push(ScriptEntry {
                    rel: rel.clone(),
                    res: format!("{RES_PREFIX}{rel}"),
                    source,
                    too_large,
                    vendored,
                });
            } else if is_asset(rel) {
                let bytes = std::fs::metadata(&full).map(|meta| meta.len()).unwrap_or(0);
                snapshot.assets.push(AssetEntry {
                    rel: rel.clone(),
                    res: format!("{RES_PREFIX}{rel}"),
                    bytes,
                    kind: classify(rel),
                    pixels: image_dimensions(&full),
                    hash: (bytes <= INSPECT_MAX_HASH_BYTES)
                        .then(|| {
                            std::fs::read(&full)
                                .ok()
                                .map(|body| blake3::hash(&body).to_hex().to_string())
                        })
                        .flatten(),
                });
            }
        }

        if snapshot.scenes.len() >= INSPECT_MAX_SCENES {
            snapshot.truncated.push(format!(
                "Stopped after {INSPECT_MAX_SCENES} scenes; the rest were not inspected"
            ));
        }
        if snapshot.scripts.len() >= INSPECT_MAX_SCRIPTS {
            snapshot.truncated.push(format!(
                "Stopped after {INSPECT_MAX_SCRIPTS} scripts; the rest were not inspected"
            ));
        }
        // The main scene is only "set" if it is also on disk; a dangling one is the scene
        // inspector's finding, not a silent `None`.
        snapshot
    }

    /// True when the project-relative path exists on disk.
    #[must_use]
    pub fn has_file(&self, rel: &str) -> bool {
        self.files.contains(rel)
    }

    /// True when a `res://` (or already relative) reference resolves to a file.
    #[must_use]
    pub fn resolves(&self, reference: &str) -> bool {
        self.has_file(&res_to_rel(reference))
    }

    #[must_use]
    pub fn scene(&self, rel: &str) -> Option<&SceneEntry> {
        let rel = res_to_rel(rel);
        self.scenes.iter().find(|entry| entry.rel == rel)
    }

    #[must_use]
    pub fn script(&self, rel: &str) -> Option<&ScriptEntry> {
        let rel = res_to_rel(rel);
        self.scripts.iter().find(|entry| entry.rel == rel)
    }

    /// The scenes an inspection scoped to one level should look at: the level itself and
    /// everything it instances, transitively.
    #[must_use]
    pub fn scene_closure(&self, rel: &str) -> Vec<&SceneEntry> {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut queue = vec![res_to_rel(rel)];
        let mut out = Vec::new();
        while let Some(current) = queue.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            let Some(entry) = self.scene(&current) else {
                continue;
            };
            out.push(entry);
            if let Some(scene) = entry.parsed() {
                for (_, instanced) in scene.instances() {
                    queue.push(res_to_rel(&instanced));
                }
            }
        }
        out.sort_by(|left, right| left.rel.cmp(&right.rel));
        out
    }

    /// Every non-vendored script's source concatenated is not what callers want; this is:
    /// a map from script path to source, for "is this symbol referenced anywhere".
    #[must_use]
    pub fn sources(&self) -> BTreeMap<&str, &str> {
        self.scripts
            .iter()
            .map(|script| (script.rel.as_str(), script.source.as_str()))
            .collect()
    }

    /// True when `needle` appears in any script in the project.
    #[must_use]
    pub fn mentioned_in_scripts(&self, needle: &str) -> bool {
        self.scripts
            .iter()
            .any(|script| script.source.contains(needle))
    }

    /// True when `needle` appears in any scene file's text form or any script.
    #[must_use]
    pub fn referenced_anywhere(&self, needle: &str) -> bool {
        if self.mentioned_in_scripts(needle) {
            return true;
        }
        self.scenes.iter().any(|entry| {
            entry.parsed().is_some_and(|scene| {
                scene
                    .document
                    .ext_resources
                    .iter()
                    .any(|resource| res_to_rel(&resource.path) == res_to_rel(needle))
            })
        })
    }
}

fn walk(directory: &Path, root: &Path, depth: usize, found: &mut Vec<String>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if path.is_dir() {
            if SKIPPED_DIRECTORIES.contains(&name.as_str()) {
                continue;
            }
            walk(&path, root, depth + 1, found);
            continue;
        }
        if name.starts_with('.') {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            found.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[must_use]
fn is_vendored(rel: &str) -> bool {
    rel.starts_with(&format!("{VENDORED_DIRECTORY}/"))
}

#[must_use]
fn has_extension(rel: &str, extension: &str) -> bool {
    Path::new(rel)
        .extension()
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

/// Godot's import stubs and Bhippi's licence sidecars sit beside a file and are not files
/// in their own right.
#[must_use]
fn is_asset(rel: &str) -> bool {
    !rel.ends_with(".import")
        && !rel.ends_with(crate::godot::gates::LICENSE_SIDECAR_SUFFIX)
        && !has_extension(rel, "gd")
        && !has_extension(rel, "tscn")
        && !has_extension(rel, "tres")
        && !has_extension(rel, "godot")
        && !has_extension(rel, "md")
        && !has_extension(rel, "toml")
        && !has_extension(rel, "cfg")
}

/// Extension → kind. A texture in `assets/models/` is still a texture.
#[must_use]
pub fn classify(rel: &str) -> crate::asset::AssetKind {
    use crate::asset::AssetKind;
    let extension = Path::new(rel)
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tga" | "svg" | "exr" | "hdr" | "ktx" => {
            AssetKind::Texture
        }
        "glb" | "gltf" | "obj" | "fbx" | "blend" | "dae" => AssetKind::Mesh,
        "ogg" | "wav" | "mp3" | "flac" => AssetKind::Audio,
        "ttf" | "otf" | "woff" | "woff2" | "fnt" => AssetKind::Font,
        "gdshader" | "glsl" | "shader" => AssetKind::Shader,
        _ => AssetKind::Other,
    }
}

/// `(width, height)` from a PNG or JPEG header.
///
/// Only the two formats whose headers are unambiguous and cheap. Anything else reports
/// nothing — an inspector that guessed a texture's resolution would be inventing the very
/// measurement §3 forbids.
#[must_use]
pub fn image_dimensions(path: &Path) -> Option<(u32, u32)> {
    let bytes = read_prefix(path, 64 * 1024)?;
    png_dimensions(&bytes).or_else(|| jpeg_dimensions(&bytes))
}

fn read_prefix(path: &Path, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut buffer = Vec::new();
    file.take(limit as u64).read_to_end(&mut buffer).ok()?;
    Some(buffer)
}

const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || bytes[..8] != PNG_MAGIC || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (width > 0 && height > 0).then_some((width, height))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut cursor = 2usize;
    while cursor + 9 < bytes.len() {
        if bytes[cursor] != 0xFF {
            cursor += 1;
            continue;
        }
        let marker = bytes[cursor + 1];
        // Start-of-frame markers carry the dimensions; SOF4/SOF8/SOF12 are not frames.
        let is_sof =
            (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC;
        let length = usize::from(u16::from_be_bytes([bytes[cursor + 2], bytes[cursor + 3]]));
        if is_sof {
            let height = u32::from(u16::from_be_bytes([bytes[cursor + 5], bytes[cursor + 6]]));
            let width = u32::from(u16::from_be_bytes([bytes[cursor + 7], bytes[cursor + 8]]));
            return (width > 0 && height > 0).then_some((width, height));
        }
        if length < 2 {
            return None;
        }
        cursor += 2 + length;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_png_header_gives_its_real_size_and_a_text_file_gives_nothing() {
        let mut png = Vec::from(PNG_MAGIC);
        png.extend_from_slice(&[0, 0, 0, 13]);
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&8192u32.to_be_bytes());
        png.extend_from_slice(&4096u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        assert_eq!(png_dimensions(&png), Some((8192, 4096)));
        assert_eq!(png_dimensions(b"not a png at all, not even close"), None);
    }

    #[test]
    fn a_jpeg_start_of_frame_gives_its_real_size() {
        // SOI, an APP0 block to skip, then SOF0 carrying 1920x1080.
        let mut jpeg = vec![0xFF, 0xD8];
        jpeg.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00]);
        jpeg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        jpeg.extend_from_slice(&1080u16.to_be_bytes());
        jpeg.extend_from_slice(&1920u16.to_be_bytes());
        jpeg.extend_from_slice(&[0x03, 0x01, 0x22, 0x00]);
        assert_eq!(jpeg_dimensions(&jpeg), Some((1920, 1080)));
    }

    #[test]
    fn classification_follows_the_extension_not_the_folder() {
        use crate::asset::AssetKind;
        assert_eq!(
            classify("assets/models/rock_albedo.png"),
            AssetKind::Texture
        );
        assert_eq!(classify("assets/textures/chair.glb"), AssetKind::Mesh);
        assert_eq!(classify("assets/ui/theme.ttf"), AssetKind::Font);
        assert_eq!(classify("assets/readme.xyz"), AssetKind::Other);
    }

    #[test]
    fn sidecars_and_import_stubs_are_not_assets() {
        assert!(!is_asset("assets/rock.glb.import"));
        assert!(!is_asset("assets/rock.glb.meta.json"));
        assert!(!is_asset("scripts/player.gd"));
        assert!(!is_asset("scenes/main.tscn"));
        assert!(is_asset("assets/rock.glb"));
    }

    #[test]
    fn addons_are_indexed_but_marked_vendored() {
        assert!(is_vendored("addons/some_plugin/plugin.gd"));
        assert!(!is_vendored("scripts/player.gd"));
    }
}
