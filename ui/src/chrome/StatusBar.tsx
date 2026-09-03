import type { AppStatus, UsageSummary } from "../lib/ipc";
import { IconPower } from "../components/icons";

type StatusBarProps = {
  status: AppStatus | null;
  usage?: UsageSummary | null;
  error: string | null;
  /** What the engine is doing right now — moved here when the title bar slimmed down. */
  runningLabel: string | null;
  onManageUsage?: () => void;
  onKillRunning?: () => void;
};

export function StatusBar({ status, error, runningLabel, onKillRunning }: StatusBarProps) {
  return (
    <footer className="statusbar">
      {status ? (
        <>
          <span className="statusbar-provider">
            {status.active_provider.toLowerCase()}
            {status.demo_mode ? " · demo" : ""}
          </span>
        </>
      ) : (
        <span>connecting…</span>
      )}
      {runningLabel ? (
        <>
          <span className="statusbar-sep" aria-hidden="true" />
          <span className="statusbar-running">
            <span className="dot" aria-hidden="true" />
            running · {runningLabel}
          </span>
        </>
      ) : null}
      <span className="spacer" />
      {error ? (
        <span className="error-line" role="alert">
          {error}
        </span>
      ) : null}
      <button
        className="statusbar-kill"
        disabled={!runningLabel}
        onClick={onKillRunning}
        title={runningLabel ? `Stop running turn: ${runningLabel}` : "Stops active turn"}
      >
        <IconPower size={11} /> kill
      </button>
    </footer>
  );
}

