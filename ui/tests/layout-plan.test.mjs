import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  MIN_COL_PX,
  columnsThatFit,
  cycleLayout,
  focusTracks,
  isFocusedTracks,
  planLayout,
  resizeTrack,
  rowsThatFit,
  slotForPointerGrid,
  trackTemplate,
} from "../src/workspace/layoutPlan.ts";

/** A 1600×900 canvas, the size the app opens at on this machine. */
const WIDE = { canvasWidth: 1600, canvasHeight: 900 };

function chats(count) {
  return Array.from({ length: count }, (_, index) => ({ id: `c${index + 1}`, kind: "chat" }));
}

function area(plan, id) {
  const found = plan.areas.find((entry) => entry.id === id);
  assert.ok(found, `${id} must be placed`);
  return found;
}

/** Every window sits inside the grid, and no two windows sit on the same cell. */
function assertTidy(plan) {
  const taken = new Set();
  for (const cell of plan.areas) {
    assert.ok(cell.column >= 1 && cell.column + cell.columnSpan - 1 <= plan.columns.length);
    assert.ok(cell.row >= 1 && cell.row + cell.rowSpan - 1 <= plan.rows.length);
    for (let column = cell.column; column < cell.column + cell.columnSpan; column += 1) {
      for (let row = cell.row; row < cell.row + cell.rowSpan; row += 1) {
        const key = `${column}:${row}`;
        assert.ok(!taken.has(key), `two windows claim cell ${key}`);
        taken.add(key);
      }
    }
  }
}

test("the canvas only claims as many columns as it can actually hold", () => {
  assert.equal(columnsThatFit(1600), 4);
  assert.equal(columnsThatFit(1000), 3);
  assert.equal(columnsThatFit(700), 2);
  assert.equal(columnsThatFit(420), 1);
  // An unmeasured canvas may not collapse to one column on the first paint.
  assert.equal(columnsThatFit(0), 4);
  assert.equal(rowsThatFit(900), 3);
  assert.equal(rowsThatFit(400), 1);
});

test("one window fills the canvas, whatever the layout", () => {
  for (const layout of ["balanced", "adaptive", "smart"]) {
    const plan = planLayout({ layout, windows: chats(1), ...WIDE });
    assert.deepEqual(plan.columns, [1]);
    assert.deepEqual(plan.rows, [1]);
    assert.deepEqual(area(plan, "c1"), {
      id: "c1",
      column: 1,
      columnSpan: 1,
      row: 1,
      rowSpan: 1,
    });
  }
});

test("balanced tiles into rows instead of shaving windows into slivers", () => {
  const plan = planLayout({ layout: "balanced", windows: chats(6), ...WIDE });
  assertTidy(plan);
  assert.equal(plan.columns.length, 4, "a 1600px canvas holds four usable columns");
  assert.equal(plan.rows.length, 2);
  assert.deepEqual(plan.columns, [1, 1, 1, 1], "balanced means balanced");
  // The short last row spreads rather than leaving a hole on the right.
  const last = area(plan, "c6");
  assert.equal(last.row, 2);
  assert.equal(last.columnSpan, 3);
});

test("a canvas too narrow for two columns stacks every layout into one", () => {
  const narrow = { canvasWidth: 420, canvasHeight: 900 };
  for (const layout of ["balanced", "adaptive", "smart"]) {
    const plan = planLayout({ layout, windows: chats(3), ...narrow });
    assertTidy(plan);
    assert.equal(plan.columns.length, 1, `${layout} must not promise a column that cannot fit`);
    assert.equal(plan.rows.length, 3);
  }
});

test("smart fit reads the window kinds: a terminal is worth less width than a chat", () => {
  const withCli = planLayout({
    layout: "smart",
    windows: [
      { id: "chat", kind: "chat" },
      { id: "term", kind: "cli" },
    ],
    ...WIDE,
  });
  const twoChats = planLayout({ layout: "smart", windows: chats(2), ...WIDE });

  const share = (plan) => plan.columns[1] / (plan.columns[0] + plan.columns[1]);
  assert.ok(
    share(withCli) < share(twoChats),
    "a terminal beside a chat takes less room than a second chat would",
  );
  // And the same two windows the other way round hand the width back to the chat.
  const cliPrimary = planLayout({
    layout: "smart",
    windows: [
      { id: "term", kind: "cli" },
      { id: "chat", kind: "chat" },
    ],
    ...WIDE,
  });
  assert.ok(
    cliPrimary.columns[0] / cliPrimary.columns[1] < withCli.columns[0] / withCli.columns[1],
    "a terminal in front does not get a chat's share",
  );
});

test("smart fit keeps one window dominant and tiles the rest beside it", () => {
  const plan = planLayout({ layout: "smart", windows: chats(4), ...WIDE });
  assertTidy(plan);
  const primary = area(plan, "c1");
  assert.equal(primary.column, 1);
  assert.equal(primary.rowSpan, plan.rows.length, "the primary runs the full height");
  assert.ok(plan.columns[0] > plan.columns[1], "and it is the widest column");
  assert.ok(plan.rows.length >= 2, "four windows do not belong in one row of slivers");
});

test("smart fit never asks for more rows than the canvas is tall enough for", () => {
  const short = { canvasWidth: 1600, canvasHeight: 500 };
  const plan = planLayout({ layout: "smart", windows: chats(6), ...short });
  assertTidy(plan);
  assert.equal(rowsThatFit(short.canvasHeight), 2);
  assert.ok(plan.rows.length <= 2, "it widens the side rather than stacking unreadable rows");
});

test("hand-set tracks are honoured, and dropped the moment they stop fitting the grid", () => {
  const base = { layout: "balanced", windows: chats(3), ...WIDE };
  const mine = planLayout({ ...base, columnOverrides: [2, 1, 1] });
  assert.deepEqual(mine.columns, [2, 1, 1]);

  // A window closes: three saved columns cannot describe a two-column grid, so the
  // planner takes over rather than leaving a window at someone else's width.
  const fewer = planLayout({ layout: "balanced", windows: chats(2), ...WIDE, columnOverrides: [2, 1, 1] });
  assert.deepEqual(fewer.columns, [1, 1]);
  // Nonsense never reaches the grid either.
  assert.deepEqual(planLayout({ ...base, columnOverrides: [0, 1, 1] }).columns, [1, 1, 1]);
});

test("resizing moves a boundary: what one window takes, its neighbour gives up", () => {
  const before = [1, 1, 1];
  const after = resizeTrack(before, 0, 0.1);
  assert.ok(after[0] > before[0]);
  assert.ok(after[1] < before[1]);
  assert.equal(after[2], before[2], "a boundary only moves the two tracks it separates");
  assert.ok(
    Math.abs(after.reduce((sum, n) => sum + n, 0) - 3) < 1e-9,
    "the canvas is still exactly full",
  );

  // The last track has no neighbour to its right, so it pushes into its left one.
  const last = resizeTrack(before, 2, 0.1);
  assert.ok(last[2] > before[2] && last[1] < before[1]);

  // Nothing may be squeezed out of existence, and a refused move changes nothing.
  assert.deepEqual(resizeTrack([1, 1], 0, 0.9), [1, 1]);
  assert.deepEqual(resizeTrack([1], 0, 0.1), [1]);
});

test("growing one window leaves the others on screen", () => {
  const grown = focusTracks([1, 1, 1], 1);
  assert.ok(isFocusedTracks(grown, 1));
  assert.ok(!isFocusedTracks(grown, 0));
  assert.ok(
    grown.every((weight) => weight > 0),
    "nothing is zero-width: a hidden pane would lose its scrollback",
  );
  assert.ok(Math.abs(grown.reduce((sum, n) => sum + n, 0) - 3) < 1e-9);
});

test("the layout cycle visits every layout and comes back", () => {
  assert.equal(cycleLayout("balanced"), "adaptive");
  assert.equal(cycleLayout("adaptive"), "smart");
  assert.equal(cycleLayout("smart"), "balanced");
});

test("tracks become a grid template with the planner's own minimum", () => {
  assert.equal(trackTemplate([1, 2], MIN_COL_PX), "minmax(300px, 1fr) minmax(300px, 2fr)");
});

test("a drop lands where the pointer is, reading left to right then top to bottom", () => {
  // Two rows of two, 100×100 windows.
  const rects = [
    { left: 0, right: 100, top: 0, bottom: 100 },
    { left: 110, right: 210, top: 0, bottom: 100 },
    { left: 0, right: 100, top: 110, bottom: 210 },
  ];
  assert.equal(slotForPointerGrid(rects, 10, 10), 0, "before everything");
  assert.equal(slotForPointerGrid(rects, 80, 10), 1, "past the first window in row one");
  assert.equal(slotForPointerGrid(rects, 200, 50), 2, "past both of row one");
  // The case a row of x-centres got wrong: low on the screen is past everything above.
  assert.equal(slotForPointerGrid(rects, 10, 160), 2, "row two, before its first window");
  assert.equal(slotForPointerGrid(rects, 90, 160), 3, "row two, past its window");
});

test("the canvas draws the plan and nothing else", () => {
  const css = readFileSync(new URL("../src/styles/multi-workspace.css", import.meta.url), "utf8");
  const workspace = readFileSync(
    new URL("../src/workspace/MultiSessionWorkspace.tsx", import.meta.url),
    "utf8",
  );

  const canvas = /\.multi-workspace-canvas\s*\{([\s\S]*?)\}/.exec(css);
  assert.ok(canvas, "the canvas needs a rule");
  assert.match(canvas[1], /display: grid/);

  // Layouts used to be hard-coded per window count in CSS, which is why the stylesheet
  // and the planner could disagree. There is one source of truth now.
  assert.ok(!css.includes("layout-smart[data-count"), "no per-count layout rules survive");
  assert.ok(!css.includes("--panel-basis"), "no flex-basis layout maths survives");
  assert.match(workspace, /gridTemplateColumns: trackTemplate\(plan\.columns/);
  assert.match(workspace, /gridTemplateRows: trackTemplate\(plan\.rows/);
});

test("the same layout controls reach both canvases", () => {
  const screen = readFileSync(new URL("../src/screens/ProjectsScreen.tsx", import.meta.url), "utf8");
  const board = readFileSync(
    new URL("../src/workspace/MultiProjectWorkspace.tsx", import.meta.url),
    "utf8",
  );

  // Both branches of the screen hand the canvas the app's layout and auto-fit.
  assert.equal(
    (screen.match(/layout=\{workspaceLayout\}/g) ?? []).length,
    2,
    "Multi mode and All Projects both take the app's layout",
  );
  assert.equal((screen.match(/onApplyLayout=\{onApplyLayout\}/g) ?? []).length, 2);
  assert.equal((screen.match(/onAutoFitChange=\{onSetAutoFit\}/g) ?? []).length, 2);
  // Auto-fit is set by value: a toggle here would drift from the canvas, which turns
  // auto-fit off by itself the moment a split is dragged.
  assert.ok(!screen.includes("onAutoFitChange={onToggleAutoFit}"));

  for (const prop of ["layout={layout}", "autoFit={autoFit}", "onApplyLayout={onApplyLayout}"]) {
    assert.ok(board.includes(prop), `the board forwards ${prop}`);
  }
});

test("the multiplexer chords are on the canvas and written down in the organizer", () => {
  const workspace = readFileSync(
    new URL("../src/workspace/MultiSessionWorkspace.tsx", import.meta.url),
    "utf8",
  );
  const organizer = readFileSync(
    new URL("../src/workspace/WorkspaceOrganizer.tsx", import.meta.url),
    "utf8",
  );

  assert.match(workspace, /if \(!event\.ctrlKey \|\| !event\.altKey/);
  for (const key of ['case "ArrowLeft"', 'case "h"', 'case "j"', 'case "k"', 'case "l"', 'case "z"']) {
    assert.ok(workspace.includes(key), `${key} must be handled`);
  }
  // Every chord the canvas answers to is discoverable from the Organize popover.
  assert.match(organizer, /LAYOUT_SHORTCUTS/);
  assert.match(organizer, /Ctrl\+Alt\+Z/);
  assert.match(organizer, /Ctrl\+Alt\+Space/);
});

test("Ctrl+Alt resizes and Alt alone moves the window — one chord may not do both", () => {
  const workspace = readFileSync(
    new URL("../src/workspace/MultiSessionWorkspace.tsx", import.meta.url),
    "utf8",
  );

  // Live proof this was needed: with the reorder handler answering to any Alt+Arrow, two
  // Ctrl+Alt+Right presses resized the split *and* walked the window two places right.
  const handler = workspace.slice(
    workspace.indexOf("onKeyDown={(e) =>"),
    workspace.indexOf("swapWithNeighbor(session.id, e.key"),
  );
  assert.ok(handler.length > 0, "the panel still handles its own keys");
  assert.match(handler, /if \(e\.ctrlKey \|\| e\.metaKey\) return;/);
});
