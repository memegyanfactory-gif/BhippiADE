//! The IPC command surface (spec §25). Every command is typed through specta so
//! `ui/src/lib/ipc.ts` stays generated, never hand-written (INV-032).

use crate::chat::{
    ConversationMeta, ConversationScope, ConversationView, DesignMode, Effort, PermissionDecision,
    ProviderInstallProgress, ProvidersChanged, TurnOptions, TurnPair, WorkspaceSession,
};
use crate::context::{summarise, ContextSummary, ContextWindow};
use crate::status::AppStatus;
use crate::tiers::{tier_budget_views, TierBudgetView};
use crate::usage::{summarise_with_accounts, UsageSummary, UsageWindow};
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
        queue_depth: 0,
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
) -> Result<TurnPair, AppError> {
    let text = text.trim().to_owned();
    if text.is_empty() {
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

#[tauri::command]
#[specta::specta]
pub async fn get_tier_budgets() -> Result<Vec<TierBudgetView>, AppError> {
    Ok(tier_budget_views())
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

/// Runs non-AI deterministic diagnostics and typechecking on the active workspace.
#[tauri::command]
#[specta::specta]
pub async fn run_project_diagnostics(
    state: tauri::State<'_, crate::Runtime>,
    workspace: Option<String>,
) -> Result<crate::debugger::DiagnosticReport, AppError> {
    let ws = match workspace {
        Some(w) => std::path::PathBuf::from(w),
        None => {
            let config = state.config.load().await.map_err(AppError::from)?;
            std::path::PathBuf::from(
                config
                    .workspace
                    .active_project
                    .unwrap_or_else(|| ".".to_owned()),
            )
        }
    };
    crate::debugger::run_diagnostics(&ws)
        .await
        .map_err(AppError::plain)
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
