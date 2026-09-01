//! Session replay dumps: `~/.bhippi/replay/<session_id>/`.
//!
//! Every provider call (`05-PIPELINES.md` P9) appends its prompt, input, and output here so
//! a quality regression can be reconstructed later. Appends are incremental and crash-safe:
//! the manifest is rewritten atomically after each record, and reopening a dump continues
//! the sequence rather than starting a second one.

use crate::SecretRedactor;
use bhippi_types::{BhippiError, ProviderId, Result, SessionId, TaskClass, Timestamp};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// The on-disk schema tag. Bump it when the layout below changes.
const SCHEMA: &str = "bhippi.replay/1";

/// A prompt file as it was sent, with the version and hash pinned into the post record.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReplayPrompt {
    pub id: String,
    pub version: u32,
    pub hash: String,
    pub path: String,
    pub content: String,
}

/// One provider call: what went in, what came back.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReplayExchange {
    pub task_class: TaskClass,
    pub provider: ProviderId,
    pub input: Value,
    pub output: Value,
    pub started_at: Timestamp,
    pub finished_at: Timestamp,
}

/// An exchange as it is indexed on disk, with the sequence the dump assigned to it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReplayExchangeRecord {
    pub sequence: u32,
    #[serde(flatten)]
    pub exchange: ReplayExchange,
}

/// A prompt index row. The body lives beside it in `prompts/`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReplayPromptRecord {
    pub id: String,
    pub version: u32,
    pub hash: String,
    pub path: String,
    pub file: String,
}

/// A whole session in one value, for the fatal-error path that dumps and gives up.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReplayBundle {
    pub session_id: SessionId,
    pub prompts: Vec<ReplayPrompt>,
    pub exchanges: Vec<ReplayExchange>,
}

/// `manifest.json`: rewritten after every record so a killed session still reads truthfully.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReplayManifest {
    pub schema: String,
    pub session_id: SessionId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub prompt_count: usize,
    pub exchange_count: u32,
}

/// Opens per-session replay dumps under one root directory.
#[derive(Clone, Debug)]
pub struct ReplayDumper {
    root: PathBuf,
    redactor: SecretRedactor,
}

impl ReplayDumper {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, redactor: SecretRedactor) -> Self {
        Self {
            root: root.into(),
            redactor,
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Opens the dump for `session_id`, creating it or resuming an existing one.
    pub async fn open(&self, session_id: SessionId) -> Result<SessionReplay> {
        let directory = self.root.join(session_id.to_string());
        create_dir(&directory).await?;
        create_dir(&directory.join("prompts")).await?;
        create_dir(&directory.join("exchanges")).await?;

        let existing = read_manifest(&directory).await?;
        let created_at = existing.as_ref().map_or_else(Utc::now, |m| m.created_at);
        let state = State {
            prompts: read_prompt_keys(&directory).await?,
            next_sequence: existing.map_or(1, |m| m.exchange_count.saturating_add(1)),
        };
        let replay = SessionReplay {
            directory,
            session_id,
            created_at,
            redactor: self.redactor.clone(),
            state: Mutex::new(state),
        };
        let state = replay.state.lock().await;
        replay.write_manifest(&state).await?;
        drop(state);
        Ok(replay)
    }

    /// Records a complete session in one call and returns its directory.
    pub async fn dump(&self, bundle: &ReplayBundle) -> Result<PathBuf> {
        let replay = self.open(bundle.session_id).await?;
        for prompt in &bundle.prompts {
            replay.record_prompt(prompt).await?;
        }
        for exchange in &bundle.exchanges {
            replay.record_exchange(exchange).await?;
        }
        Ok(replay.directory)
    }
}

/// A single session's dump directory. Cheap to share; appends are serialised internally.
#[derive(Debug)]
pub struct SessionReplay {
    directory: PathBuf,
    session_id: SessionId,
    created_at: Timestamp,
    redactor: SecretRedactor,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    prompts: HashSet<String>,
    next_sequence: u32,
}

impl SessionReplay {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.directory
    }

    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// Writes a prompt body once per `id@version`; repeat calls for the same prompt are no-ops.
    pub async fn record_prompt(&self, prompt: &ReplayPrompt) -> Result<()> {
        let key = format!("{}@{}", prompt.id, prompt.version);
        let mut state = self.state.lock().await;
        if !state.prompts.insert(key.clone()) {
            return Ok(());
        }

        let file = format!("{}.md", safe_name(&key));
        let body = format!(
            "<!-- id: {} · version: {} · hash: {} · source: {} -->\n{}\n",
            prompt.id, prompt.version, prompt.hash, prompt.path, prompt.content
        );
        if let Err(error) = self
            .write_text(&self.directory.join("prompts").join(&file), &body)
            .await
        {
            state.prompts.remove(&key);
            return Err(error);
        }
        let record = ReplayPromptRecord {
            id: prompt.id.clone(),
            version: prompt.version,
            hash: prompt.hash.clone(),
            path: prompt.path.clone(),
            file,
        };
        self.append_line(&self.directory.join("prompts.jsonl"), &record)
            .await?;
        self.write_manifest(&state).await
    }

    /// Appends one provider call and returns the sequence it was filed under.
    pub async fn record_exchange(&self, exchange: &ReplayExchange) -> Result<u32> {
        let mut state = self.state.lock().await;
        let sequence = state.next_sequence;
        let exchanges = self.directory.join("exchanges");
        self.write_json(
            &exchanges.join(format!("{sequence:05}-input.json")),
            &exchange.input,
        )
        .await?;
        self.write_json(
            &exchanges.join(format!("{sequence:05}-output.json")),
            &exchange.output,
        )
        .await?;
        let record = ReplayExchangeRecord {
            sequence,
            exchange: exchange.clone(),
        };
        self.append_line(&self.directory.join("exchanges.jsonl"), &record)
            .await?;

        state.next_sequence = sequence.saturating_add(1);
        self.write_manifest(&state).await?;
        tracing::debug!(
            session_id = %self.session_id,
            sequence,
            provider = %exchange.provider,
            "replay exchange recorded"
        );
        Ok(sequence)
    }

    /// Reads the manifest back from disk.
    pub async fn manifest(&self) -> Result<ReplayManifest> {
        read_manifest(&self.directory)
            .await?
            .ok_or_else(|| BhippiError::Io {
                operation: "read replay manifest",
                path: self.directory.display().to_string(),
                reason: "the manifest is missing".to_owned(),
                retryable: false,
                hint: Some("Delete this replay directory and start a new session.".to_owned()),
            })
    }

    async fn write_manifest(&self, state: &State) -> Result<()> {
        let manifest = ReplayManifest {
            schema: SCHEMA.to_owned(),
            session_id: self.session_id,
            created_at: self.created_at,
            updated_at: Utc::now(),
            prompt_count: state.prompts.len(),
            exchange_count: state.next_sequence.saturating_sub(1),
        };
        let text = encode(&self.directory, &manifest)?;
        let redacted = self.redactor.redact(&text);
        let temporary = self.directory.join("manifest.json.tmp");
        let destination = self.directory.join("manifest.json");
        tokio::fs::write(&temporary, redacted)
            .await
            .map_err(|error| io_error("write replay manifest", &temporary, error))?;
        tokio::fs::rename(&temporary, &destination)
            .await
            .map_err(|error| io_error("commit replay manifest", &destination, error))
    }

    async fn write_json<T: Serialize + ?Sized>(&self, path: &Path, value: &T) -> Result<()> {
        let text = encode(path, value)?;
        self.write_text(path, &text).await
    }

    async fn write_text(&self, path: &Path, text: &str) -> Result<()> {
        tokio::fs::write(path, self.redactor.redact(text))
            .await
            .map_err(|error| io_error("write replay artifact", path, error))
    }

    async fn append_line<T: Serialize>(&self, path: &Path, value: &T) -> Result<()> {
        let line = serde_json::to_string(value).map_err(|error| BhippiError::Io {
            operation: "encode replay index row",
            path: path.display().to_string(),
            reason: error.to_string(),
            retryable: false,
            hint: Some("Inspect the replay value for unsupported data.".to_owned()),
        })?;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(|error| io_error("open replay index", path, error))?;
        file.write_all(format!("{}\n", self.redactor.redact(&line)).as_bytes())
            .await
            .map_err(|error| io_error("append replay index row", path, error))?;
        file.flush()
            .await
            .map_err(|error| io_error("flush replay index", path, error))
    }
}

async fn read_manifest(directory: &Path) -> Result<Option<ReplayManifest>> {
    let path = directory.join("manifest.json");
    match tokio::fs::read_to_string(&path).await {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|error| BhippiError::Io {
                operation: "read replay manifest",
                path: path.display().to_string(),
                reason: error.to_string(),
                retryable: false,
                hint: Some("Delete this replay directory and start a new session.".to_owned()),
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_error("read replay manifest", &path, error)),
    }
}

async fn read_prompt_keys(directory: &Path) -> Result<HashSet<String>> {
    let path = directory.join("prompts.jsonl");
    let text = match tokio::fs::read_to_string(&path).await {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashSet::new()),
        Err(error) => return Err(io_error("read replay prompt index", &path, error)),
    };
    let mut keys = HashSet::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let record: ReplayPromptRecord =
            serde_json::from_str(line).map_err(|error| BhippiError::Io {
                operation: "read replay prompt index",
                path: path.display().to_string(),
                reason: error.to_string(),
                retryable: false,
                hint: Some("Delete this replay directory and start a new session.".to_owned()),
            })?;
        keys.insert(format!("{}@{}", record.id, record.version));
    }
    Ok(keys)
}

async fn create_dir(path: &Path) -> Result<()> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|error| io_error("create replay directory", path, error))
}

fn encode<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<String> {
    serde_json::to_string_pretty(value).map_err(|error| BhippiError::Io {
        operation: "encode replay artifact",
        path: path.display().to_string(),
        reason: error.to_string(),
        retryable: false,
        hint: Some("Inspect the replay value for unsupported data.".to_owned()),
    })
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn io_error(operation: &'static str, path: &Path, error: std::io::Error) -> BhippiError {
    BhippiError::Io {
        operation,
        path: path.display().to_string(),
        reason: error.to_string(),
        retryable: true,
        hint: Some("Check available disk space and directory permissions.".to_owned()),
    }
}
