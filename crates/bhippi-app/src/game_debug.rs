//! Application adapter for the engine-owned `/gamedebug` pipeline.

use bhippi_engine::game_debug::{GameDebugMode, GameDebugReport, StageStatus};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub struct GameDebugCommand {
    pub mode: GameDebugMode,
    pub fix_requested: bool,
}

pub fn parse_command(input: &str) -> Result<GameDebugCommand, String> {
    let mut parts = input.split_whitespace();
    if parts.next() != Some("/gamedebug") {
        return Err("This is not a /gamedebug command.".to_owned());
    }
    let mut mode = None;
    let mut fix_requested = false;
    for part in parts {
        match part {
            "quick" if mode.is_none() => mode = Some(GameDebugMode::Quick),
            "full" if mode.is_none() => mode = Some(GameDebugMode::Full),
            "release" if mode.is_none() => mode = Some(GameDebugMode::Release),
            "--fix" if !fix_requested => fix_requested = true,
            _ => {
                return Err(format!(
                    "Unknown game-debug option `{part}`. Use `/gamedebug [quick|full|release] [--fix]`."
                ))
            }
        }
    }
    Ok(GameDebugCommand {
        mode: mode.unwrap_or(GameDebugMode::Quick),
        fix_requested,
    })
}

pub fn run_and_store(
    project_root: &Path,
    command: &GameDebugCommand,
) -> Result<GameDebugReport, String> {
    let report = bhippi_engine::game_debug::run(project_root, command.mode);
    store_report(project_root, command, report)
}

pub fn run_and_store_with_runtime(
    project_root: &Path,
    command: &GameDebugCommand,
    runtime_result: Result<String, String>,
    duration_ms: u64,
) -> Result<GameDebugReport, String> {
    let mut report = bhippi_engine::game_debug::run(project_root, command.mode);
    match runtime_result {
        Ok(evidence) => {
            if let Err(error) = bhippi_engine::game_debug::apply_runtime_evidence(
                &mut report,
                &evidence,
                duration_ms,
            ) {
                bhippi_engine::game_debug::apply_runtime_failure(
                    &mut report,
                    &format!("The worker evidence was rejected: {error}"),
                    duration_ms,
                );
            }
        }
        Err(reason) => {
            bhippi_engine::game_debug::apply_runtime_failure(&mut report, &reason, duration_ms)
        }
    }
    store_report(project_root, command, report)
}

pub async fn run_and_store_observed(
    app: Option<&tauri::AppHandle>,
    project_root: &Path,
    command: &GameDebugCommand,
) -> Result<GameDebugReport, String> {
    if command.mode == GameDebugMode::Quick {
        return run_and_store(project_root, command);
    }
    let started = std::time::Instant::now();
    let runtime_result = match app {
        Some(app) => {
            let smoke = serde_json::json!({
                "steps": [{ "keys": [], "frames": 1, "note": "engine_smoke" }]
            });
            match crate::engine::playtest_steps(&smoke.to_string()) {
                Ok(steps) => crate::engine::request_playtest(app, project_root, steps)
                    .await
                    .map(|result| result.report)
                    .map_err(|error| observation_error(&error)),
                Err(error) => Err(observation_error(&error)),
            }
        }
        None => Err(
            "The Engine pane is unavailable in this headless session; no runtime evidence was fabricated."
                .to_owned(),
        ),
    };
    run_and_store_with_runtime(
        project_root,
        command,
        runtime_result,
        u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    )
}

fn observation_error(error: &crate::commands::AppError) -> String {
    match error.hint.as_deref() {
        Some(hint) => format!("{} Hint: {hint}", error.message),
        None => error.message.clone(),
    }
}

fn store_report(
    project_root: &Path,
    command: &GameDebugCommand,
    mut report: GameDebugReport,
) -> Result<GameDebugReport, String> {
    let report_dir = project_root.join(".bhippi/reports/game-debug");
    std::fs::create_dir_all(&report_dir).map_err(|error| {
        format!(
            "Could not create the game-debug report directory {}: {error}",
            report_dir.display()
        )
    })?;

    let json_name = format!("{}.json", report.run_id);
    let markdown_name = format!("{}.md", report.run_id);
    report.artifacts = vec![
        relative(project_root, &report_dir.join(&json_name)),
        relative(project_root, &report_dir.join(&markdown_name)),
    ];
    let markdown = render_report(&report, command.fix_requested);
    let json = report
        .dump()
        .map_err(|error| format!("Could not validate the game-debug report: {error}"))?
        .into_bytes();
    write_new_atomically(&report_dir.join(&json_name), &json)?;
    write_new_atomically(&report_dir.join(&markdown_name), markdown.as_bytes())?;

    let latest = serde_json::json!({
        "schema": "bhippi-game-debug-latest@1",
        "run_id": report.run_id,
        "report": json_name,
    });
    let latest_bytes = serde_json::to_vec_pretty(&latest)
        .map_err(|error| format!("Could not encode the latest-report pointer: {error}"))?;
    replace_pointer(&report_dir.join("latest.json"), &latest_bytes)?;
    prune_old_reports(&report_dir, &report.run_id)?;
    Ok(report)
}

pub fn render_report(report: &GameDebugReport, fix_requested: bool) -> String {
    let mut output = format!(
        "### Game Debug · {} · `{}`\n\n**{}** · schema `{}` · run `{}`\n\n",
        report.project,
        report.mode.as_str(),
        report.outcome.to_ascii_uppercase(),
        report.schema,
        report.run_id,
    );
    if report.authored_tree_unchanged() {
        output.push_str("Authored game files were byte-identical before and after this run.\n\n");
    } else {
        output.push_str("**BLOCKER:** authored game files changed during a read-only run.\n\n");
    }
    output.push_str("| Stage | Status | Time | Result |\n|---|---|---:|---|\n");
    for stage in &report.stages {
        let status = match stage.status {
            StageStatus::Passed => "passed",
            StageStatus::Failed => "failed",
            StageStatus::Skipped => "skipped",
            StageStatus::Unsupported => "unsupported",
        };
        let _ignored = writeln!(
            output,
            "| `{}` {} | **{}** | {} ms | {} |",
            stage.id, stage.label, status, stage.duration_ms, stage.summary
        );
    }
    output.push('\n');

    if report.findings.is_empty() {
        output.push_str("No static game findings.\n\n");
    } else {
        output.push_str("#### Findings\n\n");
        for finding in &report.findings {
            let _ignored = writeln!(
                output,
                "- **{} · {}** at `{}` — {}\n  - Evidence: {}\n  - Repair: {}",
                finding.severity.to_ascii_uppercase(),
                finding.code,
                finding.address,
                finding.message,
                finding.evidence,
                finding.repair,
            );
        }
        output.push('\n');
    }
    let _ignored = writeln!(
        output,
        "**Quality:** {} — {}\n\n**Sandbox:** {} — {}\n",
        report.quality.status, report.quality.reason, report.sandbox.status, report.sandbox.reason,
    );
    if let Some(runtime) = &report.runtime {
        let grants = if runtime.capabilities.is_empty() {
            "none".to_owned()
        } else {
            runtime
                .capabilities
                .iter()
                .map(|capability| format!("`{}`", capability.as_str()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ignored = writeln!(
            output,
            "#### Runtime evidence\n\n- Protocol: `{}`\n- Execution: `{}`\n- Grants: {}\n- Budgets: instructions={}/tick and {} total, call depth={}, message={} bytes, rate={}/tick, spawned={}, events={}, logs={} bytes\n- Termination: `{}`\n- Authored snapshot: `{}` → `{}`\n- Exercise: {} frames, {} checkpoints, {} faults\n",
            runtime.protocol,
            runtime.execution,
            grants,
            runtime.budgets.instructions_per_tick,
            runtime.budgets.instructions_total,
            runtime.budgets.call_depth,
            runtime.budgets.message_bytes,
            runtime.budgets.messages_per_tick,
            runtime.budgets.spawned_entities,
            runtime.budgets.emitted_events,
            runtime.budgets.log_bytes,
            runtime.termination_reason,
            runtime.authored_hash_before,
            runtime.authored_hash_after,
            runtime.frames,
            runtime.checkpoint_hashes.len(),
            runtime.fault_count,
        );
    }
    if fix_requested {
        output.push_str(
            "> `--fix` was requested, but automatic repair is not enabled in this first slice. No write transaction ran. This remains capability-gated work, never an alternate write path.\n\n",
        );
    }
    if !report.artifacts.is_empty() {
        output.push_str("**Saved reports**\n\n");
        for artifact in &report.artifacts {
            let _ignored = writeln!(output, "- `{artifact}`");
        }
    }
    output
}

fn write_new_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = temporary_path(path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("Could not create {}: {error}", temporary.display()))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("Could not write {}: {error}", temporary.display()));
    }
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("Could not publish {}: {error}", path.display())
    })
}

fn replace_pointer(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = temporary_path(path);
    write_synced(&temporary, bytes)?;
    match std::fs::rename(&temporary, path) {
        Ok(()) => return Ok(()),
        Err(error) if !path.exists() => {
            let _ignored = std::fs::remove_file(&temporary);
            return Err(format!("Could not publish {}: {error}", path.display()));
        }
        Err(_) => {}
    }

    // Unix replaces the pointer in the rename above. Windows rejects a rename onto an
    // existing file, so keep a recoverable previous pointer while swapping it.
    let backup = path.with_file_name(format!(".latest.json.{}.backup", ulid::Ulid::new()));
    std::fs::rename(path, &backup)
        .map_err(|error| format!("Could not preserve {}: {error}", path.display()))?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let restore = std::fs::rename(&backup, path);
        let _ignored = std::fs::remove_file(&temporary);
        return match restore {
            Ok(()) => Err(format!("Could not publish {}: {error}", path.display())),
            Err(restore_error) => Err(format!(
                "Could not publish {}: {error}; previous pointer is recoverable at {} but automatic restore failed: {restore_error}",
                path.display(),
                backup.display()
            )),
        };
    }
    std::fs::remove_file(&backup)
        .map_err(|error| format!("Could not remove {}: {error}", backup.display()))
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("Could not create {}: {error}", path.display()))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        let _ignored = std::fs::remove_file(path);
        return Err(format!("Could not write {}: {error}", path.display()));
    }
    Ok(())
}

fn prune_old_reports(report_dir: &Path, protected_run_id: &str) -> Result<(), String> {
    let mut run_ids = std::fs::read_dir(report_dir)
        .map_err(|error| format!("Could not list {}: {error}", report_dir.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let run_id = path.file_stem()?.to_str()?;
            (path.extension().and_then(|value| value.to_str()) == Some("json")
                && run_id != "latest"
                && report_dir.join(format!("{run_id}.md")).is_file())
            .then(|| run_id.to_owned())
        })
        .collect::<Vec<_>>();
    run_ids.sort();
    run_ids.dedup();
    let remove_count = run_ids
        .len()
        .saturating_sub(bhippi_types::ENGINE_GAME_DEBUG_RETAINED_RUNS);
    for run_id in run_ids
        .into_iter()
        .filter(|run_id| run_id != protected_run_id)
        .take(remove_count)
    {
        for extension in ["json", "md"] {
            let path = report_dir.join(format!("{run_id}.{extension}"));
            std::fs::remove_file(&path)
                .map_err(|error| format!("Could not prune {}: {error}", path.display()))?;
        }
    }
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("game-debug-report");
    path.with_file_name(format!(".{name}.{}.tmp", ulid::Ulid::new()))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::{parse_command, run_and_store, run_and_store_with_runtime};
    use bhippi_engine::game_debug::GameDebugMode;

    #[test]
    fn command_has_a_safe_quick_default_and_fixed_vocabulary() {
        let quick = parse_command("/gamedebug").expect("valid command");
        assert_eq!(quick.mode, GameDebugMode::Quick);
        assert!(!quick.fix_requested);
        let release = parse_command("/gamedebug release --fix").expect("valid command");
        assert_eq!(release.mode, GameDebugMode::Release);
        assert!(release.fix_requested);
        assert!(parse_command("/gamedebug magic").is_err());
    }

    #[test]
    fn report_pair_and_latest_pointer_are_written_outside_authored_content() {
        let root =
            std::env::temp_dir().join(format!("bhippi-game-debug-app-{}", ulid::Ulid::new()));
        bhippi_engine::scaffold::write_project(&root, "Game Debug App", false)
            .expect("fixture writes");
        let command = parse_command("/gamedebug quick").expect("valid command");
        let report = run_and_store(&root, &command).expect("report stores");
        assert!(report.authored_tree_unchanged());
        assert_eq!(report.artifacts.len(), 2);
        for path in &report.artifacts {
            assert!(root.join(path).is_file(), "missing {path}");
        }
        assert!(root
            .join(".bhippi/reports/game-debug/latest.json")
            .is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn report_store_keeps_a_bounded_number_of_complete_pairs() {
        let root =
            std::env::temp_dir().join(format!("bhippi-game-debug-retain-{}", ulid::Ulid::new()));
        bhippi_engine::scaffold::write_project(&root, "Retention", false).expect("fixture writes");
        let report_dir = root.join(".bhippi/reports/game-debug");
        std::fs::create_dir_all(&report_dir).expect("report dir");
        for index in 0..bhippi_types::ENGINE_GAME_DEBUG_RETAINED_RUNS {
            let run_id = format!("{index:026}");
            std::fs::write(report_dir.join(format!("{run_id}.json")), "{}").expect("old json");
            std::fs::write(report_dir.join(format!("{run_id}.md")), "old").expect("old markdown");
        }

        let command = parse_command("/gamedebug quick").expect("valid command");
        let current = run_and_store(&root, &command).expect("report stores");
        let json_runs = std::fs::read_dir(&report_dir)
            .expect("reports")
            .filter_map(Result::ok)
            .filter(|entry| {
                let path = entry.path();
                path.extension().and_then(|value| value.to_str()) == Some("json")
                    && path.file_name().and_then(|value| value.to_str()) != Some("latest.json")
            })
            .count();
        assert_eq!(json_runs, bhippi_types::ENGINE_GAME_DEBUG_RETAINED_RUNS);
        assert!(report_dir
            .join(format!("{}.json", current.run_id))
            .is_file());
        assert!(report_dir.join(format!("{}.md", current.run_id)).is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn full_report_persists_worker_protocol_budgets_and_hashes() {
        let root =
            std::env::temp_dir().join(format!("bhippi-game-debug-runtime-{}", ulid::Ulid::new()));
        bhippi_engine::scaffold::write_project(&root, "Runtime Evidence", false)
            .expect("fixture writes");
        let command = parse_command("/gamedebug full").expect("valid command");
        let evidence = serde_json::json!({
            "authoredUnchanged": true,
            "authoredHashBefore": "fnv1a32:12345678",
            "authoredHashAfter": "fnv1a32:12345678",
            "completed": true,
            "frames": 1,
            "samples": [{ "checkpointHash": "fnv1a32:abcdef01" }],
            "faults": [],
            "sandbox": {
                "protocol": "bhippi-runtime-protocol@1",
                "execution": "application_module_worker",
                "capabilities": [],
                "budgets": {
                    "instructionsPerTick": 200000,
                    "instructionsTotal": 20000000,
                    "callDepth": 64,
                    "messageBytes": 1024,
                    "messagesPerTick": 8,
                    "spawnedEntities": 8,
                    "emittedEvents": 8,
                    "logBytes": 1024
                },
                "terminationReason": "completed"
            }
        });
        let report = run_and_store_with_runtime(&root, &command, Ok(evidence.to_string()), 4)
            .expect("runtime report stores");
        assert_eq!(report.sandbox.status, "verified");
        assert!(report.runtime.is_some());
        let json = std::fs::read_to_string(
            root.join(format!(".bhippi/reports/game-debug/{}.json", report.run_id)),
        )
        .expect("stored report");
        assert!(json.contains("bhippi-runtime-protocol@1"));
        assert!(json.contains("messages_per_tick"));
        let _ = std::fs::remove_dir_all(root);
    }
}
