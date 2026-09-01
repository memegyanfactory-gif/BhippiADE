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

export type WorkspaceLayout = "balanced" | "adaptive" | "smart";

export const WORKSPACE_LAYOUTS: Array<{
  id: WorkspaceLayout;
  label: string;
  note: string;
  badge?: string;
}> = [
  { id: "balanced", label: "Balanced columns", note: "Equal weight per window", badge: "Auto" },
  { id: "adaptive", label: "Adaptive tidy", note: "Primary window prominent", badge: "Focus" },
  { id: "smart", label: "Smart fit", note: "Surface first, tools fitted" },
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
    const count = items.length;

    if (optionId === "balanced") {
      return (
        <span
          className="layout-preview preview-dynamic"
          style={{ gridTemplateColumns: `repeat(${count}, 1fr)` }}
          aria-hidden="true"
        >
          {items.map((s, idx) => {
            const isActive = s.id === activeSessionId || (idx === 0 && !activeSessionId);
            return (
              <i
                key={s.id || idx}
                className={isActive ? "active-win" : ""}
                title={s.title || `Window ${idx + 1}`}
              >
                <span>{s.kind === "cli" ? ">_" : `${idx + 1}`}</span>
              </i>
            );
          })}
        </span>
      );
    }

    if (optionId === "adaptive") {
      const gridCols = count <= 1 ? "1fr" : `1.6fr ${Array(count - 1).fill("1fr").join(" ")}`;
      return (
        <span
          className="layout-preview preview-dynamic"
          style={{ gridTemplateColumns: gridCols }}
          aria-hidden="true"
        >
          {items.map((s, idx) => {
            const isActive = s.id === activeSessionId || (idx === 0 && !activeSessionId);
            return (
              <i
                key={s.id || idx}
                className={isActive ? "active-win" : ""}
                title={s.title || `Window ${idx + 1}`}
              >
                <span>{s.kind === "cli" ? ">_" : `${idx + 1}`}</span>
              </i>
            );
          })}
        </span>
      );
    }

    // smart fit
    if (count <= 2) {
      return (
        <span
          className="layout-preview preview-dynamic"
          style={{ gridTemplateColumns: count === 1 ? "1fr" : "1.5fr 1fr" }}
          aria-hidden="true"
        >
          {items.map((s, idx) => (
            <i key={s.id || idx} className={idx === 0 ? "active-win" : ""}>
              <span>{s.kind === "cli" ? ">_" : `${idx + 1}`}</span>
            </i>
          ))}
        </span>
      );
    }

    return (
      <span
        className="layout-preview preview-dynamic preview-smart-grid"
        aria-hidden="true"
      >
        <i className="active-win" style={{ gridRow: `1 / ${count}` }}>
          <span>{items[0].kind === "cli" ? ">_" : "1"}</span>
        </i>
        {items.slice(1).map((item, index) => (
          <i key={item.id || index + 1}>
            <span>{item.kind === "cli" ? ">_" : `${index + 2}`}</span>
          </i>
        ))}
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
                        ? `Evenly distributing all ${windowCount} open windows`
                        : "Refits windows smoothly as chats open & close"}
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
