//! Provider-owned account and plan-usage probes.
//!
//! These are deliberately separate from fast provider detection (INV-062). Every probe
//! invokes a vendor's public status surface with explicit argv and a scrubbed environment;
//! credential files and secret values are never read (INV-002/INV-003).

use crate::command::{resolve_command, resolve_stdio_command, ResolvedCommand};
use crate::model::{AccountUsage, AccountUsageStatus, PlanWindow, ProviderInfo, ProviderKind};
use chrono::{Datelike, Local, NaiveDate, NaiveTime, TimeZone, Utc};
use futures_util::future::join_all;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const USAGE_PROBE_TIMEOUT: Duration = Duration::from_secs(12);

/// Probes every installed CLI concurrently. Local/demo providers have no vendor account.
pub async fn probe_accounts(providers: &[ProviderInfo]) -> BTreeMap<String, AccountUsage> {
    let probes = providers
        .iter()
        .filter(|row| row.kind == ProviderKind::Cli && row.installed)
        .map(|row| async move { (row.id.clone(), probe_account(&row.id).await) });
    join_all(probes).await.into_iter().collect()
}

/// Reads one account through only the provider's own non-secret status command/protocol.
pub async fn probe_account(provider_id: &str) -> AccountUsage {
    let command_name = crate::spec(provider_id)
        .and_then(|entry| entry.binary)
        .unwrap_or(provider_id);
    let binary = if provider_id == "codex" {
        resolve_stdio_command(command_name).or_else(|| resolve_command(command_name))
    } else {
        resolve_command(command_name)
    };
    let Some(binary) = binary else {
        return snapshot(
            AccountUsageStatus::Unavailable,
            "The provider CLI is not available.",
        );
    };
    match provider_id {
        "claude" => probe_claude(&binary).await,
        "codex" => probe_codex(&binary).await,
        "opencode" => probe_opencode(&binary).await,
        "grok" => probe_grok(&binary).await,
        "antigravity" => probe_antigravity(&binary).await,
        "kimi" => probe_kimi(&binary).await,
        _ => snapshot(
            AccountUsageStatus::NotReported,
            "This provider does not expose account or weekly plan usage through its CLI.",
        ),
    }
}

async fn probe_claude(binary: &ResolvedCommand) -> AccountUsage {
    let identity = match run_output(binary, &["auth", "status", "--json"]).await {
        Ok(stdout) => parse_claude_status(&stdout),
        Err(reason) => return unavailable(reason),
    };
    if identity.status != AccountUsageStatus::Authenticated {
        return identity;
    }
    match run_output_timed(
        binary,
        &[
            "-p",
            "/usage",
            "--output-format",
            "json",
            "--max-turns",
            "0",
            "--strict-mcp-config",
        ],
        USAGE_PROBE_TIMEOUT,
    )
    .await
    {
        Ok(stdout) => merge_claude_usage(identity, &stdout),
        Err(_) => identity,
    }
}

async fn probe_opencode(binary: &ResolvedCommand) -> AccountUsage {
    match run_output(binary, &["auth", "list"]).await {
        Ok(stdout) => parse_opencode_status(&stdout),
        Err(_) => match run_output(binary, &["providers", "list"]).await {
            Ok(stdout) => parse_opencode_status(&stdout),
            Err(reason) => unavailable(reason),
        },
    }
}

async fn probe_grok(binary: &ResolvedCommand) -> AccountUsage {
    // Identity still comes from `grok models` — a non-spending status surface.
    // Live credits come from Grok's own ACP `_x.ai/billing` method (the same
    // GetGrokCreditsConfig call the TUI `/usage` panel uses), not from reading
    // `auth.json` and not from a model turn.
    let identity = match run_output(binary, &["models"]).await {
        Ok(stdout) => parse_grok_status(&stdout),
        Err(reason) => return unavailable(reason),
    };
    if identity.status != AccountUsageStatus::Authenticated
        && identity.status != AccountUsageStatus::Live
    {
        return identity;
    }
    match probe_grok_billing(binary).await {
        Ok(stdout) => merge_grok_usage(identity, &stdout),
        Err(_) => identity,
    }
}

/// Asks the Grok CLI for the signed-in account's credits config over ACP stdio.
///
/// `--no-leader` is load-bearing: attaching to a running Grok leader would steal
/// that session. This probe is a short-lived sidecar, like Codex `app-server --stdio`.
async fn probe_grok_billing(binary: &ResolvedCommand) -> Result<String, String> {
    let mut command = binary.command();
    command
        .args(["agent", "--no-leader", "--always-approve", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| format!("Grok billing service could not start: {error}"))?;
    let Some(mut stdin) = child.stdin.take() else {
        return Err("Grok billing service did not open its input.".to_owned());
    };
    let Some(stdout) = child.stdout.take() else {
        return Err("Grok billing service did not open its output.".to_owned());
    };
    let exchange = async {
        write_json_line(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": 1,
                    "clientInfo": {
                        "name": "bhippi",
                        "title": "Bhippi",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "clientCapabilities": {}
                }
            }),
        )
        .await?;
        let mut lines = BufReader::new(stdout).lines();
        let mut initialized = false;
        let mut billing = None;
        while let Some(line) = lines.next_line().await.map_err(|error| error.to_string())? {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            match value.get("id").and_then(json_i64) {
                Some(1) => {
                    write_json_line(&mut stdin, &json!({"jsonrpc":"2.0","method":"initialized"}))
                        .await?;
                    write_json_line(
                        &mut stdin,
                        &json!({"jsonrpc":"2.0","id":2,"method":"_x.ai/billing","params":{}}),
                    )
                    .await?;
                    initialized = true;
                }
                Some(2) => {
                    if let Some(error) = value.get("error") {
                        return Err(error
                            .pointer("/message")
                            .and_then(Value::as_str)
                            .unwrap_or("_x.ai/billing failed")
                            .to_owned());
                    }
                    billing = value.get("result").cloned();
                    break;
                }
                _ => {}
            }
            if initialized && billing.is_some() {
                break;
            }
        }
        billing
            .ok_or_else(|| "Grok did not return a billing snapshot.".to_owned())
            .map(|value| value.to_string())
    };
    let result = tokio::time::timeout(USAGE_PROBE_TIMEOUT, exchange).await;
    let _ = child.start_kill();
    match result {
        Ok(Ok(body)) => Ok(body),
        Ok(Err(reason)) => Err(reason),
        Err(_) => Err("Grok billing protocol timed out.".to_owned()),
    }
}

async fn probe_antigravity(binary: &ResolvedCommand) -> AccountUsage {
    match run_output(binary, &["models"]).await {
        Ok(stdout) => parse_antigravity_status(&stdout),
        Err(reason) => {
            let lower = reason.to_ascii_lowercase();
            if lower.contains("auth") || lower.contains("sign in") || lower.contains("logged") {
                snapshot(
                    AccountUsageStatus::SignedOut,
                    "Antigravity is signed out. Run agy in a terminal and sign in with your Google account.",
                )
            } else {
                unavailable(reason)
            }
        }
    }
}

fn parse_antigravity_status(stdout: &str) -> AccountUsage {
    let clean = strip_ansi(stdout);
    let lower = clean.to_ascii_lowercase();
    if lower.contains("authentication required")
        || lower.contains("not logged")
        || lower.contains("signed out")
        || lower.contains("please sign in")
    {
        return snapshot(
            AccountUsageStatus::SignedOut,
            "Antigravity is signed out. Run agy in a terminal and sign in with your Google account.",
        );
    }
    let models = crate::detect::parse_model_lines(&clean);
    let account = grok_account_label(&clean)
        .or_else(|| (!models.is_empty()).then(|| "Google account".to_owned()));
    let Some(account) = account else {
        return snapshot(
            AccountUsageStatus::NotReported,
            "Antigravity is installed, but this CLI did not report the signed-in account.",
        );
    };
    AccountUsage {
        account_name: Some(account),
        plan: None,
        status: AccountUsageStatus::Authenticated,
        session: None,
        weekly: None,
        prepaid_usd: None,
        note: "Antigravity is signed in on this machine. Weekly allowance is not exposed through the CLI."
            .to_owned(),
        refreshed_at: Utc::now(),
    }
}

async fn probe_kimi(binary: &ResolvedCommand) -> AccountUsage {
    match run_output(binary, &["models"]).await {
        Ok(stdout) => parse_kimi_status(&stdout),
        Err(_) => match run_output(binary, &["--version"]).await {
            Ok(_) => snapshot(
                AccountUsageStatus::NotReported,
                "Kimi is installed, but this CLI does not expose the signed-in account or a weekly allowance.",
            ),
            Err(reason) => unavailable(reason),
        },
    }
}

async fn run_output(binary: &ResolvedCommand, args: &[&str]) -> Result<String, String> {
    run_output_timed(binary, args, PROBE_TIMEOUT).await
}

async fn run_output_timed(
    binary: &ResolvedCommand,
    args: &[&str],
    timeout: Duration,
) -> Result<String, String> {
    let mut command = binary.command();
    command.args(args).stdin(Stdio::null());
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| "The provider account probe timed out.".to_owned())?
        .map_err(|error| format!("The provider account probe could not start: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() || !stdout.is_empty() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default();
        Err(if detail.is_empty() {
            format!("The provider account probe exited with {}.", output.status)
        } else {
            format!("The provider account probe failed: {}", detail.trim())
        })
    }
}

fn parse_claude_status(stdout: &str) -> AccountUsage {
    let Ok(value) = serde_json::from_str::<Value>(stdout) else {
        return snapshot(
            AccountUsageStatus::Unavailable,
            "Claude returned an account-status shape this version does not recognise.",
        );
    };
    if value.get("loggedIn").and_then(Value::as_bool) != Some(true) {
        return snapshot(AccountUsageStatus::SignedOut, "Claude Code is signed out.");
    }
    AccountUsage {
        account_name: text_at(&value, &["email"]).or_else(|| text_at(&value, &["orgName"])),
        plan: text_at(&value, &["subscriptionType"]),
        status: AccountUsageStatus::Authenticated,
        session: None,
        weekly: None,
        prepaid_usd: None,
        note:
            "Claude account detected. Weekly allowance is read from /usage without spending a turn."
                .to_owned(),
        refreshed_at: Utc::now(),
    }
}

fn merge_claude_usage(mut identity: AccountUsage, stdout: &str) -> AccountUsage {
    let text = extract_claude_usage_text(stdout);
    let (session, weekly) = parse_usage_windows(&text);
    if session.is_none() && weekly.is_none() {
        return identity;
    }
    identity.session = session;
    identity.weekly = weekly;
    identity.status = AccountUsageStatus::Live;
    identity.note = "Live from the signed-in Claude account via /usage (no model turn).".to_owned();
    identity.refreshed_at = Utc::now();
    identity
}

fn extract_claude_usage_text(stdout: &str) -> String {
    let trimmed = stdout.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(result) = value.get("result").and_then(Value::as_str) {
            return result.to_owned();
        }
    }
    for line in trimmed.lines().rev() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        if let Some(result) = value.get("result").and_then(Value::as_str) {
            return result.to_owned();
        }
        if let Some(text) = value
            .pointer("/message/content/0/text")
            .and_then(Value::as_str)
        {
            return text.to_owned();
        }
    }
    trimmed.to_owned()
}

fn parse_usage_windows(text: &str) -> (Option<PlanWindow>, Option<PlanWindow>) {
    let mut session = None;
    let mut weekly = None;
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        let Some(percent) = parse_percent_used(&lower) else {
            continue;
        };
        let resets_at = line
            .split_once("resets")
            .or_else(|| line.split_once("Resets"))
            .and_then(|(_, rest)| parse_reset_stamp(rest));
        let used_fraction = (percent / 100.0).clamp(0.0, 1.0);
        if is_weekly_line(&lower) {
            weekly = Some(PlanWindow {
                used_fraction,
                resets_at,
                duration_minutes: Some(10_080),
            });
        } else if is_session_line(&lower) {
            session = Some(PlanWindow {
                used_fraction,
                resets_at,
                duration_minutes: Some(300),
            });
        }
    }
    (session, weekly)
}

fn is_weekly_line(line: &str) -> bool {
    let scoped = line.contains("sonnet")
        || line.contains("opus")
        || line.contains("haiku")
        || line.contains("fable");
    if scoped {
        return false;
    }
    line.contains("current week")
        || line.contains("weekly")
        || line.contains("this week")
        || line.contains("7-day")
        || line.contains("seven-day")
        || line.contains("seven day")
        || line.contains("grok build")
        || line.contains("super grok")
}

fn is_session_line(line: &str) -> bool {
    line.contains("current session")
        || line.contains("5-hour")
        || line.contains("5 hour")
        || line.contains("5h window")
        || line.contains("session limit")
}

fn parse_percent_used(line: &str) -> Option<f32> {
    let idx = line.find('%')?;
    let prefix = line[..idx].trim_end();
    let number = prefix
        .rsplit(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .next()
        .filter(|text| !text.is_empty())?;
    let value: f32 = number.parse().ok()?;
    let remaining =
        (line.contains("left") || line.contains("leftover") || line.contains("remaining"))
            && !line.contains("used");
    if remaining {
        Some((100.0 - value).clamp(0.0, 100.0))
    } else {
        Some(value)
    }
}

fn parse_reset_stamp(raw: &str) -> Option<i64> {
    let trimmed = raw.trim().trim_start_matches([':', '·', '•', '-']).trim();
    if let Some(relative) = parse_relative_reset(trimmed) {
        return Some(relative);
    }
    let core = trimmed
        .split('(')
        .next()
        .unwrap_or(trimmed)
        .trim()
        .trim_end_matches('.')
        .trim();
    parse_calendar_reset(core)
}

fn parse_relative_reset(raw: &str) -> Option<i64> {
    let lower = raw.to_ascii_lowercase();
    let rest = lower.strip_prefix("in ").unwrap_or(lower.as_str()).trim();
    if rest == "shortly" || rest == "soon" {
        return Some(Utc::now().timestamp());
    }
    let mut minutes: i64 = 0;
    let mut saw = false;
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        if let Some(value) = parse_leading_number(token) {
            let unit = token
                .trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == '.')
                .trim_start_matches(['h', 'm', 'd'])
                .is_empty()
                .then(|| {
                    token
                        .chars()
                        .filter(char::is_ascii_alphabetic)
                        .collect::<String>()
                })
                .filter(|unit| !unit.is_empty())
                .or_else(|| {
                    tokens
                        .get(i + 1)
                        .map(|unit| unit.trim_end_matches('s').to_owned())
                });
            match unit.as_deref() {
                Some("d") | Some("day") => {
                    minutes += value * 24 * 60;
                    saw = true;
                }
                Some("h") | Some("hr") | Some("hour") => {
                    minutes += value * 60;
                    saw = true;
                }
                Some("m") | Some("min") | Some("minute") => {
                    minutes += value;
                    saw = true;
                }
                _ => {}
            }
        } else if let Some(compact) = parse_compact_duration(token) {
            minutes += compact;
            saw = true;
        }
        i += 1;
    }
    saw.then(|| Utc::now().timestamp() + minutes * 60)
}

fn parse_leading_number(token: &str) -> Option<i64> {
    let digits: String = token.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn parse_compact_duration(token: &str) -> Option<i64> {
    // "3h12m", "4h", "12m"
    let mut minutes = 0i64;
    let mut number = String::new();
    let mut saw = false;
    for ch in token.chars() {
        if ch.is_ascii_digit() {
            number.push(ch);
            continue;
        }
        let value: i64 = number.parse().ok()?;
        number.clear();
        match ch {
            'd' => minutes += value * 24 * 60,
            'h' => minutes += value * 60,
            'm' => minutes += value,
            _ => return None,
        }
        saw = true;
    }
    saw.then_some(minutes)
}

fn parse_calendar_reset(raw: &str) -> Option<i64> {
    let normalized = raw.replace(',', " ");
    let mut month = None;
    let mut day = None;
    let mut hour = None;
    let mut minute = 0;
    let mut pm = None;
    for token in normalized.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if let Some(value) = month_number(&lower) {
            month = Some(value);
            continue;
        }
        if month.is_some() && day.is_none() {
            if let Ok(value) = lower.parse::<u32>() {
                if (1..=31).contains(&value) {
                    day = Some(value);
                    continue;
                }
            }
        }
        let (time_token, mer) = split_meridiem(&lower);
        if let Some(flag) = mer {
            pm = Some(flag);
        }
        if hour.is_none() {
            if let Some((h, m)) = parse_clock(time_token) {
                hour = Some(h);
                minute = m;
            }
        } else if pm.is_none() {
            if lower == "am" {
                pm = Some(false);
            } else if lower == "pm" {
                pm = Some(true);
            }
        }
    }
    let month = month?;
    let day = day?;
    let mut hour = hour?;
    if let Some(is_pm) = pm {
        hour = to_24h(hour, is_pm);
    }
    let now = Local::now();
    let mut year = now.year();
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
    let mut datetime = date.and_time(time);
    if datetime < now.naive_local() - chrono::Duration::hours(12) {
        year += 1;
        datetime = NaiveDate::from_ymd_opt(year, month, day)?.and_time(time);
    }
    Local
        .from_local_datetime(&datetime)
        .single()
        .map(|stamp| stamp.timestamp())
}

fn month_number(token: &str) -> Option<u32> {
    match token {
        "jan" | "january" => Some(1),
        "feb" | "february" => Some(2),
        "mar" | "march" => Some(3),
        "apr" | "april" => Some(4),
        "may" => Some(5),
        "jun" | "june" => Some(6),
        "jul" | "july" => Some(7),
        "aug" | "august" => Some(8),
        "sep" | "sept" | "september" => Some(9),
        "oct" | "october" => Some(10),
        "nov" | "november" => Some(11),
        "dec" | "december" => Some(12),
        _ => None,
    }
}

fn split_meridiem(token: &str) -> (&str, Option<bool>) {
    if let Some(core) = token.strip_suffix("am") {
        (core, Some(false))
    } else if let Some(core) = token.strip_suffix("pm") {
        (core, Some(true))
    } else {
        (token, None)
    }
}

fn parse_clock(token: &str) -> Option<(u32, u32)> {
    if token.is_empty() {
        return None;
    }
    let mut parts = token.split(':');
    let hour = parts.next()?.parse().ok()?;
    let minute = parts.next().unwrap_or("0").parse().ok()?;
    Some((hour, minute))
}

fn to_24h(hour: u32, pm: bool) -> u32 {
    match (hour, pm) {
        (12, false) => 0,
        (12, true) => 12,
        (h, true) => h + 12,
        (h, false) => h,
    }
}

fn parse_opencode_status(stdout: &str) -> AccountUsage {
    let clean = strip_ansi(stdout);
    let names: Vec<String> = clean
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix('•')
                .or_else(|| trimmed.strip_prefix('●'))
                .or_else(|| trimmed.strip_prefix('*'))
        })
        .filter_map(|line| {
            let mut words: Vec<&str> = line.split_whitespace().collect();
            if matches!(words.last(), Some(kind) if ["api", "oauth", "wellknown"].contains(kind)) {
                let _ = words.pop();
            }
            (!words.is_empty()).then(|| words.join(" "))
        })
        .filter(|name| !name.contains(".json") && !name.to_ascii_lowercase().contains("credential"))
        .collect();
    if names.is_empty() {
        return snapshot(
            AccountUsageStatus::SignedOut,
            "OpenCode has no configured provider account.",
        );
    }
    AccountUsage {
        account_name: Some(names.join(", ")),
        plan: Some("api".to_owned()),
        status: AccountUsageStatus::Authenticated,
        session: None,
        weekly: None,
        prepaid_usd: None,
        note: "OpenCode reports the configured backend, but not a subscription weekly allowance."
            .to_owned(),
        refreshed_at: Utc::now(),
    }
}

fn parse_grok_status(stdout: &str) -> AccountUsage {
    let clean = strip_ansi(stdout);
    let account = grok_account_label(&clean);
    let Some(account) = account else {
        return snapshot(AccountUsageStatus::SignedOut, "Grok is signed out.");
    };
    AccountUsage {
        account_name: Some(account),
        plan: None,
        status: AccountUsageStatus::Authenticated,
        session: None,
        weekly: None,
        prepaid_usd: None,
        note: "Grok is signed in. Live credits come from the CLI billing protocol on refresh."
            .to_owned(),
        refreshed_at: Utc::now(),
    }
}

fn merge_grok_usage(mut identity: AccountUsage, stdout: &str) -> AccountUsage {
    if let Some(from_json) = parse_grok_usage_json(stdout) {
        if from_json.weekly.is_some()
            || from_json.session.is_some()
            || from_json.prepaid_usd.is_some()
            || from_json.plan.is_some()
        {
            identity.session = from_json.session.or(identity.session);
            identity.weekly = from_json.weekly.or(identity.weekly);
            identity.prepaid_usd = from_json.prepaid_usd.or(identity.prepaid_usd);
            identity.plan = from_json.plan.or(identity.plan);
            identity.status = AccountUsageStatus::Live;
            identity.note =
                "Live from the signed-in Grok account via billing (no model turn).".to_owned();
            identity.refreshed_at = Utc::now();
            return identity;
        }
    }
    let text = extract_claude_usage_text(stdout);
    let (session, weekly) = parse_usage_windows(&text);
    if session.is_none() && weekly.is_none() {
        return identity;
    }
    identity.session = session.or(identity.session);
    identity.weekly = weekly.or(identity.weekly);
    identity.status = AccountUsageStatus::Live;
    identity.note = "Live from the signed-in Grok account via billing (no model turn).".to_owned();
    identity.refreshed_at = Utc::now();
    identity
}

fn parse_grok_usage_json(stdout: &str) -> Option<AccountUsage> {
    let trimmed = stdout.trim();
    let value = serde_json::from_str::<Value>(trimmed).ok().or_else(|| {
        trimmed.lines().rev().find_map(|line| {
            serde_json::from_str::<Value>(line.trim())
                .ok()
                .filter(|row| {
                    row.get("usage").is_some()
                        || row.get("usedPercent").is_some()
                        || row.get("creditUsagePercent").is_some()
                        || row.get("config").is_some()
                        || row.get("result").is_some()
                        || row.pointer("/billingCycle").is_some()
                })
        })
    })?;
    let payload = value
        .get("result")
        .and_then(|result| {
            result
                .as_str()
                .and_then(|text| serde_json::from_str::<Value>(text).ok())
                .or_else(|| result.as_object().cloned().map(Value::Object))
        })
        .unwrap_or(value);
    let config = payload.get("config").unwrap_or(&payload);
    let has_config = payload.get("config").is_some();

    let used_percent = json_f32(
        config
            .get("creditUsagePercent")
            .or_else(|| payload.get("creditUsagePercent"))
            .or_else(|| payload.pointer("/usage/usedPercent"))
            .or_else(|| payload.get("usedPercent"))
            .or_else(|| payload.pointer("/rateLimits/primary/usedPercent")),
    )
    .or_else(|| {
        let used = json_f32(
            config
                .pointer("/used/val")
                .or_else(|| payload.pointer("/usage/totalUsed/val"))
                .or_else(|| payload.pointer("/usage/totalUsed")),
        )?;
        let limit = json_f32(
            config
                .pointer("/monthlyLimit/val")
                .or_else(|| payload.pointer("/usage/monthlyLimit/val"))
                .or_else(|| payload.pointer("/usage/monthlyLimit"))
                .or_else(|| payload.get("monthlyLimit")),
        )?;
        if limit <= 0.0 {
            None
        } else {
            Some((used / limit) * 100.0)
        }
    })
    .or_else(|| has_config.then_some(0.0))?;

    let period = config.get("currentPeriod");
    let period_type = period
        .and_then(|row| row.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let resets_at = period
        .and_then(|row| row.get("end"))
        .and_then(Value::as_str)
        .and_then(parse_rfc3339_secs)
        .or_else(|| {
            config
                .get("billingPeriodEnd")
                .or_else(|| payload.pointer("/billingCycle/billingPeriodEnd"))
                .or_else(|| payload.get("resetsAt"))
                .and_then(|value| {
                    json_i64(value).or_else(|| value.as_str().and_then(parse_rfc3339_secs))
                })
        });
    let duration = period
        .and_then(|row| {
            let start = row
                .get("start")
                .and_then(Value::as_str)
                .and_then(parse_rfc3339_secs)?;
            let end = row
                .get("end")
                .and_then(Value::as_str)
                .and_then(parse_rfc3339_secs)?;
            Some(((end - start).max(0) / 60) as u64)
        })
        .or_else(|| {
            json_u64(
                payload
                    .get("billingPeriodMinutes")
                    .or_else(|| payload.pointer("/billingCycle/billingPeriodMinutes"))
                    .or_else(|| payload.pointer("/rateLimits/primary/windowDurationMins")),
            )
        })
        .or_else(|| {
            if period_type.contains("WEEKLY") {
                Some(10_080)
            } else if period_type.contains("MONTHLY") {
                Some(43_200)
            } else {
                None
            }
        });

    let window = PlanWindow {
        used_fraction: (used_percent / 100.0).clamp(0.0, 1.0),
        resets_at,
        duration_minutes: duration,
    };
    let weekly = duration
        .map(|mins| mins >= 1_440)
        .unwrap_or(true)
        .then_some(window.clone());
    let session = duration
        .map(|mins| mins < 1_440)
        .unwrap_or(false)
        .then_some(window);
    let prepaid_usd = json_f32(
        config
            .pointer("/prepaidBalance/val")
            .or_else(|| payload.pointer("/prepaidBalance/val")),
    )
    .map(|cents| f64::from(cents) / 100.0);
    let plan = text_at(&payload, &["subscriptionTier"]);
    Some(AccountUsage {
        account_name: None,
        plan,
        status: AccountUsageStatus::Live,
        session,
        weekly,
        prepaid_usd,
        note: String::new(),
        refreshed_at: Utc::now(),
    })
}

fn parse_rfc3339_secs(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|stamp| stamp.timestamp())
}

fn json_f32(value: Option<&Value>) -> Option<f32> {
    value.and_then(|row| {
        row.as_f64()
            .map(|n| n as f32)
            .or_else(|| row.as_i64().map(|n| n as f32))
            .or_else(|| row.as_str().and_then(|text| text.parse().ok()))
    })
}

fn json_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(|row| {
        row.as_u64()
            .or_else(|| row.as_i64().and_then(|n| u64::try_from(n).ok()))
            .or_else(|| row.as_str().and_then(|text| text.parse().ok()))
    })
}

fn grok_account_label(text: &str) -> Option<String> {
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        for marker in [
            "logged in with ",
            "logged in as ",
            "signed in as ",
            "account: ",
        ] {
            if let Some(at) = lower.find(marker) {
                let value = line[at + marker.len()..]
                    .trim()
                    .trim_end_matches('.')
                    .trim();
                if !value.is_empty() {
                    return Some(if value.contains('@') || value.contains('.') {
                        value.to_owned()
                    } else {
                        format!("{value} account")
                    });
                }
            }
        }
        if line.contains('@') && !lower.contains("http") {
            let email = line
                .split_whitespace()
                .find(|word| word.contains('@') && word.contains('.'))?;
            return Some(
                email
                    .trim_matches(|ch: char| {
                        !ch.is_ascii_alphanumeric()
                            && ch != '@'
                            && ch != '.'
                            && ch != '_'
                            && ch != '+'
                            && ch != '-'
                    })
                    .to_owned(),
            );
        }
    }
    None
}

fn parse_kimi_status(stdout: &str) -> AccountUsage {
    let clean = strip_ansi(stdout);
    if let Some(account) = grok_account_label(&clean) {
        return AccountUsage {
            account_name: Some(account),
            plan: None,
            status: AccountUsageStatus::Authenticated,
            session: None,
            weekly: None,
            prepaid_usd: None,
            note: "Kimi confirmed the account, but does not expose a numerical weekly allowance through this command.".to_owned(),
            refreshed_at: Utc::now(),
        };
    }
    if clean.to_ascii_lowercase().contains("not logged")
        || clean.to_ascii_lowercase().contains("signed out")
    {
        return snapshot(AccountUsageStatus::SignedOut, "Kimi is signed out.");
    }
    snapshot(
        AccountUsageStatus::NotReported,
        "Kimi does not expose the signed-in account or a weekly allowance through its CLI.",
    )
}

async fn probe_codex(binary: &ResolvedCommand) -> AccountUsage {
    let via_server = probe_codex_app_server(binary).await;
    if !matches!(via_server.status, AccountUsageStatus::Unavailable) {
        return via_server;
    }
    let status_bin = resolve_command("codex").unwrap_or_else(|| binary.clone());
    match run_output(&status_bin, &["login", "status"]).await {
        Ok(stdout) => parse_codex_login_status(&stdout, &via_server.note),
        Err(_) => via_server,
    }
}

fn parse_codex_login_status(stdout: &str, probe_note: &str) -> AccountUsage {
    let clean = strip_ansi(stdout);
    let lower = clean.to_ascii_lowercase();
    if lower.contains("logged out") || lower.contains("not logged") {
        return snapshot(AccountUsageStatus::SignedOut, "Codex is signed out.");
    }
    if !lower.contains("logged in") {
        return unavailable(probe_note.to_owned());
    }
    let account_name = clean.lines().find_map(|line| {
        let line = line.trim();
        if line.contains('@') {
            return Some(line.to_owned());
        }
        let lower = line.to_ascii_lowercase();
        lower
            .strip_prefix("logged in using ")
            .or_else(|| lower.strip_prefix("logged in as "))
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    });
    AccountUsage {
        account_name: account_name.or_else(|| Some("ChatGPT".to_owned())),
        plan: None,
        status: AccountUsageStatus::Authenticated,
        session: None,
        weekly: None,
        prepaid_usd: None,
        note: format!("{probe_note} Signed in, but live weekly windows were not returned."),
        refreshed_at: Utc::now(),
    }
}

async fn probe_codex_app_server(binary: &ResolvedCommand) -> AccountUsage {
    let mut command = binary.command();
    command
        .arg("app-server")
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return unavailable(format!("Codex account service could not start: {error}"))
        }
    };
    let Some(mut stdin) = child.stdin.take() else {
        return unavailable("Codex account service did not open its input.".to_owned());
    };
    let Some(stdout) = child.stdout.take() else {
        return unavailable("Codex account service did not open its output.".to_owned());
    };
    let exchange = async {
        write_json_line(
            &mut stdin,
            &json!({"jsonrpc":"2.0","method":"initialize","id":1,"params":{"clientInfo":{"name":"bhippi","title":"Bhippi","version":env!("CARGO_PKG_VERSION")}}}),
        )
        .await?;
        let mut lines = BufReader::new(stdout).lines();
        let mut account = None;
        let mut limits = None;
        let mut display_name = None;
        let mut account_error = None;
        let mut limits_error = None;
        while let Some(line) = lines.next_line().await.map_err(|error| error.to_string())? {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if display_name.is_none() {
                display_name = text_at(&value, &["params", "serverName"]);
            }
            match value.get("id").and_then(json_i64) {
                Some(1) => {
                    write_json_line(&mut stdin, &json!({"jsonrpc":"2.0","method":"initialized"}))
                        .await?;
                    write_json_line(
                        &mut stdin,
                        &json!({"jsonrpc":"2.0","method":"account/read","id":2,"params":{"refreshToken":false}}),
                    )
                    .await?;
                    write_json_line(
                        &mut stdin,
                        &json!({"jsonrpc":"2.0","method":"account/rateLimits/read","id":3}),
                    )
                    .await?;
                }
                Some(2) => {
                    if value.get("error").is_some() {
                        account_error = Some(
                            value
                                .pointer("/error/message")
                                .and_then(Value::as_str)
                                .unwrap_or("account/read failed")
                                .to_owned(),
                        );
                        account = Some(Value::Null);
                    } else {
                        account = value.get("result").cloned();
                    }
                }
                Some(3) => {
                    if value.get("error").is_some() {
                        limits_error = Some(
                            value
                                .pointer("/error/message")
                                .and_then(Value::as_str)
                                .unwrap_or("account/rateLimits/read failed")
                                .to_owned(),
                        );
                        limits = Some(Value::Null);
                    } else {
                        limits = value.get("result").cloned();
                    }
                }
                _ => {}
            }
            if account.is_some() && limits.is_some() {
                break;
            }
        }
        Ok::<_, String>((account, limits, display_name, account_error, limits_error))
    };
    let result = tokio::time::timeout(PROBE_TIMEOUT, exchange).await;
    let _ = child.start_kill();
    match result {
        Ok(Ok((Some(account), Some(limits), display_name, account_error, limits_error))) => {
            parse_codex_status(&account, &limits, display_name, account_error, limits_error)
        }
        Ok(Ok((Some(account), None, display_name, account_error, _))) => parse_codex_status(
            &account,
            &Value::Null,
            display_name,
            account_error,
            Some("Codex did not return rate-limit windows.".to_owned()),
        ),
        Ok(Ok(_)) => unavailable("Codex did not return an account snapshot.".to_owned()),
        Ok(Err(reason)) => unavailable(format!("Codex account protocol failed: {reason}")),
        Err(_) => unavailable("Codex account protocol timed out.".to_owned()),
    }
}

async fn write_json_line(
    stdin: &mut tokio::process::ChildStdin,
    value: &Value,
) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    stdin
        .write_all(&bytes)
        .await
        .map_err(|error| error.to_string())?;
    stdin.flush().await.map_err(|error| error.to_string())
}

fn parse_codex_status(
    account: &Value,
    limits: &Value,
    display_name: Option<String>,
    account_error: Option<String>,
    limits_error: Option<String>,
) -> AccountUsage {
    if account.is_null() {
        let message = account_error.unwrap_or_else(|| "Codex is signed out.".to_owned());
        let lower = message.to_ascii_lowercase();
        if lower.contains("auth") || lower.contains("signed out") || lower.contains("not logged") {
            return snapshot(AccountUsageStatus::SignedOut, message);
        }
        return unavailable(message);
    }
    let account_obj = account.get("account").unwrap_or(account);
    if account_obj.is_null() {
        return snapshot(AccountUsageStatus::SignedOut, "Codex is signed out.");
    }
    let bucket = limits
        .pointer("/rateLimitsByLimitId/codex")
        .or_else(|| limits.get("rateLimits"))
        .unwrap_or(&Value::Null);
    let primary = window_at(bucket, "primary");
    let secondary = window_at(bucket, "secondary");
    let (session, weekly) = classify_windows(primary, secondary);
    let account_name = text_at(account_obj, &["email"])
        .or(display_name)
        .or_else(|| text_at(account_obj, &["type"]));
    let plan = text_at(account_obj, &["planType"]).or_else(|| text_at(bucket, &["planType"]));
    let live = session.is_some() || weekly.is_some();
    let note = if live {
        "Live from the signed-in Codex account.".to_owned()
    } else if let Some(error) = limits_error {
        format!("Codex account detected. {error}")
    } else {
        "Codex account detected. Weekly windows were not in this snapshot.".to_owned()
    };
    AccountUsage {
        account_name,
        plan,
        status: if live {
            AccountUsageStatus::Live
        } else {
            AccountUsageStatus::Authenticated
        },
        session,
        weekly,
        prepaid_usd: None,
        note,
        refreshed_at: Utc::now(),
    }
}

fn classify_windows(
    primary: Option<PlanWindow>,
    secondary: Option<PlanWindow>,
) -> (Option<PlanWindow>, Option<PlanWindow>) {
    let mut session = None;
    let mut weekly = None;
    for (index, window) in [primary, secondary].into_iter().enumerate() {
        let Some(window) = window else { continue };
        match window.duration_minutes {
            Some(minutes) if minutes >= 6 * 24 * 60 => weekly = Some(window),
            Some(minutes) if minutes <= 24 * 60 => session = Some(window),
            _ if index == 0 => session = Some(window),
            _ => weekly = Some(window),
        }
    }
    (session, weekly)
}

fn window_at(value: &Value, key: &str) -> Option<PlanWindow> {
    let window = value.get(key)?.as_object()?;
    let used = window.get("usedPercent").and_then(json_f64)?;
    Some(PlanWindow {
        #[allow(clippy::cast_possible_truncation)]
        used_fraction: ((used as f32) / 100.0).clamp(0.0, 1.0),
        resets_at: window.get("resetsAt").and_then(json_i64),
        duration_minutes: window
            .get("windowDurationMins")
            .and_then(json_f64)
            .map(|mins| mins.max(0.0) as u64),
    })
}

fn json_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_u64().map(|number| number as f64))
        .or_else(|| value.as_i64().map(|number| number as f64))
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
}

fn json_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
        .or_else(|| value.as_f64().map(|number| number as i64))
}

fn text_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    current
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .map(str::to_owned)
}

fn strip_ansi(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            let _ = chars.next();
            for code in chars.by_ref() {
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn snapshot(status: AccountUsageStatus, note: impl Into<String>) -> AccountUsage {
    AccountUsage {
        account_name: None,
        plan: None,
        status,
        session: None,
        weekly: None,
        prepaid_usd: None,
        note: note.into(),
        refreshed_at: Utc::now(),
    }
}

fn unavailable(note: impl Into<String>) -> AccountUsage {
    snapshot(AccountUsageStatus::Unavailable, note)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_status_carries_identity_without_inventing_a_limit() {
        let row = parse_claude_status(
            r#"{"loggedIn":true,"email":"owner@example.com","subscriptionType":"pro"}"#,
        );
        assert_eq!(row.account_name.as_deref(), Some("owner@example.com"));
        assert_eq!(row.plan.as_deref(), Some("pro"));
        assert_eq!(row.weekly, None);
        assert_eq!(row.status, AccountUsageStatus::Authenticated);
    }

    #[test]
    fn claude_usage_text_fills_session_and_weekly_without_a_model_turn() {
        let identity = parse_claude_status(
            r#"{"loggedIn":true,"email":"owner@example.com","subscriptionType":"pro"}"#,
        );
        let stdout = r#"{"type":"result","num_turns":0,"total_cost_usd":0,"result":"You are currently using your subscription to power your Claude Code usage\n\nCurrent session: 0% used\nCurrent week (all models): 100% used · resets Sep 1, 2:29am (Asia/Kolkata)\n"}"#;
        let row = merge_claude_usage(identity, stdout);
        assert_eq!(row.status, AccountUsageStatus::Live);
        assert_eq!(
            row.session.as_ref().map(|window| window.used_fraction),
            Some(0.0)
        );
        assert_eq!(
            row.weekly.as_ref().map(|window| window.used_fraction),
            Some(1.0)
        );
        assert!(row
            .weekly
            .as_ref()
            .and_then(|window| window.resets_at)
            .is_some());
        assert_eq!(row.account_name.as_deref(), Some("owner@example.com"));
    }

    #[test]
    fn claude_usage_ignores_scoped_sonnet_week_and_local_stats() {
        let text =
            "Current week (Sonnet): 45% used\nLast 7d · 70% of your usage was at >150k context\n";
        let (session, weekly) = parse_usage_windows(text);
        assert_eq!(session, None);
        assert_eq!(weekly, None);
    }

    #[test]
    fn codex_uses_window_duration_instead_of_assuming_field_order() {
        let account =
            json!({"account":{"type":"chatgpt","email":"owner@example.com","planType":"plus"}});
        let limits = json!({"rateLimits":{"primary":{"usedPercent":4,"windowDurationMins":10080,"resetsAt":20},"secondary":{"usedPercent":25,"windowDurationMins":300,"resetsAt":10}}});
        let row = parse_codex_status(&account, &limits, None, None, None);
        assert_eq!(
            row.weekly.as_ref().map(|window| window.used_fraction),
            Some(0.04)
        );
        assert_eq!(
            row.session.as_ref().map(|window| window.used_fraction),
            Some(0.25)
        );
        assert_eq!(
            row.weekly.as_ref().and_then(|window| window.resets_at),
            Some(20)
        );
        assert_eq!(row.account_name.as_deref(), Some("owner@example.com"));
        assert_eq!(row.plan.as_deref(), Some("plus"));
    }

    #[test]
    fn codex_accepts_float_used_percent() {
        let account = json!({"account":{"email":"owner@example.com","planType":"plus"}});
        let limits = json!({"rateLimits":{"primary":{"usedPercent":16.4,"windowDurationMins":300},"secondary":{"usedPercent":"99","windowDurationMins":10080}}});
        let row = parse_codex_status(&account, &limits, None, None, None);
        assert_eq!(
            row.session
                .as_ref()
                .map(|window| (window.used_fraction * 1000.0).round() as i32),
            Some(164)
        );
        assert_eq!(
            row.weekly.as_ref().map(|window| window.used_fraction),
            Some(0.99)
        );
    }

    #[test]
    fn opencode_reports_backend_names_not_credential_paths() {
        let row = parse_opencode_status(
            "┌  Credentials ~\\.local\\share\\opencode\\auth.json\n│\n●  OpenRouter api\n│\n└  1 credentials\n",
        );
        assert_eq!(row.account_name.as_deref(), Some("OpenRouter"));
        assert!(!row.account_name.unwrap_or_default().contains("auth.json"));
        assert_eq!(row.weekly, None);
        assert_eq!(row.plan.as_deref(), Some("api"));
    }

    #[test]
    fn antigravity_models_list_means_signed_in() {
        let row = parse_antigravity_status(
            "Fetching available models...\ngemini-3.8-flash-high\tGemini 3.8 Flash (High)\n",
        );
        assert_eq!(row.account_name.as_deref(), Some("Google account"));
        assert_eq!(row.status, AccountUsageStatus::Authenticated);
        assert_eq!(row.weekly, None);
    }

    #[test]
    fn antigravity_auth_required_is_signed_out() {
        let row = parse_antigravity_status("authentication required\n");
        assert_eq!(row.status, AccountUsageStatus::SignedOut);
    }

    #[test]
    fn grok_names_only_the_scope_the_cli_reveals() {
        let row = parse_grok_status("You are logged in with grok.com.\nAvailable models:\n");
        assert_eq!(row.account_name.as_deref(), Some("grok.com"));
        assert_eq!(row.status, AccountUsageStatus::Authenticated);
        assert_eq!(row.weekly, None);
    }

    #[test]
    fn grok_prefers_an_email_when_the_cli_prints_one() {
        let row = parse_grok_status("Logged in as owner@example.com\nAvailable models:\n");
        assert_eq!(row.account_name.as_deref(), Some("owner@example.com"));
    }

    #[test]
    fn grok_weekly_left_line_becomes_used_fraction() {
        let identity = parse_grok_status("You are logged in with grok.com.\n");
        let row = merge_grok_usage(identity, "Weekly limit left 12%\nResets in 2d 4h\n");
        assert_eq!(row.status, AccountUsageStatus::Live);
        assert_eq!(
            row.weekly
                .as_ref()
                .map(|window| (window.used_fraction * 100.0).round() as i32),
            Some(88)
        );
    }

    #[test]
    fn grok_weekly_used_line_is_not_inverted() {
        let identity = parse_grok_status("You are logged in with grok.com.\n");
        let row = merge_grok_usage(identity, "Usage this week: 3% used. Resets in 4d.\n");
        assert_eq!(
            row.weekly.as_ref().map(|window| window.used_fraction),
            Some(0.03)
        );
    }

    #[test]
    fn grok_credits_config_fills_weekly_prepaid_and_plan() {
        let identity = parse_grok_status("You are logged in with grok.com.\n");
        let body = json!({
            "config": {
                "creditUsagePercent": 42.5,
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-06-01T00:00:00Z",
                    "end": "2026-06-08T00:00:00Z"
                },
                "prepaidBalance": { "val": 1250 }
            },
            "subscriptionTier": "SuperGrok"
        });
        let row = merge_grok_usage(identity, &body.to_string());
        assert_eq!(row.status, AccountUsageStatus::Live);
        assert_eq!(row.plan.as_deref(), Some("SuperGrok"));
        assert_eq!(row.prepaid_usd, Some(12.5));
        assert_eq!(
            row.weekly
                .as_ref()
                .map(|window| (window.used_fraction * 1000.0).round() as i32),
            Some(425)
        );
        assert_eq!(
            row.weekly.as_ref().and_then(|window| window.resets_at),
            chrono::DateTime::parse_from_rfc3339("2026-06-08T00:00:00Z")
                .ok()
                .map(|stamp| stamp.timestamp())
        );
    }

    #[test]
    fn grok_credits_config_treats_omitted_percent_as_zero() {
        let parsed = parse_grok_usage_json(
            r#"{"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","end":"2026-06-08T00:00:00Z"}}}"#,
        )
        .expect("a credits document with no percent is still live");
        assert_eq!(
            parsed.weekly.as_ref().map(|window| window.used_fraction),
            Some(0.0)
        );
    }

    #[test]
    fn calendar_reset_parses_claude_usage_wording() {
        let stamp = parse_reset_stamp("Sep 1, 2:29am (Asia/Kolkata)");
        assert!(stamp.is_some(), "expected a timestamp, got {stamp:?}");
    }
}
