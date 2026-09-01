import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import type { RenderManifest, RenderMaterial, RenderMesh } from "../lib/ipc";

/**
 * Mesh and material resources for the viewport (ENG-160/162).
 *
 * This is the fix for F8 — the viewport used to treat `MeshRenderer.mesh` as a primitive
 * *name*, draw any `.glb` as a grey box, and never apply a material map at all (a `grep` for
 * "albedo" in the viewport returned nothing). So what the AI generated and what the user saw
 * were different pictures, and neither could verify anything.
 *
 * Nothing here decides what a material *is*: `engine_render_manifest` resolves the
 * `.mat.json`, looks up `asset:` references and fills in defaults, and this module builds
 * Three.js objects from that (INV-073 as amended by ADR-0028 — rendering is the webview's).
 *
 * Everything is cached and shared. A hundred crates with the same mesh and material are one
 * geometry and one material, which is most of what makes the INV-077 budget reachable.
 */

/** Turn an absolute path into something the webview may load. */
async function assetUrl(path: string): Promise<string> {
  try {
    const { convertFileSrc } = await import("@tauri-apps/api/core");
    return convertFileSrc(path);
  } catch {
    // Outside Tauri (tests, a plain browser) there is no asset protocol; a file path is the
    // best available guess and a failed load is handled by the caller.
    return path;
  }
}

function builtinGeometry(name: string): THREE.BufferGeometry {
  switch (name) {
    case "sphere":
      return new THREE.SphereGeometry(0.5, 32, 16);
    case "plane":
      return new THREE.BoxGeometry(1, 0.02, 1);
    case "capsule":
      return new THREE.CapsuleGeometry(0.3, 0.6, 6, 16);
    case "cylinder":
      return new THREE.CylinderGeometry(0.5, 0.5, 1, 24);
    case "cone":
      return new THREE.ConeGeometry(0.5, 1, 24);
    case "quad":
      return new THREE.PlaneGeometry(1, 1);
    case "torus":
      return new THREE.TorusGeometry(0.4, 0.16, 12, 32);
    case "cube":
    default:
      return new THREE.BoxGeometry(1, 1, 1);
  }
}

/** The placeholder for a reference that does not resolve. */
function missingMaterial(): THREE.Material {
  // Magenta, the universal "this asset is missing" colour. Deliberately loud: a plausible
  // grey box is how a broken reference survives to a build.
  return new THREE.MeshStandardMaterial({
    color: 0xd6219a,
    roughness: 0.6,
    wireframe: true,
  });
}

export class RenderResources {
  private geometries = new Map<string, THREE.BufferGeometry>();
  private materials = new Map<string, THREE.Material>();
  private textures = new Map<string, THREE.Texture>();
  private gltfScenes = new Map<string, THREE.Object3D>();
  private loader = new GLTFLoader();
  private manifest: RenderManifest | null = null;
  private missingKeys = new Set<string>();
  /** Bumped whenever an async load finishes, so the viewport knows to rebuild. */
  private onReady: (() => void) | null = null;

  setReadyHandler(handler: (() => void) | null) {
    this.onReady = handler;
  }

  /** Adopt a new manifest, dropping anything it no longer mentions. */
  adopt(manifest: RenderManifest) {
    this.manifest = manifest;
    this.missingKeys = new Set(manifest.missing);
    const liveMaterials = new Set(manifest.materials.map((material) => material.key));
    for (const [key, material] of this.materials) {
      if (key.startsWith("__")) continue;
      if (!liveMaterials.has(key)) {
        material.dispose();
        this.materials.delete(key);
      }
    }
    void this.preload(manifest);
  }

  private async preload(manifest: RenderManifest) {
    let changed = false;
    for (const material of manifest.materials) {
      if (await this.buildMaterial(material)) changed = true;
    }
    for (const mesh of manifest.meshes) {
      if (mesh.source === "file" && !this.gltfScenes.has(mesh.key)) {
        if (await this.loadMesh(mesh)) changed = true;
      }
    }
    if (changed) this.onReady?.();
  }

  private async texture(path: string, srgb: boolean): Promise<THREE.Texture | null> {
    const cacheKey = `${srgb ? "s" : "l"}:${path}`;
    const cached = this.textures.get(cacheKey);
    if (cached) return cached;
    try {
      const url = await assetUrl(path);
      const texture = await new THREE.TextureLoader().loadAsync(url);
      // Colour maps are authored in sRGB; normal/roughness/metallic/AO carry data, not
      // colour, and must stay linear or the lighting is subtly wrong everywhere.
      texture.colorSpace = srgb ? THREE.SRGBColorSpace : THREE.LinearSRGBColorSpace;
      texture.wrapS = THREE.RepeatWrapping;
      texture.wrapT = THREE.RepeatWrapping;
      this.textures.set(cacheKey, texture);
      return texture;
    } catch {
      return null;
    }
  }

  private async buildMaterial(spec: RenderMaterial): Promise<boolean> {
    if (this.materials.has(spec.key)) return false;
    const material = new THREE.MeshStandardMaterial({
      color: new THREE.Color(spec.base_color[0], spec.base_color[1], spec.base_color[2]),
      roughness: spec.roughness,
      metalness: spec.metallic,
      emissive: new THREE.Color(
        spec.emissive[0] * spec.emissive_strength,
        spec.emissive[1] * spec.emissive_strength,
        spec.emissive[2] * spec.emissive_strength,
      ),
      side: spec.double_sided ? THREE.DoubleSide : THREE.FrontSide,
      transparent: spec.alpha_mode === "blend",
      alphaTest: spec.alpha_mode === "mask" ? spec.alpha_cutoff : 0,
    });
    // Register before awaiting: two entities sharing a material must not both start a load.
    this.materials.set(spec.key, material);

    const [albedo, normal, roughness, metallic, ao, emissive] = await Promise.all([
      spec.albedo ? this.texture(spec.albedo, true) : null,
      spec.normal ? this.texture(spec.normal, false) : null,
      spec.roughness_map ? this.texture(spec.roughness_map, false) : null,
      spec.metallic_map ? this.texture(spec.metallic_map, false) : null,
      spec.ao ? this.texture(spec.ao, false) : null,
      spec.emissive_map ? this.texture(spec.emissive_map, true) : null,
    ]);

    const tile = (texture: THREE.Texture | null) => {
      if (!texture) return null;
      const clone = texture.clone();
      clone.needsUpdate = true;
      clone.repeat.set(spec.tiling[0], spec.tiling[1]);
      clone.offset.set(spec.offset[0], spec.offset[1]);
      return clone;
    };

    material.map = tile(albedo);
    material.normalMap = tile(normal);
    if (material.normalMap) {
      material.normalScale = new THREE.Vector2(spec.normal_strength, spec.normal_strength);
    }
    material.roughnessMap = tile(roughness);
    material.metalnessMap = tile(metallic);
    material.aoMap = tile(ao);
    material.emissiveMap = tile(emissive);
    material.needsUpdate = true;
    return true;
  }

  private async loadMesh(mesh: RenderMesh): Promise<boolean> {
    try {
      const url = await assetUrl(mesh.value);
      const gltf = await this.loader.loadAsync(url);
      // Normalise to a unit-ish size so an imported model does not arrive a hundred metres
      // tall next to the primitives. The entity's own scale then means what it says.
      const scene = gltf.scene;
      const box = new THREE.Box3().setFromObject(scene);
      const size = box.getSize(new THREE.Vector3());
      const longest = Math.max(size.x, size.y, size.z);
      if (Number.isFinite(longest) && longest > 0) {
        scene.scale.multiplyScalar(1 / longest);
      }
      this.gltfScenes.set(mesh.key, scene);
      return true;
    } catch {
      // A file that will not load is as missing as one that is not there.
      this.missingKeys.add(mesh.key);
      return true;
    }
  }

  /** The material for a `MeshRenderer`, or a sensible default. */
  materialFor(materials: string[] | undefined, fallbackColor = 0x8b93a5): THREE.Material {
    const key = materials?.find((entry) => entry.length > 0);
    if (key) {
      if (this.missingKeys.has(key)) return this.missingOrCached("__missing");
      const found = this.materials.get(key);
      if (found) return found;
    }
    const cacheKey = `__default-${fallbackColor}`;
    const cached = this.materials.get(cacheKey);
    if (cached) return cached;
    const material = new THREE.MeshStandardMaterial({
      color: fallbackColor,
      roughness: 0.72,
      metalness: 0.02,
    });
    this.materials.set(cacheKey, material);
    return material;
  }

  private missingOrCached(key: string): THREE.Material {
    const cached = this.materials.get(key);
    if (cached) return cached;
    const material = missingMaterial();
    this.materials.set(key, material);
    return material;
  }

  /**
   * Build the object for one mesh reference.
   *
   * Returns `null` only when there is nothing to draw at all; a reference that does not
   * resolve comes back as a loud magenta wireframe rather than something that looks fine.
   */
  meshFor(reference: string | undefined, materials: string[] | undefined): THREE.Object3D {
    const key = reference?.trim() ?? "";
    const spec = this.manifest?.meshes.find((mesh) => mesh.key === key);

    if (spec?.source === "file") {
      const loaded = this.gltfScenes.get(key);
      if (loaded) {
        // Clone so a hundred instances share geometry but keep their own transforms.
        const instance = loaded.clone(true);
        instance.traverse((child) => {
          if (child instanceof THREE.Mesh) {
            child.castShadow = true;
            child.receiveShadow = true;
          }
        });
        return instance;
      }
      // Still loading: a neutral box holds the slot without pretending to be the model.
      const pending = new THREE.Mesh(
        this.geometry("cube"),
        this.materialFor(undefined, 0x39404d),
      );
      pending.castShadow = true;
      return pending;
    }

    if (spec?.source === "missing" || (key.length > 0 && !spec)) {
      const missing = new THREE.Mesh(this.geometry("cube"), this.missingOrCached("__missing"));
      missing.userData.__missingAsset = key;
      return missing;
    }

    const primitive = spec?.source === "builtin" ? spec.value : "cube";
    const mesh = new THREE.Mesh(this.geometry(primitive), this.materialFor(materials));
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    return mesh;
  }

  /** A shared geometry for a built-in primitive. */
  geometry(name: string): THREE.BufferGeometry {
    const cached = this.geometries.get(name);
    if (cached) return cached;
    const geometry = builtinGeometry(name);
    this.geometries.set(name, geometry);
    return geometry;
  }

  /** References the scene points at that could not be resolved. */
  get missing(): string[] {
    return [...this.missingKeys];
  }

  dispose() {
    for (const geometry of this.geometries.values()) geometry.dispose();
    for (const material of this.materials.values()) material.dispose();
    for (const texture of this.textures.values()) texture.dispose();
    this.geometries.clear();
    this.materials.clear();
    this.textures.clear();
    this.gltfScenes.clear();
  }
}
