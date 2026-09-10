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
import type { TurnChanges, TurnFileChange, TurnNotice } from "../lib/ipc";
import { FileGlyph, IconChevronDown } from "./icons";
import { CHANGES_PREVIEW, grouped } from "./turnGrouping";

export { formatDuration, groupHeadline, groupTools } from "./turnGrouping";
export type { ActivityGroupView } from "./turnGrouping";

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
