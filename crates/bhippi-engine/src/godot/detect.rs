//! Finding a Godot binary. **This module never downloads anything.**
//!
//! [`describe_install_offer`] only says what the app *may offer* the user — the official
//! release URL and how to verify it. Fetching and running an executable from the internet
//! is not something a library does on someone's behalf, so the download stays a decision the
//! person makes, in the app, with the URL in front of them.
//!
//! Probing a candidate means running `godot --version`, which is process execution and
//! therefore lives in `bhippi-app`. Here it is a [`CommandSpec`](super::command::CommandSpec)
//! builder and a pure parser for the output.
//!
//! # Windows ships two binaries
//!
//! A Godot Windows release contains `Godot_v4.7.1-stable_win64.exe` (a GUI-subsystem binary
//! whose stdout goes nowhere) and `Godot_v4.7.1-stable_win64_console.exe` (a console binary).
//! Reading `--version` from the GUI build returns nothing, so [`GodotInstall`] carries both:
//! `cli_exe` for anything that needs stdout (`--version`, `--check-only`, `--headless`,
//! `--export-*`) and `gui_exe` for a windowed run or the editor, where a console window
//! flashing behind the game is exactly what nobody wants.

use super::command::{version_command, CommandSpec};
use crate::error::{EngineError, Result};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// The Godot version Bhippi pins. Projects are scaffolded with these features.
pub const GODOT_PINNED_VERSION: &str = "4.7.1";
/// The pinned release's Git tag, which is also its download folder.
pub const GODOT_PINNED_TAG: &str = "4.7.1-stable";
/// The oldest Godot Bhippi will drive: `--check-only`, `--quit-after` and the text scene
/// format 3 all behave the same from here up.
pub const GODOT_MINIMUM: (u32, u32) = (4, 3);
/// Where the official builds live.
pub const GODOT_DOWNLOAD_BASE: &str =
    "https://github.com/godotengine/godot/releases/download/4.7.1-stable/";
/// The environment variable that overrides detection with an explicit path.
pub const GODOT_PATH_ENV: &str = "BHIPPI_GODOT";
/// The suffix Windows console builds carry.
pub const WINDOWS_CONSOLE_SUFFIX: &str = "_console.exe";
/// The prefix an unpacked official Windows/Linux build uses.
pub const GODOT_FILE_PREFIX: &str = "Godot_v4";

/// How a candidate binary was found. Ordered by the priority detection walks them in.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum GodotInstallSource {
    /// `BHIPPI_GODOT` — an explicit override always wins.
    EnvVar,
    /// The path saved in Bhippi's own settings.
    Config,
    /// Found on `PATH`.
    Path,
    /// A well-known install directory for the platform.
    CommonDir,
}

/// `4.7.1.stable.official.a13da4feb` taken apart.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GodotVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// `stable`, `rc1`, `beta3`, `dev5` …
    pub status: String,
    /// The line `--version` printed, trimmed.
    pub raw: String,
}

impl GodotVersion {
    /// `4.7.1` — the part a human recognises.
    #[must_use]
    pub fn short(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    /// True when the build is a stable release rather than a pre-release.
    #[must_use]
    pub fn is_stable(&self) -> bool {
        self.status == "stable"
    }
}

/// A usable Godot on this machine.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct GodotInstall {
    /// The binary to use whenever stdout matters. On Windows this is the `_console` build.
    pub cli_exe: PathBuf,
    /// The windowed binary, when the install ships a separate one.
    pub gui_exe: Option<PathBuf>,
    pub version: GodotVersion,
    pub source: GodotInstallSource,
}

impl GodotInstall {
    /// The binary for `--version`, `--check-only`, `--headless` and `--export-*`.
    #[must_use]
    pub fn cli(&self) -> &Path {
        &self.cli_exe
    }

    /// The binary for a windowed run or the editor; falls back to the CLI build.
    #[must_use]
    pub fn gui(&self) -> &Path {
        self.gui_exe.as_deref().unwrap_or(&self.cli_exe)
    }

    #[must_use]
    pub fn is_supported(&self) -> bool {
        is_supported(&self.version)
    }
}

/// Parse the single line `godot --version` prints.
///
/// The shape is `major.minor[.patch].status[.module_config].official[.commit]`. A 4.x build
/// without a patch number (`4.3.stable.official`) is read as patch 0, which is how Godot
/// itself names those releases.
pub fn parse_version(output: &str) -> Result<GodotVersion> {
    let raw = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_owned();
    let unsupported = || {
        EngineError::Build(
            format!("could not read a Godot version from `{raw}`"),
            Some(format!(
                "Bhippi expects the output of `godot --version`, for example `{GODOT_PINNED_VERSION}.stable.official.abc1234`."
            )),
        )
    };
    let mut parts = raw.split('.');
    let major: u32 = parts
        .next()
        .ok_or_else(unsupported)?
        .parse()
        .map_err(|_| unsupported())?;
    let minor: u32 = parts
        .next()
        .ok_or_else(unsupported)?
        .parse()
        .map_err(|_| unsupported())?;
    let third = parts.next().ok_or_else(unsupported)?;
    let (patch, status) = match third.parse::<u32>() {
        Ok(patch) => (patch, parts.next().unwrap_or("stable").to_owned()),
        Err(_) => (0, third.to_owned()),
    };
    if status.is_empty() {
        return Err(unsupported());
    }
    Ok(GodotVersion {
        major,
        minor,
        patch,
        status,
        raw,
    })
}

/// True when this Godot is new enough for the CLI surface Bhippi drives.
#[must_use]
pub fn is_supported(version: &GodotVersion) -> bool {
    let (min_major, min_minor) = GODOT_MINIMUM;
    version.major > min_major || (version.major == min_major && version.minor >= min_minor)
}

/// The typed rejection for an install that is too old, with the version in the hint so the
/// user is not left guessing which of several Godots was found.
pub fn require_supported(version: &GodotVersion) -> Result<()> {
    if is_supported(version) {
        return Ok(());
    }
    let (major, minor) = GODOT_MINIMUM;
    Err(EngineError::Build(
        format!("Godot {} is older than the supported {major}.{minor}", version.short()),
        Some(format!(
            "Install Godot {GODOT_PINNED_VERSION} and point Settings → Godot (or {GODOT_PATH_ENV}) at it."
        )),
    ))
}

/// The `--version` probe for one candidate binary.
#[must_use]
pub fn version_command_for(path: &Path) -> CommandSpec {
    version_command(path)
}

// ── candidates ───────────────────────────────────────────────────────────────────────

/// Executable names a Godot on `PATH` may go by.
pub const PATH_NAMES: &[&str] = &[
    "godot",
    "godot4",
    "godot.exe",
    "godot4.exe",
    "Godot",
    "Godot.exe",
];

/// Every place a Godot binary might be, in the order detection should try them.
///
/// `config_path` is whatever the user chose in Settings. Nothing here touches the network,
/// and a path is returned whether or not it exists — the caller probes it with `--version`,
/// which is the only answer that actually settles the question.
#[must_use]
pub fn candidate_paths(config_path: Option<&Path>) -> Vec<(PathBuf, GodotInstallSource)> {
    let mut found: Vec<(PathBuf, GodotInstallSource)> = Vec::new();
    let push = |path: PathBuf, source: GodotInstallSource, into: &mut Vec<_>| {
        if !into
            .iter()
            .any(|(existing, _): &(PathBuf, GodotInstallSource)| existing == &path)
        {
            into.push((path, source));
        }
    };

    if let Some(value) = std::env::var_os(GODOT_PATH_ENV) {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            push(path, GodotInstallSource::EnvVar, &mut found);
        }
    }
    if let Some(path) = config_path {
        push(path.to_path_buf(), GodotInstallSource::Config, &mut found);
    }
    for path in path_candidates() {
        push(path, GodotInstallSource::Path, &mut found);
    }
    for path in common_dir_candidates() {
        push(path, GodotInstallSource::CommonDir, &mut found);
    }
    found
}

/// Candidates on `PATH`: the fixed names, plus anything a Godot release unpacks as
/// (`Godot_v4.7.1-stable_win64.exe`) sitting in a `PATH` directory.
fn path_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Some(path_var) = std::env::var_os("PATH") else {
        return out;
    };
    for directory in std::env::split_paths(&path_var) {
        if directory.as_os_str().is_empty() {
            continue;
        }
        for name in PATH_NAMES {
            let candidate = directory.join(name);
            if candidate.is_file() {
                out.push(candidate);
            }
        }
        // No glob crate, and none is needed: one `read_dir` and a prefix test.
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(GODOT_FILE_PREFIX) && !name.ends_with(".zip") {
                out.push(entry.path());
            }
        }
    }
    out
}

/// Well-known install directories per platform.
///
/// The Windows list includes `%LOCALAPPDATA%\Programs\Godot\<version>\`, the versioned
/// layout an installed release uses, and the loop below descends one level into any such
/// directory so `4.7.1/Godot_v4.7.1-stable_win64_console.exe` is found without hard-coding
/// the version number.
fn common_dir_candidates() -> Vec<PathBuf> {
    let mut directories: Vec<PathBuf> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();

    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            directories.push(PathBuf::from(&local).join("Programs").join("Godot"));
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            directories.push(PathBuf::from(&program_files).join("Godot"));
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            directories.push(
                PathBuf::from(&profile)
                    .join("scoop")
                    .join("apps")
                    .join("godot")
                    .join("current"),
            );
        }
        directories.push(PathBuf::from("C:\\Godot"));
    } else if cfg!(target_os = "macos") {
        files.push(PathBuf::from(
            "/Applications/Godot.app/Contents/MacOS/Godot",
        ));
        files.push(PathBuf::from(
            "/Applications/Godot_4.app/Contents/MacOS/Godot",
        ));
        if let Some(home) = std::env::var_os("HOME") {
            files.push(PathBuf::from(&home).join("Applications/Godot.app/Contents/MacOS/Godot"));
        }
    } else {
        files.push(PathBuf::from("/usr/bin/godot"));
        files.push(PathBuf::from("/usr/local/bin/godot"));
        files.push(PathBuf::from(
            "/var/lib/flatpak/exports/bin/org.godotengine.Godot",
        ));
        if let Some(home) = std::env::var_os("HOME") {
            files.push(PathBuf::from(&home).join(".local/bin/godot"));
            files.push(
                PathBuf::from(&home).join(".local/share/flatpak/exports/bin/org.godotengine.Godot"),
            );
        }
    }

    for directory in directories {
        collect_binaries(&directory, &mut files);
        // Installed releases sit in a versioned sub-folder: Programs/Godot/4.7.1/…
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .collect();
        // Newest version folder first, so a machine with several picks the latest.
        children.sort();
        children.reverse();
        for child in children {
            collect_binaries(&child, &mut files);
        }
    }
    files
}

/// Every plausible Godot executable directly inside `directory`, console builds first.
fn collect_binaries(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut names: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let Some(name) = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
            else {
                return false;
            };
            if name.ends_with(".zip") || name.ends_with(".pck") {
                return false;
            }
            name.starts_with(GODOT_FILE_PREFIX)
                || name.eq_ignore_ascii_case("godot")
                || name.eq_ignore_ascii_case("godot.exe")
        })
        .collect();
    names.sort_by_key(|path| {
        let console = path
            .file_name()
            .map(|name| name.to_string_lossy().ends_with(WINDOWS_CONSOLE_SUFFIX))
            .unwrap_or(false);
        // `false` sorts before `true`, and the console build is the one we want first.
        (!console, path.clone())
    });
    into.extend(names);
}

/// Pair a probed binary with the other half of a Windows install.
///
/// Given either `Godot_v4.7.1-stable_win64.exe` or its `_console` sibling, this returns the
/// console build as `cli_exe` and the windowed build as `gui_exe` when both are present.
/// On every other platform there is one binary and it is both.
#[must_use]
pub fn pair_windows_binaries(path: &Path) -> (PathBuf, Option<PathBuf>) {
    let Some(name) = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
    else {
        return (path.to_path_buf(), None);
    };
    let Some(parent) = path.parent() else {
        return (path.to_path_buf(), None);
    };
    if let Some(stem) = name.strip_suffix(WINDOWS_CONSOLE_SUFFIX) {
        let gui = parent.join(format!("{stem}.exe"));
        return (path.to_path_buf(), gui.is_file().then_some(gui));
    }
    if let Some(stem) = name.strip_suffix(".exe") {
        let console = parent.join(format!("{stem}{WINDOWS_CONSOLE_SUFFIX}"));
        if console.is_file() {
            return (console, Some(path.to_path_buf()));
        }
    }
    (path.to_path_buf(), None)
}

// ── the install offer ────────────────────────────────────────────────────────────────

/// The platform a download would be for.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum InstallTarget {
    Windows,
    MacOs,
    Linux,
}

impl InstallTarget {
    /// The target this build is running on.
    #[must_use]
    pub fn host() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }

    /// The official archive name for the pinned release.
    #[must_use]
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Windows => "Godot_v4.7.1-stable_win64.exe.zip",
            Self::MacOs => "Godot_v4.7.1-stable_macos.universal.zip",
            Self::Linux => "Godot_v4.7.1-stable_linux.x86_64.zip",
        }
    }
}

/// What the app may *offer*; nothing here fetches anything.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
pub struct InstallOffer {
    pub version: String,
    /// `{file}` is replaced with an [`InstallTarget::file_name`].
    pub download_url_template: String,
    /// Where the checksums are and what to do with them.
    pub sha256_note: String,
    /// Ready-made URLs, one per platform, so the UI does not template strings itself.
    pub downloads: Vec<(InstallTarget, String)>,
}

/// Describe the pinned release. Pure: it builds strings and returns them.
#[must_use]
pub fn describe_install_offer() -> InstallOffer {
    let template = format!("{GODOT_DOWNLOAD_BASE}{{file}}");
    InstallOffer {
        version: GODOT_PINNED_VERSION.to_owned(),
        download_url_template: template,
        sha256_note: format!(
            "Verify the archive against SHA512-SUMS.txt published with the {GODOT_PINNED_TAG} \
             release before running it. Bhippi never downloads or runs the installer for you."
        ),
        downloads: [
            InstallTarget::Windows,
            InstallTarget::MacOs,
            InstallTarget::Linux,
        ]
        .into_iter()
        .map(|target| (target, download_url(target)))
        .collect(),
    }
}

/// The official download URL for one platform's pinned build.
#[must_use]
pub fn download_url(target: InstallTarget) -> String {
    format!("{GODOT_DOWNLOAD_BASE}{}", target.file_name())
}

/// Godot's own name for a version's export-template folder: `4.7.1.stable`, and
/// `4.3.stable` for a release with no patch number.
#[must_use]
pub fn export_template_folder(version: &GodotVersion) -> String {
    if version.patch == 0 {
        format!("{}.{}.{}", version.major, version.minor, version.status)
    } else {
        format!(
            "{}.{}.{}.{}",
            version.major, version.minor, version.patch, version.status
        )
    }
}

/// Where Godot looks for this version's export templates, per platform.
///
/// `--export-release` fails with "No export template found" when this folder is empty, and
/// the templates are a separate multi-hundred-megabyte download — so the app checks here and
/// says *that*, rather than surfacing Godot's message about a preset the user did configure.
#[must_use]
pub fn export_templates_dir(version: &GodotVersion) -> Option<PathBuf> {
    let folder = export_template_folder(version);
    if cfg!(windows) {
        let appdata = std::env::var_os("APPDATA")?;
        Some(
            PathBuf::from(appdata)
                .join("Godot")
                .join("export_templates")
                .join(folder),
        )
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        Some(
            PathBuf::from(home)
                .join("Library/Application Support/Godot/export_templates")
                .join(folder),
        )
    } else {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })?;
        Some(base.join("godot/export_templates").join(folder))
    }
}

/// True when this version's export templates are installed. `--export-*` needs them.
#[must_use]
pub fn export_templates_installed(version: &GodotVersion) -> bool {
    export_templates_dir(version)
        .and_then(|directory| std::fs::read_dir(directory).ok())
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_paths, describe_install_offer, is_supported, pair_windows_binaries,
        parse_version, require_supported, GodotInstallSource, InstallTarget, GODOT_MINIMUM,
        GODOT_PATH_ENV, GODOT_PINNED_VERSION,
    };
    use std::path::Path;

    #[test]
    fn the_version_line_godot_prints_parses() {
        let version = parse_version("4.7.1.stable.official.a13da4feb\n").expect("parses");
        assert_eq!((version.major, version.minor, version.patch), (4, 7, 1));
        assert_eq!(version.status, "stable");
        assert!(version.is_stable());
        assert_eq!(version.short(), "4.7.1");
        assert_eq!(version.raw, "4.7.1.stable.official.a13da4feb");
    }

    #[test]
    fn pre_releases_and_patchless_builds_parse_too() {
        let rc = parse_version("4.8.rc1.official.deadbeef").expect("parses");
        assert_eq!((rc.major, rc.minor, rc.patch), (4, 8, 0));
        assert_eq!(rc.status, "rc1");
        assert!(!rc.is_stable());

        let old = parse_version("4.3.stable.official").expect("parses");
        assert_eq!(old.patch, 0);
        assert!(is_supported(&old));

        let mono = parse_version("4.7.1.stable.mono.official.a13da4feb").expect("parses");
        assert_eq!(mono.status, "stable");
    }

    #[test]
    fn versions_below_the_minimum_are_refused_with_a_hint() {
        let (major, minor) = GODOT_MINIMUM;
        assert_eq!((major, minor), (4, 3));
        let old = parse_version("4.2.2.stable.official").expect("parses");
        assert!(!is_supported(&old));
        let error = require_supported(&old).expect_err("refused");
        assert!(error.hint().unwrap_or_default().contains("4.7.1"));

        assert!(is_supported(
            &parse_version("5.0.stable.official").expect("parses")
        ));
    }

    #[test]
    fn junk_output_is_a_typed_error_rather_than_a_panic() {
        for text in ["", "not a version", "4", "4.x.stable"] {
            let error = parse_version(text).expect_err("must reject");
            assert!(error.hint().is_some(), "{text} needs a hint");
        }
    }

    #[test]
    fn the_env_override_is_the_first_candidate() {
        // Set through the process env because that is exactly what the override is.
        std::env::set_var(GODOT_PATH_ENV, "C:/tmp/godot-override.exe");
        let candidates = candidate_paths(Some(Path::new("C:/tmp/from-settings.exe")));
        std::env::remove_var(GODOT_PATH_ENV);

        assert_eq!(candidates[0].1, GodotInstallSource::EnvVar);
        assert!(candidates[0].0.ends_with("godot-override.exe"));
        assert_eq!(candidates[1].1, GodotInstallSource::Config);
    }

    #[test]
    fn the_install_offer_names_the_official_archives_and_never_a_mirror() {
        let offer = describe_install_offer();
        assert_eq!(offer.version, GODOT_PINNED_VERSION);
        assert!(offer
            .download_url_template
            .starts_with("https://github.com/godotengine/godot/releases/download/4.7.1-stable/"));
        assert!(offer.sha256_note.contains("SHA512-SUMS.txt"));
        assert_eq!(offer.downloads.len(), 3);
        assert_eq!(
            InstallTarget::Windows.file_name(),
            "Godot_v4.7.1-stable_win64.exe.zip"
        );
        assert_eq!(
            InstallTarget::MacOs.file_name(),
            "Godot_v4.7.1-stable_macos.universal.zip"
        );
        assert_eq!(
            InstallTarget::Linux.file_name(),
            "Godot_v4.7.1-stable_linux.x86_64.zip"
        );
    }

    #[test]
    fn export_template_folders_are_named_the_way_godot_names_them() {
        use super::{export_template_folder, export_templates_dir};
        let pinned = parse_version("4.7.1.stable.official.a13da4feb").expect("parses");
        assert_eq!(export_template_folder(&pinned), "4.7.1.stable");
        let patchless = parse_version("4.3.stable.official").expect("parses");
        assert_eq!(export_template_folder(&patchless), "4.3.stable");
        let rc = parse_version("4.8.rc1.official").expect("parses");
        assert_eq!(export_template_folder(&rc), "4.8.rc1");

        // The path is built, not probed: the folder need not exist for this to answer.
        let directory = export_templates_dir(&pinned).expect("a per-user data directory");
        assert!(directory.ends_with("4.7.1.stable"));
        assert!(directory
            .to_string_lossy()
            .replace('\\', "/")
            .contains("export_templates"));
    }

    #[test]
    fn a_lone_binary_pairs_with_itself() {
        // Neither sibling exists on disk, so pairing degrades to "one binary, both jobs".
        let (cli, gui) = pair_windows_binaries(Path::new("/nowhere/godot"));
        assert_eq!(cli, Path::new("/nowhere/godot"));
        assert_eq!(gui, None);
    }
}
