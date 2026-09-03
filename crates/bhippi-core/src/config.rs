use bhippi_types::{BhippiError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BhippiConfig {
    pub app: AppConfig,
    pub workspace: WorkspaceConfig,
    pub research: ResearchConfig,
    pub domain: DomainConfig,
    pub providers: ProvidersConfig,
    pub automation: AutomationConfig,
    pub ticker: TickerConfig,
    pub publish: PublishConfig,
    pub budget: BudgetConfig,
    pub computer_use: ComputerUseConfig,
    pub engine: EngineConfig,
    pub godot: GodotConfig,
    pub tiers: TiersConfig,
    pub assets: AssetsConfig,
    pub mcp: McpConfig,
}

/// Model Context Protocol servers Bhippi attaches to a turn (SPA-201).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct McpConfig {
    pub blender: BlenderMcpConfig,
}

/// Blender over MCP: the `blender-mcp` server, which talks to an addon running inside
/// Blender. Off by default — a server the user has not set up is a hang on every turn.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BlenderMcpConfig {
    pub enabled: bool,
    /// The launcher, `uvx` by default.
    pub command: String,
    pub args: Vec<String>,
    /// An explicit Blender executable, when detection should not guess.
    pub blender_path: Option<String>,
}

impl Default for BlenderMcpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            command: "uvx".to_owned(),
            args: vec!["blender-mcp".to_owned()],
            blender_path: None,
        }
    }
}

/// The user's asset library (SPA-101): folders outside any project that Bhippi may read
/// from, search, and copy into `assets/`. Read, never written.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AssetsConfig {
    /// Absolute paths, as registered through the Assets screen.
    pub library_dirs: Vec<String>,
}

/// Where Godot lives, when the user has said (ADR-0043 §2).
///
/// Detection prefers `BHIPPI_GODOT`, then this path, then `PATH`, then the platform's
/// install folders. An empty value means "nobody has chosen", not "there is no Godot":
/// Bhippi never downloads the engine, so the only way this fills in is the user pointing
/// at a binary in Settings, and the only way it is trusted is `--version` agreeing.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct GodotConfig {
    /// An explicit Godot executable. Either the console or the windowed Windows build —
    /// detection pairs the two itself.
    pub path: Option<String>,
}

/// The three composer presets the Studio offers instead of three raw pickers (GAD-017).
///
/// A tier is a *preset over the existing pickers*, never a fourth hidden setting: choosing
/// one writes provider / model / effort into the composer, and Settings › Providers still
/// edits the raw rows. A tier whose provider is not usable is shown disabled with the
/// reason — it is never silently swapped for a backend the user did not choose.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TiersConfig {
    #[serde(default = "TierPreset::quick_default")]
    pub quick: TierPreset,
    #[serde(default = "TierPreset::balanced_default")]
    pub balanced: TierPreset,
    #[serde(default = "TierPreset::max_default")]
    pub max: TierPreset,
}

impl Default for TiersConfig {
    fn default() -> Self {
        Self {
            quick: TierPreset::quick_default(),
            balanced: TierPreset::balanced_default(),
            max: TierPreset::max_default(),
        }
    }
}

impl TiersConfig {
    /// The three names the UI and `set_tier` address, in composer order.
    pub const NAMES: [&'static str; 3] = ["quick", "balanced", "max"];

    /// One preset by name, or `None` for a name that is not a tier.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&TierPreset> {
        match name {
            "quick" => Some(&self.quick),
            "balanced" => Some(&self.balanced),
            "max" => Some(&self.max),
            _ => None,
        }
    }

    /// Replaces one preset. Returns `false` for a name that is not a tier, so the caller
    /// reports an unknown tier rather than writing nothing and claiming success.
    pub fn set(&mut self, name: &str, preset: TierPreset) -> bool {
        match name {
            "quick" => self.quick = preset,
            "balanced" => self.balanced = preset,
            "max" => self.max = preset,
            _ => return false,
        }
        true
    }
}

/// One tier row: which backend answers, optionally which model, and at what effort.
///
/// `effort` carries the composer's own vocabulary (`fast` · `balanced` · `quality` ·
/// `ultra`) as a string, because the enum that owns it lives in the app crate and this
/// one is below it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TierPreset {
    pub provider: String,
    pub model: Option<String>,
    pub effort: String,
}

impl TierPreset {
    /// The four effort levels the composer offers; anything else is a config error.
    pub const EFFORTS: [&'static str; 4] = ["fast", "balanced", "quality", "ultra"];

    /// Quick: the cheapest turn. No local backend is *known* at config-default time — the
    /// registry is built later — so this defaults to the offline demo, which is always
    /// usable, and the user points it at Ollama or another local server in Settings.
    #[must_use]
    pub fn quick_default() -> Self {
        Self {
            provider: "demo".to_owned(),
            model: None,
            effort: "fast".to_owned(),
        }
    }

    #[must_use]
    pub fn balanced_default() -> Self {
        Self {
            provider: "claude".to_owned(),
            model: None,
            effort: "balanced".to_owned(),
        }
    }

    #[must_use]
    pub fn max_default() -> Self {
        Self {
            provider: "claude".to_owned(),
            model: None,
            effort: "quality".to_owned(),
        }
    }
}

/// Project references owned by the desktop workspace.
///
/// These are pointers only: forgetting one never removes anything from disk.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub projects: Vec<ProjectRecord>,
    pub active_project: Option<String>,
    pub plugins_dir: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProjectRecord {
    pub name: String,
    pub path: String,
    pub last_opened_at: u64,
}

/// Computer Use and full PC automation configuration.
///
/// Only vision-capable providers (`claude`, `codex`, `grok`) are permitted to use
/// computer vision and control.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ComputerUseConfig {
    pub enabled: bool,
    pub full_access: bool,
    pub allowed_providers: Vec<String>,
}

/// How much the agent may change in the game engine without being asked (ENG-116).
///
/// The three modes are the plan's own vocabulary. `Auto` is the default because it is what
/// the app already did before the gate existed — every engine write is transacted and
/// journaled, so it is reversible with one Ctrl+Z, which is the safety net that makes
/// asking-by-default unnecessary. `Ask` exists for people who want a plan card first.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnginePermissionMode {
    /// Show the plan and wait for approval before anything is written.
    Ask,
    /// Apply edits to scenes that already exist; this is the default.
    #[default]
    Auto,
    /// Apply everything, including deletes, without asking.
    Autonomous,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EngineConfig {
    pub permission_mode: EnginePermissionMode,
}

impl EnginePermissionMode {
    /// Whether a batch containing `destructive` actions needs an explicit yes.
    #[must_use]
    pub fn needs_approval(self, destructive: bool) -> bool {
        match self {
            Self::Ask => true,
            // Deleting is the one thing Auto still stops for: everything else an agent does
            // is additive or adjustable, but a deleted subtree is the change a user is most
            // likely to have not wanted.
            Self::Auto => destructive,
            Self::Autonomous => false,
        }
    }
}

impl Default for ComputerUseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            full_access: true,
            allowed_providers: vec!["claude".to_owned(), "codex".to_owned(), "grok".to_owned()],
        }
    }
}

impl BhippiConfig {
    fn validate(&self) -> Result<()> {
        if self.app.telemetry {
            return Err(config_error(
                "telemetry must remain disabled",
                "Set `app.telemetry = false`; telemetry is not implemented in v1.",
            ));
        }
        if !self.research.respect_robots {
            return Err(config_error(
                "robots policy cannot be disabled",
                "Set `research.respect_robots = true`; there is no bypass mode.",
            ));
        }
        if self.domain.scope
            != [
                "technology".to_owned(),
                "artificial-intelligence".to_owned(),
            ]
        {
            return Err(config_error(
                "domain scope must remain technology and artificial intelligence",
                "Restore the two canonical values in `domain.scope`.",
            ));
        }
        if !(0.0..=1.0).contains(&self.domain.reject_threshold) {
            return Err(config_error(
                "domain.reject_threshold must be between 0 and 1",
                "Choose a rejection threshold in the inclusive range 0.0–1.0.",
            ));
        }
        if self.research.max_parallel_fetches == 0 || self.research.per_host_rps <= 0.0 {
            return Err(config_error(
                "research concurrency and rate limits must be positive",
                "Use at least one fetch permit and a positive per-host rate.",
            ));
        }
        for name in TiersConfig::NAMES {
            let Some(tier) = self.tiers.get(name) else {
                continue;
            };
            if tier.provider.trim().is_empty() {
                return Err(config_error(
                    format!("tiers.{name}.provider must name a provider"),
                    "Set a provider id such as `claude`, `ollama` or `demo`.",
                ));
            }
            if !TierPreset::EFFORTS.contains(&tier.effort.as_str()) {
                return Err(config_error(
                    format!("tiers.{name}.effort must be fast, balanced, quality or ultra"),
                    "Use one of the four effort levels the composer offers.",
                ));
            }
        }
        for provider in &self.computer_use.allowed_providers {
            if provider != "claude" && provider != "codex" && provider != "grok" {
                return Err(config_error(
                    "computer use is permitted only for vision-capable providers: claude, codex, grok",
                    "Remove unsupported providers such as opencode from `computer_use.allowed_providers`.",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub data_dir: String,
    pub theme: Theme,
    pub telemetry: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_dir: "~/.bhippi".to_owned(),
            theme: Theme::Dark,
            telemetry: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResearchConfig {
    pub default_tier: String,
    pub max_parallel_fetches: u8,
    pub per_host_rps: f32,
    pub respect_robots: bool,
    pub language: String,
}

impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            default_tier: "balanced".to_owned(),
            max_parallel_fetches: 6,
            per_host_rps: 0.5,
            respect_robots: true,
            language: "en".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DomainConfig {
    pub scope: [String; 2],
    pub reject_threshold: f32,
}

impl Default for DomainConfig {
    fn default() -> Self {
        Self {
            scope: [
                "technology".to_owned(),
                "artificial-intelligence".to_owned(),
            ],
            reject_threshold: 0.62,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ProvidersConfig {
    pub auto_detect: bool,
    pub offline_mode: bool,
    pub routing: Routing,
    /// Provider ids the user turned on in Settings › Providers. Absent ids default to
    /// off; detection never flips these (only the toggle does).
    pub enabled: Vec<String>,
    /// Unix-seconds of the last silent auto-update sweep; 0 = never ran.
    pub last_auto_update: u64,
    /// The model last chosen for each provider, keyed by provider id. Switching provider
    /// and back returns to the user's choice instead of silently resetting it.
    pub last_model: BTreeMap<String, String>,
    /// The provider the user last picked in the composer, restored on launch.
    ///
    /// This does not make a CLI the *default* — nothing is auto-selected here, and a
    /// backend that is no longer enabled or reachable is ignored on load. It only keeps
    /// an explicit choice from being thrown away every time the app restarts.
    pub last_provider: Option<String>,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            offline_mode: false,
            routing: Routing::Balanced,
            // The offline demo is always on; everything else is opt-in.
            enabled: vec!["demo".to_owned()],
            last_auto_update: 0,
            last_model: BTreeMap::new(),
            last_provider: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Routing {
    Quality,
    #[default]
    Balanced,
    Cheap,
    LocalOnly,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AutomationConfig {
    pub enabled: bool,
    pub mode: AutomationMode,
    pub interval_mins: u32,
    pub review_gate: bool,
    pub daily_post_cap: u16,
    pub quiet_hours: [String; 2],
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: AutomationMode::Off,
            interval_mins: 60,
            review_gate: true,
            daily_post_cap: 6,
            quiet_hours: ["23:30".to_owned(), "07:00".to_owned()],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutomationMode {
    #[default]
    Off,
    Timer,
    Ticker,
    Both,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TickerConfig {
    pub poll_secs: u32,
    pub burst_sources: u16,
    pub auto_trigger_score: u8,
}

impl Default for TickerConfig {
    fn default() -> Self {
        Self {
            poll_secs: 120,
            burst_sources: 3,
            auto_trigger_score: 78,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PublishConfig {
    pub target: PublishTarget,
    pub site_url: String,
    pub out_dir: String,
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            target: PublishTarget::Static,
            site_url: "https://bhippi.example".to_owned(),
            out_dir: "~/.bhippi/site".to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PublishTarget {
    #[default]
    Static,
    GithubPages,
    Netlify,
    Cloudflare,
    Wordpress,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BudgetConfig {
    pub daily_token_cap: u64,
    pub daily_wall_secs: u64,
    pub per_session_usd_cap: f64,
    /// Calendar-month spend ceiling in USD across every metered provider (SPA-003). `0.0`
    /// means no ceiling. Reaching it blocks sending until the user raises it — the composer
    /// card says so and offers the field.
    pub monthly_usd_cap: f64,
    /// Per-provider daily ceilings the usage gauge measures against, keyed by provider
    /// id. An absent id falls back to `daily_token_cap`; a stored `0` means "no ceiling"
    /// and the gauge renders as unmetered rather than as instantly full.
    pub provider_token_caps: BTreeMap<String, u64>,
}

impl BudgetConfig {
    /// The ceiling that applies to one provider, or `None` when it is uncapped.
    #[must_use]
    pub fn cap_for(&self, provider_id: &str) -> Option<u64> {
        match self.provider_token_caps.get(provider_id) {
            Some(0) => None,
            Some(cap) => Some(*cap),
            None if self.daily_token_cap == 0 => None,
            None => Some(self.daily_token_cap),
        }
    }
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            daily_token_cap: 2_000_000,
            daily_wall_secs: 14_400,
            per_session_usd_cap: 0.0,
            monthly_usd_cap: 0.0,
            provider_token_caps: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn default_path() -> Result<PathBuf> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .ok_or_else(|| {
                config_error(
                    "home directory is unavailable",
                    "Set HOME or USERPROFILE before starting Bhippi.",
                )
            })?;
        Ok(PathBuf::from(home).join(".bhippi").join("config.toml"))
    }

    pub async fn load(&self) -> Result<BhippiConfig> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(text) => {
                let config = toml::from_str::<BhippiConfig>(&text).map_err(|error| {
                    config_error(
                        format!("cannot parse {}: {error}", self.path.display()),
                        "Fix the named field or restore the documented default config.",
                    )
                })?;
                config.validate()?;
                Ok(config)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(BhippiConfig::default())
            }
            Err(error) => Err(config_error(
                format!("cannot read {}: {error}", self.path.display()),
                "Check the config file permissions and retry.",
            )),
        }
    }

    pub async fn save(&self, config: &BhippiConfig) -> Result<()> {
        config.validate()?;
        let text = toml::to_string_pretty(config).map_err(|error| {
            config_error(
                format!("cannot encode configuration: {error}"),
                "Restore the documented default values and retry.",
            )
        })?;
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            config_error(
                format!("cannot create {}: {error}", parent.display()),
                "Check the data-directory permissions and retry.",
            )
        })?;
        tokio::fs::write(&self.path, text).await.map_err(|error| {
            config_error(
                format!("cannot write {}: {error}", self.path.display()),
                "Check the config file permissions and retry.",
            )
        })
    }
}

fn config_error(reason: impl Into<String>, hint: impl Into<String>) -> BhippiError {
    BhippiError::Config {
        reason: reason.into(),
        hint: Some(hint.into()),
    }
}
