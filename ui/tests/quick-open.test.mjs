import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { evictMissingRecent, rankQuickOpen } from "../src/engine/quickOpen.ts";

test("quick open ranks recent paths first and keeps all other items stable", () => {
  const items = ["a/path/Main.bscn.json", "b/path/Main.bscn.json", "assets/ui/Main.hud.json"]
    .map((key) => ({ key, value: key }));
  assert.deepEqual(
    rankQuickOpen(items, ["assets/ui/Main.hud.json", "b/path/Main.bscn.json"]),
    ["assets/ui/Main.hud.json", "b/path/Main.bscn.json", "a/path/Main.bscn.json"],
  );
});

test("duplicate names remain disambiguated by path and missing recents are evicted", () => {
  const existing = new Set(["a/path/Main.bscn.json", "b/path/Main.bscn.json"]);
  assert.deepEqual(
    evictMissingRecent(["missing/Main.bscn.json", "b/path/Main.bscn.json", "b/path/Main.bscn.json"], existing),
    ["b/path/Main.bscn.json"],
  );
});

test("quick open remains keyboard-only reachable and searches the disambiguating path", () => {
  const palette = fs.readFileSync(new URL("../src/engine/EngineCommandPalette.tsx", import.meta.url), "utf8");
  const view = fs.readFileSync(new URL("../src/engine/EngineView.tsx", import.meta.url), "utf8");
  assert.match(view, /!e\.shiftKey && e\.key\.toLowerCase\(\) === "p"/);
  assert.match(palette, /command\.hint \?\? ""/);
  assert.match(palette, /event\.key === "ArrowDown"/);
  assert.match(palette, /event\.key === "Enter"[\s\S]*?command\.run\(\)/);
});
