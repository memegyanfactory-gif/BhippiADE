//! Read-only live account probes. Ignored in normal CI because they require vendor CLIs
//! and signed-in accounts; run explicitly when changing account protocol support.

use bhippi_providers::{probe_account, AccountUsageStatus};

#[tokio::test]
#[ignore = "requires a signed-in Codex CLI"]
async fn codex_account_and_both_plan_windows_are_live() {
    let snapshot = probe_account("codex").await;
    assert_eq!(snapshot.status, AccountUsageStatus::Live, "{snapshot:?}");
    assert!(snapshot.account_name.is_some(), "{snapshot:?}");
    assert!(snapshot.session.is_some(), "{snapshot:?}");
    assert!(snapshot.weekly.is_some(), "{snapshot:?}");
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
