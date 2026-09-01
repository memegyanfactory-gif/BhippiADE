import { useState } from "react";
import type { TurnFault } from "../lib/ipc";
import {
  IconAlert,
  IconChevronDown,
  IconClock,
  IconDownload,
  IconKey,
  IconRefresh,
  IconShrink,
  IconSwap,
} from "./icons";

/**
 * A failed turn, explained, with the one button that fixes it.
 *
 * This replaces a red string. The difference is not cosmetic: "provider 01M0YN…
 * unavailable: the CLI answered with nothing" told the user nothing they could act on,
 * and the four failures hiding behind it — a full context window, a spent five-hour
 * window, a spent week, an empty balance — each need a *different* next step. So each
 * gets its own title, its own explanation, and its own button.
 */

/** Which remedy the engine chose, mapped to the button that performs it. */
const REMEDY_ICONS: Record<string, typeof IconRefresh> = {
  compact: IconShrink,
  update: IconDownload,
  switch_provider: IconSwap,
  sign_in: IconKey,
  retry: IconRefresh,
};

/** How severe a fault looks. Money and time read differently from a broken install. */
function toneOf(kind: string): "warn" | "error" | "info" {
  if (kind === "cancelled") return "info";
  if (
    kind === "context_exceeded" ||
    kind === "rate_limited_session" ||
    kind === "rate_limited_weekly" ||
    kind === "quota_exhausted" ||
    kind === "unauthenticated"
  ) {
    // Nothing is broken — a boundary was reached. Drawing these in alarm red trains
    // the user to ignore red, which costs them the one case that is genuinely broken.
    return "warn";
  }
  return "error";
}

export function FaultCard({
  fault,
  onAct,
  busy = false,
  status = null,
}: {
  fault: TurnFault;
  /** Performs the named remedy. `null` means the host offers no handler for it. */
  onAct?: ((remedy: string) => void) | null;
  busy?: boolean;
  status?: string | null;
}) {
  const [showDetail, setShowDetail] = useState(false);
  const tone = toneOf(fault.kind);
  const Icon = REMEDY_ICONS[fault.remedy] ?? IconRefresh;
  const canAct = Boolean(onAct) && fault.remedy !== "none" && Boolean(fault.action_label);

  return (
    <div className={`fault-card tone-${tone} m-rise`} role="alert">
      <div className="fault-head">
        <span className="fault-icon" aria-hidden="true">
          <IconAlert size={14} />
        </span>
        <div className="fault-headline">
          <strong className="fault-title">{fault.title}</strong>
          <span className="fault-provider">{fault.provider}</span>
        </div>
        {fault.resets_at ? (
          <span className="fault-reset" title="Reported by the provider">
            <IconClock size={11} />
            {fault.resets_at}
          </span>
        ) : null}
      </div>

      <p className="fault-summary">{fault.summary}</p>
      <p className="fault-fix">{fault.fix}</p>
      {status ? (
        <p className="fault-progress" role="status" aria-live="polite">
          {status}
        </p>
      ) : null}

      <div className="fault-actions">
        {canAct ? (
          <button
            type="button"
            className="fault-btn primary"
            onClick={() => onAct?.(fault.remedy)}
            disabled={busy}
          >
            <Icon size={13} />
            {busy ? "Working…" : fault.action_label}
          </button>
        ) : null}

        {fault.detail ? (
          <button
            type="button"
            className={`fault-btn ghost${showDetail ? " open" : ""}`}
            onClick={() => setShowDetail((open) => !open)}
            aria-expanded={showDetail}
          >
            <IconChevronDown size={12} />
            {showDetail ? "Hide details" : "What it said"}
          </button>
        ) : null}
      </div>

      {showDetail && fault.detail ? (
        // The vendor's own words, verbatim. An unrecognised failure is only
        // debuggable if what it actually said survives to the screen.
        <pre className="fault-detail m-fade">{fault.detail}</pre>
      ) : null}
    </div>
  );
}

/**
 * The pre-emptive version: how much of a plan window is gone, before it runs out.
 *
 * Shown only once a window is close enough to matter. A gauge that is visible at 5 %
 * is furniture; one that appears at 80 % is information.
 */
export function LimitBanner({
  status,
  sessionUsed,
  weeklyUsed,
  weeklyResetsAt,
  sessionResetsAt,
  provider,
}: {
  status: string;
  sessionUsed?: number | null;
  weeklyUsed?: number | null;
  weeklyResetsAt?: number | null;
  sessionResetsAt?: number | null;
  provider: string;
}) {
  const session = sessionUsed ?? 0;
  const weekly = weeklyUsed ?? 0;
  const worst = Math.max(session, weekly);
  if (status === "allowed" && worst < 0.8) return null;

  // The window nearer its edge is the one that will actually stop the next turn.
  const weeklyBinds = weekly >= session;
  const resetsAt = weeklyBinds ? weeklyResetsAt : sessionResetsAt;

  return (
    <div className={`limit-banner${worst >= 0.95 ? " critical" : ""} m-fall`} role="status">
      <span className="limit-label">
        {provider} · {weeklyBinds ? "weekly" : "session"} limit
      </span>
      <span className="limit-track" aria-hidden="true">
        <span className="limit-fill" style={{ transform: `scaleX(${Math.min(worst, 1)})` }} />
      </span>
      <span className="limit-value">{Math.round(worst * 100)}%</span>
      {resetsAt ? <span className="limit-reset">resets {formatReset(resetsAt)}</span> : null}
    </div>
  );
}

/** A reset time as a human distance, since the exact clock time is rarely the point. */
function formatReset(epochSeconds: number): string {
  const minutes = Math.round((epochSeconds * 1000 - Date.now()) / 60000);
  if (minutes <= 0) return "shortly";
  if (minutes < 60) return `in ${minutes}m`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `in ${hours}h`;
  return `in ${Math.round(hours / 24)}d`;
}
