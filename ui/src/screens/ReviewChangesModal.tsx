import { useEffect, useState } from "react";
import type { FileDiff, ReviewSummary } from "../lib/ipc";
import { api } from "../lib/api";
import {
  FileGlyph,
  IconChevronDown,
  IconChevronRight,
  IconClose,
  IconGitMerge,
  IconRefresh,
  IconSplitView,
} from "../components/icons";

export function ReviewChangesModal({
  open,
  turnTitle,
  workspacePath,
  onClose,
}: {
  open: boolean;
  turnTitle?: string | null;
  workspacePath?: string | null;
  onClose: () => void;
}) {
  const [summary, setSummary] = useState<ReviewSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTurnFilter, setActiveTurnFilter] = useState<string | null>(turnTitle ?? null);
  const [expandedFiles, setExpandedFiles] = useState<Set<string>>(new Set());
  const [viewMode, setViewMode] = useState<"unified" | "split">("unified");

  useEffect(() => {
    setActiveTurnFilter(turnTitle ?? null);
  }, [turnTitle]);

  const loadReview = async (filter: string | null) => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.reviewChanges(workspacePath, filter);
      setSummary(data);
      // Auto-expand first 2 files if there are changes
      if (data.files.length > 0) {
        const initial = new Set<string>();
        data.files.slice(0, 2).forEach((f) => initial.add(f.path));
        setExpandedFiles(initial);
      }
    } catch (err) {
      setError(String((err as { message?: string }).message ?? err));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (open) {
      void loadReview(activeTurnFilter);
    }
  }, [open, activeTurnFilter, workspacePath]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  const toggleFile = (path: string) => {
    setExpandedFiles((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const expandAll = () => {
    if (!summary) return;
    if (expandedFiles.size === summary.files.length) {
      setExpandedFiles(new Set());
    } else {
      setExpandedFiles(new Set(summary.files.map((f) => f.path)));
    }
  };

  return (
    <div className="review-modal-backdrop" onClick={onClose} role="dialog" aria-modal="true">
      <div className="review-modal-sheet" onClick={(e) => e.stopPropagation()}>
        {/* Header */}
        <header className="review-modal-header">
          <div className="review-header-left">
            <div className="review-title-row">
              <IconGitMerge size={16} className="review-git-icon" />
              <h2>Review Changes</h2>
              {summary && summary.files.length > 0 ? (
                <span className="review-total-badge">
                  <span className="diff-add">+{summary.total_additions}</span>
                  <span className="diff-del">-{summary.total_deletions}</span>
                  <span className="diff-file-count">{summary.files.length} files</span>
                </span>
              ) : null}
            </div>

            {activeTurnFilter ? (
              <div className="review-turn-filter">
                <span className="filter-label">
                  For turn
                  <button
                    type="button"
                    className="filter-clear-btn"
                    onClick={() => setActiveTurnFilter(null)}
                    title="Show all workspace changes"
                  >
                    ×
                  </button>
                </span>
                <span className="filter-query" title={activeTurnFilter}>
                  {activeTurnFilter.length > 48
                    ? `${activeTurnFilter.slice(0, 48)}…`
                    : activeTurnFilter}
                </span>
              </div>
            ) : null}
          </div>

          <div className="review-header-actions">
            <button
              type="button"
              className="review-icon-btn"
              onClick={() => void loadReview(activeTurnFilter)}
              title="Refresh changes"
              aria-label="Refresh changes"
            >
              <IconRefresh size={14} className={loading ? "spin" : ""} />
            </button>
            <button
              type="button"
              className={`review-icon-btn${viewMode === "split" ? " active" : ""}`}
              onClick={() => setViewMode((m) => (m === "unified" ? "split" : "unified"))}
              title={viewMode === "unified" ? "Switch to side-by-side view" : "Switch to unified view"}
              aria-label="Toggle split view"
            >
              <IconSplitView size={15} />
            </button>
            {summary && summary.files.length > 0 ? (
              <button
                type="button"
                className="review-text-btn"
                onClick={expandAll}
                title="Expand/Collapse all files"
              >
                {expandedFiles.size === summary.files.length ? "Collapse all" : "Expand all"}
              </button>
            ) : null}
            <button
              type="button"
              className="review-close-btn"
              onClick={onClose}
              title="Close review (Esc)"
              aria-label="Close"
            >
              <IconClose size={15} />
            </button>
          </div>
        </header>

        {/* Content Body */}
        <div className="review-modal-body">
          {error ? (
            <div className="review-error-banner" role="alert">
              {error}
            </div>
          ) : null}

          {loading && !summary ? (
            <div className="review-loading-state">
              <span className="spinner-dot" />
              <span>Scanning project changes and git diffs…</span>
            </div>
          ) : !summary || summary.files.length === 0 ? (
            <div className="review-empty-state">
              <span style={{ opacity: 0.35, marginBottom: "8px", display: "inline-block" }}>
                <IconGitMerge size={32} />
              </span>
              <h3>No workspace changes</h3>
              <p>The active workspace is clean — all files match the repository state.</p>
            </div>
          ) : (
            <div className="review-files-list">
              {summary.files.map((file) => {
                const isExpanded = expandedFiles.has(file.path);
                return (
                  <div key={file.path} className={`review-file-card${isExpanded ? " expanded" : ""}`}>
                    {/* File Header */}
                    <div
                      className="review-file-head"
                      onClick={() => toggleFile(file.path)}
                      role="button"
                      tabIndex={0}
                      aria-expanded={isExpanded}
                    >
                      <div className="file-info-left">
                        <FileGlyph name={file.filename} size={15} />
                        <span className="file-name">{file.filename}</span>
                        <span className="file-dir">{file.directory}</span>
                      </div>

                      <div className="file-info-right">
                        <span className="file-diff-stats">
                          <span className="diff-add">+{file.additions}</span>
                          <span className="diff-del">-{file.deletions}</span>
                        </span>
                        <span className="expand-chevron">
                          {isExpanded ? <IconChevronDown size={14} /> : <IconChevronRight size={14} />}
                        </span>
                      </div>
                    </div>

                    {/* Diff Body */}
                    {isExpanded ? (
                      <div className="review-diff-container">
                        <FileDiffView file={file} viewMode={viewMode} />
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function FileDiffView({ file, viewMode }: { file: FileDiff; viewMode: "unified" | "split" }) {
  if (file.hunks.length === 0) {
    return <div className="diff-empty-msg">Binary or empty file changes.</div>;
  }

  return (
    <div className={`diff-table-wrap ${viewMode}`}>
      {file.hunks.map((hunk, hunkIdx) => (
        <div key={hunkIdx} className="diff-hunk-block">
          <div className="diff-hunk-header">{hunk.header}</div>
          <table className="diff-table">
            <tbody>
              {hunk.lines.map((line, lineIdx) => {
                const lineClass =
                  line.line_type === "added"
                    ? "diff-line-added"
                    : line.line_type === "deleted"
                      ? "diff-line-deleted"
                      : "diff-line-context";

                const prefix =
                  line.line_type === "added" ? "+" : line.line_type === "deleted" ? "-" : " ";

                return (
                  <tr key={lineIdx} className={`diff-row ${lineClass}`}>
                    <td className="diff-num old-num">
                      {line.old_line_num !== null ? line.old_line_num : ""}
                    </td>
                    <td className="diff-num new-num">
                      {line.new_line_num !== null ? line.new_line_num : ""}
                    </td>
                    <td className="diff-prefix">{prefix}</td>
                    <td className="diff-code">
                      <code>{line.content || " "}</code>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      ))}
    </div>
  );
}
