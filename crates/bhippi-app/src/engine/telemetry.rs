//! Bounded editor/runtime telemetry shared by the Output Log and the agent read surface.
//!
//! This is deliberately process-local and retrieval-only. It gives `get_console` and
//! `get_play_stats` a real source without putting transient frame data in the scene or DB.

use crate::commands::AppError;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

const MAX_CONSOLE_ROWS: usize = 200;
const MAX_TEXT_BYTES: usize = 2_000;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EngineConsoleRow {
    pub id: u64,
    pub at: String,
    pub level: String,
    pub channel: String,
    pub text: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Type)]
pub struct EnginePlayStats {
    pub fps: f64,
    pub frame_ms: f64,
    pub entities: u32,
    pub simulated_bodies: u32,
    pub contacts: u32,
    pub draw_calls: u32,
    pub scripts: u32,
    pub script_faults: u32,
    pub elapsed: f64,
    pub paused: bool,
}

#[derive(Default)]
struct Telemetry {
    console: VecDeque<EngineConsoleRow>,
    next_console_id: u64,
    play: Option<EnginePlayStats>,
}

fn telemetry() -> &'static Mutex<Telemetry> {
    static STORE: OnceLock<Mutex<Telemetry>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(Telemetry::default()))
}

#[tauri::command]
#[specta::specta]
pub async fn engine_record_console(
    level: String,
    channel: String,
    text: String,
) -> Result<(), AppError> {
    record_console(level, channel, text, None, None)
}

#[tauri::command]
#[specta::specta]
pub async fn engine_record_console_source(
    level: String,
    channel: String,
    text: String,
    file: String,
    line: u32,
) -> Result<(), AppError> {
    if file.is_empty() || file.len() > 1_024 || file.contains("..") || line == 0 {
        return Err(AppError::plain(
            "Console source must be a project-relative file and positive line.",
        ));
    }
    record_console(
        level,
        channel,
        text,
        Some(file.replace('\\', "/")),
        Some(line),
    )
}

fn record_console(
    level: String,
    channel: String,
    text: String,
    file: Option<String>,
    line: Option<u32>,
) -> Result<(), AppError> {
    if !matches!(level.as_str(), "debug" | "info" | "warn" | "error") {
        return Err(AppError::plain(
            "Console level must be debug, info, warn, or error.",
        ));
    }
    if channel.is_empty() || channel.len() > 64 || text.is_empty() || text.len() > MAX_TEXT_BYTES {
        return Err(AppError::plain(
            "Console channel or message is outside the bounded size.",
        ));
    }
    let mut store = telemetry()
        .lock()
        .map_err(|_| AppError::plain("Engine telemetry is unavailable."))?;
    store.next_console_id = store.next_console_id.saturating_add(1);
    let id = store.next_console_id;
    store.console.push_back(EngineConsoleRow {
        id,
        at: chrono::Utc::now().to_rfc3339(),
        level,
        channel,
        text: redact(&text),
        file,
        line,
    });
    while store.console.len() > MAX_CONSOLE_ROWS {
        store.console.pop_front();
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn engine_console_rows(
    level: Option<String>,
    channel: Option<String>,
    search: Option<String>,
    offset: u32,
    limit: u32,
) -> Result<Vec<EngineConsoleRow>, AppError> {
    let store = telemetry()
        .lock()
        .map_err(|_| AppError::plain("Engine telemetry is unavailable."))?;
    Ok(filtered_rows(
        &store,
        level.as_deref(),
        channel.as_deref(),
        search.as_deref(),
        offset as usize,
        limit as usize,
    ))
}

#[tauri::command]
#[specta::specta]
pub async fn engine_clear_play_stats() -> Result<(), AppError> {
    telemetry()
        .lock()
        .map_err(|_| AppError::plain("Engine telemetry is unavailable."))?
        .play = None;
    Ok(())
}

fn redact(text: &str) -> String {
    text.split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if ["api_key=", "apikey=", "token=", "password=", "secret="]
                .iter()
                .any(|marker| lower.contains(marker))
            {
                let key = token.split('=').next().unwrap_or("secret");
                format!("{key}=[redacted]")
            } else {
                token.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[tauri::command]
#[specta::specta]
pub async fn engine_record_play_stats(stats: EnginePlayStats) -> Result<(), AppError> {
    if !stats.fps.is_finite()
        || !stats.frame_ms.is_finite()
        || !stats.elapsed.is_finite()
        || stats.fps < 0.0
        || stats.frame_ms < 0.0
        || stats.elapsed < 0.0
    {
        return Err(AppError::plain(
            "Play statistics must be finite and non-negative.",
        ));
    }
    telemetry()
        .lock()
        .map_err(|_| AppError::plain("Engine telemetry is unavailable."))?
        .play = Some(stats);
    Ok(())
}

pub fn console_answer(
    level: Option<&str>,
    channel: Option<&str>,
    search: Option<&str>,
    offset: usize,
    limit: usize,
) -> String {
    let Ok(store) = telemetry().lock() else {
        return "The engine console is unavailable.".to_owned();
    };
    if store.console.is_empty() {
        return "The engine console is empty.".to_owned();
    }
    let rows = filtered_rows(&store, level, channel, search, offset, limit);
    let mut out = String::from("Latest engine console rows (newest first):\n");
    for row in rows {
        let source = row
            .file
            .as_deref()
            .zip(row.line)
            .map(|(file, line)| format!(" {file}:{line}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "[{}] [{}]{} {}\n",
            row.level, row.channel, source, row.text
        ));
    }
    out
}

fn filtered_rows(
    store: &Telemetry,
    level: Option<&str>,
    channel: Option<&str>,
    search: Option<&str>,
    offset: usize,
    limit: usize,
) -> Vec<EngineConsoleRow> {
    let needle = search.map(str::to_ascii_lowercase);
    store
        .console
        .iter()
        .rev()
        .filter(|row| level.is_none_or(|wanted| row.level == wanted))
        .filter(|row| channel.is_none_or(|wanted| row.channel == wanted))
        .filter(|row| {
            needle
                .as_ref()
                .is_none_or(|wanted| row.text.to_ascii_lowercase().contains(wanted))
        })
        .skip(offset)
        .take(limit.min(40))
        .cloned()
        .collect()
}

pub fn play_stats_answer() -> String {
    let Ok(store) = telemetry().lock() else {
        return "Play statistics are unavailable.".to_owned();
    };
    let Some(stats) = &store.play else {
        return "No play session has reported statistics yet.".to_owned();
    };
    format!(
        "Play stats: {:.0} fps, {:.2} ms, {} entities, {} bodies, {} contacts, {} draw calls, {} scripts, {} script faults, {:.2}s elapsed, paused={}.",
        stats.fps,
        stats.frame_ms,
        stats.entities,
        stats.simulated_bodies,
        stats.contacts,
        stats.draw_calls,
        stats.scripts,
        stats.script_faults,
        stats.elapsed,
        stats.paused,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        console_answer, engine_clear_play_stats, engine_console_rows, engine_record_console,
        engine_record_console_source, engine_record_play_stats, play_stats_answer, EnginePlayStats,
    };

    #[tokio::test]
    async fn console_and_play_stats_are_real_bounded_query_sources() {
        engine_record_console(
            "error".to_owned(),
            "script".to_owned(),
            "line 7 failed".to_owned(),
        )
        .await
        .expect("record console");
        assert!(console_answer(None, None, None, 0, 40).contains("line 7 failed"));
        engine_record_console(
            "info".to_owned(),
            "provider".to_owned(),
            "token=do-not-return connected".to_owned(),
        )
        .await
        .expect("record redacted console");
        let console = console_answer(None, Some("provider"), Some("token"), 0, 40);
        assert!(console.contains("token=[redacted]"));
        assert!(!console.contains("do-not-return"));
        engine_record_console_source(
            "error".to_owned(),
            "script".to_owned(),
            "fixture fault".to_owned(),
            "assets/scripts/player.bhs".to_owned(),
            7,
        )
        .await
        .expect("record source");
        let sourced = engine_console_rows(
            Some("error".to_owned()),
            Some("script".to_owned()),
            Some("fixture".to_owned()),
            0,
            40,
        )
        .await
        .expect("query rows");
        assert_eq!(
            sourced[0].file.as_deref(),
            Some("assets/scripts/player.bhs")
        );
        assert_eq!(sourced[0].line, Some(7));
        assert!(console_answer(None, None, Some("fixture"), 0, 40)
            .contains("assets/scripts/player.bhs:7"));

        engine_record_play_stats(EnginePlayStats {
            fps: 60.0,
            frame_ms: 16.67,
            entities: 12,
            simulated_bodies: 4,
            contacts: 2,
            draw_calls: 8,
            scripts: 1,
            script_faults: 0,
            elapsed: 3.0,
            paused: false,
        })
        .await
        .expect("record stats");
        let answer = play_stats_answer();
        assert!(answer.contains("60 fps"));
        assert!(answer.contains("12 entities"));
        engine_clear_play_stats().await.expect("clear stats");
        assert!(play_stats_answer().contains("No play session"));
    }
}
