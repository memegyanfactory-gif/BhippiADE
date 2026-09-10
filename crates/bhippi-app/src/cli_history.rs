//! Token consumption from vendor CLI session transcripts — the T3 Code approach.
//!
//! Bhippi's own ledger only records turns this app sent. Claude Code, Codex, and Grok
//! Build also write per-turn usage next to their sessions. Refreshing Usage re-reads
//! those files (never credential files) and overlays the larger figure so the meter
//! matches what the user can check in those tools.

use bhippi_core::{ModelTally, ProviderTally};
use chrono::{Local, TimeZone};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

/// Grok stamps `costUsdTicks` at 10^10 ticks per USD. Micro-dollars are 10^6 per USD.
const GROK_TICKS_PER_MICRO: u64 = 10_000;

const SCAN_STALE: std::time::Duration = std::time::Duration::from_secs(15);
const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// One provider's spend rolled up by local calendar day (`YYYY-MM-DD`).
#[derive(Clone, Debug, Default)]
pub struct CliHistory {
    pub days: BTreeMap<String, BTreeMap<String, ProviderTally>>,
    /// Providers whose home directory existed and was walked, even if empty.
    pub scanned: Vec<String>,
}

impl CliHistory {
    #[must_use]
    pub fn tally_between(&self, from: &str, to: &str) -> BTreeMap<String, ProviderTally> {
        let mut out: BTreeMap<String, ProviderTally> = BTreeMap::new();
        for (date, providers) in &self.days {
            if date.as_str() < from || date.as_str() > to {
                continue;
            }
            for (id, tally) in providers {
                out.entry(id.clone()).or_default().absorb(tally.clone());
            }
        }
        out
    }
}

#[derive(Default)]
pub struct CliHistoryCache {
    history: CliHistory,
    files: HashMap<PathBuf, FileCursor>,
    last_scan: Option<Instant>,
}

#[derive(Clone, Debug)]
struct FileCursor {
    mtime: Option<SystemTime>,
    len: u64,
    turns: Vec<HistoryTurn>,
}

impl CliHistoryCache {
    pub fn refresh(&mut self, force: bool) {
        let due = self
            .last_scan
            .is_none_or(|last| last.elapsed() >= SCAN_STALE);
        if !force && !due {
            return;
        }
        self.history = scan_homes(&mut self.files);
        self.last_scan = Some(Instant::now());
    }

    #[must_use]
    pub fn snapshot(&self) -> CliHistory {
        self.history.clone()
    }
}

fn scan_homes(cursors: &mut HashMap<PathBuf, FileCursor>) -> CliHistory {
    let mut history = CliHistory::default();
    let mut seen = HashMap::new();
    if let Some(root) = grok_sessions_root() {
        history.scanned.push("grok".to_owned());
        walk_named(&root, "updates.jsonl", |path| {
            absorb_file(cursors, &mut seen, path, "grok", parse_grok_updates);
        });
    }
    if let Some(root) = claude_projects_root() {
        history.scanned.push("claude".to_owned());
        walk_jsonl(&root, |path| {
            absorb_file(cursors, &mut seen, path, "claude", parse_claude_jsonl);
        });
    }
    if let Some(root) = codex_sessions_root() {
        history.scanned.push("codex".to_owned());
        walk_jsonl(&root, |path| {
            absorb_file(cursors, &mut seen, path, "codex", parse_codex_jsonl);
        });
    }
    cursors.retain(|path, _| seen.contains_key(path));
    for turns in seen.into_values() {
        for turn in turns {
            record(&mut history, turn);
        }
    }
    history
}

fn absorb_file(
    cursors: &mut HashMap<PathBuf, FileCursor>,
    seen: &mut HashMap<PathBuf, Vec<HistoryTurn>>,
    path: &Path,
    provider: &str,
    parse: fn(&str) -> Vec<HistoryTurn>,
) {
    let Ok(meta) = fs::metadata(path) else {
        return;
    };
    if meta.len() > MAX_FILE_BYTES {
        return;
    }
    let mtime = meta.modified().ok();
    let len = meta.len();
    if let Some(cached) = cursors.get(path) {
        if cached.mtime == mtime && cached.len == len {
            seen.insert(path.to_path_buf(), cached.turns.clone());
            return;
        }
    }
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let mut turns = parse(&text);
    if provider == "grok" && is_grok_subagent(path) {
        turns.clear();
    }
    cursors.insert(
        path.to_path_buf(),
        FileCursor {
            mtime,
            len,
            turns: turns.clone(),
        },
    );
    seen.insert(path.to_path_buf(), turns);
}

fn is_grok_subagent(updates: &Path) -> bool {
    let Some(dir) = updates.parent() else {
        return false;
    };
    let summary = dir.join("summary.json");
    let Ok(text) = fs::read_to_string(summary) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let kind = value
        .get("session_kind")
        .or_else(|| value.pointer("/info/session_kind"))
        .and_then(Value::as_str)
        .unwrap_or("");
    kind.starts_with("subagent")
}

fn record(history: &mut CliHistory, turn: HistoryTurn) {
    let day = history.days.entry(turn.date).or_default();
    let tally = day.entry(turn.provider).or_default();
    let mut piece = ProviderTally {
        input_tokens: turn.input,
        output_tokens: turn.output,
        cost_micros: turn.cost_micros,
        turns: turn.turns,
        balance_micros: None,
        models: BTreeMap::new(),
    };
    if let Some(model) = turn.model {
        if !model.is_empty() {
            piece.models.insert(
                model,
                ModelTally {
                    input_tokens: turn.input,
                    output_tokens: turn.output,
                    cost_micros: turn.cost_micros,
                    turns: turn.turns,
                },
            );
        }
    }
    tally.absorb(piece);
}

#[derive(Clone, Debug)]
struct HistoryTurn {
    date: String,
    provider: String,
    model: Option<String>,
    input: u64,
    output: u64,
    cost_micros: u64,
    turns: u32,
}

fn parse_grok_updates(text: &str) -> Vec<HistoryTurn> {
    let mut prev_in = 0u64;
    let mut prev_out = 0u64;
    let mut prev_ticks = 0u64;
    let mut turns = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let update = value
            .pointer("/params/update")
            .or_else(|| value.get("update"))
            .unwrap_or(&value);
        if update.get("sessionUpdate").and_then(Value::as_str) != Some("turn_completed") {
            continue;
        }
        let Some(usage) = update.get("usage") else {
            continue;
        };
        let input = json_u64(usage.get("inputTokens")).unwrap_or(0);
        let output = json_u64(usage.get("outputTokens")).unwrap_or(0);
        let ticks = json_u64(usage.get("costUsdTicks")).unwrap_or(0);
        let model = usage
            .get("modelUsage")
            .and_then(Value::as_object)
            .and_then(|map| map.keys().next().cloned());
        let date = date_of(
            value
                .get("timestamp")
                .or_else(|| value.pointer("/_meta/agentTimestampMs")),
        );
        let Some(date) = date else {
            continue;
        };
        let delta_in = if input >= prev_in {
            input - prev_in
        } else {
            input
        };
        let delta_out = if output >= prev_out {
            output - prev_out
        } else {
            output
        };
        let delta_ticks = if ticks >= prev_ticks {
            ticks - prev_ticks
        } else {
            ticks
        };
        prev_in = input;
        prev_out = output;
        prev_ticks = ticks;
        if delta_in == 0 && delta_out == 0 && delta_ticks == 0 {
            continue;
        }
        turns.push(HistoryTurn {
            date,
            provider: "grok".to_owned(),
            model,
            input: delta_in,
            output: delta_out,
            cost_micros: delta_ticks / GROK_TICKS_PER_MICRO,
            turns: 1,
        });
    }
    turns
}

fn parse_claude_jsonl(text: &str) -> Vec<HistoryTurn> {
    let mut turns = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let usage = value
            .pointer("/message/usage")
            .or_else(|| value.get("usage"));
        let Some(usage) = usage else {
            continue;
        };
        let input = json_u64(usage.get("input_tokens")).unwrap_or(0)
            + json_u64(usage.get("cache_read_input_tokens")).unwrap_or(0)
            + json_u64(usage.get("cache_creation_input_tokens")).unwrap_or(0);
        let output = json_u64(usage.get("output_tokens")).unwrap_or(0);
        if input == 0 && output == 0 {
            continue;
        }
        let model = value
            .pointer("/message/model")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let date = date_of(value.get("timestamp")).unwrap_or_else(today);
        let cost_micros = bhippi_providers::pricing_for("anthropic", model.as_deref())
            .map_or(0, |price| price.cost_micros(input, output));
        turns.push(HistoryTurn {
            date,
            provider: "claude".to_owned(),
            model,
            input,
            output,
            cost_micros,
            turns: 1,
        });
    }
    turns
}

fn parse_codex_jsonl(text: &str) -> Vec<HistoryTurn> {
    let mut prev_in = 0u64;
    let mut prev_out = 0u64;
    let mut turns = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let payload = value.get("payload").unwrap_or(&value);
        let is_token_count = payload.get("type").and_then(Value::as_str) == Some("token_count")
            || value.get("type").and_then(Value::as_str) == Some("token_count");
        if !is_token_count {
            continue;
        }
        let info = payload.get("info").unwrap_or(payload);
        let last = info
            .get("last_token_usage")
            .or_else(|| info.get("lastTokenUsage"));
        let total = info
            .get("total_token_usage")
            .or_else(|| info.get("totalTokenUsage"));
        let (input, output, incremental) = if let Some(last) = last {
            (
                json_u64(last.get("input_tokens")).unwrap_or(0),
                json_u64(last.get("output_tokens")).unwrap_or(0),
                true,
            )
        } else if let Some(total) = total {
            (
                json_u64(total.get("input_tokens")).unwrap_or(0),
                json_u64(total.get("output_tokens")).unwrap_or(0),
                false,
            )
        } else {
            continue;
        };
        let date = date_of(value.get("timestamp").or_else(|| payload.get("timestamp")))
            .unwrap_or_else(today);
        let (delta_in, delta_out) = if incremental {
            (input, output)
        } else {
            let delta_in = if input >= prev_in {
                input - prev_in
            } else {
                input
            };
            let delta_out = if output >= prev_out {
                output - prev_out
            } else {
                output
            };
            prev_in = input;
            prev_out = output;
            (delta_in, delta_out)
        };
        if delta_in == 0 && delta_out == 0 {
            continue;
        }
        let model = info.get("model").and_then(Value::as_str).map(str::to_owned);
        let cost_micros = bhippi_providers::pricing_for("openai", model.as_deref())
            .map_or(0, |price| price.cost_micros(delta_in, delta_out));
        turns.push(HistoryTurn {
            date,
            provider: "codex".to_owned(),
            model,
            input: delta_in,
            output: delta_out,
            cost_micros,
            turns: 1,
        });
    }
    turns
}

fn json_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(|row| {
        row.as_u64()
            .or_else(|| row.as_i64().and_then(|n| u64::try_from(n).ok()))
            .or_else(|| row.as_f64().and_then(|n| (n >= 0.0).then_some(n as u64)))
            .or_else(|| row.as_str().and_then(|text| text.parse().ok()))
    })
}

fn date_of(value: Option<&Value>) -> Option<String> {
    let value = value?;
    let secs = if let Some(n) = value.as_i64() {
        if n > 10_000_000_000 {
            n / 1000
        } else {
            n
        }
    } else if let Some(n) = value.as_u64() {
        let n = i64::try_from(n).ok()?;
        if n > 10_000_000_000 {
            n / 1000
        } else {
            n
        }
    } else {
        let text = value.as_str()?;
        return chrono::DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|stamp| stamp.with_timezone(&Local).date_naive().to_string())
            .or_else(|| text.get(..10).map(str::to_owned));
    };
    Local
        .timestamp_opt(secs, 0)
        .single()
        .map(|stamp| stamp.date_naive().to_string())
}

fn today() -> String {
    Local::now().date_naive().to_string()
}

fn grok_sessions_root() -> Option<PathBuf> {
    std::env::var_os("GROK_HOME")
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".grok")))
        .map(|root| root.join("sessions"))
        .filter(|path| path.is_dir())
}

fn claude_projects_root() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".claude")))
        .map(|root| root.join("projects"))
        .filter(|path| path.is_dir())
}

fn codex_sessions_root() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".codex")))
        .map(|root| root.join("sessions"))
        .filter(|path| path.is_dir())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn walk_named(root: &Path, filename: &str, mut visit: impl FnMut(&Path)) {
    walk(root, &mut |path| {
        if path.file_name().and_then(|name| name.to_str()) == Some(filename) {
            visit(path);
        }
    });
}

fn walk_jsonl(root: &Path, mut visit: impl FnMut(&Path)) {
    walk(root, &mut |path| {
        if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            visit(path);
        }
    });
}

fn walk(root: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
        } else {
            visit(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grok_turn_completed_deltas_become_one_row_per_turn() {
        let jsonl = r#"
{"timestamp":1786813790,"method":"_x.ai/session/update","params":{"update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":1000,"outputTokens":40,"costUsdTicks":200000000,"modelUsage":{"grok-4.6-build":{}}}}}}
{"timestamp":1786813795,"method":"_x.ai/session/update","params":{"update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":2500,"outputTokens":90,"costUsdTicks":500000000,"modelUsage":{"grok-4.6-build":{}}}}}}
"#;
        let turns = parse_grok_updates(jsonl);
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].input, 1000);
        assert_eq!(turns[0].output, 40);
        assert_eq!(turns[0].cost_micros, 20_000);
        assert_eq!(turns[1].input, 1500);
        assert_eq!(turns[1].output, 50);
        assert_eq!(turns[1].cost_micros, 30_000);
        assert_eq!(turns[0].model.as_deref(), Some("grok-4.6-build"));
        assert_eq!(turns[0].provider, "grok");
    }

    #[test]
    fn claude_assistant_usage_counts_cache_as_input() {
        let jsonl = r#"{"type":"assistant","timestamp":"2026-09-08T12:00:00Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":100,"output_tokens":20,"cache_read_input_tokens":400,"cache_creation_input_tokens":50}}}"#;
        let turns = parse_claude_jsonl(jsonl);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].input, 550);
        assert_eq!(turns[0].output, 20);
        assert_eq!(turns[0].provider, "claude");
        assert!(
            turns[0].cost_micros > 0,
            "API-equivalent cost from list prices"
        );
    }

    #[test]
    fn codex_last_token_usage_is_per_turn() {
        let jsonl = r#"{"type":"event_msg","timestamp":"2026-09-08T12:00:00Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":80,"output_tokens":12}}}}"#;
        let turns = parse_codex_jsonl(jsonl);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].input, 80);
        assert_eq!(turns[0].output, 12);
        assert_eq!(turns[0].provider, "codex");
    }
}
