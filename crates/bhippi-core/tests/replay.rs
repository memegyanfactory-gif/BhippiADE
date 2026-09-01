use bhippi_core::{
    ReplayBundle, ReplayDumper, ReplayExchange, ReplayExchangeRecord, ReplayManifest, ReplayPrompt,
    SecretRedactor,
};
use bhippi_types::{ProviderId, SessionId, TaskClass};
use chrono::Utc;
use serde_json::json;
use std::path::{Path, PathBuf};

const SEEDED_SECRET: &str = "bhippi-replay-seeded-secret-123456";

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("bhippi-replay-{}", SessionId::new())))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn redactor() -> SecretRedactor {
    let redactor = SecretRedactor::default();
    redactor
        .register(SEEDED_SECRET)
        .unwrap_or_else(|error| panic!("secret must register: {error}"));
    redactor
}

fn prompt() -> ReplayPrompt {
    ReplayPrompt {
        id: "research.planner".to_owned(),
        version: 4,
        hash: "blake3-placeholder".to_owned(),
        path: "prompts/research.planner.md".to_owned(),
        content: format!("version: 4\ncredential-shaped fixture: {SEEDED_SECRET}"),
    }
}

fn exchange(topic: &str) -> ReplayExchange {
    ReplayExchange {
        task_class: TaskClass::Planner,
        provider: ProviderId::new(),
        input: json!({ "topic": topic, "credential": SEEDED_SECRET }),
        output: json!({ "in_scope": true, "score": 0.94 }),
        started_at: Utc::now(),
        finished_at: Utc::now(),
    }
}

fn read(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()))
}

fn manifest_of(directory: &Path) -> ReplayManifest {
    serde_json::from_str(&read(directory.join("manifest.json")))
        .unwrap_or_else(|error| panic!("manifest must parse: {error}"))
}

#[tokio::test]
async fn a_fake_session_produces_a_readable_secret_free_dump() {
    let root = TempRoot::new();
    let session_id = SessionId::new();
    let bundle = ReplayBundle {
        session_id,
        prompts: vec![prompt()],
        exchanges: vec![exchange("local inference")],
    };

    let directory = ReplayDumper::new(root.path(), redactor())
        .dump(&bundle)
        .await
        .unwrap_or_else(|error| panic!("replay dump must succeed: {error}"));

    let manifest = manifest_of(&directory);
    assert_eq!(manifest.schema, "bhippi.replay/1");
    assert_eq!(manifest.session_id, session_id);
    assert_eq!(manifest.prompt_count, 1);
    assert_eq!(manifest.exchange_count, 1);

    let prompt_body = read(directory.join("prompts").join("research.planner_4.md"));
    let prompt_index = read(directory.join("prompts.jsonl"));
    let exchange_index = read(directory.join("exchanges.jsonl"));
    let input = read(directory.join("exchanges").join("00001-input.json"));
    let output = read(directory.join("exchanges").join("00001-output.json"));

    assert!(prompt_body.contains("research.planner"));
    assert!(prompt_index.contains("research.planner_4.md"));
    assert!(output.contains("0.94"));
    for artifact in [
        &prompt_body,
        &prompt_index,
        &exchange_index,
        &input,
        &output,
    ] {
        assert!(
            !artifact.contains(SEEDED_SECRET),
            "secret leaked into a dump"
        );
    }
    // The three artifacts that carried the credential all show the redaction marker instead.
    for artifact in [&prompt_body, &exchange_index, &input] {
        assert!(artifact.contains("[REDACTED]"));
    }
}

#[tokio::test]
async fn each_provider_call_appends_its_own_numbered_exchange() {
    let root = TempRoot::new();
    let dumper = ReplayDumper::new(root.path(), redactor());
    let replay = dumper
        .open(SessionId::new())
        .await
        .unwrap_or_else(|error| panic!("replay must open: {error}"));

    for index in 0..3 {
        replay
            .record_prompt(&prompt())
            .await
            .unwrap_or_else(|error| panic!("prompt must record: {error}"));
        let sequence = replay
            .record_exchange(&exchange(&format!("call {index}")))
            .await
            .unwrap_or_else(|error| panic!("exchange must record: {error}"));
        assert_eq!(sequence, index + 1);
    }

    let records: Vec<ReplayExchangeRecord> = read(replay.path().join("exchanges.jsonl"))
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("index row must parse: {error}"))
        })
        .collect();
    let manifest = manifest_of(replay.path());

    assert_eq!(records.len(), 3);
    assert_eq!(
        records
            .iter()
            .map(|record| record.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(manifest.exchange_count, 3);
    // The same prompt version is written once, however many calls use it.
    assert_eq!(manifest.prompt_count, 1);
    assert_eq!(read(replay.path().join("prompts.jsonl")).lines().count(), 1);
}

#[tokio::test]
async fn reopening_a_killed_session_continues_the_same_dump() {
    let root = TempRoot::new();
    let dumper = ReplayDumper::new(root.path(), redactor());
    let session_id = SessionId::new();

    let first = dumper
        .open(session_id)
        .await
        .unwrap_or_else(|error| panic!("replay must open: {error}"));
    first
        .record_prompt(&prompt())
        .await
        .unwrap_or_else(|error| panic!("prompt must record: {error}"));
    first
        .record_exchange(&exchange("before the kill"))
        .await
        .unwrap_or_else(|error| panic!("exchange must record: {error}"));
    let created_at = first.manifest().await.map(|manifest| manifest.created_at);
    drop(first);

    let resumed = dumper
        .open(session_id)
        .await
        .unwrap_or_else(|error| panic!("replay must reopen: {error}"));
    let sequence = resumed
        .record_exchange(&exchange("after the kill"))
        .await
        .unwrap_or_else(|error| panic!("exchange must record: {error}"));
    // The prompt was already written before the kill and is not duplicated on resume.
    resumed
        .record_prompt(&prompt())
        .await
        .unwrap_or_else(|error| panic!("prompt must record: {error}"));

    let manifest = manifest_of(resumed.path());
    assert_eq!(sequence, 2);
    assert_eq!(manifest.exchange_count, 2);
    assert_eq!(manifest.prompt_count, 1);
    assert_eq!(created_at.ok(), Some(manifest.created_at));
    assert_eq!(
        read(resumed.path().join("exchanges.jsonl")).lines().count(),
        2
    );
    assert!(resumed
        .path()
        .join("exchanges")
        .join("00002-input.json")
        .exists());
}
