# ADR-0021: Separate vendor plan usage from Bhippi token budgets

Date: 2026-08-30 · Status: accepted · Supersedes: ADR-0009 usage-gauge semantics and
`04-PAGES.md` A0.3/A4.0 where they describe a local token cap as provider plan usage

## Context

The existing usage surfaces combine two different quantities: Bhippi's locally recorded token
ledger and user-configured daily cap, and a provider account's rolling subscription windows. The
chat meter labels the local cap as weekly usage when a vendor has not reported a real plan window,
and labels local-midnight as that plan's reset. That is precise-looking but false. Provider account
identity is also absent, so a correct quota cannot be tied to the account that owns it.

Vendors expose different truthful seams. Claude Code exposes account identity through `auth status`
and emits rolling-window utilization during real turns. Codex exposes both identity and current
rate-limit windows through its app-server account protocol. Other CLIs may expose authentication
presence without exposing a numerical plan allowance. No static plan table can remain accurate when
vendors change account entitlements server-side.

## Decision

- Provider account identity and rolling plan windows are probed separately from the ≤1.5 second
  provider-reachability scan and cached for one minute. Opening or manually refreshing Usage forces
  a fresh account probe without slowing chat-provider detection.
- The app reads only vendor-owned, non-secret status/protocol output. It never reads credential
  values from configuration files and never infers an email, plan, or limit from a token count.
- Codex uses the native `app-server --stdio` binary (not the npm `.cmd` shim) with `account/read`
  and `account/rateLimits/read`. Claude Code uses `auth status --json` for identity and
  `claude -p /usage --max-turns 0` for rolling windows; that slash command spends no model turn.
  Live `rate_limit_event` windows from completed calls still overlay the cache. OpenCode and Grok
  report the account scope their public status commands actually reveal. Unsupported or unavailable
  limits render as **Not reported**, never as zero and never as a fabricated percentage.
- Bhippi's local token ledger, estimated cost, and configurable daily guard remain available, but
  are labelled local accounting. They are never substituted for a vendor's weekly allowance or
  reset timestamp.
- Account-derived snapshots stay in memory. Secrets are not persisted; an account switch replaces
  the old snapshot rather than carrying a quota across identities.

## Consequences

Codex and Claude Code can show the signed-in account and vendor-reported plan information on
refresh, without waiting for a chat turn. A provider that does not expose a weekly window shows
less data, but the data shown is trustworthy. Manual refresh has a visible result and automatic
refresh is bounded, while the provider detection performance invariant remains unchanged. The
composer meter prefers a live in-turn snapshot when it is newer than the last probe, and prefers
the probe once the user refreshes.

## Alternatives

- **Scale the local daily cap to seven days.** Rejected because it is a Bhippi guard, not the
  provider's entitlement, and its reset boundary is unrelated.
- **Hardcode limits by plan name.** Rejected because vendor limits vary by model, promotion,
  workspace policy, and server-side account state.
- **Spend a model turn on every refresh to obtain quota metadata.** Rejected because refreshing a
  meter must not consume the allowance it measures.
