//! Team-lead orchestration: spawn worker chats on other models, assign tasks, read status.

use serde::Deserialize;

pub const MAX_SPAWNS_PER_TURN: usize = 4;
pub const MAX_WORKERS_PER_LEAD: usize = 8;

pub const ORCHESTRATE_NUDGE: &str = "\
The user asked you to lead a team. Emit <spawn_agent> tags now — one per worker — with a \
name, provider (claude, codex, grok, antigravity, opencode, kimi) and a single task. Then emit \
<agent_status></agent_status>. Pick providers yourself. Do not ask which models unless none \
are usable. Do not stop on a plan.";

pub const WORKER_PREFACE: &str = "\
You are a worker on a Bhippi team. Do the task below and stop when it is done. \
Do not spawn agents, do not ask the user to pick a flavour, and do not wait to be told \
\"do it\". Emit engine tags and finish.";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct SpawnAgentRequest {
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
    pub provider: String,
    #[serde(default)]
    pub model: Option<String>,
    pub task: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AgentTaskRequest {
    pub id: String,
    pub task: String,
}

#[must_use]
pub fn wants_orchestration(text: &str) -> bool {
    let lower = text.to_lowercase();
    const NEEDLES: &[&str] = &[
        "team lead",
        "teamlead",
        "act as a team",
        "act as team",
        "orchestrat",
        "create an agent",
        "create agents",
        "create a agent",
        "spawn agent",
        "spawn agents",
        "subagent",
        "sub-agent",
        "sub agent",
        "other models",
        "other chats",
        "run a team",
        "multi agent",
        "multi-agent",
        "use claude and",
        "use grok and",
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

#[must_use]
pub fn extract_spawn_agents(text: &str) -> Vec<SpawnAgentRequest> {
    extract_all_tagged_json(text, "spawn_agent")
        .into_iter()
        .filter(|row: &SpawnAgentRequest| {
            !row.name.trim().is_empty()
                && !row.task.trim().is_empty()
                && !row.provider.trim().is_empty()
        })
        .take(MAX_SPAWNS_PER_TURN)
        .collect()
}

#[must_use]
pub fn extract_agent_tasks(text: &str) -> Vec<AgentTaskRequest> {
    extract_all_tagged_json(text, "agent_task")
        .into_iter()
        .filter(|row: &AgentTaskRequest| !row.id.trim().is_empty() && !row.task.trim().is_empty())
        .collect()
}

#[must_use]
pub fn has_agent_status(text: &str) -> bool {
    text.contains("<agent_status>") || text.contains("<agent_status/>")
}

/// Map a human provider name onto a catalogue id.
#[must_use]
pub fn provider_alias(raw: &str) -> Option<&'static str> {
    let t = raw.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    if t == "claude"
        || t.contains("claude")
        || t.contains("anthropic")
        || t.contains("sonnet")
        || t.contains("opus")
        || t.contains("fable")
        || t.contains("haiku")
    {
        return Some("claude");
    }
    if t.contains("grok") || t.contains("xai") {
        return Some("grok");
    }
    if t == "agy" || t.contains("antigravity") {
        return Some("antigravity");
    }
    if t.contains("opencode") || t.contains("open code") || t.contains("open-code") {
        return Some("opencode");
    }
    if t.contains("kimi") || t.contains("moonshot") {
        return Some("kimi");
    }
    if t.contains("codex")
        || t.contains("gpt")
        || t.contains("openai")
        || t.contains("astra")
        || t == "chatgpt"
    {
        return Some("codex");
    }
    None
}

#[must_use]
pub fn excerpt(text: &str, max_chars: usize) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut out = String::new();
    for ch in collapsed.chars() {
        if out.chars().count() + 1 >= max_chars {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

fn extract_all_tagged_json<T: serde::de::DeserializeOwned>(text: &str, tag: &str) -> Vec<T> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = text[cursor..].find(&open) {
        let start = cursor + rel + open.len();
        let Some(end_rel) = text[start..].find(&close) else {
            break;
        };
        let body = text[start..start + end_rel].trim();
        if let Ok(value) = serde_json::from_str::<T>(body) {
            out.push(value);
        }
        cursor = start + end_rel + close.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lead_phrases_are_orchestration() {
        assert!(wants_orchestration("act as a team lead"));
        assert!(wants_orchestration("create an agent for lighting"));
        assert!(wants_orchestration("spawn agents on claude and grok"));
        assert!(!wants_orchestration("add a bouncing ball"));
    }

    #[test]
    fn provider_aliases_cover_the_named_models() {
        assert_eq!(provider_alias("Claude"), Some("claude"));
        assert_eq!(provider_alias("gpt 6 astra"), Some("codex"));
        assert_eq!(provider_alias("OpenCode"), Some("opencode"));
        assert_eq!(provider_alias("Grok 4"), Some("grok"));
        assert_eq!(provider_alias("Antigravity"), Some("antigravity"));
        assert_eq!(provider_alias("agy"), Some("antigravity"));
        assert_eq!(provider_alias("kimi"), Some("kimi"));
        assert_eq!(provider_alias("not-a-model"), None);
    }

    #[test]
    fn spawn_tags_parse_and_cap() {
        let text = r#"
<spawn_agent>{"name":"Builder","provider":"claude","task":"make the ball"}</spawn_agent>
<spawn_agent>{"name":"Look","role":"art","provider":"grok","task":"light it"}</spawn_agent>
"#;
        let rows = extract_spawn_agents(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Builder");
        assert_eq!(rows[1].role.as_deref(), Some("art"));
        assert!(extract_spawn_agents(
            "<spawn_agent>{\"name\":\"x\",\"provider\":\"claude\",\"task\":\"\"}</spawn_agent>"
        )
        .is_empty());
    }

    #[test]
    fn status_and_task_tags() {
        assert!(has_agent_status("check them <agent_status></agent_status>"));
        assert!(has_agent_status("now <agent_status/>"));
        let tasks = extract_agent_tasks(
            r#"<agent_task>{"id":"Builder","task":"bounce on the floor"}</agent_task>"#,
        );
        assert_eq!(tasks[0].id, "Builder");
    }
}
