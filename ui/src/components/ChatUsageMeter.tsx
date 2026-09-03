import { useEffect, useRef, useState } from "react";
import type { LimitSnapshot, ProviderInfo, SpendLimitView, UsageSummary } from "../lib/ipc";
import { ProviderLogo } from "./ProviderLogo";
// One dollar formatter for the whole app: a per-turn API cost is often a fraction of a
// cent, and a local rule that floored those to "$0.00" is what made the meter unreliable.
import { usd as fmtCost } from "../lib/format";
import { UsageRing, gaugeColor } from "./UsageRing";
import { IconCopy, IconCheck, IconExternalLink, IconEye, IconReload } from "./icons";

/* ── helpers ────────────────────────────────────────────────────────────── */

/** `Resets in 1 hr 59 min`, `Resets Tue 2:30 AM` — the reference's own phrasing. */
function fmtResetEpoch(epoch: number): string {
  const diffMs = epoch * 1000 - Date.now();
  const mins = Math.round(diffMs / 60000);
  if (mins <= 0) return "shortly";
  if (mins < 60) return `in ${mins} min`;
  const hrs = Math.floor(mins / 60);
  const rem = mins % 60;
  if (hrs < 24) return rem > 0 ? `in ${hrs} hr ${rem} min` : `in ${hrs} hr`;

  const d = new Date(epoch * 1000);
  const day = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][d.getDay()];
  const h = d.getHours();
  const m = d.getMinutes();
  const ampm = h >= 12 ? "PM" : "AM";
  const h12 = h % 12 || 12;
  const mm = m.toString().padStart(2, "0");
  return `${day} ${h12}:${mm} ${ampm}`;
}

/** Format token count: 1234 → "1.2k", 1234567 → "1.2M" */
function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return `${n}`;
}

function maskAccount(value: string): string {
  const at = value.indexOf("@");
  if (at <= 1) return value.length <= 4 ? "••••" : `${value.slice(0, 2)}••••`;
  return `${value.slice(0, 2)}••••${value.slice(at)}`;
}

/**
 * What the ring in the strip measures (SPA-002): the vendor's weekly allowance when it
 * reports one, else its short window, else Bhippi's own cap for the active provider.
 * Nothing capped and nothing reported leaves the ring an empty track — the honest face.
 */
export function ringReading(
  weeklyFraction: number | null,
  sessionFraction: number | null,
  localFraction: number | null,
): { fraction: number; capped: boolean; source: "weekly" | "session" | "local" | "none" } {
  if (weeklyFraction != null) return { fraction: weeklyFraction, capped: true, source: "weekly" };
  if (sessionFraction != null) return { fraction: sessionFraction, capped: true, source: "session" };
  if (localFraction != null) return { fraction: localFraction, capped: true, source: "local" };
  return { fraction: 0, capped: false, source: "none" };
}

/** One allowance line: label · reset · percentage, with the bar under it. */
function LimitRow({
  label,
  reset,
  pct,
  right,
}: {
  label: string;
  reset: string;
  pct: number;
  /** Overrides the percentage on the right, e.g. `$3.20 of $10.00`. */
  right?: string;
}) {
  const clamped = Math.max(0, Math.min(100, pct));
  const full = clamped >= 100;
  return (
    <div className={`usage-limit-row${full ? " full" : ""}`}>
      <div className="usage-limit-line">
        <span className="usage-limit-label">{label}</span>
        <span className="usage-limit-reset">{reset}</span>
        <strong className="usage-limit-pct">{right ?? `${Math.round(clamped)}%`}</strong>
      </div>
      <div
        className="usage-limit-track"
        role="progressbar"
        aria-valuenow={Math.round(clamped)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={label}
      >
        <div
          className="usage-limit-fill"
          style={{
            width: `${Math.max(2, clamped)}%`,
            background: full ? "var(--gauge-3)" : gaugeColor(clamped / 100),
          }}
        />
      </div>
    </div>
  );
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
  const [copied, setCopied] = useState(false);

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

  /* ── the composer's provider, its row, its vendor snapshot ─────────── */

  const providerId = provider?.id ?? summary?.active_provider_id ?? "";
  const providerLabel = provider?.label ?? summary?.active?.label ?? providerId;

  const activeUsage =
    summary?.providers.find((p) => p.id.toLowerCase() === providerId.toLowerCase()) ??
    (summary?.active?.id?.toLowerCase() === providerId.toLowerCase() ? summary.active : null) ??
    null;

  const matchesLimits =
    limits && providerId && limits.provider.toLowerCase() === providerId.toLowerCase();
  const snap: LimitSnapshot | null = matchesLimits ? limits.snapshot : null;
  const account = activeUsage?.account ?? null;

  // A local Bhippi token cap is not a vendor subscription allowance. Unknown stays
  // unknown instead of becoming a precise-looking 0% or local-midnight reset.
  const weeklyFraction = snap?.weekly_used ?? account?.weekly?.used_fraction ?? null;
  const weeklyPct = weeklyFraction == null ? null : Math.round(weeklyFraction * 100);
  const weeklyResetAt = snap?.weekly_resets_at ?? account?.weekly?.resets_at ?? null;

  const sessionFraction = snap?.session_used ?? account?.session?.used_fraction ?? null;
  const sessionPct = sessionFraction == null ? null : Math.round(sessionFraction * 100);
  const sessionResetAt = snap?.session_resets_at ?? account?.session?.resets_at ?? null;

  const modelName = currentModel ?? provider?.models[0] ?? activeUsage?.models?.[0]?.label ?? "";
  const tokens = activeUsage?.total_tokens ?? 0;
  const turns = activeUsage?.turns ?? 0;
  const costUsd = activeUsage?.cost_usd ?? 0;
  const inTokens = activeUsage?.input_tokens ?? 0;
  const outTokens = activeUsage?.output_tokens ?? 0;

  /* ── the ring (SPA-002) and this provider's nearest ceiling (SPA-003) ── */

  const localFraction =
    activeUsage && activeUsage.limit_tokens !== null ? activeUsage.fraction : null;
  const ring = ringReading(weeklyFraction, sessionFraction, localFraction);
  // The row's own ceiling — Claude's spent week is not OpenCode's problem.
  const spendLimit: SpendLimitView | null = activeUsage?.spend_limit ?? null;
  const reached = Boolean(spendLimit?.reached);
  const localCap = spendLimit && spendLimit.can_raise ? spendLimit : null;
  const ringPct = Math.round(ring.fraction * 100);
  const ringSource =
    ring.source === "weekly"
      ? "weekly allowance"
      : ring.source === "session"
        ? "5-hour limit"
        : ring.source === "local"
          ? "cap"
          : null;
  const ringTitle = reached
    ? `${spendLimit?.headline ?? "Limit reached"} · ${spendLimit?.resets_label ?? ""}`
    : ringSource
      ? `${100 - ringPct}% of the ${ringSource} left · ${fmtTokens(tokens)} tokens · ${fmtCost(costUsd)}`
      : `${fmtTokens(tokens)} tokens · ${fmtCost(costUsd)} · no allowance reported`;

  const copySummary = () => {
    const lines = [
      `${providerLabel} · ${modelName || "default model"}`,
      sessionPct != null ? `5-hour limit: ${sessionPct}%${sessionResetAt ? ` (resets ${fmtResetEpoch(sessionResetAt)})` : ""}` : null,
      weeklyPct != null ? `Weekly: ${weeklyPct}%${weeklyResetAt ? ` (resets ${fmtResetEpoch(weeklyResetAt)})` : ""}` : null,
      localCap ? `${localCap.headline}: ${localCap.used_label} · ${localCap.resets_label}` : null,
      `${summary?.window_label ?? "Session"}: ${fmtCost(costUsd)} · ${turns} turns · ${fmtTokens(tokens)} tokens (${fmtTokens(inTokens)} in / ${fmtTokens(outTokens)} out)`,
    ].filter(Boolean);
    void navigator.clipboard?.writeText(lines.join("\n")).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    });
  };

  const topModels = (activeUsage?.models ?? [])
    .slice()
    .sort((a, b) => b.total_tokens - a.total_tokens)
    .slice(0, 3);

  /* ── render ────────────────────────────────────────────────────────── */

  return (
    <div className="composer-popover-anchor" ref={wrapRef}>
      <button
        type="button"
        className={`composer-bar-btn ledger-trigger ring-trigger${open ? " active" : ""}${
          reached ? " reached" : ""
        }`}
        onClick={() => {
          const next = !open;
          onOpenChange(next);
          if (next && onRefresh) handleManualRefresh();
        }}
        title={ringTitle}
        aria-label="Token usage and limits"
        aria-haspopup="dialog"
        aria-expanded={open}
      >
        <UsageRing fraction={ring.fraction} capped={ring.capped} size={16} thickness={2} />
      </button>

      {open ? (
        <div
          className="bhippi-popover ledger-popover compact"
          role="dialog"
          aria-label="Token usage and limits"
        >
          {/* ── Header: Usage · provider chip · copy ─────────────────── */}
          <div className="usage-head-line">
            <span className="usage-head-title">Usage</span>
            <span className="usage-head-provider" title={modelName || providerLabel}>
              <ProviderLogo id={providerId} size={13} />
              <span>{providerLabel}</span>
            </span>
            <button
              type="button"
              className={`ledger-icon-btn${copied ? " copied" : ""}`}
              onClick={copySummary}
              title="Copy this summary"
              aria-label="Copy usage summary"
            >
              {copied ? <IconCheck size={12} /> : <IconCopy size={12} />}
            </button>
          </div>

          {/* ── Allowances: the vendor's windows, then Bhippi's own cap ── */}
          <div className="usage-rows">
            {sessionPct != null ? (
              <LimitRow
                label="5-hour limit"
                reset={sessionResetAt ? `Resets ${fmtResetEpoch(sessionResetAt)}` : ""}
                pct={sessionPct}
              />
            ) : null}
            {weeklyPct != null ? (
              <LimitRow
                label="Weekly · all models"
                reset={weeklyResetAt ? `Resets ${fmtResetEpoch(weeklyResetAt)}` : ""}
                pct={weeklyPct}
              />
            ) : null}
            {localCap ? (
              <LimitRow
                label={localCap.headline.replace(/ reached$/, "")}
                reset={localCap.resets_label}
                pct={localCap.used_fraction * 100}
                right={localCap.used_label}
              />
            ) : null}
            {sessionPct == null && weeklyPct == null && !localCap ? (
              <div className="ledger-unreported" role="status">
                <strong>No allowance reported</strong>
                <span>{account?.note ?? `${providerLabel} does not expose a plan allowance.`}</span>
              </div>
            ) : null}
          </div>

          {/* ── This window ─────────────────────────────────────────── */}
          <div className="usage-kv">
            <div className="usage-kv-head">
              <strong>{summary?.window_label ?? "This session"}</strong>
            </div>
            <div className="usage-kv-cells">
              <span>
                <small>Cost</small>
                <b>{fmtCost(costUsd)}</b>
              </span>
              <span>
                <small>Turns</small>
                <b>{turns}</b>
              </span>
              <span>
                <small>Tokens</small>
                <b>{fmtTokens(tokens)}</b>
              </span>
            </div>
          </div>

          {/* ── Breakdown ───────────────────────────────────────────── */}
          <div className="usage-kv">
            <div className="usage-kv-head">
              <strong>Breakdown</strong>
              <span className="usage-kv-model" title={modelName}>
                {modelName || providerLabel}
              </span>
            </div>
            <div className="usage-kv-list">
              <div>
                <span>Input</span>
                <span>{fmtTokens(inTokens)}</span>
              </div>
              <div>
                <span>Output</span>
                <span>{fmtTokens(outTokens)}</span>
              </div>
              {topModels.length > 1
                ? topModels.map((model) => (
                    <div key={model.id} className="usage-kv-sub">
                      <span title={model.id}>{model.label}</span>
                      <span>{fmtTokens(model.total_tokens)}</span>
                    </div>
                  ))
                : null}
            </div>
          </div>

          {reached && localCap && onManage ? (
            <button
              type="button"
              className="ledger-limit-action"
              onClick={() => {
                onOpenChange(false);
                onManage();
              }}
            >
              Increase spend limit
            </button>
          ) : null}

          {/* ── Footer ──────────────────────────────────────────────── */}
          <div className="ledger-footer-row">
            <div className="ledger-account-block">
              {account?.plan ? (
                <span className="ledger-plan-pill">{account.plan.toUpperCase()}</span>
              ) : activeUsage?.balance_usd != null ? (
                <span className="ledger-plan-pill">{fmtCost(activeUsage.balance_usd)}</span>
              ) : null}
              <span className="ledger-account-text">
                {account?.account_name
                  ? masked
                    ? maskAccount(account.account_name)
                    : account.account_name
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
