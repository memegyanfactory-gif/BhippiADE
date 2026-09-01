import assert from "node:assert/strict";
import test from "node:test";
import { captureViewportCanvas } from "../src/engine/viewportCapture.ts";

function fixture() {
  const calls = [];
  const surface = {
    width: 0,
    height: 0,
    getContext(kind) {
      assert.equal(kind, "2d");
      return { drawImage: (source, x, y) => calls.push({ source, x, y }) };
    },
    toDataURL(kind) {
      assert.equal(kind, "image/png");
      return "data:image/png;base64,cG5n";
    },
  };
  return { surface, calls };
}

test("viewport capture preserves exact renderer dimensions with annotations off", () => {
  const { surface, calls } = fixture();
  const source = { width: 1280, height: 720 };
  const capture = captureViewportCanvas(source, () => surface);
  assert.deepEqual(capture, { imageBase64: "cG5n", width: 1280, height: 720 });
  assert.deepEqual(calls, [{ source, x: 0, y: 0 }]);
});

test("viewport annotations affect only the copied surface", () => {
  const { surface } = fixture();
  const source = { width: 640, height: 360, untouched: true };
  let annotated = false;
  const capture = captureViewportCanvas(source, () => surface, (copy) => {
    annotated = true;
    assert.equal(copy, surface);
  });
  assert.equal(annotated, true);
  assert.equal(source.untouched, true);
  assert.deepEqual([capture.width, capture.height], [640, 360]);
});

test("zero-sized or non-PNG capture surfaces fail loudly", () => {
  assert.throws(() => captureViewportCanvas({ width: 0, height: 10 }, () => fixture().surface));
  const { surface } = fixture();
  surface.toDataURL = () => "data:image/jpeg;base64,bm8=";
  assert.throws(() => captureViewportCanvas({ width: 1, height: 1 }, () => surface));
});
