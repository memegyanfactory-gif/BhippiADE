import { useCallback, useState, type ReactNode } from "react";
import type {
  ProjectSummary,
  ProviderInfo,
  UsageSummary,
  WorkspaceSession,
} from "../lib/ipc";
import {
  IconChat,
  IconClose,
  IconFolder,
  IconLayers,
  IconPlus,
  IconSplitView,
  IconTerminal,
} from "../components/icons";
import { ProviderLogo } from "../components/ProviderLogo";
import { Chat } from "./Chat";
import { CliView, type CliSession } from "./CliView";
import { MultiSessionWorkspace } from "../workspace/MultiSessionWorkspace";
import { MultiProjectWorkspace } from "../workspace/MultiProjectWorkspace";
import { projectForSession } from "../workspace/multiProjectBoard";
import type { WorkspaceLayout } from "../workspace/WorkspaceOrganizer";
import type { WorkspaceMode } from "../workspace/workspaceMode";
import type { SettingsTab } from "./SettingsModal";
import "../styles/multi-workspace.css";
import "../styles/multi-project.css";

interface ProjectsScreenProps {
  activeProject: ProjectSummary | null;
  projects?: ProjectSummary[];
  onSelectProject?: (p: ProjectSummary) => void;
  onNewProject?: (kind: "open" | "create" | "clone") => void;
  sessions: WorkspaceSession[];
  sessionsError: string | null;
  activeSessionId: string | null;
  onOpenSession: (id: string) => void;
  onCloseSession: (id: string) => void;
  onNewChat: () => void;
  onNewCli: () => void;
  onRetrySessions: () => void;
  cliSessions: CliSession[];
  onUpdateCliSession: (updated: CliSession) => void;
  workspaceMode: WorkspaceMode;
  onWorkspaceMode: (mode: WorkspaceMode) => void;
  /** Every project's sessions, for the multi-project board. `null` while loading. */
  allSessions?: WorkspaceSession[] | null;
  /** Opens a session in any project, activating that project first. */
  onOpenProjectSession?: (projectPath: string, sessionId: string) => void;
  /** Creates a chat or terminal in any project, activating that project first. */
  onNewSessionInProject?: (projectPath: string, kind: "chat" | "cli") => void;
  /** Closes a session in any project, activating that project first. */
  onCloseProjectSession?: (projectPath: string, sessionId: string) => void;
  workspaceLayout: WorkspaceLayout;
  onApplyLayout: (layout: WorkspaceLayout) => void;
  autoFit: boolean;
  /** Takes the value, not a toggle: the canvas turns auto-fit off when a split is dragged. */
  onSetAutoFit: (fit: boolean) => void;
  resetKey?: number;
  onReorderTabs?: (draggedId: string, targetId: string) => void;
  chatOptions?: ProviderInfo[];
  defaultProviderId?: string | null;
  lastModel?: Record<string, string>;
  onRunningChange?: (label: string | null) => void;
  usage?: UsageSummary | null;
  onManageUsage?: () => void;
  onOpenSettings?: (tab?: SettingsTab) => void;
  onOpenBrowser?: (url?: string) => void;
  onRefreshUsage?: () => void;
  onOpenReview?: (turnTitle?: string | null) => void;
  onConversationsChanged?: () => void;
}

export function ProjectsScreen({
  activeProject,
  projects,
  onSelectProject,
  onNewProject,
  sessions,
  sessionsError,
  activeSessionId,
  onOpenSession,
  onCloseSession,
  onNewChat,
  onNewCli,
  onRetrySessions,
  cliSessions,
  onUpdateCliSession,
  workspaceMode,
  onWorkspaceMode,
  allSessions = null,
  onOpenProjectSession,
  onNewSessionInProject,
  onCloseProjectSession,
  workspaceLayout,
  onApplyLayout,
  autoFit,
  onSetAutoFit,
  resetKey,
  onReorderTabs,
  chatOptions = [],
  defaultProviderId = null,
  lastModel = {},
  onRunningChange,
  usage,
  onManageUsage,
  onOpenSettings,
  onOpenBrowser,
  onRefreshUsage,
  onOpenReview,
  onConversationsChanged,
}: ProjectsScreenProps) {
  const [draggedTabId, setDraggedTabId] = useState<string | null>(null);

  const activeSession = sessions.find((s) => s.id === activeSessionId) ?? sessions[0] ?? null;

  const renderSessionContent = useCallback(
    (session: WorkspaceSession): ReactNode => {
      if (!activeProject) return null;

      // On the all-projects board a window's session belongs to a folder that is not
      // necessarily the active one, and its shell must run — and its chat must read and
      // send — in *that* folder. Falling back to the active project keeps the two
      // single-project modes exactly as they were.
      const owner = projectForSession(projects ?? [], session) ?? activeProject;

      if (session.kind === "cli" || session.id.startsWith("cli-")) {
        const cliData = cliSessions.find((c) => c.id === session.id) ?? {
          id: session.id,
          title: session.title,
          shell: "powershell" as const,
          createdAt: session.created_at,
          projectPath: owner.path,
        };
        return (
          <div className="projects-pane-content cli-pane" key={session.id}>
            <CliView
              session={cliData}
              projectPath={owner.path}
              onUpdateSession={onUpdateCliSession}
            />
          </div>
        );
      }

      return (
        <div className="projects-pane-content chat-pane" key={session.id}>
          <Chat
            key={session.id}
            activeId={session.id}
            onRunningChange={onRunningChange ?? (() => {})}
            chatOptions={chatOptions}
            defaultProviderId={defaultProviderId}
            lastModel={lastModel}
            onOpenConversation={onOpenSession}
            onConversationsChanged={onConversationsChanged ?? (() => {})}
            project={owner}
            projects={projects}
            onSelectProject={onSelectProject}
            onOpenReview={onOpenReview}
            usage={usage}
            onManageUsage={onManageUsage}
            onOpenSettings={onOpenSettings}
            onNewConversation={onNewChat}
            onCloseConversation={() => onCloseSession(session.id)}
            onOpenBrowser={onOpenBrowser}
            onRefreshUsage={onRefreshUsage}
          />
        </div>
      );
    },
    [
      activeProject,
      cliSessions,
      onUpdateCliSession,
      onRunningChange,
      chatOptions,
      defaultProviderId,
      lastModel,
      onOpenSession,
      onConversationsChanged,
      projects,
      onSelectProject,
      onOpenReview,
      usage,
      onManageUsage,
      onOpenSettings,
      onNewChat,
      onCloseSession,
      onOpenBrowser,
      onRefreshUsage,
    ],
  );

  if (!activeProject) {
    return (
      <div className="projects-screen empty-screen">
        <div className="projects-no-project">
          <IconFolder size={40} />
          <h2>No Active Project</h2>
          <p>Select a project from the sidebar to open its workspaces, or add an existing folder.</p>
          <div style={{ marginTop: "var(--space-3)", display: "flex", gap: "var(--space-2)", justifyContent: "center" }}>
            <button
              type="button"
              className="btn-primary"
              onClick={() => onNewProject?.("open")}
              style={{ display: "inline-flex", alignItems: "center", gap: 6, padding: "8px 16px", cursor: "pointer" }}
            >
              <IconPlus size={14} /> Open Project Folder
            </button>
            <button
              type="button"
              className="btn-secondary"
              onClick={() => onNewProject?.("create")}
              style={{ display: "inline-flex", alignItems: "center", gap: 6, padding: "8px 16px", cursor: "pointer" }}
            >
              Create Project
            </button>
          </div>
          {projects && projects.length > 0 && (
            <div style={{ marginTop: "var(--space-4)", width: "100%", maxWidth: 380, textAlign: "left" }}>
              <span className="project-eyebrow" style={{ display: "block", marginBottom: "var(--space-2)" }}>
                Projects ({projects.length})
              </span>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                {projects.map((p) => (
                  <button
                    key={p.path}
                    type="button"
                    className="session-menu-row"
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 10,
                      padding: "8px 12px",
                      borderRadius: "var(--radius-md)",
                      border: "1px solid var(--line)",
                      background: "var(--surface-2)",
                      color: "var(--text)",
                      textAlign: "left",
                      cursor: "pointer",
                      width: "100%",
                    }}
                    onClick={() => onSelectProject?.(p)}
                  >
                    <span className="session-menu-icon" style={{ flexShrink: 0, color: "var(--accent)" }}>
                      <IconFolder size={18} />
                    </span>
                    <span className="session-menu-copy" style={{ overflow: "hidden" }}>
                      <strong style={{ display: "block", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                        {p.name}
                      </strong>
                      <small style={{ display: "block", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", color: "var(--text-faint)" }}>
                        {p.path}
                      </small>
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="projects-screen">
      {workspaceMode === "multiproject" ? (
        /* Multi-project mode: every project's chats on one board */
        <div className="projects-multiproject-container">
          <MultiProjectWorkspace
            projects={
              // The active project always has a column, even before the list has loaded.
              (projects ?? []).some((p) => p.path === activeProject.path)
                ? (projects ?? [])
                : [activeProject, ...(projects ?? [])]
            }
            activeProjectPath={activeProject.path}
            sessions={allSessions}
            sessionsError={sessionsError}
            activeSessionId={activeSessionId}
            renderSession={renderSessionContent}
            onOpenSession={(projectPath, sessionId) => {
              if (onOpenProjectSession) onOpenProjectSession(projectPath, sessionId);
              else onOpenSession(sessionId);
            }}
            onFocusSingle={(projectPath, sessionId) => {
              if (onOpenProjectSession) onOpenProjectSession(projectPath, sessionId);
              else onOpenSession(sessionId);
              onWorkspaceMode("single");
            }}
            onNewSession={(projectPath, kind) => {
              if (onNewSessionInProject) onNewSessionInProject(projectPath, kind);
              else if (kind === "cli") onNewCli();
              else onNewChat();
            }}
            onCloseSession={(projectPath, sessionId) => {
              if (onCloseProjectSession) onCloseProjectSession(projectPath, sessionId);
              else onCloseSession(sessionId);
            }}
            onRetry={onRetrySessions}
            layout={workspaceLayout}
            autoFit={autoFit}
            onApplyLayout={onApplyLayout}
            onAutoFitChange={onSetAutoFit}
            resetKey={resetKey}
          />
        </div>
      ) : workspaceMode === "single" ? (
        <div className="projects-single-container">
          {/* Top Tab Strip for Single Mode */}
          <div className="projects-tab-bar" role="tablist" aria-label="Project workspaces and chats">
            <div className="projects-tabs-scroll">
              {sessions.map((tab) => {
                const isActive = tab.id === (activeSession?.id ?? activeSessionId);
                const isCli = tab.kind === "cli" || tab.id.startsWith("cli-");
                const label = tab.title.replace(/^CLI:\s*/, "") || (isCli ? "Terminal" : "Agent Chat");

                return (
                  <div
                    key={tab.id}
                    className={`projects-tab${isActive ? " active" : ""}`}
                    draggable
                    onDragStart={(e) => {
                      setDraggedTabId(tab.id);
                      e.dataTransfer.setData("text/plain", tab.id);
                    }}
                    onDragOver={(e) => {
                      if (draggedTabId && draggedTabId !== tab.id) {
                        e.preventDefault();
                      }
                    }}
                    onDrop={(e) => {
                      e.preventDefault();
                      if (draggedTabId && draggedTabId !== tab.id && onReorderTabs) {
                        onReorderTabs(draggedTabId, tab.id);
                      }
                      setDraggedTabId(null);
                    }}
                    onDragEnd={() => setDraggedTabId(null)}
                  >
                    <button
                      type="button"
                      className="projects-tab-open"
                      onClick={() => {
                        if (onOpenProjectSession) onOpenProjectSession(activeProject.path, tab.id);
                        else onOpenSession(tab.id);
                      }}
                      title={label}
                      role="tab"
                      aria-selected={isActive}
                    >
                      <span className="projects-tab-icon">
                        {isCli ? (
                          <IconTerminal size={13} />
                        ) : tab.provider ? (
                          <ProviderLogo id={tab.provider} size={13} />
                        ) : (
                          <IconChat size={13} />
                        )}
                      </span>
                      <span className="projects-tab-label">{label}</span>
                    </button>
                    <button
                      type="button"
                      className="projects-tab-close"
                      onClick={(e) => {
                        e.stopPropagation();
                        onCloseSession(tab.id);
                      }}
                      title="Close window"
                      aria-label={`Close ${label}`}
                    >
                      <IconClose size={10} />
                    </button>
                  </div>
                );
              })}
            </div>

            {/* Tab strip actions: New Chat, New CLI, Switch to Multi Mode */}
            <div className="projects-tab-actions">
              <button
                type="button"
                className="projects-tab-btn"
                onClick={onNewChat}
                title="Create new AI Chat in this project"
              >
                <IconPlus size={12} />
                <span>Chat</span>
              </button>
              <button
                type="button"
                className="projects-tab-btn"
                onClick={onNewCli}
                title="Create new Terminal CLI in this project"
              >
                <IconTerminal size={12} />
                <span>CLI</span>
              </button>
              <div className="projects-tab-divider" />
              <button
                type="button"
                className="projects-tab-btn switch-mode"
                onClick={() => onWorkspaceMode("multi")}
                title="Switch to Multi-window mode (auto-adjusted layout)"
              >
                <IconSplitView size={13} />
                <span>Multi Mode</span>
              </button>
              <button
                type="button"
                className="projects-tab-btn switch-mode"
                onClick={() => onWorkspaceMode("multiproject")}
                title="Switch to Multi-project mode (every project's chats on one screen)"
              >
                <IconLayers size={13} />
                <span>All Projects</span>
              </button>
              <div className="projects-tab-divider" />
              <button
                type="button"
                className="projects-tab-btn"
                onClick={() => onNewProject?.("open")}
                title="Open or add a project folder"
              >
                <IconFolder size={12} />
                <span>Add Project</span>
              </button>
            </div>
          </div>

          {/* Single Mode Main Content Pane */}
          <div className="projects-single-content">
            {sessions.length === 0 ? (
              <div className="projects-empty-state">
                <div className="projects-empty-card">
                  <IconFolder size={32} />
                  <h3>Project Workspaces</h3>
                  <p>Create AI chats or CLI terminal windows for <strong>{activeProject.name}</strong>.</p>
                  <div className="projects-empty-actions">
                    <button type="button" className="btn-primary" onClick={onNewChat}>
                      <IconPlus size={14} />
                      Start New AI Chat
                    </button>
                    <button type="button" className="btn-secondary" onClick={onNewCli}>
                      <IconTerminal size={14} />
                      Open Terminal CLI
                    </button>
                  </div>
                </div>
              </div>
            ) : activeSession ? (
              renderSessionContent(activeSession)
            ) : (
              <div className="projects-empty-state">
                <p>Select a tab above to continue.</p>
              </div>
            )}
          </div>
        </div>
      ) : (
        /* Multi Mode: Auto-adjusted multi-session workspace */
        <div className="projects-multi-container">
          <MultiSessionWorkspace
            projectPath={activeProject.path}
            sessions={sessions}
            sessionsError={sessionsError}
            activeSessionId={activeSessionId}
            layout={workspaceLayout}
            autoFit={autoFit}
            onAutoFitChange={onSetAutoFit}
            onApplyLayout={onApplyLayout}
            resetKey={resetKey}
            onActivate={(id) => {
              if (onOpenProjectSession) onOpenProjectSession(activeProject.path, id);
              else onOpenSession(id);
            }}
            onFocusSingle={(id) => {
              if (onOpenProjectSession) onOpenProjectSession(activeProject.path, id);
              else onOpenSession(id);
              onWorkspaceMode("single");
            }}
            onCloseSession={onCloseSession}
            onNewChat={onNewChat}
            onNewCli={onNewCli}
            onRetry={onRetrySessions}
            renderSession={renderSessionContent}
          />
        </div>
      )}
    </div>
  );
}
