use bhippi_core::{BhippiConfig, ConfigStore};
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
