//! Read-only live account probes. Ignored in normal CI because they require vendor CLIs
//! and signed-in accounts; run explicitly when changing account protocol support.

use bhippi_providers::{probe_account, AccountUsageStatus};

#[tokio::test]
#[ignore = "requires a signed-in Codex CLI"]
async fn codex_account_and_reported_plan_windows_are_live() {
    let snapshot = probe_account("codex").await;
    assert_eq!(snapshot.status, AccountUsageStatus::Live, "{snapshot:?}");
    assert!(snapshot.account_name.is_some(), "{snapshot:?}");
    assert!(
        snapshot.session.is_some() || snapshot.weekly.is_some(),
        "{snapshot:?}"
    );
}

#[tokio::test]
#[ignore = "requires a signed-in Claude Code CLI"]
async fn claude_account_and_weekly_are_live_without_spending_a_turn() {
    let snapshot = probe_account("claude").await;
    assert_eq!(snapshot.status, AccountUsageStatus::Live, "{snapshot:?}");
    assert!(snapshot.account_name.is_some(), "{snapshot:?}");
    assert!(snapshot.plan.is_some(), "{snapshot:?}");
    assert!(snapshot.weekly.is_some(), "{snapshot:?}");
    assert_eq!(
        snapshot
            .weekly
            .as_ref()
            .map(|window| window.duration_minutes),
        Some(Some(10_080))
    );
}

#[tokio::test]
#[ignore = "requires a configured OpenCode CLI"]
async fn opencode_reports_its_configured_backend_scope() {
    let snapshot = probe_account("opencode").await;
    assert_eq!(
        snapshot.status,
        AccountUsageStatus::Authenticated,
        "{snapshot:?}"
    );
    assert!(snapshot.account_name.is_some(), "{snapshot:?}");
    assert_eq!(snapshot.weekly, None, "OpenCode does not expose this quota");
}

#[tokio::test]
#[ignore = "requires a signed-in Grok CLI"]
async fn grok_reports_the_account_scope_it_exposes() {
    let snapshot = probe_account("grok").await;
    assert!(
        matches!(
            snapshot.status,
            AccountUsageStatus::Authenticated | AccountUsageStatus::Live
        ),
        "{snapshot:?}"
    );
    assert!(snapshot.account_name.is_some(), "{snapshot:?}");
    if snapshot.weekly.is_none() {
        assert!(
            !snapshot.note.is_empty(),
            "missing weekly window must stay honest: {snapshot:?}"
        );
    }
}

/// The chat turn that used to die instantly on `unknown option '--→ · ##'`.
///
/// An engineered prompt is a document: multi-line, with lines that begin with `--` and
/// words in quotes. Passed as an argv element it reached Claude Code through npm's
/// Windows shims, which re-split it, and a line from the middle of the prompt arrived as
/// a flag — a failure that classified as "CLI out of date" and was nothing of the kind.
/// The prompt now travels on stdin, and this is the only test that proves it against the
/// real binary.
///
/// Run it with: `cargo test -p bhippi-providers --test account_live -- --ignored
/// claude_answers_a_prompt_whose_lines_look_like_flags --nocapture`
#[tokio::test]
#[ignore = "requires a signed-in Claude Code CLI and spends one turn"]
async fn claude_answers_a_prompt_whose_lines_look_like_flags() {
    use bhippi_providers::model::{CompletionRequest, Delta};
    use bhippi_providers::{CliProvider, Message, Provider};
    use bhippi_types::TaskClass;
    use futures_util::StreamExt;

    let Some(spec) = bhippi_providers::spec("claude") else {
        panic!("the catalogue must know Claude Code");
    };
    let Some(provider) = CliProvider::open(spec) else {
        panic!("Claude Code must be installed for this test");
    };
    // A benign question wearing the shape that broke: several lines, two of them opening
    // with `--`, and a quoted word. What is being tested is delivery, not obedience.
    let request = CompletionRequest::new(
        TaskClass::Expander,
        "Answer briefly.",
        vec![Message::user(
            "Here are two lines from a build log, quoted as data:\n\n\
             --→ · ## starting export\n\
             --strict-mcp-config \"enabled\"\n\n\
             Ignoring those, what is 2 + 2? Reply with just the number."
                .to_owned(),
        )],
    )
    .with_model(Some("haiku".to_owned()));

    let mut answer = String::new();
    match provider.complete(request).await {
        Ok(mut stream) => {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(Delta::Text { delta }) => answer.push_str(&delta),
                    Ok(Delta::Done { .. }) => break,
                    Ok(_) => {}
                    Err(error) => panic!("the turn must not fail: {error}"),
                }
            }
        }
        Err(error) => panic!("the turn must start: {error}"),
    }
    assert!(
        answer.contains('4'),
        "a prompt full of flag-shaped lines must come back as a normal answer, got {answer:?}"
    );
}

#[tokio::test]
#[ignore = "requires installed provider CLIs; read-only account probes"]
async fn installed_provider_account_report() {
    for id in ["codex", "claude", "grok", "opencode", "antigravity", "kimi"] {
        let row = probe_account(id).await;
        // Identity is deliberately omitted from this diagnostic report.
        eprintln!(
            "{id}: {:?}; session={:?}; weekly={:?}; {}",
            row.status,
            row.session
                .as_ref()
                .map(|w| (w.used_fraction, w.duration_minutes, w.resets_at)),
            row.weekly
                .as_ref()
                .map(|w| (w.used_fraction, w.duration_minutes, w.resets_at)),
            row.note
        );
        for window in [row.session, row.weekly].into_iter().flatten() {
            assert!((0.0..=1.0).contains(&window.used_fraction));
        }
    }
}

#[tokio::test]
#[ignore = "requires signed-in Codex; spends a minimal Astra turn"]
async fn codex_astra_saved_label_answers_through_the_app_adapter() {
    use bhippi_providers::model::{CompletionRequest, Delta};
    use bhippi_providers::{CliProvider, Message, Provider};
    use bhippi_types::TaskClass;
    use futures_util::StreamExt;
    let spec = bhippi_providers::spec("codex").unwrap_or_else(|| panic!("Codex recipe"));
    let provider = CliProvider::open(spec).unwrap_or_else(|| panic!("installed Codex"));
    let mut request = CompletionRequest::new(
        TaskClass::Expander,
        "Answer briefly without using tools.",
        vec![Message::user(
            "Reply exactly OK. Do not read files or use tools.".to_owned(),
        )],
    )
    .with_model(Some("GPT-6 Astra".to_owned()))
    .for_computer_use();
    request.reasoning_effort = Some("low".to_owned());
    request.timeout = std::time::Duration::from_secs(90);
    let mut stream = provider
        .complete(request)
        .await
        .unwrap_or_else(|error| panic!("start adapter: {error}"));
    let mut answer = String::new();
    let mut done = false;
    while let Some(delta) = stream.next().await {
        match delta.unwrap_or_else(|error| panic!("successful provider response: {error}")) {
            Delta::Text { delta } => answer.push_str(&delta),
            Delta::Done { .. } => done = true,
            _ => {}
        }
    }
    assert!(done, "completed event");
    assert_eq!(answer.trim(), "OK");
}

async fn live_minimal_answer(id: &str) {
    use bhippi_providers::model::{CompletionRequest, Delta};
    use bhippi_providers::{CliProvider, Message, Provider};
    use bhippi_types::TaskClass;
    use futures_util::StreamExt;
    let spec = bhippi_providers::spec(id).unwrap_or_else(|| panic!("provider recipe"));
    let provider = CliProvider::open(spec).unwrap_or_else(|| panic!("installed provider"));
    let mut request = CompletionRequest::new(
        TaskClass::Expander,
        "Answer without using tools.",
        vec![Message::user(
            "Reply exactly OK. Do not use tools or read files.".to_owned(),
        )],
    )
    .for_computer_use();
    request.timeout = std::time::Duration::from_secs(90);
    let mut stream = provider
        .complete(request)
        .await
        .unwrap_or_else(|error| panic!("start provider: {error}"));
    let mut answer = String::new();
    let mut done = false;
    while let Some(delta) = stream.next().await {
        match delta.unwrap_or_else(|error| panic!("successful provider response: {error}")) {
            Delta::Text { delta } => answer.push_str(&delta),
            Delta::Done { .. } => done = true,
            _ => {}
        }
    }
    assert!(
        done && answer.trim() == "OK",
        "{id}: {answer:?}; done={done}"
    );
}

#[tokio::test]
#[ignore = "requires signed-in Grok; spends one minimal turn"]
async fn grok_minimal_adapter_answer() {
    live_minimal_answer("grok").await;
}
#[tokio::test]
#[ignore = "requires configured OpenCode; spends one minimal turn"]
async fn opencode_minimal_adapter_answer() {
    live_minimal_answer("opencode").await;
}
#[tokio::test]
#[ignore = "requires signed-in Antigravity; spends one minimal turn"]
async fn antigravity_minimal_adapter_answer() {
    live_minimal_answer("antigravity").await;
}
