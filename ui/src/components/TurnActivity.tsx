/**
 * The transcript's activity surface (CHT-110…CHT-118, `docs/14-CHAT-SURFACE-PLAN.md`).
 *
 * A turn's steps are grouped into collapsible rows labelled by what they actually did —
 * "Ran commands", "Edited files, ran commands" — each expanding to the real command and its
 * output, or the real diff. Detail is hidden by default so the prose stays the spine of the
 * turn; the last group of a *running* turn opens itself, because the reason to watch a live
 * turn is to see what it is doing now.
 *
 * Nothing here computes: line counts, durations, truncation and file paths all arrive
 * already decided from `bhippi-app::chat` (INV-051). This file chooses words and draws.
 */

import { useMemo, useState } from "react";
import type { ToolActivity, TurnChanges, TurnFileChange, TurnNotice } from "../lib/ipc";
import { FileGlyph, IconChevronDown, IconChevronRight } from "./icons";
import { parseToolTarget } from "../lib/toolTarget";
import {
  CHANGES_PREVIEW,
  formatDuration,
  grouped,
  labelFor,
  type ActivityGroupView,
} from "./turnGrouping";

export { formatDuration, groupHeadline, groupTools } from "./turnGrouping";
export type { ActivityGroupView } from "./turnGrouping";

function CommandBlock({ tool }: { tool: ToolActivity }) {
  const [copied, setCopied] = useState(false);
  const failed = tool.exit_code !== null && tool.exit_code !== undefined && tool.exit_code !== 0;

  const copy = () => {
    const text = [tool.command ? `$ ${tool.command}` : null, tool.output].filter(Boolean).join("\n");
    void navigator.clipboard?.writeText(text).then(
      () => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1200);
      },
      () => setCopied(false),
    );
  };

  return (
    <div className={`turn-command${failed ? " failed" : ""}`}>
      <div className="turn-command-head">
        <code className="turn-command-line">
          <span className="turn-command-prompt">$</span> {tool.command ?? tool.title}
        </code>
        {failed ? <span className="turn-command-exit">exit {tool.exit_code}</span> : null}
        {tool.elapsed_ms ? (
          <span className="turn-command-elapsed">{formatDuration(tool.elapsed_ms)}</span>
        ) : null}
        <button type="button" className="turn-command-copy" onClick={copy} title="Copy command and output">
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      {tool.output ? (
        <pre className="turn-command-output" tabIndex={0} aria-label="Command output">
          {tool.output}
        </pre>
      ) : null}
      {tool.truncated ? (
        <div className="turn-command-truncated">
          Output was truncated where it was captured — the middle is not held in memory.
        </div>
      ) : null}
    </div>
  );
}

function ChangeRow({ change }: { change: TurnFileChange }) {
  const name = change.path.split("/").pop() ?? change.path;
  const directory = change.path.slice(0, Math.max(0, change.path.length - name.length));
  return (
    <div className="turn-change-row">
      <span className="turn-change-glyph">
        <FileGlyph name={name} size={11} />
      </span>
      <span className="turn-change-path">
        <span className="turn-change-dir">{directory}</span>
        <span className="turn-change-name">{name}</span>
      </span>
      <span className="turn-change-stat">
        {change.additions > 0 ? <span className="stat-add">+{grouped(change.additions)}</span> : null}
        {change.deletions > 0 ? <span className="stat-del">−{grouped(change.deletions)}</span> : null}
      </span>
    </div>
  );
}

function ToolDetail({ tool }: { tool: ToolActivity }) {
  if (tool.command || tool.output) return <CommandBlock tool={tool} />;
  const changes = tool.changes ?? [];
  if (changes.length > 0) {
    return (
      <div className="turn-change-list">
        {changes.map((change) => (
          <ChangeRow key={change.path} change={change} />
        ))}
      </div>
    );
  }
  // A step with no record still deserves its line — but as a line, not as an empty
  // disclosure that expands to nothing (plan §3, rule 2).
  const { verb, fileName, lineRange } = parseToolTarget(tool.title, tool.detail);
  return (
    <div className={`turn-work-item ${tool.state}`}>
      <span className="turn-work-verb">{verb}</span>
      <span className="turn-work-target">
        <span className="turn-work-badge">
          <FileGlyph name={fileName} size={11} />
        </span>
        <span className="turn-work-filename">{fileName}</span>
        {lineRange ? <span className="turn-work-lines">{lineRange}</span> : null}
      </span>
      {tool.state === "running" ? <span className="turn-work-running-dot" /> : null}
    </div>
  );
}

export function ActivityGroup({
  group,
  defaultOpen,
}: {
  group: ActivityGroupView;
  defaultOpen: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const running = group.tools.some((tool) => tool.state === "running");
  const failed = group.tools.some((tool) => tool.state === "failed");
  const count = group.tools.length;

  return (
    <div className={`activity-group${open ? " open" : ""}${failed ? " failed" : ""}`}>
      <button
        type="button"
        className="activity-group-header"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        <span className="activity-group-chev" aria-hidden="true">
          {open ? <IconChevronDown size={11} /> : <IconChevronRight size={11} />}
        </span>
        <span className="activity-group-label">{labelFor(group)}</span>
        {count > 1 ? <span className="activity-group-count">{count}</span> : null}
        {running ? <span className="turn-work-running-dot" /> : null}
      </button>
      {open ? (
        <div className="activity-group-body" role="region" aria-label={labelFor(group)}>
          {group.tools.map((tool) => (
            <ToolDetail key={tool.id} tool={tool} />
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function TurnNotices({ notices }: { notices: TurnNotice[] }) {
  if (notices.length === 0) return null;
  return (
    <div className="turn-notices">
      {notices.map((notice, index) => (
        <div key={`${notice.level}-${index}`} className={`turn-notice ${notice.level}`} role="status">
          <span className="turn-notice-message">{notice.message}</span>
          {notice.hint ? <span className="turn-notice-hint">{notice.hint}</span> : null}
        </div>
      ))}
    </div>
  );
}

export function TurnChangesCard({
  changes,
  onReview,
  onUndo,
  undoDisabledReason,
  undoing,
}: {
  changes: TurnChanges;
  onReview: () => void;
  onUndo?: () => void;
  /** When set, Undo is disabled and this says why — a gate that blocks, not one that warns. */
  undoDisabledReason?: string | null;
  undoing?: boolean;
}) {
  const [expanded, setExpanded] = useState(false);
  const shown = useMemo(
    () => (expanded ? changes.files : changes.files.slice(0, CHANGES_PREVIEW)),
    [changes.files, expanded],
  );
  const hidden = changes.files.length - shown.length;

  return (
    <section className="turn-changes" aria-label="Files this turn changed">
      <header className="turn-changes-head">
        <span className="turn-changes-title">
          Edited {changes.files.length} file{changes.files.length === 1 ? "" : "s"}
        </span>
        <span className="turn-changes-totals">
          <span className="stat-add">+{grouped(changes.total_additions)}</span>
          <span className="stat-del">−{grouped(changes.total_deletions)}</span>
        </span>
        <span className="turn-changes-actions">
          {onUndo ? (
            <button
              type="button"
              className="turn-changes-btn"
              onClick={onUndo}
              disabled={Boolean(undoDisabledReason) || undoing}
              title={undoDisabledReason ?? "Put every file this turn changed back as it was"}
            >
              {undoing ? "Undoing…" : "Undo ↺"}
            </button>
          ) : null}
          <button type="button" className="turn-changes-btn primary" onClick={onReview}>
            Review
          </button>
        </span>
      </header>
      <div className="turn-changes-list">
        {shown.map((change) => (
          <ChangeRow key={change.path} change={change} />
        ))}
      </div>
      {hidden > 0 ? (
        <button
          type="button"
          className="turn-changes-more"
          onClick={() => setExpanded(true)}
          aria-expanded={false}
        >
          <IconChevronDown size={11} /> Show {hidden} more file{hidden === 1 ? "" : "s"}
        </button>
      ) : null}
      {undoDisabledReason ? <div className="turn-changes-note">{undoDisabledReason}</div> : null}
    </section>
  );
}
