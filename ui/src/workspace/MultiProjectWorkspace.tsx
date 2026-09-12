import { useCallback, useMemo, useState, type ReactNode } from "react";
import type { ProjectSummary, WorkspaceSession } from "../lib/ipc";
import { MultiSessionWorkspace, type PanelProject } from "./MultiSessionWorkspace";
import type { WorkspaceLayout } from "./layoutPlan";
import {
  cleanProjectPath as cleanPath,
  flattenBoardSessions,
  groupSessionsByProject,
  projectHue,
} from "./multiProjectBoard";

/** Namespaces the board's own saved window order, sizes and layout. */
const BOARD_SCOPE = "*all-projects*";

type MultiProjectWorkspaceProps = {
  projects: ProjectSummary[];
  activeProjectPath: string | null;
  /** Every project's sessions, not only the active one's. `null` while loading. */
  sessions: WorkspaceSession[] | null;
  sessionsError: string | null;
  activeSessionId: string | null;
  /** The live pane for one window. Called for every session on the board. */
  renderSession: (session: WorkspaceSession) => ReactNode;
  onOpenSession: (projectPath: string, sessionId: string) => void;
  onFocusSingle: (projectPath: string, sessionId: string) => void;
  onNewSession: (projectPath: string, kind: "chat" | "cli") => void;
  /** Closing carries the project: a conversation can only be deleted inside its own. */
  onCloseSession: (projectPath: string, sessionId: string) => void;
  onRetry: () => void;
  /** The layout controls are the app's, so Organize drives this board as it does Multi. */
  layout?: WorkspaceLayout;
  autoFit?: boolean;
  onApplyLayout?: (layout: WorkspaceLayout) => void;
  onAutoFitChange?: (fit: boolean) => void;
  resetKey?: number;
};

/**
 * Every project's chats and terminals on one screen, as **windows** rather than tabs.
 *
 * The board draws no chrome of its own. It gathers every project's sessions into one
 * set and hands them to the same window manager the single-project Multi mode uses, so
 * the two modes look and behave identically — a chat from one project and a terminal
 * from another sit side by side, drag, snap and resize exactly alike. The only thing
 * that marks where a window lives is its own title bar, which wears its project's name
 * and colour.
 *
 * Every window is live: a session's commands name their own project (ADR-0051), so a
 * window does not have to wait for its folder to become the active one to run.
 *
 * The layout, the auto-fit switch and every resize shortcut are the shared ones: this
 * board and Multi mode are the same canvas with a different set of windows on it.
 */
export function MultiProjectWorkspace({
  projects,
  activeProjectPath,
  sessions,
  sessionsError,
  activeSessionId,
  renderSession,
  onOpenSession,
  onFocusSingle,
  onNewSession,
  onCloseSession,
  onRetry,
  layout,
  autoFit,
  onApplyLayout,
  onAutoFitChange,
  resetKey,
}: MultiProjectWorkspaceProps) {
  const [focusedId, setFocusedId] = useState<string | null>(null);

  const columns = useMemo(
    () => groupSessionsByProject(projects, sessions ?? []),
    [projects, sessions],
  );

  // Loading has to stay `null` all the way down: the window manager draws its own
  // loading state, and an empty array there would read as "no chats anywhere".
  const boardSessions = useMemo(
    () => (sessions === null ? null : flattenBoardSessions(columns)),
    [sessions, columns],
  );

  const owners = useMemo(() => {
    const map = new Map<string, ProjectSummary>();
    for (const project of projects) map.set(cleanPath(project.path), project);
    return map;
  }, [projects]);

  const ownerOf = useCallback(
    (session: WorkspaceSession): ProjectSummary | null =>
      owners.get(cleanPath(session.project_path)) ?? null,
    [owners],
  );

  const projectFor = useCallback(
    (session: WorkspaceSession): PanelProject | null => {
      const owner = ownerOf(session);
      if (!owner) return null;
      return { name: owner.name, path: owner.path, hue: projectHue(owner.path) };
    },
    [ownerOf],
  );

  /** A window's action needs the project it belongs to, not the one that is active. */
  const withOwner = useCallback(
    (act: (projectPath: string, sessionId: string) => void) =>
      (sessionId: string) => {
        const session = boardSessions?.find((one) => one.id === sessionId);
        const owner = session ? ownerOf(session) : null;
        if (owner) act(owner.path, sessionId);
      },
    [boardSessions, ownerOf],
  );

  // Which window is focused is the board's own business. Every window is live, so a
  // click has no reason to reorganise the app around it — and it must not, because the
  // canvas fires this on the *first* pointer-down of a drag, and switching the active
  // project there would rewrite config and reload the rail mid-gesture. The app's open
  // conversation is moved only when the window already belongs to the active project,
  // where that costs nothing; anything else is an explicit act (the window's "open on
  // its own" button).
  const focused =
    focusedId && boardSessions?.some((one) => one.id === focusedId) ? focusedId : activeSessionId;

  const focusWindow = useCallback(
    (sessionId: string) => {
      setFocusedId(sessionId);
      const session = boardSessions?.find((one) => one.id === sessionId);
      const owner = session ? ownerOf(session) : null;
      if (owner && activeProjectPath && cleanPath(owner.path) === cleanPath(activeProjectPath)
        && sessionId !== activeSessionId) {
        onOpenSession(owner.path, sessionId);
      }
    },
    [boardSessions, ownerOf, onOpenSession, activeProjectPath, activeSessionId],
  );

  const newSessionProject = activeProjectPath ?? columns[0]?.project.path ?? null;

  return (
    <MultiSessionWorkspace
      projectPath={BOARD_SCOPE}
      sessions={boardSessions}
      layout={layout}
      autoFit={autoFit}
      resetKey={resetKey}
      onApplyLayout={onApplyLayout}
      onAutoFitChange={onAutoFitChange}
      sessionsError={sessionsError}
      activeSessionId={focused}
      renderSession={renderSession}
      projectFor={projectFor}
      emptyCopy={{
        title: "No chats in these projects yet",
        hint: "Start a chat or terminal and it opens as a window on this board.",
      }}
      onActivate={focusWindow}
      onFocusSingle={withOwner(onFocusSingle)}
      onCloseSession={withOwner(onCloseSession)}
      onNewChat={() => newSessionProject && onNewSession(newSessionProject, "chat")}
      onNewCli={() => newSessionProject && onNewSession(newSessionProject, "cli")}
      onRetry={onRetry}
    />
  );
}
