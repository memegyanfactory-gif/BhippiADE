//! Application adapter for the engine-owned `/gamedebug` pipeline.

use bhippi_engine::game_debug::{GameDebugMode, GameDebugReport, GameTestPlan, StageStatus};
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

#[cfg(test)]
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
    let mut report = bhippi_engine::game_debug::run(project_root, command.mode);
    let static_ready = report.stages.iter().all(|stage| {
        !matches!(
            stage.id.as_str(),
            "01_discover" | "02_validate" | "03_compile" | "06_inspect"
        ) || stage.status == StageStatus::Passed
    });
    if !static_ready {
        return store_report(project_root, command, report);
    }
    let manifest = bhippi_engine::manifest::load_manifest(project_root)
        .map_err(|error| format!("Could not load the validated game manifest: {error}"))?
        .ok_or_else(|| "Could not exercise a project without Bhippi.game.toml.".to_owned())?;
    let plan = GameTestPlan::mandatory_smoke(&manifest.game.default_scene)
        .map_err(|error| format!("Could not load the validated game-test plan: {error}"))?;
    let started = std::time::Instant::now();
    let batch_result: Result<String, String> = match app {
        Some(_) => Err(
            "Game test batch runtime execution in headless mode is not supported without a live Godot probe."
                .to_owned(),
        ),
        None => Err(
            "The Engine pane is unavailable in this headless session; no runtime evidence was fabricated."
                .to_owned(),
        ),
    };
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    match batch_result {
        Ok(evidence) => {
            if let Err(error) = bhippi_engine::game_debug::apply_game_test_batch_evidence(
                &mut report,
                &plan,
                &evidence,
                duration_ms,
            ) {
                bhippi_engine::game_debug::apply_runtime_failure(
                    &mut report,
                    &format!("The scenario-batch evidence was rejected: {error}"),
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
    let markdown = render_report(&report, command.fix_requested, Some(project_root));
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

pub fn render_report(
    report: &GameDebugReport,
    fix_requested: bool,
    project_root: Option<&Path>,
) -> String {
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
            let address = render_address(&finding.address, project_root);
            let _ignored = writeln!(
                output,
                "- **{} · {}** at {} — {}\n  - Evidence: {}\n  - Repair: {}",
                finding.severity.to_ascii_uppercase(),
                finding.code,
                address,
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
            "#### Runtime evidence\n\n- Protocol: `{}`\n- Execution: `{}`\n- Grants: {}\n- Budgets: instructions={}/tick and {} total, call depth={}, message={} bytes, rate={}/tick, spawned={}, events={}, logs={} bytes, timers={}, heap={} bytes, wall={} ms\n- Termination: `{}`\n- Authored snapshot: `{}` → `{}`\n- Exercise: {} frames, {} checkpoints, {} faults\n",
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
            runtime.budgets.timers,
            runtime.budgets.heap_estimate_bytes,
            runtime.budgets.wall_clock_millis,
            runtime.termination_reason,
            runtime.authored_hash_before,
            runtime.authored_hash_after,
            runtime.frames,
            runtime.checkpoint_hashes.len(),
            runtime.fault_count,
        );
        let usage = &runtime.trace.usage;
        let _ignored = writeln!(
            output,
            "- Trace: {} entries, {} redactions, truncated={}; usage instructions={}, messages={}, spawned={}, events={}, logs={} bytes, timers={}, heap={} bytes, wall={} ms",
            runtime.trace.entries.len(),
            runtime.trace.redactions,
            runtime.trace.truncated,
            usage.instructions,
            usage.messages,
            usage.spawned_entities,
            usage.emitted_events,
            usage.log_bytes,
            usage.timers,
            usage.heap_estimate_bytes,
            usage.wall_clock_millis,
        );
        for entry in &runtime.trace.entries {
            let detail = match entry.kind.as_str() {
                "capability" => format!(
                    "{} {}",
                    entry.capability.map(|c| c.as_str()).unwrap_or("unknown"),
                    entry.decision.as_deref().unwrap_or("unknown")
                ),
                "script_fault" => format!(
                    "{}:{} instruction {} — {}",
                    entry.subject.as_deref().unwrap_or("script"),
                    entry.line.unwrap_or(0),
                    entry.instruction.unwrap_or(0),
                    entry.message.as_deref().unwrap_or("fault")
                ),
                "log" => format!(
                    "{} — {}",
                    entry.subject.as_deref().unwrap_or("runtime"),
                    entry.message.as_deref().unwrap_or("log")
                ),
                _ => entry.message.clone().unwrap_or_else(|| entry.kind.clone()),
            };
            let _ignored = writeln!(output, "  - `{}`: {}", entry.kind, detail);
        }
        output.push('\n');
    }
    if let Some(batch) = &report.test_batch {
        let _ignored = writeln!(
            output,
            "#### Scenario batch evidence\n\n- Plan: `{}`\n- Batch: `{}`\n- Authored tree: `{}` → `{}`\n- Scenarios: {}\n",
            batch.plan_format,
            batch.format,
            batch.authored_tree_before,
            batch.authored_tree_after,
            batch.scenarios.len(),
        );
        for scenario in &batch.scenarios {
            let grants = if scenario.runtime.capabilities.is_empty() {
                "none".to_owned()
            } else {
                scenario
                    .runtime
                    .capabilities
                    .iter()
                    .map(|capability| format!("`{}`", capability.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let passed = scenario
                .assertions
                .iter()
                .filter(|assertion| assertion.passed)
                .count();
            let _ignored = writeln!(
                output,
                "##### `{}` · {}\n\n- Initial level: `{}`\n- Seed: `{}`\n- Worker identity: `{}`\n- Sandbox: `{}` via `{}`; grants {}; termination `{}`\n- Exercise: {} frames, {} checkpoints, {} faults\n- Assertions: {}/{} passed\n",
                scenario.name,
                if scenario.completed { "completed" } else { "failed" },
                scenario.initial_level,
                scenario.seed,
                scenario.worker_session_hash,
                scenario.runtime.protocol,
                scenario.runtime.execution,
                grants,
                scenario.runtime.termination_reason,
                scenario.runtime.frames,
                scenario.runtime.checkpoint_hashes.len(),
                scenario.runtime.fault_count,
                passed,
                scenario.assertions.len(),
            );
            for assertion in &scenario.assertions {
                let address = render_address(&assertion.address, project_root);
                let _ignored = writeln!(
                    output,
                    "- {} `{}` assertion {} at {} — observed `{}`",
                    if assertion.passed { "PASS" } else { "FAIL" },
                    assertion.checkpoint,
                    assertion.assertion_index,
                    address,
                    escape_markdown_label(&assertion.observed.to_string()),
                );
            }
            output.push('\n');
        }
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

fn render_address(address: &str, project_root: Option<&Path>) -> String {
    let Some((relative, explicit_line, locator)) = parse_file_address(address) else {
        return format!("`{}`", escape_markdown_label(address));
    };
    let line = explicit_line
        .or_else(|| project_root.and_then(|root| locate_line(root, relative, locator)))
        .unwrap_or(1);
    let encoded = percent_encode(relative);
    format!(
        "[`{}`](#bhippi-file={encoded}&line={line})",
        escape_markdown_label(address)
    )
}

fn parse_file_address(address: &str) -> Option<(&str, Option<u32>, Option<&str>)> {
    if address.contains("://") || address.starts_with('/') || address.contains('\\') {
        return None;
    }
    let (without_locator, locator) = address
        .split_once('#')
        .map_or((address, None), |(path, locator)| (path, Some(locator)));
    let (path, line) = without_locator
        .rsplit_once(':')
        .and_then(|(path, line)| line.parse::<u32>().ok().map(|line| (path, line)))
        .map_or((without_locator, None), |(path, line)| (path, Some(line)));
    let safe = !path.is_empty()
        && Path::new(path)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
    safe.then_some((path, line.filter(|line| *line > 0), locator))
}

fn locate_line(root: &Path, relative: &str, locator: Option<&str>) -> Option<u32> {
    let locator = locator?;
    let needle = if let Some(entity) = locator.strip_prefix("entity/") {
        entity.split('/').next().unwrap_or(entity)
    } else if let Some(binding) = locator.strip_prefix("binding/") {
        binding
    } else if locator == "settings.levels" {
        "\"levels\""
    } else {
        locator.rsplit('/').next().unwrap_or(locator)
    };
    if needle.is_empty() {
        return None;
    }
    let text = std::fs::read_to_string(root.join(relative)).ok()?;
    text.lines()
        .position(|line| line.contains(needle))
        .and_then(|index| u32::try_from(index + 1).ok())
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            let _ignored = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn escape_markdown_label(value: &str) -> String {
    value.replace('`', "\\`").replace(']', "\\]")
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
    use super::{parse_command, render_report, run_and_store, run_and_store_with_runtime};
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
    fn report_addresses_are_exact_workspace_links_without_linking_runtime_authority() {
        let root =
            std::env::temp_dir().join(format!("bhippi-game-debug-links-{}", ulid::Ulid::new()));
        bhippi_engine::scaffold::write_project(&root, "Linked Findings", false)
            .expect("fixture writes");
        let scene_path = root.join("assets/scenes/main.bscn.json");
        let scene = std::fs::read_to_string(&scene_path).expect("scene");
        let entity_id = report_entity_id(&scene);
        let entity = scene
            .lines()
            .find(|line| line.contains(entity_id))
            .expect("player id line");
        let expected_line = scene
            .lines()
            .position(|line| line == entity)
            .and_then(|index| u32::try_from(index + 1).ok())
            .expect("line fits");

        let mut report = bhippi_engine::game_debug::run(&root, GameDebugMode::Quick);
        report
            .findings
            .push(bhippi_engine::game_debug::GameDebugFinding {
                code: "BHP-GD-999".to_owned(),
                severity: "warning".to_owned(),
                stage: "06_inspect".to_owned(),
                address: format!(
                    "assets/scenes/main.bscn.json#entity/{}/Transform",
                    entity_id
                ),
                message: "fixture".to_owned(),
                evidence: "fixture".to_owned(),
                reproduction: "fixture".to_owned(),
                repair: "fixture".to_owned(),
            });
        report
            .findings
            .push(bhippi_engine::game_debug::GameDebugFinding {
                code: "BHP-GD-998".to_owned(),
                severity: "warning".to_owned(),
                stage: "06_inspect".to_owned(),
                address: "runtime://worker".to_owned(),
                message: "fixture".to_owned(),
                evidence: "fixture".to_owned(),
                reproduction: "fixture".to_owned(),
                repair: "fixture".to_owned(),
            });
        report
            .findings
            .sort_by(|left, right| left.code.cmp(&right.code));
        let markdown = render_report(&report, false, Some(&root));
        assert!(
            markdown.contains(&format!("&line={expected_line})")),
            "{markdown}"
        );
        assert!(markdown.contains("`runtime://worker`"), "{markdown}");
        assert!(!markdown.contains("#bhippi-file=runtime"), "{markdown}");
        let _ = std::fs::remove_dir_all(root);
    }

    fn report_entity_id(scene: &str) -> &str {
        let player_at = scene.find("\"name\": \"PlayerStart\"").expect("player");
        let prefix = &scene[..player_at];
        let id_key = prefix.rfind("\"id\": \"").expect("player id key") + 7;
        let rest = &scene[id_key..];
        &rest[..rest.find('"').expect("player id end")]
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
                    "timers": 4096,
                    "heapEstimateBytes": 67108864,
                    "wallClockMillis": 300000,
                    "messageBytes": 1024,
                    "messagesPerTick": 8,
                    "spawnedEntities": 8,
                    "emittedEvents": 8,
                    "logBytes": 1024
                },
                "terminationReason": "completed",
                "trace": {
                    "entries": [
                        { "kind": "capability", "capability": "entity_read", "decision": "denied" },
                        { "kind": "capability", "capability": "entity_write_runtime", "decision": "denied" },
                        { "kind": "capability", "capability": "entity_lifecycle", "decision": "denied" },
                        { "kind": "capability", "capability": "input_read", "decision": "denied" },
                        { "kind": "capability", "capability": "hud_action", "decision": "denied" },
                        { "kind": "capability", "capability": "level_travel", "decision": "denied" },
                        { "kind": "capability", "capability": "audio_event", "decision": "denied" },
                        { "kind": "capability", "capability": "deterministic_timer", "decision": "denied" }
                    ],
                    "truncated": false,
                    "redactions": 0,
                    "usage": {
                        "instructions": 0,
                        "messages": 2,
                        "spawnedEntities": 0,
                        "emittedEvents": 0,
                        "logBytes": 0,
                        "timers": 0,
                        "heapEstimateBytes": 512,
                        "wallClockMillis": 1
                    }
                }
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
        assert!(json.contains("\"trace\""));
        let markdown = std::fs::read_to_string(
            root.join(format!(".bhippi/reports/game-debug/{}.md", report.run_id)),
        )
        .expect("stored markdown report");
        // One trace entry per runtime capability; the fixture above denies all of them.
        assert!(markdown.contains("Trace: 8 entries"));
        assert!(markdown.contains("`capability`: entity_read denied"));
        let _ = std::fs::remove_dir_all(root);
    }
}
