/**
 * The transcript's activity *logic* (CHT-110, CHT-117).
 *
 * Grouping decides what each collapsed row claims the agent did, and a row that mislabels
 * its contents is worse than no row — so it lives here, in a plain module, where it can be
 * tested without a DOM. `TurnActivity.tsx` draws what this decides.
 */

import type { ToolActivity } from "../lib/ipc.ts";
import { parseToolTarget } from "../lib/toolTarget.ts";

/** How many files the changes card shows before "Show N more files". */
export const CHANGES_PREVIEW = 3;

type Group = {
  id: string;
  kind: "ran" | "edited" | "explored" | "searched" | "mixed";
  tools: ToolActivity[];
};

/** The verb bucket a step falls into, from the verb the dock already derives. */
const bucketOf = (tool: ToolActivity): Group["kind"] => {
  if (tool.action === "write_file") return "edited";
  if (tool.action === "search_web" || tool.action === "fetch_url") return "searched";
  const { verb } = parseToolTarget(tool.title, tool.detail);
  if (verb === "Ran" || verb === "Tested") return "ran";
  if (verb === "Edited" || verb === "Wrote") return "edited";
  if (verb === "Searched" || verb === "Fetched") return "searched";
  return "explored";
};

/**
 * Fold consecutive steps of the same kind into one row.
 *
 * Consecutive, not global: a turn that reads, edits, reads again and edits again did four
 * things in that order, and collapsing them into two buckets would misreport the sequence
 * the user is trying to follow.
 */
export const groupTools = (tools: ToolActivity[]): Group[] => {
  const groups: Group[] = [];
  for (const tool of tools) {
    const kind = bucketOf(tool);
    const last = groups[groups.length - 1];
    if (last && (last.kind === kind || (last.kind === "mixed" && kind !== "explored"))) {
      last.tools.push(tool);
      continue;
    }
    // "Edited files, ran commands" is a real and very common pair; keeping it as one row is
    // what the target transcript does, and it reads better than two rows of one item each.
    if (last && ((last.kind === "edited" && kind === "ran") || (last.kind === "ran" && kind === "edited"))) {
      last.kind = "mixed";
      last.tools.push(tool);
      continue;
    }
    groups.push({ id: tool.id, kind, tools: [tool] });
  }
  return groups;
};

const LABELS: Record<Group["kind"], string> = {
  ran: "Ran commands",
  edited: "Edited files",
  explored: "Explored",
  searched: "Searched",
  mixed: "Edited files, ran commands",
};

export const labelFor = (group: Group): string => {
  const base = LABELS[group.kind];
  if (group.kind === "ran" && group.tools.length === 1) return "Ran command";
  return base;
};

/** The same label, used as a whole turn's header when there is only one group. */
export const groupHeadline = (group: Group): string => labelFor(group);

export type ActivityGroupView = Group;

/** `13m 42s`, `8s`. Whole numbers only — a transcript is not a stopwatch. */
export const formatDuration = (ms: number): string => {
  const total = Math.max(0, Math.round(ms / 1000));
  if (total < 60) return `${total}s`;
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return seconds === 0 ? `${minutes}m` : `${minutes}m ${seconds}s`;
};

/** `1,198` — grouped, because a five-figure line count is unreadable otherwise. */
export const grouped = (value: number): string => value.toLocaleString("en-US");
