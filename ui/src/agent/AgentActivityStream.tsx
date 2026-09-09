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
import { BhippiMark } from "../components/BhippiMark";
import { ActivityIcon } from "./ActivityIcon";
import {
  aggregateDetail,
  buildActivityStream,
  disclosureOf,
  formatElapsed,
  isFailure,
  isLive,
  kindOf,
  markMotionOf,
  markToneOf,
  outputTail,
  resultLine,
  statusOf,
  summaryLine,
  suspendLive,
  titleFor,
  type ActivityNode,
  type AggregateEntry,
  type Disclosure,
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
    // The one animated thing on the surface, and only the mark moves — not the row.
    // It is the app's own B: what colour it takes says what sort of work this is, and a
    // sheen instead of a turn says the work is *looking* for something.
    return (
      <BhippiMark
        motion={markMotionOf(activity)}
        tone={markToneOf(activity)}
        title="Working"
      />
    );
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

/**
 * Everything a row keeps behind its chevron.
 *
 * There is one of these and it renders whatever [`disclosureOf`] found, in a fixed order:
 * what the step said, where it pointed, what it ran, what that printed. A row therefore
 * cannot open onto a blank panel — if there were nothing to show, `disclosureOf` would have
 * returned `null` and the chevron would not be there to click.
 */
function DisclosureBody({ disclosure, failed }: { disclosure: Disclosure; failed: boolean }) {
  return (
    <div className="agent-detail-list">
      {disclosure.note ? (
        <p className={`agent-detail-note${failed ? " failed" : ""}`}>{disclosure.note}</p>
      ) : null}
      {disclosure.url ? <div className="agent-detail-url">{disclosure.url}</div> : null}
      {disclosure.files.map((file) => (
        <div key={file.path} className="agent-detail-file">
          <span className="agent-detail-path">{file.path}</span>
          <Stat stat={{ additions: file.additions, deletions: file.deletions }} />
        </div>
      ))}
      {disclosure.command ? (
        <pre className="agent-detail-command" tabIndex={0} aria-label="The command that ran">
          {disclosure.command}
        </pre>
      ) : null}
      {disclosure.output ? (
        <pre className="agent-output" tabIndex={0} aria-label="Command output">
          {disclosure.output}
          {disclosure.outputTruncated ? (
            <span className="agent-output-note">
              {"\n"}Output was truncated where it was captured.
            </span>
          ) : null}
        </pre>
      ) : null}
    </div>
  );
}

/** A shell step: the command, its tail while it runs, all of it when opened (§12). */
const CommandRow = memo(function CommandRow({ activity }: { activity: ToolActivity }) {
  const live = isLive(statusOf(activity));
  const tail = live ? outputTail(activity.output, 4) : [];
  const result = resultLine(activity);
  const disclosure = disclosureOf(activity);
  const failed = isFailure(statusOf(activity));
  const subtitle = activity.command ?? activity.description ?? null;

  return (
    <>
      <Row
        activity={activity}
        title={titleFor(activity)}
        secondary={subtitle ? summaryLine(subtitle) : null}
        trailing={result ? <span className="agent-row-result">{result}</span> : null}
        expandLabel="Show what this step did"
        detail={
          disclosure ? <DisclosureBody disclosure={disclosure} failed={failed} /> : undefined
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
  const failed = isFailure(statusOf(activity));
  // The one decision: a chevron exists exactly when there is something behind it.
  const disclosure = disclosureOf(activity);

  return (
    <Row
      activity={activity}
      title={titleFor(activity)}
      secondary={secondary ? summaryLine(secondary) : null}
      trailing={
        <>
          <Stat stat={stat} />
          {result ? <span className="agent-row-result">{result}</span> : null}
        </>
      }
      expandLabel={failed ? "Show the whole message" : "Show what this step did"}
      detail={disclosure ? <DisclosureBody disclosure={disclosure} failed={failed} /> : undefined}
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

/**
 * "Read 5 files ›" — opens onto every step it folded (§8).
 *
 * It takes the steps rather than a list of paths because a folded step does not always
 * name one, and the version that took paths rendered an empty panel whenever none of them
 * did: a chevron, a click, and nothing. `aggregateDetail` accounts for every step in the
 * headline's count, with its path when it has one and its own sentence when it does not.
 */
function AggregateRow({
  title,
  entries,
  stat,
}: {
  title: string;
  entries: AggregateEntry[];
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
          title="Show every step this stands for"
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
              {entries.map((entry) => (
                <div key={entry.id} className="agent-detail-file">
                  <span className={entry.path ? "agent-detail-path" : "agent-detail-step"}>
                    {entry.title}
                  </span>
                  {entry.note ? <span className="agent-detail-note-inline">{entry.note}</span> : null}
                  <Stat stat={entry.stat} />
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
    return (
      <AggregateRow
        title={node.title}
        entries={aggregateDetail(node.activities)}
        stat={node.stat}
      />
    );
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
        <BhippiMark motion="working" tone="other" />
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
          // Thinking is looking for something, so the mark sweeps rather than turns.
          <BhippiMark motion="seeking" tone="explore" />
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
