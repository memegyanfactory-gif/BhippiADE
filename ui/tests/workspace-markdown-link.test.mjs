import assert from "node:assert/strict";
import test from "node:test";
import { workspaceMarkdownTarget } from "../src/components/workspaceMarkdownLink.ts";

test("game-debug workspace links resolve to an exact in-project line", () => {
  assert.deepEqual(
    workspaceMarkdownTarget(
      "#bhippi-file=assets/scenes/main.bscn.json&line=42",
      "C:\\Games\\Warehouse",
    ),
    { path: "C:\\Games\\Warehouse/assets/scenes/main.bscn.json", line: 42 },
  );
});

test("workspace links reject absolute traversal malformed and unrelated links", () => {
  const root = "C:\\Games\\Warehouse";
  for (const href of [
    "https://example.com",
    "#bhippi-file=../secret.txt&line=1",
    "#bhippi-file=C%3A%2Fsecret.txt&line=1",
    "#bhippi-file=%2Fetc%2Fpasswd&line=1",
    "#bhippi-file=assets%2Fmain.json&line=0",
    "#bhippi-file=assets%2Fmain.json&line=NaN",
  ]) {
    assert.equal(workspaceMarkdownTarget(href, root), null, href);
  }
});
