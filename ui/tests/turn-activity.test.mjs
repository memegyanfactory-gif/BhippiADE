/**
 * Transcript grouping and formatting (CHT-110, CHT-117).
 *
 * The grouping rule is the part of the chat surface that is genuinely logic rather than
 * layout: it decides what each collapsed row *claims* the agent did, and a row that
 * mislabels its contents is worse than no row. `formatDuration` is here for the same reason —
 * "Worked for 13m 42s" is a sentence about elapsed time, and off-by-one rounding in it reads
 * as a bug in the agent rather than in the clock.
 */

import assert from "node:assert/strict";
import test from "node:test";

import { formatDuration, groupTools } from "../src/components/turnGrouping.ts";

const tool = (overrides) => ({
  id: `t${Math.random().toString(36).slice(2)}`,
  action: "read_source",
  title: "Read file",
  detail: "src/main.rs",
  state: "ok",
  command: null,
  output: null,
  exit_code: null,
  elapsed_ms: null,
  truncated: false,
  changes: [],
  ...overrides,
});

const ran = () => tool({ title: "Ran cargo test", detail: "cargo test --workspace" });
const edited = () => tool({ action: "write_file", title: "Edited mod.rs", detail: "src/mod.rs" });
const explored = () => tool({ title: "Read mod.rs", detail: "src/mod.rs" });

test("consecutive steps of the same kind collapse into one row", () => {
  const groups = groupTools([ran(), ran(), ran()]);
  assert.equal(groups.length, 1);
  assert.equal(groups[0].kind, "ran");
  assert.equal(groups[0].tools.length, 3);
});

test("edits and commands together read as one row, the way the target transcript does", () => {
  const groups = groupTools([edited(), ran()]);
  assert.equal(groups.length, 1);
  assert.equal(groups[0].kind, "mixed");
});

test("grouping is consecutive, not global, so the order the user is following survives", () => {
  // Read, edit, read, edit is four things in that order. Folding them into two buckets would
  // misreport the sequence — which is the whole reason someone expands the rows.
  const groups = groupTools([explored(), edited(), explored(), edited()]);
  assert.equal(groups.length, 4);
  assert.deepEqual(
    groups.map((group) => group.kind),
    ["explored", "edited", "explored", "edited"],
  );
});

test("a write step is grouped by its action, not by whatever its title happens to say", () => {
  // The title comes from a model and cannot be trusted to contain "edit".
  const groups = groupTools([tool({ action: "write_file", title: "Applied the change" })]);
  assert.equal(groups[0].kind, "edited");
});

test("web steps group as searches", () => {
  const groups = groupTools([
    tool({ action: "search_web", title: "Searched" }),
    tool({ action: "fetch_url", title: "Fetched" }),
  ]);
  assert.equal(groups.length, 1);
  assert.equal(groups[0].kind, "searched");
});

test("no steps means no rows, rather than an empty row that expands to nothing", () => {
  assert.deepEqual(groupTools([]), []);
});

test("durations read as a person would say them", () => {
  assert.equal(formatDuration(0), "0s");
  assert.equal(formatDuration(8_400), "8s");
  assert.equal(formatDuration(60_000), "1m");
  assert.equal(formatDuration(822_000), "13m 42s");
  // Never negative, whatever two clocks disagree about.
  assert.equal(formatDuration(-5_000), "0s");
});
