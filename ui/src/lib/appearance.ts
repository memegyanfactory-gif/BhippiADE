/**
 * Appearance, Style Modes, Transparency & Custom Wallpaper Engine.
 * Supports three core modes:
 *  - "max": Pure solid surfaces, zero gradients, ultra-minimalist instrument feel.
 *  - "plan": Sleek multi-stop gradients, glowing borders, cool modern aesthetic.
 *  - "glass": Full frosted-glass transparency across all panels with custom wallpapers.
 */

export type StyleMode = "max" | "plan" | "glass";

export type GradientPreset = "sunset" | "neon" | "aurora" | "void" | "royal";

export interface WallpaperConfig {
  id: string;
  name: string;
  url: string; // CSS url() or gradient or data:image
  isCustom: boolean;
  thumbnailUrl?: string;
  createdAt?: number;
}

export interface AppearanceSettings {
  styleMode: StyleMode;
  colorScheme: string;
  activeWallpaperId: string | null;
  wallpaperDim: number; // 0 to 90 (% overlay darkness)
  glassBlur: number; // 0 to 60 (px)
  glassOpacity: number; // 0.15 to 0.95
  glassSaturation: number; // 100 to 250 (%)
  gradientPreset: GradientPreset;
  animatedGradient: boolean;
  customWallpapers: WallpaperConfig[];
}

export const PRESET_WALLPAPERS: WallpaperConfig[] = [
  {
    id: "preset-night-royal",
    name: "Night Royal",
    url: "linear-gradient(135deg, #090a14 0%, #151035 45%, #0b1426 100%)",
    isCustom: false,
  },
  {
    id: "preset-ember-glow",
    name: "Ember Flame",
    url: "radial-gradient(ellipse 88% 72% at 58% 16%, #8a3d0e 0%, #4a1e08 24%, #1a0c07 52%, #0a0806 76%, #070605 100%)",
    isCustom: false,
  },
  {
    id: "preset-deep-sea",
    name: "Deep Ocean",
    url: "linear-gradient(135deg, #020813 0%, #03203f 45%, #021226 100%)",
    isCustom: false,
  },
  {
    id: "preset-void-neon",
    name: "Void Neon",
    url: "linear-gradient(135deg, #0a0114 0%, #200238 45%, #020a1f 100%)",
    isCustom: false,
  },
  {
    id: "preset-forest-mist",
    name: "Forest Mist",
    url: "linear-gradient(135deg, #020f06 0%, #082914 45%, #04140b 100%)",
    isCustom: false,
  },
  {
    id: "preset-cyber-aurora",
    name: "Cyber Aurora",
    url: "linear-gradient(135deg, #0d0a21 0%, #122842 50%, #06312a 100%)",
    isCustom: false,
  },
];

export const GRADIENT_PRESETS: Record<GradientPreset, { name: string; gradient: string; accent: string }> = {
  sunset: {
    name: "Sunset Flux",
    gradient: "linear-gradient(135deg, #1c0800 0%, #3d1400 40%, #1a0314 100%)",
    accent: "#f0a02c",
  },
  neon: {
    name: "Cyber Neon",
    gradient: "linear-gradient(135deg, #001026 0%, #023859 40%, #1e003b 100%)",
    accent: "#38bdf8",
  },
  aurora: {
    name: "Deep Aurora",
    gradient: "linear-gradient(135deg, #02140c 0%, #00362b 40%, #021a36 100%)",
    accent: "#10b981",
  },
  void: {
    name: "Cosmic Void",
    gradient: "linear-gradient(135deg, #090317 0%, #260538 40%, #020921 100%)",
    accent: "#c084fc",
  },
  royal: {
    name: "Royal Obsidian",
    gradient: "linear-gradient(135deg, #0a0a0f 0%, #181822 50%, #08080c 100%)",
    accent: "#fbbf24",
  },
};

export const DEFAULT_APPEARANCE_SETTINGS: AppearanceSettings = {
  styleMode: "glass",
  colorScheme: "dark",
  activeWallpaperId: "preset-ember-glow",
  wallpaperDim: 26,
  glassBlur: 22,
  glassOpacity: 0.52,
  glassSaturation: 128,
  gradientPreset: "sunset",
  animatedGradient: false,
  customWallpapers: [],
};

const STORAGE_KEY = "bhippi-appearance-settings-v4";
const EVENT_NAME = "bhippi-appearance-changed";

export function getAppearanceSettings(): AppearanceSettings {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_APPEARANCE_SETTINGS };
    const parsed = JSON.parse(raw);
    return {
      ...DEFAULT_APPEARANCE_SETTINGS,
      ...parsed,
      customWallpapers: Array.isArray(parsed.customWallpapers) ? parsed.customWallpapers : [],
    };
  } catch {
    return { ...DEFAULT_APPEARANCE_SETTINGS };
  }
}

let saveTimeout: number | null = null;

export function saveAppearanceSettings(settings: AppearanceSettings, immediate = false): void {
  // Apply changes to DOM and dispatch events immediately for instant 60fps rendering
  applyAppearanceToDOM(settings);
  window.dispatchEvent(new CustomEvent(EVENT_NAME, { detail: settings }));

  const doPersist = () => {
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    } catch (e) {
      console.error("Failed saving appearance settings:", e);
    }
  };

  if (immediate) {
    if (saveTimeout !== null) {
      clearTimeout(saveTimeout);
      saveTimeout = null;
    }
    doPersist();
  } else {
    if (saveTimeout !== null) {
      clearTimeout(saveTimeout);
    }
    saveTimeout = window.setTimeout(doPersist, 200);
  }
}

export function addCustomWallpapers(items: { name: string; dataUrl: string }[]): WallpaperConfig[] {
  if (!items || items.length === 0) return [];
  const current = getAppearanceSettings();
  const created: WallpaperConfig[] = items.map((item, idx) => ({
    id: `custom_${Date.now()}_${idx}_${Math.random().toString(36).substring(2, 6)}`,
    name: item.name.slice(0, 30) || "Custom Wallpaper",
    url: item.dataUrl,
    isCustom: true,
    thumbnailUrl: item.dataUrl,
    createdAt: Date.now() + idx,
  }));

  const updated: AppearanceSettings = {
    ...current,
    activeWallpaperId: created[0]?.id || current.activeWallpaperId,
    customWallpapers: [...created, ...current.customWallpapers].slice(0, 20),
  };

  saveAppearanceSettings(updated);
  return created;
}

export function addCustomWallpaper(name: string, dataUrl: string): WallpaperConfig {
  return addCustomWallpapers([{ name, dataUrl }])[0];
}

export function deleteCustomWallpaper(id: string): void {
  const current = getAppearanceSettings();
  const nextCustoms = current.customWallpapers.filter((w) => w.id !== id);
  let nextActiveId = current.activeWallpaperId;
  if (nextActiveId === id) {
    nextActiveId = nextCustoms[0]?.id || PRESET_WALLPAPERS[0].id;
  }

  const updated: AppearanceSettings = {
    ...current,
    activeWallpaperId: nextActiveId,
    customWallpapers: nextCustoms,
  };

  saveAppearanceSettings(updated);
}

/**
 * Applies CSS classes and variables to the document element and background layer.
 */
export function applyAppearanceToDOM(settings: AppearanceSettings): void {
  const root = document.documentElement;

  // Set the style mode attribute: "max" | "plan" | "glass"
  root.dataset.styleMode = settings.styleMode;
  if (settings.colorScheme) {
    root.dataset.colorScheme = settings.colorScheme;
    try {
      window.localStorage.setItem("bhippi-color-scheme", settings.colorScheme);
    } catch {
      /* ignore */
    }
  }

  // CSS variables for glass optics
  root.style.setProperty("--glass-blur", `${settings.glassBlur}px`);
  root.style.setProperty("--glass-opacity", `${settings.glassOpacity}`);
  root.style.setProperty("--glass-saturation", `${settings.glassSaturation}%`);
  root.style.setProperty("--wallpaper-dim", `${settings.wallpaperDim / 100}`);

  // Resolve active wallpaper
  const allWallpapers = [...settings.customWallpapers, ...PRESET_WALLPAPERS];
  const activeWp = allWallpapers.find((w) => w.id === settings.activeWallpaperId) || PRESET_WALLPAPERS[0];

  // Background layer element
  let bgLayer = document.getElementById("app-wallpaper-layer");
  if (!bgLayer) {
    bgLayer = document.createElement("div");
    bgLayer.id = "app-wallpaper-layer";
    document.body.prepend(bgLayer);
  }

  let dimLayer = document.getElementById("app-wallpaper-dim");
  if (!dimLayer) {
    dimLayer = document.createElement("div");
    dimLayer.id = "app-wallpaper-dim";
    document.body.prepend(dimLayer);
  }

  if (settings.styleMode === "max") {
    bgLayer.style.display = "none";
    dimLayer.style.display = "none";
    bgLayer.classList.remove("plan-breathing");
    bgLayer.classList.remove("animated-flux");
    root.style.setProperty("--bg", "#100f0d");
    root.style.setProperty("--surface", "#171614");
    root.style.setProperty("--surface-2", "#1e1c19");
    root.style.setProperty("--surface-3", "#262320");
    root.style.setProperty("--line", "#2c2926");
    root.style.setProperty("--line-strong", "#3b3733");
  } else if (settings.styleMode === "plan") {
    bgLayer.style.display = "block";
    dimLayer.style.display = "block";
    bgLayer.classList.add("plan-breathing");
    const preset = GRADIENT_PRESETS[settings.gradientPreset] || GRADIENT_PRESETS.neon;
    bgLayer.style.background = preset.gradient;
    bgLayer.style.backgroundSize = "240% 240%";
    if (settings.animatedGradient) {
      bgLayer.classList.add("animated-flux");
    } else {
      bgLayer.classList.remove("animated-flux");
    }
  } else {
    // Glass mode with wallpaper
    bgLayer.style.display = "block";
    dimLayer.style.display = "block";
    bgLayer.classList.remove("animated-flux");
    bgLayer.classList.remove("plan-breathing");

    if (activeWp.url.startsWith("data:") || activeWp.url.startsWith("http") || activeWp.url.startsWith("url(")) {
      const imgUrl = activeWp.url.startsWith("url(") ? activeWp.url : `url(${activeWp.url})`;
      bgLayer.style.background = `${imgUrl} center/cover no-repeat fixed`;
    } else {
      bgLayer.style.background = activeWp.url;
      bgLayer.style.backgroundSize = "cover";
    }
  }
}

export function onAppearanceChange(callback: (settings: AppearanceSettings) => void): () => void {
  const handler = (e: Event) => {
    callback((e as CustomEvent<AppearanceSettings>).detail);
  };
  window.addEventListener(EVENT_NAME, handler);
  return () => window.removeEventListener(EVENT_NAME, handler);
}
