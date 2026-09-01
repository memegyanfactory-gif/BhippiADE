import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import * as THREE from "three";
import { cameraFromEntity, cameraPreviewRect, shouldRenderCameraPreview } from "../src/engine/cameraPreview.ts";

test("camera preview keeps a bounded 16:9 scissor while resizing", () => {
  const normal = cameraPreviewRect(1000, 1);
  const larger = cameraPreviewRect(1000, 1.4);
  assert.equal(normal.width / normal.height, 16 / 9);
  assert.ok(larger.width > normal.width);
  assert.ok(cameraPreviewRect(220, 1.4).width <= 188);
});

test("camera preview uses the authored transform, FOV and aspect", () => {
  const parent = new THREE.Object3D();
  parent.position.set(10, 2, -3);
  const source = new THREE.Object3D();
  source.position.set(1, 4, 2);
  source.rotation.set(0.2, 0.4, 0.1);
  parent.add(source);
  const camera = cameraFromEntity({ fov: Math.PI / 3, near: 0.2, far: 900 }, source, 320, 180);
  assert.ok(camera instanceof THREE.PerspectiveCamera);
  assert.deepEqual(camera.position.toArray().map((value) => Number(value.toFixed(4))), [11, 6, -1]);
  assert.equal(Number(camera.fov.toFixed(4)), 60);
  assert.equal(camera.aspect, 16 / 9);
  assert.equal(camera.near, 0.2);
  assert.equal(camera.far, 900);
});

test("PiP reuses the viewport scene and its one resource cache", () => {
  const source = fs.readFileSync(new URL("../src/engine/EngineViewport.tsx", import.meta.url), "utf8");
  assert.equal((source.match(/new RenderResources\(\)/g) ?? []).length, 1);
  assert.match(source, /renderer\.render\(scene, previewCamera\)/);
});

test("hidden, minimized and play-mode previews stop rendering", () => {
  assert.equal(shouldRenderCameraPreview(true, false, false, true), true);
  assert.equal(shouldRenderCameraPreview(false, false, false, true), false);
  assert.equal(shouldRenderCameraPreview(true, false, true, true), false);
  assert.equal(shouldRenderCameraPreview(true, true, false, true), false);
  assert.equal(shouldRenderCameraPreview(true, false, false, false), false);
});
