// The one place a Computer Use turn is watched (ADR-0054).
//
// It used to be two: a full-screen always-on-top window painted over the desktop, and this
// panel — each drawing the same run, each with animations of its own. The overlay is gone,
// and what is left here shows four facts and nothing else: the frame Bhippi is looking at,
// what it is doing now, how far through its budget it is, and how to stop it.
//
// Nothing is animated that is not a state change. A synthetic cursor chasing the real one,
// a motion trail, a scan line and a vignette all described the same action the caption
// already names, and together they read as a demo of an agent rather than an agent.

import { useEffect, useMemo, useState } from "react";
import type { ChatTurnView, ScreenCapture, ToolActivity } from "../lib/ipc";
import { api } from "../lib/api";
import { IconChevronDown, IconMonitor, IconStop } from "./icons";

type BhippiComputerPanelProps = {
  tools: ToolActivity[];
  turnState: ChatTurnView["state"];
  fullAccess: boolean;
  liveLabel?: string | null;
  /** The per-turn action budget, from Rust. Zero hides the counter rather than inventing one. */
  maxActions?: number;
  /** Ends the turn. Absent when there is nothing left to stop. */
  onStop?: () => void;
};

/**
 * Rows that are not an executed action: a correction round, or a step the user declined.
 *
 * They belong in the list — a silent retry reads as a stall to somebody watching — but they
 * cost no action budget, so counting them would make the number lie.
 */
function isExecutedAction(tool: ToolActivity): boolean {
  return !/asked again|asked for one|is not available here|^Declined$/i.test(tool.title);
}

function isActiveTurn(state: ChatTurnView["state"]): boolean {
  return state === "queued" || state === "streaming" || state === "awaiting_permission";
}

/** What the panel says it is doing, in the model's own words where there are any. */
function statusLine(tool: ToolActivity | undefined, liveLabel?: string | null): string {
  const detail = tool?.detail?.trim();
  if (detail) return detail;
  if (tool?.title?.trim()) return tool.title.trim();
  if (liveLabel?.trim()) return liveLabel.trim();
  return "Looking at the screen";
}

export function BhippiComputerPanel({
  tools,
  turnState,
  fullAccess,
  liveLabel,
  maxActions = 0,
  onStop,
}: BhippiComputerPanelProps) {
  const active = isActiveTurn(turnState);
  const latestTool = tools[tools.length - 1];
  const usedActions = tools.filter(isExecutedAction).length;
  const revision = latestTool ? `${latestTool.id}:${latestTool.state}` : "starting";
  const [frame, setFrame] = useState<ScreenCapture | null>(null);
  const [frameError, setFrameError] = useState<string | null>(null);

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    void api
      .captureScreenPreview()
      .then((capture) => {
        if (disposed) return;
        setFrame(capture);
        setFrameError(null);
      })
      .catch(() => {
        if (!disposed) setFrameError("The desktop frame could not be refreshed.");
      });
    return () => {
      disposed = true;
    };
  }, [active, revision]);

  const earlier = useMemo(() => tools.slice(0, -1).reverse(), [tools]);

  const state =
    turnState === "failed"
      ? "blocked"
      : turnState === "stopped"
        ? "stopped"
        : active
          ? "working"
          : "done";
  const stateLabel = { working: "Working", blocked: "Blocked", stopped: "Stopped", done: "Done" }[
    state
  ];

  return (
    <section className={`computer-panel ${state}`} aria-label="Computer Use">
      <header className="computer-panel-head">
        <span className={`computer-panel-state ${state}`}>
          <span className="computer-state-dot" aria-hidden="true" />
          {stateLabel}
        </span>
        {maxActions > 0 ? (
          <span className="computer-panel-steps">
            {usedActions} of {maxActions} steps
          </span>
        ) : null}
        <span className="computer-panel-spacer" />
        {active && onStop ? (
          <button type="button" className="computer-panel-stop" onClick={onStop}>
            <IconStop size={11} />
            Stop
          </button>
        ) : null}
      </header>

      <div className="computer-panel-screen">
        {frame ? (
          <img
            className="computer-panel-frame"
            src={`data:image/jpeg;base64,${frame.image_base64}`}
            alt="The screen Bhippi is looking at"
          />
        ) : (
          <div className={`computer-panel-blank${frameError ? " error" : ""}`}>
            <IconMonitor size={20} />
            <span>{frameError ?? "Waiting for the first frame"}</span>
          </div>
        )}
      </div>

      <p className="computer-panel-status" aria-live="polite">
        {active ? statusLine(latestTool, liveLabel) : "Finished with the screen"}
      </p>

      <p className="computer-panel-footnote">
        {fullAccess ? "Full access" : "Observe only"}
        {active ? " · press Esc twice to stop" : null}
      </p>

      {earlier.length > 0 ? (
        <details className="computer-panel-history">
          <summary>
            <IconChevronDown size={11} />
            {earlier.length} earlier step{earlier.length === 1 ? "" : "s"}
          </summary>
          <ol className="computer-panel-steps-list">
            {earlier.map((tool) => (
              <li key={tool.id} className={tool.state}>
                <span className="computer-step-mark" aria-hidden="true" />
                <span className="computer-step-copy">{tool.detail?.trim() || tool.title}</span>
              </li>
            ))}
          </ol>
        </details>
      ) : null}
    </section>
  );
}
