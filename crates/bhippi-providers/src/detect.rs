//! Detection over the static catalogue (spec §8.1): CLI presence + version, credential
//! presence for clouds, and loopback probes for local servers — all within the probe
//! budget, never blocking app start.

use crate::catalog::ProviderSpec;
use crate::command::{resolve_command, ResolvedCommand};
use crate::model::{ProviderInfo, ProviderKind};
use bhippi_types::Health;
use chrono::Utc;
use std::collections::HashMap;
use std::time::Duration;

/// Per-probe budget from spec §8.1d. Local LLM servers (LM Studio, Ollama, etc.) can
/// take 1–3 s to respond when loading a model, so the budget is generous by design.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// CLI `--version` budget; CLIs start slower than a TCP probe answers.
pub const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// CLI model-catalogue budget. Printing a catalogue is cheap once the CLI is warm, but a
/// cold Node launcher plus a several-hundred-model list is not, and an empty picker is a
/// worse outcome than a slightly slower scan — detection never blocks app start anyway.
pub const MODEL_LIST_TIMEOUT: Duration = Duration::from_secs(20);

/// Every catalogued backend, probed within budget. `enabled` carries the user's toggle
/// prefs and is copied onto the rows verbatim — detection never flips a toggle.
///
/// This is the **startup / Settings / toggle / install** path. It spawns CLI `--version`
/// and `models` for each installed agent, so it must never run on a timer — those
/// processes hold the same binaries a chat turn needs (INV-062).
pub async fn detect(catalogue: &[ProviderSpec], enabled: &[String]) -> Vec<ProviderInfo> {
    let mut probes = Vec::with_capacity(catalogue.len());
    for entry in catalogue {
        probes.push(detect_one(entry, enabled));
    }
    let mut rows = futures_util::future::join_all(probes).await;
    rows.push(demo_row());
    rows
}

/// Loopback probes for local servers only. No CLI is spawned.
///
/// The desktop runtime uses this on the 10 s “did Ollama come up?” tick so chat can
/// still launch `claude` / `grok` / `codex` while we watch ports.
pub async fn detect_local_servers(
    catalogue: &[ProviderSpec],
    enabled: &[String],
) -> Vec<ProviderInfo> {
    let mut probes = Vec::new();
    for entry in catalogue {
        if entry.kind == ProviderKind::LocalServer {
            probes.push(detect_one(entry, enabled));
        }
    }
    futures_util::future::join_all(probes).await
}

/// Overlay freshly probed local-server rows onto a previous full detection, keeping CLI,
/// cloud, and demo rows (and their model catalogues) untouched.
#[must_use]
pub fn merge_detection(
    previous: &[ProviderInfo],
    local_servers: &[ProviderInfo],
) -> Vec<ProviderInfo> {
    let mut by_id: HashMap<String, ProviderInfo> = previous
        .iter()
        .cloned()
        .map(|row| (row.id.clone(), row))
        .collect();
    for row in local_servers {
        by_id.insert(row.id.clone(), row.clone());
    }
    let mut out = Vec::with_capacity(by_id.len().max(1));
    for spec in crate::catalog::CATALOG {
        if let Some(row) = by_id.remove(spec.id) {
            out.push(row);
        }
    }
    if let Some(demo) = by_id.remove("demo") {
        out.push(demo);
    } else {
        out.push(demo_row());
    }
    out.extend(by_id.into_values());
    out
}

/// Copies the user's toggle list onto detection rows. Demo is always on.
pub fn stamp_enabled(rows: &mut [ProviderInfo], enabled: &[String]) {
    for row in rows {
        row.enabled = row.id == "demo" || enabled.iter().any(|id| id == &row.id);
    }
}

/// Equality that ignores timestamps and probe-latency jitter so an unchanged machine
/// does not rebuild the runtime (and does not emit `providers-changed`) every tick.
#[must_use]
pub fn detection_fingerprint(rows: &[ProviderInfo]) -> Vec<String> {
    let mut keys: Vec<String> = rows
        .iter()
        .map(|row| {
            let health = match &row.health {
                Health::Healthy { .. } => "healthy".to_owned(),
                Health::Degraded { reason } => format!("degraded:{reason}"),
                Health::Unavailable { .. } if row.kind == ProviderKind::LocalServer => {
                    "unavailable".to_owned()
                }
                Health::Unavailable { reason } => format!("unavailable:{reason}"),
                Health::Disabled => "disabled".to_owned(),
            };
            format!(
                "{}|{}|{}|{}|{}|{}|{}|{}",
                row.id,
                row.installed,
                row.enabled,
                health,
                row.detected_port
                    .map(|port| port.to_string())
                    .unwrap_or_default(),
                row.models.join(","),
                row.version.clone().unwrap_or_default(),
                row.offered
            )
        })
        .collect();
    keys.sort();
    keys
}

async fn detect_one(entry: &crate::catalog::ProviderSpec, enabled: &[String]) -> ProviderInfo {
    let is_enabled = enabled.iter().any(|id| id == entry.id);
    match entry.kind {
        ProviderKind::Cli => cli_row(entry, is_enabled).await,
        ProviderKind::LocalServer => server_row(entry, is_enabled).await,
        ProviderKind::CloudApi => cloud_row(entry, is_enabled),
        // The demo row is appended once by [`detect`], not per-catalogue entry.
        ProviderKind::Demo => demo_row(),
    }
}

async fn cli_row(entry: &crate::catalog::ProviderSpec, enabled: bool) -> ProviderInfo {
    let binary = entry.binary.unwrap_or_default();
    let found = resolve_command(binary);
    let version = match &found {
        Some(path) => read_version(path).await,
        None => None,
    };
    // Most CLIs will print the catalogue they accept, and the vendor's own answer beats
    // any list we could keep by hand. The catalogue's static names are the fallback for
    // the CLIs that will not (Claude Code documents aliases; Kimi documents nothing), and
    // an empty list means the picker offers its free-text field instead.
    let listed = match (&found, entry.list_models_args) {
        (Some(path), Some(args)) => read_models(path, args).await,
        _ => Vec::new(),
    };
    let models = if listed.is_empty() {
        entry.models.iter().map(|name| (*name).to_owned()).collect()
    } else {
        listed
    };
    let installed = found.is_some();
    ProviderInfo {
        id: entry.id.to_owned(),
        label: entry.label.to_owned(),
        kind: entry.kind,
        models,
        health: if installed {
            Health::Healthy { latency_ms: 0 }
        } else {
            Health::Unavailable {
                reason: "not on PATH".to_owned(),
            }
        },
        offered: false,
        detected_at: Utc::now(),
        installed,
        version,
        enabled,
        accepts_custom_model: entry.model_args.is_some(),
        detected_port: None,
    }
}

/// One local server answering on one port.
struct Reachable {
    port: u16,
    latency_ms: u32,
    models: Vec<String>,
}

/// The ports a given server is worth looking for, primary first.
fn candidate_ports(entry: &crate::catalog::ProviderSpec) -> Vec<u16> {
    // Each list is the vendor's default plus the ports that vendor is actually seen on.
    // Kept short deliberately: every extra entry is a socket opened on every sweep, and a
    // server on a genuinely custom port is found through Settings, not by guessing wider.
    let fallback: &[u16] = match entry.id {
        "bionic" => &[7432, 1234, 11434, 8080, 3000],
        "lmstudio" => &[1234, 53166, 11434],
        "llamacpp" => &[8080, 8000],
        "ollama" => &[11434, 11435],
        "vllm" => &[8000, 8080],
        "jan" => &[1337, 1234],
        "tgui" => &[5000, 7860],
        _ => &[1234, 8080, 11434],
    };
    let mut ports = Vec::with_capacity(fallback.len() + 1);
    if let Some(primary) = entry.port.filter(|port| *port > 0) {
        ports.push(primary);
    }
    for port in fallback {
        if !ports.contains(port) {
            ports.push(*port);
        }
    }
    ports
}

/// Asks one port whether a model server is behind it.
async fn probe_port(port: u16, primary_path: &str) -> Result<Reachable, String> {
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();
    let started = std::time::Instant::now();

    let mut paths = vec![primary_path];
    for extra in ["/v1/models", "/api/tags"] {
        if primary_path != extra {
            paths.push(extra);
        }
    }

    let mut last = format!("port {port}: no answer");
    for (index, probe_path) in paths.iter().enumerate() {
        let response = client
            .get(format!("{base}{probe_path}"))
            .header("Authorization", "Bearer local")
            .timeout(PROBE_TIMEOUT)
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => {
                let latency_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
                let value: serde_json::Value =
                    response.json().await.unwrap_or(serde_json::Value::Null);
                return Ok(Reachable {
                    port,
                    latency_ms,
                    models: extract_model_names(&value),
                });
            }
            Ok(response) => {
                last = format!("port {port}: answered HTTP {}", response.status().as_u16());
            }
            Err(error) => {
                last = format!("port {port}: {error}");
                // Nothing is listening at all, so the other paths on this port cannot
                // answer either — trying them is three timeouts to learn one fact.
                if index == 0 && (error.is_connect() || error.is_timeout()) {
                    break;
                }
            }
        }
    }
    Err(last)
}

/// Probes one local server's candidate ports **concurrently**.
///
/// Serially, this was the slowest thing in the app. Bionic alone has a dozen candidate
/// ports, and a refused connection on Windows is not always instant; at a 2 s budget each
/// that is twenty seconds of app start spent discovering that nothing is running. Every
/// candidate is a loopback request with no shared state, so there is no reason to wait for
/// one before starting the next. First success wins; the whole sweep now costs about one
/// timeout rather than one per port.
async fn server_row(entry: &crate::catalog::ProviderSpec, enabled: bool) -> ProviderInfo {
    let ports_to_try = candidate_ports(entry);
    let primary_path = entry.probe_path.unwrap_or("/v1/models");

    let probes = ports_to_try
        .iter()
        .map(|port| probe_port(*port, primary_path));
    let results = futures_util::future::join_all(probes).await;

    let mut last_err_reason = String::new();
    for result in results {
        match result {
            Ok(found) => {
                tracing::info!(
                    provider = %entry.id,
                    port = found.port,
                    latency_ms = found.latency_ms,
                    models = found.models.len(),
                    "local server detected with in-memory models"
                );
                return ProviderInfo {
                    id: entry.id.to_owned(),
                    label: entry.label.to_owned(),
                    kind: entry.kind,
                    health: Health::Healthy {
                        latency_ms: found.latency_ms,
                    },
                    models: found.models,
                    offered: false,
                    detected_at: Utc::now(),
                    installed: true,
                    version: None,
                    enabled,
                    accepts_custom_model: true,
                    detected_port: Some(found.port),
                };
            }
            Err(reason) => {
                if last_err_reason.is_empty() {
                    last_err_reason = reason;
                }
            }
        }
    }

    // Nothing answered on any port. The binary may still be on disk — and this is
    // exactly where detection used to run `<binary> --version` to say so.
    //
    // That probe is why opening Bhippi opened Bionic. A local LLM server is a *desktop
    // application*: it does not implement `--version`, so executing it with that flag
    // does not print a version, it launches the program. Detection ran on every sweep,
    // so every launch of Bhippi launched Bionic's full UI and loaded a model into RAM
    // that nobody had asked for.
    //
    // The rule now: **a server is detected by listening, never by executing.** Presence
    // on disk means "installable and launchable", which is `offered`, not `installed`,
    // and never `Healthy` — a server that is not accepting connections cannot answer a
    // prompt, and reporting it healthy is what made it win the default pick.
    let on_disk = entry
        .binary
        .and_then(resolve_command)
        .is_some_and(|command| command.target_exists());
    if on_disk {
        return ProviderInfo {
            id: entry.id.to_owned(),
            label: entry.label.to_owned(),
            kind: entry.kind,
            health: Health::Unavailable {
                reason: "installed, but not running — start it to use it".to_owned(),
            },
            models: Vec::new(),
            // `offered` is the honest word for this: found, not reachable. Settings can
            // show it with a "start it" hint; the chat picker will not offer it.
            offered: true,
            detected_at: Utc::now(),
            installed: false,
            version: None,
            enabled,
            accepts_custom_model: true,
            detected_port: None,
        };
    }

    tracing::debug!(
        provider = %entry.id,
        reason = %last_err_reason,
        "local server not detected on any port"
    );
    ProviderInfo {
        id: entry.id.to_owned(),
        label: entry.label.to_owned(),
        kind: entry.kind,
        health: Health::Unavailable {
            reason: last_err_reason,
        },
        models: Vec::new(),
        offered: false,
        detected_at: Utc::now(),
        installed: false,
        version: None,
        enabled,
        accepts_custom_model: false,
        detected_port: None,
    }
}

fn cloud_row(entry: &crate::catalog::ProviderSpec, enabled: bool) -> ProviderInfo {
    let present = entry
        .env_key
        .is_some_and(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()));
    ProviderInfo {
        id: entry.id.to_owned(),
        label: entry.label.to_owned(),
        kind: entry.kind,
        models: entry.models.iter().map(|name| (*name).to_owned()).collect(),
        health: if present {
            Health::Degraded {
                reason: "credential present; API adapter lands in S1".to_owned(),
            }
        } else {
            Health::Disabled
        },
        offered: true,
        detected_at: Utc::now(),
        installed: present,
        version: None,
        enabled,
        // Cloud adapters land in S1; until then nothing here promises a model field.
        accepts_custom_model: false,
        detected_port: None,
    }
}

fn demo_row() -> ProviderInfo {
    ProviderInfo {
        id: "demo".to_owned(),
        label: "Demo (offline)".to_owned(),
        kind: ProviderKind::Demo,
        models: vec!["scripted-v1".to_owned()],
        health: Health::Healthy { latency_ms: 0 },
        offered: false,
        detected_at: Utc::now(),
        installed: true,
        version: None,
        enabled: true,
        accepts_custom_model: false,
        detected_port: None,
    }
}

/// Model-name extraction shared by Ollama (`models[].name`), the OpenAI-compatible shape
/// (`data[].id`), and Codex's printed catalogue (`models[].slug`). Pure so tests drive it
/// without a server.
///
/// A vendor that marks an entry hidden means it: those are internal or reserved models
/// that would fail if a user picked them out of our list.
#[must_use]
pub fn extract_model_names(value: &serde_json::Value) -> Vec<String> {
    let array = value
        .get("models")
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.get("data").and_then(serde_json::Value::as_array))
        .or_else(|| value.get("tags").and_then(serde_json::Value::as_array))
        .or_else(|| value.as_array());
    match array {
        Some(items) => {
            let list: Vec<String> = items
                .iter()
                .filter(|item| {
                    item.get("visibility").and_then(serde_json::Value::as_str) != Some("hide")
                })
                .filter_map(|item| {
                    if let Some(s) = item.as_str() {
                        return Some(s.to_owned());
                    }
                    item.get("name")
                        .or_else(|| item.get("id"))
                        .or_else(|| item.get("slug"))
                        .or_else(|| item.get("model"))
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .collect();
            dedup(list)
        }
        None => {
            if let Some(m) = value
                .get("model")
                .or_else(|| value.get("active_model"))
                .or_else(|| value.get("loaded_model"))
                .or_else(|| value.get("id"))
                .and_then(serde_json::Value::as_str)
            {
                vec![m.to_owned()]
            } else {
                Vec::new()
            }
        }
    }
}

/// Reads whatever a CLI printed when asked for its models.
///
/// Vendors print one of two things and we do not want a format flag per vendor: a JSON
/// catalogue (Codex) or a text list (`opencode models` prints bare ids, `grok models`
/// prints a bulleted list under prose headings). JSON is tried first because a JSON
/// document read as text yields nothing useful, while the reverse is simply a failed parse.
#[must_use]
pub fn parse_model_list(stdout: &str) -> Vec<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
        let names = extract_model_names(&value);
        if !names.is_empty() {
            return dedup(names);
        }
    }
    parse_model_lines(stdout)
}

/// Reads a printed model list. Bullets win when the output has any, because a bulleted
/// list is always surrounded by prose the bare-line reader would have to guess about;
/// bare single-token lines are read only when there are no bullets at all.
#[must_use]
pub fn parse_model_lines(text: &str) -> Vec<String> {
    let bulleted: Vec<String> = text.lines().filter_map(bulleted_id).collect();
    if !bulleted.is_empty() {
        return dedup(bulleted);
    }
    dedup(text.lines().filter_map(bare_id).collect())
}

/// `  * grok-4.6 (default)` → `grok-4.6`.
fn bulleted_id(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(['*', '-', '•'])?;
    model_id(rest.split_whitespace().next()?)
}

/// A line that is nothing but an id, as `opencode models` prints them.
fn bare_id(line: &str) -> Option<String> {
    let line = line.trim();
    let mut tokens = line.split_whitespace();
    let first = tokens.next()?;
    if tokens.next().is_some() {
        return None;
    }
    model_id(first)
}

/// Guards the reader against prose, rules, and decoration that survived the line filters.
fn model_id(token: &str) -> Option<String> {
    let plausible = !token.is_empty()
        && token.len() <= 120
        && token
            .chars()
            .all(|glyph| glyph.is_ascii_alphanumeric() || "._:/~@+-".contains(glyph))
        && token.chars().any(|glyph| glyph.is_ascii_alphanumeric());
    plausible.then(|| token.to_owned())
}

/// Vendors repeat their default in a summary line above the list.
fn dedup(names: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

/// Asks one CLI for the models it accepts. A failure here is not a provider failure —
/// the row keeps its catalogue fallback and the picker keeps its free-text field.
async fn read_models(binary: &ResolvedCommand, args: &[&str]) -> Vec<String> {
    let mut command = binary.command();
    command.args(args);
    let output = match tokio::time::timeout(MODEL_LIST_TIMEOUT, command.output()).await {
        Ok(Ok(output)) if output.status.success() => output,
        Ok(Ok(output)) => {
            tracing::debug!(status = %output.status, "model list command failed");
            return Vec::new();
        }
        Ok(Err(error)) => {
            tracing::debug!(%error, "model list command could not run");
            return Vec::new();
        }
        Err(_) => {
            tracing::debug!("model list command timed out");
            return Vec::new();
        }
    };
    parse_model_list(&String::from_utf8_lossy(&output.stdout))
}

async fn read_version(binary: &ResolvedCommand) -> Option<String> {
    let mut command = binary.command();
    command
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    let child = command.output();
    let output = tokio::time::timeout(VERSION_TIMEOUT, child)
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stdout
        .lines()
        .chain(stderr.lines())
        .map(|line| line.trim().to_owned())
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(80).collect())
}

#[cfg(test)]
mod tests {
    use super::{demo_row, extract_model_names, parse_model_lines, parse_model_list, server_row};
    use crate::model::{ProviderInfo, ProviderKind};
    use serde_json::json;

    /// Regression pin for the bug that made opening Bhippi open Bionic.
    ///
    /// Detection used to fall back to running `<binary> --version` when no port answered.
    /// A local LLM server is a desktop application: it does not implement `--version`, so
    /// that call does not read a version, it **launches the program** — every sweep, on
    /// every app start, loading a model into RAM nobody asked for.
    ///
    /// A local server must be detected by listening and by nothing else. This probes a
    /// port that is guaranteed to have nothing on it; the row must come back unreachable
    /// rather than healthy, and no process may have been started to decide that.
    #[tokio::test]
    async fn a_local_server_that_is_not_listening_is_never_reported_healthy() {
        let Some(spec) = crate::spec("bionic") else {
            panic!("the catalogue must know Bionic");
        };
        let row = server_row(spec, true).await;

        assert_eq!(row.kind, ProviderKind::LocalServer);
        assert!(
            !row.usable(),
            "a server nothing answered for must not be usable: {:?}",
            row.health
        );
        assert!(
            row.detected_port.is_none() || row.usable(),
            "a port may only be recorded when a probe actually answered"
        );
        // And it must never be the thing the picker defaults to.
        assert!(
            !matches!(row.health, bhippi_types::Health::Healthy { .. }) || row.usable(),
            "health must follow reachability, not presence on disk"
        );
    }

    /// "On disk" and "answering" are different facts and need different words: the fix
    /// for one is to install it and for the other to start it.
    #[test]
    fn presence_and_readiness_are_reported_as_different_things() {
        use crate::model::ProviderInfo;
        use chrono::Utc;

        let idle = ProviderInfo {
            id: "bionic".to_owned(),
            label: "Bionic".to_owned(),
            kind: ProviderKind::LocalServer,
            models: Vec::new(),
            health: bhippi_types::Health::Unavailable {
                reason: "installed, but not running — start it to use it".to_owned(),
            },
            offered: true,
            detected_at: Utc::now(),
            installed: false,
            version: None,
            enabled: true,
            accepts_custom_model: true,
            detected_port: None,
        };
        assert!(!idle.usable());
        assert!(idle.installed_but_idle());

        let running = ProviderInfo {
            health: bhippi_types::Health::Healthy { latency_ms: 4 },
            detected_port: Some(11434),
            offered: false,
            ..idle.clone()
        };
        assert!(running.usable());
        assert!(!running.installed_but_idle());

        // A CLI is usable on presence alone — it starts per turn and holds no memory.
        let cli = ProviderInfo {
            kind: ProviderKind::Cli,
            installed: true,
            detected_port: None,
            health: bhippi_types::Health::Healthy { latency_ms: 0 },
            ..idle
        };
        assert!(cli.usable());
        assert!(!cli.installed_but_idle());
    }

    #[test]
    fn extracts_ollama_and_openai_style_model_lists() {
        let ollama = json!({ "models": [ { "name": "qwen2.5:7b" }, { "name": "llama3.1" } ] });
        assert_eq!(
            extract_model_names(&ollama),
            vec!["qwen2.5:7b".to_owned(), "llama3.1".to_owned()]
        );

        let openai = json!({ "data": [ { "id": "gpt-4o-mini" } ] });
        assert_eq!(extract_model_names(&openai), vec!["gpt-4o-mini".to_owned()]);

        assert!(extract_model_names(&json!({ "nothing": 1 })).is_empty());
    }

    /// Codex prints `models[].slug` and marks its internal entries hidden. Offering a
    /// hidden slug in the picker hands the user a model the vendor will refuse.
    #[test]
    fn codex_catalogue_yields_visible_slugs_only() {
        let catalogue = json!({ "models": [
            { "slug": "gpt-5.6-sol", "visibility": "list" },
            { "slug": "gpt-reserve", "visibility": "hide" },
            { "slug": "gpt-5.4-mini", "visibility": "list" },
        ] });
        assert_eq!(
            parse_model_list(&catalogue.to_string()),
            vec!["gpt-5.6-sol".to_owned(), "gpt-5.4-mini".to_owned()]
        );
    }

    /// `opencode models` prints one bare id per line, several hundred of them.
    #[test]
    fn bare_line_lists_are_read_whole() {
        let printed =
            "opencode/big-pickle\nopenrouter/anthropic/claude-opus-4.5\nzai-coding-plan/glm-4.6\n";
        assert_eq!(
            parse_model_lines(printed),
            vec![
                "opencode/big-pickle".to_owned(),
                "openrouter/anthropic/claude-opus-4.5".to_owned(),
                "zai-coding-plan/glm-4.6".to_owned(),
            ]
        );
    }

    /// `grok models` prints a bulleted list under prose. The prose must not become models,
    /// and the default marker must not become part of the id.
    #[test]
    fn bulleted_lists_drop_the_prose_around_them() {
        let printed = "You are logged in with grok.com.\n\nDefault model: grok-4.6\n\nAvailable models:\n  * grok-4.6 (default)\n  - grok-4.5\n";
        assert_eq!(
            parse_model_lines(printed),
            vec!["grok-4.6".to_owned(), "grok-4.5".to_owned()]
        );
    }

    #[test]
    fn decoration_and_prose_never_become_model_ids() {
        for noise in [
            "",
            "Available models:\n",
            "----------------\n",
            "  *  \n",
            "You are not logged in. Run grok login.\n",
        ] {
            assert!(
                parse_model_lines(noise).is_empty(),
                "read models out of {noise:?}"
            );
        }
    }

    #[test]
    fn a_repeated_default_is_listed_once() {
        assert_eq!(
            parse_model_lines("- grok-4.6\n- grok-4.5\n- grok-4.6\n"),
            vec!["grok-4.6".to_owned(), "grok-4.5".to_owned()]
        );
    }

    #[test]
    fn demo_row_reports_installed_and_enabled() {
        let row = demo_row();
        assert!(row.installed && row.enabled);
        assert!(!row.accepts_custom_model, "the demo takes no model choice");
    }

    /// The 10 s picker tick must never look like a full detect: no CLI rows, no demo.
    /// Spawning `grok models` / `claude --version` on that path is what blocked chat.
    #[tokio::test]
    async fn local_server_scope_never_returns_cli_or_demo_rows() {
        let rows = super::detect_local_servers(crate::CATALOG, &[]).await;
        assert!(!rows.is_empty(), "the catalogue has local servers to probe");
        for row in &rows {
            assert_eq!(
                row.kind,
                ProviderKind::LocalServer,
                "{} leaked onto the local-server-only path",
                row.id
            );
            assert_ne!(row.id, "claude");
            assert_ne!(row.id, "grok");
            assert_ne!(row.id, "codex");
            assert_ne!(row.id, "demo");
        }
        assert!(rows.iter().any(|row| row.id == "ollama"));
    }

    #[test]
    fn fingerprint_ignores_timestamps_and_probe_latency() {
        let mut a = demo_row();
        let mut b = a.clone();
        b.detected_at = a.detected_at + chrono::Duration::seconds(12);
        a.health = bhippi_types::Health::Healthy { latency_ms: 0 };
        b.health = bhippi_types::Health::Healthy { latency_ms: 17 };
        assert_eq!(
            super::detection_fingerprint(std::slice::from_ref(&a)),
            super::detection_fingerprint(std::slice::from_ref(&b))
        );
        b.enabled = false;
        assert_ne!(
            super::detection_fingerprint(std::slice::from_ref(&a)),
            super::detection_fingerprint(std::slice::from_ref(&b))
        );
    }

    #[test]
    fn merge_keeps_cli_model_lists_when_a_local_server_flips() {
        let grok = ProviderInfo {
            id: "grok".to_owned(),
            label: "Grok CLI".to_owned(),
            kind: ProviderKind::Cli,
            models: vec!["grok-4.6".to_owned()],
            health: bhippi_types::Health::Healthy { latency_ms: 0 },
            offered: false,
            detected_at: chrono::Utc::now(),
            installed: true,
            version: Some("1.0".to_owned()),
            enabled: true,
            accepts_custom_model: true,
            detected_port: None,
        };
        let ollama_down = ProviderInfo {
            id: "ollama".to_owned(),
            label: "Ollama".to_owned(),
            kind: ProviderKind::LocalServer,
            models: Vec::new(),
            health: bhippi_types::Health::Unavailable {
                reason: "not running".to_owned(),
            },
            offered: true,
            detected_at: chrono::Utc::now(),
            installed: false,
            version: None,
            enabled: false,
            accepts_custom_model: true,
            detected_port: None,
        };
        let ollama_up = ProviderInfo {
            health: bhippi_types::Health::Healthy { latency_ms: 4 },
            installed: true,
            offered: false,
            enabled: true,
            detected_port: Some(11434),
            models: vec!["qwen2.5:7b".to_owned()],
            ..ollama_down.clone()
        };
        let merged = super::merge_detection(&[grok.clone(), ollama_down], &[ollama_up]);
        assert!(
            merged
                .iter()
                .any(|row| row.id == "grok" && row.models == ["grok-4.6".to_owned()]),
            "CLI model list must survive a local-server merge"
        );
        assert!(
            merged
                .iter()
                .any(|row| row.id == "ollama" && row.detected_port == Some(11434)),
            "reachable Ollama must replace the idle row"
        );
        assert!(merged.iter().any(|row| row.id == "demo"));
    }
}
