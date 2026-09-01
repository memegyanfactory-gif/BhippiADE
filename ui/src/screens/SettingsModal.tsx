import { useEffect, useRef, useState } from "react";
import type {
  AppStatus,
  ComputerUseStatus,
  ProviderInfo,
  ScreenCapture,
  Skill,
  TierBudgetView,
  ToolAvailability,
} from "../lib/ipc";
import { api, events } from "../lib/api";
import {
  IconArrowLeft,
  IconBadgeCheck,
  IconBolt,
  IconBrain,
  IconCamera,
  IconCheck,
  IconClose,
  IconCode,
  IconCopy,
  IconCrown,
  IconExternal,
  IconEye,
  IconEyeOff,
  IconFetchUrl,
  IconKey,
  IconMic,
  IconMonitor,
  IconPlus,
  IconRefresh,
  IconSettings,
  IconSparkles,
  IconTrash,
  IconUser,
  IconVision,
  IconVolume,
  IconWaveform,
} from "../components/icons";
import { ProviderLogo } from "../components/ProviderLogo";
import { UsagePanel } from "./UsagePanel";
import {
  getProfile,
  maskLicenseKey,
  onProfileChange,
  saveProfile,
  type UserProfile,
} from "../lib/profile";
import {
  getAudioSettings,
  saveAudioSettings,
  onAudioSettingsChange,
  testAudioProvider,
  type AudioSettings,
  type AudioProviderId,
  type AudioProviderConfig,
  DEFAULT_AUDIO_SETTINGS,
} from "../lib/audio";
import {
  getAppearanceSettings,
  saveAppearanceSettings,
  onAppearanceChange,
  addCustomWallpapers,
  deleteCustomWallpaper,
  PRESET_WALLPAPERS,
  GRADIENT_PRESETS,
  type StyleMode,
  type GradientPreset,
  type AppearanceSettings,
} from "../lib/appearance";

const TABS = [
  "Appearance",
  "Profile",
  "Providers",
  "Audio & Voice",
  "Computer Use",
  "Skills",
  "Integrations",
  "Usage",
  "Research",
  "About",
  "Ticker",
  "Automation",
  "Mind",
  "Publishing",
] as const;


export type SettingsTab = (typeof TABS)[number];

export function SettingsModal({
  status,
  initialTab = "Profile",
  onClose,
  onRefresh,
}: {
  status: AppStatus | null;
  initialTab?: SettingsTab;
  onClose: () => void;
  onRefresh: () => void;
}) {
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const [profile, setProfile] = useState<UserProfile>(getProfile());

  useEffect(() => {
    return onProfileChange(setProfile);
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const initials =
    profile.name
      .split(" ")
      .map((w) => w[0])
      .filter(Boolean)
      .slice(0, 2)
      .join("")
      .toUpperCase() || "D";

  return (
    <div className="modal-overlay" onClick={onClose} role="presentation">
      <div
        className="modal settings-fullscreen-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        onClick={(event) => event.stopPropagation()}
      >
        {/* Top Header Bar with Go Back Button */}
        <header className="modal-top-bar">
          <div className="modal-top-left">
            <button
              type="button"
              className="modal-back-btn"
              onClick={onClose}
              title="Return to Workspace (Esc)"
              aria-label="Go Back to Workspace"
            >
              <IconArrowLeft size={16} />
              <span>Back to Workspace</span>
            </button>
            <span className="modal-breadcrumb-sep">/</span>
            <span className="modal-top-title">{tab}</span>
          </div>

          <div className="modal-top-right">
            <div
              className="modal-top-profile-chip"
              onClick={() => setTab("Profile")}
              role="button"
              tabIndex={0}
              title="View Profile & Lifetime Status"
            >
              <div className="chip-avatar-wrap">
                {profile.avatarUrl ? (
                  <img src={profile.avatarUrl} alt={profile.name} className="chip-avatar-img" />
                ) : (
                  <span className="chip-avatar-fallback">{initials}</span>
                )}
                <span className="chip-crown-badge">
                  <IconCrown size={10} />
                </span>
              </div>
              <span className="chip-user-name">{profile.name}</span>
              <span className="chip-lifetime-tag">
                <IconCrown size={10} />
                <span>Lifetime</span>
              </span>
            </div>

            <button
              className="icon-btn modal-close-btn"
              onClick={onClose}
              aria-label="Close settings"
              title="Close Settings (Esc)"
            >
              <IconClose size={15} />
            </button>
          </div>
        </header>

        {/* 2-Column Split: Nav Rail on Left, Active Tab Content on Right */}
        <div className="modal-content-split">
          <nav className="modal-rail" aria-label="Settings sections">
            {TABS.map((entry) => (
              <button
                key={entry}
                className={`modal-tab${tab === entry ? " active" : ""}`}
                onClick={() => setTab(entry)}
              >
                {entry === "Profile" ? <IconUser size={14} /> : null}
                {entry === "About" ? <IconCrown size={14} /> : null}
                {entry === "Audio & Voice" ? <IconMic size={14} /> : null}
                <span>{entry}</span>
                {entry === "Profile" ? <span className="tab-crown-pill"><IconCrown size={11} /></span> : null}
              </button>
            ))}
            <span style={{ flex: 1 }} />
            <div className="modal-rail-footer">
              <span className="rail-version">bhippi v{status?.version ?? "0.1.0"}</span>
              <span className="rail-plan">{profile.plan}</span>
            </div>
          </nav>

          <div className="modal-body">
            {tab === "Profile" ? <ProfileTab onRefresh={onRefresh} /> : null}
            {tab === "About" ? <AboutTab status={status} /> : null}
            {tab === "Providers" ? <ProvidersTab status={status} onRefresh={onRefresh} /> : null}
            {tab === "Audio & Voice" ? <AudioTab /> : null}
            {tab === "Appearance" ? <AppearanceTab /> : null}
            {tab === "Computer Use" ? <ComputerUseTab /> : null}
            {tab === "Skills" ? <SkillsTab /> : null}
            {tab === "Integrations" ? <IntegrationsTab /> : null}
            {tab === "Usage" ? <UsagePanel /> : null}
            {tab === "Research" ? <ResearchTab /> : null}
            {tab !== "Profile" &&
            tab !== "About" &&
            tab !== "Providers" &&
            tab !== "Audio & Voice" &&
            tab !== "Appearance" &&
            tab !== "Computer Use" &&
            tab !== "Skills" &&
            tab !== "Integrations" &&
            tab !== "Usage" &&
            tab !== "Research" ? (
              <PlaceholderTab tab={tab} />
            ) : null}

            <footer
              className="statusbar"
              style={{ position: "sticky", bottom: -24, margin: "24px -32px -24px" }}
            >
              <span>bhippi v{status?.version ?? "?"}</span>
              <span className="spacer" />
              <button disabled title="Lands with the hardening sprint">
                Run doctor
              </button>
            </footer>
          </div>
        </div>
      </div>
    </div>
  );
}

type ColorScheme =
  | "dark"
  | "light"
  | "frosted-glass"
  | "sapphire"
  | "emerald"
  | "cyberpunk"
  | "amethyst"
  | "crimson"
  | "glacier"
  | "titanium"
  | "slate"
  | "paper"
  | "contrast"
  | "system"
  | "gradient"
  | "transparent";

const COLOR_SCHEMES: Array<{
  id: ColorScheme;
  label: string;
  badge: string;
  desc: string;
}> = [
  { id: "dark", label: "Classic Amber", badge: "Default", desc: "Warm obsidian canvas & amber glow" },
  { id: "gradient", label: "Gradient Flux", badge: "Neon", desc: "Animated three.js-style gradient with glass" },
  { id: "frosted-glass", label: "Frosted Glass", badge: "Translucent", desc: "Ultra-sleek glassmorphic blur with cyan ice" },
  { id: "transparent", label: "Transparent", badge: "Image BG", desc: "True transparent glass with your own image" },
  { id: "sapphire", label: "Midnight Sapphire", badge: "Vibrant", desc: "Deep cobalt indigo with electric blue" },
  { id: "emerald", label: "Emerald Obsidian", badge: "Matrix", desc: "Deep forest carbon with luminous jade" },
  { id: "cyberpunk", label: "Cyberpunk Neon", badge: "Tokyo", desc: "Dark violet night with electric magenta" },
  { id: "amethyst", label: "Amethyst Void", badge: "Velvet", desc: "Deep velvet purple with radiant lavender" },
  { id: "crimson", label: "Solar Crimson", badge: "Volcanic", desc: "Dark graphite with volcanic ruby sunset" },
  { id: "glacier", label: "Nordic Glacier", badge: "Polar", desc: "Deep arctic steel with luminous teal cyan" },
  { id: "titanium", label: "Monochrome Titanium", badge: "OLED", desc: "Pure OLED zero-black with titanium white" },
  { id: "slate", label: "Slate Ash", badge: "Neutral", desc: "True graphite grey with soft mint accent" },
  { id: "light", label: "Linen Cream", badge: "Light", desc: "Clean editorial linen with warm amber" },
  { id: "paper", label: "Paper", badge: "Reading", desc: "Softened warm light for long reading sessions" },
  { id: "contrast", label: "High Contrast", badge: "Access", desc: "WCAG AAA pairs throughout on pure black" },
  { id: "system", label: "System Auto", badge: "OS", desc: "Automatically matches your operating system" },
];

function AppearanceTab() {
  const [settings, setSettings] = useState<AppearanceSettings>(() => getAppearanceSettings());
  const [savedFeedback, setSavedFeedback] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [activeTabSub, setActiveTabSub] = useState<"wallpapers" | "optics">("wallpapers");

  // Keep local state in sync with external changes
  useEffect(() => {
    return onAppearanceChange((newSettings) => {
      setSettings(newSettings);
    });
  }, []);

  const update = (patch: Partial<AppearanceSettings>, immediate = false) => {
    const next = { ...settings, ...patch };
    setSettings(next);
    saveAppearanceSettings(next, immediate);
  };

  const handleModeChange = (mode: StyleMode) => {
    update({ styleMode: mode }, true);
  };

  const handleUploadCustom = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const fileList = e.target.files;
    if (!fileList || fileList.length === 0) return;
    setUploading(true);

    const processFile = (file: File): Promise<{ name: string; dataUrl: string }> => {
      return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => {
          const rawDataUrl = reader.result as string;
          const img = new Image();
          img.onload = () => {
            const maxDim = 1400;
            let w = img.width;
            let h = img.height;
            if (w > maxDim || h > maxDim) {
              if (w > h) {
                h = Math.round((h * maxDim) / w);
                w = maxDim;
              } else {
                w = Math.round((w * maxDim) / h);
                h = maxDim;
              }
            }
            const canvas = document.createElement("canvas");
            canvas.width = w;
            canvas.height = h;
            const ctx = canvas.getContext("2d");
            if (ctx) {
              ctx.drawImage(img, 0, 0, w, h);
              const compressed = canvas.toDataURL("image/jpeg", 0.8);
              resolve({ name: file.name.replace(/\.[^/.]+$/, ""), dataUrl: compressed });
            } else {
              resolve({ name: file.name.replace(/\.[^/.]+$/, ""), dataUrl: rawDataUrl });
            }
          };
          img.onerror = reject;
          img.src = rawDataUrl;
        };
        reader.onerror = reject;
        reader.readAsDataURL(file);
      });
    };

    try {
      const results: { name: string; dataUrl: string }[] = [];
      for (let i = 0; i < fileList.length; i++) {
        try {
          const res = await processFile(fileList[i]);
          results.push(res);
        } catch (err) {
          console.warn("Failed processing image", fileList[i].name, err);
        }
      }
      if (results.length > 0) {
        addCustomWallpapers(results);
        setSettings(getAppearanceSettings());
      }
    } finally {
      setUploading(false);
      e.target.value = "";
    }
  };

  const handleDeleteCustom = (id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    deleteCustomWallpaper(id);
    setSettings(getAppearanceSettings());
  };

  const handleSelectWallpaper = (id: string) => {
    update({ activeWallpaperId: id });
  };

  const handleResetDefaults = () => {
    const defaults = {
      ...settings,
      styleMode: "glass" as StyleMode,
      colorScheme: "dark",
      activeWallpaperId: "preset-ember-glow",
      wallpaperDim: 26,
      glassBlur: 22,
      glassOpacity: 0.52,
      glassSaturation: 128,
      gradientPreset: "sunset" as GradientPreset,
      animatedGradient: false,
    };
    setSettings(defaults);
    saveAppearanceSettings(defaults);
  };

  const handleManualSave = () => {
    saveAppearanceSettings(settings);
    setSavedFeedback(true);
    setTimeout(() => setSavedFeedback(false), 2200);
  };

  return (
    <div className="appearance-container">
      <div className="appearance-head">
        <div>
          <h2 className="settings-heading" style={{ margin: 0 }}>Appearance &amp; Workspace Style</h2>
          <p className="settings-note" style={{ margin: "4px 0 0" }}>
            Choose between 3 core workspace modes: <strong>Max</strong> (solid minimal), <strong>Plan</strong> (vibrant gradients), or <strong>Glass</strong> (full-app frosted transparency with custom wallpapers).
          </p>
        </div>
        <div className="appearance-head-actions">
          {savedFeedback ? (
            <span className="appearance-saved-pill">
              <IconCheck size={12} /> Saved!
            </span>
          ) : (
            <button type="button" className="btn-appearance-save" onClick={handleManualSave}>
              Save Style
            </button>
          )}
          <button type="button" className="btn-appearance-reset" onClick={handleResetDefaults} title="Reset to defaults">
            <IconRefresh size={12} />
          </button>
        </div>
      </div>

      {/* ── 1. The Three Core Modes ────────────────────────────────────────── */}
      <section className="settings-section">
        <div className="appearance-section-title">
          <span>1. Workspace Mode</span>
          <span className="mode-active-pill">Active: {settings.styleMode.toUpperCase()}</span>
        </div>

        <div className="style-modes-grid" role="radiogroup" aria-label="Style mode">
          {/* Mode: Max */}
          <div
            className={`style-mode-card${settings.styleMode === "max" ? " active" : ""}`}
            onClick={() => handleModeChange("max")}
            role="radio"
            aria-checked={settings.styleMode === "max"}
          >
            <div className="mode-card-header">
              <span className="mode-badge solid">SOLID</span>
              <span className="mode-icon-box max-icon">■</span>
            </div>
            <div className="mode-card-title">Max (Clean Solid)</div>
            <div className="mode-card-desc">
              Pure flat surfaces without gradients or blur. High contrast, distraction-free, and lowest CPU/GPU overhead.
            </div>
            <div className="mode-card-sample max-sample">
              <div className="sample-bar" />
              <div className="sample-box" />
            </div>
          </div>

          {/* Mode: Plan */}
          <div
            className={`style-mode-card${settings.styleMode === "plan" ? " active" : ""}`}
            onClick={() => handleModeChange("plan")}
            role="radio"
            aria-checked={settings.styleMode === "plan"}
          >
            <div className="mode-card-header">
              <span className="mode-badge gradient">GRADIENT</span>
              <IconSparkles size={14} className="mode-icon-accent" />
            </div>
            <div className="mode-card-title">Plan (Gradient Flux)</div>
            <div className="mode-card-desc">
              Vibrant multi-stop gradients, glowing card borders, and sleek modern ambient color meshes that look cool.
            </div>
            <div className="mode-card-sample plan-sample">
              <div className="sample-bar-gradient" />
              <div className="sample-box-gradient" />
            </div>
          </div>

          {/* Mode: Glass */}
          <div
            className={`style-mode-card${settings.styleMode === "glass" ? " active" : ""}`}
            onClick={() => handleModeChange("glass")}
            role="radio"
            aria-checked={settings.styleMode === "glass"}
          >
            <div className="mode-card-header">
              <span className="mode-badge glass">TRANSPARENT</span>
              <span className="mode-icon-box glass-icon">⬡</span>
            </div>
            <div className="mode-card-title">Glass (Frosted Transparency)</div>
            <div className="mode-card-desc">
              Full-app frosted-glass transparency. All panels blur over custom background wallpapers and images.
            </div>
            <div className="mode-card-sample glass-sample">
              <div className="sample-glass-inner" />
            </div>
          </div>
        </div>
      </section>

      {/* ── 2. Glass Mode Options (Wallpapers, Dim & Optics) ─────────────── */}
      {settings.styleMode === "glass" && (
        <section className="settings-section glass-options-section">
          <div className="appearance-tabs-bar">
            <button
              type="button"
              className={`appearance-subtab${activeTabSub === "wallpapers" ? " active" : ""}`}
              onClick={() => setActiveTabSub("wallpapers")}
            >
              Background Wallpapers ({settings.customWallpapers.length} Custom)
            </button>
            <button
              type="button"
              className={`appearance-subtab${activeTabSub === "optics" ? " active" : ""}`}
              onClick={() => setActiveTabSub("optics")}
            >
              Frosted Optics &amp; Readability
            </button>
          </div>

          {activeTabSub === "wallpapers" ? (
            <div className="wallpapers-manager">
              {/* Custom Uploads Header */}
              <div className="wallpaper-group-header">
                <div>
                  <strong>Custom Wallpapers</strong>
                  <span className="group-hint">Saved locally in workspace storage</span>
                </div>
                <label className="btn-upload-wallpaper" title="Upload custom wallpaper image">
                  <IconPlus size={13} />
                  <span>{uploading ? "Compressing…" : "Upload Images"}</span>
                  <input
                    type="file"
                    multiple
                    accept="image/*"
                    style={{ display: "none" }}
                    disabled={uploading}
                    onChange={handleUploadCustom}
                  />
                </label>
              </div>

              {/* Custom Images Grid */}
              <div className="wallpapers-grid">
                {settings.customWallpapers.map((wp) => {
                  const isActive = settings.activeWallpaperId === wp.id;
                  return (
                    <div
                      key={wp.id}
                      className={`wallpaper-card custom${isActive ? " active" : ""}`}
                      style={{ backgroundImage: `url(${wp.url})` }}
                      onClick={() => handleSelectWallpaper(wp.id)}
                    >
                      <div className="wallpaper-card-top">
                        {isActive ? (
                          <span className="wallpaper-active-badge">
                            <IconCheck size={10} /> Active
                          </span>
                        ) : <span />}
                        <button
                          type="button"
                          className="btn-delete-wallpaper"
                          onClick={(e) => handleDeleteCustom(wp.id, e)}
                          title="Delete this custom wallpaper"
                        >
                          <IconTrash size={12} />
                        </button>
                      </div>
                      <div className="wallpaper-card-name" title={wp.name}>
                        {wp.name}
                      </div>
                    </div>
                  );
                })}

                {settings.customWallpapers.length === 0 && (
                  <label className="wallpaper-empty-card" title="Click to upload custom background image">
                    <IconPlus size={20} />
                    <span>Upload Your Wallpaper</span>
                    <span className="sub-hint">PNG, JPG, WebP supported</span>
                    <input
                      type="file"
                      multiple
                      accept="image/*"
                      style={{ display: "none" }}
                      disabled={uploading}
                      onChange={handleUploadCustom}
                    />
                  </label>
                )}
              </div>

              {/* Presets Group */}
              <div className="wallpaper-group-header" style={{ marginTop: 20 }}>
                <div>
                  <strong>Atmospheric Presets</strong>
                  <span className="group-hint">Curated dark and neon glass themes</span>
                </div>
              </div>

              <div className="wallpapers-grid">
                {PRESET_WALLPAPERS.map((wp) => {
                  const isActive = settings.activeWallpaperId === wp.id;
                  return (
                    <div
                      key={wp.id}
                      className={`wallpaper-card${isActive ? " active" : ""}`}
                      style={{ background: wp.url }}
                      onClick={() => handleSelectWallpaper(wp.id)}
                    >
                      <div className="wallpaper-card-top">
                        {isActive && (
                          <span className="wallpaper-active-badge">
                            <IconCheck size={10} /> Active
                          </span>
                        )}
                      </div>
                      <div className="wallpaper-card-name">{wp.name}</div>
                    </div>
                  );
                })}
              </div>

              {/* Readability Dim Slider */}
              <div className="wallpaper-dim-box">
                <div className="glass-slider-label">
                  <div>
                    <strong>Wallpaper Readability Overlay</strong>
                    <div className="slider-subtext">Darkens the background layer so text, code, and chat stay sharp &amp; 100% legible</div>
                  </div>
                  <span className="glass-value">{settings.wallpaperDim}% Darkened</span>
                </div>
                <input
                  type="range"
                  min="0"
                  max="90"
                  value={settings.wallpaperDim}
                  onChange={(e) => update({ wallpaperDim: Number(e.target.value) })}
                  aria-label="Wallpaper dim overlay"
                />
              </div>
            </div>
          ) : (
            /* Optics Tuning */
            <div className="optics-manager">
              <div className="glass-slider-row">
                <div className="glass-slider-label">
                  <div>
                    <strong>Glass Blur</strong>
                    <div className="slider-subtext">Depth of frosted backdrop filter</div>
                  </div>
                  <span className="glass-value">{settings.glassBlur}px</span>
                </div>
                <input
                  type="range"
                  min="0"
                  max="60"
                  value={settings.glassBlur}
                  onChange={(e) => update({ glassBlur: Number(e.target.value) })}
                  aria-label="Glass blur"
                />
              </div>

              <div className="glass-slider-row">
                <div className="glass-slider-label">
                  <div>
                    <strong>Glass Transparency &amp; Density</strong>
                    <div className="slider-subtext">Lower values make panels more transparent; higher values make them solid</div>
                  </div>
                  <span className="glass-value">{Math.round(settings.glassOpacity * 100)}%</span>
                </div>
                <input
                  type="range"
                  min="20"
                  max="95"
                  value={Math.round(settings.glassOpacity * 100)}
                  onChange={(e) => update({ glassOpacity: Number(e.target.value) / 100 })}
                  aria-label="Glass opacity"
                />
              </div>

              <div className="glass-slider-row">
                <div className="glass-slider-label">
                  <div>
                    <strong>Color Saturation Lift</strong>
                    <div className="slider-subtext">Vibrancy of colors seen through the frosted glass panels</div>
                  </div>
                  <span className="glass-value">{settings.glassSaturation}%</span>
                </div>
                <input
                  type="range"
                  min="100"
                  max="240"
                  value={settings.glassSaturation}
                  onChange={(e) => update({ glassSaturation: Number(e.target.value) })}
                  aria-label="Glass saturation"
                />
              </div>
            </div>
          )}
        </section>
      )}

      {/* ── 3. Plan Mode Options (Gradient Presets) ─────────────────────────── */}
      {settings.styleMode === "plan" && (
        <section className="settings-section plan-options-section">
          <div className="appearance-section-title">
            <span>2. Gradient Palettes</span>
            <span className="group-hint">Select your ambient gradient profile</span>
          </div>

          <div className="gradient-presets-grid">
            {(Object.keys(GRADIENT_PRESETS) as GradientPreset[]).map((key) => {
              const preset = GRADIENT_PRESETS[key];
              const isActive = settings.gradientPreset === key;
              return (
                <div
                  key={key}
                  className={`gradient-preset-card${isActive ? " active" : ""}`}
                  onClick={() => update({ gradientPreset: key })}
                >
                  <div className="preset-swatch-bar" style={{ background: preset.gradient }} />
                  <div className="preset-info">
                    <span className="preset-name">{preset.name}</span>
                    {isActive && (
                      <span className="preset-active-check">
                        <IconCheck size={11} />
                      </span>
                    )}
                  </div>
                </div>
              );
            })}
          </div>

          <div className="gradient-toggle-row">
            <label className="custom-checkbox-row">
              <input
                type="checkbox"
                checked={settings.animatedGradient}
                onChange={(e) => update({ animatedGradient: e.target.checked })}
              />
              <div>
                <strong>Animated Flux Mesh</strong>
                <div className="slider-subtext">Gently animate the gradient coordinates across the backdrop</div>
              </div>
            </label>
          </div>

          <div className="plan-breathing-banner">
            <span className="pulsing-beacon" />
            <div>
              <strong>Living Gradient Breathing is Active</strong>
              <div className="slider-subtext">The backdrop gradient continually breathes, scales, and pulses ambient hue shifts across the workspace.</div>
            </div>
          </div>
        </section>
      )}

      {/* ── 4. Max Mode Options (Solid Clean Palettes) ─────────────────────── */}
      {settings.styleMode === "max" && (
        <section className="settings-section max-options-section">
          <div className="appearance-section-title">
            <span>2. Solid Color Scheme</span>
            <span className="group-hint">Pure flat theme with zero gradient rendering</span>
          </div>

          <div className="slash-picker" role="radiogroup" aria-label="Solid theme picker">
            {COLOR_SCHEMES.filter((c) => c.id !== "gradient" && c.id !== "frosted-glass" && c.id !== "transparent").map((entry) => {
              const isSelected = settings.colorScheme === entry.id;
              return (
                <button
                  key={entry.id}
                  type="button"
                  role="radio"
                  aria-checked={isSelected}
                  className={`slash-pill ${entry.id}${isSelected ? " active" : ""}`}
                  onClick={() => {
                    update({ colorScheme: entry.id });
                    document.documentElement.dataset.colorScheme = entry.id;
                  }}
                  title={`${entry.label} — ${entry.desc}`}
                >
                  <span className="slash-pill-inner" />
                  <span className="slash-pill-label">{entry.label}</span>
                </button>
              );
            })}
          </div>
        </section>
      )}

      {/* ── 5. Active Palette Preview Swatches ─────────────────────────────── */}
      <section className="settings-section appearance-sample">
        <div className="appearance-section-title">
          <span>Active Design Tokens</span>
        </div>
        <div className="appearance-swatches" aria-label="Active theme swatches">
          <span title="Primary Accent" />
          <span title="Base Background" />
          <span title="Surface Layer" />
          <span title="Secondary Surface" />
          <span title="Status OK" />
        </div>
      </section>
    </div>
  );
}

const AUDIO_LANGUAGES = [
  { id: "en", label: "English (US / Global)" },
  { id: "en-GB", label: "English (UK)" },
  { id: "es", label: "Spanish (Español)" },
  { id: "fr", label: "French (Français)" },
  { id: "de", label: "German (Deutsch)" },
  { id: "ja", label: "Japanese (日本語)" },
  { id: "zh", label: "Chinese (中文)" },
  { id: "auto", label: "Auto-Detect Language" },
];

function AudioTab() {
  const [settings, setSettings] = useState<AudioSettings>(getAudioSettings());
  const [revealed, setRevealed] = useState<Record<string, boolean>>({});
  const [testing, setTesting] = useState<Record<string, boolean>>({});
  const [testResult, setTestResult] = useState<Record<string, { success: boolean; message: string } | null>>({});
  const [savedToast, setSavedToast] = useState(false);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  useEffect(() => {
    return onAudioSettingsChange(setSettings);
  }, []);

  const updateProvider = (id: AudioProviderId, partial: Partial<AudioProviderConfig>) => {
    setSettings((prev) => {
      const nextProviders = {
        ...prev.providers,
        [id]: { ...prev.providers[id], ...partial },
      };
      const nextSettings: AudioSettings = { ...prev, providers: nextProviders };
      saveAudioSettings(nextSettings);
      return nextSettings;
    });
  };

  const setSttProvider = (id: AudioProviderId) => {
    setSettings((prev) => {
      const next = { ...prev, activeSttProvider: id };
      saveAudioSettings(next);
      return next;
    });
  };

  const setLanguage = (language: string) => {
    setSettings((prev) => {
      const next = { ...prev, language };
      saveAudioSettings(next);
      return next;
    });
  };

  const setAutoPunctuation = (autoPunctuation: boolean) => {
    setSettings((prev) => {
      const next = { ...prev, autoPunctuation };
      saveAudioSettings(next);
      return next;
    });
  };

  const handleSaveAll = () => {
    saveAudioSettings(settings);
    setSavedToast(true);
    setTimeout(() => setSavedToast(false), 2500);
  };

  const handleResetDefaults = () => {
    if (window.confirm("Reset all audio API configurations to default? Existing stored keys will be cleared.")) {
      setSettings(DEFAULT_AUDIO_SETTINGS);
      saveAudioSettings(DEFAULT_AUDIO_SETTINGS);
      setTestResult({});
      setSavedToast(true);
      setTimeout(() => setSavedToast(false), 2500);
    }
  };

  const testProvider = async (id: AudioProviderId) => {
    const config = settings.providers[id];
    if (!config) return;
    setTesting((prev) => ({ ...prev, [id]: true }));
    setTestResult((prev) => ({ ...prev, [id]: null }));
    try {
      const res = await testAudioProvider(id, config.apiKey, config.baseUrl);
      setTestResult((prev) => ({ ...prev, [id]: res }));
    } catch (err: any) {
      setTestResult((prev) => ({
        ...prev,
        [id]: { success: false, message: err?.message || "Connection test failed." },
      }));
    } finally {
      setTesting((prev) => ({ ...prev, [id]: false }));
    }
  };

  const copyKey = async (id: string, key: string) => {
    if (!key) return;
    try {
      await navigator.clipboard.writeText(key);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    } catch {
      // ignore
    }
  };

  const providerList = Object.values(settings.providers);
  const activeConfig = settings.providers[settings.activeSttProvider];

  return (
    <div className="audio-settings-content">
      <div className="audio-settings-header">
        <div>
          <h2 className="settings-heading" style={{ margin: 0 }}>
            Audio &amp; Voice APIs
          </h2>
          <p className="settings-note" style={{ margin: "4px 0 0" }}>
            Connect cloud and local speech-to-text and voice generation engines. Keys are stored
            securely on this machine and never leave your local workspace.
          </p>
        </div>
        <div className="audio-header-actions">
          <button
            type="button"
            className={`btn-primary audio-save-btn${savedToast ? " saved" : ""}`}
            onClick={handleSaveAll}
          >
            <IconCheck size={13} />
            <span>{savedToast ? "Saved to Workspace!" : "Save Audio Config"}</span>
          </button>
        </div>
      </div>

      {/* Global Voice Input Configuration */}
      <section className="settings-section audio-global-section">
        <h3 className="settings-heading">Voice Input (Chat Microphone)</h3>
        <p className="settings-note">
          Select which engine transcribes your voice when you click the mic button in the chat area.
        </p>

        <div className="audio-global-card">
          <div className="audio-global-row">
            <div className="audio-field-col">
              <label className="audio-field-label">Active Speech-to-Text Engine</label>
              <select
                className="audio-select-input"
                value={settings.activeSttProvider}
                onChange={(e) => setSttProvider(e.target.value as AudioProviderId)}
              >
                <option value="deepgram">Deepgram (Nova-3 / Nova-2) — Recommended</option>
                <option value="openai">OpenAI Audio (Whisper-1)</option>
                <option value="groq">Groq Whisper (Ultra-fast Turbo)</option>
                <option value="web_speech">Web Speech API (Browser Native · No API Key)</option>
                <option value="assemblyai">AssemblyAI (Best / Nano)</option>
                <option value="custom">Custom Endpoint (Self-Hosted / Proxy)</option>
              </select>
            </div>

            <div className="audio-field-col">
              <label className="audio-field-label">Default Recognition Language</label>
              <select
                className="audio-select-input"
                value={settings.language}
                onChange={(e) => setLanguage(e.target.value)}
              >
                {AUDIO_LANGUAGES.map((lang) => (
                  <option key={lang.id} value={lang.id}>
                    {lang.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="audio-global-meta">
            <div className="audio-meta-left">
              <span className="audio-engine-chip active">
                <IconMic size={12} />
                <span>Active STT: {activeConfig?.name ?? settings.activeSttProvider}</span>
              </span>
              <span className="audio-engine-chip">
                <span>Model: {activeConfig?.model || "default"}</span>
              </span>
            </div>

            <label className="audio-checkbox-label">
              <input
                type="checkbox"
                checked={settings.autoPunctuation}
                onChange={(e) => setAutoPunctuation(e.target.checked)}
              />
              <span>Smart Punctuation &amp; Capitalization</span>
            </label>
          </div>
        </div>
      </section>

      {/* Provider API Key Cards */}
      <section className="settings-section">
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 }}>
          <div>
            <h3 className="settings-heading" style={{ margin: 0 }}>
              Configured Voice Providers
            </h3>
            <p className="settings-note" style={{ margin: "3px 0 0" }}>
              Provide API keys for services you wish to use. Turn switches on to make them selectable.
            </p>
          </div>
          <button
            type="button"
            className="btn-text-action"
            onClick={handleResetDefaults}
            title="Reset audio settings to factory defaults"
          >
            Reset Defaults
          </button>
        </div>

        <div className="audio-providers-grid">
          {providerList.map((p) => {
            const isRevealed = Boolean(revealed[p.id]);
            const isCopied = copiedId === p.id;
            const isTesting = Boolean(testing[p.id]);
            const result = testResult[p.id];
            const isActiveStt = settings.activeSttProvider === p.id;
            const hasKey = Boolean(p.apiKey && p.apiKey.trim().length > 0);
            const isWebSpeech = p.id === "web_speech";

            return (
              <div
                key={p.id}
                className={`audio-provider-card${p.enabled ? " enabled" : ""}${isActiveStt ? " active-stt" : ""}`}
              >
                <div className="audio-card-head">
                  <div className="audio-card-identity">
                    <div className="audio-card-avatar">
                      {p.category === "tts" ? (
                        <IconVolume size={16} />
                      ) : p.category === "both" ? (
                        <IconWaveform size={16} />
                      ) : (
                        <IconMic size={16} />
                      )}
                    </div>
                    <div>
                      <div className="audio-card-title-row">
                        <strong className="audio-provider-name">{p.name}</strong>
                        <span className={`audio-category-badge ${p.category}`}>
                          {p.category === "both"
                            ? "STT + TTS"
                            : p.category === "stt"
                              ? "Speech-to-Text"
                              : "Voice Synthesis"}
                        </span>
                        {isActiveStt ? (
                          <span className="audio-active-badge">
                            <IconCheck size={11} />
                            <span>Active Mic</span>
                          </span>
                        ) : null}
                      </div>
                      <p className="audio-provider-desc">{p.description}</p>
                    </div>
                  </div>

                  <div className="audio-card-head-controls">
                    <Toggle
                      checked={p.enabled}
                      onChange={(checked) => updateProvider(p.id, { enabled: checked })}
                      label={`Enable ${p.name}`}
                    />
                  </div>
                </div>

                {!isWebSpeech ? (
                  <div className="audio-card-body">
                    {/* API Key Input Row */}
                    <div className="audio-input-block">
                      <label className="audio-input-caption">
                        <span>API Key</span>
                        {hasKey ? (
                          <span className="audio-key-status ok">Key Configured</span>
                        ) : (
                          <span className="audio-key-status missing">No Key Entered</span>
                        )}
                      </label>
                      <div className="audio-key-bar">
                        <input
                          type={isRevealed ? "text" : "password"}
                          className="audio-key-input"
                          placeholder={
                            p.id === "deepgram"
                              ? "dg_..."
                              : p.id === "openai"
                                ? "sk-..."
                                : p.id === "groq"
                                  ? "gsk_..."
                                  : p.id === "elevenlabs"
                                    ? "xi_..."
                                    : "Enter API key…"
                          }
                          value={p.apiKey}
                          onChange={(e) => updateProvider(p.id, { apiKey: e.target.value })}
                          autoComplete="off"
                          spellCheck={false}
                        />
                        <button
                          type="button"
                          className="audio-icon-btn"
                          onClick={() =>
                            setRevealed((prev) => ({ ...prev, [p.id]: !prev[p.id] }))
                          }
                          title={isRevealed ? "Hide API key" : "Reveal API key"}
                        >
                          {isRevealed ? <IconEyeOff size={13} /> : <IconEye size={13} />}
                        </button>
                        <button
                          type="button"
                          className={`audio-icon-btn${isCopied ? " copied" : ""}`}
                          onClick={() => copyKey(p.id, p.apiKey)}
                          title="Copy API key"
                          disabled={!hasKey}
                        >
                          {isCopied ? <IconCheck size={13} /> : <IconCopy size={13} />}
                        </button>
                        {hasKey ? (
                          <button
                            type="button"
                            className="audio-icon-btn text-danger"
                            onClick={() => updateProvider(p.id, { apiKey: "" })}
                            title="Clear API key"
                          >
                            <IconTrash size={13} />
                          </button>
                        ) : null}
                      </div>
                    </div>

                    {/* Model & Base URL options */}
                    <div className="audio-model-row">
                      <div className="audio-model-col">
                        <label className="audio-input-caption">Model Selection</label>
                        {p.supportedModels.length > 1 ? (
                          <select
                            className="audio-select-input sm"
                            value={p.model}
                            onChange={(e) => updateProvider(p.id, { model: e.target.value })}
                          >
                            {p.supportedModels.map((m) => (
                              <option key={m} value={m}>
                                {m} {m === p.defaultModel ? "(Default)" : ""}
                              </option>
                            ))}
                          </select>
                        ) : (
                          <input
                            type="text"
                            className="audio-text-input sm"
                            value={p.model}
                            onChange={(e) => updateProvider(p.id, { model: e.target.value })}
                            placeholder={p.defaultModel}
                          />
                        )}
                      </div>

                      {(p.id === "custom" || p.id === "openai" || p.id === "groq") ? (
                        <div className="audio-model-col" style={{ flex: 1.5 }}>
                          <label className="audio-input-caption">API Base URL</label>
                          <input
                            type="text"
                            className="audio-text-input sm"
                            value={p.baseUrl}
                            onChange={(e) => updateProvider(p.id, { baseUrl: e.target.value })}
                            placeholder="https://..."
                          />
                        </div>
                      ) : null}
                    </div>

                    {/* Action & Verification footer */}
                    <div className="audio-card-footer">
                      <div className="audio-footer-left">
                        <button
                          type="button"
                          className="audio-test-btn"
                          onClick={() => void testProvider(p.id)}
                          disabled={isTesting}
                        >
                          <IconRefresh size={11} className={isTesting ? "spin" : ""} />
                          <span>{isTesting ? "Testing…" : "Test Key Connection"}</span>
                        </button>

                        {result ? (
                          <span className={`audio-test-status ${result.success ? "success" : "failed"}`}>
                            {result.success ? <IconCheck size={11} /> : null}
                            <span>{result.message}</span>
                          </span>
                        ) : null}
                      </div>

                      {p.docsUrl ? (
                        <a
                          href={p.docsUrl}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="audio-docs-link"
                          title={`Open ${p.name} Developer Console`}
                        >
                          <span>Get {p.name} Key</span>
                          <IconExternal size={11} />
                        </a>
                      ) : null}
                    </div>
                  </div>
                ) : (
                  /* Web Speech API Card Body */
                  <div className="audio-card-body">
                    <p className="settings-note" style={{ margin: "4px 0 10px" }}>
                      Browser-native speech recognition uses Chromium / system speech recognition
                      services without sending audio data to 3rd-party servers.
                    </p>
                    <div className="audio-card-footer">
                      <button
                        type="button"
                        className="audio-test-btn"
                        onClick={() => void testProvider("web_speech")}
                        disabled={isTesting}
                      >
                        <IconRefresh size={11} className={isTesting ? "spin" : ""} />
                        <span>{isTesting ? "Testing…" : "Check Browser Speech Support"}</span>
                      </button>
                      {result ? (
                        <span className={`audio-test-status ${result.success ? "success" : "failed"}`}>
                          {result.success ? <IconCheck size={11} /> : null}
                          <span>{result.message}</span>
                        </span>
                      ) : null}
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </section>
    </div>
  );
}

function ComputerUseTab() {
  const [status, setStatus] = useState<ComputerUseStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [preview, setPreview] = useState<ScreenCapture | null>(null);
  const [testing, setTesting] = useState(false);
  const [actionLog, setActionLog] = useState<string | null>(null);

  const load = () => {
    void api.computerUseStatus().then(setStatus).catch(() => setStatus(null));
  };

  useEffect(() => {
    load();
  }, []);

  const toggleEnabled = async (enabled: boolean) => {
    setSaving(true);
    try {
      await api.setComputerUseEnabled(enabled);
      load();
    } finally {
      setSaving(false);
    }
  };

  const toggleFullAccess = async (fullAccess: boolean) => {
    setSaving(true);
    try {
      await api.setComputerUseFullAccess(fullAccess);
      load();
    } finally {
      setSaving(false);
    }
  };

  const capturePreview = async () => {
    setTesting(true);
    setActionLog(null);
    try {
      const cap = await api.captureScreenPreview();
      setPreview(cap);
      setActionLog(`Captured screen preview: ${cap.width} × ${cap.height} px`);
    } catch (error) {
      setActionLog(typeof error === "string" ? error : "Failed to capture preview.");
    } finally {
      setTesting(false);
    }
  };

  const testCursor = async () => {
    setTesting(true);
    setActionLog(null);
    try {
      const res = await api.executeComputerAction({ type: "get_cursor_position" });
      setActionLog(res.detail);
    } catch (error) {
      setActionLog(typeof error === "string" ? error : "Cursor test failed.");
    } finally {
      setTesting(false);
    }
  };

  return (
    <>
      <h2 className="settings-heading">Computer Use</h2>
      <p className="settings-note">
        Empower AI agents to view your desktop display and autonomously perform computer tasks
        (mouse moves, clicks, drag-and-drop, typing, and shortcuts).
      </p>

      <div className="computer-use-notice">
        <strong>
          <IconCode size={14} /> Supported Vision Providers Only
        </strong>
        <p>
          Computer Use requires high-level multimodal visual reasoning. It is enabled exclusively
          for <strong>Claude Code</strong>, <strong>Codex CLI</strong>, and <strong>Grok CLI</strong>
          . <strong>OpenCode</strong> and other text-only models cannot operate your PC because they
          lack vision capabilities.
        </p>
      </div>

      <section className="settings-section">
        <h3 className="settings-heading">Automation Controls</h3>
        <div className={`computer-use-card${status?.enabled ? " active" : ""}`}>
          <div className="computer-use-info">
            <strong>Enable Computer Use</strong>
            <small>
              Allows authorized vision models to view display screenshots and send input commands.
            </small>
          </div>
          <Toggle
            checked={status?.enabled ?? false}
            disabled={saving || status === null}
            onChange={(checked) => void toggleEnabled(checked)}
            label="Enable Computer Use"
          />
        </div>

        <div className={`computer-use-card${status?.full_access ? " active" : ""}`}>
          <div className="computer-use-info">
            <strong>Full PC Access &amp; Control</strong>
            <small>
              Grants the agent permission to move the mouse, click buttons, type text, and press
              hotkeys across all applications.
            </small>
          </div>
          <Toggle
            checked={status?.full_access ?? false}
            disabled={saving || status === null || !status.enabled}
            onChange={(checked) => void toggleFullAccess(checked)}
            label="Full PC Access"
          />
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-heading">Provider Vision Compatibility</h3>
        <p className="settings-note">
          Live capability status for connected providers. Only green vision-enabled backends can be
          used with Computer Use.
        </p>
        <div className="provider-matrix">
          {(status?.supported_providers ?? []).map((provider) => (
            <div
              key={provider.id}
              className={`matrix-row ${provider.vision_supported ? "supported" : "unsupported"}`}
            >
              <div className="matrix-icon">
                <ProviderLogo id={provider.id} />
              </div>
              <div className="matrix-details">
                <strong>{provider.label}</strong>
                <small>{provider.note}</small>
              </div>
              <span
                className={`matrix-chip ${provider.vision_supported ? "supported" : "unsupported"}`}
              >
                {provider.vision_supported ? "Vision · Supported" : "No Vision · Blocked"}
              </span>
            </div>
          ))}
        </div>
      </section>

      <section className="settings-section">
        <h3 className="settings-heading">Screen Vision &amp; Diagnostic Tool</h3>
        <p className="settings-note">
          Test your display capture pipeline and verify cursor communication.
        </p>
        <div className="screen-preview-box">
          <div className="screen-preview-header">
            <strong>Live Display Inspection</strong>
            <div className="screen-preview-actions">
              <button
                className="btn-primary"
                onClick={() => void capturePreview()}
                disabled={testing}
              >
                {testing ? "Capturing…" : "Capture Screen Preview"}
              </button>
              <button onClick={() => void testCursor()} disabled={testing}>
                Inspect Cursor Pos
              </button>
            </div>
          </div>

          {actionLog ? (
            <div
              style={{
                fontFamily: "var(--mono)",
                fontSize: "11.5px",
                color: "var(--accent)",
                padding: "4px 0",
              }}
            >
              {actionLog}
            </div>
          ) : null}

          {preview ? (
            <div>
              <div
                style={{
                  fontSize: "11px",
                  color: "var(--text-faint)",
                  marginBottom: "6px",
                  fontFamily: "var(--mono)",
                }}
              >
                Resolution: {preview.width} × {preview.height} px · Captured{" "}
                {new Date(preview.captured_at).toLocaleTimeString()}
              </div>
              <img
                src={`data:image/jpeg;base64,${preview.image_base64}`}
                alt="Desktop capture preview"
                className="screen-preview-thumb"
              />
            </div>
          ) : null}
        </div>
      </section>
    </>
  );
}

function IntegrationsTab() {
  const [tools, setTools] = useState<ToolAvailability[] | null>(null);

  useEffect(() => {
    void api.projectTools().then(setTools).catch(() => setTools([]));
  }, []);

  return (
    <>
      <h2 className="settings-heading">Integrations</h2>
      <p className="settings-note">
        Bhippi detects command-line launchers on PATH. Project files never leave your machine when an
        editor is opened.
      </p>
      <section className="settings-section integration-list">
        {tools === null ? (
          <div className="progress-rail active" />
        ) : (
          tools.map((tool) => (
            <div className="integration-row" key={tool.tool}>
              <span className="integration-icon">
                <IconCode size={16} />
              </span>
              <span>
                <strong>{tool.label}</strong>
                <small>{tool.hint}</small>
              </span>
              <span className={`integration-state${tool.available ? " available" : ""}`}>
                {tool.available ? "Available" : "Not detected"}
              </span>
            </div>
          ))
        )}
      </section>
    </>
  );
}

const CLI_GROUP = ["claude", "codex", "opencode", "grok", "kimi"];
const LOCAL_GROUP = ["ollama", "lmstudio", "llamacpp", "vllm", "jan", "tgui"];
const CLOUD_GROUP = ["anthropic", "openai", "xai", "moonshot", "groq", "openrouter"];

type InstallState = {
  phase: "working" | "success" | "error";
  message: string;
};

function ProvidersTab({
  status,
  onRefresh,
}: {
  status: AppStatus | null;
  onRefresh: () => void;
}) {
  const [scanning, setScanning] = useState(false);
  const [installing, setInstalling] = useState<Record<string, InstallState>>({});

  // Silent install/update progress → per-row state chips.
  useEffect(() => {
    const off = events.providerInstallProgress.listen(({ payload }) => {
      const phase =
        payload.phase === "done" ? "success" : payload.phase === "failed" ? "error" : "working";
      setInstalling((current) => ({
        ...current,
        [payload.id]: { phase, message: payload.message },
      }));
    });
    return () => void off.then((unlisten) => unlisten());
  }, []);

  const rows = status?.providers ?? [];
  const byId = new Map(rows.map((row) => [row.id, row]));

  const toggle = async (id: string, enabled: boolean) => {
    try {
      await api.setProviderEnabled(id, enabled);
    } finally {
      onRefresh();
    }
  };

  const install = async (id: string) => {
    setInstalling((current) => ({
      ...current,
      [id]: { phase: "working", message: "Starting installer…" },
    }));
    try {
      await api.installProvider(id);
      onRefresh();
    } catch (installError) {
      setInstalling((current) => ({
        ...current,
        [id]: { phase: "error", message: installErrorMessage(installError) },
      }));
      onRefresh();
    }
  };

  const rescan = async () => {
    setScanning(true);
    try {
      await api.rescanProviders();
      setInstalling({});
      onRefresh();
    } finally {
      setScanning(false);
    }
  };

  const group = (ids: string[], title: string, note: string) => (
    <section className="settings-section">
      <h2 className="settings-heading">{title}</h2>
      <p className="settings-note">{note}</p>
      {ids.map((id) => {
        const row = byId.get(id);
        return row ? (
          <ProviderRow
            key={id}
            provider={row}
            active={status?.active_provider_id === id}
            installState={installing[id]}
            onToggle={(enabled) => void toggle(id, enabled)}
            onInstall={() => void install(id)}
          />
        ) : null;
      })}
    </section>
  );

  return (
    <>
      <h2 className="settings-heading">Providers</h2>
      <p className="settings-note">
        Turn a provider on to make it selectable in the chat model picker. Enabled CLIs are kept up
        to date automatically and quietly. When nothing is available, the labelled offline demo
        answers — never a silent fallback.
      </p>

      <div style={{ marginBottom: 12 }}>
        <button className="btn-primary" onClick={() => void rescan()} disabled={scanning}>
          <span style={{ display: "inline-flex", alignItems: "center", gap: 7 }}>
            <IconRefresh size={12} />
            {scanning ? "Scanning…" : "Re-scan"}
          </span>
        </button>
      </div>

      {group(
        CLI_GROUP,
        "Coding agents",
        "Installed CLIs answer chat through their own command line. Missing ones can be installed right here.",
      )}
      {group(
        LOCAL_GROUP,
        "Local models",
        "Servers Bhippi finds on this machine while their switch is on — your data never leaves the loopback.",
      )}
      {group(
        CLOUD_GROUP,
        "Cloud APIs",
        "Detected by credential presence only. Keys stay in your environment; adapters land in S1.",
      )}

      <section className="settings-section">
        <h2 className="settings-heading">Offline</h2>
        {byId.get("demo") ? (
          <ProviderRow
            provider={byId.get("demo") as ProviderInfo}
            active={status?.active_provider_id === "demo"}
            alwaysOn
            installState={undefined}
            onToggle={() => undefined}
            onInstall={() => undefined}
          />
        ) : null}
      </section>
    </>
  );
}

function ProviderRow({
  provider,
  active,
  alwaysOn = false,
  installState,
  onToggle,
  onInstall,
}: {
  provider: ProviderInfo;
  active: boolean;
  alwaysOn?: boolean;
  installState?: InstallState;
  onToggle: (enabled: boolean) => void;
  onInstall: () => void;
}) {
  const isCli = provider.kind === "cli";
  const isInstalling = installState?.phase === "working";
  // A local server found on disk but not listening. Distinct from "missing": the fix is
  // to start it, not to install it. Bhippi deliberately will not start it — launching a
  // model server uninvited is what used to open Bionic on every app start and load
  // gigabytes into RAM nobody had asked for.
  const idle = provider.kind === "local_server" && provider.offered && !provider.installed;

  let statusLine: React.ReactNode;
  if (installState) {
    statusLine = (
      <span className={`provider-install-status ${installState.phase}`} aria-live="polite">
        {installState.message}
      </span>
    );
  } else if (provider.health.status === "healthy") {
    statusLine =
      provider.version ??
      (provider.models.length > 0
        ? `${provider.models.length} model${provider.models.length === 1 ? "" : "s"} · ${provider.models[0]}`
        : `healthy · ${provider.health.latency_ms} ms`);
  } else if (idle) {
    statusLine = (
      <span className="provider-idle">
        installed · not running — start it yourself to use it here
      </span>
    );
  } else if (provider.health.status === "degraded" || provider.health.status === "unavailable") {
    statusLine = provider.health.reason;
  } else {
    statusLine = "disabled";
  }

  return (
    <div
      className={`provider-row${provider.enabled ? " enabled" : ""}${idle ? " idle" : ""}`}
    >
      <span className="provider-logo-cell">
        <ProviderLogo id={provider.id} />
      </span>
      <span className="provider-main">
        <div className="provider-name">
          {provider.label}{" "}
          {active ? (
            <span className="scope-chip" style={{ color: "var(--accent)" }}>
              default
            </span>
          ) : null}
        </div>
        <div className="health-reason">{statusLine}</div>
      </span>

      {isCli ? (
        <button
          className="btn-primary"
          onClick={onInstall}
          disabled={isInstalling}
          title={`Runs the official installer for ${provider.label}`}
        >
          {isInstalling
            ? provider.installed
              ? "Updating…"
              : "Installing…"
            : installState?.phase === "error"
              ? "Retry"
              : provider.installed
                ? "Update"
                : "Install"}
        </button>
      ) : null}

      <Toggle
        checked={provider.enabled || alwaysOn}
        disabled={alwaysOn}
        onChange={onToggle}
        label={`Enable ${provider.label}`}
      />
    </div>
  );
}

function installErrorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null) {
    const value = error as { message?: unknown; hint?: unknown };
    const parts = [value.message, value.hint].filter(
      (part): part is string => typeof part === "string" && part.trim().length > 0,
    );
    if (parts.length > 0) return parts.join(" · ");
  }
  if (typeof error === "string" && error.trim().length > 0) return error;
  return "Install failed. Check that Node.js and npm are available, then retry.";
}

function Toggle({
  checked,
  disabled,
  onChange,
  label,
}: {
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
  label: string;
}) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={`switch${checked ? " on" : ""}`}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <span className="knob" />
    </button>
  );
}

function ResearchTab() {
  const [budgets, setBudgets] = useState<TierBudgetView[]>([]);

  useEffect(() => {
    void api.tierBudgets().then(setBudgets);
  }, []);

  return (
    <>
      <h2 className="settings-heading">Research defaults</h2>
      <p className="settings-note">
        The full budget table — what each depth tier buys you. Crawl controls land with harvest.
      </p>
      <table className="table">
        <thead>
          <tr>
            <th>Tier</th>
            <th>Expansions</th>
            <th>Branch</th>
            <th>Sources</th>
            <th>T2 floor</th>
            <th>Primary</th>
            <th>Dots</th>
            <th>Counter</th>
            <th>Wall</th>
            <th>Tokens</th>
          </tr>
        </thead>
        <tbody>
          {budgets.map((view) => (
            <tr key={view.tier}>
              <td className="num">{view.tier}</td>
              <td className="num">{view.expansions}</td>
              <td className="num">{view.branch}</td>
              <td className="num">
                {view.sources_min}–{view.sources_max}
              </td>
              <td className="num">{view.min_tier2}</td>
              <td className="num">{view.min_primary}</td>
              <td className="num">{view.target_dots}</td>
              <td className="num">{view.counter_passes}</td>
              <td className="num">{view.wall_minutes}m</td>
              <td className="num">{view.tokens.toLocaleString()}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}

function SkillsTab() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<string>("all");
  const [scanning, setScanning] = useState(false);

  const load = () => {
    setLoading(true);
    api
      .listSkills()
      .then((data) => {
        setSkills(data);
        setLoading(false);
      })
      .catch(() => {
        setSkills([]);
        setLoading(false);
      });
  };

  useEffect(() => {
    load();
  }, []);

  const toggleSkill = async (id: string, enabled: boolean) => {
    setSkills((current) =>
      current.map((s) => (s.id === id ? { ...s, enabled } : s)),
    );
    try {
      await api.setSkillEnabled(id, enabled);
    } catch {
      load();
    }
  };

  const rescan = async () => {
    setScanning(true);
    try {
      const fresh = await api.importExternalSkills();
      setSkills(fresh);
    } finally {
      setScanning(false);
    }
  };

  const sources = ["all", "claude", "codex", "antigravity", "cursor", "workspace", "builtin"];
  const visible = filter === "all" ? skills : skills.filter((s) => s.source === filter);

  return (
    <>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "8px" }}>
        <div>
          <h2 className="settings-heading" style={{ margin: 0 }}>AI Skills &amp; Prompts</h2>
          <p className="settings-note" style={{ margin: "4px 0 0" }}>
            Auto-imported skills and specialized directives from installed AI apps (Claude Code, OpenAI Codex, Antigravity, and Cursor).
          </p>
        </div>
        <button className="btn-primary" onClick={() => void rescan()} disabled={scanning}>
          <span style={{ display: "inline-flex", alignItems: "center", gap: 7 }}>
            <IconRefresh size={12} />
            {scanning ? "Scanning…" : "Re-scan AI Apps"}
          </span>
        </button>
      </div>

      <div className="skills-filter-rail" style={{ display: "flex", gap: "6px", margin: "16px 0 12px", flexWrap: "wrap" }}>
        {sources.map((src) => (
          <button
            key={src}
            className={`tier-chip${filter === src ? " active" : ""}`}
            onClick={() => setFilter(src)}
            style={{ textTransform: "capitalize" }}
          >
            {src === "all" ? `All (${skills.length})` : `${src} (${skills.filter((s) => s.source === src).length})`}
          </button>
        ))}
      </div>

      <div className="skills-list" style={{ display: "flex", flexDirection: "column", gap: "10px", marginTop: "12px" }}>
        {loading ? (
          <div className="progress-rail active" />
        ) : visible.length === 0 ? (
          <div style={{ padding: "24px", textAlign: "center", color: "var(--text-dim)", border: "1px dashed var(--line)", borderRadius: "8px" }}>
            No skills found for this filter.
          </div>
        ) : (
          visible.map((skill) => (
            <div key={skill.id} className={`skill-card${skill.enabled ? " enabled" : ""}`} style={{
              display: "flex",
              alignItems: "flex-start",
              justifyContent: "space-between",
              padding: "12px 16px",
              background: "var(--surface)",
              border: "1px solid var(--line)",
              borderRadius: "8px",
              gap: "12px",
            }}>
              <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: "4px" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                  <strong style={{ fontSize: "13.5px", color: "var(--text)" }}>{skill.name}</strong>
                  <span className={`matrix-chip ${skill.source === "builtin" ? "unsupported" : "supported"}`} style={{ fontSize: "10.5px", padding: "2px 7px" }}>
                    {skill.source}
                  </span>
                  <span style={{ fontSize: "11px", color: "var(--accent)", fontFamily: "var(--mono)" }}>
                    @{skill.id}
                  </span>
                </div>
                <small style={{ fontSize: "12px", color: "var(--text-dim)", lineHeight: "1.4" }}>
                  {skill.description}
                </small>
                {skill.tags.length > 0 ? (
                  <div style={{ display: "flex", gap: "4px", marginTop: "4px", flexWrap: "wrap" }}>
                    {skill.tags.map((t) => (
                      <span key={t} style={{
                        fontSize: "10px",
                        padding: "1px 6px",
                        borderRadius: "4px",
                        background: "var(--surface-2)",
                        color: "var(--text-faint)",
                        fontFamily: "var(--mono)",
                      }}>
                        #{t}
                      </span>
                    ))}
                  </div>
                ) : null}
              </div>
              <Toggle
                checked={skill.enabled}
                onChange={(checked) => void toggleSkill(skill.id, checked)}
                label={`Enable ${skill.name}`}
              />
            </div>
          ))
        )}
      </div>
    </>
  );
}

const PLACEHOLDERS: Record<string, string> = {
  Ticker: "Feed polling and the live strip land with the ticker sprint.",
  Automation: "Mode switch, caps, quiet hours and the review queue land in S9.",
  Mind: "The constellation view of everything Bhippi remembers lands with memory (S5).",
  Publishing: "Site identity, deploy targets and rollback land with publishing (S8).",
};

function PlaceholderTab({ tab }: { tab: string }) {
  return (
    <>
      <h2 className="settings-heading">{tab}</h2>
      <p className="settings-note">{PLACEHOLDERS[tab] ?? ""}</p>
    </>
  );
}

function ProfileTab({ onRefresh: _onRefresh }: { onRefresh?: () => void }) {
  const [profile, setProfile] = useState<UserProfile>(getProfile());
  const [keyRevealed, setKeyRevealed] = useState(false);
  const [copied, setCopied] = useState(false);
  const [isEditing, setIsEditing] = useState(false);
  const [tempName, setTempName] = useState(profile.name);
  const [tempEmail, setTempEmail] = useState(profile.email);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    return onProfileChange((next) => {
      setProfile(next);
      setTempName(next.name);
      setTempEmail(next.email);
    });
  }, []);

  const handleAvatarUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    if (file.size > 5 * 1024 * 1024) {
      alert("Please select an image smaller than 5MB.");
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string") {
        saveProfile({ avatarUrl: reader.result });
      }
    };
    reader.readAsDataURL(file);
  };

  const handleRemoveAvatar = () => {
    saveProfile({ avatarUrl: null });
  };

  const handleSaveInfo = () => {
    saveProfile({
      name: tempName.trim() || "Developer",
      email: tempEmail.trim() || "developer@bhippi.local",
    });
    setIsEditing(false);
  };

  const copyKey = async () => {
    try {
      await navigator.clipboard.writeText(profile.licenseKey);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // ignore
    }
  };

  const initials =
    profile.name
      .split(" ")
      .map((w) => w[0])
      .filter(Boolean)
      .slice(0, 2)
      .join("")
      .toUpperCase() || "D";

  return (
    <div className="profile-tab-content">
      {/* Hero Profile Card */}
      <div className="profile-hero-card">
        <div className="profile-hero-avatar-area">
          <div className="profile-avatar-halo">
            {profile.avatarUrl ? (
              <img src={profile.avatarUrl} alt={profile.name} className="profile-avatar-large" />
            ) : (
              <div className="profile-avatar-large-fallback">{initials}</div>
            )}
            <span className="profile-crown-super-badge" title="Lifetime VIP Activation">
              <IconCrown size={18} />
            </span>
          </div>

          <div className="profile-avatar-btn-row">
            <input
              type="file"
              ref={fileInputRef}
              accept="image/*"
              style={{ display: "none" }}
              onChange={handleAvatarUpload}
            />
            <button
              type="button"
              className="btn-profile-action"
              onClick={() => fileInputRef.current?.click()}
            >
              <IconCamera size={13} />
              <span>Change Photo</span>
            </button>
            {profile.avatarUrl ? (
              <button
                type="button"
                className="btn-profile-action text-danger"
                onClick={handleRemoveAvatar}
              >
                <IconTrash size={13} />
                <span>Remove</span>
              </button>
            ) : null}
          </div>
        </div>

        <div className="profile-hero-details">
          <div className="profile-vip-ribbon">
            <span className="live-pulse-dot" />
            <IconCrown size={12} />
            <span>{profile.plan}</span>
            <span className="ribbon-tag">PRO</span>
          </div>

          {isEditing ? (
            <div className="profile-edit-fields">
              <label className="profile-input-label">
                <span>Display Name</span>
                <input
                  type="text"
                  className="profile-text-input"
                  value={tempName}
                  onChange={(e) => setTempName(e.target.value)}
                  placeholder="Enter your name"
                />
              </label>
              <label className="profile-input-label">
                <span>Email Address</span>
                <input
                  type="email"
                  className="profile-text-input"
                  value={tempEmail}
                  onChange={(e) => setTempEmail(e.target.value)}
                  placeholder="developer@bhippi.local"
                />
              </label>
              <div className="profile-edit-actions">
                <button type="button" className="btn-save-profile" onClick={handleSaveInfo}>
                  <IconCheck size={13} /> Save Profile
                </button>
                <button
                  type="button"
                  className="btn-cancel-profile"
                  onClick={() => {
                    setTempName(profile.name);
                    setTempEmail(profile.email);
                    setIsEditing(false);
                  }}
                >
                  Cancel
                </button>
              </div>
            </div>
          ) : (
            <div className="profile-display-info">
              <h2 className="profile-display-name">{profile.name}</h2>
              <p className="profile-display-email">{profile.email}</p>
              <p className="profile-status-note">
                Activated account with permanent single-seat developer entitlement.
              </p>
              <button
                type="button"
                className="btn-edit-profile"
                onClick={() => setIsEditing(true)}
              >
                Edit Account Details
              </button>
            </div>
          )}
        </div>
      </div>

      {/* Product Key & Activation Section */}
      <div className="settings-section">
        <h3 className="settings-section-title">
          <IconKey size={15} />
          <span>Product License & Activation</span>
        </h3>
        <p className="settings-note">
          Your product is permanently authenticated with full lifetime updates and cloud & local model reasoning.
        </p>

        <div className="license-box-card">
          <div className="license-box-top">
            <div className="license-badge-pair">
              <span className="license-tier-name">{profile.tier}</span>
              <span className="license-verified-pill">
                <IconBadgeCheck size={13} />
                <span>Lifetime Authorized</span>
              </span>
            </div>
            <span className="license-activation-date">{profile.activatedAt}</span>
          </div>

          <div className="license-key-bar">
            <div className="license-key-value">
              <span className="license-key-label">Product Key:</span>
              <code className="license-key-mono">
                {keyRevealed ? profile.licenseKey : maskLicenseKey(profile.licenseKey)}
              </code>
            </div>

            <div className="license-key-btn-group">
              <button
                type="button"
                className="license-icon-btn"
                onClick={() => setKeyRevealed((r) => !r)}
                title={keyRevealed ? "Hide Product Key" : "Reveal Product Key"}
              >
                {keyRevealed ? <IconEyeOff size={14} /> : <IconEye size={14} />}
                <span>{keyRevealed ? "Hide" : "Show Key"}</span>
              </button>
              <button
                type="button"
                className={`license-icon-btn${copied ? " copied" : ""}`}
                onClick={copyKey}
                title="Copy Product Key to Clipboard"
              >
                {copied ? <IconCheck size={14} /> : <IconCopy size={14} />}
                <span>{copied ? "Copied!" : "Copy"}</span>
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* Unlocked Lifetime Capabilities */}
      <div className="settings-section">
        <h3 className="settings-section-title">
          <IconSparkles size={15} />
          <span>Included Lifetime Capabilities</span>
        </h3>
        <div className="capabilities-grid">
          <div className="capability-card">
            <div className="capability-icon gold">
              <IconCrown size={17} />
            </div>
            <div className="capability-body">
              <strong>Lifetime Access</strong>
              <span>Unlimited software upgrades with zero recurring subscription fees.</span>
            </div>
          </div>
          <div className="capability-card">
            <div className="capability-icon cyan">
              <IconBolt size={16} />
            </div>
            <div className="capability-body">
              <strong>Autonomous Multi-Turn Agent</strong>
              <span>Deep technology research, codebase exploration, and self-directed task loops.</span>
            </div>
          </div>
          <div className="capability-card">
            <div className="capability-icon violet">
              <IconVision size={16} />
            </div>
            <div className="capability-body">
              <strong>Multimodal Vision & Perception</strong>
              <span>Full image reasoning, UI layout inspection, and screen perceptual analysis.</span>
            </div>
          </div>
          <div className="capability-card">
            <div className="capability-icon emerald">
              <IconMonitor size={16} />
            </div>
            <div className="capability-body">
              <strong>Computer & Browser Automation</strong>
              <span>Autonomous cursor, keyboard, and headless/headful browser instrumentation.</span>
            </div>
          </div>
          <div className="capability-card">
            <div className="capability-icon amber">
              <IconFetchUrl size={16} />
            </div>
            <div className="capability-body">
              <strong>OpenCode & Multi-Provider Bridge</strong>
              <span>Connect local Ollama, OpenCode, Claude, OpenAI, Gemini, DeepSeek, and custom endpoints.</span>
            </div>
          </div>
          <div className="capability-card">
            <div className="capability-icon blue">
              <IconBrain size={16} />
            </div>
            <div className="capability-body">
              <strong>Persistent SQLite Knowledge Graph</strong>
              <span>Continuous long-term memory, cross-session indexing, and offline vector retrieval.</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function AboutTab({ status }: { status: AppStatus | null }) {
  const [profile, setProfile] = useState<UserProfile>(getProfile());
  const [keyRevealed, setKeyRevealed] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    return onProfileChange(setProfile);
  }, []);

  const copyKey = async () => {
    try {
      await navigator.clipboard.writeText(profile.licenseKey);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // ignore
    }
  };

  return (
    <div className="about-tab-content">
      <div className="about-brand-hero">
        <div className="about-logo-mark">
          <img src="/bhippi-logo.png" className="about-logo-image" alt="Bhippi" draggable={false} />
        </div>
        <div className="about-brand-text">
          <h2 className="about-app-title">Bhippi Content & Research Agent</h2>
          <p className="about-app-version">
            Version {status?.version ?? "0.1.0"} · Desktop Edition (Stable)
          </p>
          <p className="about-app-tagline">
            Autonomous deep research, persistent knowledge graphs, and high-performance AI publishing.
          </p>
        </div>
      </div>

      {/* Product Key & Activation Info */}
      <div className="settings-section">
        <h3 className="settings-section-title">
          <IconKey size={15} />
          <span>Product Activation & Licensing</span>
        </h3>
        <div className="license-box-card">
          <div className="license-box-top">
            <div className="license-badge-pair">
              <span className="license-tier-name">{profile.plan}</span>
              <span className="license-verified-pill">
                <IconBadgeCheck size={13} />
                <span>Verified Lifetime License</span>
              </span>
            </div>
            <span className="license-activation-date">{profile.activatedAt}</span>
          </div>

          <div className="license-key-bar">
            <div className="license-key-value">
              <span className="license-key-label">Product Key:</span>
              <code className="license-key-mono">
                {keyRevealed ? profile.licenseKey : maskLicenseKey(profile.licenseKey)}
              </code>
            </div>

            <div className="license-key-btn-group">
              <button
                type="button"
                className="license-icon-btn"
                onClick={() => setKeyRevealed((r) => !r)}
                title={keyRevealed ? "Hide Product Key" : "Reveal Product Key"}
              >
                {keyRevealed ? <IconEyeOff size={14} /> : <IconEye size={14} />}
                <span>{keyRevealed ? "Hide" : "Show Key"}</span>
              </button>
              <button
                type="button"
                className={`license-icon-btn${copied ? " copied" : ""}`}
                onClick={copyKey}
                title="Copy Product Key to Clipboard"
              >
                {copied ? <IconCheck size={14} /> : <IconCopy size={14} />}
                <span>{copied ? "Copied!" : "Copy"}</span>
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* System Specifications & Diagnostics */}
      <div className="settings-section">
        <h3 className="settings-section-title">
          <IconSettings size={15} />
          <span>Engine Architecture & Diagnostics</span>
        </h3>
        <div className="about-specs-grid">
          <div className="about-spec-item">
            <span className="spec-label">Native Core</span>
            <span className="spec-value">Rust Tauri v2 + Tokio Async</span>
          </div>
          <div className="about-spec-item">
            <span className="spec-label">Frontend Layer</span>
            <span className="spec-value">React 18 + Vite + Kinetic Design System</span>
          </div>
          <div className="about-spec-item">
            <span className="spec-label">Knowledge Engine</span>
            <span className="spec-value">SQLite WAL + Local Vector Graph</span>
          </div>
          <div className="about-spec-item">
            <span className="spec-label">Perception Subsystem</span>
            <span className="spec-value">Multimodal Vision & Virtual Display</span>
          </div>
          <div className="about-spec-item">
            <span className="spec-label">Active Provider</span>
            <span className="spec-value">{status?.active_provider_id ?? "OpenCode Local Bridge"}</span>
          </div>
          <div className="about-spec-item">
            <span className="spec-label">Telemetry & Tracking</span>
            <span className="spec-value text-success">Zero Telemetry (100% Private)</span>
          </div>
        </div>
      </div>
    </div>
  );
}
