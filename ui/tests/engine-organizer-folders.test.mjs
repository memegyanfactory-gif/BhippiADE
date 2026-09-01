import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const hierarchy = fs.readFileSync(new URL("../src/engine/EngineHierarchy.tsx", import.meta.url), "utf8");
const view = fs.readFileSync(new URL("../src/engine/EngineView.tsx", import.meta.url), "utf8");
const sceneTypes = fs.readFileSync(new URL("../src/engine/EngineSceneDocument.ts", import.meta.url), "utf8");

test("organiser folders are persisted scene metadata, not transform parents", () => {
  assert.match(sceneTypes, /editor: SceneEditorMetadata/);
  assert.match(sceneTypes, /entity_folders: Record<string, string>/);
  assert.match(view, /kind: "move_entity_to_organizer_folder"/);
  assert.match(view, /kind: "move_organizer_folder"/);
  assert.doesNotMatch(view, /move_entity_to_organizer_folder"[^}]*parent/s);
  assert.match(hierarchy, /title="Organiser folder — does not affect transforms"/);
});

test("folder controls are keyboard reachable and deletion explicitly flattens", () => {
  assert.match(hierarchy, />\s*New Folder\s*</);
  assert.match(hierarchy, /aria-label={`Rename \${folder\.name}`}/);
  assert.match(hierarchy, /event\.key === "Enter"/);
  assert.match(hierarchy, /event\.key === "Escape"/);
  assert.match(hierarchy, /Flatten folder \(keeps every entity\)/);
  assert.match(hierarchy, /onDeleteFolder\(folder\.id\)/);
});

test("dragging distinguishes transform reparenting from folder arrangement", () => {
  assert.match(hierarchy, /type DragItem = \{ kind: "entity" \| "folder"; id: string \}/);
  assert.match(hierarchy, /onMoveEntityToFolder\(dragging\.id, folder\.id\)/);
  assert.match(hierarchy, /onMoveFolder\(dragging\.id, folder\.id\)/);
  assert.match(hierarchy, /if \(entityFolders\[dragging\.id\]\) onMoveEntityToFolder\(dragging\.id, null\)/);
  assert.match(hierarchy, /else onReparent\(dragging\.id, null\)/);
});
