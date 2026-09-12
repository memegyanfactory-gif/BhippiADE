use bhippi_core::{BhippiConfig, ConfigStore, TierPreset, TiersConfig};
use bhippi_types::SessionId;
use std::path::{Path, PathBuf};

fn config_path() -> PathBuf {
    std::env::temp_dir()
        .join(format!("bhippi-config-{}", SessionId::new()))
        .join("config.toml")
}

fn cleanup(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
}

#[tokio::test]
async fn config_round_trip_contains_no_secret_fields() {
    let path = config_path();
    let store = ConfigStore::new(&path);
    let config = BhippiConfig::default();

    store
        .save(&config)
        .await
        .unwrap_or_else(|error| panic!("default config must save: {error}"));
    let loaded = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("saved config must load: {error}"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("saved config must be readable: {error}"));

    assert_eq!(loaded, config);
    for forbidden in ["api_key", "password", "secret", "token ="] {
        assert!(!text.to_ascii_lowercase().contains(forbidden));
    }

    cleanup(&path);
}

#[tokio::test]
async fn locked_safety_settings_are_rejected() {
    let path = config_path();
    let store = ConfigStore::new(&path);
    let text = "[app]\ntelemetry = true\n[research]\nrespect_robots = false\n";
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("test directory must exist: {error}"));
    }
    std::fs::write(&path, text)
        .unwrap_or_else(|error| panic!("unsafe fixture config must be written: {error}"));

    let result = store.load().await;

    assert!(result.is_err());
    cleanup(&path);
}

/// The usage gauge is only honest if its ceiling survives a restart, so the per-provider
/// caps must round-trip through `config.toml` exactly as written.
#[tokio::test]
async fn per_provider_token_caps_survive_a_save_and_load() {
    let path = config_path();
    let store = ConfigStore::new(&path);
    let mut config = BhippiConfig::default();
    config
        .budget
        .provider_token_caps
        .insert("anthropic".to_owned(), 400_000);
    // A stored zero means "no ceiling", not "block everything".
    config
        .budget
        .provider_token_caps
        .insert("ollama".to_owned(), 0);

    store
        .save(&config)
        .await
        .unwrap_or_else(|error| panic!("config with caps must save: {error}"));
    let loaded = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("config with caps must load: {error}"));

    assert_eq!(
        loaded.budget.provider_token_caps,
        config.budget.provider_token_caps
    );
    assert_eq!(loaded.budget.cap_for("anthropic"), Some(400_000));
    assert_eq!(loaded.budget.cap_for("ollama"), None);
    // An unlisted provider falls back to the shared daily cap.
    assert_eq!(
        loaded.budget.cap_for("openai"),
        Some(BhippiConfig::default().budget.daily_token_cap)
    );

    cleanup(&path);
}

/// The composer must reopen where the user left it, so the per-provider model choice
/// has to survive a restart exactly as picked — including the removal that returns a
/// provider to its vendor default.
#[tokio::test]
async fn last_model_choices_survive_a_save_and_load() {
    let path = config_path();
    let store = ConfigStore::new(&path);
    let mut config = BhippiConfig::default();
    config
        .providers
        .last_model
        .insert("ollama".to_owned(), "qwen3:8b".to_owned());
    config
        .providers
        .last_model
        .insert("claude".to_owned(), "sonnet".to_owned());

    store
        .save(&config)
        .await
        .unwrap_or_else(|error| panic!("config with model choices must save: {error}"));
    let loaded = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("config with model choices must load: {error}"));

    assert_eq!(
        loaded
            .providers
            .last_model
            .get("ollama")
            .map(String::as_str),
        Some("qwen3:8b")
    );
    assert_eq!(
        loaded
            .providers
            .last_model
            .get("claude")
            .map(String::as_str),
        Some("sonnet")
    );
    assert_eq!(loaded.providers.last_model.get("codex"), None);

    // Forgetting a choice must also round-trip: the map shrinks and stays gone.
    let mut loaded = loaded;
    loaded.providers.last_model.remove("claude");
    store
        .save(&loaded)
        .await
        .unwrap_or_else(|error| panic!("config without the removed choice must save: {error}"));
    let reloaded = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("reloaded config must load: {error}"));

    assert_eq!(reloaded.providers.last_model.get("claude"), None);
    assert_eq!(
        reloaded
            .providers
            .last_model
            .get("ollama")
            .map(String::as_str),
        Some("qwen3:8b")
    );

    cleanup(&path);
}

/// The composer's provider choice must survive a restart, and forgetting it must too.
///
/// Without this the picker resets to the offline demo on every launch, which reads as
/// "the provider I chose does not work" rather than "the app forgot".
#[tokio::test]
async fn the_chosen_provider_round_trips_through_config() {
    let path = config_path();
    let store = ConfigStore::new(&path);

    let mut config = BhippiConfig::default();
    assert_eq!(
        config.providers.last_provider, None,
        "nothing is preselected"
    );
    config.providers.last_provider = Some("opencode".to_owned());
    store
        .save(&config)
        .await
        .unwrap_or_else(|error| panic!("config with a chosen provider must save: {error}"));

    let mut loaded = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("saved config must load: {error}"));
    assert_eq!(loaded.providers.last_provider.as_deref(), Some("opencode"));

    loaded.providers.last_provider = None;
    store
        .save(&loaded)
        .await
        .unwrap_or_else(|error| panic!("config without a choice must save: {error}"));
    let reloaded = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("reloaded config must load: {error}"));
    assert_eq!(reloaded.providers.last_provider, None);

    cleanup(&path);
}

#[tokio::test]
async fn workspace_projects_survive_a_restart_without_owning_their_files() {
    let path = config_path();
    let store = ConfigStore::new(&path);
    let mut config = BhippiConfig::default();
    config.workspace.projects.push(bhippi_core::ProjectRecord {
        name: "Bhippi".to_owned(),
        path: "C:/Work/Bhippi".to_owned(),
        last_opened_at: 42,
    });
    config.workspace.active_project = Some("C:/Work/Bhippi".to_owned());

    store
        .save(&config)
        .await
        .unwrap_or_else(|error| panic!("config with a project must save: {error}"));
    let loaded = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("workspace config must load: {error}"));

    assert_eq!(loaded.workspace, config.workspace);
    cleanup(&path);
}

/// The Quick / Balanced / Max chips are only trustworthy if the row behind each one
/// survives a restart exactly as edited — a tier that quietly reverts is the "silent
/// swap" GAD-017 exists to prevent.
#[tokio::test]
async fn tier_presets_round_trip_and_reject_an_unknown_effort() {
    let path = config_path();
    let store = ConfigStore::new(&path);
    let config = BhippiConfig::default();

    assert_eq!(config.tiers.quick.provider, "demo");
    assert_eq!(config.tiers.quick.effort, "fast");
    assert_eq!(config.tiers.quick.model, None);
    assert_eq!(config.tiers.balanced.provider, "claude");
    assert_eq!(config.tiers.balanced.effort, "balanced");
    assert_eq!(config.tiers.max.provider, "claude");
    assert_eq!(config.tiers.max.effort, "quality");

    let mut edited = config.clone();
    assert!(edited.tiers.set(
        "quick",
        TierPreset {
            provider: "ollama".to_owned(),
            model: Some("qwen3:8b".to_owned()),
            effort: "fast".to_owned(),
        }
    ));
    assert!(
        !edited.tiers.set("turbo", TierPreset::quick_default()),
        "a name that is not a tier is refused rather than silently ignored"
    );

    store
        .save(&edited)
        .await
        .unwrap_or_else(|error| panic!("edited tiers must save: {error}"));
    let loaded = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("saved tiers must load: {error}"));

    assert_eq!(loaded.tiers, edited.tiers);
    assert_eq!(loaded.tiers.quick.provider, "ollama");
    assert_eq!(loaded.tiers.quick.model.as_deref(), Some("qwen3:8b"));
    assert_eq!(loaded.tiers.max, config.tiers.max, "one edit moves one row");

    // A partial `[tiers]` table keeps the two rows it does not mention.
    let partial = "[tiers.max]\nprovider = \"codex\"\neffort = \"ultra\"\n";
    std::fs::write(&path, partial)
        .unwrap_or_else(|error| panic!("partial tiers fixture must be written: {error}"));
    let patched = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("partial tiers must load: {error}"));
    assert_eq!(patched.tiers.max.provider, "codex");
    assert_eq!(patched.tiers.max.effort, "ultra");
    assert_eq!(patched.tiers.quick, TierPreset::quick_default());
    assert_eq!(patched.tiers.balanced, TierPreset::balanced_default());

    // An effort the composer cannot render is a config error, not a shrug.
    let mut broken = config;
    broken.tiers.balanced.effort = "turbo".to_owned();
    assert!(store.save(&broken).await.is_err());
    assert_eq!(TiersConfig::NAMES, ["quick", "balanced", "max"]);

    cleanup(&path);
}

#[tokio::test]
async fn computer_use_config_round_trips_and_blocks_unauthorized_providers() {
    let path = config_path();
    let store = ConfigStore::new(&path);
    let mut config = BhippiConfig::default();
    config.computer_use.enabled = true;
    config.computer_use.full_access = true;
    config.computer_use.allowed_providers =
        vec!["claude".to_owned(), "codex".to_owned(), "grok".to_owned()];

    store
        .save(&config)
        .await
        .unwrap_or_else(|error| panic!("valid computer use config must save: {error}"));
    let loaded = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("saved computer use config must load: {error}"));

    assert_eq!(loaded.computer_use, config.computer_use);
    assert!(loaded.computer_use.enabled);
    assert!(loaded.computer_use.full_access);

    // Adding an unauthorized text-only provider like opencode must fail validation
    config
        .computer_use
        .allowed_providers
        .push("opencode".to_owned());
    let save_err = store.save(&config).await;
    assert!(
        save_err.is_err(),
        "opencode must be blocked from computer use"
    );

    cleanup(&path);
}

// ── the composer's permission chip (PermissionPosture) ──────────────────────────
//
// The chip used to be three unrelated switches: a `localStorage` mode that auto-clicked
// permission cards in the page, `engine.permission_mode` that nothing ever wrote, and the
// Computer Use toggle. That is why "Auto" and "Full access" did the same thing. One posture
// now decides all three, and these pin the relationship so it cannot quietly come apart
// again — a partial write is exactly how it broke the first time.

#[test]
fn each_posture_writes_all_three_settings_together() {
    use bhippi_core::{EnginePermissionMode, PermissionPosture};

    let mut config = BhippiConfig::default();

    config.apply_posture(PermissionPosture::AskApproval);
    assert_eq!(config.permission, PermissionPosture::AskApproval);
    assert_eq!(config.engine.permission_mode, EnginePermissionMode::Ask);
    assert!(
        !config.computer_use.enabled,
        "Ask approval never drives the screen"
    );
    assert!(!config.computer_use.full_access);

    config.apply_posture(PermissionPosture::Auto);
    assert_eq!(config.permission, PermissionPosture::Auto);
    // Not `Auto`: the engine's own Auto still stops for a delete, and a user who picked the
    // chip labelled Auto has said they do not want to be stopped.
    assert_eq!(
        config.engine.permission_mode,
        EnginePermissionMode::Autonomous
    );
    assert!(
        !config.computer_use.enabled,
        "Auto is the project, not the desktop — that line is the whole point of Full access"
    );
    assert!(!config.computer_use.full_access);

    config.apply_posture(PermissionPosture::FullAccess);
    assert_eq!(config.permission, PermissionPosture::FullAccess);
    assert_eq!(
        config.engine.permission_mode,
        EnginePermissionMode::Autonomous
    );
    assert!(config.computer_use.enabled);
    // Seeing the screen and being allowed to touch it arrive together: a "Full access" that
    // could look but not click would be a fourth thing to explain.
    assert!(config.computer_use.full_access);
}

#[test]
fn auto_and_full_access_differ_by_exactly_one_thing() {
    use bhippi_core::PermissionPosture;

    let mut auto = BhippiConfig::default();
    auto.apply_posture(PermissionPosture::Auto);
    let mut full = BhippiConfig::default();
    full.apply_posture(PermissionPosture::FullAccess);

    assert_eq!(
        auto.engine.permission_mode, full.engine.permission_mode,
        "inside the project the two are the same posture"
    );
    assert_ne!(
        auto.computer_use.enabled, full.computer_use.enabled,
        "the machine is the only difference; if this ever passes as equal the chip is \
         offering two names for one thing again"
    );
}

#[test]
fn stepping_down_from_full_access_takes_the_screen_back() {
    use bhippi_core::PermissionPosture;

    let mut config = BhippiConfig::default();
    config.apply_posture(PermissionPosture::FullAccess);
    config.apply_posture(PermissionPosture::AskApproval);

    assert!(
        !config.computer_use.enabled && !config.computer_use.full_access,
        "a posture change that only ever grants would make the chip a one-way door"
    );
}

#[test]
fn only_ask_approval_puts_the_card_to_the_user() {
    use bhippi_core::PermissionPosture;

    assert!(PermissionPosture::AskApproval.asks_first());
    assert!(!PermissionPosture::Auto.asks_first());
    assert!(!PermissionPosture::FullAccess.asks_first());
}

#[tokio::test]
async fn a_saved_posture_survives_a_reload() {
    use bhippi_core::PermissionPosture;

    let path = config_path();
    let store = ConfigStore::new(&path);
    let mut config = BhippiConfig::default();
    config.apply_posture(PermissionPosture::FullAccess);

    store
        .save(&config)
        .await
        .unwrap_or_else(|error| panic!("a posture must save: {error}"));
    let loaded = store
        .load()
        .await
        .unwrap_or_else(|error| panic!("a saved posture must load: {error}"));

    // The chip reads this on launch. If the posture did not round-trip, the menu would show
    // "Ask approval" over a config that still had the screen switched on.
    assert_eq!(loaded.permission, PermissionPosture::FullAccess);
    assert!(loaded.computer_use.enabled);

    cleanup(&path);
}
