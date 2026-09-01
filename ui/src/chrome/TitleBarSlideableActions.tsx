import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import type { ProjectSummary, ProjectTool, ToolAvailability } from "../lib/ipc";
import { api } from "../lib/api";
import {
  IconBrain,
  IconChevronDown,
  IconChevronLeft,
  IconChevronRight,
  IconCode,
  IconExternal,
  IconGitBranch,
  IconGitMerge,
  IconRules,
  IconTerminal,
} from "../components/icons";

const TOOL_ICONS: Record<ProjectTool, (props: { size?: number }) => JSX.Element> = {
  vs_code: IconCode,
  cursor: IconCode,
  antigravity: IconTerminal,
  explorer: IconExternal,
};

export interface TitleBarSlideableActionsProps {
  project?: ProjectSummary | null;
  tools?: ToolAvailability[];
  onOpenRules?: () => void;
  onOpenReview?: () => void;
  onOpenBrain?: () => void;
  onProjectChange?: (project: ProjectSummary) => void;
  organizeAction?: ReactNode;
}

export function TitleBarSlideableActions({
  project,
  tools = [],
  onOpenRules,
  onOpenReview,
  onOpenBrain,
  onProjectChange,
  organizeAction,
}: TitleBarSlideableActionsProps) {
  const [open, setOpen] = useState(false);
  const [currentTools, setCurrentTools] = useState<ToolAvailability[]>(tools);
  const [error, setError] = useState<string | null>(null);
  const trackRef = useRef<HTMLDivElement | null>(null);
  const toolAnchorRef = useRef<HTMLDivElement | null>(null);
  const [toolMenuPos, setToolMenuPos] = useState<{ top: number; left: number } | null>(null);
  const [canScrollLeft, setCanScrollLeft] = useState(false);
  const [canScrollRight, setCanScrollRight] = useState(false);
  const isDraggingRef = useRef(false);
  const startXRef = useRef(0);
  const scrollLeftRef = useRef(0);

  useEffect(() => {
    if (tools.length > 0) {
      setCurrentTools(tools);
    }
  }, [tools]);

  const checkScroll = () => {
    const el = trackRef.current;
    if (!el) return;
    const { scrollLeft, scrollWidth, clientWidth } = el;
    setCanScrollLeft(scrollLeft > 2);
    setCanScrollRight(scrollLeft + clientWidth < scrollWidth - 2);
  };

  const updateToolMenuPos = () => {
    if (toolAnchorRef.current) {
      const rect = toolAnchorRef.current.getBoundingClientRect();
      const menuWidth = 270;
      let left = rect.left + rect.width / 2 - menuWidth / 2;
      if (left + menuWidth > window.innerWidth - 12) {
        left = window.innerWidth - menuWidth - 12;
      }
      if (left < 12) {
        left = 12;
      }
      setToolMenuPos({
        top: rect.bottom + 6,
        left,
      });
    }
  };

  const toggleOpen = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!open) {
      updateToolMenuPos();
      setOpen(true);
      void api.projectTools().then(setCurrentTools).catch(() => {});
    } else {
      setOpen(false);
    }
  };

  useEffect(() => {
    checkScroll();
    const el = trackRef.current;
    if (!el) return;
    const onResize = () => {
      checkScroll();
      if (open) updateToolMenuPos();
    };
    window.addEventListener("resize", onResize);
    const observer = new ResizeObserver(checkScroll);
    observer.observe(el);
    return () => {
      window.removeEventListener("resize", onResize);
      observer.disconnect();
    };
  }, [project, organizeAction, open]);

  useEffect(() => {
    if (!open) return;
    updateToolMenuPos();
    const escape = (event: KeyboardEvent) => event.key === "Escape" && setOpen(false);
    const onScroll = () => updateToolMenuPos();
    window.addEventListener("keydown", escape);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      window.removeEventListener("keydown", escape);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [open]);

  const slide = (direction: "left" | "right") => {
    const el = trackRef.current;
    if (!el) return;
    const amount = direction === "left" ? -140 : 140;
    el.scrollBy({ left: amount, behavior: "smooth" });
  };

  const onWheel = (e: React.WheelEvent<HTMLDivElement>) => {
    const el = trackRef.current;
    if (!el) return;
    const delta = e.deltaX !== 0 ? e.deltaX : e.deltaY;
    if (delta !== 0) {
      el.scrollLeft += delta;
      checkScroll();
    }
  };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if ((e.target as HTMLElement).closest("button")) {
      return;
    }
    const el = trackRef.current;
    if (!el) return;
    isDraggingRef.current = true;
    startXRef.current = e.clientX;
    scrollLeftRef.current = el.scrollLeft;
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingRef.current) return;
    const el = trackRef.current;
    if (!el) return;
    const delta = e.clientX - startXRef.current;
    el.scrollLeft = scrollLeftRef.current - delta;
    checkScroll();
  };

  const onPointerUp = () => {
    isDraggingRef.current = false;
  };

  const describe = (thrown: unknown) => {
    const value = thrown as { message?: string; hint?: string };
    setError([value.message, value.hint].filter(Boolean).join(" — "));
  };

  const launch = async (tool: ProjectTool) => {
    if (!project) return;
    setError(null);
    try {
      await api.openProjectIn(project.path, tool);
      setOpen(false);
    } catch (launchError) {
      describe(launchError);
    }
  };

  const initializeGit = async () => {
    if (!project) return;
    setError(null);
    try {
      onProjectChange?.(await api.initializeGit(project.path));
      setOpen(false);
    } catch (gitError) {
      describe(gitError);
    }
  };

  if (!project) {
    return null;
  }

  return (
    <div className="titlebar-left-actions">
      <div className="titlebar-slideable compact">
        {canScrollLeft && (
          <button
            type="button"
            className="titlebar-slide-arrow left"
            onClick={() => slide("left")}
            aria-label="Slide left"
          >
            <IconChevronLeft size={11} />
          </button>
        )}

        <div
          className="titlebar-slideable-track compact"
          ref={trackRef}
          onScroll={checkScroll}
          onWheel={onWheel}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerLeave={onPointerUp}
        >
          {organizeAction}

          {onOpenReview && (
            <button
              type="button"
              className="titlebar-action-btn compact review-btn"
              onClick={onOpenReview}
              title="Review changes made by AI in this workspace"
            >
              <IconGitMerge size={12} />
              <span>Review</span>
            </button>
          )}

          {onOpenRules && (
            <button
              type="button"
              className="titlebar-action-btn compact"
              onClick={onOpenRules}
              title="Standing instructions for the agent here"
            >
              <IconRules size={12} />
              <span>Rules</span>
            </button>
          )}

          {onOpenBrain && (
            <button
              type="button"
              className="titlebar-action-btn compact"
              onClick={onOpenBrain}
              title="Project Brain: index status, rebuild, module cards, and symbol search"
            >
              <IconBrain size={12} />
              <span>Brain</span>
            </button>
          )}

          <div className="titlebar-tool-anchor" ref={toolAnchorRef}>
            <button
              type="button"
              className={`titlebar-action-btn compact project-open${open ? " active" : ""}`}
              onClick={toggleOpen}
              aria-expanded={open}
              aria-haspopup="menu"
            >
              <IconExternal size={12} />
              <span>Open in</span>
              <IconChevronDown size={10} />
            </button>

            {open && toolMenuPos && typeof document !== "undefined"
              ? createPortal(
                  <>
                    <button
                      type="button"
                      className="titlebar-menu-scrim"
                      onClick={() => setOpen(false)}
                      aria-label="Close tool menu"
                    />
                    <div
                      className="titlebar-tool-menu fixed-portal"
                      style={{ top: `${toolMenuPos.top}px`, left: `${toolMenuPos.left}px` }}
                      role="menu"
                    >
                      {currentTools.map((tool) => {
                        const Glyph = TOOL_ICONS[tool.tool];
                        return (
                          <button
                            key={tool.tool}
                            type="button"
                            role="menuitem"
                            title={tool.available ? tool.hint : `${tool.hint} Click to try anyway.`}
                            onClick={() => void launch(tool.tool)}
                            className={!tool.available ? " tool-unavailable" : ""}
                          >
                            <Glyph size={14} />
                            <span>
                              <strong>{tool.label}</strong>
                              <small>{tool.available ? tool.hint : "Not detected — click to try"}</small>
                            </span>
                          </button>
                        );
                      })}
                      {!project.is_git_repository && (
                        <button type="button" role="menuitem" onClick={() => void initializeGit()}>
                          <IconGitBranch size={14} />
                          <span>
                            <strong>Initialize Git</strong>
                            <small>Create a repository in this folder</small>
                          </span>
                        </button>
                      )}
                      {error && (
                        <div className="tool-error" role="alert">
                          {error}
                        </div>
                      )}
                    </div>
                  </>,
                  document.body,
                )
              : null}
          </div>
        </div>

        {canScrollRight && (
          <button
            type="button"
            className="titlebar-slide-arrow right"
            onClick={() => slide("right")}
            aria-label="Slide right"
          >
            <IconChevronRight size={11} />
          </button>
        )}
      </div>
    </div>
  );
}
