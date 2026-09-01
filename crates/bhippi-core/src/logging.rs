use bhippi_types::{BhippiError, Result};
use std::fmt;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, RwLock};
use tracing::Dispatch;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::MakeWriter;

const REDACTED: &str = "[REDACTED]";
const SECRET_PREFIXES: [&str; 5] = ["sk-", "ghp_", "xoxb-", "xoxp-", "AIza"];

#[derive(Clone, Default)]
pub struct SecretRedactor {
    values: Arc<RwLock<Vec<String>>>,
}

impl fmt::Debug for SecretRedactor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretRedactor")
            .finish_non_exhaustive()
    }
}

impl SecretRedactor {
    pub fn register(&self, secret: &str) -> Result<()> {
        if secret.is_empty() {
            return Ok(());
        }
        let mut values = self.values.write().map_err(|_| BhippiError::Secret {
            reason: "secret redactor lock is unavailable".to_owned(),
            hint: Some("Restart Bhippi before loading credentials again.".to_owned()),
        })?;
        if !values.iter().any(|known| known == secret) {
            values.push(secret.to_owned());
            values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        }
        Ok(())
    }

    #[must_use]
    pub fn redact(&self, input: &str) -> String {
        let Ok(values) = self.values.read() else {
            return REDACTED.to_owned();
        };
        let mut output = input.to_owned();
        for secret in values.iter() {
            output = output.replace(secret, REDACTED);
        }
        drop(values);

        for prefix in SECRET_PREFIXES {
            output = redact_prefixed(&output, prefix);
        }
        output
    }
}

pub struct LoggingGuard {
    dispatch: Dispatch,
    _worker: WorkerGuard,
}

impl LoggingGuard {
    pub fn new(log_dir: impl AsRef<Path>, redactor: SecretRedactor) -> Result<Self> {
        let appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix("bhippi")
            .filename_suffix("jsonl")
            .max_log_files(7)
            .build(log_dir)
            .map_err(|error| BhippiError::Config {
                reason: format!("cannot create rolling log writer: {error}"),
                hint: Some("Check the log-directory permissions and restart Bhippi.".to_owned()),
            })?;
        let (writer, worker) = tracing_appender::non_blocking(appender);
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_ansi(false)
            .flatten_event(true)
            .with_writer(RedactingMakeWriter { writer, redactor })
            .finish();

        Ok(Self {
            dispatch: Dispatch::new(subscriber),
            _worker: worker,
        })
    }

    pub fn with_default<T>(&self, operation: impl FnOnce() -> T) -> T {
        tracing::dispatcher::with_default(&self.dispatch, operation)
    }

    pub fn install_global(&self) -> Result<()> {
        tracing::dispatcher::set_global_default(self.dispatch.clone()).map_err(|error| {
            BhippiError::Config {
                reason: format!("cannot install the tracing subscriber: {error}"),
                hint: Some("Install logging once during application startup.".to_owned()),
            }
        })
    }
}

#[derive(Clone)]
struct RedactingMakeWriter {
    writer: NonBlocking,
    redactor: SecretRedactor,
}

impl<'writer> MakeWriter<'writer> for RedactingMakeWriter {
    type Writer = RedactingWriter;

    fn make_writer(&'writer self) -> Self::Writer {
        RedactingWriter {
            writer: self.writer.clone(),
            redactor: self.redactor.clone(),
            buffer: Vec::new(),
        }
    }
}

struct RedactingWriter {
    writer: NonBlocking,
    redactor: SecretRedactor,
    buffer: Vec<u8>,
}

impl RedactingWriter {
    fn write_buffer(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let text = String::from_utf8_lossy(&self.buffer);
        let redacted = self.redactor.redact(&text);
        self.buffer.clear();
        self.writer.write_all(redacted.as_bytes())
    }
}

impl Write for RedactingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.write_buffer()?;
        self.writer.flush()
    }
}

impl Drop for RedactingWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

fn redact_prefixed(input: &str, prefix: &str) -> String {
    let mut remaining = input;
    let mut output = String::with_capacity(input.len());

    while let Some(start) = remaining.find(prefix) {
        output.push_str(&remaining[..start]);
        let candidate = &remaining[start..];
        let end = candidate
            .char_indices()
            .skip(prefix.chars().count())
            .find(|(_, character)| {
                character.is_whitespace() || matches!(character, '"' | '\'' | ',' | ';' | '}' | ']')
            })
            .map_or(candidate.len(), |(index, _)| index);
        output.push_str(REDACTED);
        remaining = &candidate[end..];
    }
    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::SecretRedactor;

    #[test]
    fn exact_and_prefixed_secrets_are_removed() {
        let redactor = SecretRedactor::default();
        redactor
            .register("private-value-123")
            .unwrap_or_else(|error| panic!("secret must register: {error}"));

        let output = redactor.redact("a=private-value-123 b=sk-abcdef123456");

        assert_eq!(output, "a=[REDACTED] b=[REDACTED]");
    }
}
