import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { multiFieldState } from "../src/engine/multiEdit.ts";

const entity = (id, components) => ({ id, name: id, parent: null, tags: [], components });

test("multi-edit distinguishes common, mixed and unavailable fields", () => {
  const common = [
    entity("a", { Light: { intensity: 1 } }),
    entity("b", { Light: { intensity: 1 } }),
  ];
  assert.deepEqual(multiFieldState(common, "Light", "intensity"), { kind: "common", value: 1 });
  assert.deepEqual(
    multiFieldState([common[0], entity("b", { Light: { intensity: 2 } })], "Light", "intensity"),
    { kind: "mixed" },
  );
  assert.deepEqual(
    multiFieldState([common[0], entity("b", { Transform: {} })], "Light", "intensity"),
    { kind: "unavailable" },
  );
});

test("Details only writes shared fields through the atomic batch and resets from schema", () => {
  const inspector = fs.readFileSync(new URL("../src/engine/EngineInspector.tsx", import.meta.url), "utf8");
  const view = fs.readFileSync(new URL("../src/engine/EngineView.tsx", import.meta.url), "utf8");
  assert.match(inspector, /disabled=\{!shared\}/);
  assert.match(inspector, /field\.default_value/);
  assert.match(view, /engineApplyBatch/);
  assert.match(view, /eligible\.map\(\(entity\) => \(\{ kind: "patch_component"/);

  const batch = fs.readFileSync(
    new URL("../../crates/bhippi-app/tests/engine_batches.rs", import.meta.url),
    "utf8",
  );
  assert.match(batch, /fails halfway writes nothing at all/);
});
