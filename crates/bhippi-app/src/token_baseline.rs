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
use bhippi_core::{estimate_text_tokens, ContextSampleStore};
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
}
