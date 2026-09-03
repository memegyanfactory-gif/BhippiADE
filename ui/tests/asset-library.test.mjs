/**
 * SPA-101…103: the user's asset library.
 *
 * The owner's ask: click the asset area, add library folders, and the AI can use anything
 * in them. Rust owns the folders, the scan, the classification and the copy; the page owns
 * the picker, the search box and the buttons. These tests pin that split and the two
 * places the panel appears.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const read = (rel) => fs.readFileSync(path.join(here, "..", "src", rel), "utf8");
const panel = read("components/AssetLibraryPanel.tsx");
const assets = read("screens/Assets.tsx");
const dock = read("studio/StudioBottomDock.tsx");
const api = read("lib/api.ts");
const ipc = read("lib/ipc.ts");

test("the library is a Rust surface: five typed commands, generated bindings", () => {
  for (const command of [
    "assetLibraryList",
    "assetLibraryAdd",
    "assetLibraryRemove",
    "assetLibrarySearch",
    "assetLibraryImport",
  ]) {
    assert.ok(api.includes(`${command}:`), `${command} is in api.ts`);
    assert.ok(ipc.includes(`${command}:`), `${command} is generated`);
  }
  for (const shape of ["export type AssetLibraryView", "export type LibraryFolder", "export type LibraryAsset"]) {
    assert.ok(ipc.includes(shape), `${shape} is typed by Rust`);
  }
});

test("the panel classifies nothing and copies nothing itself", () => {
  assert.ok(!/\.(glb|gltf|fbx|png|wav)\b/.test(panel), "no extension table in the page");
  assert.ok(!panel.includes("copyFile"), "no page-side copy");
  assert.ok(!panel.includes("writeFile"), "no page-side sidecar");
  assert.ok(panel.includes("api.assetLibraryImport(project.path, asset.path, null)"), "the copy is Rust's");
  assert.ok(panel.includes(".assetLibrarySearch("), "the search is Rust's");
  assert.ok(panel.includes("{asset.licence ?? \"unknown\"}"), "a missing licence reads unknown, never blank");
});

test("folders are added through the native picker and removal never touches the folder", () => {
  assert.ok(panel.includes("directory: true"), "the picker asks for a folder");
  assert.ok(panel.includes("api.assetLibraryAdd(picked)"));
  assert.ok(panel.includes("api.assetLibraryRemove(path)"));
  assert.ok(panel.includes("the folder itself is untouched"), "the remove button says so");
});

test("the panel lives on the Assets screen and in the dock's Assets tab", () => {
  assert.ok(assets.includes("<AssetLibraryPanel project={project}"), "Assets screen renders it");
  assert.ok(dock.includes("<AssetLibraryPanel"), "the dock renders it");
  assert.ok(dock.includes('useState<"project" | "library">("project")'), "the dock has a scope switch");
  assert.ok(dock.includes(">\n                      Library\n") || dock.includes("Library\n"), "the switch names the library");
});
