import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import { colliderWireGeometry } from "../src/engine/colliderDebug.ts";
import { isRecognizedCollider, shapeOf } from "../src/engine/playRuntime.ts";

test("debug drawing and Play resolve collider shapes through the same function", () => {
  const capsule = shapeOf({ shape: { capsule: [0.4, 2] } }, [9, 9, 9]);
  assert.deepEqual(capsule, { kind: "capsule", radius: 0.4, half: 0.6 });
  const heightfield = shapeOf({ shape: { heightfield: { rows: 2, cols: 2, heights: [0, 1, 2, 3] } } }, [4, 2, 6]);
  assert.equal(heightfield.kind, "heightfield");
  const heightWire = colliderWireGeometry(heightfield);
  assert.equal(heightWire.getAttribute("position").count, 8, "four grid edges, two endpoints each");
  heightWire.dispose();
  assert.equal(isRecognizedCollider({ shape: { capsule: [0.4, 2] } }), true);
  assert.equal(isRecognizedCollider({ shape: { mystery: [1, 2, 3] } }), false);

  const source = fs.readFileSync(new URL("../src/engine/EngineViewport.tsx", import.meta.url), "utf8");
  assert.match(source, /const shape = shapeOf\(collider, scale\)/);
  assert.match(source, /__colliderDebug/);
  assert.match(source, /__bounds/);
});
