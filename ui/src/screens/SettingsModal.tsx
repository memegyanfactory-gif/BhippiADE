import { useCallback, useEffect, useRef, useState } from "react";
import type {
  AppStatus,
  BlenderMcpStatus,
  ComputerUseStatus,
  ProviderInfo,
  ScreenCapture,
  Skill,
  ToolAvailability,
} from "../lib/ipc";
import { api, events } from "../lib/api";
import {
  TIER_LABELS,
  TIER_NAMES,
  tierUsability,
  type TierName,
  type TierPreset,
  type Tiers,
} from "../lib/tiers";
import {
  IconBadgeCheck,
  IconBolt,
  IconBrain,
  IconCamera,
  IconCheck,
  IconClose,
  IconCode,
  IconCopy,
  IconCrown,
  IconDot,
  IconExternal,
  IconEye,
  IconEyeOff,
  IconFetchUrl,
  IconGauge,
  IconKey,
  IconMic,
  IconMonitor,
  IconPalette,
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
  DEFAULT_APPEARANCE_SETTINGS,
  GROUNDS,
  type ColorSchemeId,
  type StyleMode,
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
  "About",
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

  const TAB_ICON: Partial<Record<SettingsTab, JSX.Element>> = {
    Appearance: <IconPalette size={14} />,
    Profile: <IconUser size={14} />,
    Providers: <IconBolt size={14} />,
    "Audio & Voice": <IconMic size={14} />,
    "Computer Use": <IconMonitor size={14} />,
    Skills: <IconSparkles size={14} />,
    Integrations: <IconExternal size={14} />,
    Usage: <IconGauge size={14} />,
    About: <IconCrown size={14} />,
  };

  const PANELS: Partial<Record<SettingsTab, JSX.Element>> = {
    Appearance: <AppearanceTab />,
    Profile: <ProfileTab onRefresh={onRefresh} />,
    Providers: <ProvidersTab status={status} onRefresh={onRefresh} />,
    "Audio & Voice": <AudioTab />,
    "Computer Use": <ComputerUseTab />,
    Skills: <SkillsTab />,
    Integrations: <IntegrationsTab />,
    Usage: <UsagePanel />,
    About: <AboutTab status={status} />,
  };

  return (
    <div className="modal-overlay" onClick={onClose} role="presentation">
      <div
        className="modal settings-fullscreen-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="modal-top-bar">
          <div className="modal-top-left">
            <span className="modal-top-title">Settings</span>
            <span className="modal-breadcrumb-sep">/</span>
            <span className="modal-top-crumb">{tab}</span>
          </div>

          <div className="modal-top-right">
            <span className="modal-esc-hint">Esc</span>
            <button
              type="button"
              className="modal-close-btn"
              onClick={onClose}
              aria-label="Close settings"
              title="Close settings"
            >
              <IconClose size={15} />
            </button>
          </div>
        </header>

        <div className="modal-content-split">
          <nav className="modal-rail" aria-label="Settings sections">
            {TABS.map((entry) => (
              <button
                key={entry}
                type="button"
                className={`modal-tab${tab === entry ? " active" : ""}`}
                onClick={() => setTab(entry)}
                aria-current={tab === entry ? "page" : undefined}
              >
                {TAB_ICON[entry] ?? <IconDot size={14} />}
                <span>{entry}</span>
              </button>
            ))}
            <div className="modal-rail-footer">
              <span className="rail-version">bhippi v{status?.version ?? "0.1.0"}</span>
              <span className="rail-plan">{profile.plan}</span>
            </div>
          </nav>

          <div className="modal-body">{PANELS[tab]}</div>
        </div>
      </div>
    </div>
  );
}

/**
 * The Appearance tab.
 *
 * Palette and material are chosen separately here because they *are* separate:
 * every scheme now renders correctly in every mode, so forcing the two into one
 * combined list (which is what the old "Solid Color Scheme, Max mode only" section
 * did) would hide two thirds of the combinations for no reason.
 */

interface SchemeEntry {
  id: ColorSchemeId;
  label: string;
  note: string;
}

const SCHEME_LIST: SchemeEntry[] = [
  { id: "dark", label: "Amber", note: "Warm obsidian, the house accent" },
  { id: "sapphire", label: "Sapphire", note: "Deep cobalt indigo" },
  { id: "emerald", label: "Emerald", note: "Forest carbon, luminous jade" },
  { id: "glacier", label: "Glacier", note: "Arctic steel and cyan" },
  { id: "amethyst", label: "Amethyst", note: "Velvet purple, radiant lavender" },
  { id: "cyberpunk", label: "Neon", note: "Violet night, electric rose" },
  { id: "crimson", label: "Crimson", note: "Volcanic ruby" },
  { id: "slate", label: "Slate", note: "True neutral graphite, soft mint" },
  { id: "titanium", label: "Titanium", note: "OLED black, titanium white" },
  { id: "light", label: "Linen", note: "Bright editorial light" },
  { id: "paper", label: "Paper", note: "Warm reading light" },
  { id: "contrast", label: "Contrast", note: "WCAG AAA pairs throughout" },
  { id: "system", label: "System", note: "Follows your operating system" },
];

const MODE_LIST: Array<{ id: StyleMode; label: string; note: string }> = [
  { id: "max", label: "Max", note: "Flat and opaque. No blur, no ambience, cheapest to draw." },
  { id: "plan", label: "Plan", note: "A gradient ground under lightly frosted panels." },
  { id: "glass", label: "Glass", note: "Full frosted transparency over a ground or your own image." },
];

/** A live swatch of one palette, drawn with that palette's real token values. */
function SchemeSwatch({ id }: { id: ColorSchemeId }) {
  return (
    <span className="scheme-swatch" data-scheme-preview={id} aria-hidden="true">
      <i className="scheme-swatch-bg" />
      <i className="scheme-swatch-surface" />
      <i className="scheme-swatch-accent" />
    </span>
  );
}

function AppearanceTab() {
  const [settings, setSettings] = useState<AppearanceSettings>(() => getAppearanceSettings());
  const [uploading, setUploading] = useState(false);

  useEffect(() => onAppearanceChange(setSettings), []);

  const update = (patch: Partial<AppearanceSettings>, immediate = false) => {
    const next = { ...settings, ...patch };
    setSettings(next);
    saveAppearanceSettings(next, immediate);
  };

  const handleUpload = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = event.target.files;
    if (!files || files.length === 0) return;
    setUploading(true);
    try {
      const processed: { name: string; dataUrl: string }[] = [];
      for (let index = 0; index < files.length; index += 1) {
        try {
          processed.push(await compressImage(files[index]));
        } catch (error) {
          console.warn("Could not read wallpaper", files[index].name, error);
        }
      }
      if (processed.length > 0) {
        addCustomWallpapers(processed);
        setSettings(getAppearanceSettings());
      }
    } finally {
      setUploading(false);
      event.target.value = "";
    }
  };

  const resetDefaults = () => {
    const next: AppearanceSettings = {
      ...DEFAULT_APPEARANCE_SETTINGS,
      // Uploaded images are the user's own files; a "reset appearance" must not
      // silently delete them.
      customWallpapers: settings.customWallpapers,
    };
    setSettings(next);
    saveAppearanceSettings(next, true);
  };

  const glassy = settings.styleMode === "glass";

  return (
    <div className="settings-tab">
      <header className="settings-tab-head">
        <div>
          <h2>Appearance</h2>
          <p>
            A <strong>palette</strong> picks the colours; a <strong>mode</strong> picks how solid
            they render. Every palette works in every mode.
          </p>
        </div>
        <button type="button" className="settings-reset" onClick={resetDefaults}>
          <IconRefresh size={12} />
          <span>Reset</span>
        </button>
      </header>

      <section className="settings-block">
        <div className="settings-block-head">
          <h3>Palette</h3>
          <span className="settings-block-note">{SCHEME_LIST.find((entry) => entry.id === settings.colorScheme)?.note}</span>
        </div>
        <div className="scheme-grid" role="radiogroup" aria-label="Colour palette">
          {SCHEME_LIST.map((entry) => (
            <button
              key={entry.id}
              type="button"
              role="radio"
              aria-checked={settings.colorScheme === entry.id}
              className={`scheme-card${settings.colorScheme === entry.id ? " active" : ""}`}
              onClick={() => update({ colorScheme: entry.id }, true)}
              title={entry.note}
            >
              <SchemeSwatch id={entry.id} />
              <span className="scheme-card-label">{entry.label}</span>
              {settings.colorScheme === entry.id ? (
                <span className="scheme-card-check">
                  <IconCheck size={11} />
                </span>
              ) : null}
            </button>
          ))}
        </div>
      </section>

      <section className="settings-block">
        <div className="settings-block-head">
          <h3>Mode</h3>
        </div>
        <div className="mode-row" role="radiogroup" aria-label="Style mode">
          {MODE_LIST.map((entry) => (
            <button
              key={entry.id}
              type="button"
              role="radio"
              aria-checked={settings.styleMode === entry.id}
              className={`mode-card${settings.styleMode === entry.id ? " active" : ""}`}
              onClick={() => update({ styleMode: entry.id }, true)}
            >
              <span className={`mode-card-preview mode-preview-${entry.id}`} aria-hidden="true">
                <i />
                <i />
              </span>
              <span className="mode-card-label">{entry.label}</span>
              <span className="mode-card-note">{entry.note}</span>
            </button>
          ))}
        </div>
      </section>

      {settings.styleMode !== "max" ? (
        <>
          <section className="settings-block">
            <div className="settings-block-head">
              <h3>Ground</h3>
              <span className="settings-block-note">Shape of the light behind the app. The palette gives it colour.</span>
            </div>
            <div className="ground-grid" role="radiogroup" aria-label="Background ground">
              {GROUNDS.map((entry) => (
                <button
                  key={entry.id}
                  type="button"
                  role="radio"
                  aria-checked={!settings.activeWallpaperId && settings.ground === entry.id}
                  className={`ground-card${!settings.activeWallpaperId && settings.ground === entry.id ? " active" : ""}`}
                  onClick={() => update({ ground: entry.id, activeWallpaperId: null }, true)}
                  title={entry.hint}
                >
                  <span className={`ground-preview ground-preview-${entry.id}`} aria-hidden="true" />
                  <span className="ground-name">{entry.name}</span>
                </button>
              ))}
            </div>
          </section>

          {glassy ? (
            <section className="settings-block">
              <div className="settings-block-head">
                <h3>Your images</h3>
                <label className={`settings-upload${uploading ? " busy" : ""}`}>
                  <IconPlus size={12} />
                  <span>{uploading ? "Compressing…" : "Add image"}</span>
                  <input
                    type="file"
                    multiple
                    accept="image/*"
                    disabled={uploading}
                    onChange={handleUpload}
                    style={{ display: "none" }}
                  />
                </label>
              </div>
              {settings.customWallpapers.length === 0 ? (
                <p className="settings-empty">
                  No images yet. Add one to use it instead of the gradient ground.
                </p>
              ) : (
                <div className="ground-grid">
                  {settings.customWallpapers.map((wallpaper) => (
                    <div
                      key={wallpaper.id}
                      role="radio"
                      tabIndex={0}
                      aria-checked={settings.activeWallpaperId === wallpaper.id}
                      className={`ground-card${settings.activeWallpaperId === wallpaper.id ? " active" : ""}`}
                      onClick={() => update({ activeWallpaperId: wallpaper.id }, true)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          update({ activeWallpaperId: wallpaper.id }, true);
                        }
                      }}
                      title={wallpaper.name}
                    >
                      <span
                        className="ground-preview"
                        style={{ backgroundImage: `url("${wallpaper.url}")`, backgroundSize: "cover" }}
                        aria-hidden="true"
                      />
                      <span className="ground-name">{wallpaper.name}</span>
                      <button
                        type="button"
                        className="ground-delete"
                        onClick={(event) => {
                          event.stopPropagation();
                          deleteCustomWallpaper(wallpaper.id);
                          setSettings(getAppearanceSettings());
                        }}
                        title="Delete this image"
                        aria-label={`Delete ${wallpaper.name}`}
                      >
                        <IconTrash size={11} />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </section>
          ) : null}

          <section className="settings-block">
            <div className="settings-block-head">
              <h3>Optics</h3>
            </div>

            <ToggleRow
              label="Breathing background"
              note="Slow drifting light in the palette's own colours. Held still when off, and always still under reduced-motion."
              checked={settings.ambientMotion}
              onChange={(checked) => update({ ambientMotion: checked }, true)}
            />

            <SliderRow
              label="Readability veil"
              note="Darkens the ground so text and code stay legible over it."
              value={settings.wallpaperDim}
              min={0}
              max={90}
              display={`${settings.wallpaperDim}%`}
              onChange={(value) => update({ wallpaperDim: value })}
            />

            {glassy ? (
              <>
                <SliderRow
                  label="Blur"
                  note="Depth of the frosted backdrop behind each panel."
                  value={settings.glassBlur}
                  min={0}
                  max={60}
                  display={`${settings.glassBlur}px`}
                  onChange={(value) => update({ glassBlur: value })}
                />
                <SliderRow
                  label="Panel density"
                  note="Lower is more transparent; higher is closer to solid."
                  value={Math.round(settings.glassOpacity * 100)}
                  min={20}
                  max={95}
                  display={`${Math.round(settings.glassOpacity * 100)}%`}
                  onChange={(value) => update({ glassOpacity: value / 100 })}
                />
                <SliderRow
                  label="Saturation"
                  note="Vibrancy of whatever shows through the glass."
                  value={settings.glassSaturation}
                  min={100}
                  max={240}
                  display={`${settings.glassSaturation}%`}
                  onChange={(value) => update({ glassSaturation: value })}
                />
              </>
            ) : null}
          </section>
        </>
      ) : null}

      <section className="settings-block">
        <div className="settings-block-head">
          <h3>Live tokens</h3>
          <span className="settings-block-note">What the rest of the app is drawing with right now.</span>
        </div>
        <div className="token-strip">
          {[
            ["--bg", "Background"],
            ["--surface", "Surface"],
            ["--surface-2", "Raised"],
            ["--line", "Hairline"],
            ["--text", "Text"],
            ["--accent", "Accent"],
          ].map(([token, name]) => (
            <div className="token-chip" key={token}>
              <span className="token-chip-swatch" style={{ background: `var(${token})` }} />
              <span className="token-chip-name">{name}</span>
              <code className="token-chip-var">{token}</code>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

/** Downscales and re-encodes a picked image so a 12 MB PNG never enters localStorage. */
function compressImage(file: File): Promise<{ name: string; dataUrl: string }> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Could not read the file"));
    reader.onload = () => {
      const raw = reader.result as string;
      const image = new Image();
      image.onerror = () => reject(new Error("Could not decode the image"));
      image.onload = () => {
        const maxDimension = 1600;
        let { width, height } = image;
        if (width > maxDimension || height > maxDimension) {
          const scale = maxDimension / Math.max(width, height);
          width = Math.round(width * scale);
          height = Math.round(height * scale);
        }
        const canvas = document.createElement("canvas");
        canvas.width = width;
        canvas.height = height;
        const context = canvas.getContext("2d");
        const name = file.name.replace(/\.[^/.]+$/, "");
        if (!context) {
          resolve({ name, dataUrl: raw });
          return;
        }
        context.drawImage(image, 0, 0, width, height);
        resolve({ name, dataUrl: canvas.toDataURL("image/jpeg", 0.82) });
      };
      image.src = raw;
    };
    reader.readAsDataURL(file);
  });
}

function SliderRow({
  label,
  note,
  value,
  min,
  max,
  display,
  onChange,
}: {
  label: string;
  note: string;
  value: number;
  min: number;
  max: number;
  display: string;
  onChange: (value: number) => void;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <span className="settings-row-label">{label}</span>
        <span className="settings-row-note">{note}</span>
      </div>
      <div className="settings-row-control slider">
        <input
          type="range"
          min={min}
          max={max}
          value={value}
          aria-label={label}
          onChange={(event) => onChange(Number(event.target.value))}
        />
        <span className="settings-row-value">{display}</span>
      </div>
    </div>
  );
}

function ToggleRow({
  label,
  note,
  checked,
  onChange,
}: {
  label: string;
  note: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-text">
        <span className="settings-row-label">{label}</span>
        <span className="settings-row-note">{note}</span>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        className={`settings-switch${checked ? " on" : ""}`}
        onClick={() => onChange(!checked)}
      >
        <span />
      </button>
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

      <BlenderMcpCard />
    </>
  );
}

/**
 * Blender over MCP (SPA-204). Rust detects the launcher and Blender and says what is
 * missing; the card shows the toggle, the command, the state and the three setup steps.
 */
function BlenderMcpCard() {
  const [status, setStatus] = useState<BlenderMcpStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [failure, setFailure] = useState<string | null>(null);

  const adopt = (next: BlenderMcpStatus) => {
    setStatus(next);
    setCommand(next.command);
    setArgs(next.args.join(" "));
  };

  const load = useCallback(() => {
    void api
      .blenderMcpStatus()
      .then(adopt)
      .catch((error: unknown) => setFailure(String((error as { message?: string })?.message ?? error)));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const save = async (enabled: boolean) => {
    setSaving(true);
    setFailure(null);
    try {
      adopt(await api.setBlenderMcp(enabled, command, args.split(/\s+/).filter(Boolean)));
    } catch (error) {
      setFailure(String((error as { message?: string })?.message ?? error));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="settings-section blender-mcp">
      <h3 className="settings-heading">Blender over MCP</h3>
      <p className="settings-note">
        Let the agent build props in Blender when the asset library has nothing that fits. Bhippi
        attaches the <code>blender-mcp</code> server to turns on Claude Code or Codex; the export
        lands in <code>assets/</code> with a licence sidecar.
      </p>

      <div className={`computer-use-card${status?.enabled ? " active" : ""}`}>
        <div className="computer-use-info">
          <strong>Enable Blender over MCP</strong>
          <small>{status?.note ?? (failure ?? "Checking…")}</small>
        </div>
        <Toggle
          checked={status?.enabled ?? false}
          disabled={saving || status === null}
          onChange={(checked) => void save(checked)}
          label="Enable Blender over MCP"
        />
      </div>

      <div className="blender-mcp-grid">
        <label className="blender-mcp-field">
          <span>Launcher</span>
          <input
            type="text"
            value={command}
            onChange={(event) => setCommand(event.target.value)}
            onBlur={() => void save(status?.enabled ?? false)}
            placeholder="uvx"
            aria-label="MCP launcher command"
          />
        </label>
        <label className="blender-mcp-field">
          <span>Arguments</span>
          <input
            type="text"
            value={args}
            onChange={(event) => setArgs(event.target.value)}
            onBlur={() => void save(status?.enabled ?? false)}
            placeholder="blender-mcp"
            aria-label="MCP launcher arguments"
          />
        </label>
      </div>

      <div className="provider-matrix blender-mcp-matrix">
        <div className={`matrix-row ${status?.launcher_path ? "supported" : "unsupported"}`}>
          <div className="matrix-details">
            <strong>Launcher</strong>
            <small>{status?.launcher_path ?? `${status?.command ?? "uvx"} was not found on this machine`}</small>
          </div>
          <span className={`matrix-chip ${status?.launcher_path ? "supported" : "unsupported"}`}>
            {status?.launcher_path ? "Found" : "Missing"}
          </span>
        </div>
        <div className={`matrix-row ${status?.blender_path ? "supported" : "unsupported"}`}>
          <div className="matrix-details">
            <strong>Blender</strong>
            <small>{status?.blender_path ?? "Not detected — install Blender or start it before a turn needs it"}</small>
          </div>
          <span className={`matrix-chip ${status?.blender_path ? "supported" : "unsupported"}`}>
            {status?.blender_path ? "Found" : "Not detected"}
          </span>
        </div>
      </div>

      <ol className="blender-mcp-steps">
        <li>
          Install <strong>uv</strong> (<code>pip install uv</code>) so <code>uvx blender-mcp</code> can run.
        </li>
        <li>
          In Blender, install the <strong>blender-mcp</strong> addon and press <em>Start MCP Server</em>
          in its side panel.
        </li>
        <li>Keep Blender open while Bhippi builds; the agent exports GLB files into the project.</li>
      </ol>

      <div className="blender-mcp-actions">
        <button type="button" className="btn-secondary" onClick={load} disabled={saving}>
          Check again
        </button>
        {status?.ready ? <span className="integration-state available">Ready</span> : null}
      </div>
    </section>
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

      <TiersSection status={status} />

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

/**
 * Settings › Providers › Tiers (GAD-017).
 *
 * The raw pickers stay exactly where they were; this is where the three chips the composer
 * shows are *defined*. A tier pointing at a backend that is off or missing is left alone and
 * flagged — Bhippi does not repoint someone's tier at a provider they did not choose.
 */
function TiersSection({ status }: { status: AppStatus | null }) {
  const [tiers, setTiers] = useState<Tiers | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState<TierName | null>(null);

  const load = useCallback(() => {
    api
      .tiers()
      .then((rows) => {
        setTiers(rows);
        setError(null);
      })
      .catch((loadError: unknown) => {
        setTiers(null);
        setError(String((loadError as Error).message ?? loadError));
      });
  }, []);

  useEffect(load, [load]);

  const save = async (name: TierName, next: TierPreset) => {
    setSaving(name);
    setTiers((current) => (current ? { ...current, [name]: next } : current));
    try {
      setTiers(await api.setTier(name, next));
      setError(null);
    } catch (saveError) {
      setError(String((saveError as Error).message ?? saveError));
      load();
    } finally {
      setSaving(null);
    }
  };

  const chatOptions = status?.chat_options ?? [];
  const providers = status?.providers ?? [];

  return (
    <section className="settings-section">
      <h2 className="settings-heading">Tiers</h2>
      <p className="settings-note">
        The Quick / Balanced / Max chips in the composer. Each one is a preset over the pickers
        above; a tier whose provider is switched off is shown disabled with the reason rather
        than answered by a different backend.
      </p>

      {error ? (
        <div className="project-error" role="alert">
          {error}
        </div>
      ) : null}

      {tiers === null && error === null ? (
        <div className="progress-rail active" aria-busy="true" aria-label="Loading tiers" />
      ) : tiers === null ? (
        <button className="btn-primary" onClick={load}>
          Retry
        </button>
      ) : (
        <table className="table">
          <thead>
            <tr>
              <th scope="col">Tier</th>
              <th scope="col">Provider</th>
              <th scope="col">Model</th>
              <th scope="col">Effort</th>
            </tr>
          </thead>
          <tbody>
            {TIER_NAMES.map((name) => {
              const preset = tiers[name];
              const usable = tierUsability(preset, chatOptions);
              const models = providers.find((row) => row.id === preset.provider)?.models ?? [];
              return (
                <tr key={name}>
                  <th scope="row">
                    {TIER_LABELS[name]}
                    {usable.usable ? null : (
                      <>
                        {" "}
                        <span className="asset-licence-unknown" title={usable.reason}>
                          unavailable
                        </span>
                      </>
                    )}
                  </th>
                  <td>
                    <select
                      className="plugin-select"
                      aria-label={`${TIER_LABELS[name]} provider`}
                      value={preset.provider}
                      disabled={saving === name}
                      onChange={(event) =>
                        void save(name, { ...preset, provider: event.target.value, model: null })
                      }
                    >
                      {[preset.provider, ...providers.map((row) => row.id)]
                        .filter((id, index, all) => all.indexOf(id) === index)
                        .map((id) => (
                          <option key={id} value={id}>
                            {providers.find((row) => row.id === id)?.label ?? id}
                          </option>
                        ))}
                    </select>
                  </td>
                  <td>
                    <select
                      className="plugin-select"
                      aria-label={`${TIER_LABELS[name]} model`}
                      value={preset.model ?? ""}
                      disabled={saving === name}
                      onChange={(event) =>
                        void save(name, { ...preset, model: event.target.value || null })
                      }
                    >
                      <option value="">Provider default</option>
                      {[...(preset.model ? [preset.model] : []), ...models]
                        .filter((id, index, all) => all.indexOf(id) === index)
                        .map((model) => (
                          <option key={model} value={model}>
                            {model}
                          </option>
                        ))}
                    </select>
                  </td>
                  <td>
                    <select
                      className="plugin-select"
                      aria-label={`${TIER_LABELS[name]} effort`}
                      value={preset.effort}
                      disabled={saving === name}
                      onChange={(event) =>
                        void save(name, { ...preset, effort: event.target.value })
                      }
                    >
                      {["fast", "balanced", "quality", "ultra"].map((level) => (
                        <option key={level} value={level}>
                          {level}
                        </option>
                      ))}
                    </select>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </section>
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
              <span>Plans a game, builds it system by system, plays it back, and iterates on what breaks.</span>
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
          <h2 className="about-app-title">Bhippi Game Studio</h2>
          <p className="about-app-version">
            Version {status?.version ?? "0.1.0"} · Desktop Edition (Stable)
          </p>
          <p className="about-app-tagline">
            Bhippi is a desktop game studio: describe a game, approve a plan, watch it get built
            on Godot, play it, iterate, export.
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
            <span className="spec-label">Game Runtime</span>
            <span className="spec-value">Godot 4 + Typed Action Registry</span>
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
