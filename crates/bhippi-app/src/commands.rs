//! The IPC command surface (spec §25). Every command is typed through specta so
//! `ui/src/lib/ipc.ts` stays generated, never hand-written (INV-032).

use crate::chat::{
    ConversationMeta, ConversationScope, ConversationView, DesignMode, Effort, PermissionDecision,
    ProviderInstallProgress, ProvidersChanged, TurnOptions, TurnPair, WorkspaceSession,
};
use crate::context::{summarise, ContextSummary, ContextWindow};
use crate::status::AppStatus;
use crate::usage::{summarise_with_accounts, UsageSummary, UsageWindow};
use base64::Engine as _;
use bhippi_types::BhippiError;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

// The plugin catalogue, its records and the merge that decides each card live in
// `crate::plugins`. These commands are the IPC surface over it and nothing more.
// `PluginStatus`, `PluginAction` and `PluginWindow` reach the bindings through this
// type's own graph, so they need no separate import here.
use crate::plugins::PluginMetadata;

impl From<BhippiError> for AppError {
    fn from(error: BhippiError) -> Self {
        Self {
            message: error.to_string(),
            hint: error.hint().map(str::to_owned),
        }
    }
}

/// Serializable error crossing IPC; carries the actionable hint where one exists (R1).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AppError {
    pub message: String,
    pub hint: Option<String>,
}

impl AppError {
    #[must_use]
    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: None,
        }
    }

    /// The shape R1 actually asks for: what went wrong plus what to do about it.
    #[must_use]
    pub fn new(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }
}

/// Resolves the active project from persisted Rust state and canonicalizes it immediately
/// before use. Frontend-supplied paths never decide a chat workspace.
async fn active_project_path(state: &crate::Runtime) -> Result<Option<String>, AppError> {
    let config = state.config.load().await.map_err(AppError::from)?;
    let Some(saved) = config.workspace.active_project else {
        return Ok(None);
    };
    if !config
        .workspace
        .projects
        .iter()
        .any(|project| crate::chat::ChatEngine::paths_match(&project.path, &saved))
    {
        return Err(AppError {
            message: "The active project is not registered.".to_owned(),
            hint: Some("Choose the project again from the sidebar.".to_owned()),
        });
    }
    let canonical = std::fs::canonicalize(&saved).map_err(|error| AppError {
        message: format!("The active project is unavailable: {error}"),
        hint: Some("Restore the folder or choose another project.".to_owned()),
    })?;
    if !canonical.is_dir() {
        return Err(AppError::plain("The active project is not a directory."));
    }
    Ok(Some(crate::workspace::display_path(&canonical)))
}

pub(crate) async fn required_project_path(state: &crate::Runtime) -> Result<String, AppError> {
    active_project_path(state).await?.ok_or_else(|| AppError {
        message: "No project is open.".to_owned(),
        hint: Some("Add or select a project from the sidebar first.".to_owned()),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_app_status(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<AppStatus, AppError> {
    let state = state.inner();
    let registry = state.registry.read().await.clone();
    let saved = state
        .config
        .load()
        .await
        .map(|config| config.providers)
        .ok();
    let last_model = saved
        .as_ref()
        .map(|providers| providers.last_model.clone())
        .unwrap_or_default();
    // A remembered choice only survives while that backend is still usable; otherwise the
    // composer falls back to the default rather than pointing at something that is gone.
    let last_provider = saved
        .and_then(|providers| providers.last_provider)
        .filter(|id| registry.by_id.contains_key(id));
    let default = registry
        .providers
        .iter()
        .find(|row| row.id == registry.default_id);
    Ok(AppStatus {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        active_provider: default
            .map(|row| row.label.clone())
            .unwrap_or_else(|| "Demo (offline)".to_owned()),
        active_provider_id: registry.default_id.clone(),
        demo_mode: registry.default_id == "demo",
        chat_options: registry.chat_options(),
        providers: registry.providers.clone(),
        tokens_today: state.engine.tokens_today(),
        last_model,
        last_provider,
    })
}

/// Re-detects every catalogued backend and rebuilds the runtime, keeping the user's
/// toggle prefs intact. Emits `providers-changed` when it lands.
#[tauri::command]
#[specta::specta]
pub async fn rescan_providers(state: tauri::State<'_, crate::Runtime>) -> Result<(), AppError> {
    let enabled = state.enabled_ids().await;
    let detected: Vec<bhippi_providers::ProviderInfo> =
        bhippi_providers::detect(bhippi_providers::CATALOG, &enabled).await;
    let next = std::sync::Arc::new(crate::chat::ProviderRuntime::from_detection(
        detected.clone(),
    ));
    *state.registry.write().await = next;
    (ProvidersChanged {
        providers: detected,
    })
    .emit(&state.app_handle)
    .map_err(|error| AppError::plain(format!("event delivery failed: {error}")))?;
    Ok(())
}

/// Flips one provider's toggle, persists it in `config.toml`, and rebuilds the runtime.
#[tauri::command]
#[specta::specta]
pub async fn set_provider_enabled(
    state: tauri::State<'_, crate::Runtime>,
    provider_id: String,
    enabled: bool,
) -> Result<(), AppError> {
    if bhippi_providers::spec(&provider_id).is_none() && provider_id != "demo" {
        return Err(AppError::plain(format!("unknown provider {provider_id}")));
    }
    {
        let mut config = state.config.load().await.map_err(AppError::from)?;
        if enabled {
            if !config.providers.enabled.iter().any(|id| id == &provider_id) {
                config.providers.enabled.push(provider_id.clone());
            }
        } else {
            config.providers.enabled.retain(|id| id != &provider_id);
        }
        state.config.save(&config).await.map_err(AppError::from)?;
    }
    state.rescan_quietly().await;
    Ok(())
}

/// Runs the catalogue's install recipe for one CLI provider, streaming progress events.
/// The user's explicit click **is** the permission for this consequential action.
#[tauri::command]
#[specta::specta]
pub async fn install_provider(
    state: tauri::State<'_, crate::Runtime>,
    provider_id: String,
) -> Result<(), AppError> {
    let Some(spec) = bhippi_providers::spec(&provider_id) else {
        return Err(AppError::plain(format!("unknown provider {provider_id}")));
    };
    let Some(recipe) = spec.install else {
        return Err(AppError::plain(format!(
            "{} has nothing to install — enable it once its server is running.",
            spec.label
        )));
    };

    let emit = |phase: &'static str, message: String| {
        let handle = state.app_handle.clone();
        let id = provider_id.clone();
        async move {
            let _ignored = (ProviderInstallProgress {
                id,
                phase: phase.to_owned(),
                message,
            })
            .emit(&handle);
        }
    };

    emit(
        "starting",
        format!("Downloading and installing the latest {}…", spec.label),
    )
    .await;
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(960),
        bhippi_providers::run_recipe(&recipe),
    )
    .await;

    match outcome {
        Ok(Ok(_tail)) => {
            emit("verifying", format!("Verifying {}…", spec.label)).await;
            state.rescan_quietly().await;
            let installed = state
                .registry
                .read()
                .await
                .providers
                .iter()
                .find(|row| row.id == provider_id && row.installed)
                .cloned();
            let Some(installed) = installed else {
                let reason = format!(
                    "{} finished installing, but its command is still unavailable.",
                    spec.label
                );
                emit("failed", reason.clone()).await;
                return Err(AppError {
                    message: format!("{} could not be verified", spec.label),
                    hint: Some(reason),
                });
            };
            let message = installed.version.map_or_else(
                || format!("{} is installed and ready.", spec.label),
                |version| format!("{} {version} is installed and ready.", spec.label),
            );
            emit("done", message).await;
            tracing::info!(provider = %provider_id, "install finished");
            Ok(())
        }
        Ok(Err(reason)) => {
            emit("failed", reason.clone()).await;
            Err(AppError {
                message: format!("installing {} failed", spec.label),
                hint: Some(reason),
            })
        }
        Err(_) => {
            emit("failed", "timed out after 960s".to_owned()).await;
            Err(AppError::plain(format!(
                "installing {} timed out",
                spec.label
            )))
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn list_conversations(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<ConversationMeta>, AppError> {
    let Some(project_path) = active_project_path(state.inner()).await? else {
        return Ok(Vec::new());
    };
    Ok(state.engine.list_conversations(&project_path).await)
}

/// One session per project for the workspace rail. Unlike `list_conversations` this
/// is not scoped to the active project — the sidebar shows every project's sessions.
///
/// Turns store the provider *label*, but the icon library keys on the catalogue *id*,
/// so a session's `provider` is resolved here where the detection rows are visible.
/// Unknown/synthetic labels keep `provider: None` and let the UI fall back to a
/// generic chat mark.
#[tauri::command]
#[specta::specta]
pub async fn list_workspace_sessions(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<WorkspaceSession>, AppError> {
    let mut sessions = state.engine.workspace_sessions().await;
    let providers = state.registry.read().await.providers.clone();
    for session in &mut sessions {
        if let Some(label) = &session.provider_label {
            session.provider = providers
                .iter()
                .find(|row| &row.label == label)
                .map(|row| row.id.clone());
        }
    }
    Ok(sessions)
}

#[tauri::command]
#[specta::specta]
pub async fn new_conversation(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<ConversationMeta, AppError> {
    let project_path = required_project_path(state.inner()).await?;
    state
        .engine
        .ensure_conversation(&project_path, None)
        .await
        .map_err(AppError::plain)
}

#[tauri::command]
#[specta::specta]
pub async fn get_conversation(
    state: tauri::State<'_, crate::Runtime>,
    conversation_id: String,
) -> Result<Option<ConversationView>, AppError> {
    let project_path = required_project_path(state.inner()).await?;
    Ok(state
        .engine
        .conversation_view(&project_path, &conversation_id)
        .await)
}

/// Removes one conversation and everything in it.
///
/// The user's click on the bin **is** the permission for this destructive action, so it
/// is not asked for twice — but a turn still streaming is stopped first, or its task
/// would go on emitting events for a thread that no longer exists.
#[tauri::command]
#[specta::specta]
pub async fn delete_conversation(
    state: tauri::State<'_, crate::Runtime>,
    conversation_id: String,
) -> Result<Vec<ConversationMeta>, AppError> {
    let project_path = required_project_path(state.inner()).await?;
    state
        .engine
        .delete_conversation(&project_path, &conversation_id)
        .await;
    Ok(state.engine.list_conversations(&project_path).await)
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn send_chat_message(
    state: tauri::State<'_, crate::Runtime>,
    conversation_id: Option<String>,
    text: String,
    provider_id: Option<String>,
    model: Option<String>,
    effort: Option<Effort>,
    design: Option<DesignMode>,
    caveman: Option<bool>,
    attachments: Option<Vec<String>>,
) -> Result<TurnPair, AppError> {
    let text = text.trim().to_owned();
    let attachments = attachments.unwrap_or_default();
    if text.is_empty() && attachments.is_empty() {
        return Err(AppError::plain("Message is empty."));
    }
    let project_path = required_project_path(state.inner()).await?;
    let meta = state
        .engine
        .ensure_conversation(&project_path, conversation_id)
        .await
        .map_err(AppError::plain)?;
    let registry = state.registry.read().await.clone();
    state
        .engine
        .send(
            &registry,
            ConversationScope {
                project_path,
                conversation_id: meta.id,
            },
            text,
            TurnOptions {
                provider_id,
                model,
                effort: effort.unwrap_or_default(),
                design: design.unwrap_or_default(),
                caveman: caveman.unwrap_or(false),
                attachments,
            },
        )
        .await
        .map_err(AppError::plain)
}

#[tauri::command]
#[specta::specta]
pub async fn regenerate_last_answer(
    state: tauri::State<'_, crate::Runtime>,
    conversation_id: String,
    provider_id: Option<String>,
    model: Option<String>,
    effort: Option<Effort>,
    design: Option<DesignMode>,
    caveman: Option<bool>,
) -> Result<TurnPair, AppError> {
    let project_path = required_project_path(state.inner()).await?;
    let registry = state.registry.read().await.clone();
    let outcome = state
        .engine
        .regenerate(
            &registry,
            ConversationScope {
                project_path,
                conversation_id,
            },
            TurnOptions {
                provider_id,
                model,
                effort: effort.unwrap_or_default(),
                design: design.unwrap_or_default(),
                caveman: caveman.unwrap_or(false),
                // A regenerate re-runs the stored user turn, which already carries its
                // `Attached:` line; the picked files themselves are not re-read, so this
                // command keeps the signature it had.
                ..TurnOptions::default()
            },
        )
        .await;
    // `None` means nothing to regenerate; a wrapped Err is a provider problem.
    match outcome {
        Some(result) => result.map_err(AppError::plain),
        None => Err(AppError::plain("Nothing to regenerate yet.")),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn stop_chat_turn(
    state: tauri::State<'_, crate::Runtime>,
    turn_id: String,
) -> Result<(), AppError> {
    state.engine.stop(&turn_id).await;
    Ok(())
}

/// Put every file one turn changed back as it was (CHT-115).
///
/// Returns the number of files restored. The snapshot is session-scoped and consumed, so
/// this succeeds once and then reports honestly that there is nothing left to undo — which
/// is why the card asks `chat_turn_undoable` before offering the button at all.
#[tauri::command]
#[specta::specta]
pub async fn undo_chat_turn(
    state: tauri::State<'_, crate::Runtime>,
    turn_id: String,
) -> Result<u32, AppError> {
    state
        .engine
        .undo_turn(&turn_id)
        .await
        .map(|count| u32::try_from(count).unwrap_or(u32::MAX))
        .map_err(|message| AppError {
            message,
            hint: Some(
                "Undo only reaches back over this session's writes. Use Review to see what                  changed and revert it yourself."
                    .to_owned(),
            ),
        })
}

/// Whether a turn's changes can still be put back — what disables the Undo button, with a
/// reason, instead of letting it fail when pressed.
#[tauri::command]
#[specta::specta]
pub async fn chat_turn_undoable(
    state: tauri::State<'_, crate::Runtime>,
    turn_id: String,
) -> Result<bool, AppError> {
    Ok(state.engine.turn_undoable(&turn_id).await)
}

#[tauri::command]
#[specta::specta]
pub async fn respond_permission(
    state: tauri::State<'_, crate::Runtime>,
    request_id: String,
    allow: bool,
) -> Result<(), AppError> {
    let decision = if allow {
        PermissionDecision::AllowOnce
    } else {
        PermissionDecision::Deny
    };
    if state.engine.respond_permission(&request_id, decision).await {
        Ok(())
    } else {
        Err(AppError::plain(format!(
            "Permission request {request_id} is no longer pending."
        )))
    }
}

/// The usage gauge, its drop-up, and Settings › Usage all read this one summary.
///
/// `window` defaults to today; the chart it carries always covers the last 30 days so
/// switching windows never re-renders a different-shaped graph.
#[tauri::command]
#[specta::specta]
pub async fn get_usage_summary(
    state: tauri::State<'_, crate::Runtime>,
    window: Option<UsageWindow>,
    refresh_accounts: Option<bool>,
) -> Result<UsageSummary, AppError> {
    let state = state.inner();
    let registry = state.registry.read().await.clone();
    let accounts = {
        let mut cache = state.account_usage.lock().await;
        cache
            .refresh(&registry.providers, refresh_accounts.unwrap_or(false))
            .await;
        cache.snapshot()
    };
    let ledger = state.usage.load().await.map_err(AppError::from)?;
    let config = state.config.load().await.map_err(AppError::from)?;
    Ok(summarise_with_accounts(
        &ledger,
        &config.budget,
        &registry.providers,
        &registry.default_id,
        window.unwrap_or_default(),
        chrono::Local::now(),
        &accounts,
    ))
}

/// Sets one provider's daily token ceiling, or clears it back to the shared default.
///
/// `Some(0)` is rejected rather than silently meaning "uncapped" — the caller says what
/// it means, and an accidental zero should not quietly switch the gauge off.
#[tauri::command]
#[specta::specta]
pub async fn set_provider_token_cap(
    state: tauri::State<'_, crate::Runtime>,
    provider_id: String,
    daily_tokens: Option<u64>,
) -> Result<UsageSummary, AppError> {
    if matches!(daily_tokens, Some(0)) {
        return Err(AppError {
            message: "A cap of zero would block every call.".to_owned(),
            hint: Some("Clear the cap instead of setting it to zero.".to_owned()),
        });
    }
    let mut config = state.config.load().await.map_err(AppError::from)?;
    match daily_tokens {
        Some(cap) => {
            config.budget.provider_token_caps.insert(provider_id, cap);
        }
        None => {
            config.budget.provider_token_caps.remove(&provider_id);
        }
    }
    state.config.save(&config).await.map_err(AppError::from)?;
    get_usage_summary(state, None, None).await
}

/// Sets the calendar-month spend ceiling in USD across every metered provider, or clears
/// it with `None` (SPA-003). The composer card that blocks sending reads it back through
/// the summary, so the reply is the summary.
#[tauri::command]
#[specta::specta]
pub async fn set_monthly_spend_cap(
    state: tauri::State<'_, crate::Runtime>,
    monthly_usd: Option<f64>,
) -> Result<UsageSummary, AppError> {
    let cap = monthly_usd.unwrap_or(0.0);
    if !cap.is_finite() || cap < 0.0 {
        return Err(AppError {
            message: "A spend limit must be a dollar amount of zero or more.".to_owned(),
            hint: Some("Clear the field to remove the limit.".to_owned()),
        });
    }
    let mut config = state.config.load().await.map_err(AppError::from)?;
    config.budget.monthly_usd_cap = cap;
    state.config.save(&config).await.map_err(AppError::from)?;
    get_usage_summary(state, None, None).await
}

/// Clears recorded usage — one provider, or the whole ledger when `provider_id` is None.
/// The user's explicit click **is** the permission for this destructive action.
#[tauri::command]
#[specta::specta]
pub async fn clear_usage(
    state: tauri::State<'_, crate::Runtime>,
    provider_id: Option<String>,
) -> Result<UsageSummary, AppError> {
    state
        .usage
        .clear(provider_id.as_deref())
        .await
        .map_err(AppError::from)?;
    get_usage_summary(state, None, None).await
}

/// The context-telemetry panel: what each turn's prompt carried, by category.
///
/// `window` defaults to today; the summary is read straight from the local sample
/// log (`~/.bhippi/context.json`) and never includes message or source content.
#[tauri::command]
#[specta::specta]
pub async fn get_context_summary(
    state: tauri::State<'_, crate::Runtime>,
    window: Option<ContextWindow>,
) -> Result<ContextSummary, AppError> {
    let log = state.context.load().await.map_err(AppError::from)?;
    Ok(summarise(
        &log,
        window.unwrap_or_default(),
        chrono::Local::now(),
    ))
}

/// Clears the whole context-telemetry history.
#[tauri::command]
#[specta::specta]
pub async fn clear_context_samples(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<(), AppError> {
    state.context.clear().await.map_err(AppError::from)?;
    Ok(())
}

/// Remembers the provider chosen in the composer so it is still chosen next launch.
///
/// Only an id the runtime can actually answer with is stored: remembering a backend that
/// has since been switched off would restore a dead choice. `None` forgets it.
#[tauri::command]
#[specta::specta]
pub async fn set_active_provider(
    state: tauri::State<'_, crate::Runtime>,
    provider_id: Option<String>,
) -> Result<(), AppError> {
    let chosen = match provider_id {
        Some(id) => {
            if !state.registry.read().await.by_id.contains_key(&id) {
                return Err(AppError::plain(format!("provider {id} is not available")));
            }
            Some(id)
        }
        None => None,
    };

    let mut config = state.config.load().await.map_err(AppError::from)?;
    let previous = config.providers.last_provider.clone();
    config.providers.last_provider = chosen.clone();
    // The preference is written before the eject: which backend the user picked is the
    // thing that must survive, and an unreachable local server must never be able to
    // block a switch away from itself.
    state.config.save(&config).await.map_err(AppError::from)?;

    release_local_model(state.inner(), previous.as_deref(), chosen.as_deref()).await;
    Ok(())
}

/// Frees the RAM a local model was holding, once the user has moved off it.
///
/// A loaded 7B model is roughly 5 GB and a 70B is most of a workstation. Once the user is
/// answering from a cloud backend, that memory is doing nothing but making their machine
/// slower, so the server that holds it is asked to let go.
///
/// Everything here is best-effort and silent on failure. Ejection is a courtesy to the
/// user's memory, never a precondition for the switch that triggered it — a local server
/// that has already been closed by hand must not produce an error about a switch that
/// has, from the user's point of view, already succeeded.
async fn release_local_model(state: &crate::Runtime, previous: Option<&str>, chosen: Option<&str>) {
    let Some(previous) = previous else {
        return;
    };
    // Staying on the same backend is not a switch, and re-picking a provider must never
    // unload the model the user is about to use.
    if chosen == Some(previous) {
        return;
    }

    let row = state
        .registry
        .read()
        .await
        .providers
        .iter()
        .find(|row| row.id == previous)
        .cloned();
    let Some(row) = row else {
        return;
    };
    if row.kind != bhippi_providers::ProviderKind::LocalServer {
        return;
    }
    // Only a port a probe actually answered on. Posting an unload to whatever happens to
    // be listening on a guessed port is not something to do speculatively.
    let Some(port) = row.detected_port else {
        return;
    };

    let model = row.models.first().cloned();
    let outcome = bhippi_providers::eject(&row.id, &row.label, port, model.as_deref()).await;
    if outcome.freed() {
        tracing::info!(provider = %row.id, "{}", outcome.describe());
    } else {
        tracing::debug!(provider = %row.id, "{}", outcome.describe());
    }
}

/// Remembers the model chosen for one provider so the composer restores it next launch.
///
/// `None` forgets the choice, which returns that provider to its own default.
#[tauri::command]
#[specta::specta]
pub async fn set_provider_model(
    state: tauri::State<'_, crate::Runtime>,
    provider_id: String,
    model: Option<String>,
) -> Result<(), AppError> {
    let mut config = state.config.load().await.map_err(AppError::from)?;
    match model
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
    {
        Some(name) => {
            config.providers.last_model.insert(provider_id, name);
        }
        None => {
            config.providers.last_model.remove(&provider_id);
        }
    }
    state.config.save(&config).await.map_err(AppError::from)
}

/// One composer preset as the UI edits it (GAD-017).
///
/// A mirror of `bhippi_core::TierPreset` rather than the type itself: the config crate sits
/// below specta, and the bindings must stay generated (INV-032).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TierPresetView {
    /// Catalogue provider id the tier answers with (`claude`, `ollama`, `demo`, …).
    pub provider: String,
    /// The model to select, or `None` to leave the provider on its own default.
    pub model: Option<String>,
    /// `fast` · `balanced` · `quality` · `ultra` — the composer's own vocabulary.
    pub effort: String,
}

/// The three tiers the composer offers as Quick / Balanced / Max chips.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct TiersView {
    pub quick: TierPresetView,
    pub balanced: TierPresetView,
    pub max: TierPresetView,
}

impl From<&bhippi_core::TierPreset> for TierPresetView {
    fn from(preset: &bhippi_core::TierPreset) -> Self {
        Self {
            provider: preset.provider.clone(),
            model: preset.model.clone(),
            effort: preset.effort.clone(),
        }
    }
}

impl From<TierPresetView> for bhippi_core::TierPreset {
    fn from(view: TierPresetView) -> Self {
        Self {
            provider: view.provider,
            model: view.model,
            effort: view.effort,
        }
    }
}

/// The three composer presets as stored in `config.toml`.
///
/// Nothing here is filtered against the live registry: a tier pointing at a backend that is
/// not usable renders **disabled with the reason** in the composer, and is never swapped for
/// another provider behind the user's back.
#[tauri::command]
#[specta::specta]
pub async fn get_tiers(state: tauri::State<'_, crate::Runtime>) -> Result<TiersView, AppError> {
    let config = state.config.load().await.map_err(AppError::from)?;
    Ok(TiersView {
        quick: (&config.tiers.quick).into(),
        balanced: (&config.tiers.balanced).into(),
        max: (&config.tiers.max).into(),
    })
}

/// Rewrites one tier row. `name` is `quick`, `balanced` or `max`.
///
/// # Errors
/// Fails for a name that is not a tier, and for a preset the config layer rejects (an empty
/// provider or an effort the composer cannot render).
#[tauri::command]
#[specta::specta]
pub async fn set_tier(
    state: tauri::State<'_, crate::Runtime>,
    name: String,
    tier: TierPresetView,
) -> Result<TiersView, AppError> {
    let mut config = state.config.load().await.map_err(AppError::from)?;
    if !config.tiers.set(&name, tier.into()) {
        return Err(AppError::new(
            format!("{name} is not a tier"),
            "Use one of quick, balanced or max.",
        ));
    }
    state.config.save(&config).await.map_err(AppError::from)?;
    Ok(TiersView {
        quick: (&config.tiers.quick).into(),
        balanced: (&config.tiers.balanced).into(),
        max: (&config.tiers.max).into(),
    })
}

/// Returns the current Computer Use status, permissions, and provider vision support matrix.
#[tauri::command]
#[specta::specta]
pub async fn get_computer_use_status(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<crate::computer::ComputerUseStatus, AppError> {
    let config = state.config.load().await.map_err(AppError::from)?;
    Ok(crate::computer::ComputerUseStatus {
        enabled: config.computer_use.enabled,
        full_access: config.computer_use.full_access,
        allowed_providers: config.computer_use.allowed_providers,
        supported_providers: crate::computer::provider_vision_matrix(),
    })
}

/// Enables or disables Computer Use in config.
#[tauri::command]
#[specta::specta]
pub async fn set_computer_use_enabled(
    state: tauri::State<'_, crate::Runtime>,
    enabled: bool,
) -> Result<(), AppError> {
    let mut config = state.config.load().await.map_err(AppError::from)?;
    config.computer_use.enabled = enabled;
    state.config.save(&config).await.map_err(AppError::from)
}

/// Toggles full PC access permission for Computer Use.
#[tauri::command]
#[specta::specta]
pub async fn set_computer_use_full_access(
    state: tauri::State<'_, crate::Runtime>,
    full_access: bool,
) -> Result<(), AppError> {
    let mut config = state.config.load().await.map_err(AppError::from)?;
    config.computer_use.full_access = full_access;
    state.config.save(&config).await.map_err(AppError::from)
}

// ── Blender over MCP (SPA-201 / SPA-204) ─────────────────────────────────────────────

/// What Settings › Integrations shows for Blender over MCP.
#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct BlenderMcpStatus {
    pub enabled: bool,
    pub command: String,
    pub args: Vec<String>,
    /// Where Blender was found, or nothing. Advisory: the server talks to an addon inside a
    /// running Blender, so a found binary is a hint that the rest can work.
    pub blender_path: Option<String>,
    /// Where the launcher (`uvx` by default) was found. Required.
    pub launcher_path: Option<String>,
    /// Enabled and the launcher exists.
    pub ready: bool,
    pub note: String,
    /// The backends that can host the server this turn.
    pub supported_providers: Vec<String>,
}

fn find_blender(explicit: Option<&str>) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(path) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        let candidate = PathBuf::from(path);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    if let Some(found) = crate::workspace::find_on_path("blender") {
        return Some(found);
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    for key in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(dir) = std::env::var_os(key) {
            roots.push(PathBuf::from(dir).join("Blender Foundation"));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(
            PathBuf::from(local)
                .join("Programs")
                .join("Blender Foundation"),
        );
    }
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let exe = entry.path().join("blender.exe");
            if exe.is_file() {
                found.push(exe);
            }
        }
    }
    // The newest version folder sorts last by name (`Blender 4.2` < `Blender 4.5`).
    found.sort();
    found.pop()
}

fn find_launcher(command: &str) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let direct = PathBuf::from(command);
    if direct.components().count() > 1 && direct.is_file() {
        return Some(direct);
    }
    if let Some(found) = crate::workspace::find_on_path(command) {
        return Some(found);
    }
    let exe = if cfg!(windows) {
        format!("{command}.exe")
    } else {
        command.to_owned()
    };
    let mut candidates: Vec<PathBuf> = Vec::new();
    for key in ["APPDATA", "LOCALAPPDATA"] {
        let Some(base) = std::env::var_os(key) else {
            continue;
        };
        let python = PathBuf::from(&base).join("Python");
        if let Ok(entries) = std::fs::read_dir(&python) {
            for entry in entries.flatten() {
                candidates.push(entry.path().join("Scripts").join(&exe));
            }
        }
        let programs = PathBuf::from(&base).join("Programs").join("Python");
        if let Ok(entries) = std::fs::read_dir(&programs) {
            for entry in entries.flatten() {
                candidates.push(entry.path().join("Scripts").join(&exe));
            }
        }
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        candidates.push(PathBuf::from(&home).join(".local").join("bin").join(&exe));
        candidates.push(PathBuf::from(&home).join(".cargo").join("bin").join(&exe));
    }
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn blender_status_of(cfg: &bhippi_core::BhippiConfig) -> BlenderMcpStatus {
    let blender = &cfg.mcp.blender;
    let blender_path = find_blender(blender.blender_path.as_deref());
    let launcher_path = find_launcher(&blender.command);
    let ready = blender.enabled && launcher_path.is_some();
    let note = if !blender.enabled {
        "Off. Turn it on and the agent may build props in Blender when the library has nothing that fits.".to_owned()
    } else if launcher_path.is_none() {
        format!(
            "`{}` was not found. Install uv (`pip install uv`) or point the command at the launcher.",
            blender.command
        )
    } else if blender_path.is_none() {
        "Launcher found; Blender itself was not. Install Blender and start it with the blender-mcp addon's server running before a turn needs it.".to_owned()
    } else {
        "Ready. Keep Blender open with the blender-mcp addon's server started; the agent attaches on turns that use Claude Code or Codex.".to_owned()
    };
    BlenderMcpStatus {
        enabled: blender.enabled,
        command: blender.command.clone(),
        args: blender.args.clone(),
        blender_path: blender_path.map(|path| crate::workspace::display_path(&path)),
        launcher_path: launcher_path.map(|path| crate::workspace::display_path(&path)),
        ready,
        note,
        supported_providers: vec!["claude".to_owned(), "codex".to_owned()],
    }
}

/// Blender over MCP, as Settings shows it.
#[tauri::command]
#[specta::specta]
pub async fn get_blender_mcp_status(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<BlenderMcpStatus, AppError> {
    let config = state.config.load().await.map_err(AppError::from)?;
    tokio::task::spawn_blocking(move || blender_status_of(&config))
        .await
        .map_err(|error| AppError::plain(format!("detection did not finish: {error}")))
}

/// Turns Blender over MCP on or off and, optionally, changes the launcher.
#[tauri::command]
#[specta::specta]
pub async fn set_blender_mcp(
    state: tauri::State<'_, crate::Runtime>,
    enabled: bool,
    command: Option<String>,
    args: Option<Vec<String>>,
) -> Result<BlenderMcpStatus, AppError> {
    let mut config = state.config.load().await.map_err(AppError::from)?;
    config.mcp.blender.enabled = enabled;
    if let Some(command) = command
        .map(|value| value.trim().to_owned())
        .filter(|v| !v.is_empty())
    {
        config.mcp.blender.command = command;
    }
    if let Some(args) = args {
        config.mcp.blender.args = args
            .into_iter()
            .map(|arg| arg.trim().to_owned())
            .filter(|arg| !arg.is_empty())
            .collect();
    }
    state.config.save(&config).await.map_err(AppError::from)?;
    tokio::task::spawn_blocking(move || blender_status_of(&config))
        .await
        .map_err(|error| AppError::plain(format!("detection did not finish: {error}")))
}

// ── Composer attachments ─────────────────────────────────────────────────────────────

/// The largest image an attachment chip will carry as an inline data URL.
///
/// The chip is a 56 px thumbnail, so the whole file crosses IPC only to be drawn small —
/// and every byte of it is base64 in the page's memory for as long as the draft lives.
/// Past this ceiling the chip falls back to the file card, which costs nothing; the
/// picture still reaches the model, because that path is the file's *path*, not its bytes.
pub const ATTACHMENT_PREVIEW_MAX_BYTES: u64 = 6 * 1024 * 1024;

/// What a chip above the composer draws for one picked file.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct AttachmentPreview {
    /// The file name, already trimmed of its directory — the page renders it, never
    /// derives it.
    pub name: String,
    pub size_bytes: u64,
    /// The size a person reads — `3 KB`, `1.2 MB`. Rendered by the same Rust helper the
    /// transcript's `Attached:` line uses, so the chip and the turn never disagree (R3).
    pub size_label: String,
    pub kind: AttachmentKind,
    /// A `data:` URL for an image inside [`ATTACHMENT_PREVIEW_MAX_BYTES`]; `None` for
    /// everything else, which is drawn as a file card instead.
    pub data_url: Option<String>,
}

/// Whether a chip draws a thumbnail or a file card. Decided in Rust from the extension so
/// the page never has to keep its own list (R3).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Image,
    File,
}

/// The `image/*` media type a data URL declares, by extension.
fn image_media_type(extension: &str) -> &'static str {
    match extension {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}

/// The whole of the command below, minus Tauri — so the classification and the cap are
/// testable against real temp files without standing a desktop runtime up.
pub fn attachment_preview_of(path: &std::path::Path) -> Result<AttachmentPreview, AppError> {
    let metadata = std::fs::metadata(path).map_err(|error| AppError {
        message: format!("That attachment could not be read: {error}"),
        hint: Some("Pick the file again — it may have been moved, renamed or deleted.".to_owned()),
    })?;
    if metadata.is_dir() {
        return Err(AppError::new(
            "That attachment is a folder, not a file.",
            "Pick the files inside it instead.",
        ));
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let size_bytes = metadata.len();
    let is_image = crate::chat::is_image_attachment(&name);
    let kind = if is_image {
        AttachmentKind::Image
    } else {
        AttachmentKind::File
    };
    // Only an image under the ceiling is worth carrying as bytes; everything else is a
    // card, and the model reads it from disk either way.
    let data_url = if is_image && size_bytes <= ATTACHMENT_PREVIEW_MAX_BYTES {
        let bytes = std::fs::read(path).map_err(|error| AppError {
            message: format!("That image could not be read: {error}"),
            hint: Some("Check the file is still there and readable.".to_owned()),
        })?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Some(format!(
            "data:{};base64,{encoded}",
            image_media_type(&extension)
        ))
    } else {
        None
    };
    Ok(AttachmentPreview {
        name,
        size_bytes,
        size_label: crate::chat::format_bytes(size_bytes),
        kind,
        data_url,
    })
}

/// What the composer needs to draw a chip for a file the user just picked.
///
/// The Tauri asset protocol is off, so a `file:` or `asset:` image can never load in the
/// page — a preview has to arrive as a data URL through a command, and this is it.
#[tauri::command]
#[specta::specta]
pub async fn attachment_preview(path: String) -> Result<AttachmentPreview, AppError> {
    // Reading and base64-encoding megabytes is exactly the CPU-bound work that must stay
    // off the async runtime (R6).
    tokio::task::spawn_blocking(move || attachment_preview_of(std::path::Path::new(&path)))
        .await
        .map_err(|error| AppError::plain(format!("The preview task failed: {error}")))?
}

/// A pasted image, saved where the model can read it, plus the chip the composer draws.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct PastedImage {
    pub path: String,
    pub preview: AttachmentPreview,
}

/// The largest bitmap the page may hand over as a paste. A 4K screenshot is a few MB as
/// PNG; anything past this is not a paste, it is a file the picker should attach.
pub const PASTED_IMAGE_MAX_BYTES: usize = 32 * 1024 * 1024;

/// The file extension for a clipboard media type, or `None` for anything not an image.
fn pasted_extension(media_type: &str) -> Option<&'static str> {
    match media_type.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

/// Where pasted images live: `bhippi/pasted` under the OS temp directory. The path rides
/// in the turn like a picked file's, so the provider reads it from disk the same way.
pub fn pasted_image_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("bhippi").join("pasted")
}

/// Write the bytes of a pasted image into `dir` under a fresh timestamped name.
pub fn save_pasted_image_to(
    dir: &std::path::Path,
    bytes: &[u8],
    media_type: &str,
) -> Result<PastedImage, AppError> {
    let extension = pasted_extension(media_type).ok_or_else(|| {
        AppError::new(
            format!("Only images can be pasted into the chat ({media_type})."),
            "Copy an image, or attach the file with the paperclip.",
        )
    })?;
    if bytes.is_empty() {
        return Err(AppError::plain("The clipboard image was empty."));
    }
    if bytes.len() > PASTED_IMAGE_MAX_BYTES {
        return Err(AppError::new(
            format!(
                "That image is {} — too large to paste.",
                crate::chat::format_bytes(bytes.len() as u64)
            ),
            "Save it to a file and attach it with the paperclip instead.",
        ));
    }
    std::fs::create_dir_all(dir).map_err(|error| {
        AppError::plain(format!("The paste folder could not be created: {error}"))
    })?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let mut path = dir.join(format!("pasted-{stamp}.{extension}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("pasted-{stamp}-{n}.{extension}"));
        n += 1;
    }
    std::fs::write(&path, bytes).map_err(|error| {
        AppError::plain(format!("The pasted image could not be saved: {error}"))
    })?;
    Ok(PastedImage {
        path: path.to_string_lossy().into_owned(),
        preview: attachment_preview_of(&path)?,
    })
}

/// Ctrl+V of a bitmap in the composer: the page sends the bytes it got from the
/// clipboard, Rust lands them in a file and answers with the same chip a picked file
/// gets, so the paste then rides in the turn exactly like an attachment.
#[tauri::command]
#[specta::specta]
pub async fn save_pasted_image(
    data_base64: String,
    media_type: String,
) -> Result<PastedImage, AppError> {
    tokio::task::spawn_blocking(move || {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_base64.trim())
            .map_err(|error| {
                AppError::plain(format!("The pasted image could not be decoded: {error}"))
            })?;
        save_pasted_image_to(&pasted_image_dir(), &bytes, &media_type)
    })
    .await
    .map_err(|error| AppError::plain(format!("The paste task failed: {error}")))?
}

/// Captures a live screen preview for testing vision and resolution.
#[tauri::command]
#[specta::specta]
pub async fn capture_screen_preview(
    _state: tauri::State<'_, crate::Runtime>,
) -> Result<crate::computer::ScreenCapture, AppError> {
    crate::computer::capture_screen()
        .await
        .map_err(AppError::plain)
}

/// Executes a Computer Use action (mouse, keyboard, scroll, drag, screenshot).
#[tauri::command]
#[specta::specta]
pub async fn execute_computer_action(
    state: tauri::State<'_, crate::Runtime>,
    action: crate::computer::ComputerAction,
) -> Result<crate::computer::ComputerActionResult, AppError> {
    let config = state.config.load().await.map_err(AppError::from)?;
    if !config.computer_use.enabled {
        return Err(AppError {
            message: "Computer Use is disabled in Settings.".to_owned(),
            hint: Some(
                "Enable Computer Use in Settings › Computer Use to perform this action.".to_owned(),
            ),
        });
    }
    if action.requires_full_access() && !config.computer_use.full_access {
        return Err(AppError {
            message: "Full PC Access is disabled for Computer Use.".to_owned(),
            hint: Some(
                "Enable Full PC Access in Settings › Computer Use before sending mouse or keyboard input."
                    .to_owned(),
            ),
        });
    }
    crate::computer::execute_action(action)
        .await
        .map_err(AppError::plain)
}

/// Returns the list of discovered and configured AI skills.
#[tauri::command]
#[specta::specta]
pub async fn list_skills(
    state: tauri::State<'_, crate::Runtime>,
    workspace: Option<String>,
) -> Result<Vec<bhippi_core::Skill>, AppError> {
    let ws_path = workspace.as_deref().map(std::path::Path::new);
    Ok(state.skills.list_skills(ws_path).await)
}

/// Sets the enabled state of a specific skill.
#[tauri::command]
#[specta::specta]
pub async fn set_skill_enabled(
    state: tauri::State<'_, crate::Runtime>,
    skill_id: String,
    enabled: bool,
) -> Result<(), AppError> {
    state
        .skills
        .set_skill_enabled(&skill_id, enabled)
        .await
        .map_err(AppError::plain)
}

/// Re-scans and imports skills from pre-installed AI apps (Claude, Codex, Antigravity, Cursor).
#[tauri::command]
#[specta::specta]
pub async fn import_external_skills(
    state: tauri::State<'_, crate::Runtime>,
    workspace: Option<String>,
) -> Result<Vec<bhippi_core::Skill>, AppError> {
    let ws_path = workspace.as_deref().map(std::path::Path::new);
    Ok(state.skills.list_skills(ws_path).await)
}

/// Clears all turns in the active conversation view.
#[tauri::command]
#[specta::specta]
pub async fn clean_conversation(
    state: tauri::State<'_, crate::Runtime>,
    conversation_id: String,
) -> Result<Option<crate::chat::ConversationView>, AppError> {
    let project_path = required_project_path(state.inner()).await?;
    Ok(state
        .engine
        .clean_conversation(&project_path, &conversation_id)
        .await)
}

/// Compacts conversation history into a concise summary to preserve token budget.
#[tauri::command]
#[specta::specta]
pub async fn compact_conversation(
    state: tauri::State<'_, crate::Runtime>,
    conversation_id: String,
) -> Result<Option<crate::chat::ConversationView>, AppError> {
    let project_path = required_project_path(state.inner()).await?;
    Ok(state
        .engine
        .compact_conversation(&project_path, &conversation_id)
        .await)
}

/// Queries the git review diff summary for the active project and optional turn.
#[tauri::command]
#[specta::specta]
pub async fn get_review_changes(
    state: tauri::State<'_, crate::Runtime>,
    workspace: Option<String>,
    turn_title: Option<String>,
) -> Result<crate::review::ReviewSummary, AppError> {
    let project_path = match workspace {
        Some(w) => std::path::PathBuf::from(w),
        None => match active_project_path(state.inner()).await? {
            Some(p) => std::path::PathBuf::from(p),
            None => std::path::PathBuf::from("."),
        },
    };
    crate::review::collect_review_changes(&project_path, turn_title).await
}

#[tauri::command]
#[specta::specta]
pub async fn list_plugins(
    state: tauri::State<'_, crate::Runtime>,
) -> Result<Vec<PluginMetadata>, AppError> {
    crate::plugins::list(state.inner()).await
}

/// Installs a catalogue entry by id, or any other reference as a URL-backed plugin.
/// The user's click on Install **is** the permission for this.
#[tauri::command]
#[specta::specta]
pub async fn install_plugin(
    state: tauri::State<'_, crate::Runtime>,
    plugin_url: String,
) -> Result<(), AppError> {
    crate::plugins::install(state.inner(), &plugin_url).await
}

/// Removes a plugin. Built-ins refuse — they switch off instead.
#[tauri::command]
#[specta::specta]
pub async fn uninstall_plugin(
    state: tauri::State<'_, crate::Runtime>,
    plugin_id: String,
) -> Result<(), AppError> {
    crate::plugins::uninstall(state.inner(), &plugin_id).await
}

/// Moves an installed plugin up to the catalogue's version.
#[tauri::command]
#[specta::specta]
pub async fn update_plugin(
    state: tauri::State<'_, crate::Runtime>,
    plugin_id: String,
) -> Result<(), AppError> {
    crate::plugins::update(state.inner(), &plugin_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn activate_plugin(
    state: tauri::State<'_, crate::Runtime>,
    plugin_id: String,
) -> Result<(), AppError> {
    crate::plugins::set_enabled(state.inner(), &plugin_id, true).await
}

#[tauri::command]
#[specta::specta]
pub async fn deactivate_plugin(
    state: tauri::State<'_, crate::Runtime>,
    plugin_id: String,
) -> Result<(), AppError> {
    crate::plugins::set_enabled(state.inner(), &plugin_id, false).await
}

/// Git auto-update status returned to UI.
#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct GitUpdateStatus {
    pub update_available: bool,
    pub current_version: String,
    pub remote_version: String,
    pub current_commit: String,
    pub remote_commit: String,
    pub branch: String,
    pub commits_behind: u32,
    pub commit_message: Option<String>,
    pub error: Option<String>,
}

fn parse_version_numbers(v: &str) -> Vec<u64> {
    v.split(|c: char| c == '.' || c == '-' || c == '+' || !c.is_ascii_digit())
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}

fn is_newer_version(remote: &str, current: &str) -> bool {
    let r_nums = parse_version_numbers(remote);
    let c_nums = parse_version_numbers(current);
    if r_nums.is_empty() || c_nums.is_empty() {
        return false;
    }
    r_nums > c_nums
}

fn extract_version_from_cargo_toml(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("version =") {
            let parts: Vec<&str> = trimmed.split('=').collect();
            if parts.len() == 2 {
                let v = parts[1].trim().trim_matches('"').trim_matches('\'');
                return Some(v.to_owned());
            }
        }
    }
    None
}

/// Git auto-update result after installation.
#[derive(Clone, Debug, Deserialize, Serialize, Type)]
pub struct GitUpdateResult {
    pub success: bool,
    pub message: String,
    pub previous_commit: String,
    pub current_commit: String,
}

#[tauri::command]
#[specta::specta]
pub async fn check_app_update() -> Result<GitUpdateStatus, AppError> {
    let current_version = env!("CARGO_PKG_VERSION").to_owned();

    let current_commit = tokio::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|_| "unknown".to_owned());

    let branch = tokio::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|_| "main".to_owned());

    let fetch_res = tokio::process::Command::new("git")
        .args(["fetch", "origin", &branch])
        .output()
        .await;

    if let Err(e) = fetch_res {
        return Ok(GitUpdateStatus {
            update_available: false,
            current_version: current_version.clone(),
            remote_version: current_version,
            current_commit,
            remote_commit: "unknown".to_owned(),
            branch,
            commits_behind: 0,
            commit_message: None,
            error: Some(format!("Could not fetch from remote: {e}")),
        });
    }

    let remote_commit = tokio::process::Command::new("git")
        .args(["rev-parse", "--short", "FETCH_HEAD"])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default();

    let commits_behind: u32 = tokio::process::Command::new("git")
        .args(["rev-list", "--count", "HEAD..FETCH_HEAD"])
        .output()
        .await
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0);

    let commit_message = tokio::process::Command::new("git")
        .args(["log", "-1", "--pretty=%s", "FETCH_HEAD"])
        .output()
        .await
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty());

    let remote_cargo = tokio::process::Command::new("git")
        .args(["show", "FETCH_HEAD:Cargo.toml"])
        .output()
        .await
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let remote_version =
        extract_version_from_cargo_toml(&remote_cargo).unwrap_or_else(|| current_version.clone());

    // Only flag update if remote version is strictly higher than current version
    let update_available = is_newer_version(&remote_version, &current_version);

    Ok(GitUpdateStatus {
        update_available,
        current_version,
        remote_version,
        current_commit,
        remote_commit,
        branch,
        commits_behind,
        commit_message,
        error: None,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn install_app_update() -> Result<GitUpdateResult, AppError> {
    let previous_commit = tokio::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|_| "unknown".to_owned());

    let branch = tokio::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|_| "main".to_owned());

    let _ = tokio::process::Command::new("git")
        .args(["stash", "push", "-m", "bhippi-auto-update-stash"])
        .output()
        .await;

    let pull_output = tokio::process::Command::new("git")
        .args(["pull", "origin", &branch])
        .output()
        .await
        .map_err(|e| AppError::plain(format!("Git pull failed: {e}")))?;

    let _ = tokio::process::Command::new("git")
        .args(["stash", "pop"])
        .output()
        .await;

    let current_commit = tokio::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|_| "unknown".to_owned());

    if !pull_output.status.success() {
        let stderr = String::from_utf8_lossy(&pull_output.stderr);
        return Ok(GitUpdateResult {
            success: false,
            message: format!("Git pull reported errors:\n{stderr}"),
            previous_commit,
            current_commit,
        });
    }

    Ok(GitUpdateResult {
        success: true,
        message: "Successfully installed latest update from git repository.".to_owned(),
        previous_commit,
        current_commit,
    })
}

#[cfg(test)]
mod tests {
    use super::{attachment_preview_of, AttachmentKind, ATTACHMENT_PREVIEW_MAX_BYTES};

    #[test]
    fn a_pasted_png_lands_in_the_folder_with_its_chip() {
        let dir = std::env::temp_dir().join(format!("bhippi-paste-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        let saved = super::save_pasted_image_to(&dir, &png, "image/png").expect("saved");
        assert!(saved.path.ends_with(".png"));
        assert_eq!(saved.preview.kind, AttachmentKind::Image);
        assert!(saved
            .preview
            .data_url
            .as_deref()
            .unwrap_or("")
            .starts_with("data:image/png;base64,"));
        // A second paste in the same second must not overwrite the first.
        let again = super::save_pasted_image_to(&dir, &png, "image/png").expect("saved twice");
        assert_ne!(again.path, saved.path);
        // Not an image, and nothing at all, are both refused before anything is written.
        assert!(super::save_pasted_image_to(&dir, &png, "text/plain").is_err());
        assert!(super::save_pasted_image_to(&dir, &[], "image/png").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A scratch directory that takes itself away again, so a failed assertion cannot
    /// leave a 6 MB file behind in the user's temp folder.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("bhippi-attach-{label}-{}", ulid::Ulid::new()));
            std::fs::create_dir_all(&path).expect("a temp directory");
            Self(path)
        }

        fn write(&self, name: &str, bytes: &[u8]) -> std::path::PathBuf {
            let file = self.0.join(name);
            std::fs::write(&file, bytes).expect("a temp file");
            file
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ignored = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The classification the chip draws from, decided by extension in Rust so the page
    /// never keeps a second list that can drift from this one.
    #[test]
    fn a_small_image_previews_as_a_data_url_and_everything_else_as_a_card() {
        let scratch = Scratch::new("kinds");
        let png = scratch.write("shot.PNG", b"\x89PNG\r\n\x1a\nnot-really-a-png");
        let preview = attachment_preview_of(&png).expect("the image is readable");
        assert_eq!(preview.name, "shot.PNG");
        assert_eq!(preview.kind, AttachmentKind::Image);
        assert_eq!(preview.size_bytes, 24);
        assert_eq!(preview.size_label, "24 B");
        let url = preview.data_url.expect("a small image carries its bytes");
        assert!(url.starts_with("data:image/png;base64,"), "{url}");

        // The media type follows the extension, or the page renders a broken image.
        let jpeg = scratch.write("photo.jpeg", b"jpeg-bytes");
        let jpeg_url = attachment_preview_of(&jpeg)
            .expect("readable")
            .data_url
            .expect("an image");
        assert!(
            jpeg_url.starts_with("data:image/jpeg;base64,"),
            "{jpeg_url}"
        );

        // Anything not an image is a card: no bytes cross IPC at all.
        let notes = scratch.write("notes.txt", b"hello");
        let card = attachment_preview_of(&notes).expect("readable");
        assert_eq!(card.kind, AttachmentKind::File);
        assert_eq!(card.data_url, None);
        assert_eq!(card.size_label, "5 B");

        // No extension is a file, not an image — guessing would render a broken thumbnail.
        let bare = scratch.write("LICENSE", b"x");
        assert_eq!(
            attachment_preview_of(&bare).expect("readable").kind,
            AttachmentKind::File
        );
    }

    /// The cap is a real ceiling, not a warning: past it the chip is a card and the page
    /// never holds megabytes of base64 for a 56 px thumbnail. The picture still reaches
    /// the model, because that path is the file's path.
    #[test]
    fn an_image_over_the_cap_keeps_its_size_and_loses_only_its_thumbnail() {
        let scratch = Scratch::new("cap");
        let size = usize::try_from(ATTACHMENT_PREVIEW_MAX_BYTES).unwrap_or(usize::MAX) + 1;
        let huge = scratch.write("huge.png", &vec![0_u8; size]);
        let preview = attachment_preview_of(&huge).expect("readable");
        assert_eq!(preview.kind, AttachmentKind::Image, "it is still an image");
        assert_eq!(preview.data_url, None, "the cap has to block, not warn");
        assert_eq!(preview.size_bytes, ATTACHMENT_PREVIEW_MAX_BYTES + 1);

        // Exactly at the cap is inside it.
        let edge = scratch.write(
            "edge.png",
            &vec![0_u8; usize::try_from(ATTACHMENT_PREVIEW_MAX_BYTES).unwrap_or(usize::MAX)],
        );
        assert!(attachment_preview_of(&edge)
            .expect("readable")
            .data_url
            .is_some());
    }

    /// R1: a path that is gone answers with something the user can act on, not a panic
    /// and not a silent empty chip.
    #[test]
    fn a_missing_or_unreadable_path_is_a_typed_error_with_a_hint() {
        let scratch = Scratch::new("missing");
        let gone = scratch.0.join("never-existed.png");
        let error = attachment_preview_of(&gone).expect_err("a missing file cannot preview");
        assert!(error.message.contains("could not be read"), "{error:?}");
        assert!(
            error
                .hint
                .as_deref()
                .is_some_and(|hint| hint.contains("Pick the file again")),
            "{error:?}"
        );

        // A folder is the other way this goes wrong: the picker can hand one over.
        let folder = attachment_preview_of(&scratch.0).expect_err("a folder is not a file");
        assert!(folder.message.contains("folder"), "{folder:?}");
        assert!(folder.hint.is_some());
    }
}
