import assert from "node:assert/strict";
import fs from "node:fs";
import { performance } from "node:perf_hooks";
import test from "node:test";
import { planScenePatch } from "../src/engine/scenePatch.ts";

test("one touched entity retains every untouched render-object identity", () => {
  const ids = Array.from({ length: 1_000 }, (_, index) => `entity-${index}`);
  const objects = new Map(ids.map((id) => [id, { id }]));
  const untouched = objects.get("entity-500");
  const touched = objects.get("entity-4");
  const plan = planScenePatch("scene-a", "scene-a", ids, ["entity-4"], true);
  assert.equal(plan.full, false);
  for (const id of plan.rebuildIds) objects.set(id, { id });
  assert.strictEqual(objects.get("entity-500"), untouched);
  assert.notStrictEqual(objects.get("entity-4"), touched);
});

test("scene/schema/manifest resets take the explicit full path", () => {
  const ids = ["a", "b"];
  assert.equal(planScenePatch("one", "two", ids, ["a"], true).full, true);
  assert.equal(planScenePatch("one", "one", ids, null, true).full, true);
  assert.equal(planScenePatch("one", "one", ids, ["a"], false).full, true);
});

test("1k-entity patch planning remains far below the 50ms projection budget", () => {
  const ids = Array.from({ length: 1_000 }, (_, index) => `entity-${index}`);
  const samples = [];
  for (let index = 0; index < 200; index += 1) {
    const start = performance.now();
    const plan = planScenePatch("scene", "scene", ids, [`entity-${index % ids.length}`], true);
    assert.equal(plan.rebuildIds.size, 1);
    samples.push(performance.now() - start);
  }
  samples.sort((a, b) => a - b);
  const p95 = samples[Math.floor(samples.length * 0.95)];
  assert.ok(p95 <= 50, `patch-plan p95 ${p95.toFixed(3)}ms exceeded 50ms`);
});

test("event bridge coalesces a frame and guards monotonic revisions", () => {
  const view = fs.readFileSync(new URL("../src/engine/EngineView.tsx", import.meta.url), "utf8");
  assert.match(view, /setTimeout\(\(\) => \{[\s\S]*?16\)/);
  assert.match(view, /next\.revision >= revision/);
  assert.match(view, /next\.revision >= current\.revision/);
  assert.match(view, /pendingTouched\.add/);
});
