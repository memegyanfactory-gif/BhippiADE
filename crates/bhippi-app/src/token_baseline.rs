//! Baseline capture for the Token Engine (Phase A2–A6 of the plan).
//!
//! Phase A is "measurement first": before any token optimisation is allowed, the
//! current architecture has to be priced. This harness drives representative
//! tasks through the real turn pipeline on the offline demo provider and writes
//! what the report needs:
//!
//! - A2 — a per-category context budget for normal turns;
//! - A3 — tool-schema overhead (counted as zero today: no tool schemas are
//!   injected into requests, so the report measures that it *is* zero);
//! - A4 — multi-provider handoff overhead (measured both observed and, since the
//!   demo baseline never switches provider, from the injected note's template);
//! - A5 — repository-context overhead (workspace + project rules + engine map);
//! - A6 — a saved report (`docs/token-engine/baseline.md` + `baseline.json`).
//!
//! Everything runs on the deterministic offline demo provider, so the numbers are
//! reproducible on any machine without an API key or a network.
//!
//! Run:
//! ```text
//! cargo run -p bhippi-app --bin capture-baseline
//! ```

use crate::chat::{
    ChatEngine, ConversationScope, DesignMode, Effort, Emit, LimitSnapshot, PermissionDecision,
    PermissionRequest, ProviderRuntime, ToolActivity, TurnOptions,
};
use bhippi_core::{estimate_text_tokens, ContextCategory, ContextSampleStore};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// One representative task from the plan's "representative tasks" for A2.
pub struct TaskSpec {
    pub label: &'static str,
    pub text: &'static str,
    pub effort: Effort,
    pub design: DesignMode,
    /// When `Some`, this task continues that conversation instead of starting one.
    pub continue_conversation: Option<&'static str>,
}

/// The tasks the baseline runs, oldest first. They span the shapes a Bhippi turn
/// actually takes: a terse question, a research deep-dive, a code task against the
/// fixture repo, follow-ups that grow the history, and a design-system turn whose
/// brief is a large task directive.
pub fn representative_tasks() -> Vec<TaskSpec> {
    vec![
        TaskSpec {
            label: "short_question",
            text: "What is WebGPU and when should a desktop app choose it over WebGL?",
            effort: Effort::Fast,
            design: DesignMode::Off,
            continue_conversation: None,
        },
        TaskSpec {
            label: "research_deep_dive",
            text: "Explain how context caching changes the cost model of long-running agent sessions, covering prompt-caching tiers, cache invalidation, and where the token savings actually appear.",
            effort: Effort::Quality,
            design: DesignMode::Off,
            continue_conversation: None,
        },
        TaskSpec {
            label: "code_task",
            text: "Look at src/main.ts and src/components/Panel.tsx in this workspace. The Panel component has an unused prop and its tests never assert the empty state. Fix the prop handling, and add the missing test in tests/main.test.ts preserving the existing style.",
            effort: Effort::Balanced,
            design: DesignMode::Off,
            continue_conversation: None,
        },
        TaskSpec {
            label: "follow_up_one",
            text: "Now that the Panel component is fixed, summarise what changed and check whether the type error in src/main.ts is related.",
            effort: Effort::Balanced,
            design: DesignMode::Off,
            continue_conversation: Some("code_task"),
        },
        TaskSpec {
            label: "design_brief",
            text: "Redesign the settings page of this desktop app so every surface follows the Bhippi Design System. Call out the tokens and spacing you would apply.",
            effort: Effort::Ultra,
            design: DesignMode::On,
            continue_conversation: None,
        },
    ]
}

/// One task's measured row, with its category split.
#[derive(Clone, Debug)]
pub struct BaselinedTask {
    pub label: String,
    pub effort: &'static str,
    pub design: bool,
    pub estimated_total: u64,
    pub history_messages: u32,
    pub reserved_output: u64,
    pub stream_requests: u32,
    pub handoff: bool,
    pub categories: Vec<(String, u64)>,
}

/// Everything the baseline report prints.
#[derive(Clone, Debug)]
pub struct BaselineReport {
    pub tasks: Vec<BaselinedTask>,
    pub tool_schema_tokens: u64,
    pub handoff_note_tokens: u64,
    pub handoff_observed: usize,
    pub samples_json: PathBuf,
    pub report_md: PathBuf,
    pub workspace: PathBuf,
}

/// Runs every representative task into `output_dir/baseline.json` and writes
/// `output_dir/baseline.md`, returning the report.
///
/// # Errors
/// Fails when the fixture workspace cannot be written, a turn fails, or the report
/// cannot be written.
pub async fn capture_into(output_dir: &Path) -> Result<BaselineReport, String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|error| format!("cannot create {}: {error}", output_dir.display()))?;

    let workspace = fixture_workspace()?;
    let samples_path = output_dir.join("baseline.json");
    let workspace_path = workspace.to_string_lossy().into_owned();
    let store = Arc::new(ContextSampleStore::new(&samples_path));
    // A deterministic baseline starts from an empty log — a rerun must not inherit
    // earlier runs' samples.
    store
        .clear()
        .await
        .map_err(|error| format!("cannot reset the sample log: {error}"))?;
    let engine = Arc::new(ChatEngine::new(NoopEmitter).with_context(store.clone()));
    let registry = Arc::new(ProviderRuntime::from_detection(
        bhippi_providers::detect(&[], &["demo".to_owned()]).await,
    ));

    let mut conversation_ids: Vec<(String, usize)> = Vec::new();
    let mut measured = Vec::new();

    for task in representative_tasks() {
        // The conversation a follow-up continues is the one the referenced task opened.
        let inherited = task
            .continue_conversation
            .and_then(|label| {
                conversation_ids
                    .iter()
                    .find(|(_id, owner)| *owner == representative_labels()[label])
            })
            .map(|(id, _)| id.clone());
        let conversation_id = inherited
            .clone()
            .unwrap_or_else(|| bhippi_types::SessionId::new().to_string());
        if inherited.is_none() {
            conversation_ids.push((
                conversation_id.clone(),
                representative_labels()[&task.label],
            ));
        }

        let pair = engine
            .send(
                &registry,
                ConversationScope {
                    project_path: workspace_path.clone(),
                    conversation_id: conversation_id.clone(),
                },
                task.text.to_owned(),
                TurnOptions {
                    provider_id: Some("demo".to_owned()),
                    model: None,
                    effort: task.effort,
                    design: task.design,
                    caveman: false,
                    attachments: Vec::new(),
                },
            )
            .await
            .map_err(|error| format!("task {} failed to start: {error}", task.label))?;

        wait_for_turn(&engine, &workspace_path, &pair.assistant_turn_id).await?;

        let log = store
            .load()
            .await
            .map_err(|error| format!("cannot read the sample log: {error}"))?;
        let Some(sample) = log
            .samples
            .iter()
            .find(|sample| sample.turn_id == pair.assistant_turn_id)
            .cloned()
        else {
            return Err(format!(
                "task {} finished without a context sample (turn {})",
                task.label, pair.assistant_turn_id
            ));
        };

        let mut categories: Vec<(String, u64)> = sample
            .categories
            .iter()
            .map(|(category, tokens)| (category.as_str().to_owned(), *tokens))
            .collect();
        categories.sort_by_key(|(_, tokens)| std::cmp::Reverse(*tokens));

        measured.push(BaselinedTask {
            label: task.label.to_owned(),
            effort: effort_name(task.effort),
            design: task.design == DesignMode::On,
            estimated_total: sample.estimated_total,
            history_messages: sample.history_messages,
            reserved_output: sample.reserved_output,
            stream_requests: sample.stream_requests,
            handoff: sample.handoff,
            categories,
        });
    }

    let log = store
        .load()
        .await
        .map_err(|error| format!("cannot read the sample log: {error}"))?;
    let handoff_observed = log.samples.iter().filter(|sample| sample.handoff).count();

    // A3: the current architecture injects no tool schemas, so the answer *is* the
    // measurement. If schemas ever ship, they must subtract from this line.
    let tool_schema_tokens = 0;
    // A4: the note's template, measured because no single-provider run triggers it.
    let handoff_note_tokens = handoff_note_tokens();

    let report = BaselineReport {
        samples_json: samples_path.clone(),
        tasks: measured,
        tool_schema_tokens,
        handoff_note_tokens,
        handoff_observed,
        workspace,
        report_md: output_dir.join("baseline.md"),
    };

    let markdown = render_markdown(&report);
    std::fs::write(&report.report_md, markdown)
        .map_err(|error| format!("cannot write {}: {error}", report.report_md.display()))?;

    Ok(report)
}

fn representative_labels() -> std::collections::HashMap<&'static str, usize> {
    representative_tasks()
        .into_iter()
        .enumerate()
        .map(|(index, task)| (task.label, index))
        .collect()
}

fn effort_name(effort: Effort) -> &'static str {
    match effort {
        Effort::Fast => "fast",
        Effort::Balanced => "balanced",
        Effort::Quality => "quality",
        Effort::Ultra => "ultra",
    }
}

/// Polls until the assistant turn reaches a terminal state, or 60 s elapse.
async fn wait_for_turn(
    engine: &ChatEngine,
    workspace: &str,
    assistant_turn_id: &str,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let mut conversation_id: Option<String> = None;
        for meta in engine.list_conversations(workspace).await {
            if let Some(view) = engine.conversation_view(workspace, &meta.id).await {
                if view.turns.iter().any(|turn| turn.id == assistant_turn_id) {
                    conversation_id = Some(meta.id);
                    break;
                }
            }
        }
        let Some(conversation_id) = conversation_id else {
            return Err(format!(
                "turn {assistant_turn_id} is not in any conversation"
            ));
        };
        let Some(view) = engine.conversation_view(workspace, &conversation_id).await else {
            if tokio::time::Instant::now() >= deadline {
                return Err("conversation vanished before the turn finished".to_owned());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        };
        // Nobody answers the demo card in a batch harness, and its permission grace
        // period is ten minutes long (PERMISSION_TIMEOUT). Answer it ourselves so the
        // turn settles instead of parking on the card for the whole run.
        if let Some(permission) = view
            .turns
            .iter()
            .find(|turn| turn.id == assistant_turn_id)
            .and_then(|turn| turn.permission.as_ref())
        {
            let _answered = engine
                .respond_permission(&permission.id, PermissionDecision::Deny)
                .await;
        }
        let finished = view
            .turns
            .iter()
            .any(|turn| turn.id == assistant_turn_id && turn.state.is_terminal());
        if finished {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("turn did not finish within 60 s".to_owned());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The handoff note exactly as `run_turn` injects it, measured as tokens. Kept in
/// sync by the A4 test that also keys off this template's size.
fn handoff_note_tokens() -> u64 {
    let note = format!(
        "\n\n## 🔄 Multi-Provider Conversation Handoff\n\
         You are continuing an ongoing conversation session originally assisted by `{}`.\n\
         The previous turns are included above in the message history. Maintain full continuity, \
         respect all previously agreed decisions and code patterns, and seamlessly address the user's latest prompt.",
        "codex"
    );
    estimate_text_tokens(&note)
}

#[must_use]
pub fn render_markdown(report: &BaselineReport) -> String {
    let mut out = String::new();
    out.push_str("# Bhippi Token Engine — baseline\n\n");
    out.push_str("Captured on the offline demo provider; deterministic and rerunnable.\n\n");
    out.push_str("## Per-task context budget\n\n");
    out.push_str("| task | effort | design | history msgs | input est. | reserved output | stream reqs | handoff |\n");
    out.push_str("|---|---|---|---|---|---|---|---|\n");
    for task in &report.tasks {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            task.label,
            task.effort,
            if task.design { "on" } else { "off" },
            task.history_messages,
            task.estimated_total,
            task.reserved_output,
            task.stream_requests,
            if task.handoff { "yes" } else { "no" },
        ));
    }
    out.push('\n');

    out.push_str("## Category breakdown\n\n");
    out.push_str("| task | category | estimated tokens |\n|---|---|---|\n");
    for task in &report.tasks {
        for (category, tokens) in &task.categories {
            out.push_str(&format!("| {} | {} | {} |\n", task.label, category, tokens));
        }
    }
    out.push('\n');

    let totals = report
        .tasks
        .iter()
        .fold(0u64, |sum, task| sum.saturating_add(task.estimated_total));
    let mean_total = if report.tasks.is_empty() {
        0
    } else {
        totals / u64::try_from(report.tasks.len()).unwrap_or(u64::MAX)
    };
    out.push_str(&format!(
        "## Summary\n\n- Mean estimated input per task: **{mean_total} tokens**\n"
    ));
    out.push_str(&format!(
        "- Tool-schema overhead (A3): **{} tokens** — no tool schemas are injected into requests today, which is the measured fact.\n",
        report.tool_schema_tokens
    ));
    out.push_str(&format!(
        "- Multi-provider handoff overhead (A4): **{} observed turn(s)**; injecting the note adds **{} estimated tokens** when it fires.\n",
        report.handoff_observed, report.handoff_note_tokens
    ));
    out.push_str(
        "- Repository-context overhead (A5): see the workspace/project_rules/engine rows above — a summed mean folds in when Phase B compares against this baseline.\n",
    );
    out.push_str("\nSample log: `baseline.json`.\n");
    out
}

// =============================================================================================
// GAD-040 — engine-turn token baseline (Phase 4 §6.3 of docs/16-GAME-ADE-PLAN.md)
// =============================================================================================
//
// The plain baseline above prices a chat turn. An engine turn is a different animal: on top of
// the six-part system prompt (`run_turn`, chat.rs:2506-2589), a project with a `Bhippi.game.toml`
// manifest gets the *entire* `prompts/chat-engine.md` doctrine injected by `engine_context()`
// (chat.rs:4956-5000) — not just the capped per-project facts (ENG-191, already measured), the
// fixed doctrine text itself — and the turn can loop the model up to `ENGINE_AUTONOMY_MAX_ROUNDS`
// (chat.rs:3166) times before it settles. Phase 5 cannot know what it saved without this "before"
// number for both.
//
// This reuses the exact same harness as the baseline above: `ChatEngine` on the offline `demo`
// provider, `ContextSampleStore` recording one `ContextSample` per turn from the same strings the
// prompt was built from, and the same four-bytes-per-token estimator
// (`bhippi_core::estimate_text_tokens`). The only new ingredient is a real Godot project fixture —
// built at run time with `bhippi_engine::godot::scaffold::write_project`, the same scaffold
// `tests/godot_live.rs` checks against a real Godot — so `Bhippi.game.toml` exists and
// `engine_context()` takes the game-project path instead of returning empty.

/// The nine ENG-418 task ids, in the plan's order. `engine_task_specs()` must return exactly
/// these labels in this order (`engine_task_ids_match_the_plan` below pins it).
pub const ENGINE_TASK_IDS: [&str; 9] = [
    "bouncing_ball",
    "third_person_controller",
    "zombie_enemy",
    "health_hud",
    "rainy_weather",
    "small_fps_arena",
    "rainy_village_survival",
    "param_edit",
    "composition_edit",
];

const BALANCED_ONLY: &[Effort] = &[Effort::Balanced];
const BALANCED_AND_FAST: &[Effort] = &[Effort::Balanced, Effort::Fast];

/// One ENG-418 task: its prompt, which conversation it continues (if any), and the effort
/// level(s) it is measured at.
pub struct EngineTaskSpec {
    pub label: &'static str,
    pub text: &'static str,
    /// When `Some`, this task continues the conversation that task label opened.
    pub continue_conversation: Option<&'static str>,
    pub efforts: &'static [Effort],
}

/// The seven fresh-build ENG-418 prompts, verbatim, plus two follow-ups on the third-person
/// conversation. `param_edit` is also measured at `fast` — the plan's contrast case for a tiny
/// parameter tweak against the same doctrine a fresh build pays for; every other task is
/// `balanced` only.
pub fn engine_task_specs() -> Vec<EngineTaskSpec> {
    vec![
        EngineTaskSpec {
            label: "bouncing_ball",
            text: "add a bouncing ball to the scene",
            continue_conversation: None,
            efforts: BALANCED_ONLY,
        },
        EngineTaskSpec {
            label: "third_person_controller",
            text: "make the player a third-person character with a follow camera",
            continue_conversation: None,
            efforts: BALANCED_ONLY,
        },
        EngineTaskSpec {
            label: "zombie_enemy",
            text: "add a zombie enemy that chases the player",
            continue_conversation: None,
            efforts: BALANCED_ONLY,
        },
        EngineTaskSpec {
            label: "health_hud",
            text: "add a health bar to the HUD",
            continue_conversation: None,
            efforts: BALANCED_ONLY,
        },
        EngineTaskSpec {
            label: "rainy_weather",
            text: "make it rain",
            continue_conversation: None,
            efforts: BALANCED_ONLY,
        },
        EngineTaskSpec {
            label: "small_fps_arena",
            text: "build a small FPS arena with two enemies",
            continue_conversation: None,
            efforts: BALANCED_ONLY,
        },
        EngineTaskSpec {
            label: "rainy_village_survival",
            text: "build a rainy village survival game: gather wood, build a fire before night, survive three nights",
            continue_conversation: None,
            efforts: BALANCED_ONLY,
        },
        EngineTaskSpec {
            label: "param_edit",
            text: "make the jump 20% higher",
            continue_conversation: Some("third_person_controller"),
            efforts: BALANCED_AND_FAST,
        },
        EngineTaskSpec {
            label: "composition_edit",
            text: "add coins that play a sound when collected",
            continue_conversation: Some("third_person_controller"),
            efforts: BALANCED_ONLY,
        },
    ]
}

/// One measured (task, effort) run.
#[derive(Clone, Debug)]
pub struct EngineTaskRun {
    pub label: String,
    pub effort: &'static str,
    pub history_messages: u32,
    pub estimated_total: u64,
    pub context_window_tokens: u64,
    pub over_window: bool,
    pub reserved_output: u64,
    /// Provider requests this turn drove. Always `1` in this baseline — see the report's "what
    /// this harness could not measure" section for the confirmed reason.
    pub rounds_issued: u32,
    pub categories: Vec<(String, u64)>,
    /// `prompts/chat-engine.md` bytes/4, exactly as injected by `engine_context()`.
    pub engine_doctrine: u64,
    /// The `Engine` category minus the doctrine: the per-project map, selection and recent
    /// journal facts (ENG-191's capped budget).
    pub engine_facts: u64,
    /// Every category except `Conversation` and `ReservedResponse` — the part of the prompt
    /// that would be resent unchanged on every provider request within a turn (system,
    /// workspace, project rules, skills, computer use, engine, handoff, task directives).
    pub system_block_tokens: u64,
    /// `system_block_tokens * rounds_issued`.
    pub system_prefix_repeated_tokens: u64,
}

/// Everything the engine-turn baseline report prints.
#[derive(Clone, Debug)]
pub struct EngineBaselineReport {
    pub runs: Vec<EngineTaskRun>,
    pub studio_core_bytes: u64,
    pub studio_core_tokens: u64,
    pub chat_engine_bytes: u64,
    pub chat_engine_tokens: u64,
    pub captured_on: String,
    pub git_head: String,
    pub workspace: PathBuf,
    pub report_json: PathBuf,
    pub report_md: PathBuf,
}

/// `prompts/chat-engine.md`, included independently of `chat.rs`'s own `ENGINE_SYSTEM` so this
/// module can check the report never drifts from the file (see the test below) without editing
/// `chat.rs`.
const CHAT_ENGINE_DOCTRINE_SOURCE: &str = include_str!("../../../prompts/chat-engine.md");
/// `prompts/studio-core.md`, for the same reason.
const STUDIO_CORE_SOURCE: &str = include_str!("../../../prompts/studio-core.md");

/// `prompts/chat-engine.md` bytes/4 — the doctrine estimate every task's `engine_facts` is
/// derived against.
#[must_use]
pub fn chat_engine_doctrine_tokens() -> u64 {
    estimate_text_tokens(CHAT_ENGINE_DOCTRINE_SOURCE)
}

/// Runs the nine ENG-418 tasks (ten provider requests — `param_edit` runs twice) into
/// `output_dir/engine-baseline.json` and `.md`.
///
/// # Errors
/// Fails when the Godot fixture cannot be scaffolded, a turn fails to start or never produces a
/// context sample, or the report cannot be written.
pub async fn capture_engine_into(output_dir: &Path) -> Result<EngineBaselineReport, String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|error| format!("cannot create {}: {error}", output_dir.display()))?;

    let workspace = engine_fixture_workspace()?;
    let workspace_path = workspace.to_string_lossy().into_owned();

    // Raw per-turn samples are working data, not a deliverable — only the deterministic report
    // under docs/token-engine/ is checked in.
    let samples_path = std::env::temp_dir()
        .join("bhippi-engine-baseline-samples")
        .join(format!("run-{}.json", std::process::id()));
    if let Some(parent) = samples_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let store = Arc::new(ContextSampleStore::new(&samples_path));
    store
        .clear()
        .await
        .map_err(|error| format!("cannot reset the sample log: {error}"))?;
    let engine = Arc::new(ChatEngine::new(NoopEmitter).with_context(store.clone()));
    let registry = Arc::new(ProviderRuntime::from_detection(
        bhippi_providers::detect(&[], &["demo".to_owned()]).await,
    ));

    let specs = engine_task_specs();
    let labels: std::collections::HashMap<&'static str, usize> = specs
        .iter()
        .enumerate()
        .map(|(index, spec)| (spec.label, index))
        .collect();

    let mut conversation_ids: Vec<(String, usize)> = Vec::new();
    let mut runs = Vec::new();

    for (index, spec) in specs.iter().enumerate() {
        for &effort in spec.efforts {
            // The conversation a follow-up continues is the one the referenced task opened.
            let inherited = spec
                .continue_conversation
                .and_then(|label| {
                    conversation_ids
                        .iter()
                        .find(|(_id, owner)| *owner == labels[label])
                })
                .map(|(id, _)| id.clone());
            let conversation_id = inherited
                .clone()
                .unwrap_or_else(|| bhippi_types::SessionId::new().to_string());
            if inherited.is_none() {
                conversation_ids.push((conversation_id.clone(), index));
            }

            let pair = engine
                .send(
                    &registry,
                    ConversationScope {
                        project_path: workspace_path.clone(),
                        conversation_id: conversation_id.clone(),
                    },
                    spec.text.to_owned(),
                    TurnOptions {
                        provider_id: Some("demo".to_owned()),
                        model: None,
                        effort,
                        design: DesignMode::Off,
                        caveman: false,
                        attachments: Vec::new(),
                    },
                )
                .await
                .map_err(|error| {
                    format!(
                        "task {} ({}) failed to start: {error}",
                        spec.label,
                        effort_name(effort)
                    )
                })?;

            wait_for_turn(&engine, &workspace_path, &pair.assistant_turn_id).await?;

            let log = store
                .load()
                .await
                .map_err(|error| format!("cannot read the sample log: {error}"))?;
            let Some(sample) = log
                .samples
                .iter()
                .find(|sample| sample.turn_id == pair.assistant_turn_id)
                .cloned()
            else {
                return Err(format!(
                    "task {} ({}) finished without a context sample (turn {})",
                    spec.label,
                    effort_name(effort),
                    pair.assistant_turn_id
                ));
            };

            let mut categories: Vec<(String, u64)> = sample
                .categories
                .iter()
                .map(|(category, tokens)| (category.as_str().to_owned(), *tokens))
                .collect();
            categories.sort_by_key(|(_, tokens)| std::cmp::Reverse(*tokens));

            let engine_doctrine = chat_engine_doctrine_tokens();
            let engine_category = sample
                .categories
                .get(&ContextCategory::Engine)
                .copied()
                .unwrap_or(0);
            let engine_facts = engine_category.saturating_sub(engine_doctrine);

            let non_system = [
                ContextCategory::Conversation,
                ContextCategory::ReservedResponse,
            ];
            let system_block_tokens: u64 = sample
                .categories
                .iter()
                .filter(|(category, _)| !non_system.contains(category))
                .map(|(_, tokens)| *tokens)
                .sum();
            let rounds_issued = sample.stream_requests;
            let system_prefix_repeated_tokens =
                system_block_tokens.saturating_mul(u64::from(rounds_issued));

            runs.push(EngineTaskRun {
                label: spec.label.to_owned(),
                effort: effort_name(effort),
                history_messages: sample.history_messages,
                estimated_total: sample.estimated_total,
                context_window_tokens: sample.context_window_tokens,
                over_window: sample.over_window,
                reserved_output: sample.reserved_output,
                rounds_issued,
                categories,
                engine_doctrine,
                engine_facts,
                system_block_tokens,
                system_prefix_repeated_tokens,
            });
        }
    }

    let report = EngineBaselineReport {
        runs,
        studio_core_bytes: u64::try_from(STUDIO_CORE_SOURCE.len()).unwrap_or(u64::MAX),
        studio_core_tokens: estimate_text_tokens(STUDIO_CORE_SOURCE),
        chat_engine_bytes: u64::try_from(CHAT_ENGINE_DOCTRINE_SOURCE.len()).unwrap_or(u64::MAX),
        chat_engine_tokens: chat_engine_doctrine_tokens(),
        captured_on: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        git_head: git_head_short(),
        workspace,
        report_json: output_dir.join("engine-baseline.json"),
        report_md: output_dir.join("engine-baseline.md"),
    };

    let json = render_engine_json(&report)?;
    std::fs::write(&report.report_json, json)
        .map_err(|error| format!("cannot write {}: {error}", report.report_json.display()))?;
    let markdown = render_engine_markdown(&report);
    std::fs::write(&report.report_md, markdown)
        .map_err(|error| format!("cannot write {}: {error}", report.report_md.display()))?;

    Ok(report)
}

/// A fresh Godot project fixture with a real `Bhippi.game.toml`, so `engine_context()` has a
/// game project to describe instead of returning empty. One fixture serves every task, exactly
/// like the plain baseline's single fixture workspace serves all five of its tasks.
fn engine_fixture_workspace() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir()
        .join("bhippi-engine-baseline-fixture")
        .join(format!("run-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|error| format!("cannot clear {}: {error}", dir.display()))?;
    }
    bhippi_engine::godot::scaffold::write_project(
        &dir,
        "Baseline Game",
        bhippi_engine::godot::scaffold::ProjectTemplate::ThirdPerson3D,
        true,
    )
    .map_err(|error| {
        format!(
            "cannot scaffold the Godot fixture at {}: {error}",
            dir.display()
        )
    })?;
    Ok(dir)
}

/// The repo's current commit, short form, for the report header. `"unknown"` when `git` is
/// unavailable — metadata only, never worth failing the capture over.
fn git_head_short() -> String {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(&repo_root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|sha| sha.trim().to_owned())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

#[derive(Serialize)]
struct EngineBaselineDoc {
    captured_on: String,
    git_head: String,
    engine_autonomy_max_rounds: u32,
    constants: EngineBaselineConstantsDoc,
    waste_summary: EngineWasteSummaryDoc,
    tasks: Vec<EngineTaskDoc>,
}

#[derive(Serialize)]
struct EngineBaselineConstantsDoc {
    studio_core_bytes: u64,
    studio_core_tokens: u64,
    chat_engine_bytes: u64,
    chat_engine_tokens: u64,
}

#[derive(Serialize)]
struct EngineWasteSummaryDoc {
    doctrine_tokens_per_turn: u64,
    doctrine_share_of_system_block_percent_mean: u32,
    system_prefix_repeated_tokens_max_observed: u64,
    system_prefix_tokens_upper_bound_max: u64,
    engine_facts_identical_across_all_runs: bool,
    engine_category_identical_across_all_runs: bool,
}

#[derive(Serialize)]
struct EngineTaskDoc {
    label: String,
    effort: String,
    history_messages: u32,
    estimated_total: u64,
    context_window_tokens: u64,
    over_window: bool,
    reserved_output: u64,
    rounds_issued: u32,
    categories: BTreeMap<String, u64>,
    engine_doctrine: u64,
    engine_facts: u64,
    system_block_tokens: u64,
    system_prefix_repeated_tokens: u64,
}

/// Computed, not asserted: the doctrine share, the repeated-prefix upper bound, and whether the
/// `Engine` category (or just its facts half) actually is identical across every captured run.
fn waste_summary(report: &EngineBaselineReport) -> EngineWasteSummaryDoc {
    let shares: Vec<u32> = report
        .runs
        .iter()
        .filter(|run| run.system_block_tokens > 0)
        .map(|run| {
            ((run.engine_doctrine as f64 / run.system_block_tokens as f64) * 100.0).round() as u32
        })
        .collect();
    let doctrine_share_mean = if shares.is_empty() {
        0
    } else {
        (f64::from(shares.iter().copied().sum::<u32>()) / shares.len() as f64).round() as u32
    };
    let system_prefix_max = report
        .runs
        .iter()
        .map(|run| run.system_prefix_repeated_tokens)
        .max()
        .unwrap_or(0);
    let rounds_cap = u64::try_from(bhippi_types::ENGINE_AUTONOMY_MAX_ROUNDS).unwrap_or(u64::MAX);
    let system_prefix_upper_bound_max = report
        .runs
        .iter()
        .map(|run| run.system_block_tokens.saturating_mul(rounds_cap))
        .max()
        .unwrap_or(0);
    let facts: std::collections::BTreeSet<u64> =
        report.runs.iter().map(|run| run.engine_facts).collect();
    let engine_totals: std::collections::BTreeSet<u64> = report
        .runs
        .iter()
        .map(|run| run.engine_doctrine + run.engine_facts)
        .collect();
    EngineWasteSummaryDoc {
        doctrine_tokens_per_turn: report.chat_engine_tokens,
        doctrine_share_of_system_block_percent_mean: doctrine_share_mean,
        system_prefix_repeated_tokens_max_observed: system_prefix_max,
        system_prefix_tokens_upper_bound_max: system_prefix_upper_bound_max,
        engine_facts_identical_across_all_runs: facts.len() <= 1,
        engine_category_identical_across_all_runs: engine_totals.len() <= 1,
    }
}

/// Rebuilds `value` with every object's keys sorted, recursively — so the report JSON is
/// byte-for-byte deterministic regardless of the crate's default map ordering.
fn canonical_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<String> = map.keys().cloned().collect();
            keys.sort();
            let mut sorted = serde_json::Map::new();
            for key in keys {
                let entry = map.get(&key).cloned().unwrap_or(serde_json::Value::Null);
                sorted.insert(key, canonical_json(entry));
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonical_json).collect())
        }
        other => other,
    }
}

fn render_engine_json(report: &EngineBaselineReport) -> Result<String, String> {
    let doc = EngineBaselineDoc {
        captured_on: report.captured_on.clone(),
        git_head: report.git_head.clone(),
        engine_autonomy_max_rounds: u32::try_from(bhippi_types::ENGINE_AUTONOMY_MAX_ROUNDS)
            .unwrap_or(u32::MAX),
        constants: EngineBaselineConstantsDoc {
            studio_core_bytes: report.studio_core_bytes,
            studio_core_tokens: report.studio_core_tokens,
            chat_engine_bytes: report.chat_engine_bytes,
            chat_engine_tokens: report.chat_engine_tokens,
        },
        waste_summary: waste_summary(report),
        tasks: report
            .runs
            .iter()
            .map(|run| EngineTaskDoc {
                label: run.label.clone(),
                effort: run.effort.to_owned(),
                history_messages: run.history_messages,
                estimated_total: run.estimated_total,
                context_window_tokens: run.context_window_tokens,
                over_window: run.over_window,
                reserved_output: run.reserved_output,
                rounds_issued: run.rounds_issued,
                categories: run.categories.iter().cloned().collect(),
                engine_doctrine: run.engine_doctrine,
                engine_facts: run.engine_facts,
                system_block_tokens: run.system_block_tokens,
                system_prefix_repeated_tokens: run.system_prefix_repeated_tokens,
            })
            .collect(),
    };
    let value = serde_json::to_value(&doc)
        .map_err(|error| format!("cannot serialize the engine baseline report: {error}"))?;
    let sorted = canonical_json(value);
    serde_json::to_string_pretty(&sorted)
        .map_err(|error| format!("cannot render the engine baseline report: {error}"))
}

#[must_use]
pub fn render_engine_markdown(report: &EngineBaselineReport) -> String {
    let mut out = String::new();
    out.push_str("# Bhippi Token Engine — engine-turn baseline (GAD-040)\n\n");
    out.push_str(&format!(
        "Captured on `{}`, repo `{}`. Same mechanism as `baseline.md`: `ChatEngine` on the offline \
         `demo` provider, `ContextSampleStore` recording one `ContextSample` per turn from the strings \
         `run_turn` actually assembled, and the same four-bytes-per-token estimator \
         (`bhippi_core::estimate_text_tokens`). The addition is a real Godot project fixture — \
         `bhippi_engine::godot::scaffold::write_project(.., ProjectTemplate::ThirdPerson3D, ..)` — so \
         `Bhippi.game.toml` exists and `engine_context()` (chat.rs:4956) takes the game-project path \
         instead of returning empty. All nine ENG-418 tasks run inside that one fixture; `param_edit` \
         and `composition_edit` continue the `third_person_controller` conversation, and `param_edit` is \
         measured at both `balanced` and `fast`.\n\n",
        report.captured_on, report.git_head
    ));

    out.push_str("## Constants\n\n");
    out.push_str("| file | bytes | estimated tokens |\n|---|---:|---:|\n");
    out.push_str(&format!(
        "| prompts/studio-core.md | {} | {} |\n",
        report.studio_core_bytes, report.studio_core_tokens
    ));
    out.push_str(&format!(
        "| prompts/chat-engine.md (the doctrine) | {} | {} |\n",
        report.chat_engine_bytes, report.chat_engine_tokens
    ));
    out.push_str(&format!(
        "\n`ENGINE_AUTONOMY_MAX_ROUNDS` = {} (`crates/bhippi-types/src/engine.rs:8`). \
         `ENGINE_CONTEXT_TOKEN_BUDGET` = 1,500 (`crates/bhippi-types/src/engine.rs:12`) caps the \
         dynamic facts only — the doctrine above is fixed prompt text and is outside that budget.\n\n",
        bhippi_types::ENGINE_AUTONOMY_MAX_ROUNDS
    ));

    out.push_str("## Per-task context budget\n\n");
    out.push_str(
        "| task | effort | history msgs | input est. | context window | over window | reserved output | rounds issued |\n|---|---|---|---|---|---|---|---|\n",
    );
    for run in &report.runs {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            run.label,
            run.effort,
            run.history_messages,
            run.estimated_total,
            run.context_window_tokens,
            if run.over_window { "yes" } else { "no" },
            run.reserved_output,
            run.rounds_issued,
        ));
    }
    out.push('\n');

    out.push_str("## Category breakdown\n\n");
    out.push_str("| task | effort | category | estimated tokens |\n|---|---|---|---|\n");
    for run in &report.runs {
        for (category, tokens) in &run.categories {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                run.label, run.effort, category, tokens
            ));
        }
    }
    out.push('\n');

    out.push_str("## Doctrine / facts split\n\n");
    out.push_str(
        "| task | effort | engine category total | engine doctrine | engine facts | doctrine share of engine |\n|---|---|---|---|---|---|\n",
    );
    for run in &report.runs {
        let total = run.engine_doctrine + run.engine_facts;
        let share = if total == 0 {
            0
        } else {
            ((run.engine_doctrine as f64 / total as f64) * 100.0).round() as u32
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {}% |\n",
            run.label, run.effort, total, run.engine_doctrine, run.engine_facts, share
        ));
    }
    out.push('\n');

    let waste = waste_summary(report);
    out.push_str("## Waste summary\n\n");
    out.push_str("| metric | value |\n|---|---|\n");
    out.push_str(&format!(
        "| Doctrine tokens injected per engine turn | **{}** |\n",
        waste.doctrine_tokens_per_turn
    ));
    out.push_str(&format!(
        "| Doctrine share of the system block (mean across tasks) | **{}%** |\n",
        waste.doctrine_share_of_system_block_percent_mean
    ));
    out.push_str(&format!(
        "| System-prefix tokens repeated this run (`system_block × rounds_issued`, max observed) | **{}** |\n",
        waste.system_prefix_repeated_tokens_max_observed
    ));
    out.push_str(&format!(
        "| System-prefix tokens × rounds upper bound (`system_block × ENGINE_AUTONOMY_MAX_ROUNDS={}`, max across tasks) | **{}** |\n",
        bhippi_types::ENGINE_AUTONOMY_MAX_ROUNDS, waste.system_prefix_tokens_upper_bound_max
    ));
    out.push_str(&format!(
        "| `Engine` category identical across every one of the {} runs (doctrine + facts) | **{}** |\n",
        report.runs.len(),
        if waste.engine_category_identical_across_all_runs { "yes" } else { "no" }
    ));
    out.push_str(&format!(
        "| `engine_facts` alone (map/selection/journal, minus the doctrine) identical across every run | **{}** |\n",
        if waste.engine_facts_identical_across_all_runs { "yes" } else { "no" }
    ));
    out.push_str(
        "\nThe facts finding is a property of this offline harness, not a guarantee: the demo \
         provider's canned reply never contains an `<engine_batch>`/`<engine_action>` tag, so the \
         fixture scene never changes between tasks and the per-project facts stay byte-for-byte \
         constant. A live session that actually edits the scene between turns would see \
         `engine_facts` vary; the doctrine would not — it is fixed prompt text, not retrieval.\n\n",
    );

    out.push_str("## Where this is assembled (chat.rs)\n\n");
    out.push_str("- 2506-2589 — the six-part structured system prompt (`combined_system`)\n");
    out.push_str("- 2538-2561 — auto-scaffold on game intent, then `engine_context(&workspace)` folded into part 4 (`part4_project_brain`)\n");
    out.push_str("- 2636-2676 — the `ContextManifest` this report reads (`System`, `Workspace`, `ProjectRules`, `Skills`, `ComputerUse`, `Engine`, `Handoff`, `TaskDirectives`, history, `ReservedResponse`), and the `ContextSample` recorded from it\n");
    out.push_str("- 2677-2688 — the context-window guard (`over_window`), evaluated *after* the sample is recorded, so an overflowing turn still leaves a sample\n");
    out.push_str("- 3156-3278 — the bounded engine autonomy loop (read → act → verify), capped at `ENGINE_AUTONOMY_MAX_ROUNDS`\n");
    out.push_str("- 4956-5000 — `engine_context()`: `ENGINE_SYSTEM` (`= include_str!(\"../../../prompts/chat-engine.md\")`, chat.rs:40) plus the capped per-project facts (`cap_engine_facts`, ENG-191)\n\n");

    out.push_str("## What this harness could not measure\n\n");
    out.push_str(
        "- **Real provider cache hits.** `ContextSample.cache_hits`/`cache_misses`/`cache_bytes_loaded`/\
         `measured_input_tokens` are never set anywhere in `chat.rs` — they stay at their zero/`None` \
         defaults for every provider, demo included. A vendor that prompt-caches the system prefix would \
         make `system_prefix_repeated_tokens` an overstatement of *billed* tokens; this baseline has no \
         way to observe that discount and reports the uncached number.\n",
    );
    out.push_str(
        "- **Real engine-loop rounds.** The demo provider's reply is a fixed, scripted string \
         (`bhippi-providers/src/demo.rs::script_reply`) that never contains an `<engine_batch>`, \
         `<engine_action>`, or `<engine_query>` tag. `extract_engine_batch_tags`/`extract_engine_action_tags` \
         therefore return empty, `engine_answers` and `engine_batches` stay empty, `continuation_prompt()` \
         returns `None` on its very first check, and the loop at chat.rs:3166 breaks before a second request \
         is ever sent. `ContextSample.stream_requests` is also hardcoded to `1` at construction and is never \
         incremented by the loop, so even a provider that *did* loop would not show it here. `rounds_issued` \
         is `1` for every row in this report; `system_prefix_repeated_tokens` is therefore identical to \
         `system_block_tokens` today, and the upper-bound row above is the only place \
         `ENGINE_AUTONOMY_MAX_ROUNDS` shows up in this report.\n",
    );
    out.push('\n');

    out
}

/// A no-op emitter: the baseline costs the machinery, not the event fan-out.
struct NoopEmitter;

impl Emit for NoopEmitter {
    fn thinking(&self, _turn_id: &str, _label: &str, _phase: crate::chat::AgentPhase) {}
    fn limits(&self, _provider: &str, _limits: LimitSnapshot) {}
    fn thought_delta(&self, _turn_id: &str, _delta: &str) {}
    fn delta(&self, _turn_id: &str, _delta: &str) {}
    fn tool(&self, _turn_id: &str, _tool: ToolActivity) {}
    fn permission(&self, _turn_id: &str, _request: PermissionRequest) {}
    fn done(&self, _event: crate::chat::ChatTurnDone) {}
}

/// A small representative repository the code-task prompts can reference. Content
/// stays technology/AI-adjacent so nothing here ever contradicts Bhippi's scope.
fn fixture_workspace() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir()
        .join("bhippi-baseline-fixture")
        .join(format!("run-{}", std::process::id()));
    let write = |relative: &str, content: &str| -> Result<(), String> {
        let path = dir.join(relative);
        std::fs::create_dir_all(
            path.parent()
                .ok_or_else(|| "fixture path has no parent".to_owned())?,
        )
        .map_err(|error| format!("cannot create fixture dirs: {error}"))?;
        std::fs::write(&path, content)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))
    };

    write(".bhippi/rules.md", FIXTURE_RULES)?;
    write("src/main.ts", FIXTURE_MAIN)?;
    write("src/components/Panel.tsx", FIXTURE_PANEL)?;
    write("tests/main.test.ts", FIXTURE_TESTS)?;
    write("package.json", FIXTURE_PACKAGE)?;
    write(
        "README.md",
        "# Fixture\n\nA representative scaffolding repo for token baseline capture.\n",
    )?;

    Ok(dir)
}

const FIXTURE_RULES: &str = "\
# Project rules

This workspace is a TypeScript + React desktop app shell.

- One accent colour; never a second.
- Components stay under 120 lines.
- Export named, never default.
- Tests live under tests/ and mirror the source path.
- No lodash; native APIs only.
- The design system in docs/DESIGN-SYSTEM.md is the only style authority.
";

const FIXTURE_MAIN: &str = "\
import { createRoot } from 'react-dom/client';
import { App } from './app/App';

const root = document.getElementById('root');
if (!root) throw new Error('missing #root');

const app = createRoot(root);
app.render(<App />);

export function boot(): void {
  console.info('bhippi-shell booted');
}
";

const FIXTURE_PANEL: &str = "\
import { useState } from 'react';

export interface PanelProps {
  title: string;
  accent?: boolean;
}

export function Panel({ title, accent }: PanelProps) {
  const [open, setOpen] = useState(false);
  const className = accent ? 'panel panel--accent' : 'panel';
  return (
    <section className={className} aria-label={title}>
      <header>
        <button onClick={() => setOpen((value) => !value)}>{title}</button>
      </header>
      {open && <div className=\"panel__body\">Body</div>}
    </section>
  );
}
";

const FIXTURE_TESTS: &str = "\
import { describe, expect, it } from 'vitest';
import { Panel } from '../src/components/Panel';

describe('Panel', () => {
  it('renders its title', () => {
    const view = render(<Panel title=\"Settings\" />);
    expect(view.getByLabelText('Settings')).toBeTruthy();
  });
});
";

const FIXTURE_PACKAGE: &str = "\
{
  \"name\": \"bhippi-shell-fixture\",
  \"private\": true,
  \"type\": \"module\",
  \"scripts\": {
    \"test\": \"vitest run\"
  },
  \"devDependencies\": {
    \"vitest\": \"^2.1.0\"
  }
}
";

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_baseline_captures_every_task_into_a_report() {
        let dir = std::env::temp_dir().join(format!("bhippi-baseline-{}", std::process::id()));
        let _ignored = std::fs::remove_dir_all(&dir);

        let report = capture_into(&dir)
            .await
            .unwrap_or_else(|error| panic!("baseline must capture: {error}"));

        assert_eq!(report.tasks.len(), representative_tasks().len());
        assert!(
            report.tasks.iter().all(|task| task.estimated_total > 0),
            "every task must carry an estimated prompt"
        );
        assert!(
            report.tasks.iter().any(|task| task.history_messages > 0),
            "follow-ups must grow the conversation history"
        );
        assert_eq!(
            report.handoff_observed, 0,
            "a single-provider baseline must not mark any turn as a handoff"
        );
        assert!(
            report.tasks.iter().all(|task| {
                task.categories
                    .iter()
                    .any(|(category, _)| category.as_str() == "conversation")
            }),
            "every turn records a conversation slice"
        );
        assert!(
            report.samples_json.exists(),
            "baseline.json must be written"
        );
        assert!(report.report_md.exists(), "baseline.md must be written");

        let rendered = std::fs::read_to_string(&report.report_md)
            .unwrap_or_else(|error| panic!("report must be readable: {error}"));
        assert!(
            rendered.contains("Tool-schema overhead (A3): **0 tokens**"),
            "A3 is an explicit zero"
        );
        let _ignored = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_follow_up_label_maps_to_the_opening_task() {
        let labels = representative_labels();
        assert_eq!(labels["code_task"], 2);
        assert_eq!(labels["follow_up_one"], 3);
    }

    /// GAD-040 deliverable 2: the engine task set is exactly the nine ENG-418 ids, in the
    /// plan's order — so the report cannot silently reorder, drop, or add a task.
    #[test]
    fn engine_task_ids_match_the_plan() {
        let labels: Vec<&'static str> = engine_task_specs().iter().map(|spec| spec.label).collect();
        assert_eq!(labels, ENGINE_TASK_IDS);
    }

    /// GAD-040 deliverable 2: the doctrine estimate this report uses is derived from
    /// `prompts/chat-engine.md` itself, so the report cannot drift from the file silently.
    #[test]
    fn engine_doctrine_estimate_matches_the_prompt_file() {
        let on_disk = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("prompts")
                .join("chat-engine.md"),
        )
        .expect("prompts/chat-engine.md must be readable");
        let expected = u64::try_from(on_disk.len()).unwrap_or(u64::MAX) / 4;
        assert_eq!(chat_engine_doctrine_tokens(), expected);
        // And the constant included at compile time must be the same file `chat.rs` embeds.
        assert_eq!(CHAT_ENGINE_DOCTRINE_SOURCE.len(), on_disk.len());
    }

    /// `param_edit` is the plan's only two-effort task; everything else is `balanced` only.
    #[test]
    fn only_param_edit_runs_at_two_efforts() {
        for spec in engine_task_specs() {
            if spec.label == "param_edit" {
                assert_eq!(spec.efforts, &[Effort::Balanced, Effort::Fast][..]);
            } else {
                assert_eq!(spec.efforts, &[Effort::Balanced][..], "{}", spec.label);
            }
        }
    }

    #[tokio::test]
    async fn the_engine_baseline_captures_every_task_run_into_a_report() {
        let dir =
            std::env::temp_dir().join(format!("bhippi-engine-baseline-{}", std::process::id()));
        let _ignored = std::fs::remove_dir_all(&dir);

        let report = capture_engine_into(&dir)
            .await
            .unwrap_or_else(|error| panic!("engine baseline must capture: {error}"));

        // Nine task ids, ten runs (param_edit measured at both efforts).
        let mut seen_labels: Vec<&str> = report
            .runs
            .iter()
            .map(|run| run.label.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        seen_labels.sort_unstable();
        let mut expected_labels: Vec<&str> = ENGINE_TASK_IDS.to_vec();
        expected_labels.sort_unstable();
        assert_eq!(seen_labels, expected_labels);
        assert_eq!(report.runs.len(), 10, "param_edit must run at two efforts");
        assert_eq!(
            report
                .runs
                .iter()
                .filter(|run| run.label == "param_edit")
                .count(),
            2
        );

        // Every run carries the doctrine, unconditionally, since the fixture is a real game
        // project for the whole capture.
        assert!(
            report
                .runs
                .iter()
                .all(|run| run.engine_doctrine == report.chat_engine_tokens),
            "every engine turn must inject the same doctrine estimate"
        );
        assert!(
            report.runs.iter().all(|run| run.rounds_issued == 1),
            "the demo provider never triggers a second engine-loop round"
        );
        assert!(
            report
                .runs
                .iter()
                .any(|run| run.label == "param_edit" && run.history_messages > 0),
            "param_edit must continue the third_person_controller conversation"
        );

        assert!(
            report.report_json.exists(),
            "engine-baseline.json must be written"
        );
        assert!(
            report.report_md.exists(),
            "engine-baseline.md must be written"
        );

        let json_text = std::fs::read_to_string(&report.report_json)
            .unwrap_or_else(|error| panic!("engine-baseline.json must be readable: {error}"));
        let parsed: serde_json::Value = serde_json::from_str(&json_text)
            .unwrap_or_else(|error| panic!("engine-baseline.json must parse: {error}"));
        assert_eq!(parsed["tasks"].as_array().map(Vec::len), Some(10));
        assert!(parsed["git_head"].is_string());
        // `captured_on` is a bare date ("YYYY-MM-DD"), never a full timestamp — no other field
        // in the document carries a time component at all.
        let captured_on = parsed["captured_on"]
            .as_str()
            .expect("captured_on must be a string");
        assert_eq!(captured_on.len(), 10, "captured_on must be YYYY-MM-DD");
        assert!(
            !captured_on.contains(':'),
            "captured_on must not carry a time"
        );

        let rendered_md = std::fs::read_to_string(&report.report_md)
            .unwrap_or_else(|error| panic!("engine-baseline.md must be readable: {error}"));
        assert!(rendered_md.contains("## Waste summary"));
        assert!(rendered_md.contains("Doctrine tokens injected per engine turn"));

        let _ignored = std::fs::remove_dir_all(&dir);
        let _ignored = std::fs::remove_dir_all(&report.workspace);
    }
}
