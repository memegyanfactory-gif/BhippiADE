import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { WorkspaceSession } from "../lib/ipc";
import {
  IconChat,
  IconChevronDown,
  IconClose,
  IconGrid,
  IconTerminal,
} from "../components/icons";
import { ProviderLogo } from "../components/ProviderLogo";
import { useObstructsViewport } from "../lib/useViewportObstruction";
import { planLayout, type WorkspaceLayout } from "./layoutPlan";

export type { WorkspaceLayout };

export const WORKSPACE_LAYOUTS: Array<{
  id: WorkspaceLayout;
  label: string;
  note: string;
  badge?: string;
}> = [
  { id: "balanced", label: "Balanced columns", note: "Equal weight, tiled when it must", badge: "Auto" },
  { id: "adaptive", label: "Adaptive tidy", note: "Primary window prominent", badge: "Focus" },
  {
    id: "smart",
    label: "Smart fit",
    note: "Reads the windows: count, kind and room",
    badge: "Reads",
  },
];

/** The chords the canvas listens for, shown in the popover so they are findable. */
const LAYOUT_SHORTCUTS: Array<{ keys: string; what: string }> = [
  { keys: "Ctrl+Alt+← →", what: "Move the split beside the focused window" },
  { keys: "Ctrl+Alt+↑ ↓", what: "Move the split under it" },
  { keys: "Ctrl+Alt+Z", what: "Grow the focused window, again to restore" },
  { keys: "Ctrl+Alt+\\", what: "Even every window out" },
  { keys: "Ctrl+Alt+Space", what: "Next layout" },
  { keys: "Alt+← →", what: "Move the window itself" },
];

export interface WorkspaceOrganizerProps {
  layout: WorkspaceLayout;
  onApplyLayout: (layout: WorkspaceLayout) => void;
  autoFit: boolean;
  onToggleAutoFit: () => void;
  sessions?: WorkspaceSession[];
  activeSessionId?: string | null;
  onFocusSession?: (id: string) => void;
  onCloseSession?: (id: string) => void;
  iconOnly?: boolean;
  onEnsureMultiMode?: () => void;
  isMultiMode?: boolean;
}

export function WorkspaceOrganizer({
  layout,
  onApplyLayout,
  autoFit,
  onToggleAutoFit,
  sessions = [],
  activeSessionId,
  onFocusSession,
  onCloseSession,
  iconOnly = false,
  onEnsureMultiMode,
  isMultiMode = false,
}: WorkspaceOrganizerProps) {
  const [open, setOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<"layout" | "windows">("layout");
  // The popover is a portal over the Studio viewport; the native Godot child hides
  // while it is open so it can actually be seen (SPA-001).
  useObstructsViewport(open);
  const anchorRef = useRef<HTMLDivElement | null>(null);
  const [popoverPos, setPopoverPos] = useState<{ top: number; left: number } | null>(null);

  const windowCount = Math.max(1, sessions.length);

  const updatePos = () => {
    if (anchorRef.current) {
      const rect = anchorRef.current.getBoundingClientRect();
      const popoverWidth = 350;
      let left = rect.left + rect.width / 2 - popoverWidth / 2;
      if (left + popoverWidth > window.innerWidth - 12) {
        left = window.innerWidth - popoverWidth - 12;
      }
      if (left < 12) {
        left = 12;
      }
      setPopoverPos({
        top: rect.bottom + 6,
        left,
      });
    }
  };

  const toggleOpen = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!open) {
      onEnsureMultiMode?.();
      updatePos();
      setOpen(true);
    } else {
      setOpen(false);
    }
  };

  useEffect(() => {
    if (!open) return;
    updatePos();
    const onResize = () => updatePos();
    const onScroll = () => updatePos();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };

    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  /**
   * The preview is the real plan, drawn small.
   *
   * It runs the same planner the canvas runs, on the same windows and the same viewport,
   * so what the tile shows is exactly what applying it will do — including Smart fit
   * changing its mind when a terminal joins or the window gets narrower.
   */
  const renderPreviewTiles = (optionId: WorkspaceLayout) => {
    const items: WorkspaceSession[] =
      sessions.length > 0
        ? sessions
        : [
            {
              id: "mock-1",
              title: "Window 1",
              kind: "ai_chat",
              project_path: "",
              provider: null,
              provider_label: null,
              status: "idle",
              created_at: "",
              updated_at: "",
              turn_count: 0,
            },
          ];

    // The canvas is the screen minus the rail and the title bar; close enough that the
    // preview and the canvas agree on how many columns fit.
    const canvasWidth = typeof window === "undefined" ? 1280 : Math.max(480, window.innerWidth - 260);
    const canvasHeight = typeof window === "undefined" ? 800 : Math.max(320, window.innerHeight - 120);
    const plan = planLayout({
      layout: optionId,
      windows: items.map((session) => ({
        id: session.id,
        kind: session.kind === "cli" ? "cli" : "chat",
      })),
      canvasWidth,
      canvasHeight,
    });

    return (
      <span
        className="layout-preview preview-dynamic"
        style={{
          gridTemplateColumns: plan.columns.map((weight) => `${weight}fr`).join(" "),
          gridTemplateRows: plan.rows.map((weight) => `${weight}fr`).join(" "),
        }}
        aria-hidden="true"
      >
        {plan.areas.map((area, index) => {
          const session = items[index];
          const isActive = session.id === activeSessionId || (index === 0 && !activeSessionId);
          return (
            <i
              key={session.id || index}
              className={isActive ? "active-win" : ""}
              title={session.title || `Window ${index + 1}`}
              style={{
                gridColumn: `${area.column} / span ${area.columnSpan}`,
                gridRow: `${area.row} / span ${area.rowSpan}`,
              }}
            >
              <span>{session.kind === "cli" ? ">_" : `${index + 1}`}</span>
            </i>
          );
        })}
      </span>
    );
  };

  return (
    <div className="organizer-anchor" ref={anchorRef}>
      <button
        type="button"
        className={`organize-trigger project-quiet${iconOnly ? " icon-only" : ""}${open ? " open" : ""}${isMultiMode ? " in-multi" : ""}`}
        onClick={toggleOpen}
        aria-haspopup="dialog"
        aria-expanded={open}
        title={sessions.length > 0 ? `Organize workspace panels (${sessions.length} open)` : "Organize workspace panels"}
      >
        <IconGrid size={12} />
        {!iconOnly && (
          <>
            <span className="organize-label">Organize</span>
            {sessions.length > 1 && (
              <span className="organize-count-badge" title={`${sessions.length} open windows`}>
                {sessions.length}
              </span>
            )}
            <IconChevronDown size={10} className={open ? "flip" : ""} />
          </>
        )}
      </button>

      {open && popoverPos && typeof document !== "undefined"
        ? createPortal(
            <>
              <button
                type="button"
                className="organizer-scrim"
                aria-label="Close organizer"
                onClick={() => setOpen(false)}
              />
              <div
                className="organizer-popover fixed-portal"
                style={{ top: `${popoverPos.top}px`, left: `${popoverPos.left}px` }}
                role="dialog"
                aria-label="Organize panels"
              >
                <div className="organizer-title">Organize Workspace</div>

                <button
                  type="button"
                  className={`organizer-autofit${autoFit ? " active" : ""}`}
                  onClick={onToggleAutoFit}
                  aria-pressed={autoFit}
                >
                  <span>
                    <strong>Auto-fit Windows ({windowCount})</strong>
                    <small>
                      {autoFit
                        ? `The layout is deciding, across all ${windowCount} windows`
                        : "Your own splits are in force — turn this on to even them out"}
                    </small>
                  </span>
                  <span className="organizer-switch" aria-hidden="true">
                    <i />
                  </span>
                </button>

                <div className="organizer-tabs" aria-label="Organizer scope">
                  <button
                    type="button"
                    className={`organizer-tab-btn${activeTab === "layout" ? " active" : ""}`}
                    onClick={() => setActiveTab("layout")}
                  >
                    Layouts
                  </button>
                  <button
                    type="button"
                    className={`organizer-tab-btn${activeTab === "windows" ? " active" : ""}`}
                    onClick={() => setActiveTab("windows")}
                  >
                    Auto Windows ({sessions.length})
                  </button>
                </div>

                {activeTab === "layout" ? (
                  <>
                    <span className="organizer-eyebrow">
                      Layout Modes ({windowCount} {windowCount === 1 ? "window" : "windows"})
                    </span>
                    <div className="organizer-layouts">
                      {WORKSPACE_LAYOUTS.map((option) => (
                        <button
                          key={option.id}
                          type="button"
                          className={`organizer-layout${layout === option.id ? " active" : ""}`}
                          onClick={() => {
                            onApplyLayout(option.id);
                            if (!autoFit) onToggleAutoFit();
                          }}
                          aria-pressed={layout === option.id}
                        >
                          {renderPreviewTiles(option.id)}
                          <span className="organizer-layout-copy">
                            <strong>
                              {option.label}
                              {option.badge ? <em>{option.badge}</em> : null}
                            </strong>
                            <small>{option.note}</small>
                          </span>
                          <span className="organizer-apply" aria-hidden="true">
                            ↔
                          </span>
                        </button>
                      ))}
                    </div>

                    <span className="organizer-eyebrow">Keyboard</span>
                    <dl className="organizer-shortcuts">
                      {LAYOUT_SHORTCUTS.map((shortcut) => (
                        <div key={shortcut.keys}>
                          <dt>
                            <kbd>{shortcut.keys}</kbd>
                          </dt>
                          <dd>{shortcut.what}</dd>
                        </div>
                      ))}
                    </dl>
                  </>
                ) : (
                  <>
                    <span className="organizer-eyebrow">
                      Open Windows ({sessions.length})
                    </span>
                    {sessions.length === 0 ? (
                      <div className="organizer-empty-note">No open windows in this workspace.</div>
                    ) : (
                      <div className="organizer-windows-list">
                        {sessions.map((s, idx) => {
                          const isActive = s.id === activeSessionId;
                          const isCli = s.kind === "cli";
                          return (
                            <div
                              key={s.id}
                              className={`organizer-window-item${isActive ? " active" : ""}`}
                            >
                              <div className="organizer-window-info">
                                <span className="organizer-win-icon" aria-hidden="true">
                                  {isCli ? (
                                    <IconTerminal size={14} />
                                  ) : s.provider ? (
                                    <ProviderLogo id={s.provider} size={15} />
                                  ) : (
                                    <IconChat size={14} />
                                  )}
                                </span>
                                <div className="organizer-window-title">
                                  <strong>
                                    {idx + 1}. {s.title.replace(/^CLI:\s*/, "")}
                                  </strong>
                                  <small>
                                    {s.provider_label ?? (isCli ? "Terminal" : "Chat")} · {s.status}
                                  </small>
                                </div>
                              </div>
                              <div className="organizer-window-actions">
                                {onFocusSession && !isActive && (
                                  <button
                                    type="button"
                                    className="organizer-win-btn"
                                    onClick={() => {
                                      onFocusSession(s.id);
                                      setOpen(false);
                                    }}
                                    title="Focus this window"
                                  >
                                    Focus
                                  </button>
                                )}
                                {onCloseSession && (
                                  <button
                                    type="button"
                                    className="organizer-win-close"
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      onCloseSession(s.id);
                                    }}
                                    title="Close this window"
                                    aria-label={`Close ${s.title}`}
                                  >
                                    <IconClose size={12} />
                                  </button>
                                )}
                              </div>
                            </div>
                          );
                        })}
                      </div>
                    )}
                  </>
                )}
              </div>
            </>,
            document.body,
          )
        : null}
    </div>
  );
}
