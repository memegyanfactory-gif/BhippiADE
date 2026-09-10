import { useEffect, useRef, useState } from "react";
import type {
  LimitSnapshot,
  ModelUsage,
  ProviderInfo,
  ProviderUsage,
  SpendLimitView,
  UsageSummary,
} from "../lib/ipc";
import { ProviderLogo } from "./ProviderLogo";
// One dollar formatter for the whole app: a per-turn API cost is often a fraction of a
// cent, and a local rule that floored those to "$0.00" is what made the meter unreliable.
import { usd as fmtCost } from "../lib/format";
import { UsageRing, gaugeColor } from "./UsageRing";
import { IconCopy, IconCheck, IconExternalLink, IconEye, IconEyeOff, IconReload } from "./icons";

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

/** `grok-4.6` matches `grok-4.6-build` and `Grok 4.6`. */
export function normalizeModelKey(id: string): string {
  return id
    .toLowerCase()
    .replace(/\(1m\)/g, "")
    .replace(/-build\b/g, "")
    .replace(/-(low|medium|high)$/g, "")
    .replace(/\s+/g, "");
}

export function usageForSelectedModel(
  row: ProviderUsage | null,
  selectedModel: string | null | undefined,
): ModelUsage | null {
  if (!row || !selectedModel) return null;
  const key = normalizeModelKey(selectedModel);
  if (!key) return null;
  return (
    row.models.find((model) => {
      const other = normalizeModelKey(model.id) || normalizeModelKey(model.label);
      return other === key || other.startsWith(key) || key.startsWith(other);
    }) ?? null
  );
}

/** Format token count: 1234 → "1.2k", 1234567 → "1.2M" */
function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return `${n}`;
}

/** `just now`, `2 min ago` — so a refresh has a visible result. */
function relativePast(epochMs: number): string {
  const mins = Math.max(0, Math.round((Date.now() - epochMs) / 60000));
  if (mins <= 0) return "just now";
  if (mins < 60) return `${mins} min ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} hr ago`;
  return `${Math.floor(hrs / 24)}d ago`;
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
        {/* The one elastic item on the line. `right` can be a whole sentence
            (`2.0M of 2.0M tokens`), so the reset is what gives way — with its full text on
            hover, rather than a value broken across two lines. */}
        <span className="usage-limit-reset" title={reset}>
          {reset}
        </span>
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
  const selectedModelUsage = usageForSelectedModel(activeUsage, currentModel);
  // CLI history can list more tokens than Bhippi itself spent. The headline and the
  // local cap stay on this app's ledger; the selected-model row is used only when it
  // fits inside that ledger (this chat's grok-4.6, not every Grok session on disk).
  const ledgerModel =
    selectedModelUsage &&
    activeUsage &&
    selectedModelUsage.total_tokens <= activeUsage.total_tokens
      ? selectedModelUsage
      : null;
  const tokens = ledgerModel?.total_tokens ?? activeUsage?.total_tokens ?? 0;
  const turns = ledgerModel?.turns ?? activeUsage?.turns ?? 0;
  const costUsd = ledgerModel?.cost_usd ?? activeUsage?.cost_usd ?? 0;
  const inTokens = ledgerModel?.input_tokens ?? activeUsage?.input_tokens ?? 0;
  const outTokens = ledgerModel?.output_tokens ?? activeUsage?.output_tokens ?? 0;
  const prepaidUsd = account?.prepaid_usd ?? activeUsage?.balance_usd ?? null;
  const refreshedAt = account?.refreshed_at ? Date.parse(account.refreshed_at) : NaN;
  const refreshedLabel = Number.isFinite(refreshedAt) ? relativePast(refreshedAt) : null;

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
      prepaidUsd != null ? `Credits left: ${fmtCost(prepaidUsd)}` : null,
      `${summary?.window_label ?? "Session"}: ${fmtCost(costUsd)} · ${turns} turns · ${fmtTokens(tokens)} tokens (${fmtTokens(inTokens)} in / ${fmtTokens(outTokens)} out)`,
    ].filter(Boolean);
    void navigator.clipboard?.writeText(lines.join("\n")).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1400);
    });
  };

  // Two scopes, never one list. `ProviderUsage.total_tokens` is deliberately Bhippi's own
  // ledger so that machine-wide CLI spend cannot fill a local token cap, while `models` also
  // carries rows read out of the vendor CLI's session files — every Claude Code session on
  // the machine, not the turns this app sent. Adding them up draws a 148M model inside a 2M
  // day, which is what this drop-up used to do. So they are separated here and labelled
  // below; `from_cli_history` is the backend saying which is which, rather than the screen
  // guessing by comparing numbers.
  const byTokens = (a: ModelUsage, b: ModelUsage) => b.total_tokens - a.total_tokens;
  const allModels = activeUsage?.models ?? [];
  const ledgerModels = allModels
    .filter((model) => !model.from_cli_history)
    .slice()
    .sort(byTokens)
    .slice(0, 3);
  const historyModels = allModels
    .filter((model) => model.from_cli_history)
    .slice()
    .sort(byTokens)
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
              <ProviderLogo id={providerId} size={14} />
              <span>{providerLabel}</span>
            </span>
            <button
              type="button"
              className={`ledger-icon-btn${copied ? " copied" : ""}`}
              onClick={copySummary}
              title="Copy this summary"
              aria-label="Copy usage summary"
            >
              {copied ? <IconCheck size={14} /> : <IconCopy size={14} />}
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
                // The row already says "Token cap", so Rust's own `Cap resets at midnight`
                // says "cap" twice and is long enough to push the value onto a second line.
                // Same formatter as the two rows above it, so all three read alike.
                reset={
                  localCap.resets_at
                    ? `Resets ${fmtResetEpoch(localCap.resets_at)}`
                    : localCap.resets_label
                }
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
              {prepaidUsd != null ? (
                <span>
                  <small>Credits left</small>
                  <b>{fmtCost(prepaidUsd)}</b>
                </span>
              ) : null}
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
              {ledgerModels.length > 1
                ? ledgerModels.map((model) => (
                    <div key={model.id} className="usage-kv-sub">
                      <span title={model.id}>{model.label}</span>
                      <span>{fmtTokens(model.total_tokens)}</span>
                    </div>
                  ))
                : null}
            </div>
          </div>

          {/* ── The vendor CLI's own sessions ─────────────────────────
              A different scope from everything above, so it gets its own heading rather
              than another indented row. These figures are what the vendor's tool would
              report for the whole machine today; Bhippi's ledger above counts only the
              turns this app sent, which is why the number here can be much larger. */}
          {historyModels.length > 0 ? (
            <div className="usage-kv">
              <div className="usage-kv-head">
                <strong>All {providerLabel} sessions</strong>
                <span className="usage-kv-model">on this machine</span>
              </div>
              <div className="usage-kv-list">
                {historyModels.map((model) => (
                  <div key={model.id} className="usage-kv-sub">
                    <span title={model.id}>{model.label}</span>
                    <span>{fmtTokens(model.total_tokens)}</span>
                  </div>
                ))}
              </div>
            </div>
          ) : null}

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
              ) : prepaidUsd != null ? (
                <span className="ledger-plan-pill">{fmtCost(prepaidUsd)}</span>
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
                  {masked ? <IconEye size={14} /> : <IconEyeOff size={14} />}
                </button>
              ) : null}
            </div>

            <div className="ledger-action-btns">
              {refreshedLabel ? (
                <span className="ledger-refreshed" title={account?.note ?? undefined}>
                  {refreshing ? "Refreshing…" : refreshedLabel}
                </span>
              ) : null}
              <button
                type="button"
                className={`ledger-icon-btn${refreshing ? " is-spinning" : ""}`}
                onClick={handleManualRefresh}
                title="Reload usage and limits from the signed-in CLIs"
                aria-label="Reload usage"
              >
                <IconReload size={14} />
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
                  <IconExternalLink size={14} />
                </button>
              ) : null}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
