/**
 * The workspace opens itself (ADR-0045).
 *
 * The owner's ask: the engine is on when the app is launched — no button. So the studio must
 * open the Godot editor into the viewport as soon as it knows what the viewport holds, and
 * again whenever the active project changes. The decision is a pure function because the two
 * ways it can go wrong are both timing: asking twice for one project (the embed state event
 * fires on every layout change), and reopening a workspace the user just closed on purpose.
 *
 * What changed here, and why: the guard used to be a Set of every project settled *this
 * session*, so a project could only ever be offered a workspace once. A → B → A therefore
 * left A with a permanently empty viewport, which is the "the viewport does not follow the
 * project" bug. The guard is now the single project currently settled, so changing the
 * active project clears the suppression and the viewport follows. Three expectations moved
 * with it: `opened: Set` became `settled: string | null`; the "closing does not reopen it"
 * case now also asserts that coming *back* to a project does reopen; and the source
 * assertions read `settledProject` (a `useRef<string | null>`) instead of `autoOpened.add`.
 * Everything else — nothing before the embed state, nothing without a project, one call per
 * project, a live workspace settles rather than reopens — is unchanged.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

import { decideAutoOpen, workspaceHolds } from "../src/studio/workspaceAutoOpen.ts";

const root = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, root), "utf8");

const DEMO = "C:/Users/aayus/BhippiGames/demo-game";
const OTHER = "C:/Users/aayus/BhippiGames/other-game";
const embedWith = (project) => ({ workspace: project === null ? null : { project } });

// ── the decision ───────────────────────────────────────────────────────────────

test("nothing happens until Rust has said what the viewport already holds", () => {
  assert.deepEqual(decideAutoOpen({ projectPath: DEMO, embed: null, settled: null }), {
    open: null,
    remember: null,
  });
});

test("with no project there is nothing to open and nothing to remember", () => {
  assert.deepEqual(decideAutoOpen({ projectPath: "", embed: embedWith(null), settled: null }), {
    open: null,
    remember: null,
  });
});

test("a project with an empty viewport opens its workspace, once", () => {
  const first = decideAutoOpen({ projectPath: DEMO, embed: embedWith(null), settled: null });
  assert.equal(first.open, DEMO, "the path is passed through as the app spells it");
  assert.equal(first.remember, DEMO.toLowerCase());
  // The state event fires again — a layout change, the editor still starting. No second ask.
  const second = decideAutoOpen({
    projectPath: DEMO,
    embed: embedWith(null),
    settled: first.remember,
  });
  assert.deepEqual(second, { open: null, remember: null });
});

test("a workspace that is already live is settled, not opened again", () => {
  // Rust reports the display path; the page holds the same one. Slashes and case must not
  // make them look like two projects.
  const decision = decideAutoOpen({
    projectPath: "C:\\Users\\aayus\\BhippiGames\\Demo-Game",
    embed: embedWith(DEMO),
    settled: null,
  });
  assert.equal(decision.open, null);
  assert.equal(decision.remember, DEMO.toLowerCase());
});

test("closing the workspace does not reopen it, but changing project does", () => {
  const settled = decideAutoOpen({
    projectPath: DEMO,
    embed: embedWith(DEMO),
    settled: null,
  }).remember;
  // The user pressed "Close workspace": the viewport is empty and must stay empty while
  // they are still on this project.
  assert.deepEqual(decideAutoOpen({ projectPath: DEMO, embed: embedWith(null), settled }), {
    open: null,
    remember: null,
  });
  // A different project is a different decision, even with the first one's editor still up.
  const next = decideAutoOpen({ projectPath: OTHER, embed: embedWith(DEMO), settled });
  assert.equal(next.open, OTHER);
  assert.equal(next.remember, OTHER.toLowerCase());
});

test("the viewport follows the project: A then B then A reopens A", () => {
  // A: nothing in the hole, so the editor is offered and A becomes the settled project.
  const a = decideAutoOpen({ projectPath: DEMO, embed: embedWith(null), settled: null });
  assert.equal(a.open, DEMO);

  // B: a new active project clears the suppression, whatever is still embedded.
  const b = decideAutoOpen({ projectPath: OTHER, embed: embedWith(DEMO), settled: a.remember });
  assert.equal(b.open, OTHER);
  assert.equal(b.remember, OTHER.toLowerCase());

  // Back to A. Under the old "settled for the session" rule this returned nothing and the
  // viewport stayed on B — the bug. The workspace is offered again.
  const again = decideAutoOpen({ projectPath: DEMO, embed: embedWith(OTHER), settled: b.remember });
  assert.equal(again.open, DEMO);
  assert.equal(again.remember, DEMO.toLowerCase());

  // And it is still only asked once while A stays active.
  assert.deepEqual(
    decideAutoOpen({ projectPath: DEMO, embed: embedWith(OTHER), settled: again.remember }),
    { open: null, remember: null },
  );
});

test("a workspace open on another project is not this project's workspace", () => {
  assert.equal(workspaceHolds(embedWith(DEMO), DEMO), true);
  assert.equal(workspaceHolds(embedWith(DEMO), OTHER), false);
  assert.equal(workspaceHolds(embedWith(null), DEMO), false);
  assert.equal(workspaceHolds(null, DEMO), false);
  assert.equal(workspaceHolds(embedWith(DEMO), ""), false);
});

// ── the wiring ─────────────────────────────────────────────────────────────────

test("the studio asks once per active project, from the embed state, and never auto-plays", () => {
  const screen = read("src/screens/StudioScreen.tsx");
  assert.match(screen, /import \{ decideAutoOpen \} from "\.\.\/studio\/workspaceAutoOpen"/);
  assert.match(
    screen,
    /const settledProject = useRef<string \| null>\(null\)/,
    "one settled project, not a set — a set can never be cleared by a project change",
  );
  assert.match(
    screen,
    /decideAutoOpen\(\{ projectPath, embed, settled: settledProject\.current \}\)/,
    "the decision reads the live embed state and the settled project",
  );
  assert.match(
    screen,
    /if \(decision\.remember !== null\) settledProject\.current = decision\.remember;/,
    "a project is settled before the call, so a refusal is not retried",
  );
  assert.match(screen, /void act\("open the workspace", \(\) => api\.godotEmbedOpenWorkspace\(path\)\)/);
  assert.match(screen, /\}, \[act, embed, projectPath\]\);/, "the effect re-runs on a project change");

  // Play is the one thing that stays a button.
  assert.equal(screen.match(/api\.godotEmbedPlay\(/g).length, 1);
  assert.match(screen, /const handlePlay = useCallback/);
  // And the manual button still both opens and closes.
  assert.match(screen, /api\.godotEmbedStop\("workspace"\)/);
  assert.match(screen, /workspaceOpen \? "Close workspace" : "Workspace"/);
});
