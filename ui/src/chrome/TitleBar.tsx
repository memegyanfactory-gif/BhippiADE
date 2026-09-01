import { getCurrentWindow } from "@tauri-apps/api/window";
import { IconClose, IconGear, IconMaximize, IconMinimize } from "../components/icons";
import type { ReactNode } from "react";

export type Screen = "chat" | "research" | "automation" | "library" | "plugins";

const isMac = navigator.userAgent.includes("Macintosh");
const isTauriHost = "__TAURI_INTERNALS__" in window;

type TitleBarProps = {
  onOpenSettings: () => void;
  settingsBadge: boolean;
  demoMode: boolean;
  leftAction?: ReactNode;
  centerAction?: ReactNode;
  rightAction?: ReactNode;
};

/// The slim top strip (brief 09 §W4): wordmark, settings, window controls — and now
/// slideable mid actions for project operations, keeping the main canvas uncluttered.
export function TitleBar({
  onOpenSettings,
  settingsBadge,
  demoMode,
  leftAction,
  centerAction,
  rightAction,
}: TitleBarProps) {
  const win = isTauriHost ? getCurrentWindow() : null;

  return (
    <header className="titlebar">
      <div className="titlebar-left">
        <div className="wordmark">
          <span className="brand-logo" aria-hidden="true">
            <img src="/bhippi-logo.png" alt="" draggable={false} />
          </span>
          <span className="brand-name">bhippi</span>
          {demoMode ? <span className="badge-demo">demo</span> : null}
        </div>
        {leftAction}
      </div>

      <div className="titlebar-center">
        {centerAction}
      </div>

      <div className="titlebar-right">
        {rightAction}
        <button className="gear" onClick={onOpenSettings} aria-label="Settings">
          <IconGear />
          {settingsBadge ? <span className="badge" /> : null}
        </button>
        {!isMac && win ? (
          <div className="win-controls">
            <button className="win-btn" onClick={() => void win.minimize()} aria-label="Minimize">
              <IconMinimize />
            </button>
            <button
              className="win-btn"
              onClick={() => void win.toggleMaximize()}
              aria-label="Maximize"
            >
              <IconMaximize />
            </button>
            <button className="win-btn close" onClick={() => void win.close()} aria-label="Close">
              <IconClose />
            </button>
          </div>
        ) : null}
      </div>
    </header>
  );
}
