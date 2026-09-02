import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AppStatus, ProjectSummary, ToolAvailability, UsageSummary, WorkspaceSession } from "./lib/ipc";
import { api, events } from "./lib/api";
import { TitleBar, type Screen } from "./chrome/TitleBar";
import { Sidebar } from "./chrome/Sidebar";
import { StatusBar } from "./chrome/StatusBar";
import { Chat } from "./screens/Chat";
import { CliView, type CliSession } from "./screens/CliView";
import { release as releaseTerminal, retheme as rethemeTerminals } from "./lib/terminalStore";
import { Research } from "./screens/Research";
import { Automation } from "./screens/Automation";
import { Library } from "./screens/Library";
import { Plugins } from "./screens/Plugins";
import { SettingsModal, type SettingsTab } from "./screens/SettingsModal";
import { ProjectDialog, ProjectStart } from "./screens/ProjectStart";
import { TitleBarCenterControls } from "./chrome/TitleBarCenterControls";
import { RulesPanel } from "./screens/RulesPanel";
import { ReviewChangesModal } from "./screens/ReviewChangesModal";
import { ProjectBrainPanel } from "./screens/ProjectBrainPanel";import { Workbench } from "./workbench/Workbench";
import type { WorkbenchMode } from "./workbench/ModeSwitch";
import { WORKBENCH_ORDER } from "./workbench/ModeSwitch";
import { applyAppearanceToDOM, getAppearanceSettings, onAppearanceChange } from "./lib/appearance";
import { open } from "@tauri-apps/plugin-dialog";
import {
  IconChat,
  IconClose,
  IconGrid,
  IconPlus,
  IconTerminal,
} from "./components/icons";
import { ProviderLogo } from "./components/ProviderLogo";
import { MultiSessionWorkspace } from "./workspace/MultiSessionWorkspace";
import { WorkspaceOrganizer, type WorkspaceLayout } from "./workspace/WorkspaceOrganizer";
import { reconcileSessionOrder } from "./workspace/workspaceState";

/** Workbench sits on the right of the split. It may grow past 50%, but never past
 *  the locked stop: the chat composer (Auto / provider / model / effort / usage)
 *  has to stay on one row. The owner screenshot is that stop. */
const MIN_WORKBENCH_PX = 280;
const MIN_CHAT_PX = 540;
const MAX_WORKBENCH_FRACTION = 0.68;
const getMaxWorkbenchPx = (splitWidth?: number) => {
  const viewport = typeof window !== "undefined" ? window.innerWidth : 1600;
  const split = splitWidth && splitWidth > 0 ? splitWidth : viewport;
  const room = split - MIN_CHAT_PX;
  if (room <= MIN_WORKBENCH_PX) {
    return Math.max(MIN_WORKBENCH_PX, Math.round(split * MAX_WORKBENCH_FRACTION));
  }
  return Math.max(
    MIN_WORKBENCH_PX,
    Math.round(Math.min(split * MAX_WORKBENCH_FRACTION, room)),
  );
};

function clampWorkbenchWidth(width: number, splitWidth?: number) {
  return Math.min(getMaxWorkbenchPx(splitWidth), Math.max(MIN_WORKBENCH_PX, width));
}

function cleanPath(p?: string | null): string {
  if (!p) return "";
  return p
    .replace(/^(\/\/\?\/|\/\/\?|[\\/]{2}\?)[\\/]?/, "")
    .replace(/\\/g, "/")
    .replace(/\/+$/, "")
    .toLowerCase()
    .trim();
}

export default function App() {
  const isDesktopHost = "__TAURI_INTERNALS__" in window;
  const [screen, setScreen] = useState<Screen>("chat");
  const [historyPast, setHistoryPast] = useState<Screen[]>([]);
  const [historyFuture, setHistoryFuture] = useState<Screen[]>([]);
  /// Which way the last navigation went, so the incoming screen enters from the side
  /// it came from. A fade would be cheaper and would lose the sense of place entirely.
  const [travel, setTravel] = useState<"forward" | "back">("forward");
  const [status, setStatus] = useState<AppStatus | null>(null);
  const [usage, setUsage] = useState<UsageSummary | null>(null);
  const [settingsTab, setSettingsTab] = useState<SettingsTab | null>(null);
  const [runningLabel, setRunningLabel] = useState<string | null>(null);

  const [statusError, setStatusError] = useState<string | null>(null);
  const [railCollapsed, setRailCollapsed] = useState(false);
  const [projects, setProjects] = useState<ProjectSummary[] | null>(null);
  const [activeProject, setActiveProject] = useState<ProjectSummary | null>(null);
  const [projectTools, setProjectTools] = useState<ToolAvailability[]>([]);
  const [projectDialogOpen, setProjectDialogOpen] = useState(false);
  /// Which flow the project dialog opens on ("create" and "clone" arrive from the
  /// sidebar's "New project" menu; "open" skips the dialog for the native picker).
  const [projectDialogMode, setProjectDialogMode] = useState<"create" | "clone">("create");
  const [rulesOpen, setRulesOpen] = useState(false);
  const [reviewOpen, setReviewOpen] = useState(false);
  const [reviewTurnTitle, setReviewTurnTitle] = useState<string | null>(null);
  const [brainOpen, setBrainOpen] = useState(false);

  // Appearance initialization & live DOM synchronization
  useEffect(() => {
    applyAppearanceToDOM(getAppearanceSettings());
    rethemeTerminals();
    return onAppearanceChange((settings) => {
      applyAppearanceToDOM(settings);
      // The emulator reads its palette from CSS custom properties once, at construction,
      // so an open terminal has to be told the theme changed.
      rethemeTerminals();
    });
  }, []);

  // The workbench starts closed on every launch — see ProjectHeader for why — but the
  // pane's width and last mode are remembered, so reopening it lands where it was.
  const [workbenchOpen, setWorkbenchOpen] = useState(false);
  const [workbenchWidth, setWorkbenchWidth] = useState(() => {
    const saved = Number(window.localStorage.getItem("bhippi-workbench-width"));
    return Number.isFinite(saved) && saved >= MIN_WORKBENCH_PX
      ? clampWorkbenchWidth(saved)
      : 720;
  });
  const [dragging, setDragging] = useState(false);
  const [workbenchMode, setWorkbenchMode] = useState<WorkbenchMode>(() => {
    const saved = window.localStorage.getItem("bhippi-workbench-mode");
    return saved === "browser" || saved === "engine" ? saved : "editor";
  });
  const splitRef = useRef<HTMLDivElement | null>(null);

  const [activeConversationId, setActiveConversationId] = useState<string | null>(null);

  const [workspaceMode, setWorkspaceMode] = useState<"single" | "multi">(() =>
    window.localStorage.getItem("bhippi-workspace-mode") === "multi" ? "multi" : "single",
  );

  const [draggingTab, setDraggingTab] = useState<string | null>(null);
  const [dropTargetTab, setDropTargetTab] = useState<string | null>(null);
  const tabSwitchTimerRef = useRef<number | null>(null);
  const tabBarRef = useRef<HTMLDivElement | null>(null);

  /// Every project's sessions for the workspace rail (W4 §4.2) — deliberately not
  /// scoped to the active project, because each project card needs its own.
  const [workspaceSessions, setWorkspaceSessions] = useState<WorkspaceSession[] | null>(null);
  const [workspaceSessionsError, setWorkspaceSessionsError] = useState<string | null>(null);

  useEffect(() => {
    window.localStorage.setItem("bhippi-workspace-mode", workspaceMode);
  }, [workspaceMode]);

  const [workspaceLayout, setWorkspaceLayout] = useState<WorkspaceLayout>(() => {
    try {
      const v = window.localStorage.getItem("bhippi-workspace-layout");
      return v === "adaptive" || v === "smart" ? v : "balanced";
    } catch {
      return "balanced";
    }
  });

  const [workspaceAutoFit, setWorkspaceAutoFit] = useState<boolean>(() => {
    try {
      const v = window.localStorage.getItem("bhippi-workspace-autofit");
      return v === null ? true : v === "true";
    } catch {
      return true;
    }
  });

  const [panelOrder, setPanelOrder] = useState<string[]>(() => {
    try {
      const saved = window.localStorage.getItem("bhippi-tab-order");
      const parsed: unknown = saved ? JSON.parse(saved) : [];
      return Array.isArray(parsed)
        ? parsed.filter((value): value is string => typeof value === "string")
        : [];
    } catch {
      return [];
    }
  });

  const [workspaceSizesResetKey, setWorkspaceSizesResetKey] = useState(0);

  useEffect(() => {
    window.localStorage.setItem("bhippi-workspace-layout", workspaceLayout);
  }, [workspaceLayout]);

  useEffect(() => {
    window.localStorage.setItem("bhippi-workspace-autofit", String(workspaceAutoFit));
  }, [workspaceAutoFit]);


  const [cliSessions, setCliSessions] = useState<CliSession[]>(() => {
    try {
      const saved = window.localStorage.getItem("bhippi-cli-sessions");
      return saved ? JSON.parse(saved) : [];
    } catch {
      return [];
    }
  });

  useEffect(() => {
    window.localStorage.setItem("bhippi-cli-sessions", JSON.stringify(cliSessions));
  }, [cliSessions]);

  /// Engine sessions (all projects) merged with the locally-owned CLI sessions, so the
  /// rail's project cards account for both. CLI sessions live in localStorage only; the
  /// engine reports AiChat rows and the shell identity is a UI-side shape.
  const allSessions = useMemo<WorkspaceSession[] | null>(() => {
    if (workspaceSessions === null) return null;
    const list: WorkspaceSession[] = [...workspaceSessions];
    for (const s of cliSessions) {
      list.push({
        id: s.id,
        project_path: (s as { projectPath?: string }).projectPath ?? activeProject?.path ?? "",
        kind: "cli",
        title: s.title,
        provider: null,
        provider_label: null,
        status: "idle",
        created_at: s.createdAt,
        updated_at: s.createdAt,
        // A PTY has no turns; the rail's count is meaningless for a shell.
        turn_count: 0,
      });
    }
    return list;
  }, [workspaceSessions, cliSessions, activeProject]);

  // Activity changes updated_at, but it must never change the visual tab order.
  // Reconcile only real additions and removals, preserving every existing position.
  useEffect(() => {
    if (!allSessions) return;
    setPanelOrder((current) =>
      reconcileSessionOrder(current, allSessions.map((session) => session.id)),
    );
  }, [allSessions]);

  useEffect(() => {
    window.localStorage.setItem("bhippi-tab-order", JSON.stringify(panelOrder));
  }, [panelOrder]);

  /// Ordered list of sessions for the single tab mode.
  /// Filters to active project and preserves panelOrder, appending newly created sessions.
  const singleTabSessions = useMemo<WorkspaceSession[]>(() => {
    if (!allSessions) return [];
    const inProject = activeProject?.path
      ? allSessions.filter((s) => cleanPath(s.project_path) === cleanPath(activeProject.path))
      : allSessions;

    const ordered: WorkspaceSession[] = [];
    const map = new Map(inProject.map((s) => [s.id, s]));

    for (const id of panelOrder) {
      const s = map.get(id);
      if (s) {
        ordered.push(s);
        map.delete(id);
      }
    }
    for (const remaining of map.values()) {
      ordered.push(remaining);
    }

    return ordered;
  }, [allSessions, activeProject?.path, panelOrder]);

  const handleReorderTabs = useCallback(
    (draggedId: string, targetId: string) => {
      if (!draggedId || draggedId === targetId) {
        setDraggingTab(null);
        setDropTargetTab(null);
        return;
      }
      const currentList = singleTabSessions.map((s) => s.id);
      const dragIdx = currentList.indexOf(draggedId);
      const targetIdx = currentList.indexOf(targetId);
      if (dragIdx === -1 || targetIdx === -1) {
        setDraggingTab(null);
        setDropTargetTab(null);
        return;
      }
      const nextOrder = [...currentList];
      nextOrder.splice(dragIdx, 1);
      nextOrder.splice(targetIdx, 0, draggedId);
      setPanelOrder(nextOrder);
      setActiveConversationId(draggedId);
      setDraggingTab(null);
      setDropTargetTab(null);
    },
    [singleTabSessions],
  );

  useEffect(() => {
    if (workspaceMode === "single" && !activeConversationId && singleTabSessions.length > 0) {
      setActiveConversationId(singleTabSessions[0].id);
    }
  }, [workspaceMode, activeConversationId, singleTabSessions]);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await api.status());
      setStatusError(null);
    } catch (error) {
      setStatusError(String((error as Error).message ?? error));
    }
  }, []);


  // The gauge reads today's window; the panel asks for wider ones itself.
  const refreshUsage = useCallback(async (refreshAccounts = false) => {
    try {
      const [u] = await Promise.all([
        api.usage("day", refreshAccounts),
        refreshStatus(),
      ]);
      setUsage(u);
    } catch {
      // A missing ledger must never take the chrome down with it — the meter falls
      // back to its idle face and the next turn tries again.
      setUsage(null);
    }
  }, [refreshStatus]);

  /// Pulls every project's sessions for the rail. Statuses flip live (queued→streaming→
  /// done), so this refetches on chat events — coalesced so a burst of deltas does not
  /// hammer the backend.
  const refreshWorkspaceSessions = useCallback(async () => {
    try {
      const list = await api.workspaceSessions();
      setWorkspaceSessions(list);
      setWorkspaceSessionsError(null);
    } catch (loadError) {
      console.error(loadError);
      setWorkspaceSessionsError(String((loadError as Error).message ?? loadError));
    }
  }, []);

  const sessionsRefreshTimer = useRef<number | undefined>(undefined);
  const scheduleSessionsRefresh = useCallback(() => {
    window.clearTimeout(sessionsRefreshTimer.current);
    sessionsRefreshTimer.current = window.setTimeout(() => {
      void refreshWorkspaceSessions();
    }, 120);
  }, [refreshWorkspaceSessions]);

  const refreshProjects = useCallback(async () => {
    try {
      const rows = await api.projects();
      setProjects(rows);
      setActiveProject((current) => {
        if (current) return rows.find((row) => cleanPath(row.path) === cleanPath(current.path)) ?? null;
        return rows.find((row) => row.active) ?? null;
      });
    } catch (projectError) {
      setProjects([]);
      setStatusError(String((projectError as Error).message ?? projectError));
    }
  }, []);

  const openConversation = useCallback((id: string) => setActiveConversationId(id), []);

  /// Removes a conversation and moves off it only when it was the one on screen.
  const deleteConversation = useCallback(
    async (id: string) => {
      if (id.startsWith("cli-")) {
        // Deleting the session is the only thing that ends its shell: the terminal
        // deliberately survives unmounting so switching tab does not kill a running job.
        releaseTerminal(id);
        setCliSessions((prev) => prev.filter((s) => s.id !== id));
        setActiveConversationId((current) => (current === id ? null : current));
        return;
      }
      try {
        setWorkspaceSessions((prev) => (prev ? prev.filter((s) => s.id !== id) : prev));
        const remaining = await api.deleteConversation(id);
        setActiveConversationId((current) =>
          current === id ? (remaining[0]?.id ?? null) : current,
        );
        await refreshWorkspaceSessions();
      } catch (deleteError) {
        setStatusError(String((deleteError as Error).message ?? deleteError));
        await refreshWorkspaceSessions();
      }
    },
    [refreshWorkspaceSessions],
  );

  const chooseProject = useCallback(
    async (project: ProjectSummary, options?: { preserveConversation?: boolean }) => {
      try {
        const isSameProject = activeProject && cleanPath(activeProject.path) === cleanPath(project.path);
        const selected = await api.selectProject(project.path);
        setActiveProject(selected);
        setScreen("chat");
        await Promise.all([refreshProjects(), refreshWorkspaceSessions()]);
        if (isSameProject && activeConversationId) {
          return;
        }
        if (options?.preserveConversation) {
          return;
        }
        const existing = await api.conversations();
        if (!existing || existing.length === 0) {
          const meta = await api.newConversation();
          setActiveConversationId(meta.id);
          await refreshWorkspaceSessions();
        } else {
          setActiveConversationId((curr) => {
            if (curr && existing.some((c) => c.id === curr)) return curr;
            return existing[0].id;
          });
        }
      } catch (projectError) {
        setStatusError(String((projectError as Error).message ?? projectError));
      }
    },
    [activeProject, activeConversationId, refreshProjects, refreshWorkspaceSessions],
  );

  /// "New project" from the rail. `open` goes straight to the Windows folder picker —
  /// pick a folder, it becomes the active project. `create` and `clone` open the same
  /// dialog on the matching flow, both of which browse with the native picker too.
  const handleNewProject = useCallback(
    async (kind: "open" | "create" | "clone") => {
      if (kind === "open") {
        try {
          const selected = await open({
            directory: true,
            multiple: false,
            title: "Choose a project folder",
          });
          if (typeof selected !== "string" || !selected) return;
          await chooseProject(await api.addProject(selected));
        } catch (openError) {
          setStatusError(String((openError as Error).message ?? openError));
        }
        return;
      }
      setProjectDialogMode(kind);
      setProjectDialogOpen(true);
    },
    [chooseProject],
  );

  const newConversation = useCallback(async () => {
    try {
      if (!activeProject && projects && projects.length > 0) {
        await chooseProject(projects[0]);
      }
      const meta = await api.newConversation();
      await refreshWorkspaceSessions();
      setActiveConversationId(meta.id);
      setScreen("chat");
      setStatusError(null);
    } catch (newError) {
      setStatusError(String((newError as Error).message ?? newError));
    }
    scheduleSessionsRefresh();
  }, [activeProject, projects, chooseProject, refreshWorkspaceSessions, scheduleSessionsRefresh]);

  /// Opens a session that may belong to a different project than the active one — the
  /// rail shows every project, so clicking a chip from another project switches first.
  const openSession = useCallback(
    async (projectPath: string, sessionId: string) => {
      if (!activeProject || cleanPath(activeProject.path) !== cleanPath(projectPath)) {
        const target = (projects ?? []).find((row) => cleanPath(row.path) === cleanPath(projectPath));
        if (!target) return;
        await chooseProject(target, { preserveConversation: true });
      }
      openConversation(sessionId);
      if (screen !== "chat") setScreen("chat");
    },
    [activeProject, projects, chooseProject, openConversation, screen],
  );

  /// Creates a CLI session. `projectPathOverride` lets a session be created for a
  /// project other than the active one (the rail's per-card `+` does this).
  const newCliSession = useCallback(
    (shell = "cmd", projectPathOverride?: string) => {
      const targetPath = projectPathOverride ?? activeProject?.path;
      if (!targetPath) return;
      const shellLabels: Record<string, string> = {
        cmd: "Command Prompt",
        powershell: "PowerShell",
      };
      const label = shellLabels[shell] ?? shell;
      const id = `cli-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
      const newSession: CliSession = {
        id,
        title: `CLI: ${label}`,
        shell,
        history: [],
        createdAt: new Date().toISOString(),
        projectPath: targetPath,
      };
      setCliSessions((prev) => [newSession, ...prev]);
      setActiveConversationId(id);
      if (screen !== "chat") setScreen("chat");
    },
    [activeProject, screen],
  );

  /// Same for creating: new sessions belong to the project they are created in.
  /// `kind` picks the session type and `shell` selects the CLI shell (when CLI).
  const newSessionInProject = useCallback(
    async (projectPath: string, kind: "chat" | "cli" = "chat", shell?: string) => {
      const isSame = activeProject && cleanPath(activeProject.path) === cleanPath(projectPath);
      if (!isSame) {
        const target = (projects ?? []).find((row) => cleanPath(row.path) === cleanPath(projectPath));
        if (target) {
          await chooseProject(target, { preserveConversation: true });
        }
      }
      if (kind === "cli") newCliSession(shell ?? "powershell", projectPath);
      else await newConversation();
    },
    [activeProject, projects, chooseProject, newConversation, newCliSession],
  );

  /// Removes a project from the app — the folder stays on disk. Its sessions leave
  /// the rail and the engine with it, and a removed active project clears the board.
  const removeProject = useCallback(
    async (projectPath: string) => {
      try {
        const remaining = await api.forgetProject(projectPath);
        setProjects(remaining);
        setCliSessions((prev) => {
          const leaving = prev.filter(
            (session) => cleanPath(session.projectPath) === cleanPath(projectPath),
          );
          for (const session of leaving) releaseTerminal(session.id);
          return prev.filter(
            (session) => cleanPath(session.projectPath) !== cleanPath(projectPath),
          );
        });
        if (cleanPath(activeProject?.path) === cleanPath(projectPath)) {
          const next = remaining[0] ?? null;
          setActiveProject(next);
          if (next) {
            await chooseProject(next);
          } else {
            setActiveConversationId(null);
          }
        }
        await refreshWorkspaceSessions();
      } catch (projectError) {
        setStatusError(String((projectError as Error).message ?? projectError));
      }
    },
    [activeProject, chooseProject, refreshWorkspaceSessions],
  );

  // Global Ctrl+N / Cmd+N keyboard shortcut to create a new chat anytime
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n" && !e.shiftKey && !e.altKey) {
        e.preventDefault();
        void newConversation();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [newConversation]);

  const openBrowserToUrl = useCallback((targetUrl?: string) => {
    setWorkbenchOpen(true);
    setWorkbenchMode("browser");
    if (targetUrl) {
      window.dispatchEvent(
        new CustomEvent("bhippi:navigate-browser", { detail: { url: targetUrl } }),
      );
    }
  }, []);

  /// One place that turns a plugin card's `target` into a real destination. The
  /// catalogue in Rust names the route; anything unrecognised is ignored rather than
  /// opening the wrong thing.
  const openPluginTarget = useCallback(
    (target: string) => {
      const [kind, rest] = [target.slice(0, target.indexOf(":")), target.slice(target.indexOf(":") + 1)];
      switch (kind) {
        case "screen":
          setScreen(rest as Screen);
          return;
        case "workbench":
          setWorkbenchOpen(true);
          setWorkbenchMode(rest as WorkbenchMode);
          return;
        case "settings":
          setSettingsTab(rest as SettingsTab);
          return;
        case "panel":
          if (rest === "brain") setBrainOpen(true);
          if (rest === "review") {
            setReviewTurnTitle(null);
            setReviewOpen(true);
          }
          return;
        case "url":
          openBrowserToUrl(rest);
          return;
        default:
          console.warn("unknown plugin target", target);
      }
    },
    [openBrowserToUrl],
  );

  useEffect(() => {
    // The colour scheme is applied by applyAppearanceToDOM above. Writing
    // data-color-scheme a second time from a raw localStorage read used to
    // reintroduce retired ids ("frosted-glass", "gradient") past the migration.
    if (!isDesktopHost) {
      setStatus({
        version: "web-preview",
        active_provider: "Web preview",
        active_provider_id: "web-preview",
        demo_mode: true,
        providers: [],
        chat_options: [],
        tokens_today: 0,
        queue_depth: 0,
        last_model: {},
        last_provider: null,
      });
      setProjects([]);
      setWorkspaceSessions([]);
      setWorkspaceSessionsError(null);
      setProjectTools([]);
      setStatusError(null);
      return;
    }
    void refreshStatus();
    void refreshUsage();
    void refreshWorkspaceSessions();
    void refreshProjects();
    void api.projectTools().then(setProjectTools).catch(() => setProjectTools([]));
    const timer = window.setInterval(() => {
      void refreshStatus();
      void refreshUsage();
    }, 15_000);
    const offProviders = events.providersChanged.listen(() => {
      void refreshStatus();
      void refreshUsage();
    });
    // Every finished turn is a new ledger row, so the ring moves the moment it lands.
    const offTurn = events.chatTurnDone.listen(() => {
      void refreshUsage();
      scheduleSessionsRefresh();
    });
    // Every turn start changes that session's status on the rail (idle→running), so the
    // workspace rail keeps pace with the agent it is rendering.
    const offThinking = events.chatThinking.listen(() => scheduleSessionsRefresh());
    return () => {
      window.clearInterval(timer);
      void offProviders.then((unlisten) => unlisten());
      void offTurn.then((unlisten) => unlisten());
      void offThinking.then((unlisten) => unlisten());
    };
  }, [isDesktopHost, refreshStatus, refreshUsage, refreshWorkspaceSessions, refreshProjects, scheduleSessionsRefresh]);

  // Screen history for the sidebar's back / forward pair.
  const navigate = useCallback(
    (next: Screen) => {
      if (next === screen) return;
      setHistoryPast((past) => [...past, screen]);
      setHistoryFuture([]);
      setTravel("forward");
      setScreen(next);
    },
    [screen],
  );

  const goBack = useCallback(() => {
    setHistoryPast((past) => {
      if (past.length === 0) return past;
      const previous = past[past.length - 1];
      setHistoryFuture((future) => [screen, ...future]);
      setTravel("back");
      setScreen(previous);
      return past.slice(0, -1);
    });
  }, [screen]);

  // The splitter listens on the window rather than on itself: a fast drag outruns the
  // 6px handle, and without window-level listeners the pane stops following the pointer
  // the moment it leaves that strip.
  useEffect(() => {
    if (!dragging) return;
    const onMove = (event: PointerEvent) => {
      const bounds = splitRef.current?.getBoundingClientRect();
      if (!bounds || bounds.width === 0) return;
      const px = bounds.right - event.clientX;
      setWorkbenchWidth(clampWorkbenchWidth(px, bounds.width));
    };
    const onUp = () => setDragging(false);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
  }, [dragging]);

  useEffect(() => {
    window.localStorage.setItem("bhippi-workbench-width", String(workbenchWidth));
  }, [workbenchWidth]);


  // Re-clamp when the window or sidebar changes so a saved width cannot push the
  // composer under the workbench after a resize.
  useEffect(() => {
    if (!workbenchOpen) return;
    const clampToSplit = () => {
      const split = splitRef.current?.getBoundingClientRect().width;
      setWorkbenchWidth((width) => clampWorkbenchWidth(width, split));
    };
    clampToSplit();
    window.addEventListener("resize", clampToSplit);
    return () => window.removeEventListener("resize", clampToSplit);
  }, [workbenchOpen, railCollapsed]);

  // Ctrl/Cmd+B toggles the workbench, and Ctrl/Cmd+' cycles its mode — both reachable
  // without leaving the composer, which is where the hands already are. Ctrl/Cmd+3
  // jumps straight to the Engine pane (plan §7.9).
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || !activeProject) return;
      if (event.key.toLowerCase() === "b") {
        event.preventDefault();
        setWorkbenchOpen((open) => !open);
      }
      if (event.key === "'") {
        event.preventDefault();
        setWorkbenchOpen(true);
        setWorkbenchMode((mode) => {
          const index = WORKBENCH_ORDER.indexOf(mode);
          return WORKBENCH_ORDER[(index + 1) % WORKBENCH_ORDER.length];
        });
      }
      if (event.key === "3") {
        event.preventDefault();
        setWorkbenchOpen(true);
        setWorkbenchMode("engine");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [activeProject]);

  useEffect(() => {
    window.localStorage.setItem("bhippi-workbench-mode", workbenchMode);
  }, [workbenchMode]);

  const goForward = useCallback(() => {
    setHistoryFuture((future) => {
      if (future.length === 0) return future;
      const [next, ...rest] = future;
      setHistoryPast((past) => [...past, screen]);
      setScreen(next);
      return rest;
    });
  }, [screen]);

  return (
    <div className="shell">
      <TitleBar
        demoMode={status?.demo_mode ?? false}
        onOpenSettings={() => setSettingsTab("Providers")}
        settingsBadge={false}
        centerAction={
          activeProject ? (
            <TitleBarCenterControls
              workspaceMode={workspaceMode}
              onWorkspaceMode={setWorkspaceMode}
              workbenchOpen={workbenchOpen}
              onToggleWorkbench={() => setWorkbenchOpen((open) => !open)}
              workbenchMode={workbenchMode}
              onWorkbenchMode={(mode) => {
                setWorkbenchMode(mode);
                setWorkbenchOpen(true);
              }}
              organizeAction={
                screen === "chat" ? (
                  <WorkspaceOrganizer
                    layout={workspaceLayout}
                    onApplyLayout={(l) => {
                      setWorkspaceMode("multi");
                      setWorkspaceLayout(l);
                      setWorkspaceSizesResetKey((k) => k + 1);
                    }}
                    autoFit={workspaceAutoFit}
                    onToggleAutoFit={() => setWorkspaceAutoFit((v) => !v)}
                    sessions={
                      allSessions?.filter(
                        (session) => cleanPath(session.project_path) === cleanPath(activeProject.path),
                      ) ?? []
                    }
                    activeSessionId={activeConversationId}
                    onFocusSession={openConversation}
                    onCloseSession={(id) => void deleteConversation(id)}
                    onEnsureMultiMode={() => setWorkspaceMode("multi")}
                    isMultiMode={workspaceMode === "multi"}
                  />
                ) : null
              }
            />
          ) : null
        }
      />

      <div className="body">
        <Sidebar
          screen={screen}
          onScreen={navigate}
          onBack={goBack}
          onForward={goForward}
          canBack={historyPast.length > 0}
          canForward={historyFuture.length > 0}
          collapsed={railCollapsed}
          onToggle={() => setRailCollapsed((open) => !open)}
          sessions={allSessions}
          sessionsError={workspaceSessionsError}
          activeConversationId={activeConversationId}
          onDeleteConversation={(id) => void deleteConversation(id)}
          onOpenSession={(projectPath, sessionId) => void openSession(projectPath, sessionId)}
          onNewSessionInProject={(projectPath, kind, shell) =>
            void newSessionInProject(projectPath, kind, shell)
          }
          onRemoveProject={(projectPath) => void removeProject(projectPath)}
          demoMode={status?.demo_mode ?? false}
          project={activeProject}
          projects={projects ?? []}
          onSelectProject={(project) => void chooseProject(project)}
          onNewProject={(kind) => void handleNewProject(kind)}
          onOpenSettings={(tab) => setSettingsTab(tab ?? "Profile")}
          onRetrySessions={() => void refreshWorkspaceSessions()}
          onOpenRules={() => setRulesOpen(true)}
          onOpenReview={() => {
            setReviewTurnTitle(null);
            setReviewOpen(true);
          }}
          onOpenBrain={() => setBrainOpen(true)}
          tools={projectTools}
          onReorderSession={(fromId, toId) => handleReorderTabs(fromId, toId)}
        />

        <div className="workspace-main">
          {!activeProject ? (
            <main className="project-gate">
              <ProjectStart
                projects={projects}
                tools={projectTools}
                onProject={(project) => void chooseProject(project)}
                onRefresh={() => void refreshProjects()}
              />
            </main>
          ) : (
          <div
            className={`workspace-split${workbenchOpen ? " with-workbench" : ""}${dragging ? " dragging" : ""}`}
            ref={splitRef}
          >
          <main className={`screen travel-${travel}`} key={`${screen}:${activeProject.path}`}>
          {screen === "chat" ? (
            workspaceMode === "multi" ? (
              <MultiSessionWorkspace
                projectPath={activeProject.path}
                sessions={allSessions?.filter(
                  (session) => cleanPath(session.project_path) === cleanPath(activeProject.path),
                ) ?? allSessions}
                sessionsError={workspaceSessionsError}
                activeSessionId={activeConversationId}
                layout={workspaceLayout}
                autoFit={workspaceAutoFit}
                onAutoFitChange={setWorkspaceAutoFit}
                resetKey={workspaceSizesResetKey}
                onActivate={openConversation}
                onFocusSingle={(sessionId) => {
                  openConversation(sessionId);
                  setWorkspaceMode("single");
                }}
                onNewChat={() => void newConversation()}
                onNewCli={() => newCliSession("cmd")}
                onRetry={() => void refreshWorkspaceSessions()}
                onCloseSession={(sessionId) => void deleteConversation(sessionId)}
                renderSession={(session) => {
                  if (session.kind === "cli") {
                    const currentCli = cliSessions.find((row) => row.id === session.id);
                    return currentCli ? (
                      <CliView
                        key={currentCli.id}
                        session={currentCli}
                        projectPath={activeProject.path}
                        onUpdateSession={(updated) => {
                          setCliSessions((prev) =>
                            prev.map((row) => (row.id === updated.id ? updated : row)),
                          );
                        }}
                      />
                    ) : null;
                  }
                  return (
                    <Chat
                      key={session.id}
                      onRunningChange={(label) => {
                        if (session.id === activeConversationId) setRunningLabel(label);
                      }}
                      chatOptions={status?.chat_options ?? []}
                      defaultProviderId={status?.last_provider ?? status?.active_provider_id ?? null}
                      lastModel={status?.last_model ?? {}}
                      activeId={session.id}
                      onOpenConversation={openConversation}
                      onConversationsChanged={() => void refreshWorkspaceSessions()}
                      project={activeProject}
                      projects={projects ?? []}
                      onSelectProject={(p) => void chooseProject(p)}
                      onOpenReview={(title) => {
                        setReviewTurnTitle(title ?? null);
                        setReviewOpen(true);
                      }}
                      usage={usage}
                      onManageUsage={() => setSettingsTab("Usage")}
                      onOpenSettings={(tab) => setSettingsTab(tab ?? "Audio & Voice")}
                      onNewConversation={() => void newConversation()}
                      onCloseConversation={() => void deleteConversation(session.id)}
                      onOpenBrowser={openBrowserToUrl}
                      onRefreshUsage={() => refreshUsage(true)}
                    />
                  );
                }}
              />
            ) : (
              <div className="single-tab-mode">
                <div
                  className={`single-tab-bar${draggingTab ? " dragging-tab" : ""}`}
                  ref={tabBarRef}
                >
                  <div
                    className="single-tab-list"
                    role="tablist"
                    aria-label="Project sessions"
                    onWheel={(event) => {
                      const rail = event.currentTarget;
                      if (
                        rail.scrollWidth > rail.clientWidth &&
                        Math.abs(event.deltaY) > Math.abs(event.deltaX)
                      ) {
                        event.preventDefault();
                        rail.scrollLeft += event.deltaY;
                      }
                    }}
                    onDragOverCapture={(event) => {
                      if (!draggingTab) return;
                      const rail = event.currentTarget;
                      const box = rail.getBoundingClientRect();
                      const edge = 48;
                      if (event.clientX < box.left + edge) rail.scrollLeft -= 24;
                      if (event.clientX > box.right - edge) rail.scrollLeft += 24;
                    }}
                  >
                    {singleTabSessions.map((session) => {
                      const isActive = session.id === activeConversationId;
                      const isCli = session.kind === "cli";
                      const title = isCli
                        ? (cliSessions.find((s) => s.id === session.id)?.title.replace(/^CLI:\s*/, "") ?? "Terminal")
                        : session.title;
                      const providerIcon = isCli ? (
                        <IconTerminal size={13} />
                      ) : session.provider ? (
                        <ProviderLogo id={session.provider} size={13} />
                      ) : (
                        <IconChat size={13} />
                      );

                      return (
                        <div
                          key={session.id}
                          className={`single-tab-item${isActive ? " active" : ""}${
                            draggingTab === session.id ? " dragging" : ""
                          }${
                            dropTargetTab === session.id ? " drop-target" : ""
                          }`}
                          draggable
                          onClick={() => openConversation(session.id)}
                          onDragStart={(e) => {
                            setDraggingTab(session.id);
                            e.dataTransfer.setData("text/plain", session.id);
                            e.dataTransfer.effectAllowed = "move";
                          }}
                          onDragEnd={() => {
                            if (tabSwitchTimerRef.current !== null) {
                              window.clearTimeout(tabSwitchTimerRef.current);
                              tabSwitchTimerRef.current = null;
                            }
                            setDraggingTab(null);
                            setDropTargetTab(null);
                          }}
                          onDragOver={(e) => {
                            e.preventDefault();
                            e.dataTransfer.dropEffect = "move";
                            if (draggingTab && draggingTab !== session.id) {
                              if (dropTargetTab !== session.id) {
                                setDropTargetTab(session.id);
                              }
                              // While dragging across tabs, switch to the hovered tab after a brief pause
                              if (tabSwitchTimerRef.current === null) {
                                tabSwitchTimerRef.current = window.setTimeout(() => {
                                  openConversation(session.id);
                                  tabSwitchTimerRef.current = null;
                                }, 300);
                              }
                            }
                          }}
                          onDragLeave={() => {
                            if (tabSwitchTimerRef.current !== null) {
                              window.clearTimeout(tabSwitchTimerRef.current);
                              tabSwitchTimerRef.current = null;
                            }
                            if (dropTargetTab === session.id) {
                              setDropTargetTab(null);
                            }
                          }}
                          onDrop={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            if (tabSwitchTimerRef.current !== null) {
                              window.clearTimeout(tabSwitchTimerRef.current);
                              tabSwitchTimerRef.current = null;
                            }
                            if (draggingTab && draggingTab !== session.id) {
                              handleReorderTabs(draggingTab, session.id);
                            } else {
                              setDraggingTab(null);
                              setDropTargetTab(null);
                            }
                          }}
                          title={
                            dropTargetTab === session.id
                              ? `Drop to move ${title} here`
                              : title
                          }
                          role="tab"
                          aria-selected={isActive}
                          tabIndex={isActive ? 0 : -1}
                          data-session-id={session.id}
                          onKeyDown={(event) => {
                            if (event.key === "Enter" || event.key === " ") {
                              event.preventDefault();
                              openConversation(session.id);
                            }
                            if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
                              event.preventDefault();
                              const tabs = Array.from(
                                event.currentTarget.parentElement?.querySelectorAll<HTMLElement>(
                                  ".single-tab-item",
                                ) ?? [],
                              );
                              const index = tabs.indexOf(event.currentTarget);
                              const step = event.key === "ArrowLeft" ? -1 : 1;
                              const next = tabs[(index + step + tabs.length) % tabs.length];
                              const nextId = next?.dataset.sessionId;
                              if (next && nextId) {
                                next.focus();
                                openConversation(nextId);
                              }
                            }
                          }}
                        >
                          <span className="single-tab-icon">{providerIcon}</span>
                          <span className="single-tab-title">{title}</span>
                          <button
                            type="button"
                            className="single-tab-close"
                            title="Close tab"
                            aria-label={`Close tab ${title}`}
                            onClick={(e) => {
                              e.stopPropagation();
                              void deleteConversation(session.id);
                            }}
                          >
                            <IconClose size={11} />
                          </button>
                        </div>
                      );
                    })}

                    <button
                      type="button"
                      className="single-tab-new-btn"
                      onClick={() => void newConversation()}
                      title="New chat tab (Ctrl+T)"
                      aria-label="New chat tab"
                    >
                      <IconPlus size={13} />
                    </button>
                  </div>

                  {dropTargetTab ? (
                    <span className="single-tab-drop-hint" role="status" aria-live="polite">
                      Drop to move tab here
                    </span>
                  ) : null}

                  <div className="single-tab-tools">
                    <button
                      type="button"
                      className="single-tab-tool-btn"
                      onClick={() => setWorkspaceMode("multi")}
                      title="Switch to split-screen workspaces"
                      aria-label="Switch to workspaces"
                    >
                      <IconGrid size={12} />
                      <span>Workspaces</span>
                    </button>
                  </div>
                </div>

                <div className="single-tab-content">
                  {activeConversationId?.startsWith("cli-") ? (
                    (() => {
                      const currentCli = cliSessions.find((s) => s.id === activeConversationId);
                      return currentCli ? (
                        <CliView
                          key={currentCli.id}
                          session={currentCli}
                          projectPath={activeProject.path}
                          onUpdateSession={(updated) => {
                            setCliSessions((prev) =>
                              prev.map((s) => (s.id === updated.id ? updated : s)),
                            );
                          }}
                        />
                      ) : null;
                    })()
                  ) : (
                    <Chat
                      key={activeConversationId ?? "empty"}
                      onRunningChange={setRunningLabel}
                      chatOptions={status?.chat_options ?? []}
                      defaultProviderId={status?.last_provider ?? status?.active_provider_id ?? null}
                      lastModel={status?.last_model ?? {}}
                      activeId={activeConversationId}
                      onOpenConversation={openConversation}
                      onConversationsChanged={() => void refreshWorkspaceSessions()}
                      project={activeProject}
                      projects={projects ?? []}
                      onSelectProject={(p) => void chooseProject(p)}
                      onOpenReview={(title) => {
                        setReviewTurnTitle(title ?? null);
                        setReviewOpen(true);
                      }}
                      usage={usage}
                      onManageUsage={() => setSettingsTab("Usage")}
                      onOpenSettings={(tab) => setSettingsTab(tab ?? "Audio & Voice")}
                      onOpenBrowser={openBrowserToUrl}
                      onRefreshUsage={() => refreshUsage(true)}
                    />
                  )}
                </div>
              </div>
            )
          ) : screen === "research" ? (
            <Research />
          ) : screen === "automation" ? (
            <Automation />
          ) : screen === "plugins" ? (
            <Plugins onOpenTarget={openPluginTarget} />
          ) : (
            <Library />
          )}
          </main>

          {workbenchOpen ? (
            <>
              <div
                className="workspace-splitter"
                role="separator"
                aria-orientation="vertical"
                aria-label="Resize the workbench"
                tabIndex={0}
                onPointerDown={(event) => {
                  event.preventDefault();
                  setDragging(true);
                }}
                onKeyDown={(event) => {
                  if (event.key === "ArrowLeft") {
                    const split = splitRef.current?.getBoundingClientRect().width;
                    setWorkbenchWidth((value) => clampWorkbenchWidth(value + 40, split));
                  }
                  if (event.key === "ArrowRight") {
                    setWorkbenchWidth((value) => Math.max(MIN_WORKBENCH_PX, value - 30));
                  }
                }}
              >
                <i aria-hidden="true" />
              </div>
              <div className="workspace-workbench" style={{ width: `${workbenchWidth}px` }}>
                <Workbench
                  projectPath={activeProject.path}
                  mode={workbenchMode}
                  onMode={setWorkbenchMode}
                  onClose={() => setWorkbenchOpen(false)}
                  modalOpen={Boolean(
                    settingsTab !== null ||
                    reviewOpen ||
                    rulesOpen ||
                    projectDialogOpen ||
                    brainOpen
                  )}
                />
              </div>
            </>
          ) : null}
          </div>
          )}
        </div>
      </div>

      <StatusBar
        status={status}
        usage={usage}
        error={statusError}
        runningLabel={runningLabel}
        onManageUsage={() => setSettingsTab("Usage")}
      />

      {projectDialogOpen ? (
        <ProjectDialog
          initialMode={projectDialogMode}
          onClose={() => setProjectDialogOpen(false)}
          onCreated={(project) => {
            setProjectDialogOpen(false);
            void chooseProject(project);
          }}
        />
      ) : null}

      {rulesOpen ? <RulesPanel onClose={() => setRulesOpen(false)} /> : null}

      {reviewOpen ? (
        <ReviewChangesModal
          open={reviewOpen}
          turnTitle={reviewTurnTitle}
          workspacePath={activeProject?.path}
          onClose={() => setReviewOpen(false)}
        />
      ) : null}

      {brainOpen ? <ProjectBrainPanel onClose={() => setBrainOpen(false)} /> : null}

      {settingsTab ? (
        <SettingsModal
          status={status}
          initialTab={settingsTab}
          onClose={() => setSettingsTab(null)}
          onRefresh={() => {
            void refreshStatus();
            void refreshUsage();
          }}
        />
      ) : null}
    </div>
  );
}
