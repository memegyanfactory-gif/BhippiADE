import { useEffect, useMemo, useRef, useState } from "react";
import type { ChatTurnView, ScreenCapture, ToolActivity } from "../lib/ipc";
import { api } from "../lib/api";
import { IconCheck, IconChevronDown, IconMonitor } from "./icons";

type CursorPoint = { x: number; y: number };
type TrailSpark = CursorPoint & { id: number; angle: number; delay: number };

type BhippiComputerPanelProps = {
  tools: ToolActivity[];
  turnState: ChatTurnView["state"];
  fullAccess: boolean;
  liveLabel?: string | null;
};

function isActiveTurn(state: ChatTurnView["state"]): boolean {
  return state === "queued" || state === "streaming" || state === "awaiting_permission";
}

function coordinateFromTitle(title: string): { x: number; y: number } | null {
  if (!/^(Move pointer|.+ click|Drag )/.test(title)) return null;
  const matches = Array.from(title.matchAll(/\((-?\d+),\s*(-?\d+)\)/g));
  const match = matches[matches.length - 1];
  if (!match) return null;
  const x = Number(match[1]);
  const y = Number(match[2]);
  return Number.isFinite(x) && Number.isFinite(y) ? { x, y } : null;
}

function pointInFrame(
  coordinate: { x: number; y: number },
  frame: ScreenCapture,
): CursorPoint {
  const x = ((coordinate.x - frame.origin_x) / frame.width) * 100;
  const y = ((coordinate.y - frame.origin_y) / frame.height) * 100;
  return {
    x: Math.max(0, Math.min(100, x)),
    y: Math.max(0, Math.min(100, y)),
  };
}

function shortStatus(tool: ToolActivity | undefined, liveLabel?: string | null): string {
  if (tool?.title) return tool.title;
  if (liveLabel?.trim()) return liveLabel.trim();
  return "Reading the latest desktop frame";
}

export function BhippiComputerPanel({
  tools,
  turnState,
  fullAccess,
  liveLabel,
}: BhippiComputerPanelProps) {
  const active = isActiveTurn(turnState);
  const latestTool = tools[tools.length - 1];
  const revision = latestTool ? `${latestTool.id}:${latestTool.state}` : "starting";
  const [frame, setFrame] = useState<ScreenCapture | null>(null);
  const [frameError, setFrameError] = useState<string | null>(null);
  const [loadingFrame, setLoadingFrame] = useState(active);
  const [cursor, setCursor] = useState<CursorPoint>({ x: 50, y: 50 });
  const [trail, setTrail] = useState<TrailSpark[]>([]);
  const trailId = useRef(0);
  const previousCursor = useRef<CursorPoint>({ x: 50, y: 50 });

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    setLoadingFrame(true);
    void api
      .captureScreenPreview()
      .then((capture) => {
        if (disposed) return;
        setFrame(capture);
        setFrameError(null);
      })
      .catch(() => {
        if (!disposed) setFrameError("The live desktop frame could not be refreshed.");
      })
      .finally(() => {
        if (!disposed) setLoadingFrame(false);
      });
    return () => {
      disposed = true;
    };
  }, [active, revision]);

  useEffect(() => {
    if (!frame || !latestTool) return;
    const coordinate = coordinateFromTitle(latestTool.title);
    if (!coordinate) return;
    const next = pointInFrame(coordinate, frame);
    const previous = previousCursor.current;
    const dx = next.x - previous.x;
    const dy = next.y - previous.y;
    if (Math.abs(dx) < 0.1 && Math.abs(dy) < 0.1) return;

    const angle = Math.atan2(dy, dx) * (180 / Math.PI);
    const sparks = Array.from({ length: 8 }, (_, index) => {
      const progress = (index + 1) / 9;
      trailId.current += 1;
      return {
        id: trailId.current,
        x: previous.x + dx * progress,
        y: previous.y + dy * progress,
        angle,
        delay: index * 24,
      };
    });
    setTrail((current) => [...current.slice(-16), ...sparks]);
    setCursor(next);
    previousCursor.current = next;
    const ids = new Set(sparks.map((spark) => spark.id));
    window.setTimeout(
      () => setTrail((current) => current.filter((spark) => !ids.has(spark.id))),
      1100,
    );
  }, [frame, latestTool]);

  const earlierTools = useMemo(() => tools.slice(0, -1).reverse(), [tools]);
  const status =
    turnState === "failed"
      ? "Blocked"
      : turnState === "stopped"
        ? "Stopped"
        : active
          ? "Live"
          : "Completed";

  return (
    <section className={`bhippi-computer-panel ${status.toLowerCase()}`} aria-label="Bhippi Computer">
      <header className="bhippi-computer-head">
        <div className="bhippi-computer-identity">
          <span className="bhippi-computer-mark"><IconMonitor size={14} /></span>
          <strong>Bhippi Computer</strong>
          <span className={`bhippi-computer-access${fullAccess ? " full" : ""}`}>
            {fullAccess ? "Full access" : "Observe only"}
          </span>
          <span className={`bhippi-computer-state ${status.toLowerCase()}`}>
            <span className="bhippi-state-dot" />{status}
          </span>
        </div>
        <span className="bhippi-computer-frame-meta">
          {frame ? `${frame.width} × ${frame.height}` : "Desktop stream"}
        </span>
      </header>

      <div className="bhippi-computer-screen">
        {frame ? (
          <img
            className="bhippi-computer-frame"
            src={`data:image/jpeg;base64,${frame.image_base64}`}
            alt="Latest desktop frame seen by Bhippi Computer"
          />
        ) : (
          <div className={`bhippi-computer-frame-state${frameError ? " error" : ""}`}>
            <IconMonitor size={24} />
            <strong>{frameError ? "Frame unavailable" : "Connecting to the desktop"}</strong>
            <span>{frameError ?? "The first live frame will appear here."}</span>
          </div>
        )}

        <div className="bhippi-screen-vignette" aria-hidden="true" />
        <div className={`bhippi-screen-scan${active ? " active" : ""}`} aria-hidden="true" />
        {loadingFrame ? <span className="bhippi-frame-refresh">Refreshing frame</span> : null}

        {trail.map((spark) => (
          <span
            key={spark.id}
            className="bhippi-cursor-spark"
            style={{
              left: `${spark.x}%`,
              top: `${spark.y}%`,
              transform: `rotate(${spark.angle}deg)`,
              animationDelay: `${spark.delay}ms`,
            }}
            aria-hidden="true"
          />
        ))}

        <div
          className={`bhippi-virtual-cursor${active ? " active" : ""}`}
          style={{ left: `${cursor.x}%`, top: `${cursor.y}%` }}
          aria-hidden="true"
        >
          <span className="bhippi-cursor-name">Bhippi</span>
          <svg viewBox="0 0 28 34" role="presentation">
            <path d="M3 2.5 24 21l-10.1 1.1L8.3 31.5 3 2.5Z" />
          </svg>
          <span className="bhippi-cursor-glow" />
        </div>

        <div className="bhippi-live-caption" aria-live="polite">
          <span className="bhippi-live-kicker">{active ? "Live action" : status}</span>
          <strong>{active ? shortStatus(latestTool, liveLabel) : "Desktop actions finished"}</strong>
          <span>{latestTool?.detail || (active ? "Watching for the next verified action." : "The last live frame is retained above.")}</span>
        </div>
      </div>

      <div className="bhippi-computer-latest">
        <span className={`bhippi-latest-icon ${latestTool?.state ?? "ok"}`}>
          {latestTool?.state === "ok" || !active ? <IconCheck size={12} /> : <IconMonitor size={12} />}
        </span>
        <span className="bhippi-latest-copy">
          <strong>{shortStatus(latestTool, liveLabel)}</strong>
          <small>{latestTool?.detail || "Bhippi Computer is connected to the desktop."}</small>
        </span>
      </div>

      {earlierTools.length > 0 ? (
        <details className="bhippi-computer-history">
          <summary>
            <IconChevronDown size={12} />
            Show {earlierTools.length} earlier action{earlierTools.length === 1 ? "" : "s"}
          </summary>
          <div className="bhippi-history-list">
            {earlierTools.map((tool) => (
              <div key={tool.id} className={`bhippi-history-row ${tool.state}`}>
                <span>{tool.state === "ok" ? "✓" : tool.state === "failed" ? "×" : "•"}</span>
                <strong>{tool.title}</strong>
                <small>{tool.detail}</small>
              </div>
            ))}
          </div>
        </details>
      ) : null}
    </section>
  );
}
