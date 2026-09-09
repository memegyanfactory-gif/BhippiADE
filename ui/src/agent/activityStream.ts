/**
 * The activity stream's logic (ADR-0049).
 *
 * The runtime pushes one event per thing it does, keyed by a stable id. This module turns
 * that flat list into what the transcript draws: a live row at the bottom, rapid reads
 * folded into one "Read 6 files" row, and older work compressed into groups so a long turn
 * does not become an endless wall.
 *
 * It is a plain module because every rule in here is a claim about what happened — "six
 * files", "24 tests passed", "this row is still running" — and a row that lies about its
 * contents is worse than no row. Nothing here computes a number: line counts, test totals
 * and durations all arrive already decided from `bhippi-app` (INV-051). What it does is
 * decide *shape*: what folds into what, and what stays visible.
 */

import type { ActivityKind, ActivityStatus, AgentPhase, ToolActivity } from "../lib/ipc";

/** What the engine last said a running turn is doing, as the live phase row carries it. */
export interface LiveStepView {
  phase: AgentPhase | null;
  /** The engine's own sentence, which names the target as well as the verb. */
  label: string | null;
  /** Epoch ms this phase started, for the elapsed counter. */
  since: number | null;
}

/** How many consecutive same-kind steps it takes before folding them is worth it. */
export const AGGREGATE_THRESHOLD = 3;
/** Past this many rows, older work is compressed into groups (owner spec §24). */
export const HISTORY_LIMIT = 8;
/** How many recent rows stay loose even when the history is compressed. */
export const KEEP_RECENT = 4;

/**
 * The kind and status of a step, including one recorded before ADR-0049.
 *
 * Both fields are optional on the wire because a conversation persisted by an older build
 * has neither. Rather than render those turns as blank rows, they fall back to the legacy
 * `state` — which carries the same four outcomes the new status has names for.
 */
export function kindOf(activity: ToolActivity): ActivityKind {
  return activity.kind ?? "using_tool";
}

export function statusOf(activity: ToolActivity): ActivityStatus {
  if (activity.status) return activity.status;
  switch (activity.state) {
    case "running":
      return "in_progress";
    case "failed":
      return "failed";
    case "skipped":
      return "cancelled";
    default:
      return "completed";
  }
}

/** A row that is still going, or is suspended waiting for the user, is never hidden. */
export function isLive(status: ActivityStatus): boolean {
  return status === "in_progress" || status === "queued" || status === "waiting_for_user";
}

export function isFailure(status: ActivityStatus): boolean {
  return status === "failed";
}

/**
 * Marks whatever is still running as waiting on the user (owner spec §17).
 *
 * A turn blocked on a permission prompt has not stopped and has not failed. Without this
 * the running row would sit there spinning behind a dialog, claiming work is happening
 * when nothing is — so the row *suspends*, keeping its place in the stream, and resumes
 * when the answer comes back.
 */
export function suspendLive(activities: readonly ToolActivity[]): ToolActivity[] {
  return activities.map((activity) =>
    isLive(statusOf(activity)) ? { ...activity, status: "waiting_for_user" as const } : activity,
  );
}

/** The coarse phase a kind belongs to, used only to title a compressed group. */
export type ActivityPhase = "explore" | "implement" | "verify" | "interact" | "other";

const PHASE_OF: Partial<Record<ActivityKind, ActivityPhase>> = {
  reasoning: "explore",
  planning: "explore",
  searching_code: "explore",
  searching_files: "explore",
  listing_directory: "explore",
  reading_file: "explore",
  reading_multiple_files: "explore",
  searching_web: "explore",
  opening_webpage: "explore",
  reading_webpage: "explore",
  git_status: "explore",
  git_diff: "explore",

  editing_file: "implement",
  creating_file: "implement",
  deleting_file: "implement",
  moving_file: "implement",
  applying_patch: "implement",
  git_commit: "implement",
  installing_dependencies: "implement",

  running_tests: "verify",
  running_single_test: "verify",
  linting: "verify",
  typechecking: "verify",
  building_project: "verify",
  checking_errors: "verify",
  verifying: "verify",
  reviewing_changes: "verify",
  reviewing_diff: "verify",

  opening_browser: "interact",
  testing_browser: "interact",
  clicking_ui: "interact",
  taking_screenshot: "interact",
  inspecting_ui: "interact",
  inspecting_screenshot: "interact",
  viewing_image: "interact",
};

export function phaseOf(kind: ActivityKind): ActivityPhase {
  return PHASE_OF[kind] ?? "other";
}

/**
 * The present-tense verb a finished row should use instead (owner spec §6).
 *
 * Keyed on the leading word only, so "Reading PlayerController.ts" becomes "Read
 * PlayerController.ts" and the target it names is untouched. A verb that is not in the map
 * is left exactly as the runtime wrote it — a wrong tense is cosmetic, an invented sentence
 * is not.
 */
const PAST_TENSE: Record<string, string> = {
  Reading: "Read",
  Editing: "Edited",
  Creating: "Created",
  Deleting: "Deleted",
  Renaming: "Renamed",
  Running: "Ran",
  Searching: "Searched",
  Building: "Built",
  Checking: "Checked",
  Listing: "Listed",
  Installing: "Installed",
  Starting: "Started",
  Committing: "Committed",
  Reviewing: "Reviewed",
  Planning: "Planned",
  Verifying: "Verified",
  Taking: "Took",
  Inspecting: "Inspected",
  Interacting: "Interacted with",
  Applying: "Applied",
  Formatting: "Formatted",
  Linting: "Linted",
  Using: "Used",
  Working: "Worked",
  Looking: "Looked at",
};

export function pastTense(title: string): string {
  const gap = title.indexOf(" ");
  const head = gap === -1 ? title : title.slice(0, gap);
  const rest = gap === -1 ? "" : title.slice(gap);
  const past = PAST_TENSE[head];
  return past ? `${past}${rest}` : title;
}

/** The words a row shows, given where the step has got to. */
export function titleFor(activity: ToolActivity): string {
  return isLive(statusOf(activity)) ? activity.title : pastTense(activity.title);
}

/** Summed line counts, for a row that stands for several file changes. */
export type LineStat = { additions: number; deletions: number };

export function lineStatOf(activities: readonly ToolActivity[]): LineStat | null {
  let additions = 0;
  let deletions = 0;
  let seen = false;
  for (const activity of activities) {
    for (const change of activity.changes ?? []) {
      additions += change.additions;
      deletions += change.deletions;
      seen = true;
    }
  }
  return seen ? { additions, deletions } : null;
}

/** Every distinct file a set of steps names, in the order they were first touched. */
export function pathsOf(activities: readonly ToolActivity[]): string[] {
  const seen = new Set<string>();
  const paths: string[] = [];
  const add = (path: string) => {
    if (path && !seen.has(path)) {
      seen.add(path);
      paths.push(path);
    }
  };
  for (const activity of activities) {
    for (const path of activity.metadata?.paths ?? []) add(path);
    for (const change of activity.changes ?? []) add(change.path);
  }
  return paths;
}

/** Which kinds fold together, and what the folded row is called. */
const AGGREGATES: Array<{ kinds: ActivityKind[]; noun: (count: number) => string }> = [
  { kinds: ["reading_file"], noun: (n) => `Read ${n} files` },
  { kinds: ["editing_file", "applying_patch"], noun: (n) => `Updated ${n} files` },
  { kinds: ["creating_file"], noun: (n) => `Created ${n} files` },
  { kinds: ["searching_code", "searching_files"], noun: (n) => `Ran ${n} searches` },
];

function aggregateFor(kind: ActivityKind) {
  return AGGREGATES.find((entry) => entry.kinds.includes(kind));
}

export type ActivityNode =
  | { node: "single"; id: string; activity: ToolActivity }
  | {
      node: "aggregate";
      id: string;
      title: string;
      activities: ToolActivity[];
      paths: string[];
      stat: LineStat | null;
    }
  | {
      node: "group";
      id: string;
      title: string;
      summary: string;
      children: ActivityNode[];
    };

/** Whether anything inside a node is still running — a live node is never compressed. */
export function nodeIsLive(node: ActivityNode): boolean {
  if (node.node === "single") return isLive(statusOf(node.activity));
  if (node.node === "aggregate") return node.activities.some((one) => isLive(statusOf(one)));
  return node.children.some(nodeIsLive);
}

export function nodeHasFailure(node: ActivityNode): boolean {
  if (node.node === "single") return isFailure(statusOf(node.activity));
  if (node.node === "aggregate") return node.activities.some((one) => isFailure(statusOf(one)));
  return node.children.some(nodeHasFailure);
}

/**
 * Folds runs of the same kind of finished work into one row (owner spec §8).
 *
 * Only *finished* steps fold. A running read stays its own row because the whole reason to
 * watch a live turn is to see the file it is on right now — and only *consecutive* ones,
 * because read/edit/read/edit is four things in that order and folding it into two would
 * misreport the sequence the reader is following.
 */
export function aggregate(activities: readonly ToolActivity[]): ActivityNode[] {
  const nodes: ActivityNode[] = [];
  let index = 0;
  while (index < activities.length) {
    const current = activities[index];
    const entry = aggregateFor(kindOf(current));
    if (!entry || isLive(statusOf(current))) {
      nodes.push({ node: "single", id: current.id, activity: current });
      index += 1;
      continue;
    }
    let end = index;
    while (
      end < activities.length &&
      entry.kinds.includes(kindOf(activities[end])) &&
      !isLive(statusOf(activities[end]))
    ) {
      end += 1;
    }
    const run = activities.slice(index, end);
    if (run.length >= AGGREGATE_THRESHOLD) {
      nodes.push({
        node: "aggregate",
        id: run[0].id,
        title: entry.noun(run.length),
        activities: run,
        paths: pathsOf(run),
        stat: lineStatOf(run),
      });
    } else {
      for (const one of run) nodes.push({ node: "single", id: one.id, activity: one });
    }
    index = end;
  }
  return nodes;
}

const PHASE_TITLE: Record<ActivityPhase, string> = {
  explore: "Explored the project",
  implement: "Made changes",
  verify: "Checked the work",
  interact: "Used the app",
  other: "Worked",
};

/** "12 files read · 3 searches" — what a compressed group actually contains. */
export function summarize(children: readonly ActivityNode[]): string {
  const flat: ToolActivity[] = [];
  const walk = (nodes: readonly ActivityNode[]) => {
    for (const node of nodes) {
      if (node.node === "single") flat.push(node.activity);
      else if (node.node === "aggregate") flat.push(...node.activities);
      else walk(node.children);
    }
  };
  walk(children);

  const count = (kinds: ActivityKind[]) =>
    flat.filter((one) => kinds.includes(kindOf(one))).length;
  const parts: string[] = [];
  const reads = count(["reading_file", "reading_multiple_files"]);
  if (reads > 0) parts.push(`${reads} ${reads === 1 ? "file" : "files"} read`);
  const searches = count(["searching_code", "searching_files", "searching_web"]);
  if (searches > 0) parts.push(`${searches} ${searches === 1 ? "search" : "searches"}`);

  const edited = pathsOf(
    flat.filter((one) =>
      ["editing_file", "creating_file", "applying_patch", "deleting_file"].includes(kindOf(one)),
    ),
  ).length;
  if (edited > 0) parts.push(`${edited} ${edited === 1 ? "file" : "files"} changed`);

  const commands = count([
    "running_command",
    "running_script",
    "running_tests",
    "running_single_test",
    "building_project",
    "linting",
    "typechecking",
  ]);
  if (commands > 0) parts.push(`${commands} ${commands === 1 ? "command" : "commands"}`);

  const stat = lineStatOf(flat);
  if (stat && (stat.additions > 0 || stat.deletions > 0)) {
    parts.push(`+${stat.additions} −${stat.deletions}`);
  }
  return parts.join(" · ");
}

/**
 * Compresses older finished rows into phase groups (owner spec §24).
 *
 * Two rules it must not break. The current activity always stays visible — the whole point
 * of the surface is to answer "what is it doing now". And a group never claims to hold work
 * it does not: its summary is counted from its own children.
 */
export function collapseHistory(
  nodes: readonly ActivityNode[],
  limit: number = HISTORY_LIMIT,
  keepRecent: number = KEEP_RECENT,
): ActivityNode[] {
  if (nodes.length <= limit) return [...nodes];

  // Everything from the first live row onward stays loose, along with the recent tail.
  const firstLive = nodes.findIndex(nodeIsLive);
  const tailStart = Math.max(
    0,
    firstLive === -1 ? nodes.length - keepRecent : Math.min(firstLive, nodes.length - keepRecent),
  );
  const head = nodes.slice(0, tailStart);
  const tail = nodes.slice(tailStart);
  if (head.length === 0) return [...nodes];

  const groups: ActivityNode[] = [];
  let run: ActivityNode[] = [];
  let runPhase: ActivityPhase | null = null;

  const flush = () => {
    if (run.length === 0 || runPhase === null) return;
    if (run.length === 1) {
      groups.push(run[0]);
    } else {
      groups.push({
        node: "group",
        id: `group-${run[0].id}`,
        title: PHASE_TITLE[runPhase],
        summary: summarize(run),
        children: run,
      });
    }
    run = [];
    runPhase = null;
  };

  for (const node of head) {
    const kind =
      node.node === "single"
        ? kindOf(node.activity)
        : node.node === "aggregate"
          ? node.activities[0] && kindOf(node.activities[0])
          : undefined;
    const phase = kind ? phaseOf(kind) : "other";
    if (runPhase !== null && phase !== runPhase) flush();
    runPhase = phase;
    run.push(node);
  }
  flush();

  return [...groups, ...tail];
}

/**
 * The whole pipeline: the runtime's flat event list as the transcript draws it.
 *
 * `compress` is off for a short turn and for one the user has expanded, so the same
 * function serves both the live view and the read-back of a finished turn.
 */
export function buildActivityStream(
  activities: readonly ToolActivity[],
  options?: { compress?: boolean },
): ActivityNode[] {
  const nodes = aggregate(activities);
  return options?.compress === false ? nodes : collapseHistory(nodes);
}

/** `4.2s`, `1m 20s`. Whole seconds — a transcript is not a stopwatch. */
export function formatElapsed(ms: number): string {
  if (ms < 1000) return "0.1s";
  const seconds = ms / 1000;
  if (seconds < 10) return `${seconds.toFixed(1)}s`;
  const whole = Math.round(seconds);
  if (whole < 60) return `${whole}s`;
  const minutes = Math.floor(whole / 60);
  const rest = whole % 60;
  return rest === 0 ? `${minutes}m` : `${minutes}m ${rest}s`;
}

/** The last few lines of a running command's output (owner spec §12). */
export function outputTail(output: string | null | undefined, lines = 4): string[] {
  if (!output) return [];
  const all = output.split("\n").filter((line) => line.trim().length > 0);
  return all.slice(Math.max(0, all.length - lines));
}

/**
 * The one-line result a finished command reports — "24 tests passed", "exit 1".
 *
 * Returns `null` when the runtime reported nothing worth a line, rather than inventing a
 * reassuring one.
 */
export function resultLine(activity: ToolActivity): string | null {
  const passed = activity.metadata?.tests_passed ?? null;
  const failed = activity.metadata?.tests_failed ?? null;
  if (passed !== null || failed !== null) {
    const total = (passed ?? 0) + (failed ?? 0);
    if (failed && failed > 0) return `${passed ?? 0} of ${total} tests passed`;
    if (total > 0) return `${total} ${total === 1 ? "test" : "tests"} passed`;
  }
  if (activity.exit_code !== null && activity.exit_code !== undefined && activity.exit_code !== 0) {
    return `exit ${activity.exit_code}`;
  }
  const matches = activity.metadata?.match_count ?? null;
  if (matches !== null) return `${matches} ${matches === 1 ? "result" : "results"}`;
  return null;
}
