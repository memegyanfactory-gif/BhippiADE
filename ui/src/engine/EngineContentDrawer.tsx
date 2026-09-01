import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import { Markdown } from "../components/Markdown";
import {
  IconBox,
  IconClose,
  IconCode,
  IconFolder,
  IconFolderOpen,
  IconGauge,
  IconGrid,
  IconMaximize2,
  IconMinimize2,
  IconPlus,
  IconSearch,
  IconTerminal,
  IconVolume,
} from "../components/icons";

export interface AssetItem {
  name: string;
  path: string;
  type: "scene" | "model" | "texture" | "audio" | "script" | "material" | "generic";
  size?: string;
  entityCount?: number;
}

interface Props {
  currentScenePath?: string;
  onSelectScene: (scenePath: string) => void;
  onNewScene?: () => void;
  isCollapsed?: boolean;
  onToggleCollapse?: () => void;
  gameRoot?: string | null;
  isGame?: boolean;
  onReplaceObject?: (assetPath: string) => void;
  onImportReplace?: () => void;
  onApplyAsset?: (assetPath: string) => void;
  activeTab: EngineDrawerTab;
  onActiveTabChange: (tab: EngineDrawerTab) => void;
  outputLog: ReactNode;
  problems: ReactNode;
  height?: number;
  onHeightChange?: (height: number) => void;
  gameDebugRefreshToken?: number;
}

export type EngineDrawerTab = "content" | "output" | "problems" | "activity" | "game-debug" | "builds";

const DRAWER_TABS: { id: EngineDrawerTab; label: string }[] = [
  { id: "content", label: "Content" },
  { id: "output", label: "Output" },
  { id: "problems", label: "Problems" },
  { id: "activity", label: "AI Activity" },
  { id: "game-debug", label: "Game Debug" },
  { id: "builds", label: "Build Targets" },
];

const DEFAULT_FOLDERS = [
  { id: "assets", name: "assets", parent: "root" },
  { id: "assets/scenes", name: "scenes", parent: "assets" },
  { id: "assets/models", name: "models", parent: "assets" },
  { id: "assets/textures", name: "textures", parent: "assets" },
  { id: "assets/audio", name: "audio", parent: "assets" },
  { id: "assets/materials", name: "materials", parent: "assets" },
  { id: "assets/shaders", name: "shaders", parent: "assets" },
  { id: "assets/weather", name: "weather", parent: "assets" },
  { id: "scripts", name: "scripts", parent: "root" },
  { id: "builds", name: "builds", parent: "root" },
];

function classifyAsset(name: string, path: string): AssetItem["type"] {
  const lower = name.toLowerCase();
  if (lower.endsWith(".bscn.json")) return "scene";
  if (lower.endsWith(".glb") || lower.endsWith(".gltf") || lower.endsWith(".obj") || lower.endsWith(".fbx")) return "model";
  if (/\.(png|jpe?g|tga|exr|hdr|ktx2)$/.test(lower)) return "texture";
  if (/\.(wav|ogg|mp3|flac)$/.test(lower)) return "audio";
  if (lower.endsWith(".mat.json") || lower.endsWith(".mat") || path.includes("/materials/")) return "material";
  if (lower.endsWith(".shader.json") || lower.endsWith(".wgsl") || lower.endsWith(".glsl")) return "material";
  if (lower.endsWith(".rhai") || lower.endsWith(".rs")) return "script";
  return "generic";
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function EngineContentDrawer({
  currentScenePath,
  onSelectScene,
  onNewScene,
  isCollapsed = false,
  onToggleCollapse,
  isGame = false,
  onReplaceObject,
  onImportReplace,
  onApplyAsset,
  activeTab,
  onActiveTabChange,
  outputLog,
  problems,
  height = 205,
  onHeightChange,
  gameDebugRefreshToken = 0,
}: Props) {
  const [currentFolder, setCurrentFolder] = useState("assets/scenes");
  const [search, setSearch] = useState("");
  const [viewMode, setViewMode] = useState<"grid" | "list">("grid");
  const [folderItems, setFolderItems] = useState<AssetItem[]>([]);
  const [menu, setMenu] = useState<{ x: number; y: number; item: AssetItem } | null>(null);
  const [gameDebugReport, setGameDebugReport] = useState<{ state: "loading" | "empty" | "error" | "ready"; text?: string; message?: string }>({ state: "empty" });
  const resizeCleanup = useRef<(() => void) | null>(null);

  const clampHeight = useCallback((next: number) => {
    const available = typeof window === "undefined" ? 480 : window.innerHeight;
    return Math.round(Math.max(160, Math.min(next, available * 0.62)));
  }, []);

  const resizeBy = useCallback((delta: number) => {
    onHeightChange?.(clampHeight(height + delta));
  }, [clampHeight, height, onHeightChange]);

  useEffect(() => () => resizeCleanup.current?.(), []);

  useEffect(() => {
    if (activeTab !== "game-debug" || !isGame) return;
    let cancelled = false;
    setGameDebugReport({ state: "loading" });
    void (async () => {
      try {
        const latest = await api.readFile(".bhippi/reports/game-debug/latest.json");
        const pointer = JSON.parse(latest.text) as { run_id?: unknown };
        if (typeof pointer.run_id !== "string" || !/^[0-9A-HJKMNP-TV-Z]{26}$/.test(pointer.run_id)) {
          throw new Error("The latest report pointer is invalid.");
        }
        const report = await api.readFile(`.bhippi/reports/game-debug/${pointer.run_id}.md`);
        if (!cancelled) setGameDebugReport({ state: "ready", text: report.text });
      } catch (error) {
        const message = String((error as { message?: string }).message ?? error);
        if (!cancelled) {
          setGameDebugReport(message.toLowerCase().includes("not found")
            ? { state: "empty" }
            : { state: "error", message });
        }
      }
    })();
    return () => { cancelled = true; };
  }, [activeTab, gameDebugRefreshToken, isGame]);

  const beginResize = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (!onHeightChange) return;
    event.preventDefault();
    const startY = event.clientY;
    const startHeight = height;
    const move = (moveEvent: PointerEvent) => {
      onHeightChange(clampHeight(startHeight + startY - moveEvent.clientY));
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      resizeCleanup.current = null;
    };
    resizeCleanup.current?.();
    resizeCleanup.current = stop;
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop, { once: true });
  }, [clampHeight, height, onHeightChange]);

  useEffect(() => {
    let cancelled = false;
    if (!isGame) {
      setFolderItems([]);
      return;
    }
    void (async () => {
      try {
        const entries = await api.workspaceDir(currentFolder);
        if (cancelled) return;
        setFolderItems(
          entries
            .filter((entry) => !entry.is_directory)
            .map((entry) => ({
              name: entry.name,
              path: entry.path,
              type: classifyAsset(entry.name, entry.path),
              size: formatSize(entry.size),
            })),
        );
      } catch {
        if (!cancelled) setFolderItems([]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [currentFolder, isGame, currentScenePath]);

  const itemsInCurrentFolder = useMemo(() => {
    if (!search.trim()) return folderItems;
    const q = search.trim().toLowerCase();
    return folderItems.filter((it) => it.name.toLowerCase().includes(q) || it.type.includes(q));
  }, [folderItems, search]);

  const breadcrumbs = useMemo(() => {
    const parts = currentFolder.split("/").filter(Boolean);
    return ["Content", ...parts];
  }, [currentFolder]);

  const getAssetGlyph = (item: AssetItem) => {
    switch (item.type) {
      case "scene":
        return <IconBox size={22} className="asset-glyph-svg scene" />;
      case "model":
        return <IconBox size={22} className="asset-glyph-svg model" />;
      case "texture":
        return <IconGrid size={22} className="asset-glyph-svg texture" />;
      case "audio":
        return <IconVolume size={22} className="asset-glyph-svg audio" />;
      case "script":
        return <IconCode size={22} className="asset-glyph-svg script" />;
      default:
        return <IconFolder size={22} className="asset-glyph-svg generic" />;
    }
  };

  if (isCollapsed) {
    return (
      <aside className="engine-content-drawer collapsed" aria-label="Bottom drawer">
        <div className="drawer-collapsed-tabs">
          {DRAWER_TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              className={`drawer-expand-btn${activeTab === tab.id ? " active" : ""}`}
              onClick={() => {
                onActiveTabChange(tab.id);
                onToggleCollapse?.();
              }}
            >
              {tab.id === "content" ? <IconFolderOpen size={13} /> : null}
              {tab.id === "output" ? <IconTerminal size={13} /> : null}
              {tab.id === "builds" ? <IconGauge size={13} /> : null}
              <span>{tab.label}</span>
            </button>
          ))}
          <IconMaximize2 size={11} className="drawer-collapsed-expand" aria-hidden="true" />
        </div>
      </aside>
    );
  }

  return (
    <aside className="engine-content-drawer" aria-label="Bottom drawer" style={{ height }}>
      <div
        className="drawer-resize-handle"
        role="separator"
        aria-label="Resize bottom drawer"
        aria-orientation="horizontal"
        aria-valuemin={160}
        aria-valuemax={Math.round((typeof window === "undefined" ? 480 : window.innerHeight) * 0.62)}
        aria-valuenow={height}
        tabIndex={0}
        onPointerDown={beginResize}
        onKeyDown={(event) => {
          if (event.key === "ArrowUp") {
            event.preventDefault();
            resizeBy(16);
          } else if (event.key === "ArrowDown") {
            event.preventDefault();
            resizeBy(-16);
          } else if (event.key === "Home") {
            event.preventDefault();
            onHeightChange?.(160);
          }
        }}
      />
      {/* Top Drawer Tab Bar */}
      <div className="drawer-header-bar">
        <div className="drawer-tabs">
          {DRAWER_TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              className={`drawer-tab${activeTab === tab.id ? " active" : ""}`}
              onClick={() => onActiveTabChange(tab.id)}
            >
              {tab.id === "content" ? <IconFolderOpen size={13} /> : null}
              {tab.id === "output" ? <IconTerminal size={13} /> : null}
              {tab.id === "builds" ? <IconGauge size={13} /> : null}
              <span>{tab.label}</span>
            </button>
          ))}
        </div>

        <div className="drawer-header-tools">
          {activeTab === "content" ? <button
            type="button"
            className="drawer-tool-btn"
            onClick={() => setViewMode(viewMode === "grid" ? "list" : "grid")}
            title={`Switch to ${viewMode === "grid" ? "List" : "Grid"} view`}
          >
            <IconGrid size={12} />
          </button> : null}
          <button
            type="button"
            className="drawer-tool-btn"
            onClick={onToggleCollapse}
            title="Minimize Content Drawer"
          >
            <IconMinimize2 size={12} />
          </button>
        </div>
      </div>

      {/* Main Drawer Body */}
      {activeTab === "content" ? (
        <div className="drawer-body-split">
          {/* Left Folder Tree */}
          <div className="drawer-tree-pane">
            <div className="tree-header">
              <span className="tree-title">Folders</span>
            </div>
            <div className="tree-list">
              <div
                className={`tree-node${currentFolder === "assets" ? " active" : ""}`}
                onClick={() => setCurrentFolder("assets")}
              >
                <IconFolderOpen size={12} />
                <span>assets</span>
              </div>
              <div className="tree-sub-list">
                {DEFAULT_FOLDERS.filter((f) => f.parent === "assets").map((f) => (
                  <div
                    key={f.id}
                    className={`tree-node sub${currentFolder === f.id ? " active" : ""}`}
                    onClick={() => setCurrentFolder(f.id)}
                  >
                    <IconFolder size={11} />
                    <span>{f.name}</span>
                  </div>
                ))}
              </div>
              <div
                className={`tree-node${currentFolder === "scripts" ? " active" : ""}`}
                onClick={() => setCurrentFolder("scripts")}
              >
                <IconCode size={12} />
                <span>scripts</span>
              </div>
              <div
                className={`tree-node${currentFolder === "builds" ? " active" : ""}`}
                onClick={() => setCurrentFolder("builds")}
              >
                <IconGauge size={12} />
                <span>builds</span>
              </div>
            </div>
          </div>

          {/* Right Asset Explorer Pane */}
          <div className="drawer-assets-pane">
            {/* Breadcrumb & Filter toolbar */}
            <div className="assets-toolbar">
              <div className="assets-breadcrumbs">
                {breadcrumbs.map((crumb, idx) => (
                  <span key={crumb} className="crumb-segment">
                    <span className="crumb-text">{crumb}</span>
                    {idx < breadcrumbs.length - 1 && <span className="crumb-sep">/</span>}
                  </span>
                ))}
              </div>

              <div className="assets-actions">
                <div className="assets-search-box">
                  <IconSearch size={11} className="search-icon" />
                  <input
                    type="text"
                    placeholder="Search assets..."
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                  />
                  {search ? (
                    <button type="button" className="clear-btn" onClick={() => setSearch("")}>
                      <IconClose size={10} />
                    </button>
                  ) : null}
                </div>

                <button
                  type="button"
                  className="engine-mini-btn primary"
                  onClick={onNewScene}
                  title="Create New Scene"
                >
                  <IconPlus size={11} />
                  <span>New Scene</span>
                </button>
              </div>
            </div>

            {/* Asset Items Grid/List */}
            <div className={`assets-container ${viewMode}`}>
              {itemsInCurrentFolder.length === 0 ? (
                <div className="engine-empty-hint">
                  {!isGame
                    ? "No game in this folder. The viewport stays empty until you create one."
                    : search
                      ? "No assets match search."
                      : "Folder is empty."}
                </div>
              ) : (
                itemsInCurrentFolder.map((item) => {
                  const isCurrentScene = currentScenePath && currentScenePath.endsWith(item.name);
                  return (
                    <div
                      key={item.path}
                      className={`asset-card${isCurrentScene ? " active-scene" : ""}`}
                      draggable
                      onDragStart={(e) => {
                        e.dataTransfer.setData("text/bhippi-asset", item.path);
                        e.dataTransfer.effectAllowed = "copy";
                      }}
                      onDoubleClick={() => {
                        if (item.type === "scene") {
                          onSelectScene(item.path);
                        } else {
                          onApplyAsset?.(item.path);
                        }
                      }}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        setMenu({ x: e.clientX, y: e.clientY, item });
                      }}
                      title={`${item.name} (${item.type}) — Double-click to open / apply`}
                    >
                      <div className="asset-thumbnail">
                        {getAssetGlyph(item)}
                        {item.type === "scene" ? (
                          <span className="asset-type-badge">SCENE</span>
                        ) : null}
                        {isCurrentScene ? (
                          <span className="asset-active-dot" title="Active in Viewport" />
                        ) : null}
                      </div>
                      <div className="asset-info">
                        <span className="asset-name" title={item.name}>{item.name}</span>
                        <div className="asset-meta">
                          {item.entityCount ? (
                            <span>{item.entityCount} entities</span>
                          ) : (
                            <span>{item.size || item.type}</span>
                          )}
                        </div>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </div>
      ) : activeTab === "output" ? (
        outputLog
      ) : activeTab === "problems" ? (
        problems
      ) : activeTab === "activity" ? (
        <div className="engine-drawer-empty"><strong>No AI activity in this view</strong><span>Applied AI changes remain available in Output with their actor and undo action.</span></div>
      ) : activeTab === "game-debug" ? (
        gameDebugReport.state === "loading" ? (
          <div className="engine-drawer-empty" role="status"><strong>Loading latest game-debug report…</strong></div>
        ) : gameDebugReport.state === "ready" ? (
          <div className="game-debug-report" aria-label="Latest game-debug report"><Markdown text={gameDebugReport.text ?? ""} /></div>
        ) : gameDebugReport.state === "error" ? (
          <div className="engine-drawer-empty" role="alert"><strong>Could not open the game-debug report</strong><span>{gameDebugReport.message}</span></div>
        ) : (
          <div className="engine-drawer-empty"><strong>No game-debug report open</strong><span>Run <code>/gamedebug</code> in chat; immutable reports are stored under .bhippi/reports/game-debug.</span></div>
        )
      ) : (
        <div className="drawer-builds-pane">
          <div className="build-targets-grid">
            <div className="build-target-card">
              <div className="target-head">
                <strong>Windows (x86_64)</strong>
                <span className="target-status ready">Ready</span>
              </div>
              <p>Native DirectX12/Vulkan executable with high-framerate rendering.</p>
              <button type="button" className="engine-mini-btn primary">Build Windows</button>
            </div>
            <div className="build-target-card">
              <div className="target-head">
                <strong>Web / WASM</strong>
                <span className="target-status ready">Ready</span>
              </div>
              <p>WebGL2 / WebGPU canvas build for browser playtesting and web deployment.</p>
              <button type="button" className="engine-mini-btn primary">Build WASM</button>
            </div>
            <div className="build-target-card">
              <div className="target-head">
                <strong>Android (APK)</strong>
                <span className="target-status">Configured</span>
              </div>
              <p>ARM64 native mobile build with touch input mapping.</p>
              <button type="button" className="engine-mini-btn">Package APK</button>
            </div>
          </div>
        </div>
      )}

      {menu ? (
        <div
          className="engine-context-menu"
          style={{ left: menu.x, top: menu.y }}
          onMouseLeave={() => setMenu(null)}
        >
          {menu.item.type === "scene" ? (
            <button
              type="button"
              onClick={() => {
                onSelectScene(menu.item.path);
                setMenu(null);
              }}
            >
              Open
            </button>
          ) : null}
          {menu.item.type === "model" ? (
            <button
              type="button"
              onClick={() => {
                onReplaceObject?.(menu.item.path);
                setMenu(null);
              }}
            >
              Replace Object
            </button>
          ) : null}
          <button
            type="button"
            onClick={() => {
              onApplyAsset?.(menu.item.path);
              setMenu(null);
            }}
          >
            Apply to selected
          </button>
          <button
            type="button"
            onClick={() => {
              onImportReplace?.();
              setMenu(null);
            }}
          >
            Replace from disk…
          </button>
        </div>
      ) : null}
    </aside>
  );
}
