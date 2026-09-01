import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

test("HUD Outliner exposes pointer and keyboard tree moves with visible focus/target state", () => {
  const editor = fs.readFileSync(new URL("../src/engine/EngineHudEditor.tsx", import.meta.url), "utf8");
  assert.match(editor, /drop-target/);
  assert.match(editor, /event\.altKey/);
  for (const key of ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"]) {
    assert.match(editor, new RegExp(key));
  }
  assert.match(editor, /reparent_widget/);
  assert.match(editor, /reorder_widget/);
  assert.match(editor, /refocusWidget/);
  assert.match(editor, /CSS\.escape/);
});
