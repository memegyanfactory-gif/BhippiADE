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

test("REV-003: the counter belongs to the chat, and resets with a new one", () => {
  // The owner: "make sure each new chat resets the changes and number". The bar used to show
  // the *workspace* diff, so a brand-new chat on a project worked on before opened at
  // "15 files with changes +3446 −837" without having done anything.
  assert.ok(
    chat.includes("const shownStat = useMemo("),
    "the count is derived, not fetched",
  );
  assert.ok(
    /for \(const turn of turns\)/.test(chat),
    "it is summed from this conversation's own turns",
  );
  assert.ok(
    !chat.includes("api.reviewChanges(project.path"),
    "the workspace round trip is what carried the old project's numbers in",
  );
  assert.ok(
    chat.includes("if (paths.size === 0) return null"),
    "a chat that has changed nothing shows no bar at all",
  );

  // The same property the old two-source version protected: the number moves while the turn
  // runs. It does, because the tool-event handler folds each step into `turn.changes`.
  assert.ok(
    chat.includes("const liveChanges = foldTurnChanges(tools)"),
    "steps fold into the turn as they close",
  );
  assert.ok(
    chat.includes("const liveStat = streaming && shownStat !== null"),
    "and the bar knows when the count is still moving",
  );
  assert.ok(
    chat.includes("+{shownStat.additions}") && chat.includes("−{shownStat.deletions}"),
    "the bar renders that count",
  );
});

test("REV-003b: the bar says which changes it is counting", () => {
  // Its own button opens the workspace review, which lists more than this counts, so the
  // label has to name the smaller thing or the two read as a contradiction.
  assert.ok(chat.includes('"file" : "files"} in this chat'), "the label names the scope");
  assert.ok(chat.includes('title="Changed by this conversation"'), "and so does the tooltip");
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
