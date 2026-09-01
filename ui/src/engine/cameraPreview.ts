export type CameraPreviewRect = { width: number; height: number; margin: number };

/** One geometry source for the WebGL scissor and its DOM frame. */
export function cameraPreviewRect(containerWidth: number, scale = 1): CameraPreviewRect {
  const safeWidth = Math.max(0, containerWidth);
  const safeScale = Math.min(1.4, Math.max(0.65, scale));
  const base = Math.min(280, Math.max(160, safeWidth * 0.3));
  const width = Math.max(96, Math.min(safeWidth - 32, base * safeScale));
  return { width, height: width * 9 / 16, margin: 16 };
}

export function shouldRenderCameraPreview(
  active: boolean,
  playing: boolean,
  minimized: boolean,
  hasCamera: boolean,
): boolean {
  return active && !playing && !minimized && hasCamera;
}

export type AuthoredCamera = {
  fov?: number;
  near?: number;
  far?: number;
  orthographic?: boolean;
  orthographic_size?: number;
};

/** Build a disposable view camera from authored data and an existing scene object. */
export function cameraFromEntity(
  component: AuthoredCamera,
  source: THREE.Object3D,
  width: number,
  height: number,
): THREE.Camera {
  const aspect = Math.max(1, width) / Math.max(1, height);
  const camera: THREE.Camera = component.orthographic
    ? new THREE.OrthographicCamera(
      -(component.orthographic_size ?? 10) * aspect / 2,
      (component.orthographic_size ?? 10) * aspect / 2,
      (component.orthographic_size ?? 10) / 2,
      -(component.orthographic_size ?? 10) / 2,
      component.near ?? 0.05,
      component.far ?? 500,
    )
    : new THREE.PerspectiveCamera(
      THREE.MathUtils.radToDeg(component.fov ?? 0.9),
      aspect,
      component.near ?? 0.05,
      component.far ?? 500,
    );
  source.updateWorldMatrix(true, false);
  source.getWorldPosition(camera.position);
  source.getWorldQuaternion(camera.quaternion);
  camera.updateMatrixWorld(true);
  return camera;
}
import * as THREE from "three";
