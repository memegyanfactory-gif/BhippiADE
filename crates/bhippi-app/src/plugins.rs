//! The plugin catalogue and its on-disk records (docs/04-PAGES.md, ADR-0029).
//!
//! Two shapes live here on purpose. `PluginRecord` is what we persist: small, and every
//! field `#[serde(default)]` so a file written by an older build still loads instead of
//! blanking the screen. `PluginMetadata` is what the UI receives — the catalogue merged
//! with that record, with the badge and the primary button already decided *here*. The
//! screen renders; it never reasons about plugin state (INV-032).

use crate::commands::AppError;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// What the card's badge says. Derived, never stored — a stored badge goes stale the
/// moment the catalogue moves.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    /// Ships inside the binary. Can be switched off, never removed.
    BuiltIn,
    /// Present and usable.
    Installed,
    /// Installed, but the catalogue carries a newer version than the record.
    UpdateAvailable,
    /// The capability exists but is unusable until its Settings tab is filled in.
    NeedsSetup,
    /// Shipped early and honestly labelled: not finished yet.
    Beta,
    /// In the catalogue, not installed.
    Available,
}

/// The one primary button on a card. Also derived.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PluginAction {
    Open,
    Install,
    Update,
    Configure,
}

/// One row of the merged view the Plugins screen renders.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub category: String,
    /// A glyph key the screen maps to an icon. An unknown key falls back to a generic one.
    pub icon: String,
    pub status: PluginStatus,
    pub action: PluginAction,
    /// The enable toggle. Built-ins toggle too — they just cannot be uninstalled.
    pub activated: bool,
    pub installed: bool,
    pub built_in: bool,
    /// `screen:research`, `workbench:browser`, `panel:brain`, `settings:Usage` … The
    /// screen maps this to a route; anything it does not recognise opens nothing.
    pub target: Option<String>,
    /// The Settings tab this plugin is configured from, when it has one.
    pub settings_tab: Option<String>,
    /// Unix seconds; 0 for a catalogue entry the user never installed. Drives "Recent".
    pub installed_at: u64,
    pub window: Option<PluginWindow>,
}

/// A window a URL-installed plugin asks for.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PluginWindow {
    pub title: String,
    pub width: usize,
    pub height: usize,
    pub url: String,
}

/// What we write to `~/.bhippi/plugins/<id>.json`. Every field defaults so an older or
/// partially written file still parses.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
struct PluginRecord {
    id: String,
    name: String,
    version: String,
    description: String,
    /// Where it came from. Empty for a catalogue entry.
    source: String,
    /// The user's enable toggle.
    activated: bool,
    /// False means "the user removed a pre-installed catalogue entry" — the record has
    /// to survive so we do not silently re-install it on the next listing.
    installed: bool,
    installed_at: u64,
    window: Option<PluginWindow>,
}

/// A catalogue entry: what Bhippi ships knowing about, before any user state is applied.
struct CatalogEntry {
    id: &'static str,
    name: &'static str,
    version: &'static str,
    description: &'static str,
    category: &'static str,
    icon: &'static str,
    target: Option<&'static str>,
    settings_tab: Option<&'static str>,
    /// Ships inside the binary; uninstall is refused.
    built_in: bool,
    /// The capability is already in the app, so a fresh machine has it on.
    preinstalled: bool,
    /// Not finished. The badge says so rather than pretending otherwise.
    beta: bool,
    /// Unusable until `settings_tab` has been filled in.
    requires_setup: bool,
    /// The primary button configures rather than opens, even though a target exists.
    configure_first: bool,
}

/// Catalogue order is the "Recent" order for anything the user never installed.
const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "browser",
        name: "Browser",
        version: "1.2.0",
        description: "Browse the web and extract content with intelligent parsing.",
        category: "Web",
        icon: "browser",
        target: Some("workbench:browser"),
        settings_tab: None,
        built_in: false,
        preinstalled: true,
        beta: false,
        requires_setup: false,
        configure_first: false,
    },
    CatalogEntry {
        id: "terminal",
        name: "Terminal",
        version: "1.0.0",
        description: "Execute shell commands and scripts in an isolated environment.",
        category: "Core",
        icon: "terminal",
        target: Some("screen:chat"),
        settings_tab: None,
        built_in: true,
        preinstalled: true,
        beta: false,
        requires_setup: false,
        configure_first: false,
    },
    CatalogEntry {
        id: "git",
        name: "Git",
        version: "1.1.0",
        description: "Manage repositories, commits, branches and pull requests.",
        category: "Code",
        icon: "git",
        target: Some("panel:review"),
        settings_tab: None,
        built_in: false,
        preinstalled: true,
        beta: false,
        requires_setup: false,
        configure_first: false,
    },
    CatalogEntry {
        id: "website",
        name: "Website",
        version: "0.4.0",
        description: "Interact with websites and web apps for automation tasks.",
        category: "Web",
        icon: "website",
        target: None,
        settings_tab: Some("Publishing"),
        built_in: false,
        preinstalled: false,
        beta: true,
        requires_setup: false,
        configure_first: true,
    },
    CatalogEntry {
        id: "research",
        name: "Research",
        version: "1.3.0",
        description: "Search papers, docs and knowledge bases with AI assistance.",
        category: "Knowledge",
        icon: "research",
        target: Some("screen:research"),
        settings_tab: Some("Research"),
        built_in: false,
        preinstalled: true,
        beta: false,
        requires_setup: false,
        configure_first: false,
    },
    CatalogEntry {
        id: "automation",
        name: "Automation",
        version: "1.1.0",
        description: "Create workflows and automate repetitive tasks.",
        category: "Core",
        icon: "automation",
        target: Some("screen:automation"),
        settings_tab: Some("Automation"),
        built_in: false,
        preinstalled: true,
        beta: false,
        requires_setup: false,
        configure_first: false,
    },
    CatalogEntry {
        id: "memory",
        name: "Memory",
        version: "1.0.0",
        description: "Persist and recall context across sessions securely.",
        category: "Core",
        icon: "memory",
        target: Some("panel:brain"),
        settings_tab: Some("Mind"),
        built_in: true,
        preinstalled: true,
        beta: false,
        requires_setup: false,
        configure_first: true,
    },
    CatalogEntry {
        id: "deployment",
        name: "Deployment",
        version: "0.9.0",
        description: "Deploy apps and manage cloud infrastructure.",
        category: "Infrastructure",
        icon: "deployment",
        target: None,
        settings_tab: Some("Publishing"),
        built_in: false,
        preinstalled: false,
        beta: false,
        requires_setup: true,
        configure_first: true,
    },
    CatalogEntry {
        id: "analytics",
        name: "Analytics",
        version: "1.0.0",
        description: "Track usage, metrics and gain insights from your data.",
        category: "Data",
        icon: "analytics",
        target: Some("settings:Usage"),
        settings_tab: Some("Usage"),
        built_in: false,
        preinstalled: true,
        beta: false,
        requires_setup: false,
        configure_first: false,
    },
    CatalogEntry {
        id: "assets",
        name: "Assets",
        version: "1.0.0",
        description: "Manage and version static assets and resources.",
        category: "Data",
        icon: "assets",
        target: Some("screen:library"),
        settings_tab: None,
        built_in: false,
        preinstalled: true,
        beta: false,
        requires_setup: false,
        configure_first: false,
    },
];

fn catalog(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|entry| entry.id == id)
}

/// Merges one catalogue entry with the user's record for it.
fn merge(entry: &CatalogEntry, record: Option<&PluginRecord>) -> PluginMetadata {
    let default_on = entry.built_in || entry.preinstalled;
    let installed = record.map_or(default_on, |record| record.installed);
    let activated = record.map_or(default_on, |record| record.activated && record.installed);
    // Built-ins move with the binary, so only a *record* can be behind the catalogue.
    let stale = record.is_some_and(|record| {
        record.installed && !record.version.is_empty() && record.version != entry.version
    });

    let status = if stale {
        PluginStatus::UpdateAvailable
    } else if !installed && entry.requires_setup {
        PluginStatus::NeedsSetup
    } else if entry.built_in {
        PluginStatus::BuiltIn
    } else if installed {
        PluginStatus::Installed
    } else if entry.beta {
        PluginStatus::Beta
    } else {
        PluginStatus::Available
    };

    let action = if stale {
        PluginAction::Update
    } else if !installed {
        if entry.requires_setup {
            PluginAction::Configure
        } else {
            PluginAction::Install
        }
    } else if entry.configure_first && entry.settings_tab.is_some() {
        PluginAction::Configure
    } else if entry.target.is_some() {
        PluginAction::Open
    } else {
        PluginAction::Configure
    };

    PluginMetadata {
        id: entry.id.to_owned(),
        name: entry.name.to_owned(),
        // A stale record shows what the user *has*; the Update button carries the rest.
        version: record
            .filter(|record| record.installed && !record.version.is_empty())
            .map_or_else(|| entry.version.to_owned(), |record| record.version.clone()),
        description: entry.description.to_owned(),
        category: entry.category.to_owned(),
        icon: entry.icon.to_owned(),
        status,
        action,
        activated,
        installed,
        built_in: entry.built_in,
        target: entry.target.map(str::to_owned),
        settings_tab: entry.settings_tab.map(str::to_owned),
        installed_at: record.map_or(0, |record| record.installed_at),
        window: record.and_then(|record| record.window.clone()),
    }
}

/// A record with no catalogue entry: something the user installed from a URL.
fn foreign(record: &PluginRecord) -> PluginMetadata {
    PluginMetadata {
        id: record.id.clone(),
        name: if record.name.is_empty() {
            record.id.clone()
        } else {
            record.name.clone()
        },
        version: if record.version.is_empty() {
            "0.1.0".to_owned()
        } else {
            record.version.clone()
        },
        description: if record.description.is_empty() {
            format!("Installed from {}", record.source)
        } else {
            record.description.clone()
        },
        category: "Installed".to_owned(),
        icon: "plugin".to_owned(),
        status: PluginStatus::Installed,
        action: PluginAction::Open,
        activated: record.activated,
        installed: record.installed,
        built_in: false,
        target: record
            .window
            .as_ref()
            .map(|window| format!("url:{}", window.url)),
        settings_tab: None,
        installed_at: record.installed_at,
        window: record.window.clone(),
    }
}

// ── Storage ───────────────────────────────────────────────────────────────────────

/// `~/.bhippi/plugins`, beside `config.toml`, unless `workspace.plugins_dir` names one.
/// The old default was CWD-relative, which filed a user's installs wherever the app
/// happened to be launched from.
async fn plugins_dir(state: &crate::Runtime) -> Result<PathBuf, AppError> {
    let configured = state
        .config
        .load()
        .await
        .ok()
        .and_then(|config| config.workspace.plugins_dir)
        .filter(|dir| !dir.trim().is_empty());
    match configured {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => {
            let path = bhippi_core::ConfigStore::default_path()?;
            path.parent()
                .map(|parent| parent.join("plugins"))
                .ok_or_else(|| AppError::plain("cannot locate the Bhippi configuration directory"))
        }
    }
}

/// Everything on disk. A directory that does not exist yet simply means "nothing
/// installed", and one unreadable file is skipped rather than failing the listing.
fn read_records(dir: &Path) -> Vec<PluginRecord> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut records = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|ext| ext == "json") {
            continue;
        }
        let parsed = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<PluginRecord>(&text).ok())
            .filter(|record| !record.id.is_empty());
        match parsed {
            Some(record) => records.push(record),
            None => tracing::warn!(path = %path.display(), "skipping unreadable plugin record"),
        }
    }
    records
}

fn read_record(dir: &Path, id: &str) -> Option<PluginRecord> {
    read_records(dir).into_iter().find(|record| record.id == id)
}

fn write_record(dir: &Path, record: &PluginRecord) -> Result<(), AppError> {
    std::fs::create_dir_all(dir)
        .map_err(|error| AppError::plain(format!("cannot create {}: {error}", dir.display())))?;
    let json =
        serde_json::to_string_pretty(record).map_err(|error| AppError::plain(error.to_string()))?;
    let path = dir.join(format!("{}.json", record.id));
    std::fs::write(&path, json)
        .map_err(|error| AppError::plain(format!("cannot write {}: {error}", path.display())))
}

/// Plugin ids become file names, so anything that is not a plain slug is rejected
/// rather than escaped — an id is never allowed to reach outside `plugins_dir`.
fn safe_id(raw: &str) -> Option<String> {
    let slug: String = raw
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_owned();
    (!slug.is_empty() && slug.len() <= 64).then_some(slug)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

// ── Operations ────────────────────────────────────────────────────────────────────

/// The catalogue in its published order, then anything installed from a URL, newest
/// first. Sorting and filtering for display stay in the screen; this is the truth.
pub async fn list(state: &crate::Runtime) -> Result<Vec<PluginMetadata>, AppError> {
    let dir = plugins_dir(state).await?;
    let records = read_records(&dir);
    let mut view: Vec<PluginMetadata> = CATALOG
        .iter()
        .map(|entry| merge(entry, records.iter().find(|record| record.id == entry.id)))
        .collect();

    let mut extra: Vec<PluginMetadata> = records
        .iter()
        .filter(|record| catalog(&record.id).is_none())
        .map(foreign)
        .collect();
    extra.sort_by_key(|view| std::cmp::Reverse(view.installed_at));
    view.extend(extra);
    Ok(view)
}

/// Installs a catalogue entry by id, or anything else as a URL-backed plugin.
pub async fn install(state: &crate::Runtime, plugin_ref: &str) -> Result<(), AppError> {
    let plugin_ref = plugin_ref.trim();
    if plugin_ref.is_empty() {
        return Err(AppError::new(
            "no plugin was named",
            "Type a catalogue id such as `website`, or a URL to install from.",
        ));
    }
    let dir = plugins_dir(state).await?;

    if let Some(entry) = catalog(plugin_ref) {
        return write_record(
            &dir,
            &PluginRecord {
                id: entry.id.to_owned(),
                name: entry.name.to_owned(),
                version: entry.version.to_owned(),
                description: entry.description.to_owned(),
                source: String::new(),
                activated: true,
                installed: true,
                installed_at: now_secs(),
                window: None,
            },
        );
    }

    let looks_like_url = plugin_ref.starts_with("http://") || plugin_ref.starts_with("https://");
    let stem = plugin_ref
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or(plugin_ref)
        .trim_end_matches(".json");
    let id = safe_id(stem).ok_or_else(|| {
        AppError::new(
            format!("`{plugin_ref}` is not a usable plugin id"),
            "Use letters, digits and dashes, or paste the plugin's URL.",
        )
    })?;

    write_record(
        &dir,
        &PluginRecord {
            id: id.clone(),
            name: id.clone(),
            version: "0.1.0".to_owned(),
            description: String::new(),
            source: plugin_ref.to_owned(),
            activated: true,
            installed: true,
            installed_at: now_secs(),
            window: looks_like_url.then(|| PluginWindow {
                title: id,
                width: 800,
                height: 600,
                url: plugin_ref.to_owned(),
            }),
        },
    )
}

/// Removes a plugin. A built-in refuses; a pre-installed catalogue entry keeps a record
/// saying it is gone, so the next listing does not quietly bring it back.
pub async fn uninstall(state: &crate::Runtime, plugin_id: &str) -> Result<(), AppError> {
    let id = safe_id(plugin_id)
        .ok_or_else(|| AppError::plain(format!("`{plugin_id}` is not a plugin id")))?;
    if catalog(&id).is_some_and(|entry| entry.built_in) {
        return Err(AppError::new(
            format!("{id} is built into Bhippi and cannot be removed"),
            "Switch it off with the toggle instead.",
        ));
    }
    let dir = plugins_dir(state).await?;

    if catalog(&id).is_some_and(|entry| entry.preinstalled) {
        let mut record = read_record(&dir, &id).unwrap_or_default();
        record.id = id;
        record.installed = false;
        record.activated = false;
        return write_record(&dir, &record);
    }

    let path = dir.join(format!("{id}.json"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::plain(format!(
            "cannot remove {}: {error}",
            path.display()
        ))),
    }
}

/// The enable toggle. Only an installed plugin can be switched on.
pub async fn set_enabled(
    state: &crate::Runtime,
    plugin_id: &str,
    enabled: bool,
) -> Result<(), AppError> {
    let id = safe_id(plugin_id)
        .ok_or_else(|| AppError::plain(format!("`{plugin_id}` is not a plugin id")))?;
    let dir = plugins_dir(state).await?;
    let existing = read_record(&dir, &id);
    let entry = catalog(&id);

    if entry.is_none() && existing.is_none() {
        return Err(AppError::new(
            format!("plugin {id} is not installed"),
            "Install it from the Plugins screen first.",
        ));
    }

    let installed = existing.as_ref().map_or_else(
        || entry.is_some_and(|entry| entry.built_in || entry.preinstalled),
        |record| record.installed,
    );
    if enabled && !installed {
        return Err(AppError::new(
            format!("{id} is not installed yet"),
            "Install it first, then switch it on.",
        ));
    }

    let mut record = existing.unwrap_or_else(|| PluginRecord {
        id: id.clone(),
        name: entry.map(|entry| entry.name.to_owned()).unwrap_or_default(),
        version: entry
            .map(|entry| entry.version.to_owned())
            .unwrap_or_default(),
        description: entry
            .map(|entry| entry.description.to_owned())
            .unwrap_or_default(),
        source: String::new(),
        activated: false,
        installed,
        installed_at: now_secs(),
        window: None,
    });
    record.id = id;
    record.installed = installed;
    record.activated = enabled;
    write_record(&dir, &record)
}

/// Moves an installed record up to the catalogue's version.
pub async fn update(state: &crate::Runtime, plugin_id: &str) -> Result<(), AppError> {
    let id = safe_id(plugin_id)
        .ok_or_else(|| AppError::plain(format!("`{plugin_id}` is not a plugin id")))?;
    let entry = catalog(&id).ok_or_else(|| {
        AppError::new(
            format!("{id} is not in the catalogue"),
            "Only catalogue plugins can be updated from here.",
        )
    })?;
    let dir = plugins_dir(state).await?;
    let mut record = read_record(&dir, &id).ok_or_else(|| {
        AppError::new(
            format!("{id} is not installed"),
            "Install it before updating it.",
        )
    })?;
    record.version = entry.version.to_owned();
    record.description = entry.description.to_owned();
    record.name = entry.name.to_owned();
    record.installed_at = now_secs();
    write_record(&dir, &record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> &'static CatalogEntry {
        catalog(id).unwrap_or_else(|| panic!("{id} is missing from the catalogue"))
    }

    #[test]
    fn a_fresh_machine_shows_a_full_catalogue_not_an_empty_screen() {
        assert_eq!(CATALOG.len(), 10);
        let installed = CATALOG
            .iter()
            .map(|entry| merge(entry, None))
            .filter(|view| view.installed)
            .count();
        assert!(
            installed >= 8,
            "most of the catalogue is capability we already ship, so it is on by default"
        );
    }

    #[test]
    fn every_catalogue_id_is_a_safe_file_name() {
        for entry in CATALOG {
            assert_eq!(safe_id(entry.id).as_deref(), Some(entry.id));
        }
    }

    #[test]
    fn a_traversing_id_is_rejected_rather_than_escaped() {
        assert_eq!(safe_id("../../etc/passwd").as_deref(), Some("etc-passwd"));
        assert_eq!(safe_id("   "), None);
        assert_eq!(safe_id("///"), None);
    }

    #[test]
    fn built_ins_badge_as_built_in_and_configure_when_they_have_a_tab() {
        let memory = merge(entry("memory"), None);
        assert_eq!(memory.status, PluginStatus::BuiltIn);
        assert_eq!(memory.action, PluginAction::Configure);
        assert!(memory.activated);
    }

    #[test]
    fn an_unconfigured_capability_says_needs_setup_and_offers_configure() {
        let deployment = merge(entry("deployment"), None);
        assert_eq!(deployment.status, PluginStatus::NeedsSetup);
        assert_eq!(deployment.action, PluginAction::Configure);
        assert!(!deployment.installed);
    }

    #[test]
    fn an_unfinished_plugin_says_beta_and_offers_install() {
        let website = merge(entry("website"), None);
        assert_eq!(website.status, PluginStatus::Beta);
        assert_eq!(website.action, PluginAction::Install);
    }

    #[test]
    fn update_available_is_earned_by_a_record_behind_the_catalogue() {
        let record = PluginRecord {
            id: "automation".to_owned(),
            version: "1.0.0".to_owned(),
            installed: true,
            activated: true,
            ..PluginRecord::default()
        };
        let view = merge(entry("automation"), Some(&record));
        assert_eq!(view.status, PluginStatus::UpdateAvailable);
        assert_eq!(view.action, PluginAction::Update);
        assert_eq!(view.version, "1.0.0", "the card shows what the user has");

        let current = PluginRecord {
            version: "1.1.0".to_owned(),
            ..record
        };
        assert_eq!(
            merge(entry("automation"), Some(&current)).status,
            PluginStatus::Installed,
            "a current record never invents an update"
        );
    }

    #[test]
    fn uninstalling_a_preinstalled_entry_sticks() {
        let removed = PluginRecord {
            id: "browser".to_owned(),
            installed: false,
            activated: false,
            ..PluginRecord::default()
        };
        let view = merge(entry("browser"), Some(&removed));
        assert!(!view.installed);
        assert!(!view.activated);
        assert_eq!(view.action, PluginAction::Install);
    }

    #[test]
    fn a_disabled_plugin_stays_installed() {
        let off = PluginRecord {
            id: "research".to_owned(),
            version: "1.3.0".to_owned(),
            installed: true,
            activated: false,
            ..PluginRecord::default()
        };
        let view = merge(entry("research"), Some(&off));
        assert!(view.installed);
        assert!(!view.activated);
        assert_eq!(view.status, PluginStatus::Installed);
    }

    #[test]
    fn a_corrupt_record_is_skipped_not_fatal() {
        let dir = std::env::temp_dir().join(format!("bhippi-plugins-{}", now_secs()));
        assert!(
            std::fs::create_dir_all(&dir).is_ok(),
            "temp dir is writable"
        );
        assert!(std::fs::write(dir.join("broken.json"), "{ not json").is_ok());
        assert!(std::fs::write(dir.join("good.json"), r#"{"id":"good","installed":true}"#).is_ok());
        let records = read_records(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "good");
    }

    #[test]
    fn a_url_installed_plugin_renders_without_a_catalogue_entry() {
        let record = PluginRecord {
            id: "acme-tools".to_owned(),
            source: "https://example.com/acme-tools.json".to_owned(),
            installed: true,
            activated: true,
            ..PluginRecord::default()
        };
        let view = foreign(&record);
        assert_eq!(view.name, "acme-tools");
        assert_eq!(view.status, PluginStatus::Installed);
        assert!(view.description.contains("example.com"));
    }
}
