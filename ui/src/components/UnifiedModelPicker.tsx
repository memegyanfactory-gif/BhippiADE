import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { ProviderInfo } from "../lib/ipc";
import { IconChevronDown, IconSearch, IconStar, IconStarFilled } from "./icons";
import { ProviderLogo } from "./ProviderLogo";
import { antigravityDisplayName, isAntigravityProvider } from "../lib/antigravityModels";
import { shortModelName } from "./ComposerPopovers";
import { useObstructsViewport } from "../lib/useViewportObstruction";
import type { SettingsTab } from "../screens/SettingsModal";

export interface UnifiedModelPickerProps {
  providers: ProviderInfo[];
  currentProviderId: string | null;
  currentModel: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (providerId: string, modelId: string | null) => void;
  onOpenSettings?: (tab?: SettingsTab) => void;
}

const FAV_STORAGE_KEY = "bhippi_unified_fav_models_v3";
function loadFavorites(): string[] {
  try {
    const keys: unknown = JSON.parse(localStorage.getItem(FAV_STORAGE_KEY) ?? "[]");
    return Array.isArray(keys) ? keys.filter((key): key is string => typeof key === "string") : [];
  } catch { return []; }
}
const modelKey = (provider: string, model: string) => `${provider}::${model}`;
const modelLabel = (provider: string, model: string) =>
  isAntigravityProvider(provider) ? antigravityDisplayName(model) : shortModelName(model);

export function UnifiedModelPicker({ providers, currentProviderId, currentModel, open,
  onOpenChange, onSelect, onOpenSettings }: UnifiedModelPickerProps) {
  const anchor = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [activeTab, setActiveTab] = useState(currentProviderId ?? providers[0]?.id ?? "");
  const [query, setQuery] = useState("");
  const [customModel, setCustomModel] = useState("");
  const [favorites, setFavorites] = useState(loadFavorites);
  const [position, setPosition] = useState({ left: 8, bottom: 48, width: 360, maxHeight: 460 });
  useObstructsViewport(open);
  const activeProvider = providers.find(p => p.id === currentProviderId);
  const tabProvider = providers.find(p => p.id === activeTab);
  const label = currentModel ? modelLabel(currentProviderId ?? "", currentModel) : "Provider default";
  useEffect(() => {
    if (open) { setActiveTab(currentProviderId ?? providers[0]?.id ?? ""); input.current?.focus(); }
    else { setQuery(""); setCustomModel(""); }
  }, [open, currentProviderId]);
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const rect = anchor.current?.getBoundingClientRect();
      if (!rect) return;
      const width = Math.min(420, window.innerWidth - 16);
      const bottom = Math.max(8, window.innerHeight - rect.top + 8);
      setPosition({ width, bottom, left: Math.max(8, Math.min(rect.right - width, window.innerWidth - width - 8)),
        maxHeight: Math.max(120, window.innerHeight - bottom - 8) });
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => { window.removeEventListener("resize", place); window.removeEventListener("scroll", place, true); };
  }, [open]);
  const rows = useMemo(() => providers.flatMap(p => [...new Set(p.models)].map(id => ({
    id, providerId: p.id, providerLabel: p.label, label: modelLabel(p.id, id),
  }))), [providers]);
  const visible = rows.filter(row => query.trim()
    ? `${row.id} ${row.label} ${row.providerLabel}`.toLowerCase().includes(query.trim().toLowerCase())
    : activeTab === "favorites" ? favorites.includes(modelKey(row.providerId, row.id)) : row.providerId === activeTab);
  const select = (provider: string, model: string | null) => { onSelect(provider, model); onOpenChange(false); };
  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      const node = event.target as Node;
      if (!anchor.current?.contains(node) && !panel.current?.contains(node)) onOpenChange(false);
    };
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); onOpenChange(false); }
      if (event.ctrlKey && !event.shiftKey && !event.altKey && /^[1-9]$/.test(event.key)) {
        const row = visible[Number(event.key) - 1];
        if (row) { event.preventDefault(); select(row.providerId, row.id); }
      }
    };
    window.addEventListener("pointerdown", outside, true);
    window.addEventListener("keydown", keyboard);
    return () => { window.removeEventListener("pointerdown", outside, true); window.removeEventListener("keydown", keyboard); };
  }, [open, onOpenChange, onSelect, visible]);
  const toggleFavorite = (provider: string, model: string) => {
    const key = modelKey(provider, model);
    const next = favorites.includes(key) ? favorites.filter(k => k !== key) : [...favorites, key];
    setFavorites(next);
    try { localStorage.setItem(FAV_STORAGE_KEY, JSON.stringify(next)); } catch {}
  };
  return <div ref={anchor} className="composer-popover-anchor" onClick={event => event.stopPropagation()}>
    <button type="button" className={`composer-bar-btn unified-model-trigger${open ? " active" : ""}`}
      onClick={() => onOpenChange(!open)} aria-label={`Model: ${activeProvider?.label ?? "Provider"} · ${label}`}
      aria-expanded={open} title={`${activeProvider?.label ?? "Provider"} · ${label}`}>
      <ProviderLogo id={currentProviderId ?? ""} size={13} />
      <span className="model-trigger-text">{label}</span><IconChevronDown size={9} />
    </button>
    {open ? createPortal(<div ref={panel} className="bhippi-popover studio-model-popover" role="dialog"
      aria-label="Select Model and Provider" style={{ position: "fixed", ...position, zIndex: 10000 }}
      onClick={event => event.stopPropagation()} onPointerDown={event => event.stopPropagation()}>
      <div className="studio-model-rail">
        <button type="button" className={`rail-tab-btn fav${activeTab === "favorites" ? " active" : ""}`}
          title="Favorites" onClick={() => { setActiveTab("favorites"); setQuery(""); }}><IconStarFilled size={13} /></button>
        {providers.map(p => <button key={p.id} type="button" title={p.label}
          className={`rail-tab-btn${activeTab === p.id ? " active" : ""}`}
          onClick={() => { setActiveTab(p.id); setQuery(""); setCustomModel(""); }}><ProviderLogo id={p.id} size={14} /></button>)}
      </div>
      <div className="studio-model-main">
        <div className="studio-model-search-row"><IconSearch size={12} />
          <input ref={input} placeholder="Search models..." value={query} onChange={event => setQuery(event.target.value)} />
        </div>
        <div className="studio-model-list">
          {!query && tabProvider ? <button type="button" className="studio-model-card"
            onClick={() => select(tabProvider.id, null)}>Use {tabProvider.label} default</button> : null}
          {visible.map((row, index) => <div key={modelKey(row.providerId, row.id)}
            className={`studio-model-card${row.providerId === currentProviderId && row.id === currentModel ? " selected" : ""}`}>
            <button type="button" className="card-left" title={row.id} onClick={() => select(row.providerId, row.id)}>
              <span className="card-title">{row.label}</span><span className="card-provider-name">{row.providerLabel}</span>
            </button>
            <div className="card-right">{index < 9 ? <span className="card-shortcut">Ctrl+{index + 1}</span> : null}
              <button type="button" className="card-star-btn" title={favorites.includes(modelKey(row.providerId, row.id)) ? "Remove favorite" : "Add to favorites"}
                onClick={() => toggleFavorite(row.providerId, row.id)}>{favorites.includes(modelKey(row.providerId, row.id)) ? <IconStarFilled size={11} /> : <IconStar size={11} />}</button>
            </div>
          </div>)}
          {visible.length === 0 ? <div className="studio-model-empty">{query ? "No matching models." : activeTab === "favorites"
            ? "Star a model to add it here." : "This provider did not publish a model list. Use its default or enter a model ID below if supported."}</div> : null}
          {!query && tabProvider?.accepts_custom_model ? <form className="studio-model-search-row" onSubmit={event => {
            event.preventDefault(); if (customModel.trim()) select(tabProvider.id, customModel.trim());
          }}><input aria-label="Custom model ID" placeholder="Exact model ID" value={customModel} onChange={event => setCustomModel(event.target.value)} />
            <button type="submit" disabled={!customModel.trim()}>Use</button></form> : null}
        </div>
        {onOpenSettings ? <button type="button" className="composer-bar-btn" onClick={() => { onOpenChange(false); onOpenSettings("Providers"); }}>Manage providers</button> : null}
      </div>
    </div>, document.body) : null}
  </div>;
}
