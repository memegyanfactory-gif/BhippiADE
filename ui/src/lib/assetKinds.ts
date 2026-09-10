/**
 * How the Assets screen classifies and groups what it finds under `assets/**` (GAD-011's
 * successor surface; docs/16 §4.2 "Assets = project asset library + licence status").
 *
 * Kind is decided by extension, which is what the file *is* rather than what a folder is
 * called; a texture in `assets/models/` is still a texture.
 */

export type AssetKind = "model" | "texture" | "audio" | "ui" | "other";

const BY_EXTENSION: Record<string, AssetKind> = {
  // Models
  glb: "model",
  gltf: "model",
  obj: "model",
  fbx: "model",
  dae: "model",
  blend: "model",
  mesh: "model",
  res: "model",
  tscn: "model",
  // Textures
  png: "texture",
  jpg: "texture",
  jpeg: "texture",
  webp: "texture",
  tga: "texture",
  bmp: "texture",
  ktx2: "texture",
  hdr: "texture",
  exr: "texture",
  // Audio
  wav: "audio",
  mp3: "audio",
  ogg: "audio",
  flac: "audio",
  m4a: "audio",
  // UI
  ttf: "ui",
  otf: "ui",
  woff: "ui",
  woff2: "ui",
  svg: "ui",
  hud: "ui",
  json: "ui",
};

export const ASSET_KIND_LABEL: Record<AssetKind, string> = {
  model: "Model",
  texture: "Texture",
  audio: "Audio",
  ui: "UI",
  other: "Other",
};

export function assetKind(path: string): AssetKind {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return "other";
  return BY_EXTENSION[name.slice(dot + 1).toLowerCase()] ?? "other";
}

/** `assets/models/hero.glb` → `assets/models`. Files at the root group under `assets`. */
export function assetFolder(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut <= 0 ? "assets" : path.slice(0, cut);
}

/** Bytes as the table shows them: short, and never "0.0 KB" for a real file. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * The licence a sibling `<file>.meta.json` claims, or `null` when there is none.
 *
 * `null` is rendered as `unknown` in the warning style, never as blank: an unlicensed asset
 * that looks like a missing cell is exactly the one that ships by accident (INV: no
 * unlicensed image ships).
 */
export function licenceFromMeta(text: string | null | undefined): string | null {
  if (typeof text !== "string" || text.trim().length === 0) return null;
  try {
    const parsed: unknown = JSON.parse(text);
    if (parsed && typeof parsed === "object") {
      const value = (parsed as Record<string, unknown>).license;
      if (typeof value === "string" && value.trim().length > 0) return value.trim();
    }
  } catch {
    // A meta file we cannot parse tells us nothing, which is the same as having none.
  }
  return null;
}

/** `assets/models/hero.glb` → `assets/models/hero.glb.meta.json`. */
export function metaPathFor(path: string): string {
  return `${path}.meta.json`;
}

export type AssetProvenance = "all" | "procedural" | "bundled" | "external" | "user";

export function provenanceFromMeta(text: string | null | undefined): AssetProvenance {
  if (typeof text !== "string" || text.trim().length === 0) return "user";
  try {
    const parsed: unknown = JSON.parse(text);
    if (parsed && typeof parsed === "object") {
      const prov = (parsed as Record<string, unknown>).provenance;
      if (typeof prov === "string") {
        const lower = prov.toLowerCase();
        if (lower.includes("procedural")) return "procedural";
        if (lower.includes("bundled") || lower.includes("cc0")) return "bundled";
        if (lower.includes("external") || lower.includes("generated") || lower.includes("provider")) return "external";
      } else if (prov && typeof prov === "object") {
        const source = String((prov as Record<string, unknown>).source ?? "").toLowerCase();
        if (source.includes("procedural")) return "procedural";
        if (source.includes("bundled") || source.includes("cc0")) return "bundled";
        if (source.includes("external") || source.includes("generated") || source.includes("provider")) return "external";
      }
    }
  } catch {
    // A meta file we cannot parse defaults to user
  }
  return "user";
}

