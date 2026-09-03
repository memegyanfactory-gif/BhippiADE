// The Studio's bottom dock: Assets · Library · Code · Console · Versions.
//
// Every tab is a projection of something real in the open project, and nothing here
// invents a row. Rust decides what an asset is, what kind it is and what its licence says
// (`list_project_assets`), what the engine can do (`list_capabilities`), which scripts
// exist (`list_project_scripts`), what the engine printed (`godot_output` plus the
// `godot-output` event) and what the version history holds (`godot_list_versions`).
// This file loads those answers, keeps the four states each one can be in, and draws them.
//
// A freshly scaffolded game has no assets and no versions, and the dock says exactly that.

import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, events } from "../lib/api";
import { AssetLibraryPanel } from "../components/AssetLibraryPanel";
import type {
  CapabilityLibrary,
  GameVersion,
  ProjectAssetsView,
  ProjectScript,
  VersionsView,
} from "../lib/ipc";

export type StudioDockTab = "assets" | "library" | "code" | "console" | "versions";

interface StudioBottomDockProps {
  activeTab: StudioDockTab | null;
  onSelectTab: (tab: StudioDockTab | null) => void;
  projectPath?: string;
  projectName?: string;
}

/** The four states every panel can be in (INV-075). */
type Loadable<T> =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "ready"; data: T }
  | { state: "error"; message: string };

const IDLE: Loadable<never> = { state: "idle" };

/** The most console lines the drawer keeps. Older ones scroll out of history. */
const MAX_CONSOLE_LINES = 500;

type ConsoleLine = { id: string; stream: string; text: string };

function errorText(cause: unknown): string {
  const value = cause as { message?: string; hint?: string } | undefined;
  if (value && typeof value.message === "string") {
    return value.hint ? `${value.message} ${value.hint}` : value.message;
  }
  return String(cause);
}

/** Two project paths that differ only in separator or case are the same project. */
function samePath(left: string | undefined, right: string | undefined): boolean {
  if (!left || !right) return false;
  return left.replace(/\\/g, "/").toLowerCase() === right.replace(/\\/g, "/").toLowerCase();
}

/** Bytes as the dock shows them — never "0.0 KB" for a real file. */
function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** RFC 3339 from Rust → the local short form the row shows. */
function formatWhen(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? iso : at.toLocaleString();
}

const KIND_ICON: Record<string, string> = {
  model: "🧊",
  texture: "🖼️",
  audio: "🎵",
  scene: "🎬",
  material: "🎨",
  shader: "✨",
  other: "📄",
};

export function StudioBottomDock({
  activeTab,
  onSelectTab,
  projectPath,
  projectName = "this game",
}: StudioBottomDockProps) {
  const [assets, setAssets] = useState<Loadable<ProjectAssetsView>>(IDLE);
  const [library, setLibrary] = useState<Loadable<CapabilityLibrary>>(IDLE);
  const [scripts, setScripts] = useState<Loadable<ProjectScript[]>>(IDLE);
  const [versions, setVersions] = useState<Loadable<VersionsView>>(IDLE);
  const [consoleLines, setConsoleLines] = useState<ConsoleLine[]>([]);
  const [consoleError, setConsoleError] = useState<string | null>(null);

  const [assetSearch, setAssetSearch] = useState("");
  const [sourceFilter, setSourceFilter] = useState("all");
  const [librarySearch, setLibrarySearch] = useState("");
  const [viewMode, setViewMode] = useState<"grid" | "list">("grid");
  // SPA-103: the Assets tab shows the project's own files or the user's library folders.
  const [assetScope, setAssetScope] = useState<"project" | "library">("project");

  const [openScript, setOpenScript] = useState<string | null>(null);
  const [scriptBody, setScriptBody] = useState<Loadable<string>>(IDLE);

  const [isCreatingVersion, setIsCreatingVersion] = useState(false);
  const [newVersionLabel, setNewVersionLabel] = useState("");
  const [versionBusy, setVersionBusy] = useState(false);

  const [isUploading, setIsUploading] = useState(false);
  const consoleSeq = useRef(0);

  // ── loaders ──────────────────────────────────────────────────────────────────────
  const loadAssets = useCallback(async () => {
    if (!projectPath) {
      setAssets(IDLE);
      return;
    }
    setAssets({ state: "loading" });
    try {
      setAssets({ state: "ready", data: await api.listProjectAssets(projectPath) });
    } catch (cause) {
      setAssets({ state: "error", message: errorText(cause) });
    }
  }, [projectPath]);

  const loadScripts = useCallback(async () => {
    if (!projectPath) {
      setScripts(IDLE);
      return;
    }
    setScripts({ state: "loading" });
    try {
      setScripts({ state: "ready", data: await api.listProjectScripts(projectPath) });
    } catch (cause) {
      setScripts({ state: "error", message: errorText(cause) });
    }
  }, [projectPath]);

  const loadVersions = useCallback(async () => {
    if (!projectPath) {
      setVersions(IDLE);
      return;
    }
    setVersions({ state: "loading" });
    try {
      setVersions({ state: "ready", data: await api.godotListVersions(projectPath) });
    } catch (cause) {
      setVersions({ state: "error", message: errorText(cause) });
    }
  }, [projectPath]);

  const loadLibrary = useCallback(async () => {
    setLibrary({ state: "loading" });
    try {
      setLibrary({ state: "ready", data: await api.listCapabilities() });
    } catch (cause) {
      setLibrary({ state: "error", message: errorText(cause) });
    }
  }, []);

  // Everything the dock shows belongs to one project: switching project drops it all.
  useEffect(() => {
    setAssets(IDLE);
    setScripts(IDLE);
    setVersions(IDLE);
    setConsoleLines([]);
    setConsoleError(null);
    setOpenScript(null);
    setScriptBody(IDLE);
    setAssetSearch("");
    setSourceFilter("all");
  }, [projectPath]);

  // A tab loads the first time it is opened, and again whenever the project changes.
  useEffect(() => {
    if (activeTab === "assets" && assets.state === "idle") void loadAssets();
    if (activeTab === "code" && scripts.state === "idle") void loadScripts();
    if (activeTab === "versions" && versions.state === "idle") void loadVersions();
    if (activeTab === "library" && library.state === "idle") void loadLibrary();
  }, [
    activeTab,
    assets.state,
    scripts.state,
    versions.state,
    library.state,
    loadAssets,
    loadScripts,
    loadVersions,
    loadLibrary,
  ]);

  // Console: seed from what the engine has already printed, then follow the event. The
  // subscription runs whether or not the tab is open, so opening it is never empty for a
  // game that is already running.
  useEffect(() => {
    if (!projectPath) return;
    let cancelled = false;
    let stop: (() => void) | null = null;

    const seed = async () => {
      try {
        const lines = await api.godotOutput(projectPath);
        if (cancelled) return;
        setConsoleLines(
          lines.slice(-MAX_CONSOLE_LINES).map((line) => ({
            id: `seed-${consoleSeq.current++}`,
            stream: line.stream,
            text: line.text,
          })),
        );
      } catch (cause) {
        if (!cancelled) setConsoleError(errorText(cause));
      }
    };

    void seed();
    void events.godotOutput
      .listen(({ payload }) => {
        if (cancelled || !samePath(payload.project, projectPath)) return;
        setConsoleLines((current) =>
          [
            ...current,
            ...payload.lines.map((line) => ({
              id: `live-${consoleSeq.current++}`,
              stream: line.stream,
              text: line.text,
            })),
          ].slice(-MAX_CONSOLE_LINES),
        );
      })
      .then((unlisten) => {
        if (cancelled) unlisten();
        else stop = unlisten;
      })
      .catch(() => {
        // A dropped subscription leaves the seeded lines; the tab still shows the truth.
      });

    return () => {
      cancelled = true;
      stop?.();
    };
  }, [projectPath]);

  // The Code tab opens the first script until somebody picks another.
  useEffect(() => {
    if (scripts.state !== "ready") return;
    if (openScript && scripts.data.some((script) => script.rel === openScript)) return;
    setOpenScript(scripts.data[0]?.rel ?? null);
  }, [scripts, openScript]);

  useEffect(() => {
    if (!openScript) {
      setScriptBody(IDLE);
      return;
    }
    let cancelled = false;
    setScriptBody({ state: "loading" });
    api
      .readFile(openScript)
      .then((file) => {
        if (!cancelled) setScriptBody({ state: "ready", data: file.text });
      })
      .catch((cause: unknown) => {
        if (!cancelled) setScriptBody({ state: "error", message: errorText(cause) });
      });
    return () => {
      cancelled = true;
    };
  }, [openScript]);

  // ── actions ──────────────────────────────────────────────────────────────────────
  const handleUpload = async () => {
    if (!projectPath || isUploading) return;
    setIsUploading(true);
    try {
      const picked = await open({ multiple: true, title: "Add an asset to this game" });
      const chosen = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
      for (const source of chosen) {
        const name = source.split(/[\\/]/).pop() ?? "asset";
        await api.importWorkspaceFile(source, `assets/${name}`);
      }
      if (chosen.length > 0) await loadAssets();
    } catch (cause) {
      setAssets({ state: "error", message: errorText(cause) });
    } finally {
      setIsUploading(false);
    }
  };

  const handleCreateVersion = async () => {
    if (!projectPath || newVersionLabel.trim().length === 0) return;
    setVersionBusy(true);
    try {
      const view = await api.godotCreateVersion(projectPath, newVersionLabel.trim());
      setVersions({ state: "ready", data: view });
      setNewVersionLabel("");
      setIsCreatingVersion(false);
    } catch (cause) {
      setVersions({ state: "error", message: errorText(cause) });
    } finally {
      setVersionBusy(false);
    }
  };

  const handleRevert = async (version: GameVersion) => {
    if (!projectPath || versionBusy) return;
    setVersionBusy(true);
    try {
      await api.godotRevertTo(projectPath, version.id);
      await loadVersions();
    } catch (cause) {
      setVersions({ state: "error", message: errorText(cause) });
    } finally {
      setVersionBusy(false);
    }
  };

  // ── derived views ────────────────────────────────────────────────────────────────
  const visibleAssets = useMemo(() => {
    if (assets.state !== "ready") return [];
    const needle = assetSearch.trim().toLowerCase();
    return assets.data.assets.filter((asset) => {
      if (sourceFilter !== "all" && (asset.provenance ?? "unknown") !== sourceFilter) return false;
      if (needle.length === 0) return true;
      return (
        asset.name.toLowerCase().includes(needle) ||
        asset.rel.toLowerCase().includes(needle) ||
        asset.kind_label.toLowerCase().includes(needle)
      );
    });
  }, [assets, assetSearch, sourceFilter]);

  const visibleGroups = useMemo(() => {
    if (library.state !== "ready") return [];
    const needle = librarySearch.trim().toLowerCase();
    return library.data.groups
      .map((group) => ({
        ...group,
        items:
          needle.length === 0
            ? group.items
            : group.items.filter((item) => item.search_text.includes(needle)),
      }))
      .filter((group) => group.items.length > 0);
  }, [library, librarySearch]);

  const versionCount = versions.state === "ready" ? versions.data.versions.length : 0;

  // ── panels ───────────────────────────────────────────────────────────────────────
  const renderEmpty = (message: string, action?: React.ReactNode) => (
    <div className="studio-dock-empty">
      <p>{message}</p>
      {action}
    </div>
  );

  const renderStatus = <T,>(
    panel: Loadable<T>,
    retry: () => void,
    busyLabel: string,
  ): React.ReactNode => {
    if (panel.state === "loading") {
      return (
        <div className="studio-dock-note" aria-busy="true">
          {busyLabel}
        </div>
      );
    }
    if (panel.state === "error") {
      return (
        <div className="studio-dock-error" role="alert">
          <span>{panel.message}</span>
          <button type="button" className="studio-action-btn" onClick={retry}>
            Retry
          </button>
        </div>
      );
    }
    return null;
  };

  const assetsPanel = () => {
    if (assetScope === "library") {
      return (
        <AssetLibraryPanel
          compact
          project={projectPath ? { path: projectPath, name: projectName } : null}
          onImported={() => void loadAssets()}
        />
      );
    }
    if (!projectPath) return renderEmpty("Open a game to see its assets.");
    const status = renderStatus(assets, () => void loadAssets(), "Scanning assets…");
    if (status) return status;
    if (assets.state !== "ready") return null;

    if (assets.data.assets.length === 0) {
      return renderEmpty(
        "No assets yet — Bhippi adds them as it builds, or upload one.",
        <button
          type="button"
          className="studio-action-btn"
          onClick={() => void handleUpload()}
          disabled={isUploading}
        >
          {isUploading ? "Adding…" : "Upload Asset"}
        </button>,
      );
    }

    return (
      <div>
        <div className="studio-dock-chips">
          <span className="studio-dock-chip-label">Source:</span>
          {assets.data.sources.map((facet) => (
            <button
              key={facet.id}
              type="button"
              className={`studio-dock-chip${sourceFilter === facet.id ? " active" : ""}`}
              aria-pressed={sourceFilter === facet.id}
              onClick={() => setSourceFilter(facet.id)}
            >
              {facet.label} ({facet.count})
            </button>
          ))}
          {assets.data.unlicensed_count > 0 ? (
            <span className="studio-dock-warn">
              {assets.data.unlicensed_count} without a recorded licence
            </span>
          ) : null}
        </div>

        {visibleAssets.length === 0 ? (
          renderEmpty(
            "No assets match this filter.",
            <button
              type="button"
              className="studio-action-btn"
              onClick={() => {
                setAssetSearch("");
                setSourceFilter("all");
              }}
            >
              Clear filter
            </button>,
          )
        ) : (
          <div className={viewMode === "grid" ? "assets-grid" : "studio-dock-rows"}>
            {visibleAssets.map((asset) => (
              <div key={asset.rel} className="asset-card" title={asset.rel}>
                <div className="asset-card-icon" style={{ fontSize: "20px" }}>
                  {KIND_ICON[asset.kind] ?? KIND_ICON.other}
                </div>
                <span className="asset-card-name">{asset.name}</span>
                <span className="asset-card-meta">
                  {asset.kind_label} · {formatBytes(asset.size_bytes)}
                </span>
                <span
                  className={asset.licence ? "studio-dock-licence" : "studio-dock-licence unknown"}
                >
                  {asset.licence ?? "unknown"}
                </span>
              </div>
            ))}
          </div>
        )}
        {assets.data.truncated ? (
          <p className="studio-dock-note">
            This project has more files than the dock lists. Open the Assets screen for the
            full library.
          </p>
        ) : null}
      </div>
    );
  };

  const libraryPanel = () => {
    const status = renderStatus(library, () => void loadLibrary(), "Loading the capability registry…");
    if (status) return status;
    if (library.state !== "ready") return null;
    if (library.data.groups.length === 0) {
      return renderEmpty("The capability registry is empty.");
    }
    return (
      <div className="studio-dock-sections">
        <p className="studio-dock-note">
          {library.data.total} capabilities · registry {library.data.registry_hash.slice(0, 8)}
        </p>
        {visibleGroups.length === 0
          ? renderEmpty("Nothing in the registry matches that.")
          : visibleGroups.map((group) => (
              <section key={group.label} aria-label={group.label}>
                <div className="studio-dock-section-head">
                  {group.label} ({group.items.length})
                </div>
                <div className="assets-grid">
                  {group.items.map((item) => (
                    <div
                      key={item.id}
                      className="asset-card"
                      title={item.unavailable_reason ?? item.purpose}
                    >
                      <div className="asset-card-icon" style={{ fontSize: "20px" }}>
                        📦
                      </div>
                      <span className="asset-card-name">{item.name}</span>
                      <span className="asset-card-meta">{item.category}</span>
                      {item.available ? null : (
                        <span className="studio-dock-licence unknown">unavailable</span>
                      )}
                    </div>
                  ))}
                </div>
              </section>
            ))}
      </div>
    );
  };

  const codePanel = () => {
    if (!projectPath) return renderEmpty("Open a game to read its scripts.");
    const status = renderStatus(scripts, () => void loadScripts(), "Looking for scripts…");
    if (status) return status;
    if (scripts.state !== "ready") return null;
    if (scripts.data.length === 0) {
      return renderEmpty("No GDScript files yet — Bhippi writes them as it builds.");
    }
    return (
      <div className="studio-dock-code">
        <ul className="studio-dock-filelist">
          {scripts.data.map((script) => (
            <li key={script.rel}>
              <button
                type="button"
                className={`studio-dock-fileitem${openScript === script.rel ? " active" : ""}`}
                aria-pressed={openScript === script.rel}
                onClick={() => setOpenScript(script.rel)}
                title={script.rel}
              >
                <span>{script.name}</span>
                <span className="studio-dock-filesize">{formatBytes(script.size_bytes)}</span>
              </button>
            </li>
          ))}
        </ul>
        <div className="studio-dock-codeview">
          <div className="studio-dock-section-head">{openScript ?? "No script selected"}</div>
          {scriptBody.state === "loading" ? (
            <div className="studio-dock-note" aria-busy="true">
              Reading…
            </div>
          ) : scriptBody.state === "error" ? (
            <div className="studio-dock-error" role="alert">
              {scriptBody.message}
            </div>
          ) : scriptBody.state === "ready" ? (
            <pre>{scriptBody.data}</pre>
          ) : null}
        </div>
      </div>
    );
  };

  const consolePanel = () => {
    if (!projectPath) return renderEmpty("Open a game to see its engine output.");
    if (consoleError) {
      return (
        <div className="studio-dock-error" role="alert">
          {consoleError}
        </div>
      );
    }
    if (consoleLines.length === 0) {
      return renderEmpty("No engine output yet — press Play and Godot's log lands here.");
    }
    return (
      <div className="studio-dock-console">
        {consoleLines.map((line) => (
          <div key={line.id} className="studio-dock-console-row">
            <span className={`studio-dock-stream ${line.stream}`}>{line.stream.toUpperCase()}</span>
            <span>{line.text}</span>
          </div>
        ))}
      </div>
    );
  };

  const versionsPanel = () => {
    if (!projectPath) return renderEmpty("Open a game to see its version history.");
    const status = renderStatus(versions, () => void loadVersions(), "Loading versions…");
    if (status) return status;
    if (versions.state !== "ready") return null;
    const view = versions.data;

    return (
      <div className="studio-dock-sections">
        <div className="studio-dock-section-head studio-dock-versions-head">
          <span>Journalled checkpoints for {projectName}</span>
          <button
            type="button"
            className="studio-action-btn"
            onClick={() => setIsCreatingVersion(true)}
            disabled={isCreatingVersion || versionBusy}
          >
            + Create version
          </button>
        </div>

        {view.notice ? <p className="studio-dock-note">{view.notice}</p> : null}

        {isCreatingVersion ? (
          <div className="studio-dock-versionform">
            <input
              type="text"
              value={newVersionLabel}
              autoFocus
              placeholder="What changed?"
              aria-label="Version label"
              onChange={(event) => setNewVersionLabel(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void handleCreateVersion();
                if (event.key === "Escape") setIsCreatingVersion(false);
              }}
            />
            <button
              type="button"
              className="studio-action-btn"
              onClick={() => void handleCreateVersion()}
              disabled={versionBusy || newVersionLabel.trim().length === 0}
            >
              {versionBusy ? "Saving…" : "Save"}
            </button>
            <button
              type="button"
              className="studio-action-btn"
              onClick={() => setIsCreatingVersion(false)}
            >
              Cancel
            </button>
          </div>
        ) : null}

        {view.versions.length === 0
          ? renderEmpty("No versions yet")
          : view.versions.map((version, index) => {
              const isCurrent = version.journal_revision === view.current_revision;
              return (
                <div
                  key={version.id}
                  className={`studio-dock-version${index === 0 ? " newest" : ""}`}
                >
                  <div>
                    <div className="studio-dock-version-title">
                      <span>{version.label}</span>
                      {isCurrent ? <span className="studio-dock-badge">CURRENT</span> : null}
                    </div>
                    <div className="studio-dock-note">
                      {formatWhen(version.created_at)} · journal r{version.journal_revision}
                      {version.export ? ` · exported ${version.export.target}` : ""}
                    </div>
                  </div>
                  {isCurrent ? null : (
                    <button
                      type="button"
                      className="studio-action-btn"
                      onClick={() => void handleRevert(version)}
                      disabled={versionBusy || view.revert_blocked !== null}
                      title={view.revert_blocked ?? "Replay the journal back to this point"}
                    >
                      Revert
                    </button>
                  )}
                </div>
              );
            })}
      </div>
    );
  };

  const DRAWER_TITLE: Record<StudioDockTab, string> = {
    assets: "Project Assets (res://assets)",
    library: "Node & Archetype Library",
    code: "GDScript Viewer",
    console: "Engine & Agent Console",
    versions: "Version History",
  };

  return (
    <>
      {activeTab && (
        <div className="studio-drawer">
          <header className="studio-drawer-header">
            <div className="studio-drawer-title">
              <span>{DRAWER_TITLE[activeTab]}</span>
            </div>

            <div className="studio-drawer-controls">
              {activeTab === "assets" && (
                <>
                  <div className="studio-dock-chips studio-dock-scope" role="group" aria-label="Asset source">
                    <button
                      type="button"
                      className={`studio-dock-chip${assetScope === "project" ? " active" : ""}`}
                      aria-pressed={assetScope === "project"}
                      onClick={() => setAssetScope("project")}
                    >
                      Project
                    </button>
                    <button
                      type="button"
                      className={`studio-dock-chip${assetScope === "library" ? " active" : ""}`}
                      aria-pressed={assetScope === "library"}
                      onClick={() => setAssetScope("library")}
                    >
                      Library
                    </button>
                  </div>
                  <div className="studio-drawer-search">
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <circle cx="11" cy="11" r="8" />
                      <line x1="21" y1="21" x2="16.65" y2="16.65" />
                    </svg>
                    <input
                      type="search"
                      placeholder="Filter assets..."
                      aria-label="Filter assets"
                      value={assetSearch}
                      onChange={(event) => setAssetSearch(event.target.value)}
                      disabled={assets.state !== "ready"}
                    />
                  </div>
                  <button
                    type="button"
                    className="studio-action-btn"
                    onClick={() => void handleUpload()}
                    disabled={!projectPath || isUploading}
                  >
                    <span>{isUploading ? "Adding…" : "Upload Asset"}</span>
                  </button>
                  <button
                    type="button"
                    className="studio-action-btn"
                    onClick={() => void loadAssets()}
                    disabled={!projectPath || assets.state === "loading"}
                  >
                    <span>Refresh</span>
                  </button>
                </>
              )}

              {activeTab === "library" && (
                <div className="studio-drawer-search">
                  <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                  </svg>
                  <input
                    type="search"
                    placeholder="Filter capabilities..."
                    aria-label="Filter capabilities"
                    value={librarySearch}
                    onChange={(event) => setLibrarySearch(event.target.value)}
                    disabled={library.state !== "ready"}
                  />
                </div>
              )}

              {activeTab === "console" && consoleLines.length > 0 && (
                <button
                  type="button"
                  className="studio-action-btn"
                  onClick={() => setConsoleLines([])}
                >
                  <span>Clear</span>
                </button>
              )}

              <button
                type="button"
                className="studio-chat-close"
                onClick={() => onSelectTab(null)}
                title="Collapse drawer"
              >
                ✕
              </button>
            </div>
          </header>

          <div className="studio-drawer-body">
            {activeTab === "assets" && assetsPanel()}
            {activeTab === "library" && libraryPanel()}
            {activeTab === "code" && codePanel()}
            {activeTab === "console" && consolePanel()}
            {activeTab === "versions" && versionsPanel()}
          </div>
        </div>
      )}

      {/* Bottom Dock Bar */}
      <footer className="studio-bottom-dock">
        <div className="studio-dock-tabs" role="tablist" aria-label="Studio Dock Panels">
          <button
            type="button"
            className={`studio-dock-tab ${activeTab === "assets" ? "active" : ""}`}
            onClick={() => onSelectTab(activeTab === "assets" ? null : "assets")}
            role="tab"
            aria-selected={activeTab === "assets"}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
            </svg>
            <span>Assets</span>
          </button>

          <button
            type="button"
            className={`studio-dock-tab ${activeTab === "library" ? "active" : ""}`}
            onClick={() => onSelectTab(activeTab === "library" ? null : "library")}
            role="tab"
            aria-selected={activeTab === "library"}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
              <path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" />
            </svg>
            <span>Library</span>
          </button>

          <button
            type="button"
            className={`studio-dock-tab ${activeTab === "code" ? "active" : ""}`}
            onClick={() => onSelectTab(activeTab === "code" ? null : "code")}
            role="tab"
            aria-selected={activeTab === "code"}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="16 18 22 12 16 6" />
              <polyline points="8 6 2 12 8 18" />
            </svg>
            <span>Code</span>
          </button>

          <button
            type="button"
            className={`studio-dock-tab ${activeTab === "console" ? "active" : ""}`}
            onClick={() => onSelectTab(activeTab === "console" ? null : "console")}
            role="tab"
            aria-selected={activeTab === "console"}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <polyline points="4 17 10 11 4 5" />
              <line x1="12" y1="19" x2="20" y2="19" />
            </svg>
            <span>Console</span>
            {consoleLines.length > 0 ? <span className="studio-tab-badge-dot" /> : null}
          </button>

          <button
            type="button"
            className={`studio-dock-tab ${activeTab === "versions" ? "active" : ""}`}
            onClick={() => onSelectTab(activeTab === "versions" ? null : "versions")}
            role="tab"
            aria-selected={activeTab === "versions"}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="12" cy="12" r="10" />
              <polyline points="12 6 12 12 16 14" />
            </svg>
            <span>Versions</span>
            {versionCount > 0 ? (
              <span className="studio-dock-count" title={`${versionCount} saved versions`}>
                {versionCount}
              </span>
            ) : null}
          </button>
        </div>

        <div className="studio-dock-actions">
          <button
            type="button"
            className="studio-dock-icon-btn"
            title="Toggle View Mode"
            onClick={() => setViewMode((mode) => (mode === "grid" ? "list" : "grid"))}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <line x1="8" y1="6" x2="21" y2="6" />
              <line x1="8" y1="12" x2="21" y2="12" />
              <line x1="8" y1="18" x2="21" y2="18" />
              <line x1="3" y1="6" x2="3.01" y2="6" />
              <line x1="3" y1="12" x2="3.01" y2="12" />
              <line x1="3" y1="18" x2="3.01" y2="18" />
            </svg>
          </button>

          <button
            type="button"
            className="studio-dock-icon-btn"
            title="Expand / Collapse Dock"
            onClick={() => onSelectTab(activeTab ? null : "assets")}
          >
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <rect x="3" y="3" width="18" height="18" rx="2" />
              <line x1="3" y1="15" x2="21" y2="15" />
            </svg>
          </button>
        </div>
      </footer>
    </>
  );
}
