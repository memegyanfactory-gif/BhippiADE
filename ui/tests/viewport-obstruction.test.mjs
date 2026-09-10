/**
 * SPA-001: floating chrome over the Studio viewport hides the native Godot child.
 *
 * The owner's screenshot: the title bar's gear, update and organize buttons "did nothing"
 * in the engine. They worked — their dropdowns painted underneath the native window. The
 * registry below is the fix, and these tests pin both the registry and the wiring, because
 * the wiring is one line per surface and one line is easy to lose in a refactor.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
  isViewportObstructed,
  newObstructionToken,
  obstructViewport,
  obstructionCount,
  releaseViewport,
  resetObstructionForTests,
  subscribeObstruction,
} from "../src/lib/viewportObstruction.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const read = (rel) => fs.readFileSync(path.join(here, "..", "src", rel), "utf8");

test("the registry counts holders, not calls", () => {
  resetObstructionForTests();
  const a = newObstructionToken();
  const b = newObstructionToken();
  assert.equal(obstructionCount(), 0);
  obstructViewport(a);
  obstructViewport(a);
  assert.equal(obstructionCount(), 1, "a surface declared twice is one surface");
  obstructViewport(b);
  assert.equal(obstructionCount(), 2);
  releaseViewport(a);
  assert.ok(isViewportObstructed(), "the other surface still covers the viewport");
  releaseViewport(b);
  releaseViewport(b);
  assert.equal(obstructionCount(), 0, "releasing twice is not an error");
  assert.ok(!isViewportObstructed());
});

test("subscribers hear every transition and can leave", () => {
  resetObstructionForTests();
  const seen = [];
  const stop = subscribeObstruction((count) => seen.push(count));
  const token = newObstructionToken();
  obstructViewport(token);
  releaseViewport(token);
  stop();
  obstructViewport(token);
  assert.deepEqual(seen, [1, 0]);
  resetObstructionForTests();
});

test("every floating surface that can cross the viewport joins the registry", () => {
  for (const [file, state] of [
    ["chrome/AutoUpdateWidget.tsx", "useObstructsViewport(dropdownOpen)"],
    ["workspace/WorkspaceOrganizer.tsx", "useObstructsViewport(open)"],
    ["chrome/TitleBarCenterControls.tsx", "useObstructsViewport(modeMenuOpen)"],
  ]) {
    assert.ok(read(file).includes(state), `${file} declares its open state`);
  }
});

test("the Studio treats an open floating surface like a modal", () => {
  const studio = read("screens/StudioScreen.tsx");
  assert.ok(studio.includes("useViewportObstructed()"), "the studio reads the registry");
  assert.match(
    studio,
    /const obstructed = modalOpen \|\| gameSettingsOpen \|\| floatingOpen;/,
    "the native child hides while a surface is open",
  );
});
