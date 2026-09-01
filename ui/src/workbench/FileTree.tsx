import { useCallback, useEffect, useState } from "react";
import type { WorkspaceEntry } from "../lib/ipc";
import { api } from "../lib/api";
import { FileGlyph, IconChevronRight, IconFolder, IconFolderOpen, IconRefresh } from "../components/icons";

/**
 * The editor's file navigator.
 *
 * Directories are fetched the moment they are expanded and cached afterwards, so the
 * first paint of a large repository costs one listing of the root rather than a walk of
 * everything under it. Rust does the walking, the skipping, and the ordering; this
 * component only decides what is open (ADR-0013, R3).
 */
export function FileTree({
  activePath,
  onOpen,
  refreshToken,
}: {
  activePath: string | null;
  onOpen: (entry: WorkspaceEntry) => void;
  /** Bumped by the parent to force a re-read after a save or a project switch. */
  refreshToken: number;
}) {
  const [children, setChildren] = useState<Record<string, WorkspaceEntry[]>>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set([""]));
  const [loading, setLoading] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async (path: string) => {
    setLoading((current) => new Set(current).add(path));
    try {
      const rows = await api.workspaceDir(path);
      setChildren((current) => ({ ...current, [path]: rows }));
      setError(null);
    } catch (loadError) {
      setError(String((loadError as { message?: string }).message ?? loadError));
    } finally {
      setLoading((current) => {
        const next = new Set(current);
        next.delete(path);
        return next;
      });
    }
  }, []);

  // A project switch or an external change invalidates every cached listing at once —
  // keeping stale folders open would show files that are no longer there.
  useEffect(() => {
    setChildren({});
    setExpanded(new Set([""]));
    void load("");
  }, [load, refreshToken]);

  const toggle = useCallback(
    (entry: WorkspaceEntry) => {
      setExpanded((current) => {
        const next = new Set(current);
        if (next.has(entry.path)) {
          next.delete(entry.path);
        } else {
          next.add(entry.path);
          if (!children[entry.path]) void load(entry.path);
        }
        return next;
      });
    },
    [children, load],
  );

  const renderLevel = (path: string, depth: number): JSX.Element[] => {
    const rows = children[path];
    if (!rows) {
      return loading.has(path)
        ? [
            <div key={`${path}-loading`} className="tree-note" style={{ paddingLeft: 12 + depth * 13 }}>
              Reading…
            </div>,
          ]
        : [];
    }
    if (rows.length === 0) {
      return [
        <div key={`${path}-empty`} className="tree-note" style={{ paddingLeft: 12 + depth * 13 }}>
          Empty folder
        </div>,
      ];
    }
    return rows.flatMap((entry) => {
      const open = expanded.has(entry.path);
      const row = (
        <button
          key={entry.path}
          className={`tree-row${entry.path === activePath ? " active" : ""}${entry.is_directory ? " directory" : ""}`}
          style={{ paddingLeft: 6 + depth * 13 }}
          onClick={() => (entry.is_directory ? toggle(entry) : onOpen(entry))}
          title={entry.path}
          aria-expanded={entry.is_directory ? open : undefined}
        >
          <span className={`tree-chevron${open ? " open" : ""}`}>
            {entry.is_directory && entry.has_children ? <IconChevronRight size={11} /> : null}
          </span>
          <span className="tree-glyph">
            {entry.is_directory ? (
              open ? (
                <IconFolderOpen size={14} />
              ) : (
                <IconFolder size={14} />
              )
            ) : (
              <FileGlyph name={entry.name} size={14} />
            )}
          </span>
          <span className="tree-name">{entry.name}</span>
        </button>
      );
      return entry.is_directory && open ? [row, ...renderLevel(entry.path, depth + 1)] : [row];
    });
  };

  return (
    <div className="file-tree" role="tree" aria-label="Project files">
      <div className="tree-head">
        <span>Explorer</span>
        <button onClick={() => void load("")} aria-label="Refresh file tree" title="Refresh">
          <IconRefresh size={12} />
        </button>
      </div>
      {error ? (
        <div className="tree-error" role="alert">
          {error}
        </div>
      ) : null}
      <div className="tree-scroll">{renderLevel("", 0)}</div>
    </div>
  );
}
