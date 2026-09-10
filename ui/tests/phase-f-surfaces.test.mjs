/**
 * SPA-401…404: the owner's second round.
 *
 * "Dragging the window should pick it up and move with the cursor, drop where I leave it,
 * and show Windows' snap layouts at the edge"; "the engine viewport should follow the
 * splitter while I drag, not after"; "too many chats should slim the tabs like Chrome";
 * "the browser needs tabs and a clean Google-like start page". Source pins for each.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const read = (rel) => fs.readFileSync(path.join(here, "..", "src", rel), "utf8");

test("a window is picked up with the pointer, not the HTML5 drag protocol", () => {
  const ws = read("workspace/MultiSessionWorkspace.tsx");
  assert.ok(!ws.includes("draggable={true}"), "no dragstart/drop dance");
  assert.ok(!ws.includes("onDragOver"), "no dragover either");
  assert.ok(ws.includes("onPointerDown={(event) => beginPress(event, session.id)}"), "the title bar starts a press");
  assert.ok(ws.includes("setPointerCapture(event.pointerId)"), "the pointer is captured so a fast drag cannot escape");
  assert.ok(ws.includes("LIFT_THRESHOLD_PX"), "a click is not a lift");
  assert.ok(ws.includes('position: "fixed"'), "the lifted window rides the pointer");
  assert.ok(ws.includes("session-panel-placeholder"), "the others open a gap where it will land");
  assert.ok(ws.includes("requestAnimationFrame"), "the ghost, the gap and the zone settle once per frame");
  assert.ok(ws.includes('if (event.key === "Escape") settleLift(false);'), "Escape puts it back");
});

test("edges and the top edge snap like Windows", () => {
  const ws = read("workspace/MultiSessionWorkspace.tsx");
  assert.ok(ws.includes("EDGE_SNAP_PX"), "a half-screen snap at either edge");
  assert.ok(ws.includes("snap-edge-preview"), "…with the translucent preview Windows draws");
  assert.ok(ws.includes("TOP_SNAP_PX"), "the top edge opens the snap-layout menu");
  assert.ok(ws.includes("export const SNAP_TEMPLATES"), "the templates are data");
  for (const id of ['"halves"', '"primary"', '"thirds"', '"focus"']) {
    assert.ok(ws.includes(`id: ${id}`), `${id} is a template`);
  }
  assert.ok(ws.includes("applyLayout(template.layout)"), "a cell drop applies the layout");
  assert.ok(ws.includes("onApplyLayout?.(next)"), "…and tells the organizer");
  const projects = read("screens/ProjectsScreen.tsx");
  assert.ok(projects.includes("onApplyLayout={onApplyLayout}"), "the screen wires the callback");
  const css = read("styles/multi-workspace.css");
  for (const cls of [".session-panel.is-lifted", ".session-panel-placeholder", ".snap-layouts", ".snap-cell.hot", ".snap-edge-preview"]) {
    assert.ok(css.includes(cls), `${cls} is styled`);
  }
});

test("the engine viewport follows the splitter while it moves", () => {
  const viewport = read("studio/GodotViewport.tsx");
  assert.ok(
    !viewport.includes("if (resizingRef.current && !force) return;"),
    "a drag no longer suppresses layout pushes",
  );
  assert.ok(viewport.includes("frame = window.requestAnimationFrame(tick);"), "one push per frame at most");
  assert.ok(viewport.includes("sameBox(last.box, box)"), "and none when nothing moved");
});

test("the browser has Chrome's tabs and Google's start page", () => {
  const browser = read("workbench/BrowserView.tsx");
  assert.ok(browser.includes("type BrowserTab = {"), "a tab is its own history and address");
  assert.ok(browser.includes('role="tablist"'), "a strip of tabs");
  assert.ok(browser.includes("className=\"browser-tab-new\""), "+ opens another");
  assert.ok(browser.includes('if (key === "t")') && browser.includes('} else if (key === "w")'), "Ctrl+T and Ctrl+W");
  assert.ok(browser.includes("closing the front tab lands on the one to its left"), "Chrome's close rule");
  assert.ok(browser.includes("chrome-home-wordmark"), "the start page is a wordmark and a pill");
  assert.ok(!browser.includes("chrome-search-submit-btn"), "no Go button — Enter is the button");
  assert.ok(!browser.includes("https://www.reddit.com"), "a quieter row of shortcuts");
  const css = read("styles/workbench.css");
  assert.ok(css.includes(".browser-tabs {") && css.includes(".browser-tab.active {"), "the strip is styled");
  assert.ok(css.includes("@container (max-width: 64px)"), "tabs shrink to the favicon like Chrome's");
});
