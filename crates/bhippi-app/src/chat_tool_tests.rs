//! Offline regression tests through the real chat loop and asset dispatch.
use super::*;
use bhippi_providers::model::{Capabilities, CostClass, DeltaStream};
use std::{collections::VecDeque, path::PathBuf};

struct Silent;
impl Emit for Silent {
    fn thinking(&self, _: &str, _: &str, _: AgentPhase) {}
    fn limits(&self, _: &str, _: LimitSnapshot) {}
    fn thought_delta(&self, _: &str, _: &str) {}
    fn delta(&self, _: &str, _: &str) {}
    fn tool(&self, _: &str, _: ToolActivity) {}
    fn permission(&self, _: &str, _: PermissionRequest) {}
    fn done(&self, _: ChatTurnDone) {}
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("bhippi-tool-loop-{}", new_id()));
        std::fs::create_dir_all(path.join("assets")).unwrap();
        std::fs::write(path.join("assets/prop.glb"), b"fixture asset").unwrap();
        Self(path)
    }
    fn workspace(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _removed = std::fs::remove_dir_all(&self.0);
    }
}

enum Reply {
    Deltas(Vec<bhippi_types::Result<Delta>>),
    StartupFailure,
    CancelStream(watch::Sender<bool>),
    CancelStartup(watch::Sender<bool>),
}

struct ScriptedProvider {
    caps: Capabilities,
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<CompletionRequest>>,
}
impl ScriptedProvider {
    fn new(replies: Vec<Reply>) -> Arc<Self> {
        Arc::new(Self {
            caps: Capabilities {
                context_window: 1_000_000,
                vision: false,
                tools: true,
                streaming: true,
                tokens_per_second: None,
                cost_class: CostClass::FreeLocal,
            },
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        })
    }
}

fn provider_error() -> bhippi_types::BhippiError {
    bhippi_types::BhippiError::Provider {
        id: "tool-fixture".to_owned(),
        reason: "fixture connection lost".to_owned(),
        retryable: true,
        hint: None,
    }
}

#[async_trait::async_trait]
impl Provider for ScriptedProvider {
    fn id(&self) -> &str {
        "tool-fixture"
    }
    fn caps(&self) -> &Capabilities {
        &self.caps
    }
    async fn health(&self) -> bhippi_types::Health {
        bhippi_types::Health::Healthy { latency_ms: 0 }
    }
    async fn complete(&self, request: CompletionRequest) -> bhippi_types::Result<DeltaStream> {
        self.requests.lock().await.push(request);
        let reply = self
            .replies
            .lock()
            .await
            .pop_front()
            .expect("unexpected extra model round");
        match reply {
            Reply::Deltas(deltas) => Ok(futures_util::stream::iter(deltas).boxed()),
            Reply::StartupFailure => Err(provider_error()),
            Reply::CancelStream(cancel) => {
                cancel.send(true).unwrap();
                Ok(futures_util::stream::pending().boxed())
            }
            Reply::CancelStartup(cancel) => {
                cancel.send(true).unwrap();
                futures_util::future::pending().await
            }
        }
    }
}

fn text(value: &str) -> bhippi_types::Result<Delta> {
    Ok(Delta::Text {
        delta: value.to_owned(),
    })
}

fn reply(value: &str) -> Reply {
    Reply::Deltas(vec![
        text(value),
        Ok(Delta::Done {
            stop_reason: StopReason::Completed,
        }),
    ])
}

const INVALID_TOOL: &str = "<asset_register>{\"missing_rel\":true}</asset_register>";
const REGISTER: &str = "<asset_register>{\"rel\":\"assets/prop.glb\",\"licence\":\"CC0-1.0\",\"provenance\":\"procedural\"}</asset_register>";

async fn seed(engine: &ChatEngine, workspace: &str) -> (String, String) {
    let meta = engine.ensure_conversation(workspace, None).await.unwrap();
    let turn_id = new_id();
    let mut conversations = engine.conversations.lock().await;
    let conversation = conversations
        .iter_mut()
        .find(|c| c.meta.id == meta.id)
        .unwrap();
    for (role, content, id) in [
        (ChatRole::User, "Asset status?", new_id()),
        (ChatRole::Assistant, "", turn_id.clone()),
    ] {
        conversation.turns.push(ChatTurnView {
            id,
            conversation_id: meta.id.clone(),
            role,
            content: content.to_owned(),
            attachments: Vec::new(),
            thinking: None,
            thinking_elapsed_ms: None,
            created_at: meta.created_at,
            state: TurnState::Streaming,
            provider: None,
            model: None,
            tools: Vec::new(),
            permission: None,
            fault: None,
            worked_ms: None,
            changes: None,
            notices: Vec::new(),
            ask: None,
        });
    }
    (meta.id, turn_id)
}

async fn run(
    scratch: &Scratch,
    provider: Arc<ScriptedProvider>,
    cancel: watch::Receiver<bool>,
) -> (Outcome, ChatTurnView) {
    run_with_prompt(scratch, provider, cancel, "Asset status?").await
}

async fn run_with_prompt(
    scratch: &Scratch,
    provider: Arc<ScriptedProvider>,
    cancel: watch::Receiver<bool>,
    prompt: &str,
) -> (Outcome, ChatTurnView) {
    let engine = Arc::new(ChatEngine::new(Silent));
    let (conversation, turn) = seed(&engine, &scratch.workspace()).await;
    for user in engine
        .conversations
        .lock()
        .await
        .iter_mut()
        .flat_map(|c| &mut c.turns)
        .filter(|t| t.role == ChatRole::User)
    {
        user.content = prompt.to_owned();
    }
    let registry = Arc::new(ProviderRuntime::from_detection(Vec::new()));
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        engine.run_turn(
            &registry,
            &conversation,
            &turn,
            TurnPlan {
                provider: Some((provider, "Tool fixture".to_owned())),
                provider_id: Some("tool-fixture".to_owned()),
                model: None,
                effort: Effort::Fast,
                design: DesignMode::default(),
                caveman: false,
                workspace: scratch.workspace(),
                attachments: Vec::new(),
            },
            cancel,
        ),
    )
    .await
    .expect("the tool loop must terminate");
    let conversations = engine.conversations.lock().await;
    let view = conversations
        .iter()
        .flat_map(|c| &c.turns)
        .find(|t| t.id == turn)
        .unwrap()
        .clone();
    (result, view)
}

#[tokio::test]
async fn tools_execute_on_follow_up_and_results_reach_the_next_round() {
    let scratch = Scratch::new();
    let provider = ScriptedProvider::new(vec![
        reply(INVALID_TOOL),
        Reply::Deltas(vec![
            Ok(Delta::Step {
                id: "read-1".to_owned(),
                verb: "read".to_owned(),
                title: "Read".to_owned(),
                detail: "Check asset".to_owned(),
                paths: Vec::new(),
                done: false,
            }),
            Ok(Delta::Step {
                id: "read-1".to_owned(),
                verb: "read".to_owned(),
                title: String::new(),
                detail: String::new(),
                paths: Vec::new(),
                done: true,
            }),
            text(REGISTER),
            Ok(Delta::Usage {
                input_tokens: 7,
                output_tokens: 11,
            }),
            Ok(Delta::Done {
                stop_reason: StopReason::Completed,
            }),
        ]),
        reply("The asset is registered."),
    ]);
    let (_cancel, rx) = watch::channel(false);
    let (outcome, turn) = run(&scratch, provider.clone(), rx).await;
    assert_eq!(outcome.state, TurnState::Done);
    assert_eq!(outcome.usage.unwrap().output_tokens, 11);
    assert!(scratch.0.join("assets/prop.glb.meta.json").is_file());
    assert!(turn.content.contains("The asset is registered."));
    assert!(!turn.content.contains("<asset_"));
    assert!(turn
        .tools
        .iter()
        .any(|tool| tool.id.contains("round-0-read-1") && tool.state == ToolState::Ok));
    let requests = provider.requests.lock().await;
    assert_eq!(requests.len(), 3);
    assert!(requests[1]
        .messages
        .last()
        .unwrap()
        .content
        .contains("Invalid <asset_register>"));
    assert!(requests[2]
        .messages
        .last()
        .unwrap()
        .content
        .contains("asset registered: assets/prop.glb"));
}

#[tokio::test]
async fn follow_up_provider_failures_never_report_done_or_execute_buffered_tools() {
    for failure in [
        Reply::StartupFailure,
        Reply::Deltas(vec![text(REGISTER), Err(provider_error())]),
        Reply::Deltas(vec![
            text(REGISTER),
            Ok(Delta::Done {
                stop_reason: StopReason::Failed,
            }),
        ]),
    ] {
        let scratch = Scratch::new();
        let provider = ScriptedProvider::new(vec![reply(INVALID_TOOL), failure]);
        let (_cancel, rx) = watch::channel(false);
        let (outcome, turn) = run(&scratch, provider, rx).await;
        assert_eq!(outcome.state, TurnState::Failed);
        assert!(outcome.error.is_some());
        assert!(outcome.fault.is_some());
        assert!(!scratch.0.join("assets/prop.glb.meta.json").exists());
        assert!(!turn.content.contains("<asset_"));
    }
}

#[tokio::test]
async fn cancellation_interrupts_a_pending_follow_up_connection_or_stream() {
    for during_startup in [true, false] {
        let scratch = Scratch::new();
        let (cancel, rx) = watch::channel(false);
        let pending = if during_startup {
            Reply::CancelStartup(cancel)
        } else {
            Reply::CancelStream(cancel)
        };
        let provider = ScriptedProvider::new(vec![reply(INVALID_TOOL), pending]);
        let (outcome, _) = run(&scratch, provider, rx).await;
        assert_eq!(outcome.state, TurnState::Stopped);
    }
}

#[tokio::test]
async fn follow_up_questions_are_saved_for_the_user() {
    let scratch = Scratch::new();
    let provider = ScriptedProvider::new(vec![reply(INVALID_TOOL), reply(
        "<ask_user>{\"question\":\"Which asset?\",\"options\":[{\"label\":\"Prop\"},{\"label\":\"Terrain\"}]}</ask_user>",
    )]);
    let (_cancel, rx) = watch::channel(false);
    let (outcome, turn) = run(&scratch, provider, rx).await;
    assert_eq!(outcome.state, TurnState::Done);
    assert_eq!(turn.ask.unwrap().question, "Which asset?");
}

#[tokio::test]
async fn mixed_asset_tools_keep_results_and_strip_all_processed_markup() {
    let scratch = Scratch::new();
    let library = Scratch::new();
    let engine = Arc::new(ChatEngine::new(Silent));
    let source = library
        .0
        .join("assets/prop.glb")
        .to_string_lossy()
        .into_owned();
    let imports = format!(
        "<asset_import>{}</asset_import>",
        serde_json::json!({"source":source,"dest":"assets/copied.glb"})
    );
    let raw = format!(
        "Ready. {imports}{REGISTER}<sketchfab_find>{{\"query\":\"tree\"}}</sketchfab_find>"
    );
    let mut answers = Vec::new();
    let visible = engine
        .run_asset_tools(
            "fixture",
            Some(&scratch.0),
            &scratch.workspace(),
            &[library.workspace()],
            &raw,
            &mut answers,
        )
        .await;
    assert_eq!(visible.trim(), "Ready.");
    assert!(answers
        .iter()
        .any(|(label, _)| label.contains("asset imported")));
    assert!(answers
        .iter()
        .any(|(label, _)| label.contains("asset registered")));
    assert!(answers
        .iter()
        .any(|(label, _)| label == "sketchfab unavailable"));
    assert_eq!(
        std::fs::read(scratch.0.join("assets/copied.glb")).unwrap(),
        b"fixture asset"
    );
    assert_eq!(std::fs::read(source).unwrap(), b"fixture asset");
}

#[tokio::test]
async fn missing_workspace_and_bad_requests_return_actionable_feedback() {
    let scratch = Scratch::new();
    let engine = Arc::new(ChatEngine::new(Silent));
    let mut answers = Vec::new();
    let missing = scratch.0.join("missing").to_string_lossy().into_owned();
    let visible = engine
        .run_asset_tools("fixture", None, &missing, &[], REGISTER, &mut answers)
        .await;
    assert!(visible.is_empty());
    assert!(answers
        .iter()
        .any(|(_, answer)| answer.contains("workspace folder is missing")));
    answers.clear();
    let raw = format!("{INVALID_TOOL}{REGISTER}<asset_import>{{}}");
    engine
        .run_asset_tools(
            "fixture",
            Some(&scratch.0),
            &scratch.workspace(),
            &[],
            &raw,
            &mut answers,
        )
        .await;
    assert_eq!(
        answers
            .iter()
            .filter(|(label, _)| label == "invalid asset tool request")
            .count(),
        2
    );
    assert!(answers
        .iter()
        .any(|(label, _)| label.contains("asset registered")));
}

#[test]
fn mixed_single_and_array_imports_are_all_parsed() {
    let raw = "<asset_import>{\"source\":\"a.glb\"}</asset_import><asset_import>[{\"source\":\"b.glb\"},{\"source\":\"c.glb\"}]</asset_import><asset_import>{}</asset_import>";
    let (imports, errors) = crate::asset_library::checked_tagged::<
        crate::asset_library::AssetImportTag,
    >(raw, "asset_import", true);
    assert_eq!(imports.len(), 3);
    assert_eq!(errors.len(), 1);
    assert_eq!(
        crate::asset_library::extract_asset_import_tags(raw).len(),
        3
    );
}

#[tokio::test]
async fn failed_queries_have_failed_activity_and_repair_feedback() {
    let scratch = Scratch::new();
    let engine = Arc::new(ChatEngine::new(Silent));
    let (_conversation, turn) = seed(&engine, &scratch.workspace()).await;
    let mut batches = Vec::new();
    let mut answers = Vec::new();
    engine
        .run_godot_call(
            &turn,
            &scratch.0,
            &crate::godot_bridge::GodotCall::Query("{\"kind\":\"not_a_query\"}".to_owned()),
            &mut batches,
            &mut answers,
        )
        .await;
    assert!(answers[0].1.contains("that is not a query"));
    let conversations = engine.conversations.lock().await;
    let turn = conversations
        .iter()
        .flat_map(|c| &c.turns)
        .find(|t| t.id == turn)
        .unwrap();
    assert_eq!(turn.tools.last().unwrap().state, ToolState::Failed);
}

#[tokio::test]
async fn workspace_read_write_read_rounds_use_real_files_and_tool_results() {
    let scratch = Scratch::new();
    std::fs::write(scratch.0.join("notes.txt"), "old text").unwrap();
    let provider = ScriptedProvider::new(vec![
        reply("<read_file path=\"notes.txt\" />"),
        reply("<write_file path=\"notes.txt\">new text</write_file>"),
        reply("<read_file path='notes.txt' />"),
        reply("Verified the saved text."),
    ]);
    let (_cancel, rx) = watch::channel(false);
    let (outcome, turn) = run_with_prompt(
        &scratch,
        provider.clone(),
        rx,
        "Create updated notes.txt with new text.",
    )
    .await;
    assert_eq!(outcome.state, TurnState::Done);
    assert_eq!(
        std::fs::read_to_string(scratch.0.join("notes.txt")).unwrap(),
        "new text"
    );
    assert!(!turn.content.contains("<read_file"));
    assert!(!turn.content.contains("<write_file"));
    assert!(
        turn.tools
            .iter()
            .filter(|tool| tool.state == ToolState::Ok)
            .count()
            >= 3
    );
    let requests = provider.requests.lock().await;
    assert_eq!(requests.len(), 4);
    assert!(requests[1]
        .messages
        .last()
        .unwrap()
        .content
        .contains("old text"));
    assert!(requests[2]
        .messages
        .last()
        .unwrap()
        .content
        .contains("written_bytes"));
    assert!(requests[3]
        .messages
        .last()
        .unwrap()
        .content
        .contains("new text"));
    assert!(
        requests[3]
            .messages
            .iter()
            .any(|message| message.content.contains("old text")),
        "earlier tool observations must remain available"
    );
}

#[tokio::test]
async fn workspace_writes_preserve_undo_and_report_refused_paths() {
    let scratch = Scratch::new();
    let engine = Arc::new(ChatEngine::new(Silent));
    let (_, turn) = seed(&engine, &scratch.workspace()).await;
    std::fs::write(scratch.0.join("notes.txt"), "before").unwrap();
    let raw = "<write_file path='notes.txt'>after</write_file><read_file path='../outside.txt'/><write_file path='../escape.txt'>bad</write_file>";
    let mut answers = Vec::new();
    assert!(engine
        .run_workspace_tools(&turn, None, &scratch.workspace(), raw, &mut answers)
        .await
        .0
        .is_empty());
    assert_eq!(answers.len(), 3);
    assert_eq!(
        answers
            .iter()
            .filter(|(label, _)| label.starts_with("refused"))
            .count(),
        2
    );
    assert_eq!(
        std::fs::read_to_string(scratch.0.join("notes.txt")).unwrap(),
        "after"
    );
    assert_eq!(engine.undo_turn(&turn).await.unwrap(), 1);
    assert_eq!(
        std::fs::read_to_string(scratch.0.join("notes.txt")).unwrap(),
        "before"
    );
}

#[test]
fn file_protocol_preserves_content_and_reports_each_malformed_request() {
    let body = "hello 🖌️\r\n  indented\n";
    let raw = format!("Before<write_file path=\"notes.txt\">\r\n{body}</write_file>After<read_file path='notes.txt' />");
    let (visible, requests) = workspace_tools::parse(&raw);
    assert_eq!(visible, "BeforeAfter");
    assert_eq!(requests.len(), 2);
    match requests.into_iter().next().unwrap().unwrap() {
        workspace_tools::FileRequest::Write { content, .. } => {
            assert_eq!(content.as_bytes(), body.as_bytes())
        }
        _ => panic!("expected a write"),
    }
    let (visible, errors) = workspace_tools::parse(
        "<read_files>prose</read_files><read_file /><write_file path='x'>unfinished",
    );
    assert_eq!(visible, "<read_files>prose</read_files>");
    assert_eq!(errors.len(), 2);
    assert!(errors.iter().all(Result::is_err));
}

#[tokio::test]
async fn file_reads_refuse_binary_and_oversized_data_without_partial_success() {
    let scratch = Scratch::new();
    let path = scratch.0.join("notes.txt");
    std::fs::write(&path, b"a\0b").unwrap();
    assert!(workspace_tools::read(&path)
        .await
        .unwrap_err()
        .contains("binary"));
    std::fs::write(
        &path,
        vec![b'x'; bhippi_types::WORKSPACE_TOOL_MAX_FILE_BYTES as usize + 1],
    )
    .unwrap();
    assert!(workspace_tools::read(&path)
        .await
        .unwrap_err()
        .contains("too large"));
    let root = std::fs::canonicalize(&scratch.0).unwrap();
    for path in [
        "../escape",
        "/outside",
        "C:\\outside",
        "notes.txt:stream",
        "",
    ] {
        assert!(
            workspace_tools::resolve(&root, path).await.is_err(),
            "{path}"
        );
    }
}

#[tokio::test]
async fn cancellation_never_executes_buffered_workspace_writes() {
    let scratch = Scratch::new();
    let provider = ScriptedProvider::new(vec![
        reply(INVALID_TOOL),
        Reply::Deltas(vec![
            text("<write_file path='notes.txt'>never written</write_file>"),
            Ok(Delta::Done {
                stop_reason: StopReason::Cancelled,
            }),
        ]),
    ]);
    let (_cancel, rx) = watch::channel(false);
    let (outcome, turn) = run(&scratch, provider, rx).await;
    assert_eq!(outcome.state, TurnState::Stopped);
    assert!(!scratch.0.join("notes.txt").exists());
    assert!(!turn.content.contains("<write_file"));
}

#[cfg(windows)]
#[tokio::test]
async fn workspace_tools_refuse_a_windows_junction_before_creating_outside_folders() {
    let scratch = Scratch::new();
    let outside = Scratch::new();
    let junction = scratch.0.join("linked");
    let mut command = tokio::process::Command::new("cmd.exe");
    command
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside.0)
        .env_clear()
        .env("SystemRoot", std::env::var("SystemRoot").unwrap())
        .creation_flags(0x0800_0000)
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root = std::fs::canonicalize(&scratch.0).unwrap();
    let read = workspace_tools::resolve(&root, "linked/assets/prop.glb").await;
    let write = workspace_tools::resolve(&root, "linked/new-folder/notes.txt").await;
    // Remove only the junction itself before the temporary directories are cleaned up.
    std::fs::remove_dir(&junction).unwrap();
    assert!(read.unwrap_err().contains("outside"));
    assert!(write.unwrap_err().contains("outside"));
    assert!(!outside.0.join("new-folder").exists());
}
