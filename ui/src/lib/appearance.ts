/**
 * Appearance: palette, material, ground and ambience.
 *
 * Two independent axes, and this module's whole job is to keep them independent
 * (see the theme layer in `styles/tokens.css` for the CSS half of the contract):
 *
 *   - `colorScheme` is the PALETTE. It picks which `--base-*` colours are live.
 *   - `styleMode`   is the MATERIAL. It picks how solid those colours render:
 *       "max"   flat and opaque, no blur, no ambience, cheapest to draw
 *       "plan"  a palette-derived gradient ground under lightly frosted panels
 *       "glass" full frosted transparency over a wallpaper or gradient ground
 *
 * This file must never write a colour. The previous version set `--bg`,
 * `--surface`, `--line` and friends as INLINE styles on `<html>`, which outrank
 * every stylesheet rule — so the palette only ever changed the accent, and once
 * Max mode had been visited those inline values were never cleared and no scheme
 * worked again. `clearLegacyInlineColours` below unsticks profiles that hit that.
 */

export type StyleMode = "max" | "plan" | "glass";

/** A ground is a SHAPE; the palette supplies its colour (see tokens.css § GROUNDS). */
export type GroundId = "halo" | "aurora" | "ember" | "depth" | "mesh" | "flat";

export interface Ground {
  id: GroundId;
  name: string;
  hint: string;
}

export interface WallpaperConfig {
  id: string;
  name: string;
  /** A `data:` URL. Custom wallpapers are images only; gradients are grounds now. */
  url: string;
  isCustom: boolean;
  createdAt?: number;
}

export interface AppearanceSettings {
  styleMode: StyleMode;
  colorScheme: string;
  ground: GroundId;
  /** `null` means "use the ground gradient"; otherwise a custom wallpaper id. */
  activeWallpaperId: string | null;
  /** 0-90: how far the veil darkens the ground so text stays legible. */
  wallpaperDim: number;
  glassBlur: number;
  glassOpacity: number;
  glassSaturation: number;
  /** The breathing ambient light behind everything. Off holds it still. */
  ambientMotion: boolean;
  customWallpapers: WallpaperConfig[];
}

export const GROUNDS: Ground[] = [
  { id: "halo", name: "Halo", hint: "Light source above the canvas" },
  { id: "aurora", name: "Aurora", hint: "Diagonal sweep across the shell" },
  { id: "ember", name: "Ember", hint: "Low glow from the bottom-left" },
  { id: "depth", name: "Depth", hint: "Quiet vertical fade into the dark" },
  { id: "mesh", name: "Mesh", hint: "Three soft blooms, most colour" },
  { id: "flat", name: "Flat", hint: "No gradient at all" },
];

/** Palette ids, in the order the picker shows them. Must match tokens.css. */
export const COLOR_SCHEMES = [
  "dark",
  "sapphire",
  "emerald",
  "glacier",
  "amethyst",
  "cyberpunk",
  "crimson",
  "slate",
  "titanium",
  "light",
  "paper",
  "contrast",
  "system",
] as const;

export type ColorSchemeId = (typeof COLOR_SCHEMES)[number];

/**
 * Ids that used to exist. `frosted-glass`, `gradient` and `transparent` were never
 * palettes — they were materials wearing a palette's clothes, which is precisely
 * the confusion this rewrite removes. They map onto the nearest real palette.
 */
const LEGACY_SCHEMES: Record<string, ColorSchemeId> = {
  "frosted-glass": "glacier",
  gradient: "amethyst",
  transparent: "sapphire",
};

/** The old hard-coded gradient wallpapers, mapped onto the ground whose shape matches. */
const LEGACY_WALLPAPER_GROUNDS: Record<string, GroundId> = {
  "preset-night-royal": "depth",
  "preset-ember-glow": "ember",
  "preset-deep-sea": "aurora",
  "preset-void-neon": "aurora",
  "preset-forest-mist": "depth",
  "preset-cyber-aurora": "mesh",
};

export const DEFAULT_APPEARANCE_SETTINGS: AppearanceSettings = {
  styleMode: "glass",
  colorScheme: "titanium",
  ground: "ember",
  activeWallpaperId: null,
  wallpaperDim: 26,
  glassBlur: 22,
  glassOpacity: 0.62,
  glassSaturation: 128,
  ambientMotion: true,
  customWallpapers: [],
};

const STORAGE_KEY = "bhippi-appearance-v5";
const LEGACY_STORAGE_KEY = "bhippi-appearance-settings-v4";
const EVENT_NAME = "bhippi-appearance-changed";

function clampNumber(value: unknown, min: number, max: number, fallback: number): number {
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(parsed)) return fallback;
  return Math.min(max, Math.max(min, parsed));
}

/** Accepts anything that has ever been in storage and returns something renderable. */
function normalise(raw: unknown): AppearanceSettings {
  const input = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const defaults = DEFAULT_APPEARANCE_SETTINGS;

  const styleMode: StyleMode =
    input.styleMode === "max" || input.styleMode === "plan" || input.styleMode === "glass"
      ? input.styleMode
      : defaults.styleMode;

  const rawScheme = typeof input.colorScheme === "string" ? input.colorScheme : defaults.colorScheme;
  const colorScheme = LEGACY_SCHEMES[rawScheme]
    ?? ((COLOR_SCHEMES as readonly string[]).includes(rawScheme) ? rawScheme : defaults.colorScheme);

  const customWallpapers = Array.isArray(input.customWallpapers)
    ? (input.customWallpapers as WallpaperConfig[]).filter(
        (item) => item && typeof item.id === "string" && typeof item.url === "string" && item.isCustom !== false,
      )
    : [];

  // A v4 profile stored a preset-gradient id here; those are grounds now, so the id
  // becomes a ground and the wallpaper slot goes back to "no image".
  const storedWallpaper = typeof input.activeWallpaperId === "string" ? input.activeWallpaperId : null;
  const migratedGround = storedWallpaper ? LEGACY_WALLPAPER_GROUNDS[storedWallpaper] : undefined;
  const rawGround = typeof input.ground === "string" ? input.ground : undefined;
  const ground: GroundId =
    (GROUNDS.some((entry) => entry.id === rawGround) ? (rawGround as GroundId) : undefined)
    ?? migratedGround
    ?? defaults.ground;

  const activeWallpaperId =
    storedWallpaper && customWallpapers.some((item) => item.id === storedWallpaper)
      ? storedWallpaper
      : null;

  return {
    styleMode,
    colorScheme,
    ground,
    activeWallpaperId,
    wallpaperDim: clampNumber(input.wallpaperDim, 0, 90, defaults.wallpaperDim),
    glassBlur: clampNumber(input.glassBlur, 0, 60, defaults.glassBlur),
    glassOpacity: clampNumber(input.glassOpacity, 0.2, 0.95, defaults.glassOpacity),
    glassSaturation: clampNumber(input.glassSaturation, 100, 240, defaults.glassSaturation),
    ambientMotion:
      typeof input.ambientMotion === "boolean"
        ? input.ambientMotion
        : typeof input.animatedGradient === "boolean"
          ? input.animatedGradient
          : defaults.ambientMotion,
    customWallpapers,
  };
}

export function getAppearanceSettings(): AppearanceSettings {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY) ?? window.localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_APPEARANCE_SETTINGS };
    return normalise(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_APPEARANCE_SETTINGS };
  }
}

let saveTimeout: number | null = null;

export function saveAppearanceSettings(settings: AppearanceSettings, immediate = false): void {
  // Paint first, persist second: a slider dragged at 60 fps must not wait on a write.
  applyAppearanceToDOM(settings);
  window.dispatchEvent(new CustomEvent(EVENT_NAME, { detail: settings }));

  const persist = () => {
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
      window.localStorage.removeItem(LEGACY_STORAGE_KEY);
    } catch (error) {
      console.error("Failed saving appearance settings:", error);
    }
  };

  if (saveTimeout !== null) {
    window.clearTimeout(saveTimeout);
    saveTimeout = null;
  }
  if (immediate) persist();
  else saveTimeout = window.setTimeout(persist, 200);
}

export function addCustomWallpapers(items: { name: string; dataUrl: string }[]): WallpaperConfig[] {
  if (items.length === 0) return [];
  const current = getAppearanceSettings();
  const created: WallpaperConfig[] = items.map((item, index) => ({
    id: `custom_${Date.now()}_${index}_${Math.random().toString(36).slice(2, 6)}`,
    name: item.name.slice(0, 30) || "Wallpaper",
    url: item.dataUrl,
    isCustom: true,
    createdAt: Date.now() + index,
  }));

  saveAppearanceSettings({
    ...current,
    activeWallpaperId: created[0]?.id ?? current.activeWallpaperId,
    customWallpapers: [...created, ...current.customWallpapers].slice(0, 20),
  });
  return created;
}

export function deleteCustomWallpaper(id: string): void {
  const current = getAppearanceSettings();
  const remaining = current.customWallpapers.filter((item) => item.id !== id);
  saveAppearanceSettings({
    ...current,
    // Deleting the active image falls back to the gradient ground, never to a blank screen.
    activeWallpaperId: current.activeWallpaperId === id ? null : current.activeWallpaperId,
    customWallpapers: remaining,
  });
}

/** Ensures a fixed background layer exists, creating it on first call. */
function layer(id: string, build?: (element: HTMLDivElement) => void): HTMLElement {
  const existing = document.getElementById(id);
  if (existing) return existing;
  const created = document.createElement("div");
  created.id = id;
  created.setAttribute("aria-hidden", "true");
  build?.(created);
  document.body.prepend(created);
  return created;
}

/**
 * Removes the inline colour properties the pre-v5 build wrote onto `<html>`.
 *
 * They are the reason a scheme could look permanently stuck: an inline custom
 * property beats every `:root[data-color-scheme=…]` rule, and nothing ever removed
 * them once Max mode had been selected. Cheap enough to run on every apply.
 */
function clearLegacyInlineColours(root: HTMLElement): void {
  for (const property of ["--bg", "--surface", "--surface-2", "--surface-3", "--line", "--line-strong"]) {
    root.style.removeProperty(property);
  }
}

/** Writes the whole appearance state onto the document. Idempotent. */
export function applyAppearanceToDOM(settings: AppearanceSettings): void {
  const root = document.documentElement;
  clearLegacyInlineColours(root);

  root.dataset.styleMode = settings.styleMode;
  root.dataset.colorScheme = settings.colorScheme;
  root.dataset.ambient = settings.ambientMotion ? "breathing" : "still";

  root.style.setProperty("--glass-blur", `${settings.glassBlur}px`);
  root.style.setProperty("--glass-saturation", `${settings.glassSaturation}%`);
  root.style.setProperty("--glass-opacity", `${settings.glassOpacity}`);
  // color-mix needs a percentage, and doing the conversion once here beats a
  // calc() in every rule that mixes a surface.
  root.style.setProperty("--glass-fill", `${Math.round(settings.glassOpacity * 100)}%`);
  root.style.setProperty("--wallpaper-dim", `${settings.wallpaperDim / 100}`);

  const wallpaperLayer = layer("app-wallpaper-layer");
  layer("app-ambient", (element) => {
    element.innerHTML = '<i class="amb amb-1"></i><i class="amb amb-2"></i><i class="amb amb-3"></i>';
  });
  layer("app-wallpaper-dim");

  const image = settings.activeWallpaperId
    ? settings.customWallpapers.find((item) => item.id === settings.activeWallpaperId)
    : undefined;

  if (image) {
    root.dataset.ground = "image";
    wallpaperLayer.style.backgroundImage = `url("${image.url}")`;
  } else {
    root.dataset.ground = settings.ground;
    // The gradient comes from the stylesheet so it can reference the palette; an
    // inline image left behind would sit on top of it.
    wallpaperLayer.style.removeProperty("background-image");
  }
}

export function onAppearanceChange(callback: (settings: AppearanceSettings) => void): () => void {
  const handler = (event: Event) => callback((event as CustomEvent<AppearanceSettings>).detail);
  window.addEventListener(EVENT_NAME, handler);
  return () => window.removeEventListener(EVENT_NAME, handler);
}
