import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  GodotEmbedState,
  ProjectSummary,
  ProviderInfo,
  UsageSummary,
  WorkspaceSession,
} from "../lib/ipc";
import { api } from "../lib/api";
import { GodotViewport } from "../studio/GodotViewport";
import { decideAutoOpen } from "../studio/workspaceAutoOpen";
import { useViewportObstructed } from "../lib/useViewportObstruction";
// Both specifiers carry their extension: `ChatTabs.tsx` (the strip) and `chatTabs.ts` (the
// selection it draws) differ only in case, which an extensionless import cannot tell apart
// on Windows.
import { ChatTabs } from "../studio/ChatTabs.tsx";
import { chatTabsFor } from "../studio/chatTabs.ts";
import { Chat } from "./Chat";
import { StudioBottomDock, type StudioDockTab } from "../studio/StudioBottomDock";
import { GameSettingsModal } from "../studio/GameSettingsModal";
import type { SettingsTab } from "./SettingsModal";
import "../styles/studio.css";

/**
 * The Studio: chat on the left, the Godot viewport on the right (ADR-0045).
 *
 * The viewport is not a picture of the project — it is the project. The Godot editor
 * (the workspace) and the running game are native windows embedded over the viewport
 * card, so what the user sees is what Godot draws. This screen owns the transport
 * (Play / Stop / Workspace / Preview / Export) and makes sure nothing in the page ever
 * stands over the viewport while a surface is embedded.
 */

interface StudioScreenProps {
  sidebar?: React.ReactNode;
  activeProject: ProjectSummary | null;
  projects?: ProjectSummary[];
  onSelectProject?: (p: ProjectSummary) => void;
  onNewProject?: () => void;
  onOpenSettings?: (tab?: SettingsTab) => void;
  chatOptions?: ProviderInfo[];
  defaultProviderId?: string | null;
  lastModel?: Record<string, string>;
  activeConversationId?: string | null;
  /** Every session the app knows about; the tab strip picks this project's chats out of it. */
  sessions?: WorkspaceSession[];
  /** Close (and therefore delete) one chat from the tab strip. */
  onCloseTab?: (id: string) => void;
  onOpenConversation?: (id: string) => void;
  onConversationsChanged?: () => void;
  onRunningChange?: (label: string | null) => void;
  usage?: UsageSummary | null;
  onManageUsage?: () => void;
  onOpenBrowser?: (url?: string) => void;
  onRefreshUsage?: () => void;
  onOpenReview?: (turnTitle?: string | null) => void;
  onNewConversation?: () => void;
  onCloseConversation?: () => void;
  /** A modal owned by the shell is open over the studio: the native viewport must hide. */
  modalOpen?: boolean;
}

function describe(error: unknown): string {
  const message = (error as { message?: unknown })?.message;
  return typeof message === "string" ? message : String(error);
}

export function StudioScreen({
  sidebar,
  activeProject,
  projects = [],
  onSelectProject,
  onOpenSettings,
  chatOptions = [],
  defaultProviderId = null,
  lastModel = {},
  activeConversationId = null,
  sessions = [],
  onCloseTab,
  onOpenConversation,
  onConversationsChanged,
  onRunningChange,
  usage = null,
  onManageUsage,
  onOpenBrowser,
  onRefreshUsage,
  onOpenReview,
  onNewConversation,
  onCloseConversation,
  modalOpen = false,
}: StudioScreenProps) {
  const [chatOpen, setChatOpen] = useState(true);
  const [dockTab, setDockTab] = useState<StudioDockTab | null>(null);
  const [gameSettingsOpen, setGameSettingsOpen] = useState(false);
  const [embed, setEmbed] = useState<GodotEmbedState | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  /** The project whose workspace has already been offered; see `decideAutoOpen`. */
  const settledProject = useRef<string | null>(null);

  const resolvedProject = activeProject ?? (projects.length > 0 ? projects[0] : null);
  const projectName = resolvedProject?.name ?? "demo-game";
  const projectPath = resolvedProject?.path ?? "";

  /** This project's chats, in strip order. Conversations are per project. */
  const chatTabs = useMemo(() => chatTabsFor(sessions, projectPath), [sessions, projectPath]);

  const gameRunning = embed?.game !== null && embed?.game !== undefined;
  const workspaceOpen = embed?.workspace !== null && embed?.workspace !== undefined;
  // A dropdown, popover or menu over the viewport counts like a modal: the native child
  // cannot be painted over, so it hides for exactly as long as the surface is open (SPA-001).
  const floatingOpen = useViewportObstructed();
  const obstructed = modalOpen || gameSettingsOpen || floatingOpen;

  const DEFAULT_CHAT_WIDTH = 380;
  const [chatWidth, setChatWidth] = useState<number>(() => {
    try {
      const saved = localStorage.getItem("bhippi-studio-chat-width");
      if (saved) {
        const val = parseInt(saved, 10);
        if (!isNaN(val) && val >= 260 && val <= 1400) return val;
      }
    } catch {}
    return DEFAULT_CHAT_WIDTH;
  });
  const [isDragging, setIsDragging] = useState(false);
  const dragStartXRef = useRef(0);
  const dragStartWidthRef = useRef(0);
  const currentWidthRef = useRef(chatWidth);
  const cachedCanvasWidthRef = useRef(0);
  const canvasRef = useRef<HTMLDivElement | null>(null);
  const rafRef = useRef<number | null>(null);

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {}
    dragStartXRef.current = e.clientX;
    dragStartWidthRef.current = chatWidth;
    currentWidthRef.current = chatWidth;
    cachedCanvasWidthRef.current =
      canvasRef.current?.getBoundingClientRect().width ?? window.innerWidth;
    setIsDragging(true);
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDragging) return;
    e.preventDefault();
    const delta = e.clientX - dragStartXRef.current;
    const canvasWidth = cachedCanvasWidthRef.current || window.innerWidth;
    const maxAllowed = Math.max(380, Math.floor(canvasWidth * 0.65));
    const nextWidth = Math.min(maxAllowed, Math.max(260, dragStartWidthRef.current + delta));
    currentWidthRef.current = nextWidth;

    // Instant, snappy direct CSS custom property update at monitor refresh rate with 0 React renders
    if (rafRef.current) cancelAnimationFrame(rafRef.current);
    rafRef.current = requestAnimationFrame(() => {
      if (canvasRef.current) {
        canvasRef.current.style.setProperty("--studio-chat-width", `${nextWidth}px`);
      }
    });
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDragging) return;
    try {
      e.currentTarget.releasePointerCapture(e.pointerId);
    } catch {}
    if (rafRef.current) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    const finalWidth = currentWidthRef.current;
    setIsDragging(false);
    setChatWidth(finalWidth);
    try {
      localStorage.setItem("bhippi-studio-chat-width", String(finalWidth));
    } catch {}
  };

  const handleResetWidth = () => {
    setChatWidth(DEFAULT_CHAT_WIDTH);
    if (canvasRef.current) {
      canvasRef.current.style.setProperty("--studio-chat-width", `${DEFAULT_CHAT_WIDTH}px`);
    }
    try {
      localStorage.setItem("bhippi-studio-chat-width", String(DEFAULT_CHAT_WIDTH));
    } catch {}
  };

  const act = useCallback(async (label: string, work: () => Promise<unknown>) => {
    setNotice(null);
    try {
      await work();
    } catch (error) {
      setNotice(`Could not ${label}: ${describe(error)}`);
    }
  }, []);

  const handlePlay = useCallback(() => {
    if (!projectPath) return;
    if (gameRunning) {
      void act("stop the game", () => api.godotEmbedStop("game"));
    } else {
      void act("start the game", () => api.godotEmbedPlay(projectPath));
    }
  }, [act, gameRunning, projectPath]);

  // The engine is on by default: the viewport follows the project. The workspace is
  // offered once at mount and once every time the active project changes, as soon as Rust
  // has said what the viewport already holds. Play stays manual, and a deliberate "Close
  // workspace" is not undone while the user stays on that project — it is settled either way.
  useEffect(() => {
    const decision = decideAutoOpen({ projectPath, embed, settled: settledProject.current });
    if (decision.remember !== null) settledProject.current = decision.remember;
    const path = decision.open;
    if (path === null) return;
    void act("open the workspace", () => api.godotEmbedOpenWorkspace(path));
  }, [act, embed, projectPath]);

  const handleWorkspace = useCallback(() => {
    if (!projectPath) return;
    if (workspaceOpen) {
      void act("close the workspace", () => api.godotEmbedStop("workspace"));
    } else {
      void act("open the workspace", () => api.godotEmbedOpenWorkspace(projectPath));
    }
  }, [act, projectPath, workspaceOpen]);

  // The web export, in the workbench Browser pane — a page, so it lives in the page.
  const handlePreview = useCallback(() => {
    if (!projectPath) return;
    void act("start the preview", async () => {
      const url = await api.godotPreviewStart(projectPath);
      onOpenBrowser?.(url);
    });
  }, [act, onOpenBrowser, projectPath]);

  const handleExport = useCallback(
    (target: "web" | "windows") => {
      if (!projectPath) return;
      void act(`export for ${target}`, () => api.godotExport(projectPath, target));
    },
    [act, projectPath],
  );

  const handleUndo = useCallback(() => {
    if (!projectPath) return;
    void act("undo the last change", () => api.godotUndoLast(projectPath));
  }, [act, projectPath]);

  // Playtest and Watch play used to live in the retired Engine pane. Both run in Rust and
  // both report; the toolbar renders one line of what came back and computes none of it —
  // the frame count, the stop reason and the elapsed clock are all the report's own fields.
  const handlePlaytest = useCallback(() => {
    if (!projectPath) return;
    void act("run the playtest", async () => {
      const result = await api.godotPlaytest(projectPath, null, null);
      setNotice(
        `Playtest ${result.report.done ? "finished" : "stopped early"} — ${
          result.report.frames === null ? "no frame count" : `${result.report.frames} frames`
        }`,
      );
    });
  }, [act, projectPath]);

  const handleWatchPlay = useCallback(() => {
    if (!projectPath) return;
    void act("watch the game play", async () => {
      const result = await api.godotVisualPlaytest(projectPath, null);
      setNotice(
        `Watch play ${result.stopped_reason} in ${result.elapsed_ms} ms${
          result.stopped_detail ? ` — ${result.stopped_detail}` : ""
        }`,
      );
    });
  }, [act, projectPath]);

  const status = gameRunning
    ? "Playing in the viewport"
    : workspaceOpen
      ? "Workspace open"
      : projectPath
        ? "Ready"
        : "No project";

  return (
    <div className="studio-root">
      {/* Main studio canvas: the shared app title bar owns navigation; the engine owns its tools. */}
      <main className="studio-main-layout">
        {/* Far Left: Side Project Panel (Sidebar) */}
        {sidebar}

        {/* Everything right of the rail is the canvas; its top-left corner rounds into the chrome. */}
        <div
          ref={canvasRef}
          className={`studio-canvas${isDragging ? " resizing-active" : ""}`}
          style={{ "--studio-chat-width": `${chatWidth}px` } as React.CSSProperties}
        >
          {chatOpen && (
            <>
              <aside className="studio-left-column">
                {resolvedProject ? (
                  <>
                    {/* The tab strip replaces the chat's own top bar here (studio.css). */}
                    <ChatTabs
                      tabs={chatTabs}
                      activeId={activeConversationId}
                      onOpen={onOpenConversation ?? (() => {})}
                      onClose={onCloseTab ?? (() => {})}
                      onNew={onNewConversation ?? (() => {})}
                    />
                    <Chat
                      key={activeConversationId ?? "studio-chat"}
                      onRunningChange={onRunningChange ?? (() => {})}
                      chatOptions={chatOptions}
                      defaultProviderId={defaultProviderId}
                      lastModel={lastModel}
                      activeId={activeConversationId}
                      onOpenConversation={onOpenConversation ?? (() => {})}
                      onConversationsChanged={onConversationsChanged ?? (() => {})}
                      project={resolvedProject}
                      projects={projects}
                      onSelectProject={onSelectProject}
                      onOpenReview={onOpenReview}
                      usage={usage}
                      onManageUsage={onManageUsage}
                      onOpenSettings={onOpenSettings}
                      onNewConversation={onNewConversation}
                      onCloseConversation={onCloseConversation}
                      onOpenBrowser={onOpenBrowser}
                      onRefreshUsage={onRefreshUsage}
                    />
                  </>
                ) : (
                  <div style={{ padding: "32px", textAlign: "center", color: "var(--text-dim)" }}>
                    No active project selected.
                  </div>
                )}
              </aside>

              <div
                className={`studio-splitter${isDragging ? " dragging" : ""}`}
                role="separator"
                aria-orientation="vertical"
                aria-valuenow={chatWidth}
                aria-label="Resize chat panel (drag or double-click to reset)"
                title="Drag to resize chat panel · Double-click to reset"
                onPointerDown={handlePointerDown}
                onPointerMove={handlePointerMove}
                onPointerUp={handlePointerUp}
                onPointerCancel={handlePointerUp}
                onDoubleClick={handleResetWidth}
              >
                <div className="studio-splitter-handle" />
              </div>
            </>
          )}

          {isDragging && (
            <div
              className="studio-drag-shield"
              onPointerMove={handlePointerMove}
              onPointerUp={handlePointerUp}
              onPointerCancel={handlePointerUp}
            />
          )}

          {/* Right column: the Godot viewport, the transport, then the dock. Nothing here is
              positioned over the viewport — a native window cannot be painted over. */}
          <section className="studio-right-column">
            <div className="studio-viewport-card">
              <GodotViewport
                projectPath={projectPath}
                obstructed={obstructed}
                resizing={isDragging}
                onState={setEmbed}
              />
            </div>

            <div className="studio-engine-toolbar" role="toolbar" aria-label="Engine controls">
              <div className="studio-engine-toolbar-group">
                <button
                  type="button"
                  className={`studio-engine-control${chatOpen ? " active" : ""}`}
                  onClick={() => setChatOpen((open) => !open)}
                  aria-pressed={chatOpen}
                  title={chatOpen ? "Hide the chat" : "Show the chat"}
                >
                  <span aria-hidden="true">✦</span> Chat
                </button>
                <button
                  type="button"
                  className="studio-engine-control"
                  onClick={handleUndo}
                  disabled={!projectPath}
                  title="Undo the last change Bhippi made"
                >
                  <span aria-hidden="true">↶</span> Undo
                </button>
              </div>
              <div className="studio-engine-toolbar-group primary">
                <button
                  type="button"
                  className={`studio-engine-control primary${gameRunning ? " active" : ""}`}
                  onClick={handlePlay}
                  disabled={!projectPath}
                  title={gameRunning ? "Stop the game" : "Run the game in the viewport"}
                >
                  <span aria-hidden="true">{gameRunning ? "■" : "▶"}</span>{" "}
                  {gameRunning ? "Stop" : "Play"}
                </button>
                <button
                  type="button"
                  className="studio-engine-control"
                  onClick={handlePlaytest}
                  disabled={!projectPath}
                  title="Run the scripted playtest and report what the probe measured"
                >
                  <span aria-hidden="true">⏱</span> Playtest
                </button>
                <button
                  type="button"
                  className="studio-engine-control"
                  onClick={handleWatchPlay}
                  disabled={!projectPath}
                  title="Play the game and photograph it, step by step (ADR-0044)"
                >
                  <span aria-hidden="true">◉</span> Watch play
                </button>
                <button
                  type="button"
                  className={`studio-engine-control${workspaceOpen ? " active" : ""}`}
                  onClick={handleWorkspace}
                  disabled={!projectPath}
                  title={
                    workspaceOpen
                      ? "Close the Godot editor"
                      : "Open the Godot editor in the viewport"
                  }
                >
                  <span aria-hidden="true">⌘</span>{" "}
                  {workspaceOpen ? "Close workspace" : "Workspace"}
                </button>
                <button
                  type="button"
                  className="studio-engine-control"
                  onClick={handlePreview}
                  disabled={!projectPath}
                  title="Play the web export in the Browser pane"
                >
                  <span aria-hidden="true">▣</span> Preview
                </button>
                <div className="studio-engine-export-wrap">
                  <button
                    type="button"
                    className="studio-engine-control"
                    onClick={() => handleExport("web")}
                    disabled={!projectPath}
                  >
                    <span aria-hidden="true">↥</span> Export
                  </button>
                  <span className="studio-engine-export-hint">Web export</span>
                </div>
              </div>
              <span
                className={`studio-engine-status${notice ? " notice" : ""}`}
                aria-live="polite"
              >
                {notice ?? status}
              </span>
            </div>

            {/* Bottom Dock & Drawer (Assets with provenance/licence + Versions tab) */}
            <StudioBottomDock
              activeTab={dockTab}
              onSelectTab={setDockTab}
              projectPath={projectPath}
              projectName={projectName}
            />
          </section>
        </div>
      </main>

      {/* Game Settings Modal (GAD-023) */}
      <GameSettingsModal
        open={gameSettingsOpen}
        onClose={() => {
          setGameSettingsOpen(false);
          onOpenSettings?.();
        }}
        initialSettings={{
          title: projectName,
        }}
        onSave={(data) => {
          console.log("Saved game settings:", data);
        }}
      />
    </div>
  );
}
