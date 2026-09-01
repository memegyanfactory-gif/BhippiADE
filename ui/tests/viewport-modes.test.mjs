import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

test("viewport exposes every authored camera, shading, debug and resolution mode", () => {
  const view = fs.readFileSync(new URL("../src/engine/EngineView.tsx", import.meta.url), "utf8");
  for (const mode of ["perspective", "top", "bottom", "front", "back", "left", "right"]) {
    assert.match(view, new RegExp(`value=\\"${mode}\\"`));
  }
  for (const mode of ["lit", "unlit", "wireframe", "detail_lighting", "lighting_only", "collision"]) {
    assert.match(view, new RegExp(`\\"${mode}\\"`));
  }
  assert.match(view, /screenPercentage/);
  assert.match(view, /viewportMaximized/);

  const viewport = fs.readFileSync(new URL("../src/engine/EngineViewport.tsx", import.meta.url), "utf8");
  assert.match(viewport, /renderer\.setPixelRatio\(ratio\)/);
  assert.match(viewport, /scene\.overrideMaterial/);
  assert.match(viewport, /camera\.fov/);
});
