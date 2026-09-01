import type { FileDiff, DiffLine } from "../lib/ipc";

function DiffLineRow({ line, index }: { line: DiffLine; index: number }) {
  const cls =
    line.line_type === "added"
      ? "diff-line diff-add"
      : line.line_type === "deleted"
        ? "diff-line diff-del"
        : "diff-line diff-ctx";
  return (
    <div className={cls} aria-label={`${line.line_type} line ${index + 1}`}>
      <span className="diff-ctx-num">
        {line.old_line_num ?? ""}
      </span>
      <span className="diff-new-num">
        {line.new_line_num ?? ""}
      </span>
      <span className="diff-sign">
        {line.line_type === "added"
          ? "+"
          : line.line_type === "deleted"
            ? "−"
            : " "}
      </span>
      <span className="diff-content">{line.content || " "}</span>
    </div>
  );
}

export function DiffHunkView({ hunks }: { hunks: { lines: DiffLine[] }[] }) {
  return (
    <div className="diff-hunks">
      {hunks.map((hunk, i) => (
        <div key={i} className="diff-hunk">
          {hunk.lines.map((line, j) => (
            <DiffLineRow key={`${i}-${j}`} line={line} index={j} />
          ))}
        </div>
      ))}
    </div>
  );
}

export function FileDiffView({ diff }: { diff: FileDiff }) {
  return (
    <div className="file-diff">
      <div className="file-diff-header">
        <span className="file-diff-path">{diff.path}</span>
        <span className="file-diff-stats">
          <span className="diff-add-count">+{diff.additions}</span>
          {" "}
          <span className="diff-del-count">−{diff.deletions}</span>
        </span>
      </div>
      <DiffHunkView hunks={diff.hunks} />
    </div>
  );
}

export function DiffView({ diffs }: { diffs: FileDiff[] }) {
  if (diffs.length === 0) return null;
  return (
    <div className="diff-view">
      {diffs.map((diff, i) => (
        <FileDiffView key={`${diff.path}-${i}`} diff={diff} />
      ))}
    </div>
  );
}
