import { beginProjectDrag } from "./projectPointerDrag";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { ComponentType } from "react";
import type { ProjectSummary, WorkspaceSession, SessionStatus, ProjectTool, ToolAvailability } from "../lib/ipc";
import { clipName, clipPath, relativeTime } from "../lib/format";
import {
  IconArrowLeft,
  IconArrowRight,
  IconBrain,
  IconChat,
  IconChevronDown,
  IconCode,
  IconExternal,
  IconFolder,
  IconGear,
  IconGitBranch,
  IconGitMerge,
  IconGrid,
  IconLayers,
  IconMore,
  IconPin,
  IconPlus,
  IconRules,
  IconSearch,
  IconSidebar,
  IconTerminal,
  IconTrash,
} from "../components/icons";
import { api } from "../lib/api";
import { ProviderLogo } from "../components/ProviderLogo";
import type { Screen } from "./TitleBar";
import { SidebarAccount } from "./SidebarAccount";
import mascot from "../assets/mascot.png";
import type { SettingsTab } from "../screens/SettingsModal";

const NAV: { id: Screen; label: string; icon: ComponentType<{ size?: number }> }[] = [
  { id: "studio", label: "Engine", icon: IconChat },
  { id: "projects", label: "Projects", icon: IconFolder },
  { id: "games", label: "Games", icon: IconGrid },
  { id: "assets", label: "Assets", icon: IconLayers },
  { id: "addons", label: "Add-ons", icon: IconGear },
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
  /** Deletes a session. `projectPath` names the project that owns it, so a chat in a
      project other than the active one is deleted where it lives. */
  onDeleteConversation: (id: string, projectPath?: string) => void;
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
  /// The project whose trash has been clicked once and is waiting for the second.
  const [armedProjects, setArmedProjects] = useState<Set<string>>(new Set());
  /// Which project's per-card `+` menu is open, and where to anchor it.
  const [cardMenu, setCardMenu] = useState<{ path: string; top: number; left: number } | null>(null);
  /// The card's overflow (minimise, remove): the actions a project keeps but does not
  /// need to show. Pin and + are always on the card; these two are not worth the width.
  const [cardMore, setCardMore] = useState<{ path: string; top: number; left: number } | null>(
    null,
  );
  const [cardCliSubmenu, setCardCliSubmenu] = useState(false);
  /// Pointer-tracked paths for the reorder gesture: the card being dragged and the
  /// card it is currently hovering over (with before/after insertion position).
  const [dragPath, setDragPath] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<{ path: string; position: "before" | "after" } | null>(null);
  const dropPath = dropTarget?.path ?? null;
  const [draggedSessionId, setDraggedSessionId] = useState<string | null>(null);
  const [dropTargetSessionId, setDropTargetSessionId] = useState<string | null>(null);
  /// A card is a key, so it has to travel: `pressPath` holds it down for as long as
  /// the pointer is down, `popPath` runs the release spring once the press lands.
  const [pressPath, setPressPath] = useState<string | null>(null);
  const [popPath, setPopPath] = useState<string | null>(null);
  const popTimer = useRef<number | null>(null);
  useEffect(
    () => () => {
      if (popTimer.current !== null) window.clearTimeout(popTimer.current);
    },
    [],
  );
  /// Release: the card comes back up past its resting size and settles, so a press
  /// that opened a project reads as a key travelling rather than a colour change.
  const springBack = (path: string) => {
    setPressPath(null);
    setPopPath(path);
    if (popTimer.current !== null) window.clearTimeout(popTimer.current);
    popTimer.current = window.setTimeout(() => setPopPath(null), 420);
  };
  const [version, setVersion] = useState<string | null>(null);
  const filterRef = useRef<HTMLInputElement | null>(null);
  const newProjectBtnRef = useRef<HTMLButtonElement | null>(null);
  const projectActive = project !== null;

  // Runtime status reports the packaged release version used by the updater and About.
  useEffect(() => {
    api
      .status()
      .then((status) => setVersion(status.version))
      .catch(() => setVersion(null));
  }, []);

  // `/` focuses the session filter — but never while typing somewhere else.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setProjectMenuOpen(false);
        setOpenInMenuOpen(false);
        setExpandedProjects(new Set());
        setCardMenu(null);
        setCardCliSubmenu(false);
        setArmedProjects(new Set());
        setDragPath(null);
        setDropTarget(null);
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

  /// Give every project a slot the first time it is seen, and never move it again.
  ///
  /// Rust hands the list back newest-opened-first (`workspace.rs`, `sort_by_key(Reverse(
  /// last_opened_at))`), which is a fine order for a list nobody has arranged — but opening a
  /// project updates that timestamp, so the row the owner just clicked jumped to the top under
  /// their cursor. Anything not yet in `projectOrder` fell through to that recency order, which
  /// meant *most* rows were still being sorted by when they were last touched.
  ///
  /// Recording each new project at the end pins it down: from the first sight of a project its
  /// position is the owner's to change, by dragging, and by nothing else.
  useEffect(() => {
    const known = new Set(projectOrder.map(cleanPath));
    const fresh = projects.map((row) => cleanPath(row.path)).filter((key) => !known.has(key));
    if (fresh.length === 0) return;
    setProjectOrder((current) => [...current, ...fresh]);
  }, [projects, projectOrder]);

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

  const projectGesture = useRef<(() => void) | null>(null);
  const skipProjectClick = useRef(false);
  useEffect(() => () => projectGesture.current?.(), []);

  const dropTargetRef = useRef<{ path: string; position: "before" | "after" } | null>(null);

  const handleReorder = (drag: string, over: string, placement?: "before" | "after") => {
    const dragKey = cleanPath(drag);
    const overKey = cleanPath(over);
    if (!dragKey || !overKey || dragKey === overKey) return;

    const dragWasPinned = pinnedProjects.has(dragKey);
    const overIsPinned = pinnedProjects.has(overKey);
    const position = placement ?? dropTargetRef.current?.position ?? dropTarget?.position ?? "before";

    // Moving across the pin boundary toggles pin state so the project lands in the destination section
    if (dragWasPinned !== overIsPinned) {
      setPinnedProjects((current) => {
        const next = new Set(current);
        if (overIsPinned) next.add(dragKey);
        else next.delete(dragKey);
        return next;
      });
    }

    setProjectOrder(() => {
      const allKeys = orderedProjects.map((row) => cleanPath(row.path));
      const next = allKeys.filter((path) => path !== dragKey);
      const at = next.indexOf(overKey);
      if (at === -1) {
        if (position === "after") next.push(dragKey);
        else next.unshift(dragKey);
      } else {
        const insertAt = position === "after" ? at + 1 : at;
        next.splice(insertAt, 0, dragKey);
      }
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

  /// Filtering hides a project whose sessions all fell out of the query, and the
  /// section headers are placed by index, so both have to read the same list.
  const railProjects = orderedProjects.filter(
    (row) => !(filtering && (byProject.get(cleanPath(row.path)) ?? []).length === 0),
  );
  const pinnedCount = railProjects.filter((row) =>
    pinnedProjects.has(cleanPath(row.path)),
  ).length;

  const titlebarSlot =
    typeof document !== "undefined" ? document.getElementById("titlebar-left-slot") : null;

  const collapsedSessions = useMemo(() => {
    if (!sessions || sessions.length === 0) return [];
    const running = sessions.filter((s) => s.status === "running");
    const idle = sessions.filter((s) => s.status !== "running");
    return [...running, ...idle].slice(0, 8);
  }, [sessions]);

  return (
    <>
      <aside className={`sidebar${collapsed ? " collapsed" : ""}`} aria-label="Sidebar">
        {collapsed ? (
          <div className="side-rail-only">
            <button
              type="button"
              className="side-brand-btn side-rail-toggle"
              onClick={onToggle}
              aria-label="Expand sidebar"
              title="Expand sidebar (unhide side panel)"
              aria-expanded={false}
            >
              <IconSidebar size={15} />
            </button>

            <div className="collapsed-providers-list" role="list" aria-label="Running providers and models">
              {collapsedSessions.map((s) => {
                const isCli = s.kind === "cli";
                const isRunning = s.status === "running";
                const rowTitle = s.title.replace(/^CLI:\s*/, "").trim() || "Chat";
                const providerLabel = s.provider_label ?? (isCli ? "CLI" : "Agent");
                return (
                  <button
                    key={s.id}
                    type="button"
                    role="listitem"
                    className={`collapsed-provider-btn${s.id === activeConversationId ? " active" : ""}${isRunning ? " running" : ""}`}
                    onClick={() => onOpenSession(s.project_path, s.id)}
                    title={`${providerLabel} · ${rowTitle} · ${STATUS_LABEL[s.status]}`}
                    aria-label={`${providerLabel}: ${rowTitle} (${STATUS_LABEL[s.status]})`}
                  >
                    <span className="collapsed-provider-mark">
                      {isCli ? (
                        <IconTerminal size={15} />
                      ) : s.provider ? (
                        <ProviderLogo id={s.provider} size={16} />
                      ) : (
                        <IconChat size={15} />
                      )}
                    </span>
                    <span className={`collapsed-status-pip st-${s.status}`} />
                  </button>
                );
              })}
            </div>
          </div>
        ) : (
          <>
            {/* Top brand logo area: static display, not a button and not clickable */}
            <div className="side-brand">
              <div className="side-brand-id">
                <img
                  className="side-brand-mark"
                  src={mascot}
                  alt=""
                  width={36}
                  height={36}
                  draggable={false}
                />
                <span className="side-brand-name">Bhippi</span>
              </div>

              <span className="side-brand-actions">
                <button
                  type="button"
                  className={`side-brand-btn${filtering ? " active" : ""}`}
                  onClick={() => {
                    setFiltering((open) => !open);
                    filterRef.current?.focus();
                  }}
                  aria-label="Filter sessions"
                  title="Search & filter sessions"
                  aria-expanded={filtering}
                  disabled={!projectActive}
                >
                  <IconSearch size={15} />
                </button>
                <button
                  type="button"
                  className="side-brand-btn"
                  onClick={onToggle}
                  aria-label="Collapse sidebar"
                  title="Collapse sidebar (hide side panel)"
                  aria-expanded
                >
                  <IconSidebar size={15} />
                </button>
              </span>
            </div>

            {/* Action icons row placed directly below the top logo area */}
            <div className="side-icons" role="toolbar" aria-label="Workspace actions">
              {onOpenRules ? (
                <button
                  type="button"
                  className="side-icon"
                  onClick={onOpenRules}
                  title="Workspace rules & instructions"
                  aria-label="Workspace rules"
                >
                  <IconRules size={15} />
                </button>
              ) : null}

              {onOpenReview ? (
                <button
                  type="button"
                  className="side-icon"
                  onClick={onOpenReview}
                  title="Review changes made by AI"
                  aria-label="Review AI changes"
                >
                  <IconGitMerge size={15} />
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
                  <IconBrain size={15} />
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
                    <IconExternal size={15} />
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
                            aria-label="Open project in"
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
                            {!project.is_git_repository ? (
                              <button
                                type="button"
                                role="menuitem"
                                onClick={() => void initializeGit()}
                              >
                                <IconGitBranch size={14} />
                                <span>
                                  <strong>Initialize Git</strong>
                                  <small>Create repository</small>
                                </span>
                              </button>
                            ) : null}
                            {toolError ? (
                              <div className="tool-error" role="alert">
                                {toolError}
                              </div>
                            ) : null}
                          </div>
                        </>,
                        document.body,
                      )
                    : null}
                </div>
              ) : null}

              {onOpenSettings ? (
                <button
                  type="button"
                  className="side-icon"
                  onClick={() => onOpenSettings()}
                  title="Settings"
                  aria-label="Settings"
                >
                  <IconGear size={15} />
                </button>
              ) : null}

              <span className="grow" />

              <button
                type="button"
                className="side-icon"
                onClick={onBack}
                disabled={!canBack}
                aria-label="Back"
                title="Back"
              >
                <IconArrowLeft size={14} />
              </button>

              <button
                type="button"
                className="side-icon"
                onClick={onForward}
                disabled={!canForward}
                aria-label="Forward"
                title="Forward"
              >
                <IconArrowRight size={14} />
              </button>
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

          <div
            className="proj-list"
            aria-label="Projects"
            onDragOver={(event) => {
              if (!dragPath) return;
              if (event.target === event.currentTarget) {
                event.preventDefault();
                event.dataTransfer.dropEffect = "move";
                if (dropTarget?.path !== "__list_end__") {
                  setDropTarget({ path: "__list_end__", position: "after" });
                }
              }
            }}
            onDrop={(event) => {
              if (event.target === event.currentTarget && dragPath) {
                event.preventDefault();
                const last = railProjects[railProjects.length - 1];
                if (last && cleanPath(last.path) !== cleanPath(dragPath)) {
                  handleReorder(dragPath, cleanPath(last.path));
                }
                setDropTarget(null);
              }
            }}
          >
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
              railProjects.map((row, index) => {
                const key = cleanPath(row.path);
                const rows = byProject.get(key) ?? [];
                const activeCount = rows.filter(
                  (session) => session.kind === "ai_chat" &&
                    (session.status === "running" || session.status === "paused"),
                ).length;
                const isActiveProject = key === cleanPath(project?.path);
                const expanded = expandedProjects.has(key);
                const minimized = minimizedProjects.has(key);
                const isDragging = dragPath === key;
                const isDropTarget = dropPath === key;
                const dropPos = isDropTarget ? (dropTarget?.position ?? "before") : null;
                const isPinned = pinnedProjects.has(key);
                const projectArmed = armedProjects.has(key);
                const openMenu = cardMenu && cleanPath(cardMenu.path) === key ? cardMenu : null;
                const openMore = cardMore && cleanPath(cardMore.path) === key ? cardMore : null;
                // Give the "+N" affordance the whole matched list when it is toggled open.
                const chipRows = expanded ? rows : rows.slice(0, MAX_VISIBLE_CHIPS);
                const miniRows = rows.slice(0, MAX_VISIBLE_CHIPS);
                return (
                  <Fragment key={key}>
                    {index === 0 && pinnedCount > 0 ? (
                      <div
                        className={`side-sect side-sect-pinned${dropTarget?.path === "__pinned_header__" ? " drop-target" : ""}`}
                        onDragOver={(event) => {
                          if (!dragPath) return;
                          event.preventDefault();
                          event.dataTransfer.dropEffect = "move";
                          if (dropTarget?.path !== "__pinned_header__") {
                            setDropTarget({ path: "__pinned_header__", position: "before" });
                          }
                        }}
                        onDragLeave={(event) => {
                          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
                            if (dropTarget?.path === "__pinned_header__") setDropTarget(null);
                          }
                        }}
                        onDrop={(event) => {
                          event.preventDefault();
                          if (!dragPath) return;
                          const firstPinned = railProjects.find((r) => pinnedProjects.has(cleanPath(r.path)));
                          if (firstPinned && cleanPath(firstPinned.path) !== dragPath) {
                            handleReorder(dragPath, cleanPath(firstPinned.path));
                          } else {
                            setPinnedProjects((curr) => new Set([...curr, dragPath]));
                            setProjectOrder((curr) => [dragPath, ...curr.filter((p) => cleanPath(p) !== dragPath)]);
                          }
                          setDropTarget(null);
                        }}
                      >
                        <IconPin size={11} />
                        <span>Pinned</span>
                        <em className="side-sect-count">{pinnedCount}</em>
                      </div>
                    ) : null}
                    {index === pinnedCount ? (
                      <div
                        className={`side-sect side-sect-recent${dropTarget?.path === "__recent_header__" ? " drop-target" : ""}`}
                        onDragOver={(event) => {
                          if (!dragPath) return;
                          event.preventDefault();
                          event.dataTransfer.dropEffect = "move";
                          if (dropTarget?.path !== "__recent_header__") {
                            setDropTarget({ path: "__recent_header__", position: "before" });
                          }
                        }}
                        onDragLeave={(event) => {
                          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
                            if (dropTarget?.path === "__recent_header__") setDropTarget(null);
                          }
                        }}
                        onDrop={(event) => {
                          event.preventDefault();
                          if (!dragPath) return;
                          const firstUnpinned = railProjects.find((r) => !pinnedProjects.has(cleanPath(r.path)));
                          if (firstUnpinned && cleanPath(firstUnpinned.path) !== dragPath) {
                            handleReorder(dragPath, cleanPath(firstUnpinned.path));
                          } else {
                            setPinnedProjects((curr) => {
                              const next = new Set(curr);
                              next.delete(dragPath);
                              return next;
                            });
                          }
                          setDropTarget(null);
                        }}
                      >
                        <IconFolder size={11} />
                        <span>Projects</span>
                        <em className="side-sect-count">{railProjects.length - pinnedCount}</em>
                      </div>
                    ) : null}
                    <div
                      className={`proj-card${isActiveProject ? " active" : ""}${
                        isDragging ? " dragging" : ""
                      }${isDropTarget && dropPos ? ` drop-target drop-target-${dropPos}` : ""}${isPinned ? " pinned" : ""}${
                        pressPath === key ? " pressing" : ""
                      }${popPath === key ? " popped" : ""}`}
                      data-project-key={key}
                      onClickCapture={(event) => {
                        if (skipProjectClick.current) { event.preventDefault(); event.stopPropagation(); skipProjectClick.current = false; }
                      }}
                      onClick={(event) => {
                        const hit = event.target as HTMLElement | null;
                        if (hit?.closest("button, a, input, [role='menu']")) return;
                        onSelectProject(row);
                      }}
                      onPointerDown={(event) => {
                        projectGesture.current?.();
                        projectGesture.current = beginProjectDrag(event, key,
                          (path, target) => { setDragPath(path); setDropTarget(target); if (path) setPressPath(null); },
                          (path, target) => handleReorder(path, target.path, target.position),
                          () => { skipProjectClick.current = true; window.setTimeout(() => { skipProjectClick.current = false; }, 0); },
                        );
                        const hit = event.target as HTMLElement | null;
                        const control = hit?.closest("button, a, input, [role='menu']");
                        if (control && !control.classList.contains("proj-head")) return;
                        setPressPath(key);
                      }}
                      onPointerUp={() => {
                        if (pressPath === key) springBack(key);
                      }}
                      onPointerLeave={() => setPressPath((held) => (held === key ? null : held))}
                      onPointerCancel={() => setPressPath((held) => (held === key ? null : held))}
                      draggable={false}
                      onDragStart={(event) => {
                        const hit = event.target as HTMLElement | null;
                        if (hit?.closest(".proj-sessions, .proj-head-actions")) {
                          event.preventDefault();
                          return;
                        }
                        setDragPath(key);
                        setPressPath(null);
                        event.dataTransfer.setData("text/plain", key);
                        event.dataTransfer.effectAllowed = "move";
                        const card = event.currentTarget as HTMLElement;
                        if (event.dataTransfer.setDragImage) {
                          event.dataTransfer.setDragImage(card, 20, 20);
                        }
                      }}
                      onDragOver={(event) => {
                        if (!dragPath || dragPath === key) return;
                        event.preventDefault();
                        event.dataTransfer.dropEffect = "move";
                        const rect = event.currentTarget.getBoundingClientRect();
                        const midY = rect.top + rect.height / 2;
                        const position: "before" | "after" = event.clientY < midY ? "before" : "after";
                        if (dropTarget?.path !== key || dropTarget?.position !== position) {
                          setDropTarget({ path: key, position });
                        }
                      }}
                      onDragLeave={(event) => {
                        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
                          if (dropTarget?.path === key) setDropTarget(null);
                        }
                      }}
                      onDrop={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        if (dragPath && dragPath !== key) {
                          const rect = event.currentTarget.getBoundingClientRect();
                          const midY = rect.top + rect.height / 2;
                          const position: "before" | "after" = event.clientY < midY ? "before" : "after";
                          dropTargetRef.current = { path: key, position };
                          handleReorder(dragPath, key);
                        }
                        dropTargetRef.current = null;
                        setDropTarget(null);
                        setDragPath(null);
                        setPressPath(null);
                      }}
                      onDragEnd={() => {
                        setDragPath(null);
                        setDropTarget(null);
                        setPressPath(null);
                      }}
                      title={isDragging ? undefined : isPinned ? "Pinned · Drag to reorder" : "Drag to reorder"}
                    >
                      <div className="proj-head-row">
                        <button
                          className="proj-head"
                          onClick={() => onSelectProject(row)}
                          title={`${row.name}\n${row.path}`}
                          aria-pressed={isActiveProject}
                          draggable={false}
                          onDragStart={(event) => {
                            setDragPath(key);
                            setPressPath(null);
                            event.dataTransfer.setData("text/plain", key);
                            event.dataTransfer.effectAllowed = "move";
                            const card = event.currentTarget.closest(".proj-card") as HTMLElement | null;
                            if (card && event.dataTransfer.setDragImage) {
                              event.dataTransfer.setDragImage(card, 20, 20);
                            }
                          }}
                        >
                          <span className="proj-head-mark" aria-hidden="true">
                            <IconFolder size={14} />
                          </span>
                          <span className="proj-head-copy">
                            <strong>{clipName(row.name, 40)}</strong>
                            <small>
                              {row.is_git_repository ? (
                                <>
                                  <IconGitBranch size={10} />
                                  {clipName(row.branch ?? "repository", 22)}
                                </>
                              ) : (
                                clipPath(row.path, 34)
                              )}
                            </small>
                          </span>
                        </button>

                        {/* The count sits inside the lane rather than under it, so nothing
                            has to be faded out to make room for the buttons. */}
                        {activeCount > 0 ? (
                          <span className="proj-active-count" title={`${activeCount} active`}>
                            {activeCount}
                          </span>
                        ) : null}

                        {/* Permanent, and quiet. These used to fade in on hover behind a
                            gradient that ran over the project's own name — so the two things
                            a person reaches for most were the two things they could not see,
                            and finding them cost the name. Three small controls in a lane of
                            their own is cheaper than four on top of the title. */}
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
                              setCardMore(null);
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
                            title={`More for ${row.name}`}
                            aria-label={`More actions for ${row.name}`}
                            aria-haspopup="menu"
                            aria-expanded={cardMore?.path === row.path}
                            onClick={(event) => {
                              event.stopPropagation();
                              const rect = event.currentTarget.getBoundingClientRect();
                              setCardMenu(null);
                              setCardMore((open) =>
                                open && cleanPath(open.path) === key
                                  ? null
                                  : { path: row.path, ...anchorMenu(rect, 96) },
                              );
                            }}
                          >
                            <IconMore size={13} />
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
                                className={`proj-min-chip${session.id === activeConversationId ? " active" : ""}`}
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
                          <div
                            className={`proj-sessions${expanded ? " expanded" : ""}`}
                            role="list"
                            aria-label={`Sessions in ${row.name}`}
                          >
                            {chipRows.map((session) => {
                              const active = session.id === activeConversationId;
                              const isCli = session.kind === "cli";
                              const rowTitle =
                                session.title.replace(/^CLI:\s*/, "").trim() || "New chat";
                              const providerLabel =
                                session.provider_label ?? (isCli ? "CLI" : "Agent");
                              // The dot says one thing only: is this session working right now.
                              const state =
                                session.status === "running"
                                  ? "running"
                                  : STATUS_LABEL[session.status].toLowerCase();
                              const rowLabel = `${rowTitle} — ${providerLabel} · ${state} · ${relativeTime(
                                session.updated_at,
                              )}`;
                              return (
                                <div
                                  key={session.id}
                                  role="listitem"
                                  className={`proj-row-wrap${active ? " active" : ""}${
                                    draggedSessionId === session.id ? " dragging" : ""
                                  }${
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
                                    if (dropTargetSessionId === session.id) {
                                      setDropTargetSessionId(null);
                                    }
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
                                >
                                  <button
                                    type="button"
                                    className="proj-row"
                                    title={rowLabel}
                                    aria-label={rowLabel}
                                    aria-pressed={active}
                                    onClick={() => onOpenSession(row.path, session.id)}
                                  >
                                    <span className="proj-row-mark" aria-hidden="true">
                                      {isCli ? (
                                        <IconTerminal size={13} />
                                      ) : session.provider ? (
                                        <ProviderLogo id={session.provider} size={14} />
                                      ) : (
                                        <IconChat size={13} />
                                      )}
                                    </span>
                                    <span className="proj-row-title">{rowTitle}</span>
                                    <span
                                      className={`proj-row-dot st-${session.status}`}
                                      aria-hidden="true"
                                    />
                                  </button>
                                  {/* One click deletes: the bin only appears while the row is
                                      hovered or focused, so it is never hit by accident, and a
                                      second confirming click on a control the owner deliberately
                                      reached for is friction, not safety. The owning project
                                      travels with the id so a chat in a project that is not the
                                      active one is deleted where it actually lives. */}
                                  <button
                                    type="button"
                                    className="proj-row-del"
                                    aria-label={`Delete ${rowTitle}`}
                                    title="Delete this session"
                                    onClick={(event) => {
                                      event.stopPropagation();
                                      onDeleteConversation(session.id, row.path);
                                    }}
                                  >
                                    <IconTrash size={12} />
                                  </button>
                                </div>
                              );
                            })}

                            <button
                              type="button"
                              className="proj-row-new"
                              onClick={() => onNewSessionInProject(row.path, "chat")}
                              title={`Start a new chat in ${row.name}`}
                              aria-label={`New chat in ${row.name}`}
                            >
                              <IconPlus size={12} />
                              <span>New chat</span>
                            </button>
                          </div>
                          {rows.length > MAX_VISIBLE_CHIPS ? (
                            <div className="proj-sessions-footer">
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
                      {openMore
                        ? createPortal(
                            <>
                              <button
                                className="session-menu-scrim"
                                onClick={() => setCardMore(null)}
                                aria-label="Close project menu"
                              />
                              <div
                                className="session-create-menu compact"
                                role="menu"
                                aria-label={`More for ${row.name}`}
                                style={{ top: openMore.top, left: openMore.left }}
                              >
                                <button
                                  type="button"
                                  role="menuitem"
                                  className="session-menu-row"
                                  onClick={() => {
                                    setCardMore(null);
                                    toggleMinimize(key);
                                  }}
                                >
                                  <span className="session-menu-icon">
                                    <IconChevronDown size={15} className={minimized ? "flip" : ""} />
                                  </span>
                                  <span className="session-menu-copy">
                                    <strong>{minimized ? "Expand" : "Minimise"}</strong>
                                  </span>
                                </button>

                                {/* Two clicks, because removing a project cannot be undone. */}
                                <button
                                  type="button"
                                  role="menuitem"
                                  className={`session-menu-row danger${projectArmed ? " armed" : ""}`}
                                  onClick={() => {
                                    if (!projectArmed) {
                                      setArmedProjects(new Set([key]));
                                      return;
                                    }
                                    setArmedProjects(new Set());
                                    setCardMore(null);
                                    onRemoveProject(row.path);
                                  }}
                                >
                                  <span className="session-menu-icon">
                                    <IconTrash size={15} />
                                  </span>
                                  <span className="session-menu-copy">
                                    <strong>
                                      {projectArmed ? "Click again to remove" : "Remove project"}
                                    </strong>
                                    <small>
                                      {projectArmed
                                        ? "This cannot be undone"
                                        : "Takes it out of Bhippi, not off disk"}
                                    </small>
                                  </span>
                                </button>
                              </div>
                            </>,
                            document.body,
                          )
                        : null}
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
                  </Fragment>
                );
              })
            )}
            {dropTarget?.path === "__list_end__" ? (
              <div className="proj-card-drop-indicator" />
            ) : null}
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
          </nav>
          <SidebarAccount
            version={version}
            demoMode={demoMode}
            collapsed={false}
            onOpenSettings={onOpenSettings ?? (() => {})}
          />
        </>
      )}
    </aside>
    {titlebarSlot &&
      createPortal(
        <div className={`side-brand titlebar-brand-portal${collapsed ? " collapsed" : ""}`}>
          {collapsed ? (
            <button
              type="button"
              className="side-brand-btn side-rail-toggle"
              onClick={onToggle}
              aria-label="Expand sidebar"
              title="Expand sidebar (unhide side panel)"
            >
              <IconSidebar size={15} />
            </button>
          ) : (
            <>
              <div className="side-brand-id">
                <img
                  className="side-brand-mark"
                  src={mascot}
                  alt=""
                  width={36}
                  height={36}
                  draggable={false}
                />
                <span className="side-brand-name">Bhippi</span>
              </div>

              <span className="side-brand-actions">
                <button
                  type="button"
                  className={`side-brand-btn${filtering ? " active" : ""}`}
                  onClick={() => {
                    setFiltering((open) => !open);
                    filterRef.current?.focus();
                  }}
                  aria-label="Filter sessions"
                  title="Search & filter sessions"
                  disabled={!projectActive}
                >
                  <IconSearch size={15} />
                </button>
                <button
                  type="button"
                  className="side-brand-btn"
                  onClick={onToggle}
                  aria-label="Collapse sidebar"
                  title="Collapse sidebar (hide side panel)"
                >
                  <IconSidebar size={15} />
                </button>
              </span>
            </>
          )}
        </div>,
        titlebarSlot,
      )}
  </>
  );
}
