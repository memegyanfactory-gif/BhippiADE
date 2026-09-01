use bhippi_core::{LoggingGuard, SecretRedactor};
use bhippi_types::SessionId;
use std::path::{Path, PathBuf};

fn log_dir() -> PathBuf {
    std::env::temp_dir().join(format!("bhippi-logs-{}", SessionId::new()))
}

fn read_logs(path: &Path) -> String {
    let mut output = String::new();
    let entries = std::fs::read_dir(path)
        .unwrap_or_else(|error| panic!("log directory must be readable: {error}"));
    for entry in entries.filter_map(std::result::Result::ok) {
        if entry.path().is_file() {
            output.push_str(
                &std::fs::read_to_string(entry.path())
                    .unwrap_or_else(|error| panic!("log file must be readable: {error}")),
            );
        }
    }
    output
}

#[test]
fn rolling_json_log_scrubs_a_seeded_key() {
    let path = log_dir();
    let redactor = SecretRedactor::default();
    let seeded_key = "sk-bhippi-seeded-test-key-123456";
    redactor
        .register(seeded_key)
        .unwrap_or_else(|error| panic!("seeded key must register: {error}"));
    let logging = LoggingGuard::new(&path, redactor)
        .unwrap_or_else(|error| panic!("rolling logger must start: {error}"));

    logging.with_default(|| {
        tracing::info!(
            session_id = "01TEST",
            credential = seeded_key,
            "provider probe"
        );
    });
    drop(logging);

    let output = read_logs(&path);
    assert!(!output.contains(seeded_key));
    assert!(output.contains("[REDACTED]"));
    assert!(output.contains("\"session_id\":\"01TEST\""));

    let _ = std::fs::remove_dir_all(path);
}
