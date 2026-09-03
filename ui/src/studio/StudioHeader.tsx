import { useState } from "react";
import type { ProjectSummary } from "../lib/ipc";

interface StudioHeaderProps {
  projectName?: string;
  projects?: ProjectSummary[];
  onSelectProject?: (project: ProjectSummary) => void;
  onNewProject?: () => void;
  onUndo?: () => void;
  onRedo?: () => void;
  onPlay?: () => void;
  onPreview?: () => void;
  onExport?: (target: "web" | "windows") => void;
  scenePanelOpen: boolean;
  onToggleScenePanel: () => void;
  inspectorPanelOpen: boolean;
  onToggleInspectorPanel: () => void;
  isPlaying?: boolean;
  onOpenSettings?: () => void;
}

export function StudioHeader({
  projectName = "demo-game",
  projects = [],
  onSelectProject,
  onNewProject,
  onOpenSettings,
  onUndo,
  onRedo,
  onPlay,
  onPreview,
  onExport,
  scenePanelOpen: _scenePanelOpen = false,
  onToggleScenePanel: _onToggleScenePanel,
  inspectorPanelOpen: _inspectorPanelOpen = false,
  onToggleInspectorPanel: _onToggleInspectorPanel,
  isPlaying = false,
}: StudioHeaderProps) {
  const [projectMenuOpen, setProjectMenuOpen] = useState(false);
  const [exportMenuOpen, setExportMenuOpen] = useState(false);
  const [moreMenuOpen, setMoreMenuOpen] = useState(false);

  return (
    <header className="studio-header">
      {/* Left Branding & Project Selector */}
      <div className="studio-header-left">
        <div className="studio-brand">
          <div className="studio-brand-logo">
            <svg viewBox="0 0 24 24" fill="currentColor">
              <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 14H9V8h2v8zm4 0h-2V8h2v8z" />
            </svg>
          </div>
          <span>bhippi</span>
        </div>

        {/* Project Selector Dropdown */}
        <div style={{ position: "relative" }}>
          <button
            type="button"
            className="studio-project-pill"
            onClick={() => setProjectMenuOpen((prev) => !prev)}
            aria-label="Select Game Project"
          >
            <span>{projectName}</span>
            <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
              <polyline points="6 9 12 15 18 9" />
            </svg>
          </button>

          {projectMenuOpen && (
            <div
              className="subagent-popover"
              style={{ top: "36px", left: "0", bottom: "auto", width: "220px" }}
            >
              <div className="subagent-popover-head">
                <span>Game Projects</span>
              </div>
              <div className="subagent-list">
                {projects.length > 0 ? (
                  projects.map((p) => (
                    <div
                      key={p.path}
                      className={`studio-tree-item ${p.name === projectName ? "selected" : ""}`}
                      onClick={() => {
                        onSelectProject?.(p);
                        setProjectMenuOpen(false);
                      }}
                    >
                      <span>📁 {p.name}</span>
                    </div>
                  ))
                ) : (
                  <div
                    className="studio-tree-item selected"
                    onClick={() => setProjectMenuOpen(false)}
                  >
                    <span>📁 demo-game (current)</span>
                  </div>
                )}
              </div>
              <div
                style={{
                  borderTop: "1px solid rgba(255,255,255,0.08)",
                  paddingTop: "6px",
                  display: "flex",
                }}
              >
                <button
                  type="button"
                  onClick={() => {
                    onNewProject?.();
                    setProjectMenuOpen(false);
                  }}
                  style={{
                    color: "var(--studio-accent)",
                    fontSize: "11.5px",
                    fontWeight: 600,
                    cursor: "pointer",
                    padding: "4px 8px",
                  }}
                >
                  + Create New Game...
                </button>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Center Actions (Undo, Redo, Play, Preview, Export, More) */}
      <div className="studio-header-center">
        {/* Undo */}
        <button
          type="button"
          className="studio-action-btn icon-only"
          onClick={onUndo}
          title="Undo (Ctrl+Z)"
          aria-label="Undo"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
            <path d="M3 7v6h6" />
            <path d="M21 17a9 9 0 0 0-9-9 9 9 0 0 0-6 2.3L3 13" />
          </svg>
        </button>

        {/* Redo */}
        <button
          type="button"
          className="studio-action-btn icon-only"
          onClick={onRedo}
          title="Redo (Ctrl+Y)"
          aria-label="Redo"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
            <path d="M21 7v6h-6" />
            <path d="M3 17a9 9 0 0 1 9-9 9 9 0 0 1 6 2.3L21 13" />
          </svg>
        </button>

        <div className="studio-header-divider" />

        {/* Play */}
        <button
          type="button"
          className={`studio-action-btn ${isPlaying ? "active" : ""}`}
          onClick={onPlay}
          title="Play Game (Launch Godot / Playtest)"
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
            <polygon points="5 3 19 12 5 21 5 3" />
          </svg>
          <span>{isPlaying ? "Stop" : "Play"}</span>
        </button>

        {/* Preview */}
        <button
          type="button"
          className="studio-action-btn"
          onClick={onPreview}
          title="Preview Game"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <rect x="2" y="3" width="20" height="14" rx="2" />
            <line x1="8" y1="21" x2="16" y2="21" />
            <line x1="12" y1="17" x2="12" y2="21" />
          </svg>
          <span>Preview</span>
        </button>

        {/* Export */}
        <div style={{ position: "relative" }}>
          <button
            type="button"
            className="studio-action-btn"
            onClick={() => setExportMenuOpen((prev) => !prev)}
            title="Export Game"
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
              <polyline points="17 8 12 3 7 8" />
              <line x1="12" y1="3" x2="12" y2="15" />
            </svg>
            <span>Export</span>
          </button>

          {exportMenuOpen && (
            <div
              className="subagent-popover"
              style={{ top: "36px", left: "0", bottom: "auto", width: "160px" }}
            >
              <div
                className="studio-tree-item"
                onClick={() => {
                  onExport?.("web");
                  setExportMenuOpen(false);
                }}
              >
                🌐 Web (HTML5 / WASM)
              </div>
              <div
                className="studio-tree-item"
                onClick={() => {
                  onExport?.("windows");
                  setExportMenuOpen(false);
                }}
              >
                🖥️ Windows Desktop (.exe)
              </div>
            </div>
          )}
        </div>

        {/* More Menu */}
        <div style={{ position: "relative" }}>
          <button
            type="button"
            className="studio-action-btn icon-only"
            onClick={() => setMoreMenuOpen((prev) => !prev)}
            title="More Options"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
              <circle cx="12" cy="12" r="2" />
              <circle cx="19" cy="12" r="2" />
              <circle cx="5" cy="12" r="2" />
            </svg>
          </button>

          {moreMenuOpen && (
            <div
              className="subagent-popover"
              style={{ top: "36px", right: "0", left: "auto", bottom: "auto", width: "180px" }}
            >
              <div
                className="studio-tree-item"
                onClick={() => {
                  onOpenSettings?.();
                  setMoreMenuOpen(false);
                }}
              >
                ⚙️ Project Settings
              </div>
              <div className="studio-tree-item" onClick={() => setMoreMenuOpen(false)}>
                📷 Take Screenshot
              </div>
              <div
                className="studio-tree-item"
                onClick={() => {
                  onOpenSettings?.();
                  setMoreMenuOpen(false);
                }}
              >
                🤖 AI Multi-Agent Settings
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Right Scene, Inspector, & Window Controls */}
      <div className="studio-header-right">
        {/* Top-Right Window Controls (Minimize, Maximize, Close) */}
        <div className="studio-window-controls" role="group" aria-label="Window Controls">
          <button
            type="button"
            className="studio-win-btn"
            onClick={async () => {
              if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
                try {
                  const { getCurrentWindow } = await import("@tauri-apps/api/window");
                  void getCurrentWindow().minimize();
                } catch {
                  // ignore
                }
              }
            }}
            title="Minimize"
            aria-label="Minimize Window"
          >
            <svg width="10" height="2" viewBox="0 0 10 2" fill="currentColor">
              <rect width="10" height="2" />
            </svg>
          </button>

          <button
            type="button"
            className="studio-win-btn"
            onClick={async () => {
              if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
                try {
                  const { getCurrentWindow } = await import("@tauri-apps/api/window");
                  void getCurrentWindow().toggleMaximize();
                } catch {
                  // ignore
                }
              }
            }}
            title="Maximize / Restore"
            aria-label="Maximize Window"
          >
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.2">
              <rect x="0.6" y="0.6" width="8.8" height="8.8" rx="1" />
            </svg>
          </button>

          <button
            type="button"
            className="studio-win-btn close"
            onClick={async () => {
              if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
                try {
                  const { getCurrentWindow } = await import("@tauri-apps/api/window");
                  void getCurrentWindow().close();
                } catch {
                  // ignore
                }
              }
            }}
            title="Close"
            aria-label="Close Window"
          >
            <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.2">
              <line x1="1" y1="1" x2="9" y2="9" />
              <line x1="9" y1="1" x2="1" y2="9" />
            </svg>
          </button>
        </div>
      </div>
    </header>
  );
}
