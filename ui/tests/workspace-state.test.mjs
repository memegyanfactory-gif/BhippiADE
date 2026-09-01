import assert from "node:assert/strict";
import test from "node:test";

import {
  reconcileSessionOrder,
  resolveWorkspaceProvider,
} from "../src/workspace/workspaceState.ts";

test("session activity cannot reorder existing panels", () => {
  const current = ["chat-a", "chat-b", "chat-c"];
  const next = reconcileSessionOrder(current, ["chat-c", "chat-a", "chat-b"]);
  assert.strictEqual(next, current);
});

test("session reconciliation removes closed panels and appends new panels", () => {
  assert.deepEqual(
    reconcileSessionOrder(["chat-a", "chat-b", "closed"], ["new", "chat-b", "chat-a"]),
    ["chat-a", "chat-b", "new"],
  );
});

test("workspace provider stays shared until it becomes unavailable", () => {
  assert.equal(resolveWorkspaceProvider("codex", ["claude", "codex"], ["claude"]), "codex");
  assert.equal(resolveWorkspaceProvider("removed", ["claude", "codex"], ["codex"]), "codex");
  assert.equal(resolveWorkspaceProvider(null, ["claude", "codex"], []), "claude");
});
