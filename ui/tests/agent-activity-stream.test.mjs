import assert from "node:assert/strict";
import test from "node:test";

import {
  aggregate,
  aggregateDetail,
  buildActivityStream,
  disclosureOf,
  isClamped,
  markMotionOf,
  markToneOf,
  SUMMARY_LIMIT,
  summaryLine,
  visibleText,
  collapseHistory,
  formatElapsed,
  isLive,
  lineStatOf,
  nodeIsLive,
  outputTail,
  pastTense,
  pathsOf,
  phaseOf,
  resultLine,
  summarize,
  suspendLive,
  titleFor,
} from "../src/agent/activityStream.ts";

let seq = 0;
function step(kind, title, extra = {}) {
  seq += 1;
  return {
    id: `a${seq}`,
    action: "read_source",
    title,
    detail: "",
    state: "ok",
    command: null,
    output: null,
    exit_code: null,
    elapsed_ms: null,
    truncated: false,
    changes: [],
    kind,
    status: "completed",
    description: null,
    metadata: {
      query: null,
      match_count: null,
      paths: [],
      url: null,
      host: null,
      tests_passed: null,
      tests_failed: null,
      agent_label: null,
      parent_id: null,
    },
    started_at: 0,
    completed_at: 1,
    ...extra,
  };
}

function read(path, extra = {}) {
  return step("reading_file", `Reading ${path.split("/").pop()}`, {
    metadata: { ...step("reading_file", "x").metadata, paths: [path] },
    ...extra,
  });
}

function edit(path, additions, deletions, extra = {}) {
  return step("editing_file", `Editing ${path.split("/").pop()}`, {
    changes: [{ path, additions, deletions, status: "modified" }],
    ...extra,
  });
}

test("a finished row speaks in the past, a running one in the present", () => {
  const running = step("reading_file", "Reading PlayerController.ts", { status: "in_progress" });
  const done = step("reading_file", "Reading PlayerController.ts", { status: "completed" });
  assert.equal(titleFor(running), "Reading PlayerController.ts");
  assert.equal(titleFor(done), "Read PlayerController.ts");
  assert.equal(pastTense("Running tests"), "Ran tests");
  assert.equal(pastTense("Checking types"), "Checked types");
  // A verb with no past form is left exactly as the runtime wrote it.
  assert.equal(pastTense("Frobnicating widgets"), "Frobnicating widgets");
});

test("a suspended row counts as live, so a permission prompt never disappears", () => {
  assert.equal(isLive("in_progress"), true);
  assert.equal(isLive("queued"), true);
  assert.equal(isLive("waiting_for_user"), true);
  assert.equal(isLive("completed"), false);
  assert.equal(isLive("cancelled"), false);
});

test("six rapid reads become one row that names all six files", () => {
  const nodes = aggregate([
    read("src/player/PlayerController.ts"),
    read("src/player/Movement.ts"),
    read("src/camera/CameraController.ts"),
    read("src/camera/Shake.ts"),
    read("src/input/Input.ts"),
    read("src/game/Loop.ts"),
  ]);
  assert.equal(nodes.length, 1);
  assert.equal(nodes[0].node, "aggregate");
  assert.equal(nodes[0].title, "Read 6 files");
  assert.equal(nodes[0].paths.length, 6);
  assert.equal(nodes[0].paths[0], "src/player/PlayerController.ts");
});

test("two reads are not worth hiding, so they stay their own rows", () => {
  const nodes = aggregate([read("a.ts"), read("b.ts")]);
  assert.deepEqual(
    nodes.map((node) => node.node),
    ["single", "single"],
  );
});

test("a running read is never folded away — it is the row worth watching", () => {
  const nodes = aggregate([
    read("a.ts"),
    read("b.ts"),
    read("c.ts"),
    read("live.ts", { status: "in_progress" }),
  ]);
  assert.equal(nodes.length, 2);
  assert.equal(nodes[0].node, "aggregate");
  assert.equal(nodes[0].title, "Read 3 files");
  assert.equal(nodes[1].node, "single");
  assert.equal(nodes[1].activity.title, "Reading live.ts");
});

test("folding is consecutive, so it never reorders what the reader is following", () => {
  const nodes = aggregate([
    read("a.ts"),
    read("b.ts"),
    read("c.ts"),
    edit("a.ts", 4, 1),
    read("d.ts"),
    read("e.ts"),
    read("f.ts"),
  ]);
  assert.deepEqual(
    nodes.map((node) => (node.node === "aggregate" ? node.title : node.activity.title)),
    ["Read 3 files", "Editing a.ts", "Read 3 files"],
  );
});

test("an edit aggregate carries the real summed line counts", () => {
  const nodes = aggregate([edit("a.ts", 10, 2), edit("b.ts", 20, 5), edit("c.ts", 4, 1)]);
  assert.equal(nodes[0].node, "aggregate");
  assert.equal(nodes[0].title, "Updated 3 files");
  assert.deepEqual(nodes[0].stat, { additions: 34, deletions: 8 });
});

test("line counts and paths come only from what the steps recorded", () => {
  assert.equal(lineStatOf([read("a.ts")]), null);
  assert.deepEqual(lineStatOf([edit("a.ts", 3, 1)]), { additions: 3, deletions: 1 });
  // The same file touched twice is one file, not two.
  assert.deepEqual(pathsOf([edit("a.ts", 1, 0), edit("a.ts", 2, 0)]), ["a.ts"]);
});

test("a short stream is left exactly as it is", () => {
  const nodes = [read("a.ts"), edit("b.ts", 1, 1)].map((activity) => ({
    node: "single",
    id: activity.id,
    activity,
  }));
  assert.equal(collapseHistory(nodes).length, 2);
});

test("a long stream compresses its history but never its live row", () => {
  const activities = [];
  for (let index = 0; index < 12; index += 1) activities.push(read(`file${index}.ts`));
  activities.push(edit("Game.tsx", 12, 3));
  activities.push(step("running_tests", "Running tests", { status: "in_progress" }));

  const nodes = buildActivityStream(activities);
  const live = nodes.filter(nodeIsLive);
  assert.equal(live.length, 1, "the running row must survive compression");
  assert.equal(live[0].node, "single");
  assert.equal(live[0].activity.title, "Running tests");
  assert.ok(nodes.length <= 8, "a compressed stream is short enough to read");
});

test("a group says what is actually inside it", () => {
  const children = [
    { node: "aggregate", id: "g1", title: "Read 12 files", activities: Array.from({ length: 12 }, (_, index) => read(`f${index}.ts`)), paths: [], stat: null },
    { node: "single", id: "g2", activity: edit("Game.tsx", 182, 61) },
  ];
  const summary = summarize(children);
  assert.match(summary, /12 files read/);
  assert.match(summary, /1 file changed/);
  assert.match(summary, /\+182 −61/);
});

test("phases group work the way a person would describe it", () => {
  assert.equal(phaseOf("reading_file"), "explore");
  assert.equal(phaseOf("searching_code"), "explore");
  assert.equal(phaseOf("editing_file"), "implement");
  assert.equal(phaseOf("running_tests"), "verify");
  assert.equal(phaseOf("typechecking"), "verify");
  assert.equal(phaseOf("clicking_ui"), "interact");
  assert.equal(phaseOf("using_tool"), "other");
});

test("a permission prompt suspends the running row instead of leaving it spinning", () => {
  const activities = [read("a.ts"), step("running_command", "Running", { status: "in_progress" })];
  const suspended = suspendLive(activities);
  assert.equal(suspended[0].status, "completed", "finished work is untouched");
  assert.equal(suspended[1].status, "waiting_for_user");
  // Suspended still counts as live, so the row keeps its place and is never compressed.
  assert.equal(isLive(suspended[1].status), true);
});

test("a command's result line is the runner's own number, or nothing", () => {
  const passing = step("running_tests", "Running tests", {
    metadata: { ...step("x", "y").metadata, tests_passed: 24, tests_failed: 0 },
  });
  assert.equal(resultLine(passing), "24 tests passed");

  const failing = step("running_tests", "Running tests", {
    metadata: { ...step("x", "y").metadata, tests_passed: 12, tests_failed: 2 },
  });
  assert.equal(resultLine(failing), "12 of 14 tests passed");

  const broke = step("building_project", "Building project", { exit_code: 1 });
  assert.equal(resultLine(broke), "exit 1");

  // Nothing reported means no line, rather than a reassuring invention.
  assert.equal(resultLine(step("running_command", "Running")), null);
});

test("only the tail of a running command's output is shown", () => {
  const output = Array.from({ length: 40 }, (_, index) => `line ${index}`).join("\n");
  const tail = outputTail(output, 4);
  assert.deepEqual(tail, ["line 36", "line 37", "line 38", "line 39"]);
  assert.deepEqual(outputTail(null), []);
  assert.deepEqual(outputTail("a\n\n\nb", 4), ["a", "b"]);
});

test("elapsed time reads the way a person says it", () => {
  assert.equal(formatElapsed(4200), "4.2s");
  assert.equal(formatElapsed(45_000), "45s");
  assert.equal(formatElapsed(60_000), "1m");
  assert.equal(formatElapsed(82_000), "1m 22s");
});

// ── a row that opens must have something behind it ───────────────────────────────────

test("a failure discloses its whole message, however the row clipped it", () => {
  // The shape the owner hit: an engine rejection whose entire content is its description.
  const message =
    "that is not a Godot action batch: control character (\u0000-\u001f) found while parsing " +
    "the payload at line 1 column 812 — every action must be one of the typed verbs in the " +
    "schema, and the batch must be a JSON array";
  const failed = step("failed", "Engine call failed", {
    state: "failed",
    status: "failed",
    description: message,
  });

  const disclosure = disclosureOf(failed);
  assert.ok(disclosure, "a failure with a message always opens");
  // The whole thing, not the clipped line — that is the entire point of opening it.
  assert.ok(disclosure.note.includes("must be a JSON array"));
  assert.ok(disclosure.note.length > SUMMARY_LIMIT);

  // And the collapsed line is honestly marked as partial.
  const line = summaryLine(message);
  assert.ok(line.length <= SUMMARY_LIMIT);
  assert.ok(line.endsWith("…"));
  assert.equal(isClamped(message), true);
});

test("a short line says everything, so it offers nothing to open", () => {
  const quiet = step("reading_file", "Reading main.rs", { description: "src/main.rs" });
  assert.equal(disclosureOf(quiet), null);
  assert.equal(isClamped("src/main.rs"), false);
  assert.equal(summaryLine("src/main.rs"), "src/main.rs");
});

test("a short failure still opens — an error you cannot finish reading is the failure twice", () => {
  const failed = step("failed", "Engine change rejected", {
    state: "failed",
    status: "failed",
    description: "no such node",
  });
  const disclosure = disclosureOf(failed);
  assert.ok(disclosure);
  assert.equal(disclosure.note, "no such node");
});

test("control characters are shown rather than swallowed", () => {
  // The error in the screenshot is *about* these bytes; a <pre> renders them as nothing,
  // so the one thing the reader needs to see is the one thing that disappears.
  const raw = "a" + String.fromCharCode(0) + "b" + String.fromCharCode(31) + "c";
  assert.equal(visibleText(raw), String.raw`a\u0000b\u001fc`);
  // Newlines and tabs are layout, not evidence, and are left alone.
  assert.equal(visibleText("a\nb\tc"), "a\nb\tc");
  assert.equal(summaryLine("a" + String.fromCharCode(0) + "b"), String.raw`a\u0000b`);
});

test("a disclosure carries every kind of thing a step can leave behind", () => {
  const ran = step("running_command", "Running cargo test", {
    command: "cargo test --workspace --all-targets",
    output: "running 320 tests\nok",
    truncated: true,
    changes: [{ path: "src/a.rs", additions: 3, deletions: 1 }],
    metadata: { ...step("x", "y").metadata, paths: ["src/a.rs", "src/b.rs"], url: "https://e.dev" },
  });
  const disclosure = disclosureOf(ran);
  assert.ok(disclosure);
  assert.equal(disclosure.url, "https://e.dev");
  assert.equal(disclosure.output, "running 320 tests\nok");
  assert.equal(disclosure.outputTruncated, true);
  // The changed file keeps its counts; a file merely named gets zeroes, and neither is
  // listed twice.
  assert.deepEqual(disclosure.files, [
    { path: "src/a.rs", additions: 3, deletions: 1 },
    { path: "src/b.rs", additions: 0, deletions: 0 },
  ]);
});

test("a folded row always accounts for every step it counted", () => {
  // The owner's other report: "Read 3 files ›" opening onto an empty box, because none of
  // the folded steps recorded a path.
  const pathless = [
    step("reading_file", "Reading the project", { description: "project.godot" }),
    step("reading_file", "Reading the scene"),
    step("reading_file", "Reading the script"),
  ];
  const entries = aggregateDetail(pathless);
  assert.equal(entries.length, 3, "one line per step the headline counted");
  assert.deepEqual(
    entries.map((entry) => entry.title),
    ["Read the project", "Read the scene", "Read the script"],
  );
  assert.equal(entries[0].note, "project.godot");
  assert.equal(entries[0].path, null);

  // And it still prefers real paths when the steps have them.
  const withPaths = [read("src/a.ts"), read("src/b.ts"), read("src/a.ts")];
  const named = aggregateDetail(withPaths);
  assert.deepEqual(
    named.map((entry) => entry.path),
    ["src/a.ts", "src/b.ts"],
    "paths are listed once each, in the order they were first touched",
  );

  // The rule that matters: a non-empty run never opens onto nothing.
  for (const run of [pathless, withPaths, [step("reading_file", "Reading")]]) {
    assert.ok(aggregateDetail(run).length > 0);
  }
  assert.deepEqual(aggregateDetail([]), []);
});

// ── the mark ─────────────────────────────────────────────────────────────────────────

test("the mark sweeps while looking and turns while doing", () => {
  const live = { state: "running", status: "in_progress" };
  assert.equal(markMotionOf(step("searching_code", "Searching", live)), "seeking");
  assert.equal(markMotionOf(step("reading_file", "Reading", live)), "seeking");
  assert.equal(markMotionOf(step("editing_file", "Editing", live)), "working");
  assert.equal(markMotionOf(step("running_tests", "Testing", live)), "working");
  // A finished row is history and a suspended one is not working: neither animates.
  assert.equal(markMotionOf(step("editing_file", "Edited")), "still");
  assert.equal(
    markMotionOf(step("editing_file", "Editing", { state: "running", status: "waiting_for_user" })),
    "still",
  );
});

test("the mark's colour names the state before it names the kind", () => {
  const live = { state: "running", status: "in_progress" };
  assert.equal(markToneOf(step("reading_file", "Reading", live)), "explore");
  assert.equal(markToneOf(step("editing_file", "Editing", live)), "implement");
  assert.equal(markToneOf(step("running_tests", "Testing", live)), "verify");
  assert.equal(markToneOf(step("clicking_ui", "Clicking", live)), "interact");
  assert.equal(markToneOf(step("using_tool", "Using", live)), "other");

  // Why it stopped outranks what it was in the middle of.
  assert.equal(
    markToneOf(step("editing_file", "Editing", { state: "failed", status: "failed" })),
    "failed",
  );
  assert.equal(
    markToneOf(step("editing_file", "Editing", { state: "running", status: "waiting_for_user" })),
    "waiting",
  );
  // Finished rows stay monochrome: only the row happening now is emphasised.
  assert.equal(markToneOf(step("editing_file", "Edited")), "quiet");
});
