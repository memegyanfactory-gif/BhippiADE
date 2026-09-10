import React, { useEffect, useMemo, useRef, useState } from "react";
import type { ProviderInfo } from "../lib/ipc";
import {
  IconChevronDown,
  IconChevronRight,
  IconSearch,
  IconStar,
  IconStarFilled,
} from "./icons";
import { ProviderLogo } from "./ProviderLogo";
import {
  antigravityDisplayName,
  antigravityFamilyId,
  collapseAntigravityModels,
  isAntigravityProvider,
} from "../lib/antigravityModels";
import { shortModelName } from "./ComposerPopovers";
import type { SettingsTab } from "../screens/SettingsModal";

export interface UnifiedModelItem {
  id: string;
  label: string;
  providerId: string;
  providerLabel: string;
  isNew?: boolean;
  isFree?: boolean;
  isLegacy?: boolean;
  meta?: string | null;
}

export interface UnifiedModelPickerProps {
  providers: ProviderInfo[];
  currentProviderId: string | null;
  currentModel: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (providerId: string, modelId: string | null) => void;
  onOpenSettings?: (tab?: SettingsTab) => void;
}

// Preset catalogues with flagship models and legacy models
const CLAUDE_MODELS: UnifiedModelItem[] = [
  { id: "Claude Fable 5.1", label: "Claude Fable 5.1", providerId: "claude", providerLabel: "Claude", isNew: true },
  { id: "Claude Opus 5", label: "Claude Opus 5", providerId: "claude", providerLabel: "Claude" },
  { id: "Claude Sonnet 5", label: "Claude Sonnet 5", providerId: "claude", providerLabel: "Claude" },
  // Legacy models (expandable)
  { id: "Claude Fable 5", label: "Claude Fable 5", providerId: "claude", providerLabel: "Claude", isLegacy: true },
  { id: "Claude Opus 4.8", label: "Claude Opus 4.8", providerId: "claude", providerLabel: "Claude", isLegacy: true },
  { id: "Claude Sonnet 4.5", label: "Claude Sonnet 4.5", providerId: "claude", providerLabel: "Claude", isLegacy: true },
  { id: "Claude Haiku 4.5", label: "Claude Haiku 4.5", providerId: "claude", providerLabel: "Claude", isLegacy: true },
  { id: "Claude 3.7 Sonnet", label: "Claude 3.7 Sonnet", providerId: "claude", providerLabel: "Claude", isLegacy: true },
  { id: "Claude 3.5 Sonnet", label: "Claude 3.5 Sonnet", providerId: "claude", providerLabel: "Claude", isLegacy: true },
  { id: "Claude 3 Opus", label: "Claude 3 Opus", providerId: "claude", providerLabel: "Claude", isLegacy: true },
];

const CODEX_MODELS: UnifiedModelItem[] = [
  { id: "GPT-5.6 Sol", label: "GPT-5.6 Sol", providerId: "codex", providerLabel: "OpenAI", isNew: true },
  { id: "GPT-5.6 Luna", label: "GPT-5.6 Luna", providerId: "codex", providerLabel: "OpenAI" },
  { id: "GPT-6 Astra", label: "GPT-6 Astra", providerId: "codex", providerLabel: "OpenAI" },
  { id: "GPT-5 Codex", label: "GPT-5 Codex", providerId: "codex", providerLabel: "OpenAI" },
  // Legacy models
  { id: "o3-mini", label: "o3-mini", providerId: "codex", providerLabel: "OpenAI", isLegacy: true },
  { id: "o1", label: "o1", providerId: "codex", providerLabel: "OpenAI", isLegacy: true },
  { id: "GPT-4o", label: "GPT-4o", providerId: "codex", providerLabel: "OpenAI", isLegacy: true },
  { id: "GPT-4o-mini", label: "GPT-4o-mini", providerId: "codex", providerLabel: "OpenAI", isLegacy: true },
];

const GROK_MODELS: UnifiedModelItem[] = [
  { id: "Grok 4.6", label: "Grok 4.6", providerId: "grok", providerLabel: "Grok", isNew: true },
  { id: "Grok 2.5 Vision", label: "Grok 2.5 Vision", providerId: "grok", providerLabel: "Grok" },
  { id: "Grok Beta", label: "Grok Beta", providerId: "grok", providerLabel: "Grok" },
  { id: "Grok 2", label: "Grok 2", providerId: "grok", providerLabel: "Grok", isLegacy: true },
];

const ANTIGRAVITY_MODELS: UnifiedModelItem[] = [
  { id: "Gemini 3.8 Flash", label: "Gemini 3.8 Flash", providerId: "antigravity", providerLabel: "Antigravity", isNew: true },
  { id: "Gemini 3.7 Flash", label: "Gemini 3.7 Flash", providerId: "antigravity", providerLabel: "Antigravity" },
  { id: "Gemini 3.1 Pro", label: "Gemini 3.1 Pro", providerId: "antigravity", providerLabel: "Antigravity" },
  { id: "Claude Sonnet 4.6", label: "Claude Sonnet 4.6", providerId: "antigravity", providerLabel: "Antigravity" },
  { id: "Claude Opus 4.6", label: "Claude Opus 4.6", providerId: "antigravity", providerLabel: "Antigravity" },
  { id: "GPT-OSS 120B", label: "GPT-OSS 120B", providerId: "antigravity", providerLabel: "Antigravity" },
  // Legacy models
  { id: "Gemini 2.5 Pro", label: "Gemini 2.5 Pro", providerId: "antigravity", providerLabel: "Antigravity", isLegacy: true },
  { id: "Gemini 2.5 Flash", label: "Gemini 2.5 Flash", providerId: "antigravity", providerLabel: "Antigravity", isLegacy: true },
  { id: "Gemini 2.0 Flash", label: "Gemini 2.0 Flash", providerId: "antigravity", providerLabel: "Antigravity", isLegacy: true },
];

const OPENCODE_MODELS: UnifiedModelItem[] = [
  { id: "Nemotron 3.5 Lightning Free", label: "Nemotron 3.5 Lightning Free", providerId: "opencode", providerLabel: "OpenCode", isFree: true },
  { id: "Big Pickle", label: "Big Pickle", providerId: "opencode", providerLabel: "OpenCode", isFree: true },
  { id: "Nano Banana Pro", label: "Nano Banana Pro", providerId: "opencode", providerLabel: "OpenCode" },
  { id: "DeepSeek R1 Free", label: "DeepSeek R1 Free", providerId: "opencode", providerLabel: "OpenCode", isFree: true },
  { id: "Qwen 2.5 72B Free", label: "Qwen 2.5 72B Free", providerId: "opencode", providerLabel: "OpenCode", isFree: true },
];

const DEFAULT_FAVORITES: UnifiedModelItem[] = [
  { id: "Claude Fable 5.1", label: "Claude Fable 5.1", providerId: "claude", providerLabel: "Claude", isNew: true },
  { id: "Claude Opus 5", label: "Claude Opus 5", providerId: "claude", providerLabel: "Claude" },
  { id: "Claude Sonnet 5", label: "Claude Sonnet 5", providerId: "claude", providerLabel: "Claude" },
  { id: "Gemini 3.8 Flash", label: "Gemini 3.8 Flash", providerId: "antigravity", providerLabel: "Antigravity", isNew: true },
  { id: "GPT-5.6 Sol", label: "GPT-5.6 Sol", providerId: "codex", providerLabel: "OpenAI", isNew: true },
  { id: "Grok 4.6", label: "Grok 4.6", providerId: "grok", providerLabel: "Grok", isNew: true },
];

const FAV_STORAGE_KEY = "bhippi_unified_fav_models_v2";

function loadFavoriteKeys(): string[] {
  try {
    const raw = localStorage.getItem(FAV_STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) return parsed;
    }
  } catch {}
  return DEFAULT_FAVORITES.map((f) => `${f.providerId.toLowerCase()}::${f.id.toLowerCase()}`);
}

function saveFavoriteKeys(keys: string[]) {
  try {
    localStorage.setItem(FAV_STORAGE_KEY, JSON.stringify(keys));
  } catch {}
}

const RAIL_PROVIDERS: { id: string; label: string }[] = [
  { id: "claude", label: "Claude" },
  { id: "codex", label: "OpenAI" },
  { id: "grok", label: "Grok" },
  { id: "opencode", label: "OpenCode" },
  { id: "antigravity", label: "Antigravity" },
];

const PROVIDER_KEYWORDS: Record<string, string[]> = {
  claude: ["claude", "anthropic", "sonnet", "opus", "haiku", "fable"],
  codex: ["openai", "codex", "chatgpt", "gpt", "o1", "o3", "o3-mini", "4o"],
  openai: ["openai", "codex", "chatgpt", "gpt", "o1", "o3", "o3-mini", "4o"],
  grok: ["grok", "xai", "twitter", "musk"],
  opencode: ["opencode", "openrouter", "free", "nemotron", "deepseek", "qwen", "pickle", "banana"],
  antigravity: ["antigravity", "agy", "google", "gemini", "deepmind", "flash", "pro"],
  agy: ["antigravity", "agy", "google", "gemini", "deepmind", "flash", "pro"],
};

export function UnifiedModelPicker({
  providers,
  currentProviderId,
  currentModel,
  open,
  onOpenChange,
  onSelect,
}: UnifiedModelPickerProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const [activeTab, setActiveTab] = useState<string>("claude");
  const [searchQuery, setSearchQuery] = useState("");
  const [favoriteKeys, setFavoriteKeys] = useState<string[]>(loadFavoriteKeys);
  const [legacyExpanded, setLegacyExpanded] = useState<Record<string, boolean>>({});

  // Sync activeTab with currentProviderId when opening
  useEffect(() => {
    if (open && currentProviderId) {
      const low = currentProviderId.toLowerCase();
      if (low === "openai") setActiveTab("codex");
      else if (low === "agy") setActiveTab("antigravity");
      else setActiveTab(low);
    }
  }, [open, currentProviderId]);

  // Outside click & keyboard listener
  useEffect(() => {
    if (!open) {
      setSearchQuery("");
      return undefined;
    }

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (!target || !containerRef.current) return;
      if (!containerRef.current.contains(target)) {
        onOpenChange(false);
      }
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onOpenChange(false);
      }
    };

    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);

    const timer = setTimeout(() => {
      inputRef.current?.focus();
    }, 40);

    return () => {
      clearTimeout(timer);
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open, onOpenChange]);

  const activeProvider = useMemo(() => {
    return (
      providers.find((p) => p.id.toLowerCase() === (currentProviderId ?? "").toLowerCase()) ??
      providers[0] ??
      null
    );
  }, [providers, currentProviderId]);

  const activeProviderId = activeProvider?.id.toLowerCase() ?? "claude";

  // Build the complete combined catalogue across all providers
  const allModels = useMemo<UnifiedModelItem[]>(() => {
    const list: UnifiedModelItem[] = [];

    const addList = (
      pId: string,
      pLabel: string,
      presets: UnifiedModelItem[],
      detected: string[] = [],
    ) => {
      const merged = [...presets];
      for (const raw of detected) {
        if (!merged.some((e) => e.id.toLowerCase() === raw.toLowerCase())) {
          merged.push({
            id: raw,
            label: isAntigravityProvider(pId) ? antigravityDisplayName(raw) : shortModelName(raw),
            providerId: pId,
            providerLabel: pLabel,
          });
        }
      }
      list.push(...merged);
    };

    // 1. Claude
    const claudeRow = providers.find((p) => p.id.toLowerCase() === "claude");
    addList("claude", "Claude", CLAUDE_MODELS, claudeRow?.models);

    // 2. Codex / OpenAI
    const codexRow = providers.find((p) => p.id.toLowerCase() === "codex" || p.id.toLowerCase() === "openai");
    addList("codex", "OpenAI", CODEX_MODELS, codexRow?.models);

    // 3. Grok
    const grokRow = providers.find((p) => p.id.toLowerCase() === "grok");
    addList("grok", "Grok", GROK_MODELS, grokRow?.models);

    // 4. OpenCode
    const opencodeRow = providers.find((p) => p.id.toLowerCase() === "opencode");
    addList("opencode", "OpenCode", OPENCODE_MODELS, opencodeRow?.models);

    // 5. Antigravity
    const agyRow = providers.find((p) => isAntigravityProvider(p.id));
    const agyModels = agyRow ? collapseAntigravityModels(agyRow.models).map((m) => m.label) : [];
    addList("antigravity", "Antigravity", ANTIGRAVITY_MODELS, agyModels);

    // 6. Other connected providers
    for (const p of providers) {
      const low = p.id.toLowerCase();
      if (!["claude", "codex", "openai", "grok", "opencode", "antigravity", "agy"].includes(low)) {
        for (const m of p.models) {
          list.push({
            id: m,
            label: shortModelName(m),
            providerId: low,
            providerLabel: p.label,
          });
        }
      }
    }

    return list;
  }, [providers]);

  // Model key helper
  const modelKey = (pId: string, mId: string) =>
    `${pId.toLowerCase()}::${mId.toLowerCase()}`;

  const isFavorite = (pId: string, mId: string) =>
    favoriteKeys.includes(modelKey(pId, mId));

  const toggleFavorite = (item: UnifiedModelItem, e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const key = modelKey(item.providerId, item.id);
    const exists = favoriteKeys.includes(key);
    const next = exists ? favoriteKeys.filter((k) => k !== key) : [...favoriteKeys, key];
    setFavoriteKeys(next);
    saveFavoriteKeys(next);
  };

  // Check if a model is currently active
  const isModelSelected = (item: UnifiedModelItem) => {
    const curP = (currentProviderId ?? "").toLowerCase();
    const itemP = item.providerId.toLowerCase();
    const isP =
      curP === itemP ||
      (isAntigravityProvider(curP) && isAntigravityProvider(itemP)) ||
      ((curP === "openai" || curP === "codex") && (itemP === "openai" || itemP === "codex"));

    if (!isP) return false;

    if (isAntigravityProvider(curP)) {
      const curFam = antigravityFamilyId(currentModel ?? "gemini-3.8-flash");
      const itemFam = antigravityFamilyId(item.id);
      return curFam.toLowerCase() === itemFam.toLowerCase();
    }

    const curM = (currentModel ?? "").toLowerCase();
    const itemM = item.id.toLowerCase();
    const itemL = item.label.toLowerCase();
    return curM === itemM || curM === itemL;
  };

  // Global search across ALL providers or tab filter
  const isSearching = searchQuery.trim().length > 0;

  const { activeItems, legacyItems } = useMemo(() => {
    if (isSearching) {
      // Global search across ALL models & providers
      const rawQ = searchQuery.toLowerCase().trim();
      const terms = rawQ.split(/\s+/).filter(Boolean);

      const matched = allModels.filter((m) => {
        const idLower = m.id.toLowerCase();
        const labelLower = m.label.toLowerCase();
        const pIdLower = m.providerId.toLowerCase();
        const pLabelLower = m.providerLabel.toLowerCase();
        const keywords = PROVIDER_KEYWORDS[pIdLower] ?? [];
        const freeTag = m.isFree ? "free" : "";
        const newTag = m.isNew ? "new" : "";

        return terms.every((term) =>
          labelLower.includes(term) ||
          idLower.includes(term) ||
          pLabelLower.includes(term) ||
          pIdLower.includes(term) ||
          freeTag.includes(term) ||
          newTag.includes(term) ||
          keywords.some((kw) => kw.includes(term) || term.includes(kw))
        );
      });
      return { activeItems: matched, legacyItems: [] };
    }

    if (activeTab === "favorites") {
      const favs = allModels.filter((m) => isFavorite(m.providerId, m.id));
      return {
        activeItems: favs.length > 0 ? favs : DEFAULT_FAVORITES,
        legacyItems: [],
      };
    }

    // Provider tab
    const pModels = allModels.filter((m) => {
      const pId = m.providerId.toLowerCase();
      const tab = activeTab.toLowerCase();
      if (tab === "antigravity" && (pId === "agy" || pId === "antigravity")) return true;
      if (tab === "codex" && (pId === "codex" || pId === "openai")) return true;
      return pId === tab;
    });

    const active = pModels.filter((m) => !m.isLegacy);
    const legacy = pModels.filter((m) => m.isLegacy);
    return { activeItems: active, legacyItems: legacy };
  }, [allModels, activeTab, favoriteKeys, isSearching, searchQuery]);

  const isLegacyOpen = Boolean(legacyExpanded[activeTab]);

  const toggleLegacy = () => {
    setLegacyExpanded((prev) => ({
      ...prev,
      [activeTab]: !prev[activeTab],
    }));
  };

  // Keyboard shortcut handler (Ctrl+1, Ctrl+2, etc.)
  useEffect(() => {
    if (!open) return undefined;

    const onKey = (e: KeyboardEvent) => {
      // Check Ctrl+1 .. Ctrl+9
      if (e.ctrlKey && !e.shiftKey && !e.altKey) {
        const num = parseInt(e.key, 10);
        if (num >= 1 && num <= 9) {
          const target = activeItems[num - 1];
          if (target) {
            e.preventDefault();
            onSelect(target.providerId, target.id);
            onOpenChange(false);
          }
        }
      }
    };

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, activeItems, onSelect, onOpenChange]);

  // Active trigger label in composer
  const triggerLabel = useMemo(() => {
    if (!currentModel) {
      if (isAntigravityProvider(activeProviderId)) return "Gemini 3.8 Flash";
      if (activeProviderId === "claude") return "Claude Fable 5.1";
      if (activeProviderId === "grok") return "Grok 4.6";
      if (activeProviderId === "opencode") return "Nemotron 3.5";
      return "GPT-5.6 Sol";
    }
    if (isAntigravityProvider(activeProviderId)) {
      return antigravityDisplayName(currentModel);
    }
    return shortModelName(currentModel);
  }, [currentModel, activeProviderId]);

  return (
    <div className="composer-popover-anchor" ref={containerRef}>
      {/* ── Single Compact Trigger Button in Composer ── */}
      <button
        type="button"
        className={`composer-bar-btn unified-model-trigger${open ? " active" : ""}`}
        onClick={() => onOpenChange(!open)}
        aria-label={`Model: ${activeProvider?.label ?? "Provider"} · ${triggerLabel}`}
        aria-expanded={open}
        title={`${activeProvider?.label ?? "Provider"} · ${triggerLabel}`}
      >
        <ProviderLogo id={activeProviderId} size={13} transparent={activeProviderId === "claude"} />
        <span className="model-trigger-text">{triggerLabel}</span>
        <IconChevronDown size={9} />
      </button>

      {/* ── Two-Column Studio Model Picker Popover ── */}
      {open ? (
        <div
          className="bhippi-popover studio-model-popover"
          role="dialog"
          aria-label="Select Model and Provider"
        >
          {/* Left Vertical Rail / Sidebar */}
          <div className="studio-model-rail">
            {/* Favorites Tab */}
            <button
              type="button"
              className={`rail-tab-btn fav${activeTab === "favorites" ? " active" : ""}`}
              onClick={() => {
                setActiveTab("favorites");
                setSearchQuery("");
              }}
              title="Favorites"
            >
              <IconStarFilled size={13} className="rail-star-icon" />
              {activeTab === "favorites" ? <span className="rail-active-indicator" /> : null}
            </button>

            {/* Provider Tabs */}
            {RAIL_PROVIDERS.map((p) => {
              const isTabActive = activeTab.toLowerCase() === p.id.toLowerCase();
              return (
                <button
                  key={p.id}
                  type="button"
                  className={`rail-tab-btn${isTabActive ? " active" : ""}`}
                  onClick={() => {
                    setActiveTab(p.id);
                    setSearchQuery("");
                  }}
                  title={p.label}
                >
                  <ProviderLogo id={p.id} size={14} transparent />
                  {isTabActive ? <span className="rail-active-indicator" /> : null}
                </button>
              );
            })}
          </div>

          {/* Right Main Content Area */}
          <div className="studio-model-main">
            {/* Search Input Header */}
            <div className="studio-model-search-row">
              <IconSearch size={12} className="studio-search-icon" />
              <input
                ref={inputRef}
                type="text"
                placeholder="Search models..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
              {searchQuery ? (
                <button
                  type="button"
                  className="studio-search-clear"
                  onClick={() => setSearchQuery("")}
                  title="Clear search"
                >
                  ×
                </button>
              ) : null}
            </div>

            {/* Models Scrollable List */}
            <div className="studio-model-list">
              {activeItems.length === 0 && legacyItems.length === 0 ? (
                <div className="studio-model-empty">
                  {isSearching
                    ? `No models found matching "${searchQuery}".`
                    : "No favorite models yet. Click the star on any model to pin it here."}
                </div>
              ) : (
                <>
                  {/* Active / Current Models */}
                  {activeItems.map((item, idx) => {
                    const selected = isModelSelected(item);
                    const fav = isFavorite(item.providerId, item.id);
                    const shortcutNum = idx + 1;

                    return (
                      <div
                        key={`${item.providerId}-${item.id}`}
                        className={`studio-model-card${selected ? " selected" : ""}`}
                        onClick={() => {
                          onSelect(item.providerId, item.id);
                          onOpenChange(false);
                        }}
                        title={`${item.label} (${item.providerLabel})`}
                      >
                        <div className="card-left">
                          <div className="card-title-row">
                            <span className="card-title">{item.label}</span>
                            {item.isNew ? <span className="card-badge new">NEW</span> : null}
                            {item.isFree ? <span className="card-badge free">FREE</span> : null}
                          </div>

                          <div className="card-sub-row">
                            <ProviderLogo id={item.providerId} size={10} transparent />
                            <span className="card-provider-name">{item.providerLabel}</span>
                          </div>
                        </div>

                        <div className="card-right">
                          {shortcutNum <= 9 ? (
                            <span className="card-shortcut">Ctrl+{shortcutNum}</span>
                          ) : null}

                          <button
                            type="button"
                            className={`card-star-btn${fav ? " active" : ""}`}
                            onClick={(e) => toggleFavorite(item, e)}
                            title={fav ? "Remove favorite" : "Add to favorites"}
                          >
                            {fav ? <IconStarFilled size={11} /> : <IconStar size={11} />}
                          </button>
                        </div>
                      </div>
                    );
                  })}

                  {/* Collapsible Legacy Models Section */}
                  {legacyItems.length > 0 && !isSearching ? (
                    <div className="studio-legacy-section">
                      <div className="studio-legacy-header" onClick={toggleLegacy}>
                        <div className="legacy-header-text">
                          <span className="legacy-title">Legacy models</span>
                          <span className="legacy-count">{legacyItems.length} models</span>
                        </div>
                        <span className="legacy-chevron">
                          {isLegacyOpen ? <IconChevronDown size={11} /> : <IconChevronRight size={11} />}
                        </span>
                      </div>

                      {isLegacyOpen ? (
                        <div className="studio-legacy-list">
                          {legacyItems.map((item, idx) => {
                            const selected = isModelSelected(item);
                            const fav = isFavorite(item.providerId, item.id);
                            const shortcutNum = activeItems.length + idx + 1;

                            return (
                              <div
                                key={`${item.providerId}-${item.id}`}
                                className={`studio-model-card legacy${selected ? " selected" : ""}`}
                                onClick={() => {
                                  onSelect(item.providerId, item.id);
                                  onOpenChange(false);
                                }}
                                title={`${item.label} (${item.providerLabel})`}
                              >
                                <div className="card-left">
                                  <div className="card-title-row">
                                    <span className="card-title">{item.label}</span>
                                  </div>

                                  <div className="card-sub-row">
                                    <ProviderLogo id={item.providerId} size={10} transparent />
                                    <span className="card-provider-name">{item.providerLabel}</span>
                                  </div>
                                </div>

                                <div className="card-right">
                                  {shortcutNum <= 9 ? (
                                    <span className="card-shortcut">Ctrl+{shortcutNum}</span>
                                  ) : null}

                                  <button
                                    type="button"
                                    className={`card-star-btn${fav ? " active" : ""}`}
                                    onClick={(e) => toggleFavorite(item, e)}
                                    title={fav ? "Remove favorite" : "Add to favorites"}
                                  >
                                    {fav ? <IconStarFilled size={11} /> : <IconStar size={11} />}
                                  </button>
                                </div>
                              </div>
                            );
                          })}
                        </div>
                      ) : null}
                    </div>
                  ) : null}
                </>
              )}
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
