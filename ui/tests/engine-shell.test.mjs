import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const view = fs.readFileSync(new URL("../src/engine/EngineView.tsx", import.meta.url), "utf8");
const css = fs.readFileSync(new URL("../src/styles/workbench.css", import.meta.url), "utf8");

test("the default engine shell has one mode rail and one contextual viewport toolbar", () => {
  assert.match(view, /const \[isDrawerCollapsed, setIsDrawerCollapsed\] = useState\(true\)/);
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

test("the shell has explicit narrow-window degradation instead of toolbar scrolling", () => {
  assert.match(css, /@media \(max-width: 1200px\)/);
  assert.match(css, /@media \(max-width: 900px\)/);
  assert.doesNotMatch(css, /\.engine-toolbar[^}]*overflow-x:\s*(auto|scroll)/s);
  assert.match(css, /\.engine-viewport-stage \{ flex: 1; min-height: 0;/);
});
