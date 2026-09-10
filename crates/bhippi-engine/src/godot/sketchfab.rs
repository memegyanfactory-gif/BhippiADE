//! Sketchfab, decided: what a licence permits, where a file lands, and what the panel shows
//! (ADR-0055, GAD-180…184).
//!
//! `bhippi-providers::sketchfab` speaks HTTP and decides nothing. This module decides
//! everything and speaks no HTTP — which is why the licence gate, the destination paths and
//! the whole editor channel are testable with the network switched off, and why a rule about
//! what may ship lives somewhere a socket cannot reach.
//!
//! Three things live here.
//!
//! **The licence ruling** ([`rule`]). Sketchfab publishes a licence slug per model, and the
//! distance between "the API returned a string" and "this may be in a game you sell" is the
//! whole risk of this feature. [`LicenceUsage::Refused`] is a real answer: Sketchfab Standard is
//! *editorial use only* and is refused at import, not merely marked unknown — INV-074 would
//! catch it at the Release gate, but by then the model is in the project, in a scene, and
//! the person has built on it. An unrecognised slug is [`LicenseState::Unknown`], which
//! blocks Release exactly as an unlabelled file does. There is no bypass, per R10.
//!
//! **The channel** ([`LibraryState`], [`PanelRequest`]) — the same shape as
//! [`super::live`], for the same reason. The panel inside the Godot editor is a *view*: it
//! reads `.bhippi/live/sketchfab.json` and renders it, and when someone clicks Add it writes
//! `.bhippi/live/sketchfab_request.json` and waits. Every search, every download, every
//! licence decision happens in Rust. A GDScript file that did any of this would be business
//! logic in the one place this project refuses to put it, and would put a credential inside
//! a user-editable file in a user's project.
//!
//! **The destination** ([`plan_import`]) — one folder per model under
//! `assets/models/sketchfab/`, named from the model rather than from the archive, and a
//! sidecar carrying the licence and the attribution verbatim so the credits page
//! ([`super::credits`]) can print it without asking anything.

use super::gates::LICENSE_SIDECAR_SUFFIX;
use super::live::LIVE_DIR_REL;
use crate::asset::LicenseState;
use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// What the panel reads, project-relative. Forward slashes: the addon joins this too.
pub const LIBRARY_STATE_REL: &str = ".bhippi/live/sketchfab.json";
/// The temp file the state is renamed over, so a poll never reads half a file.
pub const LIBRARY_STATE_TMP_REL: &str = ".bhippi/live/sketchfab.json.tmp";
/// What the panel writes when someone clicks something.
pub const PANEL_REQUEST_REL: &str = ".bhippi/live/sketchfab_request.json";
/// The temp file the request is renamed over.
pub const PANEL_REQUEST_TMP_REL: &str = ".bhippi/live/sketchfab_request.json.tmp";
/// Where thumbnails are cached, project-relative. Under `.bhippi/`, so Godot's importer
/// never sees them and no `.import` sibling is ever generated for a picture of a menu.
pub const THUMBNAIL_DIR_REL: &str = ".bhippi/cache/sketchfab";
/// Where imported models land, project-relative.
pub const IMPORT_DIR_REL: &str = "assets/models/sketchfab";
/// The schema version both sides check. A state the addon does not recognise is ignored,
/// which is how an older addon in a user's project fails quiet rather than wrong.
pub const CHANNEL_VERSION: u32 = 1;
/// How often the addon reads the state, in milliseconds. Mirrored by `sketchfab_panel.gd`;
/// `the_addon_and_the_channel_agree` pins the two together.
pub const PANEL_POLL_MS: u32 = 400;
/// The most results one state file carries. The panel scrolls; it does not page a catalogue.
pub const MAX_RESULTS: usize = 24;
/// Longest folder name generated from a model title.
const MAX_SLUG: usize = 48;

// ── the licence ruling ───────────────────────────────────────────────────────────────

/// What Bhippi may do with a model, decided from its Sketchfab licence slug.
///
/// [`LicenceUsage::Unknown`] is the default on purpose: a row that arrived without a ruling — an
/// older state file, a field a future Bhippi added — is treated as unshippable rather than
/// as permitted. A permissive default is how an unlicensed asset reaches a build.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LicenceUsage {
    /// Ships in a Release export. The sidecar names the SPDX licence.
    Allowed,
    /// May be imported and played with, but the Release gate will block it (INV-074),
    /// and the panel says so on the card before the click, not after.
    #[default]
    Unknown,
    /// Refused at import. The licence forbids the use a game makes of it.
    Refused,
}

/// One licence, ruled on.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct LicenceRuling {
    /// Sketchfab's own slug, as it arrived.
    pub slug: String,
    /// The SPDX identifier Bhippi records, when the licence has one.
    pub spdx: Option<String>,
    /// What the sidecar and the gates will see.
    pub license: LicenseState,
    pub usage: LicenceUsage,
    /// True when the credits page must name the author. Every CC licence except CC0.
    pub requires_attribution: bool,
    /// A sentence for the card and for the refusal. Written here, not in the screen.
    pub note: String,
}

/// Rule on a Sketchfab licence slug.
///
/// The mapping is deliberately explicit and short. Sketchfab's slugs have been stable for
/// years, and the failure mode of a clever fuzzy match — treating an unrecognised
/// `by-nc-nd` variant as permissive — is a legal problem in a shipped game. Anything not
/// listed is [`LicenceUsage::Unknown`], which imports but never ships.
///
/// `nc` (non-commercial) is **allowed but recorded**: plenty of Bhippi games are never sold,
/// and refusing at import would take a decision that belongs to the person. The SPDX id
/// carries the `-NC-`, the credits page prints it, and anyone selling the game can see what
/// they are carrying. `nd` (no-derivatives) is the same. Sketchfab Standard is different in
/// kind — it is not a content licence for redistribution at all, so it is refused.
#[must_use]
pub fn rule(slug: &str, label: &str) -> LicenceRuling {
    let normalised = slug.trim().to_ascii_lowercase();
    let (spdx, attribution, note): (Option<&str>, bool, &str) = match normalised.as_str() {
        "cc0" | "cc0-1.0" | "publicdomain" => (
            Some("CC0-1.0"),
            false,
            "Public domain. Ships with no attribution required.",
        ),
        "by" | "cc-by" | "cc-by-4.0" => (
            Some("CC-BY-4.0"),
            true,
            "Ships. The author must be credited; Bhippi adds the line to the credits page.",
        ),
        "by-sa" | "cc-by-sa" | "cc-by-sa-4.0" => (
            Some("CC-BY-SA-4.0"),
            true,
            "Ships with attribution. Share-alike: derivatives of this model carry the same licence.",
        ),
        "by-nd" | "cc-by-nd" | "cc-by-nd-4.0" => (
            Some("CC-BY-ND-4.0"),
            true,
            "Ships with attribution. No derivatives: use it as it is, do not remodel it.",
        ),
        "by-nc" | "cc-by-nc" | "cc-by-nc-4.0" => (
            Some("CC-BY-NC-4.0"),
            true,
            "Ships with attribution, but non-commercial only. Do not sell a game that uses it.",
        ),
        "by-nc-sa" | "cc-by-nc-sa" | "cc-by-nc-sa-4.0" => (
            Some("CC-BY-NC-SA-4.0"),
            true,
            "Non-commercial and share-alike, with attribution. Do not sell a game that uses it.",
        ),
        "by-nc-nd" | "cc-by-nc-nd" | "cc-by-nc-nd-4.0" => (
            Some("CC-BY-NC-ND-4.0"),
            true,
            "Non-commercial, no derivatives, with attribution. Do not sell a game that uses it.",
        ),
        // Sketchfab's own store/standard licence. Editorial use; not a redistribution
        // licence for a game, so the import is refused rather than merely flagged.
        "st" | "standard" | "sketchfab-standard" | "ed" | "editorial" => (
            None,
            true,
            "Sketchfab's Standard/Editorial licence does not permit redistributing the model inside a game. Filter the search to Creative Commons.",
        ),
        _ => (None, true, ""),
    };
    let refused = matches!(
        normalised.as_str(),
        "st" | "standard" | "sketchfab-standard" | "ed" | "editorial"
    );
    let usage = if refused {
        LicenceUsage::Refused
    } else if spdx.is_some() {
        LicenceUsage::Allowed
    } else {
        LicenceUsage::Unknown
    };
    let note = if note.is_empty() {
        let named = if label.trim().is_empty() {
            slug.trim()
        } else {
            label.trim()
        };
        if named.is_empty() {
            "Sketchfab did not state a licence. The model imports, but a Release export will block on it.".to_owned()
        } else {
            format!(
                "Bhippi does not recognise the licence \"{named}\". The model imports, but a Release export will block on it."
            )
        }
    } else {
        note.to_owned()
    };
    LicenceRuling {
        license: spdx.map_or(LicenseState::Unknown, |id| {
            LicenseState::Known(id.to_owned())
        }),
        spdx: spdx.map(str::to_owned),
        usage,
        requires_attribution: attribution,
        note,
        slug: slug.trim().to_owned(),
    }
}

/// The Sketchfab licence slugs a "only what I can ship" search filters on.
///
/// Used as the default filter for the agent's own searches: when a model is picking assets
/// unattended, the right default is the set that cannot produce a blocked Release later.
pub const SHIPPABLE_SLUGS: &[&str] = &["cc0", "by", "by-sa", "by-nd"];

// ── what the panel shows ─────────────────────────────────────────────────────────────

/// Whether anyone is signed in, as the panel's header renders it.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// No credential. The panel shows one button: Connect.
    #[default]
    SignedOut,
    /// The browser is open and Bhippi is waiting for the redirect.
    Connecting,
    /// A credential is in the keychain and Sketchfab accepted it.
    Connected,
}

/// One row of the library strip. Everything the panel draws, already decided.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct LibraryEntry {
    pub uid: String,
    pub name: String,
    pub author: String,
    /// An **absolute** path to the cached thumbnail, or empty when there is none yet.
    /// Absolute because the addon loads it with `Image.load_from_file`, which does not
    /// resolve `res://` for a file outside the imported filesystem.
    pub thumbnail_path: String,
    /// The licence label as the chip prints it, e.g. `CC0-1.0`.
    pub licence_label: String,
    pub usage: LicenceUsage,
    /// The one-sentence explanation, shown on the card's tooltip and on a refusal.
    pub note: String,
    pub face_count: u64,
    pub is_animated: bool,
    pub view_url: String,
    /// Set once the model is in the project: `assets/models/sketchfab/…`. The card then
    /// says "In project" instead of offering to add it again.
    pub imported_rel: Option<String>,
}

/// The whole panel state, rewritten whole on every change.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct LibraryState {
    /// [`CHANNEL_VERSION`] at the time of writing.
    pub version: u32,
    /// Monotonic. The panel redraws only when this moves.
    pub seq: u64,
    pub connection: ConnectionState,
    /// The signed-in account's display name, for the header.
    pub account: String,
    /// The query these results answer, echoed so the panel's box can show it.
    pub query: String,
    /// True while a search or a download is in flight, so the panel can show a spinner
    /// instead of an empty strip that looks like "nothing found".
    pub busy: bool,
    /// What the panel is waiting on, in words. Empty when idle.
    pub status: String,
    /// The last failure, in words the person can act on. Empty when there is none.
    pub error: String,
    pub results: Vec<LibraryEntry>,
}

/// What the panel asks Bhippi to do. One request at a time; the file is deleted once read.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PanelRequest {
    /// Open the browser and sign in.
    Connect,
    /// Forget the credential.
    Disconnect,
    /// Run a search and refresh the strip.
    Search {
        query: String,
        /// True when the person ticked "only what I can ship".
        #[serde(default)]
        shippable_only: bool,
        #[serde(default)]
        animated_only: bool,
    },
    /// Download one model and import it into the project.
    Import { uid: String },
    /// Open the model's Sketchfab page in the system browser.
    Open { uid: String },
}

/// Where the state file lives inside `root`.
#[must_use]
pub fn state_path(root: &Path) -> PathBuf {
    root.join(LIBRARY_STATE_REL)
}

/// Where the request file lives inside `root`.
#[must_use]
pub fn request_path(root: &Path) -> PathBuf {
    root.join(PANEL_REQUEST_REL)
}

/// Where a model's thumbnail is cached inside `root`.
///
/// Named from the uid alone, which Sketchfab guarantees is unique and which
/// [`safe_uid`] has already restricted to characters that cannot escape the folder.
#[must_use]
pub fn thumbnail_path(root: &Path, uid: &str) -> PathBuf {
    root.join(THUMBNAIL_DIR_REL)
        .join(format!("{}.jpg", safe_uid(uid)))
}

/// The state currently on disk, or `None` when there is none or it does not parse.
#[must_use]
pub fn read_state(root: &Path) -> Option<LibraryState> {
    let text = std::fs::read_to_string(state_path(root)).ok()?;
    let state: LibraryState = serde_json::from_str(&text).ok()?;
    (state.version == CHANNEL_VERSION).then_some(state)
}

/// Write the next state, giving it the sequence number after whatever is on disk.
///
/// Whole or not at all, by the same temp-file-and-rename the live channel uses: the panel
/// reads this on a timer and would otherwise read a truncated file.
pub fn publish(root: &Path, mut state: LibraryState) -> Result<LibraryState> {
    let seq = read_state(root).map_or(1, |previous| previous.seq.saturating_add(1));
    state.version = CHANNEL_VERSION;
    state.seq = seq;
    state.results.truncate(MAX_RESULTS);

    let directory = root.join(LIVE_DIR_REL);
    std::fs::create_dir_all(&directory)
        .map_err(|error| io("sketchfab state", &directory, &error))?;
    let text = serde_json::to_string(&state).map_err(|error| EngineError::Io {
        operation: "sketchfab state",
        path: LIBRARY_STATE_REL.to_owned(),
        reason: error.to_string(),
        hint: Some("This is a Bhippi bug: the panel state must always serialise.".to_owned()),
    })?;
    let temporary = root.join(LIBRARY_STATE_TMP_REL);
    std::fs::write(&temporary, text.as_bytes())
        .map_err(|error| io("sketchfab state", &temporary, &error))?;
    let target = state_path(root);
    std::fs::rename(&temporary, &target).map_err(|error| {
        let _ignored = std::fs::remove_file(&temporary);
        io("sketchfab state", &target, &error)
    })?;
    Ok(state)
}

/// Take the pending request, removing it.
///
/// Taking rather than reading is the whole protocol: the panel writes one request and
/// Bhippi consumes it, so a request can never be executed twice — which for an `Import`
/// would mean downloading the same model twice and leaving two copies in the project.
///
/// A request that does not parse is removed too, and reported as `None`. Leaving it would
/// make the pump read the same broken file every 400 ms for the rest of the session.
#[must_use]
pub fn take_request(root: &Path) -> Option<PanelRequest> {
    let path = request_path(root);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ignored = std::fs::remove_file(&path);
    serde_json::from_str(&text).ok()
}

/// Write a request, as the panel does. Also how Bhippi's own UI drives the panel, so both
/// entry points go through one code path.
pub fn put_request(root: &Path, request: &PanelRequest) -> Result<()> {
    let directory = root.join(LIVE_DIR_REL);
    std::fs::create_dir_all(&directory)
        .map_err(|error| io("sketchfab request", &directory, &error))?;
    let text = serde_json::to_string(request).map_err(|error| EngineError::Io {
        operation: "sketchfab request",
        path: PANEL_REQUEST_REL.to_owned(),
        reason: error.to_string(),
        hint: Some("This is a Bhippi bug: a panel request must always serialise.".to_owned()),
    })?;
    let temporary = root.join(PANEL_REQUEST_TMP_REL);
    std::fs::write(&temporary, text.as_bytes())
        .map_err(|error| io("sketchfab request", &temporary, &error))?;
    let target = request_path(root);
    std::fs::rename(&temporary, &target).map_err(|error| {
        let _ignored = std::fs::remove_file(&temporary);
        io("sketchfab request", &target, &error)
    })
}

// ── where a model lands ──────────────────────────────────────────────────────────────

/// Everything an import needs to know before a byte is written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportPlan {
    /// The folder for this model, project-relative with forward slashes.
    pub folder_rel: String,
    /// The main file, project-relative. `res://` + this is what a scene references.
    pub file_rel: String,
    /// Where the sidecar goes, project-relative.
    pub sidecar_rel: String,
    pub ruling: LicenceRuling,
}

/// Decide where a model goes, and refuse the ones that may not come in at all.
///
/// The folder is `assets/models/sketchfab/<slug>-<uid>`: the slug so a person browsing the
/// FileSystem dock can tell what it is, the uid so two models called "knight" cannot
/// collide and so a re-import of the same model is idempotent rather than accumulating
/// `knight-1`, `knight-2`.
///
/// `extension` is the archive's real format (`glb`, `gltf`), not a guess from a URL.
pub fn plan_import(
    name: &str,
    uid: &str,
    ruling: &LicenceRuling,
    extension: &str,
) -> Result<ImportPlan> {
    if ruling.usage == LicenceUsage::Refused {
        return Err(EngineError::Asset(
            format!("`{name}` cannot be imported: {}", ruling.note),
            Some(
                "Search again with the Creative Commons filter on — those models may ship."
                    .to_owned(),
            ),
        ));
    }
    let uid = safe_uid(uid);
    if uid.is_empty() {
        return Err(EngineError::Asset(
            "the model has no usable Sketchfab id".to_owned(),
            Some(
                "This is a Bhippi bug: a search result without a uid should never reach an import."
                    .to_owned(),
            ),
        ));
    }
    let extension = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if !matches!(extension.as_str(), "glb" | "gltf") {
        return Err(EngineError::Asset(
            format!("Bhippi imports Sketchfab models as glb or gltf, not `{extension}`"),
            Some(
                "Pick a model that offers a glTF download; Godot reads those natively.".to_owned(),
            ),
        ));
    }
    let slug = slugify(name);
    let folder_rel = format!("{IMPORT_DIR_REL}/{slug}-{uid}");
    let file_rel = format!("{folder_rel}/{slug}.{extension}");
    Ok(ImportPlan {
        sidecar_rel: format!("{file_rel}{LICENSE_SIDECAR_SUFFIX}"),
        folder_rel,
        file_rel,
        ruling: ruling.clone(),
    })
}

/// The sidecar body for an imported model.
///
/// The shape matches what [`super::credits`] reads and what the release gate checks, so a
/// Sketchfab model is credited on the export page with no special case anywhere: `license`
/// is the SPDX id (or `null`, which blocks Release), `source` is the sentence the credits
/// page prints, and the rest is provenance for a person asking "where did this come from".
#[must_use]
pub fn sidecar_json(
    plan: &ImportPlan,
    model_name: &str,
    author: &str,
    view_url: &str,
    imported_at: &str,
) -> String {
    let attribution = if plan.ruling.requires_attribution {
        format!("\"{model_name}\" by {author} — {view_url}")
    } else {
        format!("\"{model_name}\" by {author} (no attribution required)")
    };
    let value = serde_json::json!({
        "license": plan.ruling.license,
        "importer": "sketchfab",
        "imported_at": imported_at,
        "source": attribution,
        "provenance": "external",
        "sketchfab": {
            "licence_slug": plan.ruling.slug,
            "requires_attribution": plan.ruling.requires_attribution,
            "url": view_url,
            "author": author,
        },
    });
    // Pretty, because a person opens this file to answer a licence question.
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_owned())
}

/// A model title reduced to a folder-safe slug: lowercase, ASCII, single hyphens.
///
/// Never empty — a model named entirely in a script this function cannot transliterate
/// still needs a folder, and `model-<uid>` is a better answer than a folder called `-`.
#[must_use]
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut hyphen = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            if hyphen && !out.is_empty() {
                out.push('-');
            }
            hyphen = false;
            out.push(character.to_ascii_lowercase());
            if out.len() >= MAX_SLUG {
                break;
            }
        } else {
            hyphen = true;
        }
    }
    if out.is_empty() {
        "model".to_owned()
    } else {
        out
    }
}

/// A Sketchfab uid restricted to what may appear in a path.
///
/// Sketchfab's uids are 32 hex characters, so this normally changes nothing. It exists
/// because the uid arrives from the network and is then joined onto a path: a uid of
/// `../../../project.godot` must not be able to name a file outside the folder.
#[must_use]
pub fn safe_uid(uid: &str) -> String {
    uid.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(40)
        .collect()
}

/// The project-relative path of an already-imported model, when one is there.
///
/// The panel uses this to say "In project" instead of offering to download it again, and
/// the agent uses it to avoid paying for the same archive twice in one session.
#[must_use]
pub fn existing_import(root: &Path, uid: &str) -> Option<String> {
    let uid = safe_uid(uid);
    if uid.is_empty() {
        return None;
    }
    let directory = root.join(IMPORT_DIR_REL);
    let entries = std::fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(&format!("-{uid}")) {
            continue;
        }
        // The folder is named for the model; the file inside carries the slug and one of
        // the two extensions `plan_import` allows.
        let files = std::fs::read_dir(entry.path()).ok()?;
        for file in files.flatten() {
            let file_name = file.file_name().to_string_lossy().into_owned();
            if file_name.ends_with(".glb") || file_name.ends_with(".gltf") {
                return Some(format!("{IMPORT_DIR_REL}/{name}/{file_name}"));
            }
        }
    }
    None
}

/// Every folder under `assets/models/sketchfab/` that carries a model, newest first is not
/// promised — the order is the filesystem's, and the caller sorts if it cares.
#[must_use]
pub fn imported_uids(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root.join(IMPORT_DIR_REL)) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.rsplit_once('-')
                .map(|(_, uid)| uid.to_owned())
                .filter(|uid| !uid.is_empty())
        })
        .collect();
    out.sort();
    out
}

fn io(operation: &'static str, path: &Path, error: &std::io::Error) -> EngineError {
    EngineError::Io {
        operation,
        path: path.display().to_string(),
        reason: error.to_string(),
        hint: Some("Check that the project folder is writable.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("bhippi-sketchfab-{tag}-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&root).expect("temp root");
        root
    }

    #[test]
    fn creative_commons_licences_ship_and_carry_their_spdx_id() {
        for (slug, spdx) in [
            ("cc0", "CC0-1.0"),
            ("by", "CC-BY-4.0"),
            ("by-sa", "CC-BY-SA-4.0"),
            ("by-nd", "CC-BY-ND-4.0"),
            ("by-nc", "CC-BY-NC-4.0"),
            ("by-nc-sa", "CC-BY-NC-SA-4.0"),
            ("by-nc-nd", "CC-BY-NC-ND-4.0"),
        ] {
            let ruling = rule(slug, "");
            assert_eq!(ruling.usage, LicenceUsage::Allowed, "{slug}");
            assert_eq!(
                ruling.license,
                LicenseState::Known(spdx.to_owned()),
                "{slug}"
            );
            assert_eq!(ruling.spdx.as_deref(), Some(spdx));
        }
        // CC0 is the only one that does not need a credit line.
        assert!(!rule("cc0", "").requires_attribution);
        assert!(rule("by", "").requires_attribution);
    }

    /// The gate this whole module exists for. Sketchfab Standard is editorial-use-only, so
    /// the import is **blocked**, not warned about — and it stays blocked whichever spelling
    /// of the slug arrives.
    #[test]
    fn the_editorial_licence_blocks_the_import() {
        for slug in [
            "st",
            "standard",
            "sketchfab-standard",
            "ed",
            "editorial",
            "ST",
        ] {
            let ruling = rule(slug, "Sketchfab Standard");
            assert_eq!(ruling.usage, LicenceUsage::Refused, "{slug}");
            assert_eq!(ruling.license, LicenseState::Unknown, "{slug}");

            let refusal = plan_import("Stone Bridge", "ccc333", &ruling, "glb")
                .expect_err("an editorial-licence model must be refused an import plan");
            let text = refusal.to_string();
            assert!(text.contains("Stone Bridge"), "{text}");
            assert!(
                refusal
                    .to_string()
                    .contains("does not permit redistributing"),
                "the refusal must say why: {text}"
            );
        }
    }

    #[test]
    fn an_unrecognised_licence_imports_but_never_ships() {
        let ruling = rule("some-new-slug", "A Brand New Licence");
        assert_eq!(ruling.usage, LicenceUsage::Unknown);
        assert_eq!(ruling.license, LicenseState::Unknown);
        assert!(
            ruling.note.contains("A Brand New Licence"),
            "{}",
            ruling.note
        );
        assert!(
            ruling.note.contains("Release export will block"),
            "{}",
            ruling.note
        );
        // Unknown still gets a plan: it may be imported and played with.
        assert!(plan_import("Thing", "abc", &ruling, "glb").is_ok());
        // And the sidecar it produces is exactly what the release gate refuses.
        let plan = plan_import("Thing", "abc", &ruling, "glb").expect("plan");
        let sidecar: serde_json::Value =
            serde_json::from_str(&sidecar_json(&plan, "Thing", "Someone", "https://x", "now"))
                .expect("sidecar parses");
        assert!(sidecar["license"].is_null(), "unknown serialises as null");
    }

    #[test]
    fn a_hostile_uid_cannot_escape_the_import_folder() {
        let ruling = rule("cc0", "");
        let plan = plan_import("Knight", "../../../project.godot", &ruling, "glb").expect("plan");
        assert_eq!(
            plan.folder_rel, "assets/models/sketchfab/knight-projectgodot",
            "every path separator and dot is stripped from the uid"
        );
        assert!(!plan.file_rel.contains(".."), "{}", plan.file_rel);
        assert!(plan.file_rel.starts_with(IMPORT_DIR_REL));

        // The same for a name made entirely of separators.
        let plan = plan_import("../..", "abc123", &ruling, "glb").expect("plan");
        assert_eq!(plan.folder_rel, "assets/models/sketchfab/model-abc123");
    }

    #[test]
    fn only_gltf_formats_are_planned() {
        let ruling = rule("cc0", "");
        assert!(plan_import("K", "a1", &ruling, "glb").is_ok());
        assert!(plan_import("K", "a1", &ruling, ".gltf").is_ok());
        let refusal = plan_import("K", "a1", &ruling, "blend").expect_err("blend is refused");
        assert!(refusal.to_string().contains("glb or gltf"), "{refusal}");
    }

    #[test]
    fn the_sidecar_says_what_the_credits_page_prints() {
        let ruling = rule("by", "CC Attribution");
        let plan = plan_import("Low Poly Knight", "aaa111", &ruling, "glb").expect("plan");
        assert_eq!(
            plan.file_rel,
            "assets/models/sketchfab/low-poly-knight-aaa111/low-poly-knight.glb"
        );
        assert_eq!(plan.sidecar_rel, format!("{}.meta.json", plan.file_rel));

        let text = sidecar_json(
            &plan,
            "Low Poly Knight",
            "Ada Modeller",
            "https://sketchfab.com/3d-models/aaa111",
            "2026-09-10T00:00:00Z",
        );
        let value: serde_json::Value = serde_json::from_str(&text).expect("sidecar parses");
        assert_eq!(value["license"], "CC-BY-4.0");
        assert_eq!(value["importer"], "sketchfab");
        assert_eq!(value["provenance"], "external");
        assert_eq!(
            value["source"],
            "\"Low Poly Knight\" by Ada Modeller — https://sketchfab.com/3d-models/aaa111"
        );
        assert_eq!(value["sketchfab"]["licence_slug"], "by");

        // CC0 needs no credit, and the sidecar says so rather than inventing a URL line.
        let cc0 = rule("cc0", "");
        let plan = plan_import("Rock", "b2", &cc0, "glb").expect("plan");
        let value: serde_json::Value =
            serde_json::from_str(&sidecar_json(&plan, "Rock", "Bo", "https://x", "now"))
                .expect("parses");
        assert_eq!(value["source"], "\"Rock\" by Bo (no attribution required)");
    }

    #[test]
    fn the_state_is_sequenced_and_capped() {
        let root = temp_root("state");
        assert_eq!(read_state(&root), None, "nothing published yet");

        let first = publish(&root, LibraryState::default()).expect("publish");
        assert_eq!(first.seq, 1);
        assert_eq!(first.version, CHANNEL_VERSION);

        let second = publish(
            &root,
            LibraryState {
                connection: ConnectionState::Connected,
                account: "Ada".to_owned(),
                results: (0..MAX_RESULTS + 9)
                    .map(|index| LibraryEntry {
                        uid: format!("uid{index}"),
                        ..LibraryEntry::default()
                    })
                    .collect(),
                ..LibraryState::default()
            },
        )
        .expect("publish");
        assert_eq!(second.seq, 2, "the sequence continues from disk");
        assert_eq!(second.results.len(), MAX_RESULTS, "the strip is capped");

        let read = read_state(&root).expect("state reads back");
        assert_eq!(read, second, "what is written is what is read");
        assert_eq!(read.connection, ConnectionState::Connected);

        // A state from a future Bhippi is ignored rather than half-applied.
        let mut future = read;
        future.version = CHANNEL_VERSION + 1;
        std::fs::write(
            state_path(&root),
            serde_json::to_string(&future).expect("serialise"),
        )
        .expect("write");
        assert_eq!(read_state(&root), None);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_request_is_taken_exactly_once() {
        let root = temp_root("request");
        assert_eq!(take_request(&root), None);

        let request = PanelRequest::Search {
            query: "knight".to_owned(),
            shippable_only: true,
            animated_only: false,
        };
        put_request(&root, &request).expect("put");
        assert_eq!(take_request(&root), Some(request));
        assert_eq!(
            take_request(&root),
            None,
            "a second take finds nothing — an import can never run twice"
        );
        assert!(!request_path(&root).exists());

        // A corrupt request is consumed too, so the pump does not loop on it forever.
        std::fs::write(request_path(&root), b"{ not json").expect("write");
        assert_eq!(take_request(&root), None);
        assert!(!request_path(&root).exists(), "the broken file is removed");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn every_request_shape_round_trips() {
        for request in [
            PanelRequest::Connect,
            PanelRequest::Disconnect,
            PanelRequest::Search {
                query: "goblin".to_owned(),
                shippable_only: false,
                animated_only: true,
            },
            PanelRequest::Import {
                uid: "aaa111".to_owned(),
            },
            PanelRequest::Open {
                uid: "bbb222".to_owned(),
            },
        ] {
            let text = serde_json::to_string(&request).expect("serialise");
            let back: PanelRequest = serde_json::from_str(&text).expect("parse");
            assert_eq!(back, request, "{text}");
        }
        // The panel writes the minimal shape; the defaults fill the rest in.
        let minimal: PanelRequest =
            serde_json::from_str(r#"{"action":"search","query":"tree"}"#).expect("parse");
        assert_eq!(
            minimal,
            PanelRequest::Search {
                query: "tree".to_owned(),
                shippable_only: false,
                animated_only: false,
            }
        );
    }

    #[test]
    fn an_imported_model_is_found_again_by_its_uid() {
        let root = temp_root("existing");
        assert_eq!(existing_import(&root, "aaa111"), None);
        assert!(imported_uids(&root).is_empty());

        let folder = root.join("assets/models/sketchfab/low-poly-knight-aaa111");
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(folder.join("low-poly-knight.glb"), b"glTF").expect("model");
        assert_eq!(
            existing_import(&root, "aaa111").as_deref(),
            Some("assets/models/sketchfab/low-poly-knight-aaa111/low-poly-knight.glb")
        );
        assert_eq!(imported_uids(&root), vec!["aaa111".to_owned()]);
        assert_eq!(existing_import(&root, "zzz999"), None);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn thumbnails_are_cached_outside_godots_filesystem() {
        let root = Path::new("/project");
        let path = thumbnail_path(root, "aaa111");
        assert!(
            path.to_string_lossy()
                .replace('\\', "/")
                .contains(".bhippi/cache/sketchfab"),
            "{}",
            path.display()
        );
        // Godot skips dot-directories, so no `.import` sibling is ever produced for these.
        assert!(THUMBNAIL_DIR_REL.starts_with(".bhippi/"));
        // And a hostile uid still lands inside that folder.
        let escaped = thumbnail_path(root, "../../evil");
        assert!(escaped.ends_with("evil.jpg"), "{}", escaped.display());
    }

    /// Every generated path has to sit under the folder the release gate walks, or a
    /// Sketchfab model would ship with nobody ever having checked its licence.
    #[test]
    fn imports_land_where_the_licence_gate_looks() {
        assert!(
            IMPORT_DIR_REL.starts_with(&format!("{}/", super::super::ASSETS_DIR)),
            "IMPORT_DIR_REL ({IMPORT_DIR_REL}) must be under ASSETS_DIR"
        );
        let plan = plan_import("Knight", "aaa111", &rule("cc0", ""), "glb").expect("plan");
        assert!(plan
            .file_rel
            .starts_with(&format!("{}/", super::super::ASSETS_DIR)));
        assert!(plan.sidecar_rel.ends_with(LICENSE_SIDECAR_SUFFIX));
    }

    #[test]
    fn the_shippable_filter_is_exactly_the_licences_that_pass_the_release_gate() {
        for slug in SHIPPABLE_SLUGS {
            assert_eq!(
                rule(slug, "").usage,
                LicenceUsage::Allowed,
                "{slug} is offered as shippable"
            );
            assert!(
                rule(slug, "").spdx.is_some(),
                "{slug} must carry an SPDX id or the Release gate blocks it"
            );
        }
        // Non-commercial licences are deliberately *not* in the default filter.
        assert!(!SHIPPABLE_SLUGS.contains(&"by-nc"));
    }
}
