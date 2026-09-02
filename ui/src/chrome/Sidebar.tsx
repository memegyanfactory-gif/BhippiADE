import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { getVersion } from "@tauri-apps/api/app";
import type { ComponentType } from "react";
import type { ProjectSummary, WorkspaceSession, SessionStatus, ProjectTool, ToolAvailability } from "../lib/ipc";
import { clipName, clipPath, relativeTime } from "../lib/format";
import {
  IconArrowLeft,
  IconArrowRight,
  IconBrain,
  IconChat,
  IconChevronDown,
  IconClose,
  IconCode,
  IconDownload,
  IconExternal,
  IconExtractDots,
  IconFolder,
  IconGear,
  IconGitBranch,
  IconGitMerge,
  IconLibrary,
  IconPanelLeft,
  IconPin,
  IconPlus,
  IconRules,
  IconSearch,
  IconTerminal,
  IconTimer,
  IconTrash,
} from "../components/icons";
import { api } from "../lib/api";
import { ProviderLogo } from "../components/ProviderLogo";
import type { Screen } from "./TitleBar";
import { SidebarAccount } from "./SidebarAccount";
import type { SettingsTab } from "../screens/SettingsModal";
import type { PluginMetadata } from "../lib/ipc";

const NAV: { id: Screen; label: string; icon: ComponentType<{ size?: number }> }[] = [
  { id: "chat", label: "Agent", icon: IconChat },
  { id: "research", label: "Research", icon: IconExtractDots },
  { id: "automation", label: "Automation", icon: IconTimer },
  { id: "library", label: "Library", icon: IconLibrary },
  { id: "plugins", label: "Plugins", icon: IconGear },
];

/// A card shows the first few session icons, then a `+N` affordance for the rest.
/// Eight chips is an uncluttered row at ~280px; everything else hides behind the
/// expander until it is clicked.
const MAX_VISIBLE_CHIPS = 8;

const STATUS_LABEL: Record<SessionStatus, string> = {
  running: "Running",
  paused: "Paused",
  idle: "Idle",
  failed: "Failed",
};

const TOOL_ICONS: Record<ProjectTool, (props: { size?: number }) => JSX.Element> = {
  vs_code: IconCode,
  cursor: IconCode,
  antigravity: IconTerminal,
  explorer: IconExternal,
};

type SidebarProps = {
  screen: Screen;
  onScreen: (screen: Screen) => void;
  onBack: () => void;
  onForward: () => void;
  canBack: boolean;
  canForward: boolean;
  collapsed: boolean;
  onToggle: () => void;
  /** `null` while the first load is in flight — the rail says so instead of lying empty. */
  sessions: WorkspaceSession[] | null;
  sessionsError: string | null;
  activeConversationId: string | null;
  onDeleteConversation: (id: string) => void;
  /** Opens a session, switching to its project first when it is not the active one. */
  onOpenSession: (projectPath: string, sessionId: string) => void;
  /** Creates a new session inside a specific project's card. `kind` picks chat or a
      CLI shell; sessions always move to the project they are created in. */
  onNewSessionInProject: (projectPath: string, kind: "chat" | "cli", shell?: string) => void;
  /** Removes a project from the app entirely (the folder itself stays on disk). */
  onRemoveProject: (projectPath: string) => void;
  demoMode: boolean;
  /** The open project, or `null` on the first-run screen. */
  project: ProjectSummary | null;
  projects: ProjectSummary[];
  onSelectProject: (project: ProjectSummary) => void;
  /** Adds a project. `open` picks an existing folder with the native picker directly;
      `create` and `clone` open the project dialog on the matching flow. */
  onNewProject: (kind: "open" | "create" | "clone") => void;
  onOpenSettings?: (tab?: SettingsTab) => void;
  onRetrySessions: () => void;
  onOpenRules?: () => void;
  onOpenReview?: () => void;
  onOpenBrain?: () => void;
  tools?: ToolAvailability[];
  onReorderSession?: (fromId: string, toId: string) => void;
};

/// Places a `.session-create-menu` under its trigger and keeps it on screen. A card
/// near the bottom of a tall rail would otherwise open its menu past the window edge,
/// which looks exactly like the button doing nothing.
function anchorMenu(rect: DOMRect, height: number): { top: number; left: number } {
  const width = 240;
  const edge = 8;
  const gap = 4;
  const left = Math.max(edge, Math.min(rect.left, window.innerWidth - width - edge));
  const below = rect.bottom + gap;
  const fitsBelow = below + height <= window.innerHeight - edge;
  const top = fitsBelow ? below : Math.max(edge, rect.top - gap - height);
  return { top, left };
}

/// Reads a JSON array of strings from localStorage, tolerating corruption.
function readPathList(key: string): string[] {
  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

/// The workspace rail: every project is its own card showing that project's session
/// icons with live status dots, so a glance at the rail tells you what is running
/// where. Project identity leads, because everything under it is scoped to that
/// project (ADR-0012, ADR-0013).
export function Sidebar({
  screen,
  onScreen,
  onBack,
  onForward,
  canBack,
  canForward,
  collapsed,
  onToggle,
  sessions,
  sessionsError,
  activeConversationId,
  onDeleteConversation,
  onOpenSession,
  onNewSessionInProject,
  onRemoveProject,
  demoMode,
  project,
  projects,
  onSelectProject,
  onNewProject,
  onOpenSettings,
  onRetrySessions,
  onOpenRules,
  onOpenReview,
  onOpenBrain,
  tools = [],
  onReorderSession,
}: SidebarProps) {
  const [filtering, setFiltering] = useState(false);
  const [filter, setFilter] = useState("");
  const [openInMenuOpen, setOpenInMenuOpen] = useState(false);
  const [openInMenuPos, setOpenInMenuPos] = useState<{ top: number; left: number } | null>(null);
  const openInAnchorRef = useRef<HTMLDivElement | null>(null);
  const [toolError, setToolError] = useState<string | null>(null);
  const [currentTools, setCurrentTools] = useState<ToolAvailability[]>(tools);

  useEffect(() => {
    if (tools.length > 0) setCurrentTools(tools);
  }, [tools]);

  const toggleOpenInMenu = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!openInMenuOpen) {
      if (openInAnchorRef.current) {
        const rect = openInAnchorRef.current.getBoundingClientRect();
        setOpenInMenuPos({ top: rect.bottom + 6, left: Math.max(10, rect.left) });
      }
      setOpenInMenuOpen(true);
      void api.projectTools().then(setCurrentTools).catch(() => {});
    } else {
      setOpenInMenuOpen(false);
    }
  };

  const launchTool = async (tool: ProjectTool) => {
    if (!project?.path) return;
    try {
      await api.openProjectIn(project.path, tool);
      setOpenInMenuOpen(false);
    } catch (e) {
      setToolError(e instanceof Error ? e.message : String(e));
    }
  };

  const initializeGit = async () => {
    if (!project?.path) return;
    try {
      const updated = await api.initializeGit(project.path);
      onSelectProject(updated);
      setOpenInMenuOpen(false);
    } catch (e) {
      setToolError(e instanceof Error ? e.message : String(e));
    }
  };

  /// The "New project" menu (open / create / clone) and where to anchor it.
  const [projectMenuOpen, setProjectMenuOpen] = useState(false);
  const [projectMenuPos, setProjectMenuPos] = useState<{ top: number; left: number } | null>(
    null,
  );
  /// Project cards whose overflow `+N` has been clicked open.
  const [expandedProjects, setExpandedProjects] = useState<Set<string>>(new Set());
  /// Project cards the owner has collapsed to an icon summary (persisted per session).
  const [minimizedProjects, setMinimizedProjects] = useState<Set<string>>(
    () => new Set(readPathList("bhippi-project-minimized")),
  );
  /// The owner's drag-reordering of the rail, persisted so it survives restarts.
  const [projectOrder, setProjectOrder] = useState<string[]>(
    () => readPathList("bhippi-project-order"),
  );
  /// Pinned projects form a stable group at the top and cannot be displaced by dragging.
  const [pinnedProjects, setPinnedProjects] = useState<Set<string>>(
    () => new Set(readPathList("bhippi-project-pins")),
  );
  /// The conversation whose bin has been clicked once and is waiting for the second.
  const [armed, setArmed] = useState<string | null>(null);
  /// The project whose trash has been clicked once and is waiting for the second.
  const [armedProjects, setArmedProjects] = useState<Set<string>>(new Set());
  /// Which project's per-card `+` menu is open, and where to anchor it.
  const [cardMenu, setCardMenu] = useState<{ path: string; top: number; left: number } | null>(null);
  const [cardCliSubmenu, setCardCliSubmenu] = useState(false);
  /// HTML5 drag-tracked paths for the reorder gesture: the card being dragged and the
  /// card it is currently hovering over (gets the drop highlight).
  const [dragPath, setDragPath] = useState<string | null>(null);
  const [dropPath, setDropPath] = useState<string | null>(null);
  const [draggedSessionId, setDraggedSessionId] = useState<string | null>(null);
  const [dropTargetSessionId, setDropTargetSessionId] = useState<string | null>(null);
  const [version, setVersion] = useState<string | null>(null);
  const filterRef = useRef<HTMLInputElement | null>(null);
  const newProjectBtnRef = useRef<HTMLButtonElement | null>(null);
  const projectActive = project !== null;
  const [plugins, setPlugins] = useState<PluginMetadata[]>([]);
  const [filterPlugins, setFilterPlugins] = useState("");
  const [expandedPlugins, setExpandedPlugins] = useState(false);
  const [installPluginUrl, setInstallPluginUrl] = useState("");

  useEffect(() => {
    void api
      .listPlugins()
      .then(setPlugins)
      .catch(() => setPlugins([]));
  }, []);

  useEffect(() => {
    if (plugins.length > 0) {
      window.localStorage.setItem("bhippi-plugins", JSON.stringify(plugins));
    }
  }, [plugins]);

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(null));
  }, []);

  // `/` focuses the session filter — but never while typing somewhere else.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setProjectMenuOpen(false);
        setExpandedProjects(new Set());
        setCardMenu(null);
        setCardCliSubmenu(false);
        setArmedProjects(new Set());
        setDragPath(null);
        setDropPath(null);
      }
      if (event.key !== "/" || collapsed) return;
      const target = event.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
      event.preventDefault();
      setFiltering(true);
      filterRef.current?.focus();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [collapsed]);

function cleanPath(p?: string | null): string {
  if (!p) return "";
  return p
    .replace(/^(\/\/\?\/|\/\/\?|[\\/]{2}\?)[\\/]?/, "")
    .replace(/\\/g, "/")
    .replace(/\/+$/, "")
    .toLowerCase()
    .trim();
}

  const query = filter.trim().toLowerCase();
  const visible = (sessions ?? []).filter((session) =>
    session.title.toLowerCase().includes(query),
  );
  const byProject = new Map<string, WorkspaceSession[]>();
  for (const session of visible) {
    const key = cleanPath(session.project_path);
    const rows = byProject.get(key);
    if (rows) rows.push(session);
    else byProject.set(key, [session]);
  }

  const activeProjectSessions = project
    ? [...(byProject.get(cleanPath(project.path)) ?? [])].sort((a, b) => {
        const liveLeft = a.status === "running" || a.status === "paused";
        const liveRight = b.status === "running" || b.status === "paused";
        if (liveLeft !== liveRight) return liveLeft ? -1 : 1;
        return b.updated_at.localeCompare(a.updated_at);
      })
    : [];

  /// The owner's stored drag order first, engine order appended for anything new.
  /// Pinned rows always form a stable group at the top.
  const orderedProjects = useMemo(() => {
    const uniqueProjects: ProjectSummary[] = [];
    const seen = new Set<string>();
    for (const p of projects) {
      const key = cleanPath(p.path);
      if (!seen.has(key)) {
        seen.add(key);
        uniqueProjects.push(p);
      }
    }

    const placed: ProjectSummary[] = [];
    for (const path of projectOrder) {
      const found = uniqueProjects.find((row) => cleanPath(row.path) === cleanPath(path));
      if (found && !placed.some((r) => cleanPath(r.path) === cleanPath(found.path))) {
        placed.push(found);
      }
    }
    const remaining = uniqueProjects.filter(
      (row) => !placed.some((r) => cleanPath(r.path) === cleanPath(row.path)),
    );
    const all = [...placed, ...remaining];
    return [
      ...all.filter((row) => pinnedProjects.has(cleanPath(row.path))),
      ...all.filter((row) => !pinnedProjects.has(cleanPath(row.path))),
    ];
  }, [projects, projectOrder, pinnedProjects]);

  useEffect(() => {
    window.localStorage.setItem("bhippi-project-order", JSON.stringify(projectOrder));
  }, [projectOrder]);

  useEffect(() => {
    window.localStorage.setItem("bhippi-project-pins", JSON.stringify([...pinnedProjects]));
  }, [pinnedProjects]);

  useEffect(() => {
    window.localStorage.setItem(
      "bhippi-project-minimized",
      JSON.stringify([...minimizedProjects]),
    );
  }, [minimizedProjects]);

  const handleReorder = (drag: string, over: string) => {
    const dragKey = cleanPath(drag);
    const overKey = cleanPath(over);
    if (dragKey === overKey || pinnedProjects.has(dragKey) || pinnedProjects.has(overKey)) return;
    setProjectOrder(() => {
      const next = orderedProjects
        .map((row) => cleanPath(row.path))
        .filter((path) => path !== dragKey);
      const at = next.indexOf(overKey);
      if (at === -1) next.push(dragKey);
      else next.splice(at, 0, dragKey);
      return next;
    });
  };

  const togglePin = (path: string) =>
    setPinnedProjects((current) => {
      const target = cleanPath(path);
      const next = new Set<string>();
      let found = false;
      for (const p of current) {
        if (cleanPath(p) === target) found = true;
        else next.add(cleanPath(p));
      }
      if (!found) next.add(target);
      return next;
    });

  const toggleMinimize = (path: string) =>
    setMinimizedProjects((current) => {
      const target = cleanPath(path);
      const next = new Set<string>();
      let found = false;
      for (const p of current) {
        if (cleanPath(p) === target) found = true;
        else next.add(cleanPath(p));
      }
      if (!found) next.add(target);
      return next;
    });

  const refreshPlugins = () => {
    void api
      .listPlugins()
      .then(setPlugins)
      .catch((error: unknown) => console.error("Failed to list plugins:", error));
  };

  const activatePlugin = (pluginId: string) => {
    void api
      .activatePlugin(pluginId)
      .then(refreshPlugins)
      .catch((error: unknown) => console.error("Failed to activate plugin:", error));
  };

  const deactivatePlugin = (pluginId: string) => {
    void api
      .deactivatePlugin(pluginId)
      .then(refreshPlugins)
      .catch((error: unknown) => console.error("Failed to deactivate plugin:", error));
  };

  const installPlugin = (pluginUrl: string) => {
    void api
      .installPlugin(pluginUrl)
      .then(refreshPlugins)
      .catch((error: unknown) => console.error("Failed to install plugin:", error));
    setInstallPluginUrl("");
  };

  return (
    <aside className={`sidebar${collapsed ? " collapsed" : ""}`} aria-label="Sidebar">
      <div className="side-icons">
        <button
          className="side-icon"
          onClick={onToggle}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-expanded={!collapsed}
        >
          <IconPanelLeft />
        </button>
        {!collapsed ? (
          <>
            <button
              className={`side-icon${filtering ? " active" : ""}`}
              onClick={() => {
                setFiltering((open) => !open);
                filterRef.current?.focus();
              }}
              aria-label="Filter sessions"
              title="Search & filter sessions"
              aria-expanded={filtering}
              disabled={!projectActive}
            >
              <IconSearch />
            </button>

            {onOpenReview ? (
              <button
                type="button"
                className="side-icon"
                onClick={onOpenReview}
                title="Review changes made by AI"
                aria-label="Review AI changes"
              >
                <IconGitMerge />
              </button>
            ) : null}

            {onOpenRules ? (
              <button
                type="button"
                className="side-icon"
                onClick={onOpenRules}
                title="Workspace rules & instructions"
                aria-label="Workspace rules"
              >
                <IconRules />
              </button>
            ) : null}

            {onOpenBrain ? (
              <button
                type="button"
                className="side-icon"
                onClick={onOpenBrain}
                title="Project Brain: index status & symbols"
                aria-label="Project Brain"
              >
                <IconBrain />
              </button>
            ) : null}

            {project ? (
              <div className="side-icon-custom-wrap" ref={openInAnchorRef}>
                <button
                  type="button"
                  className={`side-icon${openInMenuOpen ? " active" : ""}`}
                  onClick={toggleOpenInMenu}
                  title="Open in external editor or explorer"
                  aria-label="Open in external editor"
                  aria-expanded={openInMenuOpen}
                >
                  <IconExternal />
                </button>
                {openInMenuOpen && openInMenuPos && typeof document !== "undefined"
                  ? createPortal(
                      <>
                        <button
                          type="button"
                          className="titlebar-menu-scrim"
                          onClick={() => setOpenInMenuOpen(false)}
                          aria-label="Close menu"
                        />
                        <div
                          className="titlebar-tool-menu fixed-portal"
                          style={{ top: `${openInMenuPos.top}px`, left: `${openInMenuPos.left}px` }}
                          role="menu"
                        >
                          {currentTools.map((t) => {
                            const Glyph = TOOL_ICONS[t.tool] || IconExternal;
                            return (
                              <button
                                key={t.tool}
                                type="button"
                                role="menuitem"
                                title={t.available ? t.hint : `${t.hint} Click to try anyway.`}
                                onClick={() => void launchTool(t.tool)}
                                className={!t.available ? " tool-unavailable" : ""}
                              >
                                <Glyph size={14} />
                                <span>
                                  <strong>{t.label}</strong>
                                  <small>{t.available ? t.hint : "Not detected — click to try"}</small>
                                </span>
                              </button>
                            );
                          })}
                          {!project.is_git_repository && (
                            <button
                              type="button"
                              role="menuitem"
                              onClick={() => void initializeGit()}
                            >
                              <IconGitBranch size={14} />
                              <span>
                                <strong>Initialize Git</strong>
                                <small>Create a repository in this folder</small>
                              </span>
                            </button>
                          )}
                          {toolError && (
                            <div className="tool-error" role="alert">
                              {toolError}
                            </div>
                          )}
                        </div>
                      </>,
                      document.body,
                    )
                  : null}
              </div>
            ) : null}

            <span className="grow" />
            <button className="side-icon" onClick={onBack} disabled={!canBack} aria-label="Back" title="Back">
              <IconArrowLeft />
            </button>
            <button
              className="side-icon"
              onClick={onForward}
              disabled={!canForward}
              aria-label="Forward"
              title="Forward"
            >
              <IconArrowRight />
            </button>
          </>
        ) : (
          <>
            <span className="grow" />
            {onOpenReview && (
              <button
                type="button"
                className="side-icon"
                onClick={onOpenReview}
                title="Review AI changes"
              >
                <IconGitMerge />
              </button>
            )}
            {onOpenRules && (
              <button
                type="button"
                className="side-icon"
                onClick={onOpenRules}
                title="Workspace rules"
              >
                <IconRules />
              </button>
            )}
            {onOpenBrain && (
              <button
                type="button"
                className="side-icon"
                onClick={onOpenBrain}
                title="Project Brain"
              >
                <IconBrain />
              </button>
            )}
          </>
        )}
      </div>

      {!collapsed ? (
        <>
          <div className="new-session-dropdown">
            <button
              ref={newProjectBtnRef}
              className="side-new"
              onClick={() => {
                const rect = newProjectBtnRef.current?.getBoundingClientRect();
                if (rect) setProjectMenuPos(anchorMenu(rect, 184));
                setProjectMenuOpen((open) => !open);
              }}
              aria-haspopup="menu"
              aria-expanded={projectMenuOpen}
            >
              <IconPlus size={14} /> New project
            </button>

            {projectMenuOpen &&
              createPortal(
                <>
                  <button
                    className="session-menu-scrim"
                    onClick={() => setProjectMenuOpen(false)}
                    aria-label="Close project menu"
                  />
                  <div
                    className="session-create-menu"
                    role="menu"
                    style={
                      projectMenuPos
                        ? { top: projectMenuPos.top, left: projectMenuPos.left }
                        : undefined
                    }
                  >
                  <span className="session-cli-head">Add a project</span>

                  <button
                    type="button"
                    role="menuitem"
                    className="session-menu-row"
                    onClick={() => {
                      setProjectMenuOpen(false);
                      onNewProject("open");
                    }}
                  >
                    <span className="session-menu-icon">
                      <IconFolder size={16} />
                    </span>
                    <span className="session-menu-copy">
                      <strong>Open a folder</strong>
                      <small>Choose an existing project</small>
                    </span>
                  </button>

                  <button
                    type="button"
                    role="menuitem"
                    className="session-menu-row"
                    onClick={() => {
                      setProjectMenuOpen(false);
                      onNewProject("create");
                    }}
                  >
                    <span className="session-menu-icon">
                      <IconPlus size={16} />
                    </span>
                    <span className="session-menu-copy">
                      <strong>Create a project</strong>
                      <small>New empty folder</small>
                    </span>
                  </button>

                  <button
                    type="button"
                    role="menuitem"
                    className="session-menu-row"
                    onClick={() => {
                      setProjectMenuOpen(false);
                      onNewProject("clone");
                    }}
                  >
                    <span className="session-menu-icon">
                      <IconGitBranch size={16} />
                    </span>
                    <span className="session-menu-copy">
                      <strong>Clone from Git</strong>
                      <small>HTTPS or SSH repository</small>
                    </span>
                  </button>
                </div>
                </>,
                document.body,
              )}
          </div>

<nav className="side-nav" aria-label="Screens">
            {NAV.map(({ id, label, icon: Glyph }) => (
              <button
                key={id}
                className={`side-nav-row${screen === id ? " active" : ""}`}
                onClick={() => onScreen(id)}
                disabled={projects.length === 0}
                aria-current={screen === id ? "page" : undefined}
              >
                <Glyph size={15} />
                {label}
              </button>
            ))}
            {screen === "plugins" && (
              <button
                className={`side-nav-row${expandedPlugins ? " active" : ""}`}
                onClick={() => setExpandedPlugins((v) => !v)}
                aria-current="page"
              >
                <IconGear size={15} />
                Plugins
              </button>
            )}
          </nav>

          {screen === "plugins" && expandedPlugins ? (
            <div className="plugin-section">
              <div className="plugin-search">
                <input
                  type="text"
                  className="side-filter"
                  placeholder="Search plugins..."
                  value={filterPlugins}
                  onChange={(e) => setFilterPlugins(e.target.value)}
                  aria-label="Search plugins"
                />
                <button
                  className="side-new"
                  onClick={() => setExpandedPlugins(false)}
                  aria-label="Close plugins section"
                >
                  <IconClose size={14} />
                </button>
              </div>
              <div className="plugin-list">
                {plugins
                  // Only what is actually installed: activating a catalogue entry the
                  // user has not installed is refused, so it does not belong here. The
                  // full catalogue is the Plugins screen's job.
                  .filter(
                    (plugin) =>
                      plugin.installed &&
                      (plugin.name.toLowerCase().includes(filterPlugins.toLowerCase()) ||
                        plugin.description.toLowerCase().includes(filterPlugins.toLowerCase()))
                  )
                  .map((plugin) => {
                    const isActivated = plugin.activated;
                    return (
                      <div
                        key={plugin.id}
                        className={`plugin-card${isActivated ? " activated" : ""}`}
                        title={`${plugin.name} v${plugin.version}`}
                      >
                        <div className="plugin-icon">
                          <IconGear size={20} />
                        </div>
                        <div className="plugin-info">
                          <strong>{plugin.name}</strong>
                          <small>{plugin.version}</small>
                          <p className="plugin-desc">{plugin.description}</p>
                        </div>
                        <div className="plugin-actions">
                          {!isActivated ? (
                            <button
                              className="plugin-action-btn"
                              onClick={() => activatePlugin(plugin.id)}
                            >
                              Activate
                            </button>
                          ) : (
                            <button
                              className="plugin-action-btn"
                              onClick={() => deactivatePlugin(plugin.id)}
                            >
                              Deactivate
                            </button>
                          )}
                        </div>
                      </div>
                    );
                  })}
                {plugins.every((plugin) => !plugin.installed) ? (
                  <div className="plugin-empty">No plugins installed</div>
                ) : null}
              </div>
              <div className="plugin-install">
                <input
                  type="text"
                  className="side-filter"
                  placeholder="Enter plugin ID or URL to install..."
                  value={installPluginUrl}
                  onChange={(e) => setInstallPluginUrl(e.target.value)}
                  aria-label="Plugin install URL"
                />
                <button
                  className="side-new"
                  onClick={() => installPlugin(installPluginUrl)}
                  disabled={!installPluginUrl.trim()}
                  title="Install plugin"
                >
                  <IconDownload size={14} />
                </button>
              </div>
            </div>
          ) : null}

          {/* Single/Multi lives in the title bar's centre controls; a second copy here
              was the same switch twice on one screen. */}
          <div className="side-sect">
            <span>{projectActive ? "Projects" : "Workspace"}</span>
          </div>

          {filtering ? (
            <input
              ref={filterRef}
              className="side-filter"
              value={filter}
              placeholder="Filter sessions…"
              aria-label="Filter sessions"
              onChange={(event) => setFilter(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  setFilter("");
                  setFiltering(false);
                }
              }}
            />
          ) : null}

          <div className="proj-list" role="list" aria-label="Project sessions">
            {sessionsError ? (
              <div className="conv-empty side-session-error" role="alert">
                <span>Sessions unavailable</span>
                <button type="button" onClick={onRetrySessions}>Retry</button>
              </div>
            ) : sessions === null ? (
              <div className="conv-empty">Loading…</div>
            ) : projects.length === 0 ? (
              <div className="conv-empty">Choose a project to begin</div>
            ) : (
              orderedProjects.map((row) => {
                const key = cleanPath(row.path);
                const rows = byProject.get(key) ?? [];
                // While filtering, only projects that actually matched stay on the rail.
                if (filtering && rows.length === 0) return null;
                const activeCount = rows.filter(
                  (session) => session.kind === "ai_chat" &&
                    (session.status === "running" || session.status === "paused"),
                ).length;
                const isActiveProject = key === cleanPath(project?.path);
                const expanded = expandedProjects.has(key);
                const minimized = minimizedProjects.has(key);
                const isDragging = dragPath === key;
                const isDropTarget = dropPath === key;
                const isPinned = pinnedProjects.has(key);
                const projectArmed = armedProjects.has(key);
                const openMenu = cardMenu && cleanPath(cardMenu.path) === key ? cardMenu : null;
                // Give the "+N" affordance the whole matched list when it is toggled open.
                const chipRows = expanded ? rows : rows.slice(0, MAX_VISIBLE_CHIPS);
                const miniRows = rows.slice(0, MAX_VISIBLE_CHIPS);
                return (
                  <div
                    key={key}
                    role="listitem"
                    className={`proj-card${isActiveProject ? " active" : ""}${
                      isDragging ? " dragging" : ""
                    }${isDropTarget ? " drop-target" : ""}${isPinned ? " pinned" : ""}`}
                    draggable={!isPinned}
                    onDragStart={(event) => {
                      if (isPinned) return;
                      setDragPath(key);
                      event.dataTransfer.setData("text/plain", key);
                      event.dataTransfer.effectAllowed = "move";
                    }}
                    onDragOver={(event) => {
                      if (!dragPath || dragPath === key) return;
                      event.preventDefault();
                      setDropPath(key);
                    }}
                    onDragLeave={() => {
                      if (dropPath === key) setDropPath(null);
                    }}
                    onDrop={(event) => {
                      event.preventDefault();
                      setDropPath(null);
                      if (dragPath && dragPath !== key) handleReorder(dragPath, key);
                    }}
                    onDragEnd={() => {
                      setDragPath(null);
                      setDropPath(null);
                    }}
                    title={isDragging ? undefined : isPinned ? "Pinned at the top" : "Drag to reorder"}
                  >
                    <div className="proj-head-row">
                      <button
                        className="proj-head"
                        onClick={() => onSelectProject(row)}
                        title={`${row.name}\n${row.path}`}
                        aria-pressed={isActiveProject}
                      >
                        <span className="proj-head-mark" aria-hidden="true">
                          <IconFolder size={15} />
                        </span>
                        <span className="proj-head-copy">
                          <strong>{clipName(row.name, 24)}</strong>
                          <small>
                            {row.is_git_repository ? (
                              <>
                                <IconGitBranch size={10} />
                                {clipName(row.branch ?? "repository", 14)}
                              </>
                            ) : (
                              clipPath(row.path, 24)
                            )}
                          </small>
                        </span>
                        {activeCount > 0 ? (
                          <span className="proj-active-count" title={`${activeCount} active`}>
                            {activeCount} active
                          </span>
                        ) : null}
                      </button>

                      <span className="proj-head-actions">
                        <button
                          className={`proj-head-action pin${isPinned ? " active" : ""}`}
                          title={isPinned ? `Unpin ${row.name}` : `Pin ${row.name} to the top`}
                          aria-label={isPinned ? `Unpin ${row.name}` : `Pin ${row.name} to the top`}
                          aria-pressed={isPinned}
                          onClick={(event) => {
                            event.stopPropagation();
                            togglePin(key);
                          }}
                        >
                          <IconPin size={13} />
                        </button>
                        <button
                          className="proj-head-action"
                          title={`Add chat or CLI to ${row.name}`}
                          aria-label={`Add chat or CLI to ${row.name}`}
                          aria-haspopup="menu"
                          aria-expanded={cardMenu?.path === row.path}
                          onClick={(event) => {
                            event.stopPropagation();
                            const rect = event.currentTarget.getBoundingClientRect();
                            setCardCliSubmenu(false);
                            // A second press on the same card closes it again.
                            setCardMenu((open) =>
                              open && cleanPath(open.path) === key
                                ? null
                                : { path: row.path, ...anchorMenu(rect, 116) },
                            );
                          }}
                        >
                          <IconPlus size={13} />
                        </button>
                        <button
                          className="proj-head-action"
                          title={minimized ? `Expand ${row.name}` : `Minimize ${row.name}`}
                          aria-label={minimized ? `Expand ${row.name}` : `Minimize ${row.name}`}
                          aria-expanded={!minimized}
                          onClick={() => toggleMinimize(key)}
                        >
                          <IconChevronDown size={13} className={minimized ? "flip" : ""} />
                        </button>
                        {/* Two clicks, because removing a project cannot be undone. */}
                        <button
                          className={`proj-head-action trash${projectArmed ? " armed" : ""}`}
                          title={
                            projectArmed
                              ? `Click again to remove ${row.name} — this cannot be undone`
                              : `Remove ${row.name} from Bhippi`
                          }
                          aria-label={
                            projectArmed
                              ? `Confirm removing ${row.name} from Bhippi`
                              : `Remove ${row.name} from Bhippi`
                          }
                          onBlur={() => setArmedProjects(new Set())}
                          onClick={(event) => {
                            event.stopPropagation();
                            if (!projectArmed) {
                              setArmedProjects(new Set([key]));
                              return;
                            }
                            setArmedProjects(new Set());
                            onRemoveProject(row.path);
                          }}
                        >
                          <IconTrash size={13} />
                        </button>
                      </span>
                    </div>

                    {rows.length === 0 ? (
                      <div className="proj-empty">
                        <span className="proj-empty-text">No sessions yet</span>
                        <button
                          type="button"
                          className="proj-empty-new-btn"
                          onClick={() => onNewSessionInProject(row.path, "chat")}
                          title={`Start a new chat in ${row.name}`}
                          aria-label={`New chat in ${row.name}`}
                        >
                          <IconPlus size={11} />
                          <span>New chat</span>
                        </button>
                      </div>
                    ) : minimized ? (
                      /* Collapsed: one line of session icons, so the name and what is
                         inside the project survive the collapse. */
                      <div
                        className="proj-min-summary"
                        aria-label={`${rows.length} ${rows.length === 1 ? "session" : "sessions"} in ${row.name}`}
                      >
                        {miniRows.map((session) => {
                          const isCli = session.kind === "cli";
                          return (
                            <button
                              key={session.id}
                              className="proj-min-chip"
                              title={`${session.provider_label ?? (isCli ? "CLI" : "Agent")} · ${
                                session.title.replace(/^CLI:\s*/, "")
                              } · ${STATUS_LABEL[session.status]}`}
                              aria-label={`${
                                session.provider_label ?? (isCli ? "CLI" : "Agent")
                              } · ${session.title.replace(/^CLI:\s*/, "")}`}
                              onClick={() => onOpenSession(row.path, session.id)}
                            >
                              {isCli ? (
                                <IconTerminal size={12} />
                              ) : session.provider ? (
                                <ProviderLogo id={session.provider} size={14} />
                              ) : (
                                <IconChat size={12} />
                              )}
                            </button>
                          );
                        })}
                        {rows.length > MAX_VISIBLE_CHIPS ? (
                          <span className="proj-min-more" title="More sessions inside">
                            +{rows.length - MAX_VISIBLE_CHIPS}
                          </span>
                        ) : null}
                      </div>
                    ) : (
                      <>
                        <div className={`proj-chips${expanded ? " expanded" : ""}`}>
                          {chipRows.map((session) => {
                              const active = session.id === activeConversationId;
                              const isCli = session.kind === "cli";
                              const chipTitle = session.title.replace(/^CLI:\s*/, "");
                              return (
                                <span
                                  key={session.id}
                                  className={`proj-chip-wrap${active ? " active" : ""}${
                                    armed === session.id ? " armed" : ""
                                  }${draggedSessionId === session.id ? " dragging" : ""}${
                                    dropTargetSessionId === session.id ? " drop-target" : ""
                                  }`}
                                  draggable
                                  onDragStart={(event) => {
                                    event.stopPropagation();
                                    setDraggedSessionId(session.id);
                                    event.dataTransfer.setData("text/plain", session.id);
                                    event.dataTransfer.effectAllowed = "move";
                                  }}
                                  onDragOver={(event) => {
                                    if (!draggedSessionId || draggedSessionId === session.id) return;
                                    event.preventDefault();
                                    event.stopPropagation();
                                    event.dataTransfer.dropEffect = "move";
                                    if (dropTargetSessionId !== session.id) {
                                      setDropTargetSessionId(session.id);
                                    }
                                  }}
                                  onDragLeave={(event) => {
                                    event.stopPropagation();
                                    if (dropTargetSessionId === session.id) setDropTargetSessionId(null);
                                  }}
                                  onDrop={(event) => {
                                    event.preventDefault();
                                    event.stopPropagation();
                                    if (draggedSessionId && draggedSessionId !== session.id) {
                                      onReorderSession?.(draggedSessionId, session.id);
                                      onOpenSession(row.path, draggedSessionId);
                                    }
                                    setDraggedSessionId(null);
                                    setDropTargetSessionId(null);
                                  }}
                                  onDragEnd={(event) => {
                                    event.stopPropagation();
                                    setDraggedSessionId(null);
                                    setDropTargetSessionId(null);
                                  }}
                                  title={dropTargetSessionId === session.id ? `Drop to move ${chipTitle} here` : undefined}
                                >
                                  <button
                                    className="proj-chip"
                                    title={`${session.provider_label ?? (isCli ? "CLI" : "Agent")} · ${
                                      chipTitle
                                    } · ${STATUS_LABEL[session.status]} · ${relativeTime(
                                      session.updated_at,
                                    )}`}
                                    aria-label={`${session.provider_label ?? (isCli ? "CLI" : "Agent")} · ${
                                      chipTitle
                                    }, ${STATUS_LABEL[session.status].toLowerCase()}, ${relativeTime(
                                      session.updated_at,
                                    )}`}
                                    aria-pressed={active}
                                    onClick={() => onOpenSession(row.path, session.id)}
                                  >
                                    {isCli ? (
                                      <IconTerminal size={14} />
                                    ) : session.provider ? (
                                      <ProviderLogo id={session.provider} size={18} />
                                    ) : (
                                      <IconChat size={14} />
                                    )}
                                    <span
                                      className={`proj-chip-dot st-${session.status}`}
                                      aria-hidden="true"
                                    />
                                  </button>
                                  {/* Two clicks, because deleting a session cannot be undone. The
                                      bin only appears while the chip is hovered or focused, so it
                                      never crowds the rail; the second click confirms. */}
                                  <button
                                    className="proj-chip-del"
                                    aria-label={
                                      armed === session.id
                                        ? `Confirm deleting ${chipTitle}`
                                        : `Delete ${chipTitle}`
                                    }
                                    title={
                                      armed === session.id
                                        ? "Click again to delete — this cannot be undone"
                                        : "Delete this session"
                                    }
                                    onBlur={() =>
                                      setArmed((current) =>
                                        current === session.id ? null : current,
                                      )
                                    }
                                    onClick={(event) => {
                                      event.stopPropagation();
                                      if (armed !== session.id) {
                                        setArmed(session.id);
                                        return;
                                      }
                                      setArmed(null);
                                      onDeleteConversation(session.id);
                                    }}
                                  >
                                    <IconTrash size={9} />
                                  </button>
                                </span>
                              );
                            })}
                        </div>
                        {rows.length > MAX_VISIBLE_CHIPS ? (
                          <div className="proj-chips-footer">
                            <button
                              className="proj-more-reset"
                              onClick={() =>
                                setExpandedProjects((current) => {
                                  const next = new Set(current);
                                  if (expanded) next.delete(key);
                                  else next.add(key);
                                  return next;
                                })
                              }
                              title={
                                expanded
                                  ? `Hide the rest of ${row.name}'s sessions`
                                  : `Show all ${rows.length} sessions`
                              }
                              aria-label={
                                expanded
                                  ? `Hide the rest of ${row.name}'s sessions`
                                  : `Show all ${rows.length} sessions`
                              }
                              aria-expanded={expanded}
                            >
                              <IconChevronDown size={11} className={expanded ? "flip" : ""} />
                              {expanded ? "Show fewer" : `Show all ${rows.length}`}
                            </button>
                          </div>
                        ) : null}
                      </>
                    )}
                    {openMenu
                      ? createPortal(
                          <>
                            <button
                              className="session-menu-scrim"
                              onClick={() => {
                                setCardMenu(null);
                                setCardCliSubmenu(false);
                              }}
                              aria-label="Close session menu"
                            />
                            <div
                              className="session-create-menu"
                              role="menu"
                              aria-label={`New session in ${row.name}`}
                              style={{ top: openMenu.top, left: openMenu.left }}
                            >
                          <button
                            type="button"
                            role="menuitem"
                            className="session-menu-row"
                            onClick={() => {
                              setCardMenu(null);
                              setCardCliSubmenu(false);
                              onNewSessionInProject(row.path, "chat");
                            }}
                          >
                            <span className="session-menu-icon">
                              <IconChat size={16} />
                            </span>
                            <span className="session-menu-copy">
                              <strong>Chat</strong>
                              <small>New agent conversation</small>
                            </span>
                          </button>

                          <button
                            type="button"
                            role="menuitem"
                            className="session-menu-row"
                            onClick={() => setCardCliSubmenu((open) => !open)}
                            aria-expanded={cardCliSubmenu}
                          >
                            <span className="session-menu-icon">
                              <IconTerminal size={16} />
                            </span>
                            <span className="session-menu-copy">
                              <strong>CLI / Terminal</strong>
                              <small>Open command line in project</small>
                            </span>
                            <IconChevronDown size={11} />
                          </button>

                          {cardCliSubmenu && (
                            <div className="session-cli-submenu">
                              <span className="session-cli-head">Select Shell</span>
                              {[
                                { id: "cmd", label: "Command Prompt", glyph: "CMD" },
                                { id: "powershell", label: "PowerShell", glyph: "PS" },
                              ].map((sh) => (
                                <button
                                  key={sh.id}
                                  type="button"
                                  className="session-cli-subitem"
                                  onClick={() => {
                                    setCardMenu(null);
                                    setCardCliSubmenu(false);
                                    onNewSessionInProject(row.path, "cli", sh.id);
                                  }}
                                >
                                  <span>
                                    <span className="cli-shell-badge">{sh.glyph}</span>
                                    {sh.label}
                                  </span>
                                  <small>In-App</small>
                                </button>
                              ))}
                            </div>
                          )}
                          </div>
                          </>,
                          document.body,
                        )
                      : null}
                  </div>
                );
              })
            )}
          </div>
        </>
      ) : (
        <div className="proj-list">
          {project && activeProjectSessions.length > 0 ? (
            <div className="rail-mini" aria-label={`Sessions in ${project.name}`}>
              {activeProjectSessions.map((session) => {
                const isCli = session.kind === "cli";
                const chipTitle = session.title.replace(/^CLI:\s*/, "");
                const active = session.id === activeConversationId;
                return (
                  <button
                    key={session.id}
                    className={`rail-mini-chip${active ? " active" : ""}`}
                    title={`${session.provider_label ?? (isCli ? "CLI" : "Agent")} · ${chipTitle} · ${STATUS_LABEL[session.status]}`}
                    aria-label={`${session.provider_label ?? (isCli ? "CLI" : "Agent")} · ${chipTitle}`}
                    onClick={() => onOpenSession(project.path, session.id)}
                  >
                    {isCli ? (
                      <IconTerminal size={16} />
                    ) : session.provider ? (
                      <ProviderLogo id={session.provider} size={17} />
                    ) : (
                      <IconChat size={16} />
                    )}
                    {session.status !== "idle" && (
                      <i className={`rail-mini-dot st-${session.status}`} aria-hidden="true" />
                    )}
                  </button>
                );
              })}
            </div>
          ) : null}
        </div>
      )}

      <SidebarAccount
        version={version}
        demoMode={demoMode}
        collapsed={collapsed}
        onOpenSettings={onOpenSettings ?? (() => {})}
      />
    </aside>
  );
}
