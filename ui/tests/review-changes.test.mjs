/**
 * The Review Changes surfaces: the counter above the composer, and the panel behind
 * "See the work".
 *
 * Three complaints from the owner, one shape — the studio did the work and would not show
 * it. A whole game was built and the review said the workspace was clean; the turn card said
 * "Edited 5 files +0 −0"; and "See the work" opened a panel that was clipped away by the
 * pane it was drawn inside. What the page owns of the fix is pinned here; the counting
 * itself is Rust's (INV-051) and is pinned in `bhippi-app::review`.
 */

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

// .gitattributes pins the sources to LF but not every file, so a Windows checkout can hand
// these back with CRLF and every multi-line needle below would silently miss.
const read = (rel) =>
  readFileSync(new URL(rel, import.meta.url), "utf8").replaceAll("\r\n", "\n");

const chat = read("../src/screens/Chat.tsx");
const dock = read("../src/screens/ActivityDock.tsx");
const modal = read("../src/screens/ReviewChangesModal.tsx");

test("REV-001: the work panel is drawn outside its pane, so nothing can clip it", () => {
  // It is `position: fixed`, which is not enough on its own: a transformed or clipping
  // ancestor becomes the containing block, and in the project and engine layouts the chat
  // sits inside one. Rendering into `document.body` is what actually gets it out.
  assert.ok(dock.includes("createPortal"), "the panel must be portalled");
  assert.ok(
    dock.includes('import { createPortal } from "react-dom"'),
    "createPortal must come from react-dom",
  );
  assert.match(
    dock,
    /createPortal\([\s\S]*document\.body,\s*\)/,
    "the panel and its scrim must be portalled to document.body",
  );
});

test("REV-002: the panel and its scrim outrank the surfaces they cover", () => {
  const css = read("../src/styles/activity.css");
  const zIndex = (selector) => {
    const block = css.slice(css.indexOf(selector));
    const match = block.slice(0, block.indexOf("}")).match(/z-index:\s*(\d+)/);
    return match ? Number(match[1]) : null;
  };
  const scrim = zIndex(".activity-scrim {");
  const panel = zIndex(".activity-panel {");
  assert.ok(scrim >= 1000, `the scrim must sit above the studio chrome, got ${scrim}`);
  assert.ok(panel > scrim, "the panel must sit above its own scrim");
});

test("REV-003: the counter moves while the turn runs, not only once it ends", () => {
  // The owner asked for a number that "keeps changing according to the update that the ai
  // does". The workspace review is a round trip and is only asked for when the files stop
  // moving, so the running turn's own folded changes carry the count in the meantime.
  assert.ok(chat.includes("const liveStat = useMemo("), "there must be a live count");
  assert.ok(
    chat.includes("const shownStat = liveStat ?? reviewStat"),
    "the live count must win while it exists, and the measured one otherwise",
  );
  assert.ok(
    chat.includes("+{shownStat.additions}") && chat.includes("−{shownStat.deletions}"),
    "the bar must render the chosen count, not the idle one",
  );
  assert.ok(
    !chat.includes("+{reviewStat.additions}"),
    "no path may still render the idle-only count",
  );
});

test("REV-004: a running turn folds every step's changes as they close", () => {
  // Without this the turn card sits at "+0 −0" until the turn ends, which is the screenshot
  // the owner sent.
  assert.ok(chat.includes("function foldTurnChanges("), "steps must fold into a turn total");
  assert.ok(
    chat.includes("const liveChanges = foldTurnChanges(tools)"),
    "the fold must run on every tool event",
  );
});

test("REV-005: both review surfaces tell the viewport they are covering it", () => {
  for (const [name, source] of [["the dock", dock], ["the modal", modal]]) {
    assert.ok(
      source.includes("useObstructsViewport"),
      `${name} must join the obstruction registry so the Godot window yields to it`,
    );
  }
});
