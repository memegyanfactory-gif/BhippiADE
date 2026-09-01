//! The AI ↔ engine bridge (ENG-112, ENG-113).
//!
//! Two jobs:
//!
//! 1. **Extract engine calls out of a streaming response.** Providers here emit text, not
//!    tool calls (`CompletionRequest` has no `tools` field and the CLI adapters run their
//!    own tool loops), so the protocol is a tag. Scanning it *as it streams* rather than
//!    after the turn is what turns "the AI edits the scene" into a loop the model can
//!    verify inside one turn.
//! 2. **Keep protocol text out of the visible answer.** The user reads prose and watches
//!    Activity Dock cards; raw JSON in the transcript is noise. Same rule Computer Use
//!    already follows.

use super::session::{EngineActionOutcome, EngineBatchResult};
use serde::{Deserialize, Serialize};

const ACTION_OPEN: &str = "<engine_action>";
const ACTION_CLOSE: &str = "</engine_action>";
const BATCH_OPEN: &str = "<engine_batch>";
const BATCH_CLOSE: &str = "</engine_batch>";
const QUERY_OPEN: &str = "<engine_query>";
const QUERY_CLOSE: &str = "</engine_query>";

/// One complete engine call pulled out of the stream.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum EngineCall {
    /// A single `<engine_action>` payload — its own one-action batch.
    Action(String),
    /// An `<engine_batch>` payload: `{ "label": "...", "actions": [...] }`.
    Batch(String),
    /// An `<engine_query>` payload — a read, answered back to the model in the same turn.
    Query(String),
}

/// Incremental scanner over a model's text stream.
///
/// Deltas arrive at arbitrary boundaries — an opening tag routinely straddles two chunks —
/// so the scanner buffers, and only releases text as "visible" once it is certain that text
/// is not the beginning of a tag.
#[derive(Debug, Default)]
pub struct EngineCallScanner {
    buffer: String,
    inside: Option<Inside>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inside {
    Action,
    Batch,
    Query,
}

impl EngineCallScanner {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one delta. Returns the text safe to show the user, plus any calls that closed.
    pub fn push(&mut self, delta: &str) -> (String, Vec<EngineCall>) {
        self.buffer.push_str(delta);
        let mut visible = String::new();
        let mut calls = Vec::new();

        loop {
            match self.inside {
                None => {
                    // Whichever opening tag comes first wins, so calls interleaved with
                    // prose stay in the order the model wrote them.
                    let candidates = [
                        (
                            self.buffer.find(ACTION_OPEN),
                            Inside::Action,
                            ACTION_OPEN.len(),
                        ),
                        (
                            self.buffer.find(BATCH_OPEN),
                            Inside::Batch,
                            BATCH_OPEN.len(),
                        ),
                        (
                            self.buffer.find(QUERY_OPEN),
                            Inside::Query,
                            QUERY_OPEN.len(),
                        ),
                    ];
                    let Some((at, kind, open_len)) = candidates
                        .into_iter()
                        .filter_map(|(at, kind, len)| at.map(|at| (at, kind, len)))
                        .min_by_key(|(at, _, _)| *at)
                    else {
                        // No tag in sight. Hold back a tail that could still grow into an
                        // opening tag; release everything before it.
                        let keep = partial_tag_suffix(&self.buffer);
                        let release = self.buffer.len() - keep;
                        visible.push_str(&self.buffer[..release]);
                        self.buffer.drain(..release);
                        break;
                    };
                    visible.push_str(&self.buffer[..at]);
                    self.buffer.drain(..at + open_len);
                    self.inside = Some(kind);
                }
                Some(kind) => {
                    let close = match kind {
                        Inside::Action => ACTION_CLOSE,
                        Inside::Batch => BATCH_CLOSE,
                        Inside::Query => QUERY_CLOSE,
                    };
                    let Some(at) = self.buffer.find(close) else {
                        // The call has not finished arriving; nothing inside it is visible.
                        break;
                    };
                    let payload = self.buffer[..at].trim().to_owned();
                    self.buffer.drain(..at + close.len());
                    self.inside = None;
                    if !payload.is_empty() {
                        calls.push(match kind {
                            Inside::Action => EngineCall::Action(payload),
                            Inside::Batch => EngineCall::Batch(payload),
                            Inside::Query => EngineCall::Query(payload),
                        });
                    }
                }
            }
        }
        (visible, calls)
    }

    /// Flush at end of stream. An unterminated tag is a truncated response, so its partial
    /// payload is dropped rather than half-applied; whatever plain text was held back is
    /// released so the answer is not silently truncated.
    pub fn finish(&mut self) -> String {
        let tail = if self.inside.is_some() {
            String::new()
        } else {
            std::mem::take(&mut self.buffer)
        };
        self.buffer.clear();
        self.inside = None;
        tail
    }

    /// True when a call is still arriving.
    #[must_use]
    pub fn is_mid_call(&self) -> bool {
        self.inside.is_some()
    }
}

/// How many trailing bytes of `text` could still turn into an opening tag once more of the
/// stream arrives (`"…here is a <engine_"` must not be shown yet).
fn partial_tag_suffix(text: &str) -> usize {
    let mut best = 0;
    for open in [ACTION_OPEN, BATCH_OPEN, QUERY_OPEN] {
        // The longest proper prefix of `open` that is also a suffix of `text`.
        for len in (1..open.len()).rev() {
            if len > text.len() {
                continue;
            }
            let start = text.len() - len;
            if !text.is_char_boundary(start) {
                continue;
            }
            if text[start..] == open[..len] {
                best = best.max(len);
                break;
            }
        }
    }
    best
}

/// The follow-up message that continues a turn (ENG-113/ENG-115).
///
/// Two things can oblige a continuation:
///
/// * the model asked a question (`<engine_query>`) and is owed the answer, and
/// * a batch was rejected, and is owed the failing index, the engine's message and — when
///   the action named a component — that component's real schema.
///
/// Both are evidence, not scolding: terse, mechanical, and enough to act on. `None` means
/// the turn is finished.
#[must_use]
pub fn continuation_prompt(
    answers: &[(String, String)],
    results: &[EngineBatchResult],
) -> Option<String> {
    let failed: Vec<&EngineBatchResult> = results.iter().filter(|row| !row.applied).collect();
    if answers.is_empty() && failed.is_empty() {
        return None;
    }
    let mut out = String::new();
    if !answers.is_empty() {
        out.push_str("Engine observations, query answers, and typed errors:\n");
        for (query, answer) in answers {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\n### {query}\n{answer}\n"));
        }
        if failed.is_empty() {
            out.push_str(
                "\nContinue. Emit the engine changes you decided on, or say what you found.\n",
            );
            return Some(out);
        }
    }
    out.push_str(
        "\nYour engine changes were REJECTED and nothing was written. A batch is all-or-nothing, \
         so fix the failing action and resend the whole batch.\n",
    );
    for result in failed {
        out.push_str(&format!("\n## Batch \"{}\"\n", result.label));
        out.push_str(&format!("Scene: {}\n", result.scene_path));
        for outcome in &result.outcomes {
            out.push_str(&outcome_line(outcome));
        }
    }
    out.push_str(
        "\nResend one corrected <engine_batch>. The whole batch was rolled back, so include \
         the actions that reported ok as well — corrected where needed.\n",
    );
    Some(out)
}

fn outcome_line(outcome: &EngineActionOutcome) -> String {
    if outcome.ok {
        return format!(
            "- [{}] {} — ok (rolled back)\n",
            outcome.index, outcome.label
        );
    }
    let mut line = format!(
        "- [{}] {} — FAILED: {}\n",
        outcome.index, outcome.label, outcome.message
    );
    if let Some(hint) = &outcome.hint {
        line.push_str(&format!("  hint: {hint}\n"));
    }
    if let Some(excerpt) = &outcome.schema_excerpt {
        line.push_str("  schema:\n");
        for row in excerpt.lines() {
            line.push_str(&format!("    {row}\n"));
        }
    }
    line
}

/// A human-readable summary of what a batch is about to do (ENG-116).
///
/// This is the plan card the user approves in Ask mode: counts first, because "+18
/// entities, −1 removed" is the shape of the question, and the list second.
#[must_use]
pub fn plan_preview(label: &str, actions: &[serde_json::Value]) -> String {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for action in actions {
        let kind = action
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        *counts.entry(kind).or_default() += 1;
    }
    let summary = counts
        .iter()
        .map(|(kind, count)| format!("{count}× {kind}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{label} — {} action(s): {summary}", actions.len())
}

/// Whether a batch contains something a user would not want done silently.
///
/// Deletes are the honest line: everything else an agent writes is additive or a value the
/// user can drag back, but a removed subtree is the change most likely to be unwanted.
#[must_use]
pub fn is_destructive(actions: &[serde_json::Value]) -> bool {
    actions.iter().any(|action| {
        matches!(
            action.get("kind").and_then(serde_json::Value::as_str),
            Some("delete" | "remove_component")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{continuation_prompt, EngineCall, EngineCallScanner};
    use crate::engine::session::{EngineActionOutcome, EngineBatchResult, EngineSceneState};

    fn scan_all(chunks: &[&str]) -> (String, Vec<EngineCall>) {
        let mut scanner = EngineCallScanner::new();
        let mut text = String::new();
        let mut calls = Vec::new();
        for chunk in chunks {
            let (visible, found) = scanner.push(chunk);
            text.push_str(&visible);
            calls.extend(found);
        }
        text.push_str(&scanner.finish());
        (text, calls)
    }

    #[test]
    fn a_call_split_across_deltas_is_still_found() {
        // The realistic case: tags never arrive whole.
        let (text, calls) = scan_all(&[
            "Adding a crate. <engine_ac",
            "tion>{\"kind\":\"spawn\",",
            "\"template\":\"cube\"}</engine_",
            "action> Done.",
        ]);
        assert_eq!(
            calls,
            vec![EngineCall::Action(
                "{\"kind\":\"spawn\",\"template\":\"cube\"}".to_owned()
            )]
        );
        assert_eq!(text, "Adding a crate.  Done.");
    }

    #[test]
    fn protocol_text_never_reaches_the_visible_answer() {
        let (text, calls) = scan_all(&[
            "Before <engine_batch>{\"label\":\"x\",\"actions\":[]}</engine_batch> after",
        ]);
        assert_eq!(calls.len(), 1);
        assert!(!text.contains("engine_batch"));
        assert!(!text.contains("actions"));
        assert_eq!(text, "Before  after");
    }

    #[test]
    fn several_calls_in_one_delta_all_come_out_in_order() {
        let (_, calls) = scan_all(&[
            "<engine_action>{\"kind\":\"a\"}</engine_action>middle<engine_action>{\"kind\":\"b\"}</engine_action>",
        ]);
        assert_eq!(
            calls,
            vec![
                EngineCall::Action("{\"kind\":\"a\"}".to_owned()),
                EngineCall::Action("{\"kind\":\"b\"}".to_owned()),
            ]
        );
    }

    #[test]
    fn text_that_merely_looks_like_a_tag_is_released() {
        // A held-back tail must not swallow ordinary prose.
        let (text, calls) = scan_all(&["I will use the <engine_ helper", " later."]);
        assert!(calls.is_empty());
        assert_eq!(text, "I will use the <engine_ helper later.");
    }

    #[test]
    fn a_truncated_call_is_dropped_rather_than_half_applied() {
        let (text, calls) = scan_all(&["Working <engine_action>{\"kind\":\"spa"]);
        assert!(calls.is_empty(), "an unterminated call must not be applied");
        assert_eq!(text, "Working ");
    }

    fn state() -> EngineSceneState {
        EngineSceneState {
            scene_path: "assets/scenes/level_01.bscn.json".to_owned(),
            name: "level_01".to_owned(),
            kind: bhippi_engine::document::SceneKind::Level,
            settings: bhippi_engine::document::SceneSettings::default(),
            entity_count: 0,
            dirty: false,
            can_undo: false,
            can_redo: false,
            undo_label: None,
            redo_label: None,
            revision: 0,
            selection: vec![],
            disk_conflict: false,
            recovery_available: false,
            document_json: String::new(),
        }
    }

    #[test]
    fn the_repair_prompt_carries_the_index_the_message_and_the_schema() {
        let result = EngineBatchResult {
            applied: false,
            label: "build a warehouse".to_owned(),
            scene_path: "assets/scenes/level_01.bscn.json".to_owned(),
            outcomes: vec![
                EngineActionOutcome {
                    index: 0,
                    ok: true,
                    label: "spawn cube".to_owned(),
                    message: "ok".to_owned(),
                    hint: None,
                    schema_excerpt: None,
                },
                EngineActionOutcome {
                    index: 1,
                    ok: false,
                    label: "edit RigidBody".to_owned(),
                    message: "RigidBody.kind invalid enum value \"bouncy\"".to_owned(),
                    hint: Some("Valid values: static, dynamic, kinematic".to_owned()),
                    schema_excerpt: bhippi_engine::schema::excerpt("RigidBody"),
                },
            ],
            edit: None,
            state: state(),
        };
        let prompt =
            continuation_prompt(&[], &[result]).expect("a failed batch produces a repair round");
        assert!(prompt.contains("REJECTED"));
        assert!(prompt.contains("build a warehouse"));
        assert!(prompt.contains("[1] edit RigidBody — FAILED"));
        assert!(prompt.contains("Valid values: static, dynamic, kinematic"));
        assert!(
            prompt.contains("lock_rotation"),
            "the real schema is quoted"
        );
        assert!(prompt.contains("rolled back"));
    }

    #[test]
    fn a_query_alone_continues_the_turn_without_a_repair_notice() {
        let prompt = continuation_prompt(
            &[("{\"kind\":\"scene\"}".to_owned(), "entities: 4".to_owned())],
            &[],
        )
        .expect("an unanswered query owes a continuation");
        assert!(prompt.contains("entities: 4"));
        assert!(
            !prompt.contains("REJECTED"),
            "nothing failed, so nothing to repair"
        );
    }

    #[test]
    fn a_query_tag_is_extracted_like_the_others() {
        let (text, calls) =
            scan_all(&["Checking <engine_query>{\"kind\":\"scene\"}</engine_query> now"]);
        assert_eq!(
            calls,
            vec![EngineCall::Query("{\"kind\":\"scene\"}".to_owned())]
        );
        assert_eq!(text, "Checking  now");
    }

    #[test]
    fn the_plan_preview_leads_with_counts() {
        let actions = vec![
            serde_json::json!({ "kind": "spawn", "template": "cube" }),
            serde_json::json!({ "kind": "spawn", "template": "cube" }),
            serde_json::json!({ "kind": "delete", "entity": "Old" }),
        ];
        let preview = super::plan_preview("build a warehouse", &actions);
        assert!(preview.contains("3 action(s)"));
        assert!(preview.contains("2× spawn"));
        assert!(preview.contains("1× delete"));
    }

    #[test]
    fn only_removals_count_as_destructive() {
        let additive = vec![serde_json::json!({ "kind": "spawn", "template": "cube" })];
        assert!(!super::is_destructive(&additive));
        let removes = vec![serde_json::json!({ "kind": "delete", "entity": "Crate" })];
        assert!(super::is_destructive(&removes));
        let strips = vec![serde_json::json!({ "kind": "remove_component", "component": "Light" })];
        assert!(super::is_destructive(&strips));
    }

    #[test]
    fn a_batch_that_applied_needs_no_repair_round() {
        let result = EngineBatchResult {
            applied: true,
            label: "ok".to_owned(),
            scene_path: "s".to_owned(),
            outcomes: vec![],
            edit: None,
            state: state(),
        };
        assert!(continuation_prompt(&[], &[result]).is_none());
    }
}
