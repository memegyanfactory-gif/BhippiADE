//! The external asset library bridge: Epic's Fab vault, read for what Godot can use.
//!
//! A Fab vault is an *Unreal* cache, and Bhippi's engine is Godot. Most of what is in one
//! is therefore either directly usable, usable after a conversion the user must install, or
//! not usable at all — and the difference matters more than the file count. A pack whose
//! whole payload is `.uasset` cannot be opened by Godot at any effort, and offering it in a
//! picker as though it could is how a studio wastes an afternoon.
//!
//! So [`scan`] classifies rather than lists:
//!
//! * [`FabUsability::Direct`] — glTF, PNG, JPEG, TGA. Godot imports these unchanged.
//! * [`FabUsability::NeedsFbx`] — `.fbx`, which Godot 4 imports only with FBX2glTF present.
//! * [`FabUsability::NeedsUnpack`] — a Unity `.unitypackage`, a gzipped tar this module can
//!   open; the textures inside it are ordinary PNGs.
//! * [`FabUsability::Unusable`] — an Unreal-only entry: a manifest with nothing downloaded,
//!   or `.uasset` payloads. Reported with the reason, never silently hidden.
//!
//! Nothing here guesses a licence. The Fab metadata records a title, a seller and a
//! category but no licence terms, so [`import_icons`] takes the licence as an argument and
//! refuses an empty one: INV-074 blocks a release on an asset whose sidecar cannot name its
//! terms, and inventing terms here would defeat the gate rather than satisfy it.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Points the scan somewhere other than the default vault.
pub const FAB_VAULT_ENV: &str = "BHIPPI_FAB_VAULT";
/// The vault Epic's launcher writes on Windows, under `%ProgramData%`.
pub const FAB_VAULT_SUFFIX: &str = "Epic/EpicGamesLauncher/VaultCache/FabLibrary";
/// The sidecar suffix the release gates read.
pub const LICENSE_SIDECAR_SUFFIX: &str = ".meta.json";
/// The importer name written into every sidecar this module produces.
pub const IMPORTER: &str = "bhippi-fab@1";
/// A pack larger than this is not read into memory to be unpacked.
pub const MAX_UNPACK_BYTES: u64 = 512 * 1024 * 1024;
/// How deep the per-pack file walk goes.
const MAX_SCAN_DEPTH: usize = 8;
/// Past this a pack is described by its counts rather than by every path.
const MAX_FILES_PER_PACK: usize = 20_000;

/// What Godot can do with a pack.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FabUsability {
    /// glTF, PNG, JPEG or TGA: Godot imports these as they are.
    Direct,
    /// FBX, which Godot 4 imports only when FBX2glTF is installed.
    NeedsFbx,
    /// A Unity package Bhippi can open to get at the textures inside.
    NeedsUnpack,
    /// Unreal-only. Godot reads neither a `.uasset` nor an undownloaded manifest.
    Unusable,
}

impl FabUsability {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::NeedsFbx => "needs_fbx",
            Self::NeedsUnpack => "needs_unpack",
            Self::Unusable => "unusable",
        }
    }
}

/// Roughly what is inside, which is what decides where a pack is offered.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FabContent {
    /// 2D art small enough and square enough to be a HUD icon.
    Icons,
    /// Textures and images that are not icons.
    Textures,
    Models,
    Animations,
    Unknown,
}

impl FabContent {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Icons => "icons",
            Self::Textures => "textures",
            Self::Models => "models",
            Self::Animations => "animations",
            Self::Unknown => "unknown",
        }
    }
}

/// One entry in the vault.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct FabPack {
    /// The vault folder name, which is also its stable id.
    pub id: String,
    /// The listing title from the pack's own metadata, falling back to the folder name.
    pub title: String,
    /// The seller, when the metadata names one. Attribution the credits page needs.
    pub seller: String,
    /// Absolute path to the pack folder.
    pub root: String,
    pub usability: FabUsability,
    pub content: FabContent,
    /// Extension to file count, lowercase, without the dot.
    pub counts: BTreeMap<String, u32>,
    /// A cover image inside the pack, if it has one.
    pub thumbnail: Option<String>,
    /// One line saying why the pack is classified as it is. Shown next to it in the picker,
    /// so an unusable pack explains itself instead of just being greyed out.
    pub note: String,
    /// True when [`import_icons`] can pull HUD icons out of this pack.
    pub supplies_icons: bool,
}

/// The default vault for this machine, when it exists.
#[must_use]
pub fn default_vault() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var(FAB_VAULT_ENV) {
        let path = PathBuf::from(override_path);
        return path.is_dir().then_some(path);
    }
    let base = std::env::var("ProgramData")
        .or_else(|_| std::env::var("PROGRAMDATA"))
        .ok()?;
    let path = Path::new(&base).join(FAB_VAULT_SUFFIX);
    path.is_dir().then_some(path)
}

/// Read a vault. One entry per direct child folder; a folder that cannot be read is skipped
/// rather than failing the whole scan, because one unreadable pack should not hide the
/// twenty-nine beside it.
pub fn scan(root: &Path) -> Result<Vec<FabPack>> {
    let entries = std::fs::read_dir(root).map_err(|error| EngineError::Io {
        operation: "read the asset library",
        path: root.display().to_string(),
        reason: error.to_string(),
        hint: Some(
            "Point BHIPPI_FAB_VAULT at a folder of asset packs, or install one through the \
             Epic Games Launcher."
                .to_owned(),
        ),
    })?;

    let mut packs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        packs.push(describe(&path));
    }
    packs.sort_by_key(|pack| pack.title.to_lowercase());
    Ok(packs)
}

fn describe(root: &Path) -> FabPack {
    let id = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut thumbnail: Option<PathBuf> = None;
    let mut files = 0usize;
    walk(root, 0, &mut |path| {
        files += 1;
        if files > MAX_FILES_PER_PACK {
            return false;
        }
        let extension = path
            .extension()
            .map(|value| value.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !extension.is_empty() {
            *counts.entry(extension.clone()).or_insert(0) += 1;
        }
        if thumbnail.is_none() && matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
            let name = path
                .file_stem()
                .map(|value| value.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if name.contains("thumb") || name.contains("cover") || name.contains("preview") {
                thumbnail = Some(path.to_path_buf());
            }
        }
        true
    });

    let (title, seller) = listing(root).unwrap_or_else(|| (prettify(&id), String::new()));
    let count = |extension: &str| counts.get(extension).copied().unwrap_or(0);
    let models = count("glb") + count("gltf");
    let fbx = count("fbx");
    let unity = count("unitypackage");
    let images = count("png") + count("jpg") + count("jpeg") + count("tga");

    // Classification is deliberately about *the files on disk*, not about what the listing
    // says the pack contains: an entry whose Unreal payload was never downloaded still has a
    // listing full of promises.
    let (usability, note) = if models > 0 {
        (
            FabUsability::Direct,
            format!("{models} glTF models — Godot imports these unchanged."),
        )
    } else if unity > 0 {
        (
            FabUsability::NeedsUnpack,
            "A Unity package. Bhippi can unpack the textures inside it for Godot.".to_owned(),
        )
    } else if fbx > 0 {
        (
            FabUsability::NeedsFbx,
            format!("{fbx} FBX files — Godot 4 needs FBX2glTF installed to import them."),
        )
    } else if images > 0 {
        (
            FabUsability::Direct,
            format!("{images} images — Godot imports these unchanged."),
        )
    } else {
        (
            FabUsability::Unusable,
            "Unreal-only: nothing here is a format Godot can open. Re-download the pack in a \
             glTF or Unity format if the listing offers one."
                .to_owned(),
        )
    };

    let content = if unity > 0 && looks_like_icons(root) {
        FabContent::Icons
    } else if models > 0 {
        FabContent::Models
    } else if fbx > 0 {
        FabContent::Animations
    } else if images > 0 || unity > 0 {
        FabContent::Textures
    } else {
        FabContent::Unknown
    };

    FabPack {
        id,
        title,
        seller,
        root: root.display().to_string(),
        usability,
        content,
        counts: counts.into_iter().filter(|(_, value)| *value > 0).collect(),
        thumbnail: thumbnail.map(|path| path.display().to_string()),
        note,
        supplies_icons: usability == FabUsability::NeedsUnpack || content == FabContent::Icons,
    }
}

/// Epic writes the listing beside the payload as UTF-16 JSON. It carries a title, a seller
/// and a category — and, notably, no licence, which is why nothing here invents one.
fn listing(root: &Path) -> Option<(String, String)> {
    let mut found: Option<PathBuf> = None;
    walk(root, 0, &mut |path| {
        if path.file_name().is_some_and(|name| name == "metadata") {
            found = Some(path.to_path_buf());
            return false;
        }
        true
    });
    let bytes = std::fs::read(found?).ok()?;
    let text = decode_utf16(&bytes)?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let listing = value.get("listing")?;
    let title = listing
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let seller = listing
        .get("user")
        .and_then(|user| user.get("sellerName"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    (!title.is_empty()).then_some((title, seller))
}

fn decode_utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 {
        return None;
    }
    let (units, _) = match (bytes[0], bytes[1]) {
        (0xFF, 0xFE) => (
            bytes[2..]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
            true,
        ),
        (0xFE, 0xFF) => (
            bytes[2..]
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect::<Vec<_>>(),
            false,
        ),
        _ => return String::from_utf8(bytes.to_vec()).ok(),
    };
    String::from_utf16(&units).ok()
}

fn looks_like_icons(root: &Path) -> bool {
    let name = root
        .file_name()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    name.contains("icon") || name.contains("ui") || name.contains("hud")
}

fn prettify(id: &str) -> String {
    // Vault folders are `Some_Pack_Name-1275623a`. The hash is not a title.
    let stem = id.rsplit_once('-').map_or(id, |(head, _)| head);
    stem.replace('_', " ").trim().to_owned()
}

fn walk(dir: &Path, depth: usize, visit: &mut impl FnMut(&Path) -> bool) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, depth + 1, visit);
        } else if !visit(&path) {
            return;
        }
    }
}

// ------------------------------------------------------------------- unity packages

/// One file inside a `.unitypackage`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnityAsset {
    /// The path Unity would have written it to, e.g. `Assets/…/Icon_Resources_Heart01_Red.png`.
    pub path: String,
    pub bytes: Vec<u8>,
}

/// Open a `.unitypackage`.
///
/// The format is a gzipped tar of one directory per asset GUID: `pathname` holds the
/// destination path and `asset` holds the bytes. Nothing else in there is of use to Godot,
/// and the `._`-prefixed AppleDouble entries macOS leaves behind are skipped outright.
pub fn unity_assets(path: &Path) -> Result<Vec<UnityAsset>> {
    let size = std::fs::metadata(path)
        .map_err(|error| io_error("stat the asset package", path, &error.to_string()))?
        .len();
    if size > MAX_UNPACK_BYTES {
        return Err(EngineError::Asset(
            format!(
                "{} is {size} bytes, over the {MAX_UNPACK_BYTES}-byte unpack ceiling",
                path.display()
            ),
            Some("Unpack this one outside Bhippi and import the files you want.".to_owned()),
        ));
    }
    let file = std::fs::File::open(path)
        .map_err(|error| io_error("open the asset package", path, &error.to_string()))?;
    let mut decoder = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
    let mut tar = Vec::new();
    decoder
        .read_to_end(&mut tar)
        .map_err(|error| io_error("decompress the asset package", path, &error.to_string()))?;

    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut payloads: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (name, bytes) in read_tar(&tar)? {
        let mut parts = name.trim_start_matches("./").split('/');
        let Some(guid) = parts.next() else { continue };
        if guid.is_empty() || guid.starts_with("._") {
            continue;
        }
        match parts.next() {
            Some("pathname") => {
                let text = String::from_utf8_lossy(&bytes);
                if let Some(first) = text.lines().next() {
                    names.insert(guid.to_owned(), first.trim().to_owned());
                }
            }
            Some("asset") => {
                payloads.insert(guid.to_owned(), bytes);
            }
            _ => {}
        }
    }

    let mut assets: Vec<UnityAsset> = names
        .into_iter()
        .filter_map(|(guid, asset_path)| {
            payloads.remove(&guid).map(|bytes| UnityAsset {
                path: asset_path,
                bytes,
            })
        })
        .collect();
    assets.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(assets)
}

/// A tar reader for exactly the subset a `.unitypackage` uses: ustar headers, regular files
/// and directories, plus GNU long names. Hand-rolled because the alternative is a new
/// dependency for 512-byte headers and one octal parse.
fn read_tar(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>> {
    const BLOCK: usize = 512;
    let mut entries = Vec::new();
    let mut cursor = 0usize;
    let mut long_name: Option<String> = None;

    while cursor + BLOCK <= bytes.len() {
        let header = &bytes[cursor..cursor + BLOCK];
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        let name = long_name.take().unwrap_or_else(|| field(&header[0..100]));
        let size = octal(&header[124..136]).ok_or_else(|| {
            EngineError::Asset(
                "the asset package has a header with an unreadable size".to_owned(),
                Some("The file is truncated or is not a tar archive.".to_owned()),
            )
        })?;
        let kind = header[156];
        cursor += BLOCK;

        let end = cursor.saturating_add(size).min(bytes.len());
        let payload = &bytes[cursor..end];
        match kind {
            // GNU long name: the next header's real name is this entry's payload.
            b'L' => long_name = Some(field(payload)),
            b'0' | b'\0' => entries.push((name, payload.to_vec())),
            _ => {}
        }
        cursor += size.div_ceil(BLOCK) * BLOCK;
    }
    Ok(entries)
}

fn field(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).trim().to_owned()
}

fn octal(bytes: &[u8]) -> Option<usize> {
    let text = field(bytes);
    let digits = text.trim_matches(|character: char| !character.is_ascii_digit());
    if digits.is_empty() {
        return Some(0);
    }
    usize::from_str_radix(digits, 8).ok()
}

fn io_error(operation: &'static str, path: &Path, reason: &str) -> EngineError {
    EngineError::Io {
        operation,
        path: path.display().to_string(),
        reason: reason.to_owned(),
        hint: None,
    }
}

// ------------------------------------------------------------------------ icon import

/// One icon the caller wants, and the words that identify it in a pack whose file names
/// nobody standardised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconRequest {
    pub role: String,
    pub keywords: Vec<String>,
}

/// What one icon import produced.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct ImportedIcon {
    /// The HUD role this answers, e.g. `heart`.
    pub role: String,
    /// Project-relative path of the written PNG.
    pub rel_path: String,
    /// The `res://` form, which is what the HUD script loads.
    pub res_path: String,
    /// The path it came from inside the pack, recorded in the sidecar.
    pub source: String,
}

/// The result of importing a pack's icons into a project.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct IconImport {
    pub imported: Vec<ImportedIcon>,
    /// Roles the pack had nothing for. The HUD falls back to a caption for these, so they
    /// are reported rather than treated as a failure.
    pub unmatched: Vec<String>,
    /// The licence written into every sidecar.
    pub license: String,
}

/// Pull the icons a HUD asked for out of a pack and write them into a project.
///
/// Every file gets a `.meta.json` beside it naming the licence, the seller and the path it
/// came from — which is exactly what the release gates read and what the credits page
/// prints. An empty licence is refused here rather than at export time, because the useful
/// moment to ask is while the user is looking at the pack.
pub fn import_icons(
    pack: &FabPack,
    project_root: &Path,
    icon_dir: &str,
    requests: &[IconRequest],
    license: &str,
) -> Result<IconImport> {
    let license = license.trim();
    if license.is_empty() || license.eq_ignore_ascii_case("unknown") {
        return Err(EngineError::Gate(
            format!("importing from {} needs a licence", pack.title),
            Some(
                "INV-074 blocks a release on an asset whose sidecar cannot name its terms, and \
                 the Fab metadata does not record one. State the licence the pack was \
                 downloaded under."
                    .to_owned(),
            ),
        ));
    }

    let package = find_unity_package(Path::new(&pack.root)).ok_or_else(|| {
        EngineError::NotFound(
            format!("{} has no .unitypackage to unpack", pack.title),
            Some(format!(
                "{} is classified {}.",
                pack.title,
                pack.usability.as_str()
            )),
        )
    })?;
    let assets = unity_assets(&package)?;

    let destination = project_root.join(icon_dir.replace('\\', "/"));
    std::fs::create_dir_all(&destination)
        .map_err(|error| io_error("create the icon folder", &destination, &error.to_string()))?;

    let mut imported = Vec::new();
    let mut unmatched = Vec::new();
    let mut used: BTreeSet<String> = BTreeSet::new();

    for request in requests {
        let Some(asset) = best_match(&assets, request, &used) else {
            unmatched.push(request.role.clone());
            continue;
        };
        used.insert(asset.path.clone());

        let file_name = format!("{}.png", request.role);
        let target = destination.join(&file_name);
        std::fs::write(&target, &asset.bytes)
            .map_err(|error| io_error("write the icon", &target, &error.to_string()))?;

        let rel_path = format!("{}/{file_name}", icon_dir.trim_end_matches('/'));
        let sidecar = target.with_file_name(format!("{file_name}{LICENSE_SIDECAR_SUFFIX}"));
        let source = if pack.seller.is_empty() {
            format!("{} ({})", pack.title, asset.path)
        } else {
            format!("{} by {} ({})", pack.title, pack.seller, asset.path)
        };
        let meta = serde_json::json!({
            "license": license,
            "source": source,
            "importer": IMPORTER,
            "pack": pack.id,
        });
        let text = serde_json::to_string_pretty(&meta).unwrap_or_default();
        std::fs::write(&sidecar, text)
            .map_err(|error| io_error("write the licence sidecar", &sidecar, &error.to_string()))?;

        imported.push(ImportedIcon {
            role: request.role.clone(),
            res_path: format!("res://{rel_path}"),
            rel_path,
            source: asset.path.clone(),
        });
    }

    Ok(IconImport {
        imported,
        unmatched,
        license: license.to_owned(),
    })
}

fn find_unity_package(root: &Path) -> Option<PathBuf> {
    let mut found = None;
    walk(root, 0, &mut |path| {
        if path
            .extension()
            .is_some_and(|value| value.eq_ignore_ascii_case("unitypackage"))
        {
            found = Some(path.to_path_buf());
            return false;
        }
        true
    });
    found
}

/// The best PNG for a role, or nothing.
///
/// Packs ship the same icon at several resolutions. A HUD draws these at around 26 px, so
/// the *smallest* candidate over 100 px is the right one: a 512 px heart in a corner is
/// four hundred kilobytes of nothing.
fn best_match<'a>(
    assets: &'a [UnityAsset],
    request: &IconRequest,
    used: &BTreeSet<String>,
) -> Option<&'a UnityAsset> {
    let mut best: Option<(usize, usize, &UnityAsset)> = None;
    for asset in assets {
        if used.contains(&asset.path) || !asset.path.to_lowercase().ends_with(".png") {
            continue;
        }
        let lower = asset.path.to_lowercase();
        let Some(rank) = request
            .keywords
            .iter()
            .position(|keyword| lower.contains(&keyword.to_lowercase()))
        else {
            continue;
        };
        // Earlier keywords win; among equals the smaller file wins.
        let size = asset.bytes.len();
        let better = match best {
            None => true,
            Some((best_rank, best_size, _)) => {
                rank < best_rank || (rank == best_rank && size < best_size)
            }
        };
        if better {
            best = Some((rank, size, asset));
        }
    }
    best.map(|(_, _, asset)| asset)
}

/// Icons a project already carries, by role, read from its own icon folder.
///
/// This is the reason a HUD built into a project that has been dressed once keeps its art:
/// the project's own files answer first, and the external library is only consulted for the
/// roles they do not cover.
#[must_use]
pub fn project_icons(
    project_root: &Path,
    icon_dir: &str,
    roles: &[&str],
) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let dir = project_root.join(icon_dir.replace('\\', "/"));
    for role in roles {
        for extension in ["png", "svg", "webp"] {
            let candidate = dir.join(format!("{role}.{extension}"));
            if candidate.is_file() {
                found.insert(
                    (*role).to_owned(),
                    format!(
                        "res://{}/{role}.{extension}",
                        icon_dir.trim_end_matches('/')
                    ),
                );
                break;
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("bhippi-fab-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temp dir");
        path
    }

    /// A minimal ustar archive: one 512-byte header per file, name and octal size, payload
    /// padded to the block.
    fn tar_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, payload) in files {
            let mut header = [0u8; 512];
            let bytes = name.as_bytes();
            header[..bytes.len()].copy_from_slice(bytes);
            let size = format!("{:011o}\0", payload.len());
            header[124..136].copy_from_slice(size.as_bytes());
            header[156] = b'0';
            header[257..262].copy_from_slice(b"ustar");
            out.extend_from_slice(&header);
            out.extend_from_slice(payload);
            let pad = (512 - payload.len() % 512) % 512;
            out.extend(std::iter::repeat_n(0u8, pad));
        }
        out.extend(std::iter::repeat_n(0u8, 1024));
        out
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use std::io::Write;
        let mut encoder = GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(bytes).expect("gzip");
        encoder.finish().expect("gzip")
    }

    fn sample_package(path: &Path) {
        let archive = tar_of(&[
            (
                "./aaa/pathname",
                b"Assets/Pack/Icons/128/Icon_Resources_Heart01_Red.png",
            ),
            ("./aaa/asset", b"small-heart"),
            (
                "./bbb/pathname",
                b"Assets/Pack/Icons/512/Icon_Resources_Heart01_Red.png",
            ),
            ("./bbb/asset", b"a-much-larger-heart-payload-here"),
            (
                "./ccc/pathname",
                b"Assets/Pack/Icons/128/Icon_Resources_Coin01_Gold.png",
            ),
            ("./ccc/asset", b"coin"),
            // The AppleDouble sidecars macOS leaves in these archives are not assets.
            ("./._ddd/pathname", b"junk"),
        ]);
        std::fs::write(path, gzip(&archive)).expect("write package");
    }

    #[test]
    fn a_unity_package_reads_as_pathnames_and_payloads() {
        let dir = temp_dir("unpack");
        let package = dir.join("pack.unitypackage");
        sample_package(&package);
        let assets = unity_assets(&package).expect("it unpacks");
        assert_eq!(assets.len(), 3, "the AppleDouble entry is skipped");
        assert!(assets
            .iter()
            .any(|asset| asset.path.ends_with("Heart01_Red.png")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_smallest_copy_of_an_icon_wins() {
        let dir = temp_dir("smallest");
        let package = dir.join("pack.unitypackage");
        sample_package(&package);
        let assets = unity_assets(&package).expect("it unpacks");
        let request = IconRequest {
            role: "heart".to_owned(),
            keywords: vec!["heart".to_owned()],
        };
        let chosen = best_match(&assets, &request, &BTreeSet::new()).expect("a heart matches");
        assert_eq!(
            chosen.bytes, b"small-heart",
            "a HUD draws icons at 26 px; the 512 px copy is waste"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_import_writes_a_licence_sidecar_beside_every_icon() {
        let dir = temp_dir("import");
        let pack_root = dir.join("pack");
        std::fs::create_dir_all(&pack_root).expect("pack dir");
        sample_package(&pack_root.join("icons.unitypackage"));
        let project = dir.join("game");
        std::fs::create_dir_all(&project).expect("project dir");

        let pack = FabPack {
            id: "pack-1".to_owned(),
            title: "Casual Icons".to_owned(),
            seller: "LayerLab".to_owned(),
            root: pack_root.display().to_string(),
            usability: FabUsability::NeedsUnpack,
            content: FabContent::Icons,
            counts: BTreeMap::new(),
            thumbnail: None,
            note: String::new(),
            supplies_icons: true,
        };
        let requests = vec![
            IconRequest {
                role: "heart".to_owned(),
                keywords: vec!["heart".to_owned()],
            },
            IconRequest {
                role: "ammo".to_owned(),
                keywords: vec!["ammo".to_owned(), "bullet".to_owned()],
            },
        ];
        let result = import_icons(
            &pack,
            &project,
            "assets/ui/icons",
            &requests,
            "Fab Standard License",
        )
        .expect("the import runs");

        assert_eq!(result.imported.len(), 1);
        assert_eq!(result.unmatched, vec!["ammo".to_owned()]);
        assert_eq!(
            result.imported[0].res_path,
            "res://assets/ui/icons/heart.png"
        );

        let sidecar = project.join("assets/ui/icons/heart.png.meta.json");
        let text = std::fs::read_to_string(&sidecar).expect("the sidecar exists");
        assert!(text.contains("Fab Standard License"), "{text}");
        assert!(text.contains("LayerLab"), "attribution is recorded: {text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_import_without_a_licence_is_refused() {
        let dir = temp_dir("nolicence");
        let pack_root = dir.join("pack");
        std::fs::create_dir_all(&pack_root).expect("pack dir");
        sample_package(&pack_root.join("icons.unitypackage"));
        let pack = FabPack {
            id: "pack-1".to_owned(),
            title: "Casual Icons".to_owned(),
            seller: String::new(),
            root: pack_root.display().to_string(),
            usability: FabUsability::NeedsUnpack,
            content: FabContent::Icons,
            counts: BTreeMap::new(),
            thumbnail: None,
            note: String::new(),
            supplies_icons: true,
        };
        let error = import_icons(&pack, &dir, "assets/ui/icons", &[], "unknown")
            .expect_err("unknown is not a licence");
        assert!(error.to_string().contains("licence"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_pack_with_nothing_godot_can_open_says_so() {
        let dir = temp_dir("unreal");
        let pack_root = dir.join("Rural_Australia-1c1467ce");
        std::fs::create_dir_all(pack_root.join("unreal-engine")).expect("dirs");
        std::fs::write(pack_root.join("unreal-engine/manifest"), b"{}").expect("manifest");
        let described = describe(&pack_root);
        assert_eq!(described.usability, FabUsability::Unusable);
        assert!(
            described.note.contains("Godot can open"),
            "{}",
            described.note
        );
        assert_eq!(
            described.title, "Rural Australia",
            "the hash is not a title"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_gltf_pack_is_directly_usable_and_an_fbx_pack_needs_a_converter() {
        let dir = temp_dir("classify");
        let gltf = dir.join("Low_Poly_Truck-8ed4a892");
        std::fs::create_dir_all(gltf.join("glb")).expect("dirs");
        std::fs::write(gltf.join("glb/truck.glb"), b"x").expect("glb");
        assert_eq!(describe(&gltf).usability, FabUsability::Direct);

        let fbx = dir.join("Animations-1234abcd");
        std::fs::create_dir_all(fbx.join("fbx")).expect("dirs");
        std::fs::write(fbx.join("fbx/walk.fbx"), b"x").expect("fbx");
        let described = describe(&fbx);
        assert_eq!(described.usability, FabUsability::NeedsFbx);
        assert!(described.note.contains("FBX2glTF"), "{}", described.note);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_projects_own_icons_answer_before_any_library() {
        let dir = temp_dir("projecticons");
        let icons = dir.join("assets/ui/icons");
        std::fs::create_dir_all(&icons).expect("dirs");
        std::fs::write(icons.join("heart.png"), b"x").expect("icon");
        let found = project_icons(&dir, "assets/ui/icons", &["heart", "coin"]);
        assert_eq!(
            found.get("heart").map(String::as_str),
            Some("res://assets/ui/icons/heart.png")
        );
        assert!(!found.contains_key("coin"), "an absent role stays absent");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
