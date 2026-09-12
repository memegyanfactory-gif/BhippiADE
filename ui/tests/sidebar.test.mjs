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
  // Deleting hides behind a hover control, and it carries the owning project so a
  // row from a project that is not the active one is deleted where it actually lives.
  assert.match(rows, /className="proj-row-del"/);
  assert.match(rows, /onDeleteConversation\(session\.id, row\.path\)/);
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
  assert.match(sidebar, /const handleReorder = \(drag: string, over: string, placement\?: "before" \| "after"\)/);
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

// -- Projects lead the rail (2026-09-10) ----------------------------------------

test("the projects list sits above the screen navigation, not below it", () => {
  const list = sidebar.indexOf('className="proj-list"');
  const nav = sidebar.indexOf('<nav className="side-nav"');
  const icons = sidebar.indexOf('className="side-icons"');
  assert.ok(list !== -1 && nav !== -1, "the rail lost its project list or its nav");
  assert.ok(icons < list, "the project list no longer follows the workspace actions");
  assert.ok(list < nav, "the screen navigation is still rendered above the projects");
  // Adding a project reads as the list's own header, so it stays directly above it.
  const add = sidebar.indexOf('className="new-session-dropdown"');
  assert.ok(add < list && add > icons, "New project drifted away from the list it heads");
  assert.match(css, /\.side-nav \{[^}]*border-top: 1px solid var\(--line\)/);
});

test("pressing anywhere on a card opens that project, not just the folder button", () => {
  // The card carries the handler; its own controls (pin, +, bin, session rows)
  // are excluded by hit-testing, so they keep doing their own job.
  assert.match(
    sidebar,
    /className=\{`proj-card[\s\S]{0,2200}?onClick=\{\(event\) => \{[\s\S]{0,400}?closest\("button, a, input, \[role='menu'\]"\)\) return;[\s\S]{0,80}?onSelectProject\(row\);/,
  );
  // The name button survives, because a card div alone is not keyboard reachable.
  assert.match(sidebar, /className="proj-head"\s+onClick=\{\(\) => onSelectProject\(row\)\}/);
  assert.match(css, /\.proj-card \{\s*cursor: pointer;/);
});

test("the card's controls are permanent and sit beside the name, not over it", () => {
  // The owner, on the shipped build: *when I hover, the pin and new project come up; I need
  // it to be there permanently, but change the area where you have added those.* They used
  // to be absolutely positioned over the tail of the card at `opacity: 0`, faded in behind a
  // gradient that ran across the project's own name — so the two controls reached for most
  // were the two that could not be seen, and showing them cost the title.
  const block = css.slice(css.indexOf(".proj-head-actions {"), css.indexOf(".proj-head-action {"));
  assert.doesNotMatch(block, /position: absolute/, "the lane is in the row, not over it");
  assert.doesNotMatch(block, /opacity: 0/, "nothing is hidden until hover");
  assert.doesNotMatch(block, /linear-gradient/, "the name is never covered to make room");

  // Present, but drawn well under the name's weight: quiet at rest, full strength when the
  // card is reached for.
  assert.match(css, /\.proj-head-action \{[^}]*opacity: 0\.55/);
  assert.match(
    css,
    /\.proj-card:hover \.proj-head-action,[\s\S]{0,160}?opacity: 1/,
    "hovering the card brings them up rather than into existence",
  );

  // Three controls, not four: what is rare or destructive moved behind the overflow.
  const lane = sidebar.slice(
    sidebar.indexOf('<span className="proj-head-actions">'),
    sidebar.indexOf("</span>", sidebar.indexOf('<span className="proj-head-actions">')),
  );
  assert.equal(
    (lane.match(/className=\{?[`"]proj-head-action(?!s)/g) ?? []).length,
    3,
    "pin, add and overflow — the bin and the collapse live in the menu",
  );
  assert.match(sidebar, /<IconMore size=\{13\} \/>/, "the overflow has a glyph of its own");
  assert.match(sidebar, /setCardMore\(/, "and a menu behind it");

  // The name is no longer pre-clipped to a width the actions used to leave over.
  assert.match(sidebar, /<strong>\{clipName\(row\.name, 40\)\}<\/strong>/);
  // The live count is a digit next to the name, not a spelled-out pill fighting it.
  assert.match(sidebar, /className="proj-active-count" title=\{`\$\{activeCount\} active`\}>\s*\{activeCount\}/);
});

test("removing a project from the overflow still takes two presses", () => {
  // It cannot be undone, so moving it behind a menu must not also make it a single click.
  assert.match(sidebar, /session-menu-row danger\$\{projectArmed \? " armed" : ""\}/);
  assert.match(sidebar, /if \(!projectArmed\) \{\s*setArmedProjects\(new Set\(\[key\]\)\);\s*return;/);
  assert.match(sidebar, /Click again to remove/);
});

// -- A project card is a key (2026-09-10) ---------------------------------------

test("a card travels when it is pressed and springs back on release", () => {
  // Held down while the pointer is down, released with a one-shot spring — the
  // two states are separate, or the animation would restart on every re-render.
  assert.match(sidebar, /const \[pressPath, setPressPath\] = useState<string \| null>\(null\)/);
  assert.match(sidebar, /const \[popPath, setPopPath\] = useState<string \| null>\(null\)/);
  assert.match(sidebar, /pressPath === key \? " pressing" : ""/);
  assert.match(sidebar, /popPath === key \? " popped" : ""/);
  assert.match(sidebar, /onPointerDown=\{\(event\) => \{[\s\S]{0,1200}?setPressPath\(key\)/);
  assert.match(sidebar, /onPointerUp=\{\(\) => \{\s*if \(pressPath === key\) springBack\(key\)/);
  // The name button is the card's own target, so it presses with the card; the
  // pin, +, bin and session rows press themselves instead.
  assert.match(sidebar, /if \(control && !control\.classList\.contains\("proj-head"\)\) return;/);
  // Leaving or cancelling the gesture must let the card back up.
  assert.match(sidebar, /onPointerLeave=\{\(\) => setPressPath\(/);
  assert.match(sidebar, /onPointerCancel=\{\(\) => setPressPath\(/);
  // The spring is a timer that is cleared on unmount, not a leak.
  assert.match(sidebar, /popTimer\.current = window\.setTimeout\(\(\) => setPopPath\(null\), 420\)/);
  assert.match(sidebar, /if \(popTimer\.current !== null\) window\.clearTimeout\(popTimer\.current\)/);
});

test("the press is real motion, and it collapses under reduced motion", () => {
  assert.match(css, /\.proj-card\.pressing \{[^}]*transform: scale\(0\.968\)/);
  assert.match(css, /\.proj-card\.popped \{\s*animation: proj-press-spring/);
  assert.match(css, /@keyframes proj-press-spring \{[\s\S]*?45% \{\s*transform: scale\(1\.016\)/);
  // The release ring rides ::after, because .proj-card's own box-shadow is
  // pinned to none by the flat-chrome rules above.
  assert.match(css, /\.proj-card\.popped::after \{\s*animation: proj-press-ring/);
  const reduced = css.slice(css.lastIndexOf("@media (prefers-reduced-motion: reduce)"));
  for (const sel of [".proj-card.pressing", ".proj-card.popped", ".side-new:active"]) {
    assert.ok(reduced.includes(sel), `${sel} still animates under reduced motion`);
  }
  assert.match(reduced, /transform: none;\s*animation: none;/);
});

test("each group says how many projects are under it", () => {
  assert.match(sidebar, /<em className="side-sect-count">\{pinnedCount\}<\/em>/);
  assert.match(sidebar, /<em className="side-sect-count">\{railProjects\.length - pinnedCount\}<\/em>/);
  assert.match(css, /\.side-sect-count \{[^}]*border-radius: var\(--radius-sm/);
  // A pinned card carries one pin, not a badge *and* a button: the pin control is the
  // state, so it stays at full strength whether or not the card is hovered.
  assert.doesNotMatch(sidebar, /proj-pin-flag/, "the duplicate badge is gone");
  assert.match(css, /\.proj-head-action\.pin\.active \{[^}]*opacity: 1/);
});

test("the card is clean and minimalistic: crisp corners, compact mark and sharp controls", () => {
  assert.match(css, /\.proj-card \{[^}]*border-radius: var\(--proj-radius/);
  assert.match(css, /\.proj-head-mark \{[^}]*width: var\(--proj-tile/);
  assert.match(css, /\.proj-head-action \{[^}]*border-radius: var\(--radius-sm/);
  // "New project" is a clean, compact, non-rounded button on the list's header line.
  assert.match(css, /\.side-new \{[^}]*border-radius: var\(--radius-sm/);
  assert.match(css, /\.new-session-dropdown \{[^}]*justify-content: flex-end/);
});

// -- Drag and drop reordering ---------------------------------------------------

test("all project cards can be picked up and dragged up or down", () => {
  // Draggable regardless of pin state
  assert.match(sidebar, /className=\{`proj-card[\s\S]*?draggable=\{false\}/);
  assert.match(sidebar, /className="proj-head"[\s\S]*?draggable=\{false\}/);
  // Cursor provides clear grab/grabbing affordance
  assert.match(css, /\.proj-card\[data-project-key\],\s*\.proj-card\[data-project-key\] \.proj-head \{\s*cursor: grab;/);
  assert.match(css, /\.proj-card\.dragging,\s*\.proj-card\.dragging \.proj-head \{\s*opacity: 0\.45;\s*cursor: grabbing;/);
});

test("dragging tracks up vs down relative to card midpoint and shows precision drop indicator", () => {
  assert.match(sidebar, /const midY = rect\.top \+ rect\.height \/ 2;/);
  assert.match(sidebar, /const position: "before" \| "after" = event\.clientY < midY \? "before" : "after";/);
  assert.match(sidebar, /drop-target-\$\{dropPos\}/);
  // Visual drop line styling in CSS
  assert.match(css, /\.proj-card\.drop-target-before::before/);
  assert.match(css, /\.proj-card\.drop-target-after::after/);
  assert.match(css, /\.proj-card-drop-indicator/);
});

test("cross-section drag moves projects between Pinned and Projects sections", () => {
  assert.match(sidebar, /const dragWasPinned = pinnedProjects\.has\(dragKey\);/);
  assert.match(sidebar, /const overIsPinned = pinnedProjects\.has\(overKey\);/);
  assert.match(sidebar, /if \(dragWasPinned !== overIsPinned\) \{/);
  assert.match(sidebar, /if \(overIsPinned\) next\.add\(dragKey\);/);
  assert.match(sidebar, /else next\.delete\(dragKey\);/);
  // Section headers also handle dropping
  assert.match(sidebar, /className=\{`side-sect side-sect-pinned\$\{dropTarget\?\.path === "__pinned_header__" \? " drop-target" : ""\}`\}/);
  assert.match(sidebar, /className=\{`side-sect side-sect-recent\$\{dropTarget\?\.path === "__recent_header__" \? " drop-target" : ""\}`\}/);
});
