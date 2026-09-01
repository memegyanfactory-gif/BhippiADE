import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const view = fs.readFileSync(new URL("../src/engine/EngineView.tsx", import.meta.url), "utf8");
const drawer = fs.readFileSync(new URL("../src/engine/EngineContentDrawer.tsx", import.meta.url), "utf8");
const css = fs.readFileSync(new URL("../src/styles/workbench.css", import.meta.url), "utf8");

test("the default engine shell has one mode rail and one contextual viewport toolbar", () => {
  assert.match(view, /collapsed: typeof parsed\?\.collapsed === "boolean" \? parsed\.collapsed : true/);
  assert.equal((view.match(/className="engine-mode-rail"/g) ?? []).length, 1);
  assert.equal((view.match(/className="engine-viewport-toolbar"/g) ?? []).length, 1);
  assert.match(view, /aria-label="Engine editor mode"/);
  assert.match(view, /aria-label="Viewport tools"/);
  assert.match(view, />Scene</);
  assert.match(view, />HUD</);
});

test("advanced controls remain reachable while legacy toolbar groups are not duplicated visually", () => {
  assert.match(css, /toolbar-section\.center > :not\(\.engine-transport-group\)/);
  assert.match(css, /toolbar-section\.right > :not\(\.engine-primary-add\):not\(\.engine-simplified-action\)/);
  assert.match(view, />Command palette <kbd>Ctrl Shift P<\/kbd>/);
  assert.match(view, />Quick open <kbd>Ctrl P<\/kbd>/);
  assert.match(view, /setViewportMaximized/);
  assert.match(view, /setEnginePermissionMode/);
  assert.match(view, /engineSetAgentCapability/);
});

test("play keeps Stop one-click and reveals advanced simulation controls only while running", () => {
  assert.match(css, /:not\(\.engine-play-options\)/);
  assert.match(view, /isPlaying \? \(\s*<div className="spawn-entity-wrap engine-play-options">/s);
  assert.match(view, /aria-label="Play options"/);
  assert.match(view, />Restart simulation</);
  assert.match(view, />Step one frame</);
  assert.match(view, />Break on script error</);
  assert.match(view, /setPlayOptionsOpen\(false\)/);
});

test("content, output and diagnostics share one collapsed-by-default bottom drawer", () => {
  assert.match(view, /useState<EngineDrawerTab>\(\(\) => readDrawerPreference\(projectPath\)\.tab\)/);
  assert.match(view, /e\.key\.toLowerCase\(\) === "j"/);
  assert.match(view, /bhippi\.engine\.drawer\.\$\{projectPath\}/);
  assert.match(view, /if \(level === "error"\) \{\s*setDrawerTab\("problems"\);\s*setIsDrawerCollapsed\(false\);/s);
  assert.equal((view.match(/<EngineContentDrawer/g) ?? []).length, 1);
  assert.doesNotMatch(view, /\{logOpen \? \(\s*<EngineOutputLog/s);
  for (const label of ["Content", "Output", "Problems", "AI Activity", "Game Debug", "Build Targets"]) {
    assert.match(drawer, new RegExp(`label: "${label}"`));
  }
  assert.match(drawer, /activeTab === "output" \? \(\s*outputLog/s);
});

test("the shell has explicit narrow-window degradation instead of toolbar scrolling", () => {
  assert.match(css, /@media \(max-width: 1200px\)/);
  assert.match(css, /@media \(max-width: 900px\)/);
  assert.doesNotMatch(css, /\.engine-toolbar[^}]*overflow-x:\s*(auto|scroll)/s);
  assert.match(css, /\.engine-viewport-stage \{ flex: 1; min-height: 0;/);
});
