import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import type { WorkbenchMode } from "../workbench/ModeSwitch";
import {
  IconBrowser,
  IconChat,
  IconCheck,
  IconChevronDown,
  IconEditor,
  IconEngine,
  IconPanelRight,
  IconSplitView,
} from "../components/icons";

export interface TitleBarCenterControlsProps {
  workspaceMode: "single" | "multi";
  onWorkspaceMode: (mode: "single" | "multi") => void;
  workbenchOpen: boolean;
  onToggleWorkbench: () => void;
  workbenchMode: WorkbenchMode;
  onWorkbenchMode: (mode: WorkbenchMode) => void;
  organizeAction?: ReactNode;
}

const MODES: Array<{
  id: WorkbenchMode;
  label: string;
  desc: string;
  icon: typeof IconEditor;
}> = [
  { id: "editor", label: "Code Editor", desc: "File tree & source editor", icon: IconEditor },
  { id: "engine", label: "Game Engine", desc: "2D/3D viewport, scene hierarchy & HUD", icon: IconEngine },
  { id: "browser", label: "Web Browser", desc: "Local preview & live dev tools", icon: IconBrowser },
];

export function TitleBarCenterControls({
  workspaceMode,
  onWorkspaceMode,
  workbenchOpen,
  onToggleWorkbench,
  workbenchMode,
  onWorkbenchMode,
  organizeAction,
}: TitleBarCenterControlsProps) {
  const [modeMenuOpen, setModeMenuOpen] = useState(false);
  const menuAnchorRef = useRef<HTMLDivElement | null>(null);
  const [menuPos, setMenuPos] = useState<{ top: number; left: number } | null>(null);

  const updateMenuPos = () => {
    if (menuAnchorRef.current) {
      const rect = menuAnchorRef.current.getBoundingClientRect();
      const menuWidth = 230;
      let left = rect.left + rect.width / 2 - menuWidth / 2;
      if (left + menuWidth > window.innerWidth - 12) {
        left = window.innerWidth - menuWidth - 12;
      }
      if (left < 12) left = 12;
      setMenuPos({
        top: rect.bottom + 6,
        left,
      });
    }
  };

  useEffect(() => {
    if (!modeMenuOpen) return;
    updateMenuPos();

    const onPointerDown = (e: PointerEvent) => {
      const target = e.target as Node | null;
      if (menuAnchorRef.current && !menuAnchorRef.current.contains(target)) {
        setModeMenuOpen(false);
      }
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setModeMenuOpen(false);
    };

    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", updateMenuPos);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("resize", updateMenuPos);
    };
  }, [modeMenuOpen]);

  const ActiveModeIcon =
    workbenchMode === "browser"
      ? IconBrowser
      : workbenchMode === "engine"
      ? IconEngine
      : IconEditor;

  const modeLabel =
    workbenchMode === "browser" ? "Browser" : workbenchMode === "engine" ? "Engine" : "Editor";

  return (
    <div className="titlebar-center-cluster" role="toolbar" aria-label="Main controls">
      {/* Button 1: Fluid sliding toggle between Single Chat and Multiple Chats */}
      <div
        className={`chat-layout-toggle ${workspaceMode}`}
        role="radiogroup"
        aria-label="Chat layout mode"
      >
        <span className="chat-layout-slider" aria-hidden="true" />
        <button
          type="button"
          role="radio"
          aria-checked={workspaceMode === "single"}
          className={`chat-layout-btn${workspaceMode === "single" ? " active" : ""}`}
          onClick={() => onWorkspaceMode("single")}
          title="Single chat view (focus on one conversation)"
        >
          <IconChat size={12} />
          <span>Single</span>
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={workspaceMode === "multi"}
          className={`chat-layout-btn${workspaceMode === "multi" ? " active" : ""}`}
          onClick={() => onWorkspaceMode("multi")}
          title="Multiple chats view (side-by-side split grid)"
        >
          <IconSplitView size={12} />
          <span>Multi</span>
        </button>
      </div>

      {/* Button 2 (In Between): Organize Layout Button */}
      {organizeAction ? (
        <div className="titlebar-center-organize-slot">
          {organizeAction}
        </div>
      ) : null}

      {/* Button 3: Right panel workbench toggle (Editor, Engine, Browser) */}
      <div
        className={`workbench-center-toggle${workbenchOpen ? " on" : ""}`}
        ref={menuAnchorRef}
      >
        <button
          type="button"
          className="workbench-center-btn"
          onClick={onToggleWorkbench}
          aria-pressed={workbenchOpen}
          title={
            workbenchOpen
              ? `Hide right panel (${modeLabel})`
              : `Open right panel (${modeLabel})`
          }
        >
          <span className="workbench-center-glyph" aria-hidden="true">
            <ActiveModeIcon size={12} />
          </span>
          <span className="workbench-center-label">{modeLabel}</span>
          <span className={`workbench-panel-indicator${workbenchOpen ? " open" : ""}`}>
            <IconPanelRight size={13} />
          </span>
        </button>

        <button
          type="button"
          className={`workbench-center-dropdown-btn${modeMenuOpen ? " active" : ""}`}
          onClick={(e) => {
            e.stopPropagation();
            if (!modeMenuOpen) updateMenuPos();
            setModeMenuOpen((prev) => !prev);
          }}
          aria-haspopup="menu"
          aria-expanded={modeMenuOpen}
          title="Switch active panel (Editor, Engine, Browser)"
        >
          <IconChevronDown size={10} className={modeMenuOpen ? "flip" : ""} />
        </button>

        {modeMenuOpen && menuPos && typeof document !== "undefined"
          ? createPortal(
              <>
                <button
                  type="button"
                  className="titlebar-menu-scrim fixed-portal"
                  onClick={() => setModeMenuOpen(false)}
                  aria-label="Close menu"
                />
                <div
                  className="workbench-mode-menu fixed-portal"
                  style={{ top: `${menuPos.top}px`, left: `${menuPos.left}px` }}
                  role="menu"
                  aria-label="Select panel mode"
                >
                  <div className="workbench-mode-menu-header">Side Panel Mode</div>
                  {MODES.map((item) => {
                    const Glyph = item.icon;
                    const isSelected = workbenchMode === item.id;
                    return (
                      <button
                        key={item.id}
                        type="button"
                        role="menuitem"
                        className={`workbench-mode-menu-item${isSelected ? " selected" : ""}`}
                        onClick={() => {
                          onWorkbenchMode(item.id);
                          setModeMenuOpen(false);
                        }}
                      >
                        <span className="workbench-mode-menu-icon">
                          <Glyph size={14} />
                        </span>
                        <div className="workbench-mode-menu-text">
                          <strong>{item.label}</strong>
                          <small>{item.desc}</small>
                        </div>
                        {isSelected && (
                          <span className="workbench-mode-menu-check">
                            <IconCheck size={12} />
                          </span>
                        )}
                      </button>
                    );
                  })}
                </div>
              </>,
              document.body,
            )
          : null}
      </div>
    </div>
  );
}
