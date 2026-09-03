// The user's asset library (SPA-103): folders Bhippi may take assets from, and the search
// across them. Shared by the Assets screen and the Studio dock's Assets tab.
//
// Rust owns every fact here — which folders exist, what is in them, what a file is, what
// its licence says, and what path an import landed on (`asset_library_*`). This panel adds
// the folder picker, the search box and the buttons; it classifies nothing and writes
// nothing itself. The same folders are what the agent sees in its prompt, so what the user
// registers here is exactly what the AI may draw on.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import type {
  AssetLibraryView,
  LibraryAsset,
  ProjectAsset,
  ProjectAssetKind,
} from "../lib/ipc";

import { IconClose, IconFolder, IconPlus, IconSearch } from "./icons";

/** The two facts an import needs about the open game. */
export type LibraryTarget = { path: string; name: string };

type Loadable<T> =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "ready"; data: T }
  | { state: "error"; message: string };

const KIND_FILTERS: { id: ProjectAssetKind | "all"; label: string }[] = [
  { id: "all", label: "All" },
  { id: "model", label: "Models" },
  { id: "texture", label: "Textures" },
  { id: "audio", label: "Audio" },
  { id: "scene", label: "Scenes" },
  { id: "material", label: "Materials" },
  { id: "shader", label: "Shaders" },
  { id: "other", label: "Other" },
];

const KIND_GLYPH: Record<ProjectAssetKind, string> = {
  model: "🧊",
  texture: "🖼️",
  audio: "🎵",
  scene: "🎬",
  material: "🎨",
  shader: "✨",
  other: "📄",
};

/** How many results the panel asks for. Rust caps harder; this keeps the grid readable. */
const SEARCH_LIMIT = 120;

function errorText(cause: unknown): string {
  const value = cause as { message?: string; hint?: string } | undefined;
  if (value && typeof value.message === "string") {
    return value.hint ? `${value.message} ${value.hint}` : value.message;
  }
  return String(cause);
}

/** Bytes as the cards show them — never "0.0 KB" for a real file. */
export function formatLibraryBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

type AssetLibraryPanelProps = {
  /** The game an import lands in; `null` disables the Add buttons with a reason. */
  project: LibraryTarget | null;
  /** The dock is short; compact drops the explainer and tightens the grid. */
  compact?: boolean;
  /** Fired after a successful import so the caller can refresh its project list. */
  onImported?: (asset: ProjectAsset) => void;
};

export function AssetLibraryPanel({ project, compact = false, onImported }: AssetLibraryPanelProps) {
  const [library, setLibrary] = useState<Loadable<AssetLibraryView>>({ state: "idle" });
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<ProjectAssetKind | "all">("all");
  const [results, setResults] = useState<Loadable<LibraryAsset[]>>({ state: "idle" });
  const [importing, setImporting] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const searchSeq = useRef(0);

  const loadLibrary = useCallback(async () => {
    setLibrary({ state: "loading" });
    try {
      setLibrary({ state: "ready", data: await api.assetLibraryList() });
    } catch (cause) {
      setLibrary({ state: "error", message: errorText(cause) });
    }
  }, []);

  useEffect(() => {
    void loadLibrary();
  }, [loadLibrary]);

  const folders = library.state === "ready" ? library.data.folders : [];
  const hasFolders = folders.length > 0;

  // Search follows the box and the chips, coalesced so typing does not fan out a scan per
  // keystroke. A stale answer is dropped by sequence number, never shown out of order.
  useEffect(() => {
    if (!hasFolders) {
      setResults({ state: "idle" });
      return undefined;
    }
    const seq = ++searchSeq.current;
    setResults({ state: "loading" });
    const timer = window.setTimeout(() => {
      void api
        .assetLibrarySearch(query.trim() || null, kind === "all" ? null : kind, SEARCH_LIMIT)
        .then((rows) => {
          if (seq === searchSeq.current) setResults({ state: "ready", data: rows });
        })
        .catch((cause: unknown) => {
          if (seq === searchSeq.current) setResults({ state: "error", message: errorText(cause) });
        });
    }, 180);
    return () => window.clearTimeout(timer);
  }, [query, kind, hasFolders, library]);

  const addFolder = async () => {
    if (adding) return;
    setAdding(true);
    setNotice(null);
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Add a folder Bhippi may take assets from",
      });
      if (typeof picked !== "string" || !picked) return;
      setLibrary({ state: "ready", data: await api.assetLibraryAdd(picked) });
    } catch (cause) {
      setLibrary({ state: "error", message: errorText(cause) });
    } finally {
      setAdding(false);
    }
  };

  const removeFolder = async (path: string) => {
    setNotice(null);
    try {
      setLibrary({ state: "ready", data: await api.assetLibraryRemove(path) });
    } catch (cause) {
      setLibrary({ state: "error", message: errorText(cause) });
    }
  };

  const importAsset = async (asset: LibraryAsset) => {
    if (!project || importing) return;
    setImporting(asset.path);
    setNotice(null);
    try {
      const landed = await api.assetLibraryImport(project.path, asset.path, null);
      setNotice(`Added ${landed.name} as ${landed.rel}`);
      onImported?.(landed);
    } catch (cause) {
      setNotice(`Could not add ${asset.name}: ${errorText(cause)}`);
    } finally {
      setImporting(null);
    }
  };

  const totalFiles = library.state === "ready" ? library.data.total_files : 0;
  const summary = useMemo(() => {
    if (!hasFolders) return "";
    const missing = folders.filter((folder) => !folder.exists).length;
    return `${folders.length} ${folders.length === 1 ? "folder" : "folders"} · ${totalFiles} files${
      missing > 0 ? ` · ${missing} missing` : ""
    }`;
  }, [folders, hasFolders, totalFiles]);

  return (
    <section className={`asset-library${compact ? " compact" : ""}`} aria-label="Asset library">
      <header className="asset-library-head">
        <div className="asset-library-title">
          <IconFolder size={14} />
          <strong>Library folders</strong>
          {summary ? <span className="asset-library-summary">{summary}</span> : null}
        </div>
        <button
          type="button"
          className="studio-action-btn asset-library-add"
          onClick={() => void addFolder()}
          disabled={adding || library.state === "loading"}
        >
          <IconPlus size={12} />
          <span>{adding ? "Choosing…" : "Add folder"}</span>
        </button>
      </header>

      {!compact ? (
        <p className="asset-library-note">
          Point Bhippi at folders of models, textures and sounds you already own. Bhippi lists
          them to the AI, which can pull any of them into <code>assets/</code> with a licence
          sidecar. Folders are only ever read.
        </p>
      ) : null}

      {library.state === "error" ? (
        <div className="studio-dock-error" role="alert">
          <span>{library.message}</span>
          <button type="button" className="studio-action-btn" onClick={() => void loadLibrary()}>
            Retry
          </button>
        </div>
      ) : null}

      {library.state === "loading" ? (
        <div className="studio-dock-note" aria-busy="true">
          Reading the library…
        </div>
      ) : null}

      {library.state === "ready" && !hasFolders ? (
        <div className="asset-library-empty">
          <p>No library folders yet. Add one and the AI can use anything in it.</p>
        </div>
      ) : null}

      {hasFolders ? (
        <>
          <div className="asset-library-folders" role="list" aria-label="Registered folders">
            {folders.map((folder) => (
              <div
                key={folder.path}
                role="listitem"
                className={`asset-library-folder${folder.exists ? "" : " missing"}`}
                title={folder.path}
              >
                <IconFolder size={12} />
                <span className="asset-library-folder-name">{folder.name}</span>
                <span className="asset-library-folder-meta">
                  {folder.exists
                    ? `${folder.file_count}${folder.truncated ? "+" : ""} files${
                        folder.licence ? ` · ${folder.licence}` : ""
                      }`
                    : "missing"}
                </span>
                <button
                  type="button"
                  className="asset-library-folder-remove"
                  onClick={() => void removeFolder(folder.path)}
                  title="Forget this folder (the folder itself is untouched)"
                  aria-label={`Forget ${folder.name}`}
                >
                  <IconClose size={10} />
                </button>
              </div>
            ))}
          </div>

          <div className="asset-library-tools">
            <div className="studio-drawer-search asset-library-search">
              <IconSearch size={12} />
              <input
                type="search"
                placeholder="Search the library…"
                aria-label="Search the library"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
            <div className="studio-dock-chips asset-library-kinds" role="group" aria-label="Kind">
              {KIND_FILTERS.map((filter) => (
                <button
                  key={filter.id}
                  type="button"
                  className={`studio-dock-chip${kind === filter.id ? " active" : ""}`}
                  aria-pressed={kind === filter.id}
                  onClick={() => setKind(filter.id)}
                >
                  {filter.label}
                </button>
              ))}
            </div>
          </div>

          {notice ? (
            <div className="studio-dock-note asset-library-notice" role="status">
              {notice}
            </div>
          ) : null}

          {results.state === "loading" ? (
            <div className="studio-dock-note" aria-busy="true">
              Searching…
            </div>
          ) : results.state === "error" ? (
            <div className="studio-dock-error" role="alert">
              {results.message}
            </div>
          ) : results.state === "ready" && results.data.length === 0 ? (
            <div className="studio-dock-empty">
              <p>Nothing in the library matches that.</p>
            </div>
          ) : results.state === "ready" ? (
            <div className="assets-grid asset-library-grid">
              {results.data.map((asset) => (
                <div key={asset.path} className="asset-card asset-library-card" title={asset.path}>
                  <div className="asset-card-icon" style={{ fontSize: "20px" }}>
                    {KIND_GLYPH[asset.kind] ?? KIND_GLYPH.other}
                  </div>
                  <span className="asset-card-name">{asset.name}</span>
                  <span className="asset-card-meta">
                    {asset.kind_label} · {formatLibraryBytes(asset.size_bytes)}
                  </span>
                  <span className="asset-library-card-rel">{asset.rel}</span>
                  <span
                    className={asset.licence ? "studio-dock-licence" : "studio-dock-licence unknown"}
                  >
                    {asset.licence ?? "unknown"}
                  </span>
                  <button
                    type="button"
                    className="studio-action-btn asset-library-import"
                    onClick={() => void importAsset(asset)}
                    disabled={!project || importing !== null}
                    title={
                      project
                        ? `Copy into ${project.name}/assets with a licence sidecar`
                        : "Open a game to add assets to it"
                    }
                  >
                    {importing === asset.path ? "Adding…" : "Add to project"}
                  </button>
                </div>
              ))}
              {results.data.length >= SEARCH_LIMIT ? (
                <p className="studio-dock-note asset-library-more">
                  Showing the first {SEARCH_LIMIT}. Narrow the search to see the rest.
                </p>
              ) : null}
            </div>
          ) : null}
        </>
      ) : null}
    </section>
  );
}
