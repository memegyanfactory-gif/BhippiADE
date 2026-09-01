/**
 * Scene *types* only (ENG-105, INV-073).
 *
 * This file used to hold the engine's logic: it created scenes, generated ULIDs, built
 * default entities, applied weather presets, duplicated entities and merged Main + HUD +
 * level for play. All of that now lives in `bhippi-engine` — `scaffold.rs`, `weather.rs`,
 * `action.rs`, `transaction.rs`, `compose.rs` — where it is tested, undoable and journaled.
 * Two of those functions were also quietly wrong: `newEntityId`/`mergeScenes` produced ids
 * (`level_01JD…`) that the Rust parser rejects, so a "composed" scene could never round-trip.
 *
 * What remains is the shape of `bhippi-scene@1` as the webview reads it, plus one decode
 * helper. Decoding transport is not business logic; deciding what a component *means* is,
 * and that does not happen here.
 */

export type SceneVec3 = [number, number, number];
export type SceneQuat = [number, number, number, number];

export interface SceneTransform {
  pos: SceneVec3;
  rot: SceneQuat;
  scale: SceneVec3;
}

export interface SceneMeshRenderer {
  mesh: string;
  materials: string[];
  cast_shadows: boolean;
}

export interface SceneLight {
  kind: string;
  color?: number[];
  intensity?: number;
  range?: number;
  outer_angle?: number;
}

export interface SceneCamera {
  fov: number;
  near: number;
  far: number;
  orthographic: boolean;
}

export interface SceneCharacterController {
  height: number;
  max_slope: number;
  radius: number;
  step_height?: number;
  move_speed?: number;
  jump_speed?: number;
}

export interface SceneRigidBody {
  kind: string;
  lock_rotation: boolean;
  mass: number;
}

export interface SceneComponentMap {
  Transform?: SceneTransform;
  MeshRenderer?: SceneMeshRenderer;
  Light?: SceneLight;
  Camera?: SceneCamera;
  CharacterController?: SceneCharacterController;
  RigidBody?: SceneRigidBody;
  [key: string]: any;
}

export interface SceneEntity {
  id: string;
  name: string;
  parent: string | null;
  tags: string[];
  components: SceneComponentMap;
}

export interface SceneOrganizerFolder {
  id: string;
  name: string;
  parent: string | null;
}

export interface SceneEditorMetadata {
  folders: SceneOrganizerFolder[];
  entity_folders: Record<string, string>;
}

export type SceneKind = "main" | "level" | "hud" | "empty";
export type WeatherId = "clear" | "overcast" | "rain" | "snow" | "fog" | "storm" | "sunset" | "night";

export interface SceneSettings {
  ambient: SceneVec3;
  skybox: string | null;
  kind?: SceneKind;
  hud?: string | null;
  levels?: string[];
  weather?: WeatherId | string | null;
}

export interface MaterialMaps {
  albedo?: string | null;
  normal?: string | null;
  roughness?: string | null;
  metallic?: string | null;
  ao?: string | null;
  emissive?: string | null;
  color?: number[];
  shader?: string | null;
}

export interface SceneDoc {
  format: string;
  id: string;
  name: string;
  settings: SceneSettings;
  editor: SceneEditorMetadata;
  entities: SceneEntity[];
}

/**
 * A weather preset as the engine defines it (`bhippi-engine::weather`). The pane fetches
 * these once via `engineWeatherPresets()`; the numbers are not duplicated here.
 */
export interface WeatherPreset {
  id: string;
  label: string;
  ambient: SceneVec3;
  sun: number;
  fog: number;
  /** Packed 0xRRGGBB. */
  sky: number;
  precip: string;
}

/** The empty document the pane renders before a scene is open, or for a non-game folder. */
export const EMPTY_SCENE: SceneDoc = {
  format: "bhippi-scene@1",
  id: "",
  name: "Untitled",
  settings: { ambient: [0.08, 0.09, 0.1], skybox: null, kind: "empty", hud: null, levels: [], weather: "clear" },
  editor: { folders: [], entity_folders: {} },
  entities: [],
};

/**
 * Decode the `document_json` an `EngineSceneState` carries. The engine sends its own
 * canonical, already-validated `bhippi-scene@1` text, so this is a transport decode with a
 * safety net — never a repair step. A document that fails here is an engine bug, not
 * something the webview should try to fix.
 */
export function decodeSceneDocument(documentJson: string): SceneDoc {
  if (!documentJson) return EMPTY_SCENE;
  try {
    const parsed = JSON.parse(documentJson) as SceneDoc;
    if (!parsed || !Array.isArray(parsed.entities)) return EMPTY_SCENE;
    parsed.editor ??= { folders: [], entity_folders: {} };
    return parsed;
  } catch {
    return EMPTY_SCENE;
  }
}
