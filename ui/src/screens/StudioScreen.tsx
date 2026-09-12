import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  GodotEmbedState,
  ProjectSummary,
  ProviderInfo,
  UsageSummary,
  WorkspaceSession,
} from "../lib/ipc";
import { api, events } from "../lib/api";
import { projectKey } from "../lib/gameCards.ts";
import { GodotViewport } from "../studio/GodotViewport";
import { decideAutoOpen, workspaceHolds } from "../studio/workspaceAutoOpen";
import { useViewportObstructed } from "../lib/useViewportObstruction";
// Both specifiers carry their extension: `ChatTabs.tsx` (the strip) and `chatTabs.ts` (the
// selection it draws) differ only in case, which an extensionless import cannot tell apart
// on Windows.
import { ChatTabs } from "../studio/ChatTabs.tsx";
import { chatTabsFor } from "../studio/chatTabs.ts";
import { TeamBoard } from "../studio/TeamBoard.tsx";
import { Chat } from "./Chat";
import { GameSettingsModal } from "../studio/GameSettingsModal";
import type { SettingsTab } from "./SettingsModal";
import "../styles/studio.css";

/**
 * The Studio: chat on the left, the Godot viewport on the right (ADR-0045).
 *
 * The viewport is not a picture of the project — it is the project. The Godot editor
 * (the workspace) and the running game are native windows embedded over the viewport
 * card, so what the user sees is what Godot draws. This screen owns exactly one control —
 * Play / Stop — and makes sure nothing in the page ever stands over the viewport while a
 * surface is embedded. The engine toolbar and the tab dock that used to sit beneath it were
 * removed at the owner's word: the preview is the game, not a frame around it.
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
  // No toggle any more, so the conversation is simply always there.
  const [gameSettingsOpen, setGameSettingsOpen] = useState(false);
  const [embed, setEmbed] = useState<GodotEmbedState | null>(null);
  /**
   * Bumped when the project changes under the studio while nothing is open — the agent
   * just created it (ADR-0047) — so the auto-open below runs again for a key it had
   * already settled. A refusal for "there is no project.godot" must not outlive the
   * moment there is one.
   */
  const [reopenTick, setReopenTick] = useState(0);
  const [notice, setNotice] = useState<string | null>(null);
  /** The project whose workspace has already been offered; see `decideAutoOpen`. */
  const settledProject = useRef<string | null>(null);

  const resolvedProject = activeProject ?? (projects.length > 0 ? projects[0] : null);
  const projectName = resolvedProject?.name ?? "demo-game";
  const projectPath = resolvedProject?.path ?? "";

  /** This project's chats, in strip order. Conversations are per project. */
  const chatTabs = useMemo(() => chatTabsFor(sessions, projectPath), [sessions, projectPath]);

  const gameRunning = embed?.game !== null && embed?.game !== undefined;
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
  }, [act, embed, projectPath, reopenTick]);

  // The Play button inside the Godot toolbar (ADR-0064). The addon leaves a request on disk,
  // Rust notices it, and it lands here — so the editor's Play and the studio's are the same
  // launch, with the same guards and the same embedding.
  useEffect(() => {
    let cancelled = false;
    const unlisten = events.godotPlayRequested.listen((event) => {
      if (cancelled || !projectPath) return;
      if (projectKey(event.payload.project) !== projectKey(projectPath)) return;
      void act("start the game", () => api.godotEmbedPlay(projectPath));
    });
    return () => {
      cancelled = true;
      void unlisten.then((stop) => stop());
    };
  }, [act, projectPath]);

  // The one signal Rust sends for "this project changed under you". While the viewport
  // holds nothing for the active project, that is the agent having just created it, and
  // the settled refusal is stale: forget it and let the effect above ask again.
  useEffect(() => {
    let cancelled = false;
    const unlisten = events.godotSceneChanged.listen((event) => {
      if (cancelled) return;
      const key = projectKey(projectPath);
      if (key.length === 0 || projectKey(event.payload.project) !== key) return;
      if (workspaceHolds(embed, projectPath)) return;
      settledProject.current = null;
      setReopenTick((tick) => tick + 1);
    });
    return () => {
      cancelled = true;
      void unlisten.then((stop) => stop());
    };
  }, [embed, projectPath]);

  // Workspace, Preview, Export, Inspect, Undo, Playtest and Watch play were the engine
  // toolbar's buttons and had no other caller. The toolbar is gone, so they are too; the
  // commands behind them are untouched in `api` and in Rust, waiting for wherever they land
  // next. `act` and `notice` stay — a Play that fails still has to say so.

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
                    <TeamBoard
                      sessions={sessions}
                      projectPath={projectPath}
                      activeId={activeConversationId ?? null}
                      onOpen={onOpenConversation ?? (() => {})}
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

          {isDragging && (
            <div
              className="studio-drag-shield"
              onPointerMove={handlePointerMove}
              onPointerUp={handlePointerUp}
              onPointerCancel={handlePointerUp}
            />
          )}

          {/* Right column: one button, then the viewport. Nothing is positioned over the
              viewport — it is a native child window and a native window cannot be painted on. */}
          <section className="studio-right-column">
            {/* The whole of the studio's chrome. The engine toolbar and the tab dock that used
                to sit under the viewport are gone at the owner's word — the preview is the
                game, and the one thing you do to it is start and stop it. */}
            {/* Play moved into the Godot toolbar, where the owner asked for it (ADR-0064) —
                so this strip is empty in the ordinary case and the preview is the editor and
                nothing else.

                Stop cannot move with it. The running game is embedded *over* the editor, so
                the toolbar holding Play is underneath it the moment it starts: a Stop in
                there would be a Stop nobody can reach. It appears only while there is a game
                to stop, and goes again the instant there is not. */}
            {gameRunning || notice ? (
              <div className="studio-viewport-topbar">
                {gameRunning ? (
                  <button
                    type="button"
                    className="studio-viewport-play running"
                    onClick={handlePlay}
                    title="Stop the game"
                  >
                    <span aria-hidden="true">■</span>
                    Stop
                  </button>
                ) : null}
                {notice ? (
                  <span className="studio-viewport-notice" role="status" aria-live="polite">
                    {notice}
                  </span>
                ) : null}
              </div>
            ) : null}
            <div className="studio-viewport-card">
              <GodotViewport
                projectPath={projectPath}
                obstructed={obstructed}
                resizing={isDragging}
                onState={setEmbed}
              />
            </div>

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
