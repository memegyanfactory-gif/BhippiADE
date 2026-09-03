import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconClose, IconMaximize, IconMinimize } from "../components/icons";
import type { ReactNode } from "react";
import { AutoUpdateWidget } from "./AutoUpdateWidget";

// The Studio's four destinations (GAD-008). Defined in `lib/screens`.
export type { Screen } from "../lib/screens";

const isMac = navigator.userAgent.includes("Macintosh");
const isTauriHost = "__TAURI_INTERNALS__" in window;

type TitleBarProps = {
  onOpenSettings?: () => void;
  settingsBadge?: boolean;
  demoMode?: boolean;
  leftAction?: ReactNode;
  centerAction?: ReactNode;
  rightAction?: ReactNode;
  organizeAction?: ReactNode;
  onOpenDependencies?: () => void;
};

/// The slim top strip (brief 09 §W4): wordmark, window controls, and organize action
export function TitleBar({
  leftAction,
  centerAction,
  rightAction,
  organizeAction,
  onOpenDependencies,
}: TitleBarProps) {
  const win = isTauriHost ? getCurrentWindow() : null;

  return (
    <header className="titlebar">
      <div className="titlebar-left" id="titlebar-left-slot">
        {leftAction}
      </div>

      <div className="titlebar-center">
        {centerAction}
      </div>

      <div className="titlebar-right">
        {rightAction}
        {onOpenDependencies && (
          <button
            type="button"
            className="titlebar-update-btn"
            onClick={onOpenDependencies}
            title="Setup Engine Dependencies (Godot, Templates, Providers)"
            aria-label="Engine Dependencies Setup"
          >
            <span style={{ fontSize: "12px" }}>⚙</span>
          </button>
        )}
        <AutoUpdateWidget />
        {organizeAction ? (
          <div className="titlebar-organize-slot">
            {organizeAction}
          </div>
        ) : null}
        {!isMac ? (
          <div className="win-controls">
            <button
              className="win-btn"
              onClick={() => (win ? void win.minimize() : undefined)}
              aria-label="Minimize"
              title="Minimize"
            >
              <IconMinimize />
            </button>
            <button
              className="win-btn"
              onClick={() => (win ? void win.toggleMaximize() : undefined)}
              aria-label="Maximize"
              title="Maximize"
            >
              <IconMaximize />
            </button>
            <button
              className="win-btn close"
              onClick={() => (win ? void win.close() : undefined)}
              aria-label="Close"
              title="Close"
            >
              <IconClose />
            </button>
          </div>
        ) : null}
      </div>
    </header>
  );
}
