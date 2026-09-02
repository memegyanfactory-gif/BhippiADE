import { useEffect, useRef, useState } from "react";
import type { LimitSnapshot, ProviderInfo, UsageSummary } from "../lib/ipc";
import { ProviderLogo } from "./ProviderLogo";
// One dollar formatter for the whole app: a per-turn API cost is often a fraction of a
// cent, and a local rule that floored those to "$0.00" is what made the meter unreliable.
import { usd as fmtCost } from "../lib/format";
import {
  IconExternalLink,
  IconEye,
  IconReload,
} from "./icons";

/* ── helpers ────────────────────────────────────────────────────────────── */

/** Human-friendly countdown from an epoch-seconds timestamp. */
function fmtResetEpoch(epoch: number): string {
  const diffMs = epoch * 1000 - Date.now();
  const mins = Math.round(diffMs / 60000);
  if (mins <= 0) return "shortly";
  if (mins < 60) return `in ${mins}m`;
  const hrs = Math.floor(mins / 60);
  const rem = mins % 60;
  if (hrs < 24) return rem > 0 ? `in ${hrs}h ${rem}m` : `in ${hrs}h`;

  // Format as "Day HH:MM AM/PM"
  const d = new Date(epoch * 1000);
  const day = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][d.getDay()];
  const h = d.getHours();
  const m = d.getMinutes();
  const ampm = h >= 12 ? "PM" : "AM";
  const h12 = h % 12 || 12;
  const mm = m.toString().padStart(2, "0");
  return `${day} ${h12}:${mm} ${ampm}`;
}

/** Format token count: 1234 → "1.2K", 1234567 → "1.2M" */
function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return `${n}`;
}


function maskAccount(value: string): string {
  const at = value.indexOf("@");
  if (at <= 1) return value.length <= 4 ? "••••" : `${value.slice(0, 2)}••••`;
  return `${value.slice(0, 2)}••••${value.slice(at)}`;
}

/* ── component ──────────────────────────────────────────────────────────── */

type ChatUsageMeterProps = {
  provider: ProviderInfo | null;
  currentModel?: string | null;
  summary: UsageSummary | null;
  limits: { provider: string; snapshot: LimitSnapshot } | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onRefresh?: () => Promise<void> | void;
  onManage?: () => void;
};

export function ChatUsageMeter({
  provider,
  currentModel,
  summary,
  limits,
  open,
  onOpenChange,
  onRefresh,
  onManage,
}: ChatUsageMeterProps) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [masked, setMasked] = useState(true);

  const handleManualRefresh = () => {
    if (onRefresh && !refreshing) {
      setRefreshing(true);
      Promise.resolve(onRefresh()).finally(() => {
        setTimeout(() => setRefreshing(false), 600);
      });
    }
  };

  // close on outside click or Escape
  useEffect(() => {
    if (!open) return undefined;
    const onPointer = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) onOpenChange(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onOpenChange(false);
    };
    window.addEventListener("mousedown", onPointer);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onPointer);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, onOpenChange]);

  /* ── resolve active provider usage from summary ────────────────────── */

  const providerId = provider?.id ?? summary?.active_provider_id ?? "";
  const providerLabel = provider?.label ?? summary?.active?.label ?? providerId;

  // Find the matching ProviderUsage row inside the summary
  const activeUsage =
    summary?.providers.find(
      (p) => p.id.toLowerCase() === providerId.toLowerCase(),
    ) ??
    (summary?.active?.id?.toLowerCase() === providerId.toLowerCase()
      ? summary.active
      : null) ??
    summary?.active ??
    null;

  /* ── resolve limits snapshot ───────────────────────────────────────── */

  const matchesLimits =
    limits &&
    providerId &&
    limits.provider.toLowerCase() === providerId.toLowerCase();

  const snap: LimitSnapshot | null = matchesLimits ? limits.snapshot : null;
  const account = activeUsage?.account ?? null;

  /* ── weekly ────────────────────────────────────────────────────────── */

  // A local Bhippi token cap is not a vendor subscription allowance. Unknown stays
  // unknown instead of becoming a precise-looking 0% or local-midnight reset.
  const weeklyFraction = snap?.weekly_used ?? account?.weekly?.used_fraction ?? null;
  const weeklyPct = weeklyFraction == null ? null : Math.round(weeklyFraction * 100);
  const weeklyLeftPct = weeklyPct == null ? null : Math.max(0, 100 - weeklyPct);

  // Reset info
  const weeklyResetAt = snap?.weekly_resets_at ?? account?.weekly?.resets_at ?? null;
  const weeklyResetText = weeklyResetAt ? `Resets ${fmtResetEpoch(weeklyResetAt)}` : "";

  /* ── session / 5-hour window ──────────────────────────────────────── */

  const sessionFraction = snap?.session_used ?? account?.session?.used_fraction ?? null;
  const sessionPct = sessionFraction == null ? null : Math.round(sessionFraction * 100);
  const sessionResetAt = snap?.session_resets_at ?? account?.session?.resets_at ?? null;
  const sessionResetText = sessionResetAt ? fmtResetEpoch(sessionResetAt) : "";

  /* ── display values ───────────────────────────────────────────────── */

  const modelName =
    currentModel ?? provider?.models[0] ?? activeUsage?.models?.[0]?.label ?? "";

  const tokens = activeUsage?.total_tokens ?? 0;
  const turns = activeUsage?.turns ?? 0;
  const costUsd = activeUsage?.cost_usd ?? 0;
  const metered = activeUsage?.metered ?? false;

  /* ── accent colour based on usage level ────────────────────────────── */

  const accentColor =
    weeklyPct != null && weeklyPct >= 90 ? "var(--accent-danger, #e06c53)"
    : weeklyPct != null && weeklyPct >= 75 ? "var(--accent-warning, #f59e0b)"
    : "var(--accent)";

  /* ── render ────────────────────────────────────────────────────────── */

  return (
    <div className="composer-popover-anchor" ref={wrapRef}>
      <button
        type="button"
        className={`composer-bar-btn ledger-trigger${open ? " active" : ""}`}
        onClick={() => {
          const next = !open;
          onOpenChange(next);
          if (next && onRefresh) handleManualRefresh();
        }}
        title={
          weeklyPct == null
            ? "Usage · weekly limit not reported"
            : `Usage · ${weeklyLeftPct}% of weekly limit remaining`
        }
        aria-label="Token usage and limits"
        aria-haspopup="dialog"
        aria-expanded={open}
      >
        <span className="ledger-trigger-pill">
          <span
            className="ledger-dot-meter"
            style={{ background: accentColor }}
          />
          <span className="ledger-trigger-text">
            {weeklyPct == null ? "—" : `${weeklyLeftPct}% left`}
          </span>
        </span>
      </button>

      {open ? (
        <div
          className="bhippi-popover ledger-popover"
          role="dialog"
          aria-label="Token usage and limits"
        >
          {/* ── Header ──────────────────────────────────────────────── */}
          <div className="ledger-head-row">
            <div className="ledger-provider-brand">
              <ProviderLogo id={providerId} size={18} />
              <span className="ledger-provider-title">
                {providerLabel.toUpperCase()}
              </span>
            </div>
            {modelName ? (
              <span className="ledger-model-badge" title={modelName}>
                {modelName}
              </span>
            ) : null}
          </div>

          {/* ── Weekly section ───────────────────────────────────────── */}
          <div className="ledger-section">
            <div className="ledger-subhead-line">
              <span className="ledger-period-tag">WEEKLY</span>
              {weeklyResetText ? (
                <span className="ledger-reset-info">{weeklyResetText}</span>
              ) : null}
            </div>

            {weeklyPct == null ? (
              <div className="ledger-unreported" role="status">
                <strong>Not reported</strong>
                <span>{account?.note ?? "This provider does not expose a weekly allowance."}</span>
              </div>
            ) : (
              <>
                <div className="ledger-metric-line">
                  <div className="ledger-big-usage">
                    <span className="ledger-big-number">{weeklyPct}%</span>
                    <span className="ledger-big-unit">used</span>
                  </div>
                  <span className="ledger-remaining-stat">{weeklyLeftPct}% left</span>
                </div>
                <div
                  className="ledger-track-bar"
                  role="progressbar"
                  aria-valuenow={weeklyPct}
                  aria-valuemin={0}
                  aria-valuemax={100}
                >
                  <div
                    className="ledger-fill-bar"
                    style={{ width: `${weeklyPct}%`, background: accentColor }}
                  />
                </div>
              </>
            )}
          </div>

          {/* ── Session section (only if snap provides it) ──────────── */}
          {sessionPct != null ? (
            <div className="ledger-section secondary">
              <div className="ledger-subhead-line">
                <span className="ledger-period-tag muted">5H WINDOW</span>
              </div>
              <div className="ledger-metric-line">
                <div className="ledger-session-usage">
                  <strong className="ledger-session-number">
                    {sessionPct}%
                  </strong>
                  {sessionResetText ? (
                    <span className="ledger-session-time">
                      {sessionResetText}
                    </span>
                  ) : null}
                </div>
              </div>
              <div
                className="ledger-track-bar dark"
                role="progressbar"
                aria-valuenow={sessionPct}
                aria-valuemin={0}
                aria-valuemax={100}
              >
                <div
                  className="ledger-fill-bar"
                  style={{
                    width: `${sessionPct}%`,
                    background: accentColor,
                  }}
                />
              </div>
            </div>
          ) : null}

          {/* ── Stats row (tokens, turns, cost) ─────────────────────── */}
          {tokens > 0 || turns > 0 ? (
            <div className="ledger-section secondary">
              <div className="ledger-subhead-line">
                <span className="ledger-period-tag muted">
                  {summary?.window_label?.toUpperCase() ?? "SESSION"}
                </span>
              </div>
              <div className="ledger-stats-row">
                <span className="ledger-stat">
                  <strong>{fmtTokens(tokens)}</strong> tokens
                </span>
                <span className="ledger-stat">
                  <strong>{turns}</strong> turns
                </span>
                {metered && costUsd > 0 ? (
                  <span className="ledger-stat">
                    <strong>{fmtCost(costUsd)}</strong>
                  </span>
                ) : null}
              </div>
            </div>
          ) : null}

          {/* ── Footer ──────────────────────────────────────────────── */}
          <div className="ledger-footer-row">
            <div className="ledger-account-block">
              {account?.plan ? (
                <span className="ledger-plan-pill">{account.plan.toUpperCase()}</span>
              ) : activeUsage?.balance_usd != null ? (
                <span className="ledger-plan-pill">
                  {fmtCost(activeUsage.balance_usd)}
                </span>
              ) : null}
              <span className="ledger-account-text">
                {account?.account_name
                  ? (masked ? maskAccount(account.account_name) : account.account_name)
                  : account?.status === "signed_out"
                    ? "Signed out"
                    : "Account not reported"}
              </span>
              {account?.account_name ? (
                <button
                  type="button"
                  className="ledger-icon-btn"
                  onClick={() => setMasked((value) => !value)}
                  title={masked ? "Show account" : "Hide account"}
                  aria-label={masked ? "Show provider account" : "Hide provider account"}
                >
                  <IconEye size={12} />
                </button>
              ) : null}
            </div>

            <div className="ledger-action-btns">
              <button
                type="button"
                className={`ledger-icon-btn${refreshing ? " is-spinning" : ""}`}
                onClick={handleManualRefresh}
                title="Reload usage data"
                aria-label="Reload usage"
              >
                <IconReload size={13} />
              </button>

              {onManage ? (
                <button
                  type="button"
                  className="ledger-icon-btn"
                  onClick={() => {
                    onOpenChange(false);
                    onManage();
                  }}
                  title="Full usage dashboard"
                  aria-label="Open usage dashboard"
                >
                  <IconExternalLink size={13} />
                </button>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
