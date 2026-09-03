//! `bhippi-versions@1` — the named points a project can be put back to (GAD-083, GAD-094).
//!
//! A version is **not** a copy of the project. It is a label plus the journal revision the
//! project stood at when the label was made, which is the only thing that stays true when
//! the same folder is edited by Bhippi, by Godot's own editor and by a person with a text
//! editor. Reverting is therefore a *replay*: `bhippi-app` collects the journal rows newer
//! than the recorded revision and applies their inverses as one new transaction, so the
//! revert is itself undoable and nothing is ever restored from a snapshot that could have
//! gone stale.
//!
//! The file lives at `<project>/.bhippi/versions.json` and is written by Rust only. It is
//! JSON rather than TOML because nobody hand-edits it and the ordering has to be exact.

use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

// ── limits ───────────────────────────────────────────────────────────────────────────

/// The format string every file this module writes carries.
pub const VERSIONS_FORMAT: &str = "bhippi-versions@1";
/// The major this build understands. A file claiming a higher one is refused, never
/// half-read: a version list Bhippi does not understand is a revert it must not attempt.
pub const VERSIONS_FORMAT_MAJOR: u32 = 1;
/// Where the list lives, project-relative.
pub const VERSIONS_FILE: &str = ".bhippi/versions.json";
/// How many versions one project keeps. Past this the oldest go, and the caller is told.
pub const MAX_VERSIONS: usize = 200;
/// The longest label a version may carry.
pub const MAX_VERSION_LABEL_CHARS: usize = 80;

// ── the file ─────────────────────────────────────────────────────────────────────────

/// The export one version produced, when it produced one (GAD-094).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct VersionExport {
    /// `web` | `windows` — the preset target, as its slug.
    pub target: String,
    /// Project-relative, forward slashes.
    pub output_path: String,
    pub created_at: String,
}

/// One named point in a project's history.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GameVersion {
    /// A ULID, so the ids sort the way the versions do.
    pub id: String,
    pub label: String,
    /// RFC 3339, UTC.
    pub created_at: String,
    /// The latest Godot journal revision when this version was made. Reverting replays
    /// every row above it.
    pub journal_revision: i64,
    #[serde(default)]
    pub export: Option<VersionExport>,
}

/// The whole file.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct VersionsFile {
    pub format: String,
    #[serde(default)]
    pub versions: Vec<GameVersion>,
}

impl Default for VersionsFile {
    fn default() -> Self {
        Self {
            format: VERSIONS_FORMAT.to_owned(),
            versions: Vec::new(),
        }
    }
}

impl VersionsFile {
    /// Newest first, and never longer than [`MAX_VERSIONS`].
    ///
    /// Ordering is by creation time with the ULID as the tie-break, because two versions
    /// made inside the same second are still ordered by their ids — and a list whose order
    /// depends on how fast the machine is would make "the newest version" a race.
    pub fn sort_and_prune(&mut self) -> usize {
        self.versions.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        let dropped = self.versions.len().saturating_sub(MAX_VERSIONS);
        self.versions.truncate(MAX_VERSIONS);
        dropped
    }

    /// Add one version and re-order. Returns how many old ones fell off the end.
    pub fn push(&mut self, version: GameVersion) -> usize {
        self.versions.push(version);
        self.sort_and_prune()
    }

    #[must_use]
    pub fn find(&self, id: &str) -> Option<&GameVersion> {
        self.versions.iter().find(|version| version.id == id)
    }

    /// Attach an export to a version already in the list.
    pub fn set_export(&mut self, id: &str, export: VersionExport) -> bool {
        match self.versions.iter_mut().find(|version| version.id == id) {
            Some(version) => {
                version.export = Some(export);
                true
            }
            None => false,
        }
    }

    /// The newest export recorded on any version, for the Games card's "last export".
    #[must_use]
    pub fn last_export(&self) -> Option<&VersionExport> {
        self.versions
            .iter()
            .filter_map(|version| version.export.as_ref())
            .max_by(|left, right| left.created_at.cmp(&right.created_at))
    }
}

/// The absolute path of one project's version list.
#[must_use]
pub fn versions_path(project_root: &Path) -> PathBuf {
    project_root.join(VERSIONS_FILE)
}

/// The major of a `name@major` format string, or `None` when it is not one.
#[must_use]
pub fn format_major(format: &str) -> Option<u32> {
    let (name, major) = format.trim().rsplit_once('@')?;
    if name != "bhippi-versions" {
        return None;
    }
    major.trim().parse().ok()
}

/// Parse and validate one version file.
///
/// An unknown **major** is refused rather than tolerated: the whole point of the file is to
/// say which journal rows a revert replays, and guessing at a shape this build has never
/// seen is how a revert deletes work.
pub fn parse_versions(text: &str) -> Result<VersionsFile> {
    let file: VersionsFile = serde_json::from_str(text).map_err(|error| {
        EngineError::Schema(
            format!("{VERSIONS_FILE} is not readable: {error}"),
            Some(
                "Delete the file to start a fresh version list; the project is untouched."
                    .to_owned(),
            ),
        )
    })?;
    let Some(major) = format_major(&file.format) else {
        return Err(EngineError::Schema(
            format!(
                "{VERSIONS_FILE} does not say `{VERSIONS_FORMAT}`; it says `{}`.",
                file.format
            ),
            Some("Only bhippi-versions files belong at this path.".to_owned()),
        ));
    };
    if major != VERSIONS_FORMAT_MAJOR {
        return Err(EngineError::Schema(
            format!(
                "{VERSIONS_FILE} is format major {major}; this build reads {VERSIONS_FORMAT_MAJOR}."
            ),
            Some("Update Bhippi, or move the file aside to start a new version list.".to_owned()),
        ));
    }
    for version in &file.versions {
        if version.id.trim().is_empty() {
            return Err(EngineError::Schema(
                format!("A version in {VERSIONS_FILE} has no id."),
                Some("Every version needs a ULID; move the file aside to start again.".to_owned()),
            ));
        }
        if version.journal_revision < 0 {
            return Err(EngineError::Schema(
                format!(
                    "Version `{}` records journal revision {}, which cannot exist.",
                    version.label, version.journal_revision
                ),
                Some("Revisions start at 0; move the file aside to start again.".to_owned()),
            ));
        }
    }
    let mut file = file;
    file.sort_and_prune();
    Ok(file)
}

/// The file's text. Pretty-printed with a trailing newline, so a diff of two versions of it
/// reads as a list rather than as one very long line.
#[must_use]
pub fn render_versions(file: &VersionsFile) -> String {
    let mut text = serde_json::to_string_pretty(file).unwrap_or_else(|_| {
        format!("{{\n  \"format\": \"{VERSIONS_FORMAT}\",\n  \"versions\": []\n}}")
    });
    text.push('\n');
    text
}

/// Read the list. A project with no file has no versions — that is not an error.
pub fn load_versions(project_root: &Path) -> Result<VersionsFile> {
    let path = versions_path(project_root);
    if !path.is_file() {
        return Ok(VersionsFile::default());
    }
    let text = std::fs::read_to_string(&path).map_err(|error| EngineError::Io {
        operation: "read",
        path: path.display().to_string(),
        reason: error.to_string(),
        hint: Some("Check the project folder is readable.".to_owned()),
    })?;
    parse_versions(&text)
}

/// Write the list, creating `.bhippi/` if it is not there yet.
pub fn save_versions(project_root: &Path, file: &VersionsFile) -> Result<()> {
    let path = versions_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| EngineError::Io {
            operation: "create directory",
            path: parent.display().to_string(),
            reason: error.to_string(),
            hint: Some("Check the project folder is writable.".to_owned()),
        })?;
    }
    std::fs::write(&path, render_versions(file)).map_err(|error| EngineError::Io {
        operation: "write",
        path: path.display().to_string(),
        reason: error.to_string(),
        hint: Some("Check the project folder is writable.".to_owned()),
    })
}

/// A label that is safe to store: trimmed, never empty, never past the cap.
pub fn check_label(label: &str) -> Result<String> {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err(EngineError::Schema(
            "A version needs a label.".to_owned(),
            Some("Name it after what changed — \"feathers collectable\".".to_owned()),
        ));
    }
    if trimmed.chars().count() > MAX_VERSION_LABEL_CHARS {
        return Err(EngineError::Schema(
            format!(
                "That label is {} characters; the limit is {MAX_VERSION_LABEL_CHARS}.",
                trimmed.chars().count()
            ),
            Some("A version label is a phrase, not a changelog.".to_owned()),
        ));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        check_label, format_major, load_versions, parse_versions, render_versions, save_versions,
        GameVersion, VersionExport, VersionsFile, MAX_VERSIONS, VERSIONS_FORMAT,
    };

    fn version(id: &str, at: &str, revision: i64) -> GameVersion {
        GameVersion {
            id: id.to_owned(),
            label: format!("v {id}"),
            created_at: at.to_owned(),
            journal_revision: revision,
            export: None,
        }
    }

    #[test]
    fn a_versions_file_round_trips_through_its_own_writer() {
        let mut file = VersionsFile::default();
        file.push(version("01A", "2026-09-01T10:00:00Z", 3));
        file.push(GameVersion {
            export: Some(VersionExport {
                target: "web".to_owned(),
                output_path: "export/web/index.html".to_owned(),
                created_at: "2026-09-02T11:00:00Z".to_owned(),
            }),
            ..version("01B", "2026-09-02T11:00:00Z", 9)
        });

        let text = render_versions(&file);
        assert!(text.ends_with('\n'));
        let parsed = parse_versions(&text).expect("the writer's own output parses");
        assert_eq!(parsed, file);
        assert_eq!(parsed.versions[0].id, "01B", "newest first");
        assert_eq!(parsed.versions[1].journal_revision, 3);
        assert_eq!(
            parsed.last_export().map(|export| export.target.clone()),
            Some("web".to_owned())
        );
        assert_eq!(
            parsed.find("01A").map(|found| found.label.clone()),
            Some("v 01A".to_owned())
        );
        assert!(parsed.find("nope").is_none());
    }

    #[test]
    fn the_list_stops_at_the_cap_and_the_oldest_are_the_ones_that_go() {
        let mut file = VersionsFile::default();
        let mut dropped = 0;
        for index in 0..MAX_VERSIONS + 5 {
            dropped += file.push(version(
                &format!("{index:05}"),
                &format!("2026-09-01T10:{:02}:{:02}Z", index / 60, index % 60),
                i64::try_from(index).unwrap_or_default(),
            ));
        }
        assert_eq!(file.versions.len(), MAX_VERSIONS);
        assert_eq!(dropped, 5, "each push past the cap drops exactly one");
        assert_eq!(
            file.versions.last().map(|version| version.id.clone()),
            Some("00005".to_owned()),
            "the five oldest went"
        );
    }

    #[test]
    fn an_unknown_major_blocks_rather_than_being_half_read() {
        let text = r#"{"format":"bhippi-versions@2","versions":[]}"#;
        let error = parse_versions(text).expect_err("a future format must block");
        assert!(error.to_string().contains("major 2"));
        assert!(error.hint().is_some());

        let wrong_name = r#"{"format":"bhippi-snapshots@1","versions":[]}"#;
        assert!(parse_versions(wrong_name).is_err());
        assert!(parse_versions("{ not json").is_err());
        assert_eq!(format_major(VERSIONS_FORMAT), Some(1));
        assert_eq!(format_major("bhippi-versions@1"), Some(1));
        assert_eq!(format_major("nonsense"), None);
        assert_eq!(format_major("bhippi-versions@x"), None);
    }

    #[test]
    fn a_negative_revision_is_refused() {
        let text = r#"{"format":"bhippi-versions@1","versions":[
            {"id":"01A","label":"bad","created_at":"2026-09-01T10:00:00Z","journal_revision":-1}]}"#;
        assert!(parse_versions(text).is_err());
    }

    #[test]
    fn a_project_with_no_file_has_no_versions_and_saving_creates_the_folder() {
        let root = std::env::temp_dir().join(format!("bhippi-versions-{}", ulid::Ulid::new()));
        std::fs::create_dir_all(&root).expect("temp root");
        assert!(load_versions(&root)
            .expect("no file is not an error")
            .versions
            .is_empty());

        let mut file = VersionsFile::default();
        file.push(version("01A", "2026-09-01T10:00:00Z", 0));
        save_versions(&root, &file).expect("save");
        assert_eq!(load_versions(&root).expect("reload"), file);
        let _ignored = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_label_is_trimmed_and_bounded() {
        assert_eq!(check_label("  first light  ").expect("ok"), "first light");
        assert!(check_label("   ").is_err());
        assert!(check_label(&"x".repeat(super::MAX_VERSION_LABEL_CHARS + 1)).is_err());
    }
}
