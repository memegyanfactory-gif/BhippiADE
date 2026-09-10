/**
 * The embedded Godot viewport (ADR-0045).
 *
 * The viewport is a hole in the page with the real Godot window in it. What the page owns is
 * small and worth pinning: when a layout change is worth telling Rust about, that nothing in
 * the studio draws a scene of its own any more, that every Play and Workspace button goes
 * through the embed, and that nothing in the right column is positioned over the hole.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { hostVisible, roundBox, sameBox } from "../src/studio/viewportRect.ts";

const root = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, root), "utf8");

// ── the box ────────────────────────────────────────────────────────────────────

test("the box is rounded edge by edge, so the far edge never drifts from the next pane", () => {
  // 239.6 + 1000.9 = 1240.5 → 1241: the far edge is rounded on its own, not the width.
  const box = roundBox({ left: 239.6, top: 40.4, width: 1000.9, height: 700.3 });
  assert.deepEqual(box, { x: 240, y: 40, width: 1001, height: 701 });
});

test("a sub-pixel jitter is the same box and not a layout call", () => {
  const a = roundBox({ left: 240.2, top: 40.1, width: 800.3, height: 600.2 });
  const b = roundBox({ left: 239.8, top: 39.9, width: 800.8, height: 600.3 });
  assert.ok(sameBox(a, b));
  assert.ok(!sameBox(null, b));
  assert.ok(!sameBox(a, { ...b, width: b.width + 1 }));
});

test("the native window is shown only for a real, uncovered box", () => {
  const box = { x: 10, y: 10, width: 640, height: 480 };
  assert.equal(hostVisible(box, false), true);
  assert.equal(hostVisible(box, true), false);
  assert.equal(hostVisible({ ...box, width: 0 }, false), false);
  assert.equal(hostVisible({ ...box, height: 0 }, false), false);
});

// ── the page draws no scene of its own ─────────────────────────────────────────

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else if (/\.(ts|tsx)$/.test(entry.name)) out.push(full);
  }
  return out;
}

test("the studio mounts the Godot viewport and nothing in ui/src imports a renderer", () => {
  const screen = read("src/screens/StudioScreen.tsx");
  assert.match(screen, /import \{ GodotViewport \} from "\.\.\/studio\/GodotViewport"/);
  assert.doesNotMatch(screen, /Studio3DViewport/);
  assert.equal(fs.existsSync(new URL("src/studio/Studio3DViewport.tsx", root)), false);

  const offenders = walk(fileURLToPath(new URL("src/", root))).filter((file) =>
    /from\s+["']three["'/]/.test(fs.readFileSync(file, "utf8")),
  );
  assert.deepEqual(offenders, []);

  const pkg = JSON.parse(read("package.json"));
  assert.equal(pkg.dependencies.three, undefined);
  assert.equal(pkg.devDependencies["@types/three"], undefined);
});

// ── every Godot surface goes through the embed ─────────────────────────────────

test("Play and Workspace go through the embedded viewport, never a window of their own", () => {
  const screen = read("src/screens/StudioScreen.tsx");
  assert.match(screen, /api\.godotEmbedPlay\(projectPath\)/);
  assert.match(screen, /api\.godotEmbedStop\("game"\)/);
  assert.match(screen, /api\.godotEmbedOpenWorkspace\(projectPath\)/);
  assert.doesNotMatch(screen, /api\.godotRun\(|api\.godotOpenEditor\(/);
});

test("leaving the studio or opening a modal hides the native window", () => {
  const viewport = read("src/studio/GodotViewport.tsx");
  assert.match(viewport, /api\.godotEmbedLayout\(last\.box, false\)/);
  assert.match(viewport, /hostVisible\(box, obstructedRef\.current\)/);
  const screen = read("src/screens/StudioScreen.tsx");
  assert.match(screen, /obstructed=\{obstructed\}/);
  const app = read("src/App.tsx");
  assert.match(app, /<StudioScreen\s+modalOpen=\{Boolean\(/);
});

// ── nothing stands over the hole ───────────────────────────────────────────────

function block(css, selector) {
  const start = css.indexOf(`${selector} {`);
  assert.notEqual(start, -1, `${selector} is styled`);
  return css.slice(start, css.indexOf("}", start));
}

test("the dock and its drawer are in flow beside the viewport, not positioned over it", () => {
  const css = read("src/styles/studio.css");
  for (const selector of [".studio-bottom-dock", ".studio-drawer"]) {
    assert.doesNotMatch(block(css, selector), /position:\s*absolute/, `${selector} is in flow`);
  }
  assert.match(block(css, ".godot-viewport"), /position:\s*absolute;\s*inset:\s*0/);
  assert.doesNotMatch(css, /\.studio-restore-chat-pill/);
});
