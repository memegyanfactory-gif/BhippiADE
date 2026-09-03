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
// Window-targeted Computer Use: watching and playing a game in its own native window.
pub mod computer_window;
mod context;
// Public so the integration test can drive the real session store rather than a copy of it.
pub mod engine;
// The typed error every command returns; public so integration tests can name it.
pub use commands::AppError;
mod files;
mod game_debug;
// The Godot process runner (ADR: Godot 4 runtime). Public so the IPC layer and the
// integration tests can drive the real runner rather than a copy of it.
pub mod godot;
// The AI ↔ Godot bridge (ADR-0043 §6): the streaming tag protocol, the query set over a
// Godot project, and the per-turn engine context. Public so the goldens can drive it.
pub mod godot_bridge;
// The Godot pane's IPC surface and its per-project session store. Public so the integration
// tests can exercise the session rules and the stderr parser without a Tauri runtime.
pub mod godot_commands;
/// The embedded Godot viewport: the editor and the game live inside Bhippi's window (ADR-0045).
pub mod godot_embed;
// The Computer Use playtest loop (ADR-0044): the game in a real window, watched and played.
// Public so the live test can drive the loop without a Tauri runtime.
pub mod godot_observe;
// The loopback static server the Preview button points the Browser pane at.
pub mod godot_preview;
// Versions, game settings and publish (GAD-083, GAD-022/023, GAD-092/094). Public so the
// revert planner and the settings rules can be tested without a Tauri runtime.
mod asset_library;
pub mod godot_versions;
mod overlay;
// Public so the catalogue merge can be unit-tested without a Tauri runtime.
pub mod plugins;
pub mod review;
mod status;
// What the Studio's bottom dock lists: the project's real assets, its scripts and the
// engine capability registry. Public so the classification can be tested without Tauri.
pub mod studio_dock;
pub mod terminal;
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
    activate_plugin, attachment_preview, capture_screen_preview, chat_turn_undoable,
    check_app_update, clean_conversation, clear_context_samples, clear_usage, compact_conversation,
    deactivate_plugin, delete_conversation, execute_computer_action, get_app_status,
    get_blender_mcp_status, get_computer_use_status, get_context_summary, get_conversation,
    get_review_changes, get_tiers, get_usage_summary, import_external_skills, install_app_update,
    install_plugin, install_provider, list_conversations, list_plugins, list_skills,
    list_workspace_sessions, new_conversation, regenerate_last_answer, rescan_providers,
    respond_permission, save_pasted_image, send_chat_message, set_active_provider, set_blender_mcp,
    set_computer_use_enabled, set_computer_use_full_access, set_monthly_spend_cap,
    set_provider_enabled, set_provider_model, set_provider_token_cap, set_skill_enabled, set_tier,
    stop_chat_turn, undo_chat_turn, uninstall_plugin, update_plugin,
};
use files::{
    import_workspace_file, list_workspace_dir, preview_targets, read_project_rules,
    read_workspace_file, write_project_rules, write_workspace_file,
};
use godot_commands::{
    check_system_dependencies, download_and_install_godot, godot_apply_batch, godot_create_project,
    godot_export, godot_export_template_offer, godot_export_templates_status, godot_gates,
    godot_list_scenes, godot_node, godot_open_editor, godot_output, godot_playtest,
    godot_preview_start, godot_preview_stop, godot_run, godot_scene_tree, godot_status, godot_stop,
    godot_undo_last, godot_visual_playtest, set_godot_path, GodotOutput, GodotProcessState,
    GodotSceneChanged, GodotSessionStore, GodotSessions,
};
use godot_versions::{
    game_card_info, game_settings_get, game_settings_set, godot_capture_poster,
    godot_create_version, godot_list_versions, godot_package_export, godot_publish_web,
    godot_reveal_export, godot_revert_to,
};
use std::path::PathBuf;
use std::sync::Arc;
use studio_dock::{list_capabilities, list_project_assets, list_project_scripts};
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
            get_app_status,
            list_plugins,
            activate_plugin,
            deactivate_plugin,
            install_plugin,
            uninstall_plugin,
            update_plugin,
            check_app_update,
            install_app_update,
            rescan_providers,
            set_provider_enabled,
            install_provider,
            list_conversations,
            list_workspace_sessions,
            new_conversation,
            get_conversation,
            delete_conversation,
            send_chat_message,
            attachment_preview,
            regenerate_last_answer,
            stop_chat_turn,
            undo_chat_turn,
            chat_turn_undoable,
            respond_permission,
            get_usage_summary,
            get_context_summary,
            clear_context_samples,
            set_provider_token_cap,
            save_pasted_image,
            set_monthly_spend_cap,
            set_provider_model,
            set_active_provider,
            get_tiers,
            set_tier,
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
            get_blender_mcp_status,
            set_blender_mcp,
            capture_screen_preview,
            execute_computer_action,
            list_skills,
            set_skill_enabled,
            import_external_skills,
            clean_conversation,
            compact_conversation,
            get_review_changes,
            run_cli_command,
            terminal::terminal_open,
            terminal::terminal_write,
            terminal::terminal_resize,
            terminal::terminal_close,
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
            // The Godot pane (ADR-0043 §5). Detection, the scene projection, the typed
            // action path, the four kinds of run, the gates and the preview server.
            godot_status,
            set_godot_path,
            check_system_dependencies,
            download_and_install_godot,
            godot_create_project,
            godot_scene_tree,
            godot_node,
            godot_list_scenes,
            godot_apply_batch,
            godot_undo_last,
            godot_run,
            godot_stop,
            godot_playtest,
            godot_visual_playtest,
            godot_export,
            godot_open_editor,
            godot_embed::godot_embed_open_workspace,
            godot_embed::godot_embed_play,
            godot_embed::godot_embed_stop,
            godot_embed::godot_embed_layout,
            godot_embed::godot_embed_state,
            godot_gates,
            godot_preview_start,
            godot_preview_stop,
            godot_export_templates_status,
            godot_export_template_offer,
            godot_output,
            godot_list_versions,
            godot_create_version,
            godot_revert_to,
            godot_reveal_export,
            godot_package_export,
            godot_publish_web,
            godot_capture_poster,
            game_settings_get,
            game_settings_set,
            game_card_info,
            // The Studio bottom dock (GAD-022): assets, scripts and the capability library.
            list_project_assets,
            list_project_scripts,
            list_capabilities,
            // The asset library (SPA-101): the user's folders, searched and imported from.
            asset_library::asset_library_list,
            asset_library::asset_library_add,
            asset_library::asset_library_remove,
            asset_library::asset_library_search,
            asset_library::asset_library_import,
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
            terminal::TerminalOutput,
            terminal::TerminalExited,
            GodotOutput,
            GodotProcessState,
            GodotSceneChanged,
            godot_embed::GodotEmbedState,
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
            // Terminals live outside `Runtime` because they own OS threads and PTY
            // handles rather than engine state, and they must be reachable from the
            // window-close handler that kills them.
            app.manage(Arc::new(terminal::TerminalRegistry::default()));
            app.manage(godot_embed::GodotEmbedHost::default());
            // Godot sessions live outside `Runtime` for the same reason terminals do: they
            // own child processes and a listening socket, and the window-close handler has
            // to be able to reach them to stop both.
            app.manage::<GodotSessionStore>(Arc::new(std::sync::Mutex::new(GodotSessions::new())));

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

    app.run(move |app_handle, event| {
        // The overlay (ADR-0019) is a second window, so closing the main one no longer
        // ends the process on its own: Tauri keeps running while any window is alive,
        // and the hidden overlay is. The main window *is* the app — when it goes,
        // everything goes with it, which raises `Exit` below and cleans up.
        if let tauri::RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::Destroyed,
            ..
        } = &event
        {
            if label == "main" {
                app_handle.exit(0);
            }
        }
        if let tauri::RunEvent::Exit = event {
            // A hard kill mid-turn must not leave the Windows arrow blanked.
            tauri::async_runtime::spawn(computer::restore_system_cursor());
            // Nor may it leave a shell running with no window attached to it: a PTY
            // child outlives its parent unless it is killed on the way out.
            if let Some(terminals) = app_handle.try_state::<Arc<terminal::TerminalRegistry>>() {
                terminals.shutdown();
            }
            // Nor a headless export, a game window or a preview socket with nothing left to
            // report to: a Godot child outlives its parent unless it is killed on the way out.
            if let Some(godot) = app_handle.try_state::<GodotSessionStore>() {
                if let Ok(mut sessions) = godot.lock() {
                    sessions.shutdown();
                }
            }
            if let Some(viewport) = app_handle.try_state::<godot_embed::GodotEmbedHost>() {
                godot_embed::shutdown(&viewport);
            }
        }
    });
}
