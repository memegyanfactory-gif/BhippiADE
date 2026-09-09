/**
 * The agent activity stream (owner spec §4–§26, ADR-0049).
 *
 * A vertical rail inside the assistant turn, one row per thing the runtime actually did.
 * The row that is happening now carries the only motion on the surface; finished rows go
 * quiet and, once there are enough of them, fold into groups so a long turn stays readable.
 *
 * Everything drawn here arrives already decided — the kind, the sentence, the line counts,
 * the test totals, the truncation. `activityStream.ts` decides shape; this file draws it.
 * Neither invents an activity: a turn that did nothing renders nothing.
 */

import { memo, useEffect, useState, type ReactNode } from "react";
import type { ToolActivity } from "../lib/ipc";
import { IconChevronDown, IconChevronRight, IconClose, IconCheck } from "../components/icons";
import { ActivityIcon } from "./ActivityIcon";
import {
  buildActivityStream,
  formatElapsed,
  isFailure,
  isLive,
  kindOf,
  outputTail,
  resultLine,
  statusOf,
  suspendLive,
  titleFor,
  type ActivityNode,
  type LineStat,
} from "./activityStream";
import "../styles/agent-activity.css";

/** The indicator at the head of a row: running, done, failed, or waiting. */
function Indicator({ activity }: { activity: ToolActivity }) {
  const status = statusOf(activity);
  if (status === "waiting_for_user") {
    return (
      <span className="agent-mark waiting" aria-label="Waiting for you">
        <span className="agent-mark-pause" />
      </span>
    );
  }
  if (isLive(status)) {
    // The one animated thing on the surface, and only the ring moves — not the row.
    return <span className="agent-mark live" aria-label="Running" />;
  }
  if (isFailure(status)) {
    return (
      <span className="agent-mark failed" aria-label="Failed">
        !
      </span>
    );
  }
  if (status === "cancelled") {
    return (
      <span className="agent-mark cancelled" aria-label="Cancelled">
        <IconClose size={8} />
      </span>
    );
  }
  return (
    <span className="agent-mark done" aria-label="Done">
      <IconCheck size={9} />
    </span>
  );
}

function Stat({ stat }: { stat: LineStat | null }) {
  if (!stat || (stat.additions === 0 && stat.deletions === 0)) return null;
  return (
    <span className="agent-stat">
      {stat.additions > 0 ? <span className="agent-stat-add">+{stat.additions}</span> : null}
      {stat.deletions > 0 ? <span className="agent-stat-del">−{stat.deletions}</span> : null}
    </span>
  );
}

/** A row's shell: the rail, the mark, the words, and whatever it discloses. */
function Row({
  activity,
  title,
  secondary,
  trailing,
  detail,
  expandLabel,
}: {
  activity: ToolActivity;
  title: string;
  secondary?: string | null;
  trailing?: ReactNode;
  /** Rendered only when the row is open. Absent means the row does not open. */
  detail?: ReactNode;
  expandLabel?: string;
}) {
  const [open, setOpen] = useState(false);
  const live = isLive(statusOf(activity));
  const failed = isFailure(statusOf(activity));
  const elapsed =
    activity.elapsed_ms !== null && activity.elapsed_ms !== undefined && !live
      ? formatElapsed(activity.elapsed_ms)
      : null;

  const head = (
    <>
      <span className="agent-row-glyph" aria-hidden="true">
        <ActivityIcon kind={kindOf(activity)} />
      </span>
      <span className="agent-row-title">{title}</span>
      {trailing}
      {elapsed ? <span className="agent-row-time">{elapsed}</span> : null}
      {detail ? (
        <span className="agent-row-chev" aria-hidden="true">
          {open ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
        </span>
      ) : null}
    </>
  );

  return (
    <div className={`agent-row${live ? " live" : ""}${failed ? " failed" : ""}`}>
      <span className="agent-rail" aria-hidden="true">
        <Indicator activity={activity} />
      </span>
      <div className="agent-row-body">
        {detail ? (
          <button
            type="button"
            className="agent-row-head"
            onClick={() => setOpen((value) => !value)}
            aria-expanded={open}
            title={expandLabel}
          >
            {head}
          </button>
        ) : (
          <div className="agent-row-head static">{head}</div>
        )}
        {secondary ? <div className="agent-row-sub">{secondary}</div> : null}
        {open && detail ? <div className="agent-row-detail">{detail}</div> : null}
      </div>
    </div>
  );
}

/** A shell step: the command, its tail while it runs, all of it when opened (§12). */
const CommandRow = memo(function CommandRow({ activity }: { activity: ToolActivity }) {
  const live = isLive(statusOf(activity));
  const tail = live ? outputTail(activity.output, 4) : [];
  const result = resultLine(activity);

  return (
    <>
      <Row
        activity={activity}
        title={titleFor(activity)}
        secondary={activity.command ?? activity.description ?? null}
        trailing={result ? <span className="agent-row-result">{result}</span> : null}
        expandLabel="Show terminal output"
        detail={
          activity.output ? (
            <pre className="agent-output" tabIndex={0} aria-label="Command output">
              {activity.output}
              {activity.truncated ? (
                <span className="agent-output-note">
                  {"\n"}Output was truncated where it was captured.
                </span>
              ) : null}
            </pre>
          ) : undefined
        }
      />
      {tail.length > 0 ? (
        <div className="agent-row-tail" aria-hidden="true">
          {tail.map((line, index) => (
            <div key={`${index}-${line}`} className="agent-tail-line">
              {line}
            </div>
          ))}
        </div>
      ) : null}
    </>
  );
});

/** A file row, a search row, a web row — anything whose detail is a short list. */
const SimpleRow = memo(function SimpleRow({ activity }: { activity: ToolActivity }) {
  const meta = activity.metadata;
  const paths = meta?.paths ?? [];
  const stat =
    (activity.changes ?? []).length > 0
      ? (activity.changes ?? []).reduce(
          (total, change) => ({
            additions: total.additions + change.additions,
            deletions: total.deletions + change.deletions,
          }),
          { additions: 0, deletions: 0 },
        )
      : null;

  // The quiet second line: the directory for a file, the query for a search, the host for
  // a page. Never the raw URL — that is behind the disclosure.
  const secondary =
    meta?.query ??
    (paths[0] && paths[0].includes("/")
      ? paths[0].slice(0, paths[0].lastIndexOf("/"))
      : null) ??
    (kindOf(activity) === "reading_webpage" ? null : activity.description ?? null);

  const result = resultLine(activity);
  const changed = activity.changes ?? [];

  return (
    <Row
      activity={activity}
      title={titleFor(activity)}
      secondary={secondary}
      trailing={
        <>
          <Stat stat={stat} />
          {result ? <span className="agent-row-result">{result}</span> : null}
        </>
      }
      expandLabel={meta?.url ? "Show the address" : "Show what changed"}
      detail={
        meta?.url || changed.length > 0 ? (
          <div className="agent-detail-list">
            {meta?.url ? (
              <div className="agent-detail-url">{meta.url}</div>
            ) : null}
            {changed.map((change) => (
              <div key={change.path} className="agent-detail-file">
                <span className="agent-detail-path">{change.path}</span>
                <Stat stat={{ additions: change.additions, deletions: change.deletions }} />
              </div>
            ))}
          </div>
        ) : undefined
      }
    />
  );
});

const COMMAND_KINDS = new Set([
  "running_command",
  "running_script",
  "running_tests",
  "running_single_test",
  "building_project",
  "linting",
  "typechecking",
  "starting_dev_server",
  "installing_dependencies",
  "git_status",
  "git_diff",
  "git_commit",
]);

function ActivityRow({ activity }: { activity: ToolActivity }) {
  if (COMMAND_KINDS.has(kindOf(activity)) && (activity.command || activity.output)) {
    return <CommandRow activity={activity} />;
  }
  return <SimpleRow activity={activity} />;
}

/** "Read 6 files ›" — opens to every file it stands for (§8). */
function AggregateRow({
  title,
  paths,
  stat,
}: {
  title: string;
  paths: string[];
  stat: LineStat | null;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="agent-row">
      <span className="agent-rail" aria-hidden="true">
        <span className="agent-mark done">
          <IconCheck size={9} />
        </span>
      </span>
      <div className="agent-row-body">
        <button
          type="button"
          className="agent-row-head"
          onClick={() => setOpen((value) => !value)}
          aria-expanded={open}
        >
          <span className="agent-row-glyph" aria-hidden="true">
            <ActivityIcon kind="reading_multiple_files" />
          </span>
          <span className="agent-row-title">{title}</span>
          <Stat stat={stat} />
          <span className="agent-row-chev" aria-hidden="true">
            {open ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
          </span>
        </button>
        {open ? (
          <div className="agent-row-detail">
            <div className="agent-detail-list">
              {paths.map((path) => (
                <div key={path} className="agent-detail-file">
                  <span className="agent-detail-path">{path}</span>
                </div>
              ))}
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}

/** Compressed history: "Explored the project · 12 files read · 3 searches" (§24). */
function GroupRow({ node }: { node: Extract<ActivityNode, { node: "group" }> }) {
  const [open, setOpen] = useState(false);
  return (
    <div className={`agent-row group${open ? " open" : ""}`}>
      <span className="agent-rail" aria-hidden="true">
        <span className="agent-mark done">
          <IconCheck size={9} />
        </span>
      </span>
      <div className="agent-row-body">
        <button
          type="button"
          className="agent-row-head"
          onClick={() => setOpen((value) => !value)}
          aria-expanded={open}
        >
          <span className="agent-row-title">{node.title}</span>
          {node.summary ? <span className="agent-row-summary">{node.summary}</span> : null}
          <span className="agent-row-chev" aria-hidden="true">
            {open ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
          </span>
        </button>
        {open ? (
          <div className="agent-group-children">
            {node.children.map((child) => (
              <StreamNode key={child.id} node={child} />
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function StreamNode({ node }: { node: ActivityNode }) {
  if (node.node === "single") return <ActivityRow activity={node.activity} />;
  if (node.node === "aggregate") {
    return <AggregateRow title={node.title} paths={node.paths} stat={node.stat} />;
  }
  return <GroupRow node={node} />;
}

/**
 * The stream itself.
 *
 * `compress` is off for a finished turn the user has chosen to open in full, so the same
 * component serves the live view and the read-back.
 */
export function AgentActivityStream({
  activities,
  compress = true,
  suspended = false,
  header,
  footer,
}: {
  activities: ToolActivity[];
  compress?: boolean;
  /** The turn is blocked on a permission prompt: running rows suspend rather than spin. */
  suspended?: boolean;
  /** Rendered inside the rail, above the events — the reasoning row. */
  header?: ReactNode;
  /** Rendered inside the rail, below them — the live phase row. */
  footer?: ReactNode;
}) {
  // An empty stream is the correct rendering of a turn that did nothing (ADR-0049 §3).
  if (activities.length === 0 && !header && !footer) return null;
  const nodes = buildActivityStream(suspended ? suspendLive(activities) : activities, {
    compress,
  });
  return (
    <div className="agent-stream" role="list" aria-label="What the agent did">
      {header}
      {nodes.map((node) => (
        <StreamNode key={node.id} node={node} />
      ))}
      {footer}
    </div>
  );
}

/**
 * The bottom row of a running turn, when no single step owns the moment (GAD-172).
 *
 * The engine reports a phase with a label that names its target — "Reading src/main.rs",
 * "Waiting for Claude Code". That sentence is the engine's own; this row prints it and
 * never invents one. The counter beside it is not decoration: a frozen elapsed time is the
 * only thing on screen that separates a turn that is working from a turn that is stuck.
 */
export function LivePhaseRow({ label, since }: { label: string | null; since: number | null }) {
  // Nothing else re-renders this row between two phases, so the clock ticks itself.
  const [, tick] = useState(0);
  useEffect(() => {
    if (since === null) return undefined;
    const timer = window.setInterval(() => tick((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [since]);

  const elapsed = since === null ? null : formatElapsed(Date.now() - since);
  return (
    <div className="agent-row live">
      <span className="agent-rail" aria-hidden="true">
        <span className="agent-mark live" />
      </span>
      <div className="agent-row-body">
        <div className="agent-row-head static">
          <span className="agent-row-title" role="status" aria-live="polite">
            {label?.trim() || "Working"}
          </span>
          {elapsed ? <span className="agent-row-time">{elapsed}</span> : null}
        </div>
      </div>
    </div>
  );
}

/**
 * The reasoning row (owner spec §7).
 *
 * Shows only the summary the agent itself volunteered, collapsed by default. There is no
 * fabricated inner monologue here and no private chain-of-thought: if the runtime streamed
 * nothing, this renders the word and no body.
 */
export function ReasoningRow({
  text,
  streaming,
  elapsedMs,
}: {
  text: string | null;
  streaming: boolean;
  elapsedMs?: number | null;
}) {
  const [open, setOpen] = useState(false);
  const summary = text?.trim() ?? "";
  const elapsed = elapsedMs ? formatElapsed(elapsedMs) : null;

  return (
    <div className={`agent-row${streaming ? " live" : ""}`}>
      <span className="agent-rail" aria-hidden="true">
        {streaming ? (
          <span className="agent-mark live" />
        ) : (
          <span className="agent-mark done">
            <IconCheck size={9} />
          </span>
        )}
      </span>
      <div className="agent-row-body">
        {summary ? (
          <button
            type="button"
            className="agent-row-head"
            onClick={() => setOpen((value) => !value)}
            aria-expanded={open}
          >
            <span className="agent-row-glyph" aria-hidden="true">
              <ActivityIcon kind="reasoning" />
            </span>
            <span className="agent-row-title">{streaming ? "Thinking" : "Thought"}</span>
            {elapsed ? <span className="agent-row-time">{elapsed}</span> : null}
            <span className="agent-row-chev" aria-hidden="true">
              {open ? <IconChevronDown size={10} /> : <IconChevronRight size={10} />}
            </span>
          </button>
        ) : (
          <div className="agent-row-head static">
            <span className="agent-row-glyph" aria-hidden="true">
              <ActivityIcon kind="reasoning" />
            </span>
            <span className="agent-row-title">Thinking</span>
          </div>
        )}
        {open && summary ? <div className="agent-reasoning">{summary}</div> : null}
      </div>
    </div>
  );
}
