import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { WorkspaceSession } from "../lib/ipc";
import {
  IconChat,
  IconChevronLeft,
  IconChevronRight,
  IconClose,
  IconGrid,
  IconGripVertical,
  IconMaximize2,
  IconMinimize2,
  IconTerminal,
} from "../components/icons";
import { ProviderLogo } from "../components/ProviderLogo";

import type { WorkspaceLayout } from "./WorkspaceOrganizer";
import { reconcileSessionOrder } from "./workspaceState";
export type { WorkspaceLayout };

const MIN_PANEL_WIDTH = 300;
const MAX_PANEL_FRACTION = 0.85;

function readBoolean(key: string, fallback: boolean): boolean {
  try {
    const value = window.localStorage.getItem(key);
    return value === null ? fallback : value === "true";
  } catch {
    return fallback;
  }
}

function readLayout(key: string): WorkspaceLayout {
  try {
    const value = window.localStorage.getItem(key);
    return value === "adaptive" || value === "smart" ? value : "balanced";
  } catch {
    return "balanced";
  }
}

function readSizes(key: string): Record<string, number> {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(key) ?? "{}");
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    return Object.fromEntries(
      Object.entries(value).filter(
        (entry): entry is [string, number] =>
          typeof entry[1] === "number" && Number.isFinite(entry[1]),
      ),
    );
  } catch {
    return {};
  }
}

function readOrder(key: string): string[] {
  try {
    const saved = window.localStorage.getItem(key);
    if (!saved) return [];
    const parsed = JSON.parse(saved);
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}

function defaultBasis(layout: WorkspaceLayout, index: number, count: number): number {
  if (count <= 1) return 900;
  if (layout === "smart" && index === 0) return 640;
  if (layout === "adaptive" && index === 0) return 560;
  return count === 2 ? 540 : 380;
}

function computePanelFlex(
  layout: WorkspaceLayout,
  index: number,
  count: number,
  autoFit: boolean,
  customSize?: number,
  isPrimary = false,
): CSSProperties {
  if (customSize && !autoFit) {
    return {
      flex: `0 0 ${customSize}px`,
      width: `${customSize}px`,
      minWidth: `${MIN_PANEL_WIDTH}px`,
    };
  }

  if (autoFit) {
    if (count <= 1) {
      return { flex: "1 1 100%", width: "100%" };
    }
    if (layout === "smart") {
      const weight = isPrimary ? 1.4 : 0.9;
      return {
        flex: `${weight} 1 0px`,
        minWidth: isPrimary ? "340px" : "260px",
      };
    }
    // Balanced and Adaptive: completely stable equal distribution so clicking a panel never shifts or resizes windows
    return {
      flex: "1 1 0px",
      minWidth: "280px",
    };
  }

  const basis = defaultBasis(layout, index, count);
  return {
    flex: `1 1 ${basis}px`,
    width: `${basis}px`,
    minWidth: `${MIN_PANEL_WIDTH}px`,
  };
}

function statusText(status: WorkspaceSession["status"]): string {
  return status.charAt(0).toUpperCase() + status.slice(1);
}

type MultiSessionWorkspaceProps = {
  projectPath: string;
  sessions: WorkspaceSession[] | null;
  sessionsError: string | null;
  activeSessionId: string | null;
  renderSession: (session: WorkspaceSession) => ReactNode;
  onActivate: (sessionId: string) => void;
  onFocusSingle: (sessionId: string) => void;
  onCloseSession?: (sessionId: string) => void;
  onNewChat: () => void;
  onNewCli: () => void;
  onRetry: () => void;
  layout?: WorkspaceLayout;
  autoFit?: boolean;
  resetKey?: number;
  onAutoFitChange?: (fit: boolean) => void;
};

export function MultiSessionWorkspace({
  projectPath,
  sessions,
  sessionsError,
  activeSessionId,
  renderSession,
  onActivate,
  onFocusSingle,
  onCloseSession,
  onNewChat,
  onNewCli,
  onRetry,
  layout: propLayout,
  autoFit: propAutoFit,
  resetKey,
  onAutoFitChange,
}: MultiSessionWorkspaceProps) {
  const storagePrefix = `bhippi-multi-workspace:${projectPath}`;
  const [internalAutoFit, setInternalAutoFit] = useState(() => readBoolean(`${storagePrefix}:auto-fit`, true));
  const [internalLayout] = useState<WorkspaceLayout>(() => readLayout(`${storagePrefix}:layout`));
  const autoFit = propAutoFit ?? internalAutoFit;
  const layout = propLayout ?? internalLayout;

  const setAutoFit = (fit: boolean) => {
    setInternalAutoFit(fit);
    onAutoFitChange?.(fit);
  };
  const [sizes, setSizes] = useState<Record<string, number>>(() =>
    readSizes(`${storagePrefix}:sizes`),
  );
  const [panelOrder, setPanelOrder] = useState<string[]>(() => readOrder(`${storagePrefix}:order`));
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [draggedPanelId, setDraggedPanelId] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const draggedPanelIdRef = useRef<string | null>(null);

  const canvasRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<{ id: string; startX: number; startBasis: number } | null>(null);

  useEffect(() => {
    const onGlobalDragEnd = () => {
      draggedPanelIdRef.current = null;
      setDraggedPanelId(null);
      setDropTarget(null);
    };
    window.addEventListener("dragend", onGlobalDragEnd);
    return () => window.removeEventListener("dragend", onGlobalDragEnd);
  }, []);

  useEffect(() => {
    if (resetKey !== undefined) {
      setSizes({});
    }
  }, [resetKey]);

  const orderedSessions = useMemo(() => {
    if (!sessions) return [];
    const map = new Map(sessions.map((s) => [s.id, s]));
    const result: WorkspaceSession[] = [];

    // Honor saved user-dragged order first
    for (const id of panelOrder) {
      const s = map.get(id);
      if (s) {
        result.push(s);
        map.delete(id);
      }
    }

    // New sessions appear once at the end; later activity cannot move them.
    return [...result, ...map.values()];
  }, [sessions, panelOrder]);

  useEffect(() => {
    window.localStorage.setItem(`${storagePrefix}:auto-fit`, String(autoFit));
  }, [autoFit, storagePrefix]);

  useEffect(() => {
    window.localStorage.setItem(`${storagePrefix}:layout`, layout);
  }, [layout, storagePrefix]);

  useEffect(() => {
    window.localStorage.setItem(`${storagePrefix}:sizes`, JSON.stringify(sizes));
  }, [sizes, storagePrefix]);

  useEffect(() => {
    if (panelOrder.length > 0) {
      window.localStorage.setItem(`${storagePrefix}:order`, JSON.stringify(panelOrder));
    } else {
      window.localStorage.removeItem(`${storagePrefix}:order`);
    }
  }, [panelOrder, storagePrefix]);

  // The backend returns newest activity first. Capture the initial visual order and
  // reconcile only real additions/removals so sending in one chat never moves panels.
  useEffect(() => {
    if (!sessions) return;
    setPanelOrder((current) =>
      reconcileSessionOrder(current, sessions.map((session) => session.id)),
    );
  }, [sessions]);

  useEffect(() => {
    if (!autoFit) return;
    setSizes({});
  }, [autoFit, layout, orderedSessions.length]);

  useEffect(() => {
    const onPointerMove = (event: PointerEvent) => {
      const drag = dragRef.current;
      const canvasWidth = canvasRef.current?.getBoundingClientRect().width ?? 0;
      if (!drag || canvasWidth <= 0) return;
      const otherPanelsMin = Math.max(0, (orderedSessions.length - 1) * MIN_PANEL_WIDTH);
      const max = Math.max(
        MIN_PANEL_WIDTH,
        Math.min(Math.round(canvasWidth * MAX_PANEL_FRACTION), canvasWidth - otherPanelsMin - 24),
      );
      const next = Math.min(max, Math.max(MIN_PANEL_WIDTH, drag.startBasis + event.clientX - drag.startX));
      setSizes((current) => ({ ...current, [drag.id]: next }));
    };
    const onPointerUp = () => {
      dragRef.current = null;
      setDraggingId(null);
    };
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    };
  }, [orderedSessions.length]);

  const resizePanel = (sessionId: string, delta: number) => {
    const canvasWidth = canvasRef.current?.getBoundingClientRect().width ?? 0;
    const index = orderedSessions.findIndex((session) => session.id === sessionId);
    const basis = sizes[sessionId] ?? defaultBasis(layout, index, orderedSessions.length);
    const otherPanelsMin = Math.max(0, (orderedSessions.length - 1) * MIN_PANEL_WIDTH);
    const max = Math.max(
      MIN_PANEL_WIDTH,
      Math.min(Math.round(canvasWidth * MAX_PANEL_FRACTION), canvasWidth - otherPanelsMin - 24),
    );
    setAutoFit(false);
    setSizes((current) => ({
      ...current,
      [sessionId]: Math.min(max, Math.max(MIN_PANEL_WIDTH, basis + delta)),
    }));
  };

  const handleSwitchPanels = (dragId: string, targetId: string) => {
    if (dragId === targetId) return;
    const currentOrder = orderedSessions.map((s) => s.id);
    const dragIdx = currentOrder.indexOf(dragId);
    const targetIdx = currentOrder.indexOf(targetId);
    if (dragIdx === -1 || targetIdx === -1) return;
    const nextOrder = [...currentOrder];
    const temp = nextOrder[dragIdx];
    nextOrder[dragIdx] = nextOrder[targetIdx];
    nextOrder[targetIdx] = temp;
    setPanelOrder(nextOrder);
    onActivate(dragId);
  };

  const swapWithNeighbor = (sessionId: string, direction: "left" | "right") => {
    const currentOrder = orderedSessions.map((s) => s.id);
    const idx = currentOrder.indexOf(sessionId);
    if (idx === -1) return;
    const targetIdx = direction === "left" ? idx - 1 : idx + 1;
    if (targetIdx < 0 || targetIdx >= currentOrder.length) return;
    const nextOrder = [...currentOrder];
    const temp = nextOrder[idx];
    nextOrder[idx] = nextOrder[targetIdx];
    nextOrder[targetIdx] = temp;
    setPanelOrder(nextOrder);
  };

  return (
    <section className="multi-session-workspace" aria-label="Multi-session workspace">
      {sessionsError ? (
        <div className="multi-workspace-state error" role="alert">
          <strong>Sessions could not be loaded.</strong>
          <span>{sessionsError}</span>
          <button type="button" onClick={onRetry}>
            Retry
          </button>
        </div>
      ) : sessions === null ? (
        <div className="multi-workspace-state loading" role="status">
          <span className="multi-loading-line" />
          <strong>Loading project sessions…</strong>
        </div>
      ) : orderedSessions.length === 0 ? (
        <div className="multi-workspace-state empty">
          <IconGrid size={22} />
          <strong>No sessions in this project yet</strong>
          <span>Start a chat or terminal and it will join this workspace.</span>
          <div>
            <button type="button" onClick={onNewChat}>
              <IconChat size={13} /> New chat
            </button>
            <button type="button" onClick={onNewCli}>
              <IconTerminal size={13} /> New CLI
            </button>
          </div>
        </div>
      ) : (
        <div
          ref={canvasRef}
          className={`multi-workspace-canvas layout-${layout}${draggingId ? " resizing" : ""}${
            draggedPanelId ? " dragging-panel" : ""
          }`}
          data-count={orderedSessions.length}
          onDragOver={(e) => {
            if (draggedPanelIdRef.current || draggedPanelId) {
              e.preventDefault();
              e.dataTransfer.dropEffect = "move";
            }
          }}
          onDrop={(e) => {
            const dragId = draggedPanelIdRef.current || draggedPanelId || e.dataTransfer.getData("text/plain");
            if (dragId && e.target === canvasRef.current) {
              e.preventDefault();
              const currentOrder = orderedSessions.map((s) => s.id);
              const withoutDrag = currentOrder.filter((id) => id !== dragId);
              setPanelOrder([...withoutDrag, dragId]);
              draggedPanelIdRef.current = null;
              setDraggedPanelId(null);
              setDropTarget(null);
            }
          }}
        >
          {orderedSessions.map((session, index) => {
            const isActive = session.id === activeSessionId;
            const isSmartPrimary = layout === "smart" && index === 0;
            const panelStyle = computePanelFlex(
              layout,
              index,
              orderedSessions.length,
              autoFit,
              sizes[session.id],
              isSmartPrimary,
            );
            const isCli = session.kind === "cli";
            const isBeingDragged = (draggedPanelIdRef.current ?? draggedPanelId) === session.id;
            const isTarget = dropTarget === session.id;

            return (
              <article
                key={session.id}
                className={`session-panel${isActive ? " active" : ""}${
                  draggingId === session.id ? " resizing" : ""
                }${isBeingDragged ? " is-dragging" : ""}${
                  isTarget ? " drop-target-switch drop-target" : ""
                }${isSmartPrimary ? " smart-primary" : ""}`}
                style={panelStyle}
                onPointerDown={() => onActivate(session.id)}
                onDragOver={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  e.dataTransfer.dropEffect = "move";
                  const dragId = draggedPanelIdRef.current || draggedPanelId || e.dataTransfer.getData("text/plain");
                  if (!dragId || dragId === session.id) return;
                  if (dropTarget !== session.id) {
                    setDropTarget(session.id);
                  }
                }}
                onDragLeave={(e) => {
                  const related = e.relatedTarget as Node | null;
                  if (!related || !e.currentTarget.contains(related)) {
                    if (dropTarget === session.id) {
                      setDropTarget(null);
                    }
                  }
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  const dragId = draggedPanelIdRef.current || draggedPanelId || e.dataTransfer.getData("text/plain");
                  if (!dragId || dragId === session.id) {
                    draggedPanelIdRef.current = null;
                    setDraggedPanelId(null);
                    setDropTarget(null);
                    return;
                  }
                  handleSwitchPanels(dragId, session.id);
                  draggedPanelIdRef.current = null;
                  setDraggedPanelId(null);
                  setDropTarget(null);
                }}
                onKeyDown={(e) => {
                  if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
                    e.preventDefault();
                    swapWithNeighbor(session.id, e.key === "ArrowLeft" ? "left" : "right");
                  }
                }}
                tabIndex={0}
                aria-label={`${session.title} panel. Press Alt+Left/Right to reorder.`}
              >
                <header
                  className="session-panel-head"
                  draggable={true}
                  onDragStart={(e) => {
                    if ((e.target as HTMLElement).closest("button")) {
                      e.preventDefault();
                      return;
                    }
                    e.dataTransfer.setData("text/plain", session.id);
                    e.dataTransfer.effectAllowed = "move";
                    draggedPanelIdRef.current = session.id;
                    setDraggedPanelId(session.id);
                  }}
                  onDragEnd={() => {
                    draggedPanelIdRef.current = null;
                    setDraggedPanelId(null);
                    setDropTarget(null);
                  }}
                >
                  <div className="session-panel-reorder-group" onClick={(e) => e.stopPropagation()}>
                    {index > 0 && (
                      <button
                        type="button"
                        className="session-panel-move-btn"
                        onClick={(e) => {
                          e.stopPropagation();
                          swapWithNeighbor(session.id, "left");
                        }}
                        title="Move window left"
                        aria-label="Move window left"
                      >
                        <IconChevronLeft size={11} />
                      </button>
                    )}
                    <span
                      className="session-panel-drag-handle"
                      title="Drag to swap window position (or use ‹ › buttons or Alt+Left/Right)"
                      aria-hidden="true"
                    >
                      <IconGripVertical size={13} />
                    </span>
                    {index < orderedSessions.length - 1 && (
                      <button
                        type="button"
                        className="session-panel-move-btn"
                        onClick={(e) => {
                          e.stopPropagation();
                          swapWithNeighbor(session.id, "right");
                        }}
                        title="Move window right"
                        aria-label="Move window right"
                      >
                        <IconChevronRight size={11} />
                      </button>
                    )}
                  </div>
                  <span className="session-panel-provider" aria-hidden="true">
                    {isCli ? (
                      <IconTerminal size={14} />
                    ) : session.provider ? (
                      <ProviderLogo id={session.provider} size={16} />
                    ) : (
                      <IconChat size={14} />
                    )}
                  </span>
                  <span className="session-panel-title" title={session.title}>
                    <strong>{session.title.replace(/^CLI:\s*/, "")}</strong>
                    <small>{session.provider_label ?? (isCli ? "Terminal" : "Agent chat")}</small>
                  </span>
                  <span className={`session-panel-status st-${session.status}`}>
                    <i aria-hidden="true" />
                    {statusText(session.status)}
                  </span>
                  <button
                    type="button"
                    className="session-panel-action"
                    onClick={(event) => {
                      event.stopPropagation();
                      if (sizes[session.id]) {
                        setAutoFit(true);
                        setSizes((current) => {
                          const next = { ...current };
                          delete next[session.id];
                          return next;
                        });
                      } else {
                        resizePanel(session.id, 200);
                      }
                    }}
                    title={
                      sizes[session.id]
                        ? "Return to auto-fit width"
                        : "Custom expand this window"
                    }
                    aria-label={
                      sizes[session.id] ? "Return to auto-fit width" : "Custom expand window"
                    }
                  >
                    {sizes[session.id] ? <IconMinimize2 size={13} /> : <IconMaximize2 size={13} />}
                  </button>
                  <button
                    type="button"
                    className="session-panel-action"
                    onClick={(event) => {
                      event.stopPropagation();
                      onFocusSingle(session.id);
                    }}
                    title="Open this session in Single mode"
                    aria-label="Open this session in Single mode"
                  >
                    <IconGrid size={13} />
                  </button>
                  {onCloseSession ? (
                    <button
                      type="button"
                      className="session-panel-action session-panel-close"
                      onClick={(event) => {
                        event.stopPropagation();
                        onCloseSession(session.id);
                      }}
                      title="Close session"
                      aria-label={`Close ${session.title}`}
                    >
                      <IconClose size={12} />
                    </button>
                  ) : null}
                </header>

                <div className="session-panel-body">{renderSession(session)}</div>

                <div
                  className="session-panel-resizer"
                  role="separator"
                  aria-orientation="vertical"
                  title="Drag to resize panel, double-click to auto-fit"
                  onDoubleClick={() => {
                    setAutoFit(true);
                    setSizes({});
                  }}
                  onPointerDown={(event) => {
                    event.stopPropagation();
                    const el = event.currentTarget.parentElement;
                    const basis = el?.getBoundingClientRect().width ?? defaultBasis(layout, index, orderedSessions.length);
                    dragRef.current = { id: session.id, startX: event.clientX, startBasis: basis };
                    setDraggingId(session.id);
                    setAutoFit(false);
                  }}
                >
                  <span className="session-resizer-line" />
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
