import * as THREE from "three";
import type { Shape } from "./playRuntime.ts";

/** Wire geometry for the exact resolved shape used by Play. Caller owns the result. */
export function colliderWireGeometry(shape: Shape): THREE.BufferGeometry {
  if (shape.kind === "heightfield") {
    const points: number[] = [];
    const point = (row: number, col: number): [number, number, number] => [
      -shape.half[0] + (shape.half[0] * 2 * col) / Math.max(1, shape.cols - 1),
      shape.heights[row * shape.cols + col] ?? 0,
      -shape.half[2] + (shape.half[2] * 2 * row) / Math.max(1, shape.rows - 1),
    ];
    const segment = (a: [number, number, number], b: [number, number, number]) => {
      points.push(...a, ...b);
    };
    for (let row = 0; row < shape.rows; row += 1) {
      for (let col = 0; col < shape.cols; col += 1) {
        if (col + 1 < shape.cols) segment(point(row, col), point(row, col + 1));
        if (row + 1 < shape.rows) segment(point(row, col), point(row + 1, col));
      }
    }
    return new THREE.BufferGeometry().setAttribute(
      "position",
      new THREE.Float32BufferAttribute(points, 3),
    );
  }
  const solid: THREE.BufferGeometry = shape.kind === "sphere"
    ? new THREE.SphereGeometry(shape.radius, 16, 10)
    : shape.kind === "capsule"
      ? new THREE.CapsuleGeometry(shape.radius, shape.half * 2, 8, 12)
      : new THREE.BoxGeometry(shape.half[0] * 2, shape.half[1] * 2, shape.half[2] * 2);
  const wire = new THREE.EdgesGeometry(solid);
  solid.dispose();
  return wire;
}
