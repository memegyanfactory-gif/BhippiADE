import React, { useEffect, useMemo, useRef, useState } from "react";
import type { ProviderInfo } from "../lib/ipc";
import {
  IconAttach,
  IconBolt,
  IconCheck,
  IconChevronDown,
  IconChevronRight,
  IconHand,
  IconMonitor,
  IconPalette,
  IconSearch,
  IconShield,
  IconPlus,
  IconStar,
  IconStarFilled,
} from "./icons";
import { ProviderLogo } from "./ProviderLogo";
import {
  antigravityDisplayName,
  antigravityFamilyId,
  collapseAntigravityModels,
  getSupportedSpeedsForAntigravityModel,
  isAntigravityProvider,
  resolveAntigravitySlug,
  type AntigravitySpeed,
} from "../lib/antigravityModels";

export type Effort = "fast" | "medium" | "balanced" | "extra" | "quality" | "ultra";
export type PermissionMode = "ask_approval" | "auto" | "full_access";

/**
 * Whether a posture puts a permission card to the user rather than answering it for them.
 *
 * The mirror of `PermissionPosture::asks_first` in `bhippi-core`. It is a table rather than
 * an inline `mode !== "ask_approval"` so that adding a fourth posture makes every reader of
 * this fact fail to compile instead of silently defaulting to "does not ask".
 */
export const PERMISSION_ASKS_FIRST: Record<PermissionMode, boolean> = {
  ask_approval: true,
  auto: false,
  full_access: false,
};

function useClickOutside<T extends HTMLElement>(isOpen: boolean, onClose: () => void) {
  const ref = useRef<T | null>(null);

  useEffect(() => {
    if (!isOpen) return undefined;

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (!target || !ref.current) return;
      if (!ref.current.contains(target)) {
        onClose();
      }
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    };

    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);

    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [isOpen, onClose]);

  return ref;
}

/* ────────────────────────────────────────────────────────────────────────── */
/* 1. PROVIDER POPOVER (Screenshot 1)                                        */
/* ────────────────────────────────────────────────────────────────────────── */

const KNOWN_PROVIDER_CATALOG: { id: string; label: string }[] = [
  { id: "claude", label: "Claude" },
  { id: "codex", label: "Codex" },
  { id: "grok", label: "Grok" },
  { id: "antigravity", label: "Antigravity" },
  { id: "kimi", label: "Kimi" },
  { id: "opencode", label: "OpenCode" },
  { id: "custom", label: "Custom" },
  { id: "local_models", label: "Local models" },
];

export function ProviderPopover({
  providers,
  currentId,
  open,
  onOpenChange,
  onSelect,
}: {
  providers: ProviderInfo[];
  currentId: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (id: string) => void;
}) {
  const containerRef = useClickOutside<HTMLDivElement>(open, () => onOpenChange(false));
  const activeMap = new Map(providers.map((p) => [p.id.toLowerCase(), p]));

  // Find active or fallback label
  const active =
    providers.find((p) => p.id.toLowerCase() === (currentId ?? "").toLowerCase()) ??
    providers[0] ??
    null;

  return (
    <div className="composer-popover-anchor" ref={containerRef}>
      <button
        type="button"
        className={`composer-bar-btn provider-trigger${open ? " active" : ""}`}
        onClick={() => onOpenChange(!open)}
        aria-label={`Provider: ${active?.label ?? "Select provider"}`}
        aria-expanded={open}
        title={active?.label ?? "Select provider"}
      >
        <ProviderLogo id={active?.id ?? "demo"} size={16} />
        <IconChevronDown size={10} />
      </button>

      {open ? (
        <div className="bhippi-popover provider-popover" role="dialog" aria-label="Choose a provider">
          <div className="popover-head-simple">Provider</div>
          <div className="popover-item-list">
            {KNOWN_PROVIDER_CATALOG.map((item) => {
              const connected = activeMap.has(item.id) || providers.some((p) => p.label.toLowerCase() === item.label.toLowerCase());
              const isSelected =
                active?.id.toLowerCase() === item.id ||
                active?.label.toLowerCase() === item.label.toLowerCase();
              const resolvedId = activeMap.get(item.id)?.id ?? item.id;

              return (
                <button
                  key={item.id}
                  type="button"
                  className={`popover-row-btn${isSelected ? " selected" : ""}${!connected ? " disabled" : ""}`}
                  disabled={!connected}
                  onClick={() => {
                    if (connected) {
                      onSelect(resolvedId);
                      onOpenChange(false);
                    }
                  }}
                >
                  <span className="popover-row-left">
                    <ProviderLogo id={item.id} size={18} />
                    <span className="popover-row-name">{item.label}</span>
                  </span>
                  <span className="popover-row-right">
                    {!connected ? (
                      <span className="popover-muted-tag">Not connected</span>
                    ) : isSelected ? (
                      <IconCheck size={14} />
                    ) : null}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}

/* ────────────────────────────────────────────────────────────────────────── */
/* 2. MODEL POPOVER (Screenshots 2 & 4)                                      */
/* ────────────────────────────────────────────────────────────────────────── */

// Preset Claude models. The row is a name and, at most, one muted word of meta —
// the blue capability dot meters that used to sit here read as noise at this width.
const CLAUDE_PRESETS = [
  { id: "Fable 5 (1M)" },
  { id: "Opus 5 (1M)" },
  { id: "Sonnet 5" },
  { id: "Sonnet 5 (1M)" },
  { id: "Haiku 4.5" },
];

/**
 * Presentation only: a trailing `(1M)` is the model's context window, and it reads
 * better as muted meta on the right than as part of the name. The full id is what
 * gets selected and compared — this only decides what the row prints.
 */
export function splitModelMeta(id: string): { name: string; meta: string | null } {
  const match = id.match(/^(.+?)\s*\(([^()]{1,12})\)$/);
  if (match && match[1] && match[2]) return { name: match[1], meta: match[2] };
  return { name: id, meta: null };
}

/**
 * `opencode/big-pickle` → `big-pickle`, `openrouter/qwen/qwen-2.5-72b` → `qwen-2.5-72b`
 * (SPA-406). The backend a catalogue prefixes onto an id is the group the row sits under,
 * not part of the model's name — so the trigger and the rows stay short.
 */
/** Display labels the picker shows → the id the vendor CLI actually accepts. */
export function vendorModelId(
  providerId: string | null,
  model: string | null,
  effort?: string | null,
  catalog?: readonly string[] | null,
): string | null {
  if (!model) return null;
  if (isAntigravityProvider(providerId)) {
    return resolveAntigravitySlug(model, effort, catalog);
  }
  const key = model.trim().toLowerCase();
  const aliases: Record<string, string> = {
    "fable 5 (1m)": "fable",
    "fable 5": "fable",
    "opus 5 (1m)": "opus",
    "opus 5": "opus",
    "sonnet 5 (1m)": "sonnet",
    "sonnet 5": "sonnet",
    "haiku 4.5": "haiku",
    "grok 4.6": "grok-4.6",
    "grok 2.5 vision": "grok-2-vision",
    "grok beta": "grok-beta",
    "gpt-5 codex": "gpt-5-codex",
    "nemotron 3.5 lightning free": "opencode/nemotron-3.5-nano-free",
    "big pickle": "opencode/big-pickle",
  };
  if (aliases[key]) return aliases[key];
  const provider = providerId?.toLowerCase() ?? "";
  if (provider.includes("claude") && ["fable", "opus", "sonnet", "haiku"].includes(key)) {
    return key;
  }
  return model;
}

export function shortModelName(id: string): string {
  const { name } = splitModelMeta(id);
  const cut = name.lastIndexOf("/");
  return cut >= 0 ? name.slice(cut + 1) : name;
}

/** The backend prefix of an id (`openrouter/…` → `Openrouter`), or the fallback. */
export function modelGroup(id: string, fallback: string | null): string | null {
  const cut = id.indexOf("/");
  if (cut > 0) {
    const head = id.slice(0, cut);
    return head.charAt(0).toUpperCase() + head.slice(1);
  }
  return fallback;
}

type ModelRow = { id: string; isFree?: boolean; backend?: string; label?: string };

/**
 * Rows under the backend that serves them. One backend needs no head at all; a mixed
 * list gets one head per backend, so `big-pickle` sits under "OpenCode Zen" rather than
 * carrying `opencode/` in its own name.
 */
export function groupModels(
  items: readonly ModelRow[],
  fallbackHead: string | null,
): { head: string | null; items: ModelRow[] }[] {
  const groups = new Map<string, ModelRow[]>();
  for (const item of items) {
    const head = item.backend ?? modelGroup(item.id, fallbackHead) ?? "";
    const list = groups.get(head);
    if (list) list.push(item);
    else groups.set(head, [item]);
  }
  const entries = [...groups.entries()].map(([head, list]) => ({ head: head || null, items: list }));
  return entries.length <= 1 ? entries.map((group) => ({ ...group, head: null })) : entries;
}

// Preset OpenCode models with Free/Paid tags matching Screenshot 4
const OPENCODE_PRESETS = [
  { id: "Nano Banana Pro", isFree: false, backend: "OpenRouter" },
  { id: "Nemotron 3.5 Lightning Free", isFree: true, backend: "OpenCode Zen" },
  { id: "Big Pickle", isFree: true, backend: "OpenCode Zen" },
  { id: "DeepSeek R1 Free", isFree: true, backend: "OpenRouter" },
  { id: "Qwen 2.5 72B Free", isFree: true, backend: "OpenRouter" },
];

const GROK_PRESETS = [
  { id: "Grok 4.6", isFree: false, backend: "xAI" },
  { id: "Grok 2.5 Vision", isFree: false, backend: "xAI" },
  { id: "Grok Beta", isFree: false, backend: "xAI" },
];

const CODEX_PRESETS = [
  { id: "GPT-5 Codex", isFree: false, backend: "OpenAI" },
  { id: "o3-mini", isFree: false, backend: "OpenAI" },
  { id: "o1", isFree: false, backend: "OpenAI" },
  { id: "GPT-4o", isFree: false, backend: "OpenAI" },
];

export function ModelPopover({
  provider,
  currentModel,
  open,
  onOpenChange,
  onSelect,
}: {
  provider: ProviderInfo | null;
  currentModel: string | null;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (model: string | null) => void;
}) {
  const containerRef = useClickOutside<HTMLDivElement>(open, () => onOpenChange(false));
  const [showAllSearch, setShowAllSearch] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [favMap, setFavMap] = useState<Record<string, string[]>>(() => {
    try {
      const raw = localStorage.getItem("bhippi_fav_models");
      return raw ? JSON.parse(raw) : {};
    } catch {
      return {};
    }
  });

  const providerId = provider?.id.toLowerCase() ?? "claude";
  const antigravity = isAntigravityProvider(providerId);

  const toggleFav = (model: string, e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const list = favMap[providerId] ?? [];
    const exists = list.includes(model);
    const nextList = exists ? list.filter((m) => m !== model) : [...list, model];
    const nextMap = { ...favMap, [providerId]: nextList };
    setFavMap(nextMap);
    try {
      localStorage.setItem("bhippi_fav_models", JSON.stringify(nextMap));
    } catch {}
  };

  const isFav = (model: string) => (favMap[providerId] ?? []).includes(model);

  if (!provider) return null;

  // Selected label format
  const activeLabel = currentModel ?? provider.models[0] ?? (providerId.includes("claude") ? "Opus 5 (1M)" : providerId.includes("grok") ? "Grok 4.6" : providerId.includes("opencode") ? "Nemotron 3.5 Lightning Free" : "GPT-5 Codex");

  // Check which preset family matches
  const isClaude = providerId.includes("claude") || providerId.includes("anthropic");
  const isOpenCode = providerId.includes("opencode") || providerId.includes("openrouter");
  const isGrok = providerId.includes("grok") || providerId.includes("xai");

  // Build model catalog
  let baseList: ModelRow[] = [];

  if (antigravity) {
    baseList = collapseAntigravityModels(provider.models);
  } else if (isClaude) {
    baseList = [...CLAUDE_PRESETS];
    // merge dynamically discovered models if any
    for (const m of provider.models) {
      if (!baseList.some((b) => b.id.toLowerCase() === m.toLowerCase())) {
        baseList.push({ id: m });
      }
    }
  } else if (isOpenCode) {
    baseList = [...OPENCODE_PRESETS];
    for (const m of provider.models) {
      if (!baseList.some((b) => b.id.toLowerCase() === m.toLowerCase())) {
        const lower = m.toLowerCase();
        const free = lower.includes(":free") || lower.includes("free");
        baseList.push({ id: m, isFree: free, backend: "OpenRouter" });
      }
    }
  } else if (isGrok) {
    baseList = [...GROK_PRESETS];
    for (const m of provider.models) {
      if (!baseList.some((b) => b.id.toLowerCase() === m.toLowerCase())) {
        baseList.push({ id: m, isFree: false, backend: "xAI" });
      }
    }
  } else {
    baseList = [...CODEX_PRESETS];
    for (const m of provider.models) {
      if (!baseList.some((b) => b.id.toLowerCase() === m.toLowerCase())) {
        baseList.push({ id: m, isFree: false, backend: provider.label });
      }
    }
  }

  const filteredList = searchQuery.trim()
    ? baseList.filter((m) => {
        const q = searchQuery.toLowerCase();
        return (
          m.id.toLowerCase().includes(q) ||
          (m.label ?? "").toLowerCase().includes(q) ||
          shortModelName(m.id).toLowerCase().includes(q)
        );
      })
    : baseList;

  const triggerLabel = antigravity
    ? antigravityDisplayName(currentModel ?? provider.models[0] ?? "")
    : shortModelName(activeLabel);

  return (
    <div className="composer-popover-anchor" ref={containerRef}>
      <button
        type="button"
        className={`composer-bar-btn model-trigger${open ? " active" : ""}`}
        onClick={() => onOpenChange(!open)}
        aria-label={`Model: ${triggerLabel || activeLabel}`}
        aria-expanded={open}
        /* A long id like `opencode/big-pickle` used to wrap the whole strip onto a
           second line and drop the usage dot below it. The label ellipsises; the
           full name is one hover away. */
        title={triggerLabel || activeLabel}
      >
        <span className="model-trigger-text">{triggerLabel}</span>
        <IconChevronDown size={10} />
      </button>

      {open ? (
        <div className="bhippi-popover model-popover" role="dialog" aria-label={`${provider.label} model`}>
          {/* Header with Provider Icon and Title */}
          <div className="popover-head-row">
            <ProviderLogo id={provider.id} size={18} />
            <span className="popover-head-title">{provider.label} model</span>
          </div>

          {showAllSearch ? (
            <div className="popover-search-box">
              <IconSearch size={13} />
              <input
                autoFocus
                placeholder={`Search ${provider.label} models…`}
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
            </div>
          ) : null}

          {/* Model Item List — grouped under the backend that serves each row (SPA-406), so a
              row reads `big-pickle` under "OpenCode Zen" rather than `opencode/big-pickle`,
              and the panel stays narrow. */}
          <div className="popover-item-list model-list">
            {groupModels(filteredList, isOpenCode ? provider.label : null).map((group) => (
              <div key={group.head ?? "__all"} className="model-group">
                {group.head ? <div className="popover-group-head">{group.head}</div> : null}
                {group.items.map((item) => {
                  const isSelected = antigravity
                    ? antigravityFamilyId(currentModel ?? provider.models[0] ?? "") === item.id
                    : activeLabel.toLowerCase() === item.id.toLowerCase();
                  const fav = isFav(item.id);
                  const { meta } = splitModelMeta(item.id);
                  // One muted word at most: the context window for a paid catalogue, `Free`
                  // for OpenCode. The backend is the group head now, not a suffix.
                  // Antigravity speed lives on the effort control, never on the row.
                  const rowMeta = antigravity ? null : isOpenCode ? (item.isFree ? "Free" : null) : meta;
                  const rowName = item.label ?? shortModelName(item.id);

                  return (
                    <button
                      key={item.id}
                      type="button"
                      className={`popover-row-btn model-row${isSelected ? " selected" : ""}`}
                      onClick={() => {
                        onSelect(item.id);
                        onOpenChange(false);
                      }}
                      title={item.id}
                    >
                      <span className="popover-row-left">
                        {isOpenCode ? (
                          <span
                            className={`model-fav-star${fav ? " active" : ""}`}
                            onClick={(e) => toggleFav(item.id, e)}
                            title={fav ? "Remove favorite" : "Favorite"}
                          >
                            {fav ? <IconStarFilled size={13} /> : <IconStar size={13} />}
                          </span>
                        ) : null}

                        <span className="popover-row-name model-id-text">{rowName}</span>
                      </span>

                      <span className="popover-row-right">
                        {rowMeta ? <span className="model-meta-text">{rowMeta}</span> : null}
                        {isSelected ? <IconCheck size={14} /> : null}
                      </span>
                    </button>
                  );
                })}
              </div>
            ))}
          </div>

          {/* Footer: More models */}
          <div className="popover-foot-action">
            <button
              type="button"
              className="popover-more-btn"
              onClick={() => setShowAllSearch(!showAllSearch)}
            >
              <span>&gt;</span>
              <span>More models</span>
              <IconChevronRight size={11} />
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/* ────────────────────────────────────────────────────────────────────────── */
/* 3. THINKING / EFFORT POPOVER (Screenshot 3)                               */
/* ────────────────────────────────────────────────────────────────────────── */

export interface EffortStep {
  id: Effort;
  key: string;
  label: string;
  name: string;
  isUltra?: boolean;
}

const DEFAULT_EFFORT_STEPS: EffortStep[] = [
  { id: "fast", key: "low", label: "Low", name: "Low" },
  { id: "medium", key: "medium", label: "Medium", name: "Medium" },
  { id: "balanced", key: "high", label: "High", name: "High" },
  { id: "extra", key: "extra", label: "Extra", name: "Extra" },
  { id: "quality", key: "max", label: "Max", name: "Max" },
  { id: "ultra", key: "ultracode", label: "Ultracode", name: "Ultracode", isUltra: true },
];

export function getEffortStepsForModel(
  providerId: string | null | undefined,
  model: string | null | undefined,
  catalog?: readonly string[] | null | undefined,
): EffortStep[] {
  const pId = (providerId ?? "").trim().toLowerCase();

  // 1. Antigravity
  if (isAntigravityProvider(pId)) {
    const speeds = getSupportedSpeedsForAntigravityModel(model, catalog);
    if (speeds.length === 0) {
      const isThinking = model?.toLowerCase().includes("opus") || model?.toLowerCase().includes("claude");
      return [
        {
          id: "balanced",
          key: isThinking ? "thinking" : "standard",
          label: isThinking ? "Thinking" : "Standard",
          name: isThinking ? "Thinking" : "Standard",
          isUltra: isThinking,
        },
      ];
    }
    const stepMap: Record<AntigravitySpeed, EffortStep> = {
      low: { id: "fast", key: "low", label: "Low", name: "Low" },
      medium: { id: "medium", key: "medium", label: "Medium", name: "Medium" },
      high: { id: "balanced", key: "high", label: "High", name: "High" },
    };
    const steps = speeds.map((s) => ({ ...stepMap[s] }));
    // Highest effort level gets the ultracode type animation
    if (steps.length > 0) {
      steps[steps.length - 1].isUltra = true;
    }
    return steps;
  }

  // 2. Grok
  if (pId.includes("grok")) {
    return [
      { id: "fast", key: "low", label: "Low", name: "Low" },
      { id: "medium", key: "medium", label: "Medium", name: "Medium" },
      { id: "balanced", key: "high", label: "High", name: "High", isUltra: true },
    ];
  }

  // 3. Codex / OpenAI
  if (pId.includes("codex") || pId.includes("openai")) {
    return [
      { id: "fast", key: "low", label: "Low", name: "Low" },
      { id: "medium", key: "medium", label: "Medium", name: "Medium" },
      { id: "balanced", key: "high", label: "High", name: "High" },
      { id: "extra", key: "extra", label: "Extra", name: "Extra", isUltra: true },
    ];
  }

  // 4. Claude
  if (pId.includes("claude")) {
    return [
      { id: "fast", key: "low", label: "Low", name: "Low" },
      { id: "medium", key: "medium", label: "Medium", name: "Medium" },
      { id: "balanced", key: "high", label: "High", name: "High" },
      { id: "quality", key: "max", label: "Max", name: "Max", isUltra: true },
    ];
  }

  // 5. Local / unmetered
  if (pId.includes("opencode") || pId.includes("ollama") || pId.includes("lmstudio")) {
    return [
      { id: "balanced", key: "standard", label: "Standard", name: "Standard" },
    ];
  }

  return DEFAULT_EFFORT_STEPS;
}

export function ThinkingPopover({
  effort,
  open,
  onOpenChange,
  onSelect,
  providerId,
  currentModel,
  catalog,
}: {
  effort: Effort;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (effort: Effort) => void;
  providerId?: string | null;
  currentModel?: string | null;
  catalog?: readonly string[] | null;
}) {
  const containerRef = useClickOutside<HTMLDivElement>(open, () => onOpenChange(false));
  const trackRef = useRef<HTMLDivElement | null>(null);

  const steps = useMemo(
    () => getEffortStepsForModel(providerId, currentModel, catalog),
    [providerId, currentModel, catalog],
  );

  const activeStep = useMemo(() => {
    const direct = steps.find((s) => s.id === effort);
    if (direct) return direct;
    return steps[steps.length - 1] ?? DEFAULT_EFFORT_STEPS[0];
  }, [steps, effort]);

  const [activeKey, setActiveKey] = useState<string>(() => activeStep.key);

  useEffect(() => {
    setActiveKey(activeStep.key);
    if (!steps.some((s) => s.id === effort) && activeStep) {
      onSelect(activeStep.id);
    }
  }, [activeStep, steps, effort, onSelect]);

  const stepIndex = Math.max(0, steps.findIndex((s) => s.key === activeKey));
  const currentStep = steps[stepIndex] ?? activeStep;
  const fillPct = steps.length <= 1 ? 100 : (stepIndex / Math.max(1, steps.length - 1)) * 100;
  const isUltracode = Boolean(currentStep.isUltra);

  const selectStep = (next: EffortStep) => {
    setActiveKey(next.key);
    try {
      localStorage.setItem("bhippi_effort_step", next.key);
    } catch {}
    onSelect(next.id);
  };

  const pickFromClientX = (clientX: number) => {
    const rect = trackRef.current?.getBoundingClientRect();
    if (!rect || rect.width <= 0) return;
    if (steps.length <= 1) return;
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    const targetIdx = Math.round(ratio * (steps.length - 1));
    const next = steps[targetIdx];
    if (next) selectStep(next);
  };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    (e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId);
    pickFromClientX(e.clientX);
  };

  return (
    <div className="composer-popover-anchor" ref={containerRef}>
      <button
        type="button"
        className={`composer-bar-btn thinking-trigger${open ? " active" : ""}${isUltracode ? " ultracode" : ""}`}
        onClick={() => onOpenChange(!open)}
        aria-label={`Effort: ${currentStep.name}`}
        aria-expanded={open}
      >
        {/* The effort word sits in a slot as wide as the widest word this provider can
            show, so cycling Low → Medium → High does not shove the model chip beside it.
            The composer's right group is right-aligned, so any width change here moved
            everything to its left; a fixed slot is what stops that.

            The slot is sized by rendering every label into the same grid cell and hiding
            all but the current one. Measuring text in JS would need a font, a zoom level
            and a resize observer to stay right; this is exact for free. */}
        <span className="effort-slot">
          {steps.map((step) => (
            <span key={step.key} className="effort-slot-ghost" aria-hidden="true">
              {step.name}
            </span>
          ))}
          <span className="effort-slot-value">{currentStep.name}</span>
        </span>
        <IconChevronDown size={10} />
      </button>

      {open ? (
        <div
          className={`bhippi-popover thinking-popover tier-${currentStep.id}${isUltracode ? " ultracode" : ""}`}
          role="dialog"
          aria-label="Effort slider"
        >
          <div className="thinking-head-row">
            <span className="thinking-label">Effort</span>
            <strong className="thinking-val">{currentStep.name}</strong>
          </div>

          <div
            className="thinking-track-wrap"
            onPointerDown={onPointerDown}
            onPointerMove={(e) => {
              if (e.currentTarget.hasPointerCapture(e.pointerId)) pickFromClientX(e.clientX);
            }}
            role="slider"
            aria-valuemin={0}
            aria-valuemax={Math.max(1, steps.length - 1)}
            aria-valuenow={stepIndex}
            aria-valuetext={currentStep.name}
            tabIndex={0}
            onKeyDown={(e) => {
              if (steps.length <= 1) return;
              if (e.key === "ArrowRight" || e.key === "ArrowUp") {
                const next = steps[Math.min(steps.length - 1, stepIndex + 1)];
                if (next) selectStep(next);
              }
              if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
                const next = steps[Math.max(0, stepIndex - 1)];
                if (next) selectStep(next);
              }
            }}
          >
            {/* The knob measures the rail, not the padded wrap (see .thinking-rail-stage). */}
            <div className="thinking-rail-stage" ref={trackRef}>
              <div className="thinking-rail-bg">
                {steps.map((step, idx) => (
                  <span
                    key={step.key}
                    className={`rail-step-dot${idx <= stepIndex ? " lit" : ""}`}
                    style={{
                      left: `${steps.length <= 1 ? 50 : (idx / (steps.length - 1)) * 100}%`,
                    }}
                  />
                ))}
                <div className="thinking-rail-filled" style={{ width: `${fillPct}%` }} />
                <div className="thinking-rail-particles" style={{ width: `${fillPct}%` }}>
                  {isUltracode ? (
                    <div className="git-commit-matrix" aria-hidden="true">
                      {Array.from({ length: 28 }).map((_, col) => (
                        <div key={col} className="matrix-col">
                          <span className={`matrix-cell c-${(col * 3) % 5}`} />
                          <span className={`matrix-cell c-${(col * 7 + 2) % 5}`} />
                          <span className={`matrix-cell c-${(col * 2 + 4) % 5}`} />
                        </div>
                      ))}
                    </div>
                  ) : null}
                </div>
              </div>
              <div className="thinking-pill-knob" style={{ left: `${fillPct}%` }} />
            </div>
          </div>

          {/* The scale, under the rail where it belongs: three words, not a legend. */}
          <div className="thinking-scale-row" aria-hidden="true">
            <span>Faster</span>
            <span>Balanced</span>
            <span>Smarter</span>
          </div>
        </div>
      ) : null}
    </div>
  );
}

/* ────────────────────────────────────────────────────────────────────────── */
/* 4. PERMISSION POPOVER (Screenshot 5)                                      */
/* ────────────────────────────────────────────────────────────────────────── */

/**
 * The three postures, as one table.
 *
 * They were three hand-written blocks with the same markup copied out, which is how the
 * menu came to offer "Auto" and "Full access" as different things while the code behind
 * them did exactly the same thing. One row per posture means a change to what a posture
 * *is* has one place to land, and `detail` is the promise the backend keeps
 * (`PermissionPosture::apply_posture` in `bhippi-core`).
 */
export const PERMISSION_MODES: {
  id: PermissionMode;
  label: string;
  /** What picking it actually changes, in the user's terms. Shown under the label,
   *  because a name alone never told anyone how Auto and Full access differ. */
  detail: string;
  color: string;
  icon: (size: number) => React.ReactNode;
}[] = [
  {
    id: "ask_approval",
    label: "Ask approval",
    detail: "Every change waits for a yes",
    color: "#22c55e",
    icon: (size) => <IconHand size={size} />,
  },
  {
    id: "auto",
    label: "Auto",
    detail: "Builds the game without asking",
    color: "#3b82f6",
    icon: (size) => <IconBolt size={size} />,
  },
  {
    id: "full_access",
    label: "Full access",
    detail: "Auto, and may drive the screen",
    color: "#38bdf8",
    icon: (size) => <IconShield size={size} />,
  },
];

/** The short word on the chip itself. The menu has room for "Ask approval"; the composer
 *  bar does not, and the bar is where this is read a hundred times a day. */
const CHIP_LABEL: Record<PermissionMode, string> = {
  ask_approval: "Ask",
  auto: "Auto",
  full_access: "Full",
};

export function PermissionPopover({
  mode,
  computerBrowser,
  open,
  onOpenChange,
  onSelectMode,
  onToggleComputerBrowser,
}: {
  mode: PermissionMode;
  computerBrowser: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelectMode: (mode: PermissionMode) => void;
  onToggleComputerBrowser: () => void;
}) {
  const containerRef = useClickOutside<HTMLDivElement>(open, () => onOpenChange(false));
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const listRef = useRef<HTMLDivElement | null>(null);

  const active = PERMISSION_MODES.find((item) => item.id === mode) ?? PERMISSION_MODES[0];

  // Opening lands focus on the posture already in force, so the keyboard starts from where
  // the user is rather than from the top of the list — and so a screen reader announces the
  // current answer before it reads the alternatives.
  useEffect(() => {
    if (!open) return;
    listRef.current
      ?.querySelector<HTMLButtonElement>('[role="radio"][aria-checked="true"]')
      ?.focus();
  }, [open]);

  // Escape already closes via `useClickOutside`; this puts focus back on the chip, which is
  // the half a dropdown usually forgets.
  const close = () => {
    onOpenChange(false);
    triggerRef.current?.focus();
  };

  // Arrow keys move between postures the way a radio group is supposed to: one stop per
  // option, wrapping, with Home/End for the ends.
  const onListKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const index = PERMISSION_MODES.findIndex((item) => item.id === mode);
    const last = PERMISSION_MODES.length - 1;
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? last
          : event.key === "ArrowDown"
            ? (index + 1) % PERMISSION_MODES.length
            : (index - 1 + PERMISSION_MODES.length) % PERMISSION_MODES.length;
    // Moving is choosing in a radio group. The menu stays open so the next press can move on.
    onSelectMode(PERMISSION_MODES[next].id);
    // Focus travels with the selection, or the ring sits on the row the user just left while
    // the tick moves without it — and the next arrow press then starts from the wrong place.
    // The rows already exist in the DOM, so this does not wait for the re-render.
    listRef.current?.querySelectorAll<HTMLButtonElement>('[role="radio"]')[next]?.focus();
  };

  return (
    <div className="composer-popover-anchor" ref={containerRef}>
      <button
        type="button"
        ref={triggerRef}
        className={`composer-bar-btn permission-trigger${open ? " active" : ""}`}
        style={{ color: active.color }}
        onClick={() => onOpenChange(!open)}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={`Permission: ${active.label}. ${active.detail}`}
        title={`${active.label} — ${active.detail}`}
      >
        {active.icon(13)}
        <span style={{ color: active.color }}>{CHIP_LABEL[active.id]}</span>
        <IconChevronDown size={10} />
      </button>

      {open ? (
        <div
          className="bhippi-popover permission-popover"
          role="dialog"
          aria-label="Permissions"
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.stopPropagation();
              close();
            }
          }}
        >
          <div className="popover-head-simple">PERMISSION</div>

          <div
            className="popover-item-list"
            role="radiogroup"
            aria-label="What this chat may do"
            ref={listRef}
            onKeyDown={onListKeyDown}
          >
            {PERMISSION_MODES.map((item) => {
              const selected = item.id === mode;
              return (
                <button
                  key={item.id}
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  // One tab stop for the whole group, as a radio group has: Tab reaches the
                  // current answer, the arrows move within it.
                  tabIndex={selected ? 0 : -1}
                  className={`popover-row-btn permission-row${selected ? " selected" : ""}`}
                  onClick={() => {
                    onSelectMode(item.id);
                    close();
                  }}
                >
                  <span className="popover-row-left">
                    <span className="permission-row-icon" style={{ color: item.color }}>
                      {item.icon(16)}
                    </span>
                    <span className="permission-row-copy">
                      <span className="popover-row-name">{item.label}</span>
                      <span className="permission-row-detail">{item.detail}</span>
                    </span>
                  </span>
                  {selected ? <IconCheck size={14} /> : null}
                </button>
              );
            })}
          </div>

          <div className="popover-divider" />

          {/* What Full access adds over Auto, said once more as the thing it is. This header
              used to read NEXT ONLY, which promised a grant lasting one turn — nothing ever
              expired it, so the screen stayed reachable until somebody noticed. */}
          <div className="popover-subhead-row" id="permission-reach-label">
            <span>BEYOND THE PROJECT</span>
            <span className="mini-icons" aria-hidden="true">
              <IconHand size={12} />
              <IconBolt size={12} />
            </span>
          </div>

          <button
            type="button"
            role="switch"
            aria-checked={computerBrowser}
            aria-describedby="permission-reach-label"
            className={`popover-row-btn toggle-row${computerBrowser ? " active" : ""}`}
            onClick={onToggleComputerBrowser}
            title={
              computerBrowser
                ? "Bhippi may see and drive the screen. Turning this off steps back to Auto."
                : "Let Bhippi see and drive the screen. This is Full access."
            }
          >
            <span className="popover-row-left">
              <span className="permission-row-icon" style={{ color: "#38bdf8" }}>
                <IconMonitor size={14} />
              </span>
              <span className="popover-row-name muted-text">Computer + Browser included</span>
            </span>
            {computerBrowser ? <IconCheck size={13} /> : null}
          </button>
        </div>
      ) : null}
    </div>
  );
}

/* ────────────────────────────────────────────────────────────────────────── */
/* 5. OPTIONS POPOVER (Screenshot 2)                                         */
/* ────────────────────────────────────────────────────────────────────────── */

export function OptionsPopover({
  open,
  onOpenChange,
  onAttach,
  designOn,
  onToggleDesign,
  focusMode,
  onToggleFocus,
  agentMode,
  onToggleAgentMode,
  predictiveText,
  onTogglePredictiveText,
  indexMapOn,
  onToggleIndexMap,
  caveman,
  onToggleCaveman,
  fontSize,
  onChangeFontSize,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAttach: () => void;
  designOn: boolean;
  onToggleDesign: () => void;
  focusMode: boolean;
  onToggleFocus: () => void;
  agentMode: boolean;
  onToggleAgentMode: () => void;
  predictiveText: boolean;
  onTogglePredictiveText: () => void;
  indexMapOn?: boolean;
  onToggleIndexMap?: () => void;
  caveman?: boolean;
  onToggleCaveman?: () => void;
  fontSize: number;
  onChangeFontSize: (size: number) => void;
}) {
  const containerRef = useClickOutside<HTMLDivElement>(open, () => onOpenChange(false));

  return (
    <div className="composer-popover-anchor" ref={containerRef}>
      <button
        type="button"
        className={`tool-btn options-trigger${open ? " active" : ""}`}
        title="Add and options"
        aria-label="Add and options"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => onOpenChange(!open)}
      >
        <IconPlus size={15} />
      </button>

      {open ? (
        <div className="bhippi-popover options-popover" role="dialog" aria-label="Composer Options">
          {/* Section: ESSENTIALS */}
          <div className="popover-head-simple">ESSENTIALS</div>

          <div className="popover-item-list">
            {/* Attach — the first row, and the one the `+` exists for: it opens the
                native file picker and the chosen files appear as chips above the input. */}
            <button
              type="button"
              className="popover-row-btn"
              onClick={() => {
                onAttach();
                onOpenChange(false);
              }}
            >
              <span className="popover-row-left">
                <span className="popover-row-name bold-label">Attach photos &amp; files</span>
              </span>
              <span className="popover-row-right muted-icon">
                <IconAttach size={14} />
              </span>
            </button>

            {/* Bhippi Design */}
            <button
              type="button"
              className="popover-row-btn"
              onClick={onToggleDesign}
                aria-pressed={Boolean(designOn)}
            >
              <span className="popover-row-left">
                <IconPalette size={14} />
                <span className="popover-row-name bold-label">Bhippi Design</span>
              </span>
              <span className={`popover-switch${designOn ? " on" : ""}`} aria-hidden="true" />
            </button>
          </div>

          <div className="popover-divider" />

          {/* Section: ADVANCED */}
          <div className="popover-head-simple">ADVANCED</div>

          <div className="popover-item-list">
            {/* Focus */}
            <button
              type="button"
              className="popover-row-btn"
              onClick={onToggleFocus}
                aria-pressed={Boolean(focusMode)}
            >
              <span className="popover-row-left">
                <span className="popover-row-name bold-label">Focus</span>
              </span>
              <span className={`popover-switch${focusMode ? " on" : ""}`} aria-hidden="true" />
            </button>

            {/* Agent mode */}
            <button
              type="button"
              className="popover-row-btn"
              onClick={onToggleAgentMode}
                aria-pressed={Boolean(agentMode)}
            >
              <span className="popover-row-left">
                <span className="popover-row-name bold-label">Agent mode</span>
              </span>
              <span className={`popover-switch${agentMode ? " on" : ""}`} aria-hidden="true" />
            </button>

            {/* Predictive text */}
            <button
              type="button"
              className="popover-row-btn"
              onClick={onTogglePredictiveText}
                aria-pressed={Boolean(predictiveText)}
            >
              <span className="popover-row-left">
                <span className="popover-row-name bold-label">Predictive text</span>
              </span>
              <span className={`popover-switch${predictiveText ? " on" : ""}`} aria-hidden="true" />
            </button>

            {/* Caveman */}
            {onToggleCaveman ? (
              <button
                type="button"
                className="popover-row-btn"
                onClick={onToggleCaveman}
                aria-pressed={Boolean(caveman)}
                title="Caveman mode: telegraphic, high-density responses. Slashes token usage & cost by up to 70%."
              >
                <span className="popover-row-left">
                  <span className="popover-row-name bold-label">Caveman</span>
                </span>
                <span className={`popover-switch${caveman ? " on" : ""}`} aria-hidden="true" />
              </button>
            ) : null}

            {/* IndexMap */}
            {onToggleIndexMap ? (
              <button
                type="button"
                className="popover-row-btn"
                onClick={onToggleIndexMap}
                aria-pressed={Boolean(indexMapOn)}
              >
                <span className="popover-row-left">
                  <span className="popover-row-name bold-label">IndexMap</span>
                </span>
                <span className={`popover-switch${indexMapOn ? " on" : ""}`} aria-hidden="true" />
              </button>
            ) : null}

            {/* Text size with - 15 + */}
            <div className="popover-row-btn non-clickable">
              <span className="popover-row-left">
                <span className="popover-row-name bold-label">Text size</span>
              </span>
              <div className="text-size-stepper">
                <button
                  type="button"
                  className="stepper-btn"
                  onClick={(event) => {
                    event.stopPropagation();
                    onChangeFontSize(Math.max(11, fontSize - 1));
                  }}
                  title="Decrease text size"
                  aria-label="Decrease text size"
                >
                  −
                </button>
                <span className="stepper-val">{fontSize}</span>
                <button
                  type="button"
                  className="stepper-btn"
                  onClick={(event) => {
                    event.stopPropagation();
                    onChangeFontSize(Math.min(22, fontSize + 1));
                  }}
                  title="Increase text size"
                  aria-label="Increase text size"
                >
                  +
                </button>
              </div>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
