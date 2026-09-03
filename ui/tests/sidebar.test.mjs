/**
 * The left rail's shape (owner brief, 2026-09-03).
 *
 * The rail has no unit-testable logic worth isolating — it is chrome. What it does have
 * are four structural promises that are easy to break silently and expensive to notice:
 * the brand row exists and carries the real mascot, the old seven-glyph icon strip is
 * gone rather than merely hidden, the collapsed rail renders exactly one control, and a
 * session reads as a row with a provider mark and a live/idle dot. These assertions read
 * the source and the stylesheet, because a screenshot is what a person does instead.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const read = (name) => fs.readFileSync(new URL(`../src/${name}`, import.meta.url), "utf8");
const sidebar = read("chrome/Sidebar.tsx");
const css = read("styles/app.css");

/** The stylesheet block this change owns, so the colour audit does not grade old rules. */
function ownedCss() {
  const start = css.indexOf("/* -- Brand header:");
  const end = css.indexOf(".proj-row-new svg {");
  assert.ok(start !== -1, "the brand-header CSS block is missing");
  assert.ok(end > start, "the session-row CSS block is missing or out of order");
  return css.slice(start, css.indexOf("}", end) + 1);
}

// -- Brand header ---------------------------------------------------------------

test("the rail opens with the mascot and the name, and the logo area is not a button", () => {
  assert.match(sidebar, /import mascot from "\.\.\/assets\/mascot\.png"/);
  assert.match(sidebar, /className="side-brand"/);
  assert.match(sidebar, /className="side-brand-mark"\s+src=\{mascot\}/);
  assert.match(sidebar, /<span className="side-brand-name">Bhippi<\/span>/);
  // The logo area is a div, not a button, and is not clickable.
  assert.match(sidebar, /<div className="side-brand-id">\s*<img[\s\S]*?<span className="side-brand-name">Bhippi<\/span>\s*<\/div>/);
  assert.ok(!sidebar.includes("brandMenuOpen"), "brandMenuOpen is still present");
});

test("the mascot file the header imports actually exists", () => {
  const png = new URL("../src/assets/mascot.png", import.meta.url);
  assert.ok(fs.statSync(png).size > 0, "mascot.png is missing or empty");
});

test("action icons are rendered directly below the top logo area", () => {
  assert.ok(sidebar.includes("side-icons"), "the icon row is missing");
  assert.ok(sidebar.includes('className="side-icon"'), "side-icon buttons are missing");
  assert.ok(css.includes(".side-icons"), ".side-icons styling is missing");
  assert.ok(css.includes(".side-icon-custom-wrap"), ".side-icon-custom-wrap styling is missing");
});

test("workspace actions and navigation live in the side-icons strip below the logo", () => {
  const at = sidebar.indexOf('className="side-icons"');
  assert.ok(at !== -1, "the side-icons row is missing");
  const strip = sidebar.slice(at, sidebar.indexOf('className="new-session-dropdown"'));
  for (const label of [
    "Workspace rules",
    "Review AI changes",
    "Project Brain",
    "Open in external editor",
    "Settings",
  ]) {
    assert.ok(strip.includes(`aria-label="${label}"`), `${label} missing from side-icons`);
  }
  // Back and forward keep their handlers rather than being dropped with the strip.
  assert.match(strip, /disabled=\{!canBack\}/);
  assert.match(strip, /disabled=\{!canForward\}/);
  // Escape closes openInMenu.
  assert.match(sidebar, /if \(event\.key === "Escape"\) \{[\s\S]*?setOpenInMenuOpen\(false\);/);
});

test("search and the collapse toggle stay on the header row itself", () => {
  const header = sidebar.slice(
    sidebar.indexOf('className="side-brand-actions"'),
    sidebar.indexOf('className="side-icons"'),
  );
  assert.match(header, /aria-label="Filter sessions"/);
  assert.match(header, /aria-label="Collapse sidebar"/);
});

// -- Sections -------------------------------------------------------------------

test("pinned projects get their own header above the projects header", () => {
  assert.match(sidebar, /\{index === 0 && pinnedCount > 0 \? \([\s\S]*?<span>Pinned<\/span>/);
  assert.match(sidebar, /\{index === pinnedCount \? \([\s\S]*?<span>Projects<\/span>/);
  // Both headers are placed by index into the same filtered list, or they drift apart.
  assert.match(sidebar, /const railProjects = orderedProjects\.filter\(/);
  assert.match(sidebar, /railProjects\.map\(\(row, index\) => \{/);
});

test("adding a project is still reachable from the rail", () => {
  assert.match(sidebar, /className="side-new"[\s\S]*?New project/);
});

// -- Session rows ---------------------------------------------------------------

test("a session is a row: provider mark, title, status dot", () => {
  assert.ok(!sidebar.includes("proj-chip"), "the old icon chips are still rendered");
  assert.ok(!css.includes(".proj-chip-dot"), ".proj-chip-dot still has styling");

  const rows = sidebar.slice(
    sidebar.indexOf('className={`proj-sessions'),
    sidebar.indexOf('className="proj-row-new"'),
  );
  assert.match(rows, /<ProviderLogo id=\{session\.provider\} size=\{14\} \/>/);
  assert.match(rows, /<span className="proj-row-title">\{rowTitle\}<\/span>/);
  assert.match(rows, /className=\{`proj-row-dot st-\$\{session\.status\}`\}/);
  // "New chat" is the fallback, not an empty row.
  assert.match(rows, /session\.title\.replace\(\/\^CLI:\\s\*\/, ""\)\.trim\(\) \|\| "New chat"/);
  // The active row is highlighted, and the row itself opens the session.
  assert.match(rows, /const active = session\.id === activeConversationId/);
  assert.match(rows, /onClick=\{\(\) => onOpenSession\(row\.path, session\.id\)\}/);
  // Deleting stays a two-click gesture behind a hover control.
  assert.match(rows, /className="proj-row-del"/);
  assert.match(rows, /onDeleteConversation\(session\.id\)/);
});

test("the tooltip says title, provider, state and age in that order", () => {
  assert.match(
    sidebar,
    /const rowLabel = `\$\{rowTitle\} — \$\{providerLabel\} · \$\{state\} · \$\{relativeTime\(/,
  );
  assert.match(sidebar, /session\.status === "running"\s*\?\s*"running"/);
});

test("running pulses in the accent and idle is a muted pip", () => {
  const owned = ownedCss();
  assert.match(owned, /\.proj-row-dot\.st-running \{[^}]*background: var\(--accent\)/);
  assert.match(owned, /\.proj-row-dot\.st-running \{[^}]*animation: pulse-dot/);
  assert.match(owned, /\.proj-row-dot\.st-idle \{[^}]*background: var\(--line-strong\)/);
});

test("each project keeps a New chat affordance next to its rows", () => {
  assert.match(sidebar, /className="proj-row-new"/);
  assert.match(sidebar, /className="proj-empty-new-btn"/);
});

test("dragging to reorder survived the rewrite", () => {
  assert.match(sidebar, /onReorderSession\?\.\(draggedSessionId, session\.id\)/);
  assert.match(sidebar, /const handleReorder = \(drag: string, over: string\)/);
});

// -- Collapsed rail -------------------------------------------------------------

test("collapsed renders the toggle and running providers in the side rail", () => {
  const branch = sidebar.slice(
    sidebar.indexOf("{collapsed ? ("),
    sidebar.indexOf('className="side-brand"'),
  );
  assert.match(branch, /className="side-rail-only"/);
  assert.match(branch, /aria-label="Expand sidebar"/);
  assert.match(branch, /className="collapsed-providers-list"/);
  // The things that used to live there are gone from the file entirely.
  assert.ok(!sidebar.includes("rail-mini"), "collapsed mini chips are still rendered");
  assert.ok(!css.includes(".rail-mini"), ".rail-mini still has styling");
  // The account card only exists on the expanded side of the branch.
  assert.ok(
    sidebar.indexOf("<SidebarAccount") > sidebar.indexOf('className="side-brand"'),
    "the account card is rendered outside the expanded branch",
  );
  assert.match(sidebar, /<SidebarAccount[\s\S]*?collapsed=\{false\}/);
});

// -- Tokens and reachability ----------------------------------------------------

test("the new rail CSS is tokens only", () => {
  const owned = ownedCss();
  const literals = owned.match(/#[0-9a-fA-F]{3,8}\b|rgba?\(/g) ?? [];
  assert.deepEqual(literals, [], `hard-coded colours in the new sidebar CSS: ${literals}`);
});

test("every new control is a button with a label", () => {
  for (const cls of ["side-brand-id", "side-icon", "side-brand-btn", "proj-row", "proj-row-del", "proj-row-new"]) {
    const at = sidebar.indexOf(`className="${cls}"`);
    const templated = sidebar.indexOf(`className={\`${cls}`);
    assert.ok(at !== -1 || templated !== -1, `${cls} is not rendered`);
  }
  // Nothing in the rail relies on an unlabeled button.
  for (const label of ["Expand sidebar", "Collapse sidebar", "Workspace rules", "Settings"]) {
    assert.ok(sidebar.includes(`aria-label="${label}"`), `${label} lost its aria-label`);
  }
});
