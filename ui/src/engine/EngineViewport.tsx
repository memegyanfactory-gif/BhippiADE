import { useCallback, useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { TransformControls } from "three/examples/jsm/controls/TransformControls.js";
import { ViewHelper } from "three/examples/jsm/helpers/ViewHelper.js";
import type { SceneTransform, WeatherId, WeatherPreset } from "./EngineSceneDocument";
import type { HudWidgetView, RenderManifest, RuntimeBudgets } from "../lib/ipc";
import { api, events } from "../lib/api";
import { RenderResources } from "./renderResources";
import { captureViewportCanvas, type CaptureSurface } from "./viewportCapture.ts";
import { cameraFromEntity, cameraPreviewRect, shouldRenderCameraPreview } from "./cameraPreview.ts";
import { colliderWireGeometry } from "./colliderDebug.ts";
import { planScenePatch } from "./scenePatch.ts";
import {
  isRecognizedCollider,
  shapeOf,
  type InputDocument,
  type RuntimeCapability,
  type RuntimeEvent,
  type RuntimeStats,
} from "./playRuntime.ts";
import type { ScriptProgram } from "./scriptVm.ts";
import {
  RuntimeWorkerClient,
  RuntimeWorkerReportedError,
} from "./runtimeWorkerClient.ts";

/// The HUD overlay Play draws. Rectangles arrive already resolved into reference-resolution
/// pixels by `hud_action::resolve_rect`, so the viewport scales them and nothing more —
/// anchor maths lives in the engine, in one place (INV-073).
export type PlayHud = {
  widgets: HudWidgetView[];
  reference: [number, number];
  document: string;
};

export type PlayControls = {
  paused: boolean;
  stepToken: number;
  restartToken: number;
  timeScale: number;
  gameView: boolean;
  ejected: boolean;
  gravity: [number, number, number];
  input: InputDocument;
  /// Gameplay programs compiled by `bhippi-engine::script`, keyed by entity id (ADR-0030).
  scripts: Map<string, ScriptProgram>;
  scriptPaths: Map<string, string>;
  runtimeCapabilities: RuntimeCapability[];
  runtimeBudgets: RuntimeBudgets;
  /// Stop the sim on the frame a script faults, so the broken frame is the one on screen.
  pauseOnError: boolean;
  onTogglePause: () => void;
  onStats: (stats: RuntimeStats & { drawCalls: number }) => void;
  onEvent: (event: RuntimeEvent) => void;
  onStop: () => void;
  onLoadLevel: (level: string) => void;
};

type HudRuntimeAction = {
  action: string;
  level?: string;
  name?: string;
  value?: string;
};

type Vec3 = [number, number, number];
type Quat = [number, number, number, number];

export type SceneEntity = {
  id: string;
  name: string;
  parent: string | null;
  tags: string[];
  components: Record<string, any>;
};

export type SceneDoc = {
  format: string;
  id: string;
  name: string;
  settings: { ambient: Vec3; skybox: string | null };
  entities: SceneEntity[];
};

export function EngineViewport({
  doc,
  selectedId,
  onSelect,
  onTransform,
  wireframe = false,
  shadingMode = "lit",
  isPlaying = false,
  cameraMode = "perspective",
  gizmoMode = "translate",
  gizmoSpace = "world",
  snap = 1,
  empty = false,
  weather = "clear",
  weatherPresets = [],
  manifest = null,
  showFlags = { grid: true, icons: true, bounds: false, colliders: false },
  cameraSpeed = 1,
  fov = 58,
  screenPercentage = 100,
  hud = null,
  playControls = null,
  active = true,
  touchedIds = null,
  onDropAsset,
}: {
  doc: SceneDoc | null;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onTransform?: (id: string, transform: SceneTransform) => void;
  wireframe?: boolean;
  shadingMode?: "lit" | "unlit" | "wireframe" | "detail_lighting" | "lighting_only" | "collision";
  isPlaying?: boolean;
  cameraMode?: "perspective" | "top" | "bottom" | "front" | "back" | "left" | "right";
  gizmoMode?: "select" | "translate" | "rotate" | "scale";
  gizmoSpace?: "world" | "local";
  snap?: number | null;
  empty?: boolean;
  weather?: WeatherId | string;
  /** Presets come from `bhippi-engine::weather` — the viewport keeps no copy (INV-073). */
  weatherPresets?: WeatherPreset[];
  /** Meshes and materials resolved by `engine_render_manifest` (ENG-160/162). */
  manifest?: RenderManifest | null;
  /** Viewport Show flags (ENG-144). */
  showFlags?: { grid: boolean; icons: boolean; bounds: boolean; colliders: boolean };
  /** Fly-camera speed multiplier. */
  cameraSpeed?: number;
  fov?: number;
  screenPercentage?: number;
  hud?: PlayHud | null;
  playControls?: PlayControls | null;
  active?: boolean;
  /** Entities changed by the transaction that produced `doc`; null means a full scene reset. */
  touchedIds?: readonly string[] | null;
  onDropAsset?: (path: string) => void;
}) {
  const mountRef = useRef<HTMLDivElement | null>(null);
  const sceneRef = useRef<THREE.Scene | null>(null);
  const rendererRef = useRef<THREE.WebGLRenderer | null>(null);
  const cameraRef = useRef<THREE.PerspectiveCamera | null>(null);
  const controlsRef = useRef<OrbitControls | null>(null);
  const groupRef = useRef<THREE.Group | null>(null);
  const rayRef = useRef<{ meshById: Map<string, THREE.Object3D> } | null>(null);
  const docRef = useRef<SceneDoc | null>(doc);
  const selectedRef = useRef(selectedId);
  const isPlayingRef = useRef(isPlaying);
  const activeRef = useRef(active);
  const onDropAssetRef = useRef(onDropAsset);
  const onTransformRef = useRef(onTransform);
  const gizmoModeRef = useRef(gizmoMode);
  const gizmoSpaceRef = useRef(gizmoSpace);
  const snapRef = useRef(snap);
  const showFlagsRef = useRef(showFlags);
  const cameraSpeedRef = useRef(cameraSpeed);
  const viewHelperRef = useRef<ViewHelper | null>(null);
  const transformRef = useRef<TransformControls | null>(null);
  const gizmoDraggingRef = useRef(false);
  const sceneIdRef = useRef<string | null>(null);
  const rmbRef = useRef(false);
  const flyKeysRef = useRef<Record<string, boolean>>({});
  const clockRef = useRef(new THREE.Clock());
  const outlineRef = useRef<THREE.BoxHelper | null>(null);
  const boundsHelpersRef = useRef<THREE.BoxHelper[]>([]);
  const reportedCollidersRef = useRef(new Set<string>());
  const shadingMaterialsRef = useRef<Record<string, THREE.Material> | null>(null);
  if (!shadingMaterialsRef.current) {
    shadingMaterialsRef.current = {
      unlit: new THREE.MeshBasicMaterial({ color: 0xd7dbe2 }),
      wireframe: new THREE.MeshBasicMaterial({ color: 0x8fb6ff, wireframe: true }),
      detail_lighting: new THREE.MeshNormalMaterial(),
      lighting_only: new THREE.MeshLambertMaterial({ color: 0xffffff }),
      collision: new THREE.MeshBasicMaterial({ color: 0x273142, wireframe: true }),
    };
  }
  const previewCameraRef = useRef<THREE.Camera | null>(null);
  const previewEntityIdRef = useRef<string | null>(null);
  /// Meshes, materials and textures resolved by the engine (ENG-160/162). One cache for the
  /// life of the pane, so a hundred crates are one geometry and one material.
  const resourcesRef = useRef(new RenderResources());
  const [manifestRevision, setManifestRevision] = useState(0);
  const builtManifestRevisionRef = useRef(-1);
  const runtimeRef = useRef<RuntimeWorkerClient | null>(null);
  const playControlsRef = useRef(playControls);
  const lastStepRef = useRef(0);
  const lastRestartRef = useRef(0);
  const lastStatsAtRef = useRef(0);
  const [runtimeVariables, setRuntimeVariables] = useState<Readonly<Record<string, string | number | boolean>>>({});
  const [viewportWidth, setViewportWidth] = useState(0);
  const [previewRequested, setPreviewRequested] = useState(false);
  const [previewMinimized, setPreviewMinimized] = useState(false);
  const [previewScale, setPreviewScale] = useState(1);
  const [previewHasCamera, setPreviewHasCamera] = useState(false);
  const previewMinimizedRef = useRef(false);
  const previewScaleRef = useRef(1);

  useEffect(() => { docRef.current = doc; }, [doc]);
  useEffect(() => { selectedRef.current = selectedId; }, [selectedId]);
  useEffect(() => { isPlayingRef.current = isPlaying; }, [isPlaying]);
  useEffect(() => { activeRef.current = active; }, [active]);
  useEffect(() => { onDropAssetRef.current = onDropAsset; }, [onDropAsset]);
  useEffect(() => { onTransformRef.current = onTransform; }, [onTransform]);
  useEffect(() => { gizmoModeRef.current = gizmoMode; }, [gizmoMode]);
  useEffect(() => { gizmoSpaceRef.current = gizmoSpace; }, [gizmoSpace]);
  useEffect(() => { snapRef.current = snap; }, [snap]);
  useEffect(() => { showFlagsRef.current = showFlags; }, [showFlags]);
  useEffect(() => { cameraSpeedRef.current = cameraSpeed; }, [cameraSpeed]);
  useEffect(() => { playControlsRef.current = playControls; }, [playControls]);
  useEffect(() => { previewMinimizedRef.current = previewMinimized; }, [previewMinimized]);
  useEffect(() => { previewScaleRef.current = previewScale; }, [previewScale]);

  const canvasIdToMesh = useCallback(() => rayRef.current?.meshById ?? new Map(), []);

  // Adopting a manifest starts the texture and GLTF loads; each completed batch bumps a
  // revision so the entity pass re-runs and swaps the placeholders for the real thing.
  useEffect(() => {
    const resources = resourcesRef.current;
    resources.setReadyHandler(() => setManifestRevision((value) => value + 1));
    return () => resources.setReadyHandler(null);
  }, []);

  useEffect(() => {
    if (!manifest) return;
    resourcesRef.current.adopt(manifest);
    setManifestRevision((value) => value + 1);
  }, [manifest]);

  useEffect(() => {
    const resources = resourcesRef.current;
    return () => resources.dispose();
  }, []);

  // ENG-186: the model sees the exact renderer canvas, not a desktop crop. The request is
  // event-driven because ADR-0028 keeps rendering in the webview; only this one bounded PNG
  // crosses IPC. Optional annotations are drawn into the capture, never into the live scene.
  useEffect(() => {
    const unlisten = events.engineScreenshotRequested.listen((event) => {
      if (!activeRef.current) return;
      void (async () => {
        const renderer = rendererRef.current;
        const scene = sceneRef.current;
        const editorCamera = cameraRef.current;
        if (!renderer || !scene || !editorCamera) return;
        try {
          const camera = captureCameraForRequest(
            event.payload.camera,
            editorCamera,
            docRef.current,
            canvasIdToMesh(),
            renderer.domElement.width,
            renderer.domElement.height,
          );
          renderer.clear();
          renderer.render(scene, camera);
          const source = renderer.domElement;
          const capture = captureViewportCanvas(
            source,
            () => document.createElement("canvas") as CaptureSurface,
            event.payload.annotate
              ? (surface) => {
                const context = surface.getContext("2d") as CanvasRenderingContext2D | null;
                if (!context) throw new Error("The annotation surface is unavailable.");
                drawCaptureAnnotations(
                  context,
                  camera,
                  canvasIdToMesh(),
                  surface.width,
                  surface.height,
                );
              }
              : undefined,
          );
          await api.engineSubmitScreenshot(
            event.payload.request_id,
            capture.imageBase64,
            capture.width,
            capture.height,
          );
        } catch (error) {
          // Submitting an empty payload resolves the Rust waiter with a typed invalid-PNG
          // error instead of leaving the autonomy loop hanging until timeout.
          await api.engineSubmitScreenshot(
            event.payload.request_id,
            btoa(String(error)),
            0,
            0,
          ).catch(() => undefined);
        }
      })();
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, [canvasIdToMesh]);

  // init three once
  useEffect(() => {
    const mount = mountRef.current;
    if (!mount) return;
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x1a1d22);
    scene.fog = new THREE.Fog(0x1a1d22, 40, 120);
    sceneRef.current = scene;

    const camera = new THREE.PerspectiveCamera(58, 1, 0.1, 500);
    camera.position.set(8, 7, 12);
    camera.lookAt(0, 0, 0);
    cameraRef.current = camera;

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    renderer.autoClear = false;
    renderer.setPixelRatio(Math.min(window.devicePixelRatio ?? 1, 1.25));
    renderer.setSize(mount.clientWidth, mount.clientHeight);
    mount.appendChild(renderer.domElement);
    rendererRef.current = renderer;

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;
    controls.target.set(0, 0.5, 0);
    controls.minDistance = 1.5;
    controls.maxDistance = 80;
    // Unreal: LMB selects, RMB looks/orbits, MMB pans, wheel dollies.
    controls.mouseButtons = {
      LEFT: null as unknown as THREE.MOUSE,
      MIDDLE: THREE.MOUSE.PAN,
      RIGHT: THREE.MOUSE.ROTATE,
    };
    controlsRef.current = controls;

    const viewHelper = new ViewHelper(camera, renderer.domElement);
    viewHelper.setLabels("X", "Y", "Z");
    viewHelper.location.top = 10;
    viewHelper.location.right = 8;
    viewHelper.location.left = null;
    viewHelper.location.bottom = 0;
    viewHelperRef.current = viewHelper;

    const transform = new TransformControls(camera, renderer.domElement);
    transform.setSpace(gizmoSpaceRef.current);
    transform.setSize(0.85);
    if (snapRef.current && snapRef.current > 0) {
      transform.setTranslationSnap(snapRef.current);
      transform.setRotationSnap(THREE.MathUtils.degToRad(15));
      transform.setScaleSnap(snapRef.current);
    }
    transform.detach();
    scene.add(transform.getHelper());
    transformRef.current = transform;
    transform.addEventListener("dragging-changed", (event) => {
      const dragging = Boolean(event.value);
      gizmoDraggingRef.current = dragging;
      controls.enabled = !dragging;
    });
    transform.addEventListener("mouseUp", () => {
      const obj = transform.object as THREE.Object3D | undefined;
      const id = obj?.userData.__entityId as string | undefined;
      if (!id || !obj) return;
      // `obj.position` is already local to its parent (ENG-146), which is exactly what the
      // document stores — so a dragged child writes a local transform and its own children
      // follow it rather than being left behind.
      onTransformRef.current?.(id, {
        pos: [obj.position.x, obj.position.y, obj.position.z],
        rot: [obj.quaternion.x, obj.quaternion.y, obj.quaternion.z, obj.quaternion.w],
        scale: [obj.scale.x, obj.scale.y, obj.scale.z],
      });
    });

    // lights
    const amb = new THREE.AmbientLight(0xffffff, 0.55);
    amb.name = "__ambient";
    scene.add(amb);
    const hemi = new THREE.HemisphereLight(0xcfe4ff, 0x2a2f3a, 0.7);
    hemi.name = "__hemi";
    scene.add(hemi);
    const dir = new THREE.DirectionalLight(0xfff2d6, 2.2);
    dir.name = "__key";
    dir.position.set(10, 16, 8);
    dir.castShadow = true;
    dir.shadow.mapSize.set(1024, 1024);
    scene.add(dir);

    // grid + ground
    const grid = new THREE.GridHelper(80, 80, 0x3a4554, 0x232830);
    grid.name = "__grid";
    (grid.material as THREE.Material).transparent = true;
    (grid.material as THREE.Material).opacity = 0.72;
    scene.add(grid);
    const ground = new THREE.Mesh(
      new THREE.PlaneGeometry(40, 40),
      new THREE.MeshStandardMaterial({ color: 0x15181e, roughness: 0.92, transparent: true, opacity: 0.35 }),
    );
    ground.rotation.x = -Math.PI / 2;
    ground.position.y = 0;
    ground.receiveShadow = true;
    ground.name = "__ground";
    ground.visible = false;
    scene.add(ground);

    const group = new THREE.Group();
    group.name = "bscn";
    scene.add(group);
    groupRef.current = group;
    rayRef.current = { meshById: new Map() };

    const onResize = () => {
      if (!mount || !camera || !renderer) return;
      const w = mount.clientWidth, h = mount.clientHeight;
      if (w < 8 || h < 8) return;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
      setViewportWidth(w);
    };
    const ro = new ResizeObserver(onResize);
    ro.observe(mount);
    onResize();

    // click picking — Unreal: LMB select, ignore gizmo / axis widget hits
    const raycaster = new THREE.Raycaster();
    const pointer = new THREE.Vector2();
    const onPointerDown = (e: PointerEvent) => {
      if ((e.target as HTMLElement)?.closest?.("button")) return;
      if (e.button === 2) rmbRef.current = true;
      if (e.button !== 0) return;
      if (viewHelper.handleClick(e)) return;
      if (transform.axis || transform.dragging || gizmoDraggingRef.current) return;
      const rect = renderer.domElement.getBoundingClientRect();
      pointer.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
      pointer.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
      raycaster.setFromCamera(pointer, camera);
      const meshes = Array.from(rayRef.current?.meshById.values() ?? []);
      const hits = raycaster.intersectObjects(meshes, true);
      if (hits.length) {
        let o: THREE.Object3D | null = hits[0].object;
        while (o && !o.userData.__entityId) o = o.parent;
        if (o?.userData.__entityId) onSelect(o.userData.__entityId as string);
        else onSelect(null);
      } else {
        const groundHits = raycaster.intersectObject(ground);
        if (groundHits.length) onSelect(null);
      }
    };
    const onPointerUp = (e: PointerEvent) => {
      if (e.button === 2) rmbRef.current = false;
    };
    const onContextMenu = (e: Event) => e.preventDefault();
    renderer.domElement.addEventListener("pointerdown", onPointerDown);
    renderer.domElement.addEventListener("pointerup", onPointerUp);
    renderer.domElement.addEventListener("contextmenu", onContextMenu);

    const onFlyKey = (e: KeyboardEvent) => {
      const tag = (document.activeElement?.tagName || "").toLowerCase();
      if (tag === "input" || tag === "textarea") return;
      flyKeysRef.current[e.key.toLowerCase()] = e.type === "keydown";
    };
    window.addEventListener("keydown", onFlyKey);
    window.addEventListener("keyup", onFlyKey);

    let frame = 0;
    let running = true;
    const animate = () => {
      if (!running) return;
      frame = requestAnimationFrame(animate);
      if (document.hidden || !activeRef.current) return;
      const delta = clockRef.current.getDelta();
      if (rmbRef.current && !isPlayingRef.current) {
        const keys = flyKeysRef.current;
        // ENG-144: the toolbar's camera-speed slider scales the fly speed, the way UE5's does.
        const speed =
          (keys["shift"] ? 0.42 : 0.14) *
          (60 * Math.min(delta, 0.05)) *
          (cameraSpeedRef.current || 1);
        const before = camera.position.clone();
        const forward = new THREE.Vector3();
        camera.getWorldDirection(forward);
        const right = new THREE.Vector3().crossVectors(forward, camera.up).normalize();
        if (keys["w"]) camera.position.addScaledVector(forward, speed);
        if (keys["s"]) camera.position.addScaledVector(forward, -speed);
        if (keys["a"]) camera.position.addScaledVector(right, -speed);
        if (keys["d"]) camera.position.addScaledVector(right, speed);
        if (keys["e"]) camera.position.y += speed;
        if (keys["q"]) camera.position.y -= speed;
        controls.target.add(camera.position.clone().sub(before));
      }
      controls.update();
      for (const helper of boundsHelpersRef.current) helper.update();
      if (viewHelper.animating) viewHelper.update(delta);
      renderer.clear();
      renderer.render(scene, camera);
      viewHelper.render(renderer);

      // UE-style camera preview: selecting a Camera shows its actual authored view without
      // moving the editor camera or mutating the scene (ENG-166).
      const previewCamera = previewCameraRef.current;
      const previewSource = previewEntityIdRef.current
        ? rayRef.current?.meshById.get(previewEntityIdRef.current)
        : null;
      if (previewCamera && previewSource && shouldRenderCameraPreview(
        activeRef.current,
        isPlayingRef.current,
        previewMinimizedRef.current,
        true,
      )) {
        previewSource.updateWorldMatrix(true, false);
        previewSource.getWorldPosition(previewCamera.position);
        previewSource.getWorldQuaternion(previewCamera.quaternion);
        previewCamera.updateMatrixWorld(true);
        const { width, height, margin } = cameraPreviewRect(mount.clientWidth, previewScaleRef.current);
        const x = mount.clientWidth - width - margin;
        const y = margin;
        const wasVisible = previewSource.visible;
        previewSource.visible = false;
        renderer.setScissorTest(true);
        renderer.setViewport(x, y, width, height);
        renderer.setScissor(x, y, width, height);
        renderer.clear(true, true, true);
        renderer.render(scene, previewCamera);
        renderer.setScissorTest(false);
        renderer.setViewport(0, 0, mount.clientWidth, mount.clientHeight);
        previewSource.visible = wasVisible;
      }
    };
    animate();
    const onVis = () => {
      if (!document.hidden && running) renderer.render(scene, camera);
    };
    document.addEventListener("visibilitychange", onVis);

    return () => {
      running = false;
      cancelAnimationFrame(frame);
      document.removeEventListener("visibilitychange", onVis);
      window.removeEventListener("keydown", onFlyKey);
      window.removeEventListener("keyup", onFlyKey);
      ro.disconnect();
      renderer.domElement.removeEventListener("pointerdown", onPointerDown);
      renderer.domElement.removeEventListener("pointerup", onPointerUp);
      renderer.domElement.removeEventListener("contextmenu", onContextMenu);
      transform.dispose();
      controls.dispose();
      renderer.dispose();
      for (const material of Object.values(shadingMaterialsRef.current ?? {})) material.dispose();
      if (renderer.domElement.parentElement === mount) mount.removeChild(renderer.domElement);
    };
  }, [onSelect]);

  // ENG-144: the Show menu drives what the editor draws over the scene. These are helpers,
  // never gameplay — hiding the grid must not change what a build renders.
  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return;
    const grid = scene.getObjectByName("__grid");
    if (grid) grid.visible = showFlags.grid;
    for (const child of scene.children) {
      if (child.name === "__bounds") child.visible = showFlags.bounds;
    }
    const group = groupRef.current;
    if (group) {
      for (const child of group.children) {
        // Lights and cameras render as billboard-ish helper groups; `icons` hides them so
        // a screenshot of the level is not full of editor furniture.
        if (child.userData.__helper === true) child.visible = showFlags.icons;
      }
      group.traverse((child) => {
        if (child.userData.__colliderDebug === true) child.visible = showFlags.colliders || shadingMode === "collision";
      });
    }
  }, [showFlags, doc, shadingMode]);

  useEffect(() => {
    const scene = sceneRef.current;
    const ground = scene?.getObjectByName("__ground") as THREE.Mesh | undefined;
    if (!scene) return;
    const preset =
      weatherPresets.find((row) => row.id === weather) ?? weatherPresets[0] ?? null;
    // Before the presets arrive the viewport keeps its neutral editor sky rather than
    // guessing numbers it no longer owns.
    const sky = preset ? preset.sky : 0x1a1d22;
    const fog = preset ? preset.fog : 0;
    scene.background = new THREE.Color(sky);
    scene.fog = new THREE.Fog(sky, fog > 0.3 ? 8 : 28, fog > 0.3 ? 42 : 110);

    // ENG-164: the preset's own ambient and sun values drive the editor lights, so picking
    // "storm" darkens the scene rather than only repainting the backdrop. The numbers come
    // from `bhippi-engine::weather` — the viewport keeps no copy of them.
    if (preset) {
      const ambient = scene.getObjectByName("__ambient") as THREE.AmbientLight | undefined;
      if (ambient) {
        ambient.color.setRGB(preset.ambient[0], preset.ambient[1], preset.ambient[2]);
        // Ambient carries the sky's own brightness; the sun is a separate directional.
        ambient.intensity = 0.35 + preset.sun * 0.25;
      }
      const key = scene.getObjectByName("__key") as THREE.DirectionalLight | undefined;
      if (key) {
        key.intensity = Math.max(preset.sun, 0.05);
        key.color.setHex(preset.sky).lerp(new THREE.Color(0xfff2d6), 0.6);
      }
      const hemi = scene.getObjectByName("__hemi") as THREE.HemisphereLight | undefined;
      if (hemi) hemi.intensity = 0.25 + preset.sun * 0.2;
    }
    if (ground) ground.visible = !empty;
  }, [weather, weatherPresets, empty]);

  // Patch only the transaction's touched objects. Scene-open, renderer-manifest refresh and
  // schema resets pass null and take the deliberately slower full rebuild path (ENG-107).
  useEffect(() => {
    const group = groupRef.current;
    const scene = sceneRef.current;
    if (!group || !scene) return;
    if (gizmoDraggingRef.current) return;
    const patchPlan = planScenePatch(
      sceneIdRef.current,
      doc?.id ?? null,
      doc?.entities.map((entity) => entity.id) ?? [],
      touchedIds,
      builtManifestRevisionRef.current === manifestRevision,
    );
    const canPatch = !!doc && !patchPlan.full;
    builtManifestRevisionRef.current = manifestRevision;
    const rebuildIds = patchPlan.rebuildIds;

    // Bounds are lightweight editor helpers. Recreate them so parent/child world bounds are
    // exact after a parent moves, while retaining every untouched render object/resource.
    for (const helper of boundsHelpersRef.current) {
      helper.removeFromParent();
      helper.dispose();
    }
    boundsHelpersRef.current = [];
    const previous = rayRef.current?.meshById ?? new Map<string, THREE.Object3D>();
    if (!canPatch) {
      for (const id of previous.keys()) rebuildIds.add(id);
    }
    for (const id of rebuildIds) {
      const object = previous.get(id);
      if (!object) continue;
      object.traverse((child) => {
        if (child.userData.__colliderDebug !== true || !(child instanceof THREE.LineSegments)) return;
        child.geometry.dispose();
        const materials = Array.isArray(child.material) ? child.material : [child.material];
        for (const material of materials) material.dispose();
      });
      object.removeFromParent();
      previous.delete(id);
    }

    if (!doc) return;

    const byId = new Map<string, THREE.Object3D>(previous);

    if (sceneIdRef.current !== doc.id) {
      sceneIdRef.current = doc.id;
      const camEnt = doc.entities.find((e) => e.name === "MainCamera" || e.name === "GameCamera" || e.tags.includes("camera"));
      if (camEnt?.components?.Transform && cameraRef.current && controlsRef.current) {
        const p = camEnt.components.Transform.pos as Vec3;
        if (Array.isArray(p) && p.length === 3) {
          cameraRef.current.position.set(p[0], p[1], p[2]);
          controlsRef.current.target.set(0, 0.6, 0);
          controlsRef.current.update();
        }
      }
    }

    for (const ent of doc.entities.filter((entity) => rebuildIds.has(entity.id))) {
      const tf = ent.components.Transform as { pos: Vec3; rot: Quat; scale: Vec3 } | undefined;
      const pos: Vec3 = tf?.pos ?? [0, 0.5, 0];
      const scale: Vec3 = tf?.scale ?? [1, 1, 1];
      const meshComp = ent.components.MeshRenderer as
        | { mesh?: string; materials?: string[] }
        | undefined;
      const lightComp = ent.components.Light as any | undefined;
      const camComp = ent.components.Camera as any | undefined;

      let obj: THREE.Object3D;

      if (lightComp) {
        const kind = lightComp.kind ?? "point";
        // ENG-163: colour, intensity, range and cone angle all come from the component, so
        // the Details panel and the picture agree. They used to be partly hard-coded.
        const lightColor = new THREE.Color(
          lightComp.color?.[0] ?? 1,
          lightComp.color?.[1] ?? 0.98,
          lightComp.color?.[2] ?? 0.9,
        );
        if (kind === "directional") {
          const l = new THREE.DirectionalLight(lightColor, lightComp.intensity ?? 1.6);
          l.position.set(pos[0], pos[1], pos[2]);
          l.castShadow = true;
          l.shadow.mapSize.set(1024, 1024);
          obj = l;
          const helper = new THREE.Mesh(
            new THREE.SphereGeometry(0.14, 10, 10),
            new THREE.MeshBasicMaterial({ color: 0xffe9a3 }),
          );
          helper.position.set(0, 0, 0);
          l.add(helper);
        } else {
          // A spot is a point light with a cone. The schema has carried `outer_angle` since
          // the registry was written; the viewport ignored it and drew every non-directional
          // light as a bare point.
          const light =
            kind === "spot"
              ? new THREE.SpotLight(
                  lightColor,
                  lightComp.intensity ?? 1.2,
                  lightComp.range ?? 22,
                  lightComp.outer_angle ?? Math.PI / 6,
                  0.35,
                )
              : new THREE.PointLight(lightColor, lightComp.intensity ?? 1.2, lightComp.range ?? 22);
          light.position.set(0, 0, 0);
          light.castShadow = true;
          obj = new THREE.Group();
          obj.position.set(pos[0], pos[1], pos[2]);
          (obj as THREE.Group).add(light);
          if (light instanceof THREE.SpotLight) {
            // A spot needs a target in the graph, or Three aims it at the world origin.
            light.target.position.set(0, -1, 0);
            (obj as THREE.Group).add(light.target);
          }
          const bulb = new THREE.Mesh(
            new THREE.SphereGeometry(0.18, 12, 12),
            new THREE.MeshStandardMaterial({
              emissive: lightColor,
              emissiveIntensity: 1.4,
              color: 0x111111,
            }),
          );
          (obj as THREE.Group).add(bulb);
        }
      } else if (camComp) {
        // High-fidelity 3D Cinema / Studio Camera Model
        const camGroup = new THREE.Group();

        const bodyMat = new THREE.MeshStandardMaterial({
          color: 0x181a20,
          roughness: 0.35,
          metalness: 0.75,
        });
        const accentMat = new THREE.MeshStandardMaterial({
          color: 0xf0a02c,
          roughness: 0.2,
          metalness: 0.8,
        });
        const lensMat = new THREE.MeshStandardMaterial({
          color: 0x0a0c10,
          roughness: 0.15,
          metalness: 0.9,
        });
        const glassMat = new THREE.MeshPhysicalMaterial({
          color: 0x60a5fa,
          transmission: 0.85,
          roughness: 0.08,
          metalness: 0.05,
          ior: 1.52,
        });

        // 1. Main Chassis & Bevels
        const chassis = new THREE.Mesh(new THREE.BoxGeometry(0.48, 0.36, 0.65), bodyMat);
        chassis.castShadow = true;
        camGroup.add(chassis);

        const stripe = new THREE.Mesh(new THREE.BoxGeometry(0.49, 0.04, 0.4), accentMat);
        stripe.position.set(0, 0.1, 0.05);
        camGroup.add(stripe);

        // 2. Optical Cinema Lens (Multi-Stage Barrel + Glass)
        const lensBase = new THREE.Mesh(new THREE.CylinderGeometry(0.18, 0.19, 0.18, 24), lensMat);
        lensBase.rotation.x = Math.PI / 2;
        lensBase.position.set(0, 0, 0.41);
        camGroup.add(lensBase);

        const lensRing = new THREE.Mesh(new THREE.CylinderGeometry(0.16, 0.16, 0.15, 24), accentMat);
        lensRing.rotation.x = Math.PI / 2;
        lensRing.position.set(0, 0, 0.52);
        camGroup.add(lensRing);

        const lensFront = new THREE.Mesh(new THREE.CylinderGeometry(0.19, 0.16, 0.12, 24), lensMat);
        lensFront.rotation.x = Math.PI / 2;
        lensFront.position.set(0, 0, 0.64);
        camGroup.add(lensFront);

        const glassElement = new THREE.Mesh(new THREE.SphereGeometry(0.14, 16, 16, 0, Math.PI * 2, 0, Math.PI * 0.5), glassMat);
        glassElement.rotation.x = Math.PI / 2;
        glassElement.position.set(0, 0, 0.69);
        camGroup.add(glassElement);

        // 3. Top Cinema Rig Handle
        const handle = new THREE.Mesh(new THREE.BoxGeometry(0.1, 0.12, 0.42), lensMat);
        handle.position.set(0, 0.24, -0.05);
        camGroup.add(handle);

        // 4. Rear Monitor & Viewfinder Display
        const monitorFrame = new THREE.Mesh(new THREE.BoxGeometry(0.4, 0.26, 0.04), bodyMat);
        monitorFrame.position.set(0, 0, -0.34);
        camGroup.add(monitorFrame);

        const screenDisplay = new THREE.Mesh(
          new THREE.PlaneGeometry(0.34, 0.2),
          new THREE.MeshBasicMaterial({ color: 0x0284c7 })
        );
        screenDisplay.rotation.y = Math.PI;
        screenDisplay.position.set(0, 0, -0.365);
        camGroup.add(screenDisplay);

        // 5. Dual Top Sound/Film Magazines
        const mag1 = new THREE.Mesh(new THREE.CylinderGeometry(0.12, 0.12, 0.08, 16), bodyMat);
        mag1.position.set(-0.14, 0.26, -0.15);
        camGroup.add(mag1);

        const mag2 = new THREE.Mesh(new THREE.CylinderGeometry(0.12, 0.12, 0.08, 16), bodyMat);
        mag2.position.set(0.14, 0.26, -0.15);
        camGroup.add(mag2);

        // 6. Camera Frustum Line Guide (FOV projection)
        const fovLines = new THREE.LineSegments(
          new THREE.BufferGeometry().setFromPoints([
            new THREE.Vector3(0, 0, 0.7), new THREE.Vector3(-1.2, 0.8, 3.2),
            new THREE.Vector3(0, 0, 0.7), new THREE.Vector3(1.2, 0.8, 3.2),
            new THREE.Vector3(0, 0, 0.7), new THREE.Vector3(1.2, -0.8, 3.2),
            new THREE.Vector3(0, 0, 0.7), new THREE.Vector3(-1.2, -0.8, 3.2),
            new THREE.Vector3(-1.2, 0.8, 3.2), new THREE.Vector3(1.2, 0.8, 3.2),
            new THREE.Vector3(1.2, 0.8, 3.2), new THREE.Vector3(1.2, -0.8, 3.2),
            new THREE.Vector3(1.2, -0.8, 3.2), new THREE.Vector3(-1.2, -0.8, 3.2),
            new THREE.Vector3(-1.2, -0.8, 3.2), new THREE.Vector3(-1.2, 0.8, 3.2),
          ]),
          new THREE.LineBasicMaterial({ color: 0x38bdf8, transparent: true, opacity: 0.45 })
        );
        camGroup.add(fovLines);

        camGroup.position.set(pos[0], pos[1], pos[2]);
        obj = camGroup;
      } else {
        // ENG-160/162: the mesh and its material come from the engine's resolved render
        // manifest, not from sniffing the reference string. This branch used to guess:
        // an entity named "floor" got a grey box, a `.glb` got a *different* grey box, and
        // material maps were never applied at all — so the viewport and the document
        // disagreed about what the scene was (F8).
        const renderer = resourcesRef.current;
        const mesh = renderer.meshFor(meshComp?.mesh, meshComp?.materials);
        mesh.position.set(pos[0], pos[1], pos[2]);
        mesh.scale.set(scale[0], scale[1], scale[2]);
        if (tf?.rot) mesh.quaternion.set(tf.rot[0], tf.rot[1], tf.rot[2], tf.rot[3]);
        obj = mesh;
      }

      obj.userData.__entityId = ent.id;
      // Lights and cameras draw as editor furniture (a bulb model, a camera body, a
      // frustum). The Show > icons flag hides them so a screenshot of the level is the
      // level, not the editor.
      obj.userData.__helper = !!lightComp || !!camComp;
      obj.name = ent.name;
      obj.position.set(pos[0], pos[1], pos[2]);
      if (tf?.rot) obj.quaternion.set(tf.rot[0], tf.rot[1], tf.rot[2], tf.rot[3]);
      if (!(obj instanceof THREE.Mesh)) obj.scale.set(scale[0], scale[1], scale[2]);
      const collider = ent.components.Collider;
      if (collider) {
        const recognized = isRecognizedCollider(collider);
        const shape = shapeOf(collider, scale);
        const debug = new THREE.LineSegments(
          colliderWireGeometry(shape),
          new THREE.LineBasicMaterial({ color: recognized ? (collider.sensor ? 0x22d3ee : 0x65e572) : 0xff00cc }),
        );
        debug.userData.__colliderDebug = true;
        debug.visible = showFlagsRef.current.colliders;
        // The entity root already carries Transform.scale. Collider dimensions are resolved
        // in world units by `shapeOf`, so cancel that scale for this child while retaining
        // the entity's rotation and hierarchy.
        debug.scale.set(
          1 / Math.max(Math.abs(scale[0]), 0.0001),
          1 / Math.max(Math.abs(scale[1]), 0.0001),
          1 / Math.max(Math.abs(scale[2]), 0.0001),
        );
        obj.add(debug);
        if (!recognized && !reportedCollidersRef.current.has(ent.id)) {
          reportedCollidersRef.current.add(ent.id);
          void api.engineRecordConsole(
            "error",
            "physics",
            `${ent.name} has an unrecognized Collider shape; debug draw uses its bounds.`,
          ).catch(() => undefined);
        } else if (recognized) {
          reportedCollidersRef.current.delete(ent.id);
        }
      }
      // allow raycaster to find
      if (obj instanceof THREE.Mesh || obj instanceof THREE.Group || obj instanceof THREE.Light) {
        // store top-level for picking
        if (obj instanceof THREE.Mesh) rayRef.current?.meshById.set(ent.id, obj);
        else {
          // for lights/camera, pick the group itself
          rayRef.current?.meshById.set(ent.id, obj);
        }
      }
      byId.set(ent.id, obj);
    }

    // ENG-146 (F9): parent the objects to each other so a transform *accumulates*.
    //
    // These used to all be added flat to the scene group, with a comment saying the
    // hierarchy was "logical, not transform-accumulated". That is what made moving a parent
    // leave its children behind — and it is why prefabs, rigs and grouped level pieces
    // could not work. A scene's `Transform` is now local to its parent, exactly as the
    // stable-path addressing (`scene:/Parent/Child`) already implied.
    //
    // Cycles cannot occur (`SceneDocument::validate` rejects them), but an entity whose
    // parent failed to build still has to land somewhere, so it falls back to the root.
    for (const ent of doc.entities) {
      const obj = byId.get(ent.id);
      if (!obj) continue;
      const parent = ent.parent ? byId.get(ent.parent) : undefined;
      (parent ?? group).add(obj);
    }
    for (const ent of doc.entities) {
      const obj = byId.get(ent.id);
      if (!obj) continue;
      const bounds = new THREE.BoxHelper(obj, 0x60a5fa);
      bounds.name = "__bounds";
      bounds.visible = showFlagsRef.current.bounds;
      scene.add(bounds);
      boundsHelpersRef.current.push(bounds);
    }
  }, [doc, manifestRevision, touchedIds]);

  // Attach Unreal-style transform gizmo to the selected object (W/E/R).
  useEffect(() => {
    const tc = transformRef.current;
    if (!tc) return;
    if (isPlaying || gizmoMode === "select" || !selectedId) {
      tc.detach();
      return;
    }
    tc.enabled = true;
    tc.setMode(gizmoMode);
    const mesh = rayRef.current?.meshById.get(selectedId);
    if (mesh) tc.attach(mesh);
    else tc.detach();
  }, [selectedId, gizmoMode, isPlaying, doc]);

  useEffect(() => {
    const tc = transformRef.current;
    if (!tc) return;
    tc.setSpace(gizmoSpace);
  }, [gizmoSpace]);

  useEffect(() => {
    const tc = transformRef.current;
    if (!tc) return;
    if (snap == null || snap <= 0) {
      tc.setTranslationSnap(null);
      tc.setRotationSnap(null);
      tc.setScaleSnap(null);
    } else {
      tc.setTranslationSnap(snap);
      tc.setRotationSnap(THREE.MathUtils.degToRad(15));
      tc.setScaleSnap(snap);
    }
  }, [snap]);

  // Yellow selection box — scale is left to the gizmo, not bumped.
  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return;
    if (outlineRef.current) {
      scene.remove(outlineRef.current);
      outlineRef.current.dispose();
      outlineRef.current = null;
    }
    if (!selectedId) return;
    const mesh = canvasIdToMesh().get(selectedId);
    if (!mesh) return;
    const box = new THREE.BoxHelper(mesh, 0xf5c542);
    box.name = "__selection";
    scene.add(box);
    outlineRef.current = box;
  }, [selectedId, doc, canvasIdToMesh]);

  useEffect(() => {
    previewCameraRef.current = null;
    previewEntityIdRef.current = null;
    setPreviewHasCamera(false);
    if (!selectedId || !doc?.entities.find((entity) => entity.id === selectedId)?.components.Camera) {
      return;
    }
    setPreviewRequested(true);
    setPreviewMinimized(false);
    try {
      previewCameraRef.current = captureCameraForRequest(
        `entity:${selectedId}`,
        cameraRef.current ?? new THREE.PerspectiveCamera(),
        doc,
        canvasIdToMesh(),
        16,
        9,
      );
      previewEntityIdRef.current = selectedId;
      setPreviewHasCamera(true);
    } catch {
      // The entity rebuild and selection effects can cross for one render. The next state
      // push retries; a missing renderer object is not an authored scene error.
    }
  }, [selectedId, doc, canvasIdToMesh, manifestRevision]);

  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return;
    scene.overrideMaterial = shadingMode === "lit"
      ? null
      : shadingMaterialsRef.current?.[shadingMode] ?? null;
  }, [shadingMode]);

  useEffect(() => {
    const camera = cameraRef.current;
    if (!camera) return;
    camera.fov = Math.max(25, Math.min(110, fov));
    camera.updateProjectionMatrix();
  }, [fov]);

  useEffect(() => {
    const renderer = rendererRef.current;
    const mount = mountRef.current;
    if (!renderer || !mount) return;
    const ratio = Math.min(window.devicePixelRatio ?? 1, 1.25) * Math.max(0.5, Math.min(1, screenPercentage / 100));
    renderer.setPixelRatio(ratio);
    renderer.setSize(mount.clientWidth, mount.clientHeight);
  }, [screenPercentage]);

  useEffect(() => {
    const map = canvasIdToMesh();
    for (const mesh of map.values()) {
      if (!((mesh as THREE.Mesh).material instanceof THREE.MeshStandardMaterial)) continue;
      const m = (mesh as THREE.Mesh).material as THREE.MeshStandardMaterial;
      if (!mesh.userData.__hasBase) {
        mesh.userData.__baseEmissive = m.emissive.getHex();
        mesh.userData.__hasBase = true;
      }
      const selected = mesh.userData.__entityId === selectedId;
      m.emissive.setHex(selected ? 0x3a2a08 : (mesh.userData.__baseEmissive as number));
    }
  }, [selectedId, canvasIdToMesh]);

  // Wireframe toggle
  useEffect(() => {
    const group = groupRef.current;
    if (!group) return;
    group.traverse((child) => {
      if (child instanceof THREE.Mesh && child.material) {
        if (Array.isArray(child.material)) {
          child.material.forEach((m) => { m.wireframe = wireframe; });
        } else {
          child.material.wireframe = wireframe;
        }
      }
    });
  }, [wireframe]);

  // Camera preset view modes
  useEffect(() => {
    const cam = cameraRef.current;
    const ctrl = controlsRef.current;
    if (!cam || !ctrl) return;
    if (cameraMode === "top") {
      cam.position.set(0, 32, 0.001);
      ctrl.target.set(0, 0, 0);
    } else if (cameraMode === "bottom") {
      cam.position.set(0, -32, 0.001);
      ctrl.target.set(0, 0, 0);
    } else if (cameraMode === "front") {
      cam.position.set(0, 5, -22);
      ctrl.target.set(0, 2, 0);
    } else if (cameraMode === "back") {
      cam.position.set(0, 5, 22);
      ctrl.target.set(0, 2, 0);
    } else if (cameraMode === "left") {
      cam.position.set(-22, 5, 0);
      ctrl.target.set(0, 2, 0);
    } else if (cameraMode === "right") {
      cam.position.set(22, 5, 0);
      ctrl.target.set(0, 2, 0);
    } else {
      cam.position.set(8, 7, 12);
      ctrl.target.set(0, 0.5, 0);
    }
    ctrl.update();
  }, [cameraMode]);

  // "F" key to frame/focus selected entity
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const activeTag = (document.activeElement?.tagName || "").toLowerCase();
      if (activeTag === "input" || activeTag === "textarea") return;
      if (e.key === "f" || e.key === "F") {
        if (!selectedRef.current) return;
        const mesh = canvasIdToMesh().get(selectedRef.current);
        const cam = cameraRef.current;
        const ctrl = controlsRef.current;
        if (mesh && cam && ctrl) {
          const worldPos = new THREE.Vector3();
          mesh.getWorldPosition(worldPos);
          ctrl.target.copy(worldPos);
          cam.position.set(worldPos.x + 5, worldPos.y + 4, worldPos.z + 7);
          ctrl.update();
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [canvasIdToMesh]);

  // Deterministic gameplay simulation. The runtime owns a clone and applies positions only
  // to rendered objects; the authored document is never mutated (ENG-171).
  useEffect(() => {
    if (!isPlaying || !doc || !playControls) {
      runtimeRef.current?.terminate();
      runtimeRef.current = null;
      setRuntimeVariables({});
      return;
    }
    if (controlsRef.current) controlsRef.current.enabled = playControls.ejected;
    lastStepRef.current = playControls.stepToken;
    lastRestartRef.current = playControls.restartToken;
    const onKeyDown = (e: KeyboardEvent) => {
      const tag = (document.activeElement?.tagName || "").toLowerCase();
      if (tag === "input" || tag === "textarea") return;
      runtimeRef.current?.input(e.code, true);
      if (e.code === "Escape" && !e.repeat) playControlsRef.current?.onTogglePause();
    };
    const onKeyUp = (e: KeyboardEvent) => runtimeRef.current?.input(e.code, false);

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);

    const runtimeGroup = new THREE.Group();
    runtimeGroup.name = "__runtime";
    groupRef.current?.add(runtimeGroup);

    let cancelled = false;
    let animId = 0;
    let inFlight = false;
    let previous = performance.now();
    let lastPaused = playControls.paused;
    const loop = (now: number) => {
      animId = requestAnimationFrame(loop);
      const delta = Math.max(0, (now - previous) / 1000);
      previous = now;
      const controls = playControlsRef.current;
      const runtime = runtimeRef.current;
      if (!controls || !runtime || inFlight) return;
      if (controlsRef.current) controlsRef.current.enabled = controls.ejected;
      if (controls.paused !== lastPaused) {
        lastPaused = controls.paused;
        runtime.pause(controls.paused);
      }
      if (controls.restartToken !== lastRestartRef.current) {
        lastRestartRef.current = controls.restartToken;
        runtime.reset();
        runtime.pause(controls.paused);
      }
      const forceStep = controls.stepToken !== lastStepRef.current;
      if (forceStep) lastStepRef.current = controls.stepToken;
      inFlight = true;
      void runtime
        .tick(delta, controls.timeScale, forceStep)
        .then((frame) => {
          if (cancelled) return;
          const objects = canvasIdToMesh();

          // Runtime-only objects stay in the disposable render group and disappear on Stop.
          for (const entity of frame.spawned) {
            const meshRef = entity.components.MeshRenderer?.mesh;
            const spawned = resourcesRef.current.meshFor(meshRef, undefined);
            spawned.name = entity.name;
            spawned.userData.__entityId = entity.id;
            spawned.userData.__runtime = true;
            runtimeGroup.add(spawned);
            objects.set(entity.id, spawned);
          }
          for (const id of frame.removed) {
            const object = objects.get(id);
            if (object) {
              object.removeFromParent();
              objects.delete(id);
            }
          }
          for (const [id, position] of frame.transforms) {
            objects.get(id)?.position.set(position[0], position[1], position[2]);
          }
          for (const [id, rotation] of frame.rotations) {
            objects.get(id)?.rotation.set(rotation[0], rotation[1], rotation[2]);
          }
          for (const event of frame.events) controls.onEvent(event);
          if (now - lastStatsAtRef.current >= 250) {
            lastStatsAtRef.current = now;
            setRuntimeVariables(frame.variables);
            controls.onStats({
              ...frame.stats,
              drawCalls: rendererRef.current?.info.render.calls ?? 0,
            });
          }
        })
        .catch((error) => {
          if (!cancelled && !(error instanceof RuntimeWorkerReportedError)) {
            controls.onEvent({
              kind: "fault",
              message: error instanceof Error ? error.message : String(error),
              hint: "The disposable gameplay worker stopped. Restart Play after fixing the reported fault.",
            });
          }
        })
        .finally(() => {
          inFlight = false;
        });
    };

    void RuntimeWorkerClient.start({
      document: doc,
      gravity: playControls.gravity,
      input: playControls.input,
      programs: [...playControls.scripts].map(([entity, program]) => ({
        entity,
        path: playControls.scriptPaths.get(entity) ?? program.file,
        program,
      })),
      capabilities: playControls.runtimeCapabilities,
      budgets: playControls.runtimeBudgets,
      pauseOnError: playControls.pauseOnError,
      onFault: (message) =>
        playControls.onEvent({
          kind: "fault",
          message,
          hint: "The disposable gameplay worker stopped. Restart Play after fixing the reported fault.",
        }),
    })
      .then((runtime) => {
        if (cancelled) {
          runtime.terminate();
          return;
        }
        runtimeRef.current = runtime;
        runtime.pause(playControls.paused);
        animId = requestAnimationFrame(loop);
      })
      .catch((error) => {
        if (!cancelled && !(error instanceof RuntimeWorkerReportedError)) {
          playControls.onEvent({
            kind: "fault",
            message: error instanceof Error ? error.message : String(error),
            hint: "The gameplay worker could not start. Review its capability and budget report.",
          });
        }
      });

    return () => {
      cancelled = true;
      cancelAnimationFrame(animId);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      runtimeRef.current?.stop();
      runtimeRef.current = null;
      // Stop drops every runtime-spawned object with the group that held them, so the editor
      // shows the authored world and nothing else (INV-081).
      for (const child of [...runtimeGroup.children]) {
        canvasIdToMesh().delete(child.userData.__entityId);
      }
      runtimeGroup.removeFromParent();
      if (controlsRef.current) controlsRef.current.enabled = true;
    };
  }, [
    isPlaying,
    doc,
    playControls?.gravity,
    playControls?.input,
    playControls?.scripts,
    playControls?.scriptPaths,
    playControls?.runtimeCapabilities,
    playControls?.runtimeBudgets,
    playControls?.pauseOnError,
    canvasIdToMesh,
  ]);

  return (
    <div
      ref={mountRef}
      className={`engine-viewport-canvas${empty ? " is-empty" : ""}`}
      aria-label="3D viewport"
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("text/bhippi-asset")) e.preventDefault();
      }}
      onDrop={(e) => {
        const path = e.dataTransfer.getData("text/bhippi-asset");
        if (path) {
          e.preventDefault();
          onDropAssetRef.current?.(path);
        }
      }}
    >
      {/* Viewport HUD Overlays */}
      <div className={`viewport-hud-top${isPlaying && playControls?.gameView ? " game-view" : ""}`}>
        <div className="viewport-hud-left">
          <span className="hud-badge mode">
            {cameraMode.toUpperCase()} · {wireframe ? "WIREFRAME" : "LIT"}
          </span>
          <span className="hud-badge stats">
            {doc?.entities?.length ?? 0} Entities
          </span>
          <span className="hud-badge stats">
            {gizmoSpace.toUpperCase()} · SNAP {snap == null ? "OFF" : snap}
          </span>
          {isPlaying ? (
            <span className="hud-badge playing">
              <span className="hud-pulse-dot" />
              {playControls?.paused ? "PAUSED" : "PLAY MODE"} · mapped input
            </span>
          ) : null}
        </div>
        <div className="viewport-hud-right" aria-hidden="true" />
      </div>

      <div className={`viewport-hud-bottom${isPlaying && playControls?.gameView ? " game-view" : ""}`}>
        <span className="hud-hint">
          {empty
            ? "Empty level — New Game to scaffold Main, HUD, and Level 1"
            : "LMB Select · RMB Look + WASD fly · MMB Pan · W/E/R gizmo · X world/local · Ctrl+D duplicate · Del · F Focus"}
        </span>
      </div>

      {!isPlaying && previewRequested ? (
        <div
          className={`viewport-camera-preview-label${previewMinimized ? " minimized" : ""}${previewHasCamera ? "" : " no-camera"}`}
          aria-label="Selected camera preview"
          style={previewMinimized ? undefined : {
            width: cameraPreviewRect(viewportWidth, previewScale).width,
            height: cameraPreviewRect(viewportWidth, previewScale).height,
          }}
        >
          <span>
            {previewHasCamera
              ? `Camera Preview · ${doc?.entities.find((entity) => entity.id === selectedId)?.name ?? "Camera"}`
              : "Camera Preview · no active camera"}
          </span>
          <span className="viewport-camera-preview-controls">
            {!previewMinimized ? (
              <>
                <button type="button" aria-label="Make camera preview smaller" onClick={() => setPreviewScale((value) => Math.max(0.65, value - 0.15))}>−</button>
                <button type="button" aria-label="Make camera preview larger" onClick={() => setPreviewScale((value) => Math.min(1.4, value + 0.15))}>+</button>
              </>
            ) : null}
            <button type="button" aria-label={previewMinimized ? "Restore camera preview" : "Minimize camera preview"} onClick={() => setPreviewMinimized((value) => !value)}>
              {previewMinimized ? "□" : "_"}
            </button>
            <button type="button" aria-label="Close camera preview" onClick={() => setPreviewRequested(false)}>×</button>
          </span>
        </div>
      ) : null}

      {isPlaying && hud ? (
        <HudOverlay
          hud={hud}
          variables={runtimeVariables}
          onAction={(action) => {
            if (action.action === "pause_game" || action.action === "resume_game") {
              playControls?.onTogglePause();
            } else if (action.action === "stop_game" || action.action === "quit_to_main") {
              playControls?.onStop();
            } else if (action.action === "load_level" && action.level) {
              playControls?.onLoadLevel(action.level);
            } else if (action.action === "set_var" && action.name && action.value !== undefined) {
              runtimeRef.current?.setVariable(action.name, action.value);
            }
          }}
        />
      ) : null}
    </div>
  );
}

function drawCaptureAnnotations(
  context: CanvasRenderingContext2D,
  camera: THREE.Camera,
  objects: Map<string, THREE.Object3D>,
  width: number,
  height: number,
): void {
  context.save();
  context.font = "12px ui-monospace, monospace";
  context.lineWidth = 1.5;
  for (const object of objects.values()) {
    if (!object.visible || object.userData.__runtime) continue;
    const box = new THREE.Box3().setFromObject(object);
    if (box.isEmpty()) continue;
    const corners = [
      new THREE.Vector3(box.min.x, box.min.y, box.min.z),
      new THREE.Vector3(box.min.x, box.min.y, box.max.z),
      new THREE.Vector3(box.min.x, box.max.y, box.min.z),
      new THREE.Vector3(box.min.x, box.max.y, box.max.z),
      new THREE.Vector3(box.max.x, box.min.y, box.min.z),
      new THREE.Vector3(box.max.x, box.min.y, box.max.z),
      new THREE.Vector3(box.max.x, box.max.y, box.min.z),
      new THREE.Vector3(box.max.x, box.max.y, box.max.z),
    ].map((point) => point.project(camera));
    const xs = corners.map((point) => (point.x * 0.5 + 0.5) * width);
    const ys = corners.map((point) => (-point.y * 0.5 + 0.5) * height);
    const left = Math.max(0, Math.min(...xs));
    const right = Math.min(width, Math.max(...xs));
    const top = Math.max(0, Math.min(...ys));
    const bottom = Math.min(height, Math.max(...ys));
    if (right <= left || bottom <= top) continue;
    context.strokeStyle = "rgba(240,160,44,0.9)";
    context.strokeRect(left, top, right - left, bottom - top);
    const label = object.name || String(object.userData.__entityId ?? "entity");
    const labelWidth = context.measureText(label).width + 8;
    context.fillStyle = "rgba(13,16,22,0.88)";
    context.fillRect(left, Math.max(0, top - 18), labelWidth, 18);
    context.fillStyle = "#f6b54a";
    context.fillText(label, left + 4, Math.max(12, top - 5));
  }
  context.restore();
}

/** Resolve an observation camera without moving the live editor camera or scene. */
export function captureCameraForRequest(
  requested: string,
  editorCamera: THREE.PerspectiveCamera,
  doc: SceneDoc | null,
  objects: Map<string, THREE.Object3D>,
  width: number,
  height: number,
): THREE.Camera {
  if (requested === "editor") return editorCamera;
  if (!doc) throw new Error("No scene is open for that camera observation.");

  const entity = requested === "game"
    ? doc.entities.find((candidate) => candidate.components.Camera)
    : doc.entities.find((candidate) => candidate.id === requested.replace(/^entity:/, ""));
  if (!entity?.components.Camera) {
    throw new Error(
      requested === "game"
        ? "The scene has no game Camera entity."
        : `Camera entity ${requested.replace(/^entity:/, "")} was not found.`,
    );
  }
  const source = objects.get(entity.id);
  if (!source) throw new Error(`Camera entity ${entity.id} is not rendered in the active scene.`);

  return cameraFromEntity(entity.components.Camera, source, width, height);
}


/// Draws the HUD document over the viewport during play (ENG-138).
///
/// Widgets are positioned from the engine-resolved rects, scaled to whatever the viewport
/// happens to be, so what the HUD editor shows and what Play shows cannot disagree.
function HudOverlay({
  hud,
  variables,
  onAction,
}: {
  hud: PlayHud;
  variables: Readonly<Record<string, string | number | boolean>>;
  onAction: (action: HudRuntimeAction) => void;
}) {
  const parsed = (() => {
    try {
      return JSON.parse(hud.document) as {
        widgets: {
          id: string;
          kind: string;
          props: Record<string, unknown>;
          style: Record<string, unknown>;
          bind: Record<string, string>;
          on_click?: HudRuntimeAction;
        }[];
      };
    } catch {
      return null;
    }
  })();

  return (
    <div className="game-hud-overlay" aria-label="Game HUD">
      {hud.widgets
        .filter((widget) => widget.visible)
        .map((widget) => {
          const doc = parsed?.widgets.find((entry) => entry.id === widget.id);
          const style = (doc?.style ?? {}) as Record<string, string | number | undefined>;
          const props = (doc?.props ?? {}) as Record<string, unknown>;
          const boundProps = { ...props };
          for (const [property, path] of Object.entries(doc?.bind ?? {})) {
            if (path in variables) boundProps[property] = variables[path];
          }
          // Percentages of the reference resolution, so the overlay tracks the viewport at
          // any size without re-resolving anything.
          const box: React.CSSProperties = {
            left: `${(widget.rect[0] / hud.reference[0]) * 100}%`,
            top: `${(widget.rect[1] / hud.reference[1]) * 100}%`,
            width: `${(widget.rect[2] / hud.reference[0]) * 100}%`,
            height: `${(widget.rect[3] / hud.reference[1]) * 100}%`,
            background: style.bg as string | undefined,
            color: style.fg as string | undefined,
            borderRadius: style.radius as number | undefined,
            opacity: style.opacity as number | undefined,
            fontSize: style.font_size as number | undefined,
            textAlign: (style.align as React.CSSProperties["textAlign"]) ?? undefined,
          };

          if (widget.kind === "progress_bar") {
            const value = Number(boundProps.value ?? 100);
            return (
              <div key={widget.id} className="game-hud-widget bar" style={box}>
                <span
                  className="game-hud-fill"
                  style={{
                    background: style.fill as string | undefined,
                    width: `${Math.max(0, Math.min(value, 100))}%`,
                  }}
                />
                <span className="game-hud-text">{String(boundProps.format ?? value)}</span>
              </div>
            );
          }
          const text = String(boundProps.text ?? boundProps.value ?? widget.name);
          return doc?.on_click ? (
            <button
              type="button"
              key={widget.id}
              className={`game-hud-widget ${widget.kind}`}
              style={box}
              onClick={() => doc.on_click && onAction(doc.on_click)}
            >
              {text}
            </button>
          ) : (
            <div key={widget.id} className={`game-hud-widget ${widget.kind}`} style={box}>
              {text}
            </div>
          );
        })}
    </div>
  );
}
