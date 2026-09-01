//! Shared application command surface for the CLI and desktop clients.
//!
//! The desktop binary wires the chat engine (ADR-0006) into Tauri with typed IPC
//! bindings exported to `ui/src/lib/ipc.ts` in debug builds (INV-032). Provider
//! toggles persist in `~/.bhippi/config.toml`; enabled CLIs are silently kept
//! up to date at most once per day, failures logged but never surfaced.

#![cfg_attr(
    test,
    allow(clippy::expect_used, clippy::unwrap_used),
    doc = "Tests may panic on purpose: `expect` is how a test states its precondition, and a panic there is a failing test rather than a crashed app. The workspace `deny` stands everywhere else."
)]

mod brain;
mod chat;
mod commands;
pub mod computer;
mod context;
// Public so the integration test can drive the real scanner rather than a copy of it.
pub mod debugger;
// Public so the integration test can drive the real session store rather than a copy of it.
pub mod engine;
// The typed error every command returns; public so integration tests can name it.
pub use commands::AppError;
mod files;
mod game_debug;
mod overlay;
// Public so the catalogue merge can be unit-tested without a Tauri runtime.
pub mod plugins;
pub mod review;
mod status;
mod tiers;
// Public so the capture-baseline bin and the CLI can price the architecture.
pub mod token_baseline;
mod usage;
mod workspace;

use brain::{
    get_project_module_card, list_project_module_cards, project_brain_status,
    rebuild_project_brain, search_project_symbols, world_brain_asset_usage, world_brain_assets,
    world_brain_assets_by_kind, world_brain_find_entity, world_brain_index_assets,
    world_brain_index_scene, world_brain_physics, world_brain_physics_by_entity,
    world_brain_physics_by_scene, world_brain_scene_entities, world_brain_status,
};
use chat::{
    ChatDelta, ChatEngine, ChatLimits, ChatPermissionRequested, ChatThinking, ChatThoughtDelta,
    ChatTool, ChatTurnDone, ProviderInstallProgress, ProviderRuntime, ProvidersChanged,
    TauriEmitter,
};
use commands::{
    activate_plugin, capture_screen_preview, chat_turn_undoable, clean_conversation,
    clear_context_samples, clear_usage, compact_conversation, deactivate_plugin,
    delete_conversation, execute_computer_action, get_app_status, get_computer_use_status,
    get_context_summary, get_conversation, get_review_changes, get_tier_budgets, get_usage_summary,
    import_external_skills, install_plugin, install_provider, list_conversations, list_plugins,
    list_skills, list_workspace_sessions, new_conversation, regenerate_last_answer,
    rescan_providers, respond_permission, run_project_diagnostics, send_chat_message,
    set_active_provider, set_computer_use_enabled, set_computer_use_full_access,
    set_provider_enabled, set_provider_model, set_provider_token_cap, set_skill_enabled,
    stop_chat_turn, undo_chat_turn, uninstall_plugin, update_plugin,
};
use engine::{
    engine_agent_capabilities, engine_apply_action, engine_apply_batch, engine_begin_interaction,
    engine_cancel_interaction, engine_check_content, engine_clear_play_stats, engine_close_scene,
    engine_commit_interaction, engine_component_schema, engine_console_rows,
    engine_create_game_manifest, engine_history, engine_list_assets, engine_open_scene,
    engine_permission_mode, engine_play_world, engine_query_animation_graph,
    engine_query_asset_dependencies, engine_query_asset_users, engine_query_children,
    engine_query_components, engine_query_entity, engine_query_find_entities,
    engine_query_material_graph, engine_query_parent, engine_query_physics, engine_query_scene,
    engine_query_scene_view, engine_query_scripts, engine_query_shader, engine_record_console,
    engine_record_console_source, engine_record_interaction, engine_record_play_stats,
    engine_recover_scene, engine_redo, engine_reload_scene, engine_render_manifest,
    engine_save_all, engine_save_scene, engine_scene_diff, engine_set_agent_capability,
    engine_set_selection, engine_submit_game_test_batch, engine_submit_playtest,
    engine_submit_screenshot, engine_templates, engine_undo, engine_undo_journalled,
    engine_weather_presets, get_engine_status, hud_apply, hud_apply_many, hud_open, hud_redo,
    hud_reload, hud_save, hud_select, hud_undo, hud_widget_catalog, set_engine_permission_mode,
    EngineGameTestBatchRequested, EnginePlaytestRequested, EngineSceneChanged,
    EngineScreenshotRequested, HudChanged,
};
use files::{
    import_workspace_file, list_workspace_dir, preview_targets, read_project_rules,
    read_workspace_file, write_project_rules, write_workspace_file,
};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tauri_specta::Event;
use tokio::sync::{Mutex, RwLock};
use workspace::{
    add_existing_project, clone_project, create_project, forget_project, initialize_project_git,
    list_projects, open_external_terminal, open_external_url, open_project_in, project_tools,
    run_cli_command, select_project,
};

/// Everything the commands need, handed to Tauri as managed state.
pub struct Runtime {
    pub engine: Arc<ChatEngine>,
    pub registry: RwLock<Arc<ProviderRuntime>>,
    pub app_handle: tauri::AppHandle,
    pub config: Arc<bhippi_core::ConfigStore>,
    pub usage: Arc<bhippi_core::UsageStore>,
    pub context: Arc<bhippi_core::ContextSampleStore>,
    pub account_usage: Arc<Mutex<usage::AccountUsageCache>>,
    pub skills: Arc<bhippi_core::SkillStore>,
    /// Project Brain storage (structure/embedding index + module cards) shared by
    /// all brain IPC commands. Opened once at `~/.bhippi/brain.db`; `None` means the
    /// database could not be opened (commands report a friendly error).
    pub brain_db: Arc<Option<bhippi_db::Database>>,
    /// One detect at a time. Overlapping ticks used to spawn the same CLIs a chat turn
    /// needed, which is how the picker looked connected while send failed.
    rescan_lock: Mutex<()>,
}

impl Runtime {
    /// The user's toggle list; empty when the config cannot be read (logged upstream).
    pub async fn enabled_ids(&self) -> Vec<String> {
        match self.config.load().await {
            Ok(config) => config.providers.enabled,
            Err(error) => {
                tracing::warn!(%error, "config unreadable; provider toggles default off");
                Vec::new()
            }
        }
    }

    /// Re-detects backends honouring saved toggles, swaps the runtime, notifies the UI.
    ///
    /// Local servers that come online while the app is running are auto-enabled so they
    /// appear in the chat picker without requiring a manual toggle. Cloud providers and
    /// CLIs are never auto-enabled — those require explicit opt-in.
    pub async fn rescan_quietly(&self) {
        self.rescan(true).await;
    }

    /// Port probes only. Does not spawn CLI `--version` or `models`.
    async fn rescan_local_quietly(&self) {
        self.rescan(false).await;
    }

    async fn rescan(&self, full: bool) {
        let Ok(_busy) = self.rescan_lock.try_lock() else {
            tracing::debug!("provider rescan already in flight; skip");
            return;
        };

        let mut enabled = self.enabled_ids().await;
        let previous = self.registry.read().await.providers.clone();
        let mut detected = if full {
            bhippi_providers::detect(bhippi_providers::CATALOG, &enabled).await
        } else if previous.is_empty() {
            tracing::debug!("skipping local-server probe until the first full detection lands");
            return;
        } else {
            let locals =
                bhippi_providers::detect_local_servers(bhippi_providers::CATALOG, &enabled).await;
            bhippi_providers::merge_detection(&previous, &locals)
        };

        // Auto-enable local servers that just came online and were not previously toggled.
        // This makes Ollama and friends appear in the chat picker the moment they start.
        let mut changed = false;
        for row in &detected {
            if row.kind == bhippi_providers::ProviderKind::LocalServer
                && row.installed
                && matches!(row.health, bhippi_types::Health::Healthy { .. })
                && !enabled.iter().any(|id| id == &row.id)
            {
                tracing::info!(provider = %row.id, "auto-enabling local server that came online");
                enabled.push(row.id.clone());
                changed = true;
            }
        }

        // Persist the newly enabled IDs so the toggle stays on across restarts.
        if changed {
            if let Ok(mut config) = self.config.load().await {
                config.providers.enabled = enabled.clone();
                let _ignored = self.config.save(&config).await;
            }
        }

        // Detection copies the *old* toggle list onto rows. Stamp the list we actually
        // want — including auto-enabled locals — or they stay unusable until the next scan.
        bhippi_providers::stamp_enabled(&mut detected, &enabled);

        if !changed
            && bhippi_providers::detection_fingerprint(&detected)
                == bhippi_providers::detection_fingerprint(&previous)
        {
            tracing::debug!(
                usable = detected.iter().filter(|row| row.usable()).count(),
                "provider detection unchanged"
            );
            return;
        }

        let next = Arc::new(ProviderRuntime::from_detection(detected.clone()));
        tracing::info!(
            default = %next.default_id,
            usable = next.by_id.len(),
            full,
            "provider runtime rebuilt"
        );
        *self.registry.write().await = next;
        if let Err(error) = (ProvidersChanged {
            providers: detected,
        })
        .emit(&self.app_handle)
        {
            tracing::warn!(%error, "providers_changed delivery failed");
        }
    }

    /// Silently updates every **enabled + installed** CLI at most once per 24 h.
    ///
    /// Per the owner's direction this runs without a notification; outcomes land in the
    /// log only, and the picker refreshes quietly afterwards. Failures never block chat.
    async fn silent_update_sweep(&self) {
        let Ok(mut config) = self.config.load().await else {
            return;
        };
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        const INTERVAL: u64 = 60 * 60;
        if now < config.providers.last_auto_update.saturating_add(INTERVAL) {
            return;
        }
        config.providers.last_auto_update = now;
        let _ignored = self.config.save(&config).await;

        let installed_providers: Vec<(String, Option<String>)> = self
            .registry
            .read()
            .await
            .providers
            .iter()
            .filter(|row| row.installed)
            .map(|row| (row.id.clone(), row.version.clone()))
            .collect();

        for (id, version) in installed_providers {
            let Some(spec) = bhippi_providers::spec(&id) else {
                continue;
            };
            let Some(recipe) = spec.install else {
                continue;
            };
            let verdict = bhippi_providers::check_update(spec, version.as_deref()).await;
            if !verdict.should_install() {
                tracing::debug!(provider = %id, ?verdict, "auto-update not needed");
                continue;
            }
            tracing::info!(provider = %id, ?verdict, "auto-update starting");
            match bhippi_providers::run_recipe(&recipe).await {
                Ok(_tail) => tracing::info!(provider = %id, "silent auto-update finished"),
                Err(reason) => {
                    tracing::warn!(provider = %id, %reason, "silent auto-update skipped")
                }
            }
        }
        self.rescan_quietly().await;
    }
}

const WINDOW_LABEL: &str = "main";

/// Builds the specta collector. Shared by the desktop runtime and the bindings exporter
/// so the two can never drift.
fn ipc_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            engine_create_game_manifest,
            engine_query_scene,
            engine_query_scene_view,
            engine_query_entity,
            engine_query_find_entities,
            engine_query_components,
            engine_query_children,
            engine_query_parent,
            engine_query_scripts,
            engine_query_asset_users,
            engine_query_asset_dependencies,
            engine_query_material_graph,
            engine_query_shader,
            engine_query_animation_graph,
            engine_query_physics,
            engine_record_console,
            engine_record_console_source,
            engine_console_rows,
            engine_record_play_stats,
            engine_clear_play_stats,
            engine_apply_action,
            engine_apply_batch,
            engine_permission_mode,
            set_engine_permission_mode,
            engine_agent_capabilities,
            engine_set_agent_capability,
            engine_undo_journalled,
            engine_open_scene,
            engine_reload_scene,
            engine_scene_diff,
            engine_recover_scene,
            engine_close_scene,
            engine_save_scene,
            engine_save_all,
            engine_undo,
            engine_redo,
            engine_begin_interaction,
            engine_record_interaction,
            engine_commit_interaction,
            engine_cancel_interaction,
            engine_set_selection,
            engine_history,
            engine_weather_presets,
            engine_templates,
            engine_play_world,
            engine_check_content,
            engine_component_schema,
            engine_list_assets,
            engine_render_manifest,
            engine_submit_screenshot,
            engine_submit_playtest,
            engine_submit_game_test_batch,
            hud_open,
            hud_apply,
            hud_apply_many,
            hud_undo,
            hud_redo,
            hud_save,
            hud_reload,
            hud_select,
            hud_widget_catalog,
            get_app_status,
            get_engine_status,
            list_plugins,
            activate_plugin,
            deactivate_plugin,
            install_plugin,
            uninstall_plugin,
            update_plugin,
            rescan_providers,
            set_provider_enabled,
            install_provider,
            list_conversations,
            list_workspace_sessions,
            new_conversation,
            get_conversation,
            delete_conversation,
            send_chat_message,
            regenerate_last_answer,
            stop_chat_turn,
            undo_chat_turn,
            chat_turn_undoable,
            respond_permission,
            get_tier_budgets,
            get_usage_summary,
            get_context_summary,
            clear_context_samples,
            set_provider_token_cap,
            set_provider_model,
            set_active_provider,
            clear_usage,
            list_projects,
            add_existing_project,
            create_project,
            clone_project,
            select_project,
            forget_project,
            project_tools,
            open_project_in,
            initialize_project_git,
            list_workspace_dir,
            read_workspace_file,
            write_workspace_file,
            import_workspace_file,
            preview_targets,
            read_project_rules,
            write_project_rules,
            get_computer_use_status,
            set_computer_use_enabled,
            set_computer_use_full_access,
            capture_screen_preview,
            execute_computer_action,
            list_skills,
            set_skill_enabled,
            import_external_skills,
            run_project_diagnostics,
            clean_conversation,
            compact_conversation,
            get_review_changes,
            run_cli_command,
            open_external_terminal,
            open_external_url,
            project_brain_status,
            rebuild_project_brain,
            list_project_module_cards,
            get_project_module_card,
            search_project_symbols,
            world_brain_status,
            world_brain_scene_entities,
            world_brain_find_entity,
            world_brain_index_scene,
            world_brain_assets,
            world_brain_assets_by_kind,
            world_brain_asset_usage,
            world_brain_index_assets,
            world_brain_physics,
            world_brain_physics_by_scene,
            world_brain_physics_by_entity,
        ])
        .events(tauri_specta::collect_events![
            ChatThinking,
            ChatThoughtDelta,
            ChatDelta,
            ChatTool,
            ChatPermissionRequested,
            ChatTurnDone,
            ChatLimits,
            ProvidersChanged,
            ProviderInstallProgress,
            EngineSceneChanged,
            EngineScreenshotRequested,
            EnginePlaytestRequested,
            EngineGameTestBatchRequested,
            HudChanged,
        ])
}

fn bindings_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("ui")
        .join("src")
        .join("lib")
        .join("ipc.ts")
}

/// Writes `ui/src/lib/ipc.ts`; returns the path it landed on.
///
/// # Errors
/// Fails when the file cannot be rendered or written; CI treats this as a broken build.
pub fn export_bindings() -> std::result::Result<PathBuf, String> {
    let path = bindings_path();
    let header = "// @ts-nocheck\ntype Value = any;\n";
    ipc_builder()
        .export(
            specta_typescript::Typescript::default().header(header),
            &path,
        )
        .map_err(|error| error.to_string())?;
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    if !content.starts_with("// @ts-nocheck") {
        std::fs::write(&path, format!("{header}{content}")).map_err(|e| e.to_string())?;
    }
    Ok(path)
}

/// Starts the redacting JSONL log in `~/.bhippi/logs` and returns its guard.
///
/// The returned guard must outlive the app: dropping it stops the background writer, so
/// the caller holds it for the whole of [`run`]. Logging is best-effort — a desktop app
/// that will not start because it could not open a log file is worse than one running
/// unlogged — but without it every `tracing` call in this crate goes nowhere, which is
/// how a broken provider ends up with no evidence anywhere to explain it.
fn install_logging() -> Option<bhippi_core::LoggingGuard> {
    let dir = bhippi_core::ConfigStore::default_path()
        .ok()?
        .parent()?
        .join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    let guard = bhippi_core::LoggingGuard::new(&dir, bhippi_core::SecretRedactor::default())
        .map_err(|error| eprintln!("bhippi: logging unavailable: {error}"))
        .ok()?;
    guard
        .install_global()
        .map_err(|error| eprintln!("bhippi: logging unavailable: {error}"))
        .ok()?;
    Some(guard)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Immediately restore the Windows system cursor scheme in case a previous crash
    // or abnormal termination left it blanked or corrupted.
    tauri::async_runtime::spawn(computer::restore_system_cursor());

    // Held for the whole process: the guard owns the log writer's worker thread.
    let _logging = install_logging();

    #[allow(unused_mut)]
    let mut builder = ipc_builder();

    #[cfg(debug_assertions)]
    if let Err(error) = export_bindings() {
        eprintln!("failed to export IPC bindings: {error}");
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch focuses the existing window instead of starting a twin.
            if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
                let _ignored = window.show();
                let _ignored = window.set_focus();
            }
        }))
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            let handle = app.handle().clone();
            builder.mount_events(&handle);

            // Config lives at ~/.bhippi/config.toml per spec §5; fall back to a temp
            // path only so a broken HOME still yields a working (demo-only) session.
            let config_path = bhippi_core::ConfigStore::default_path()
                .unwrap_or_else(|_| std::env::temp_dir().join("bhippi-config.toml"));
            let config = Arc::new(bhippi_core::ConfigStore::new(config_path));

            // The token ledger lives beside the config so both survive a reinstall.
            let usage_path = bhippi_core::UsageStore::default_path()
                .unwrap_or_else(|_| std::env::temp_dir().join("bhippi-usage.json"));
            let usage = Arc::new(bhippi_core::UsageStore::new(usage_path));
            let account_usage = Arc::new(Mutex::new(usage::AccountUsageCache::default()));

            // Context telemetry lives beside the ledger: same directory, same survival.
            let context_path = bhippi_core::ContextSampleStore::default_path()
                .unwrap_or_else(|_| std::env::temp_dir().join("bhippi-context.json"));
            let context = Arc::new(bhippi_core::ContextSampleStore::new(context_path));

            // Discovered and custom AI skills store.
            let skills_path = bhippi_core::SkillStore::default_path()
                .unwrap_or_else(|| std::env::temp_dir().join("bhippi-skills.json"));
            let skills = Arc::new(bhippi_core::SkillStore::new(skills_path));

            // Project Brain storage lives beside the config so it survives a reinstall.
            // Fall back to a temp path (demo-only) rather than refusing to start.
            let brain_db_path = bhippi_core::ConfigStore::default_path()
                .ok()
                .and_then(|path| path.parent().map(|dir| dir.join("brain.db")))
                .unwrap_or_else(|| std::env::temp_dir().join("bhippi-brain.db"));
            let brain_db = Arc::new(tauri::async_runtime::block_on(async {
                match bhippi_db::Database::connect(&brain_db_path).await {
                    Ok(db) => Some(db),
                    Err(error) => {
                        tracing::warn!(%error, path = %brain_db_path.display(), "project brain database unavailable; trying fallback");
                        bhippi_db::Database::connect(
                            std::env::temp_dir().join("bhippi-brain-fallback.db"),
                        )
                        .await
                        .map_err(|error| {
                            tracing::error!(%error, "project brain database could not be opened at all");
                        })
                        .ok()
                    }
                }
            }));

            // Start demo-only; detection swaps real backends in within one probe budget.
            // Computer Use uses in-app chrome (OverlayGuard::inert) so Windows clicks
            // are never swallowed by a secondary full-screen webview.
            let engine = Arc::new(
                ChatEngine::new(TauriEmitter::new(handle.clone()))
                    .with_usage(usage.clone())
                    .with_context(context.clone())
                    .with_account_usage(account_usage.clone())
                    .with_config(config.clone())
                    .with_skills(skills.clone())
                    .with_desktop_overlay(handle.clone()),
            );
            // The engine journal (INV-071) is written from both the IPC commands and the
            // chat bridge, so it is registered process-wide rather than threaded through
            // the chat engine.
            if let Some(database) = brain_db.as_ref().as_ref() {
                engine::register_journal_db(database.clone());
            }

            app.manage(Runtime {
                engine,
                registry: RwLock::new(Arc::new(ProviderRuntime::from_detection(Vec::new()))),
                app_handle: handle.clone(),
                config,
                usage,
                context,
                account_usage,
                skills,
                brain_db,
                rescan_lock: Mutex::new(()),
            });

            let state_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                let Some(state) = state_handle.try_state::<Runtime>() else {
                    return;
                };
                state.rescan_quietly().await;
                state.silent_update_sweep().await;

                // Periodic re-detection: local-server *ports* only. A full detect also
                // runs every CLI `--version` / `models` (up to 20 s each), which held the
                // same binaries a chat turn needs and rebuilt the runtime every ~12 s.
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    state.rescan_local_quietly().await;
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .unwrap_or_else(|error| {
            eprintln!("bhippi desktop failed to start: {error}");
            std::process::exit(1);
        });

    // The Computer Use overlay (ADR-0019) is created hidden at startup and only shown while
    // a desktop turn runs. Missing UI assets must never take the app down.
    if let Err(error) = overlay::create_overlay_window(&app) {
        tracing::warn!(%error, "desktop overlay unavailable; Computer Use aura stays in-app only");
    }

    app.run(move |_app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            // A hard kill mid-turn must not leave the Windows arrow blanked.
            tauri::async_runtime::spawn(computer::restore_system_cursor());
        }
    });
}
