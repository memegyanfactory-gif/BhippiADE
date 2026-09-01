import { useCallback, useEffect, useRef, useState } from "react";
import type { ProviderInfo } from "../lib/ipc";
import {
  IconCheck,
  IconGift,
  IconSearch,
  IconSparkle,
  IconStar,
  IconStarFilled,
  IconVision,
} from "./icons";
import { ProviderLogo } from "./ProviderLogo";

export type Effort = "fast" | "balanced" | "quality" | "ultra";

export const EFFORT_LEVELS: { id: Effort; name: string; blurb: string }[] = [
  { id: "fast", name: "Fast", blurb: "shortest useful answer" },
  { id: "balanced", name: "Balanced", blurb: "the everyday default" },
  { id: "quality", name: "Quality", blurb: "trade-offs and caveats" },
  { id: "ultra", name: "Ultra", blurb: "deepest, most thorough" },
];

export function isFreeModel(name: string): boolean {
  const lower = name.toLowerCase();
  return lower.includes(":free") || lower.includes("(free)") || lower.endsWith("-free") || lower === "free";
}

export function isFreeQuery(query: string): boolean {
  return query === "free" || query === "/free" || query === ":free" || query === "free:" || query.startsWith("/free ");
}

export function isVisionModel(model: string | null, providerId: string | null): boolean {
  if (!model) {
    if (providerId === "claude" || providerId === "codex" || providerId === "openai") return true;
    return false;
  }
  const m = model.toLowerCase();
  return (
    m.includes("vision") ||
    m.includes("claude-3") ||
    m.includes("claude-4") ||
    m.includes("gpt-4o") ||
    m.includes("gpt-4.5") ||
    m.includes("gemini-1.5") ||
    m.includes("gemini-2") ||
    m.includes("sonnet") ||
    m.includes("opus") ||
    m.includes("pixtral") ||
    m.includes("qwen-vl") ||
    m.includes("llava")
  );
}

export function shortModel(name: string): string {
  if (name.length <= 18) return name;
  return `${name.slice(0, 8)}…${name.slice(-8)}`;
}

const FAV_KEY = "bhippi_fav_models";

function loadFavMap(): Record<string, string[]> {
  try {
    const raw = localStorage.getItem(FAV_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, string[]>;
    if (parsed && typeof parsed === "object") return parsed;
  } catch {}
  return {};
}

function saveFavMap(map: Record<string, string[]>) {
  try {
    localStorage.setItem(FAV_KEY, JSON.stringify(map));
  } catch {}
}

export function UnifiedModelPicker({
  options,
  currentOption,
  currentModel,
  effort,
  designOn,
  hasVision,
  open,
  disabled,
  onOpenChange,
  onSelectProvider,
  onSelectModel,
  onSelectEffort,
  onToggleDesign,
}: {
  options: ProviderInfo[];
  currentOption: ProviderInfo | null;
  currentModel: string | null;
  effort: Effort;
  designOn: boolean;
  hasVision: boolean;
  open: boolean;
  disabled: boolean;
  onOpenChange: (open: boolean) => void;
  onSelectProvider: (id: string) => void;
  onSelectModel: (model: string | null) => void;
  onSelectEffort: (effort: Effort) => void;
  onToggleDesign: () => void;
}) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const [draft, setDraft] = useState("");
  const [favMap, setFavMap] = useState<Record<string, string[]>>(() => loadFavMap());

  const providerId = currentOption?.id ?? null;

  useEffect(() => {
    setDraft("");
  }, [providerId, open]);

  useEffect(() => {
    if (!open) return undefined;
    const onPointer = (event: MouseEvent) => {
      if (!wrapRef.current?.contains(event.target as Node)) {
        onOpenChange(false);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onOpenChange(false);
      }
    };
    window.addEventListener("mousedown", onPointer);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onPointer);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, onOpenChange]);

  const toggleFav = useCallback(
    (model: string, e?: React.MouseEvent) => {
      if (e) {
        e.preventDefault();
        e.stopPropagation();
      }
      if (!providerId) return;
      const list = favMap[providerId] ?? [];
      const exists = list.includes(model);
      const nextList = exists ? list.filter((m) => m !== model) : [...list, model];
      const nextMap = { ...favMap, [providerId]: nextList };
      if (nextList.length === 0) delete nextMap[providerId];
      setFavMap(nextMap);
      saveFavMap(nextMap);
    },
    [favMap, providerId],
  );

  const isFav = useCallback(
    (model: string) => {
      if (!providerId) return false;
      return (favMap[providerId] ?? []).includes(model);
    },
    [favMap, providerId],
  );

  const activeEffort = EFFORT_LEVELS.find((l) => l.id === effort) ?? EFFORT_LEVELS[1];

  const providerLabel = currentOption?.label ?? "Provider";
  const modelLabel = currentModel ?? "Default";

  // Model filtering
  const models = currentOption?.models ?? [];
  const rawQuery = draft.trim().toLowerCase();
  const freeFilter = isFreeQuery(rawQuery);
  const freeCount = models.filter(isFreeModel).length;

  let matches: string[];
  if (freeFilter) {
    matches = models.filter(isFreeModel);
    const tail = rawQuery.replace(/^\/free\s*/, "").trim();
    if (tail && tail !== "free" && tail !== "/free" && tail !== ":free") {
      matches = matches.filter((m) => m.toLowerCase().includes(tail));
    }
  } else if (rawQuery) {
    matches = models.filter((model) => model.toLowerCase().includes(rawQuery));
  } else {
    matches = models;
  }

  const favSet = new Set(providerId ? (favMap[providerId] ?? []) : []);
  const favMatches = matches.filter((m) => favSet.has(m));
  const restMatches = matches.filter((m) => !favSet.has(m));
  const orderedMatches = [...favMatches, ...restMatches];

  const typedIsNew =
    currentOption?.accepts_custom_model &&
    draft.trim().length > 0 &&
    !freeFilter &&
    !models.some((model) => model.toLowerCase() === rawQuery);

  const choose = (model: string | null) => {
    onSelectModel(model);
    setDraft("");
    onOpenChange(false);
  };

  const submitTyped = () => {
    const name = draft.trim();
    if (name) choose(name);
  };

  return (
    <div className="unified-model-picker" ref={wrapRef}>
      {/* ── Unified Button in Composer Bar ──────────────────────────── */}
      <button
        type="button"
        className={`model-btn unified-trigger${open ? " active" : ""}`}
        onClick={() => onOpenChange(!open)}
        disabled={disabled}
        aria-haspopup="dialog"
        aria-expanded={open}
        title={`${providerLabel} · ${modelLabel} (${activeEffort.name} Speed)`}
      >
        <ProviderLogo id={currentOption?.id ?? "demo"} size={15} />
        <span className="unified-provider-name">{providerLabel}</span>
        <span className="unified-sep">/</span>
        <span className="unified-model-name">{shortModel(modelLabel)}</span>

        <span className={`unified-speed-chip tier-${effort}`}>
          {activeEffort.name}
        </span>

        {hasVision && (
          <span className="unified-vision-indicator" title="Vision Multimodal Ready">
            <IconVision size={11} />
          </span>
        )}

        <span className="chev" aria-hidden="true">▾</span>
      </button>

      {/* ── Consolidated Dropup Popover ──────────────────────────────── */}
      {open && (
        <div className="dropup unified-model-popover" role="dialog" aria-label="Model & Speed Settings">
          {/* 1. Provider Tabs Header */}
          {options.length > 1 && (
            <div className="unified-provider-tabs">
              {options.map((opt) => (
                <button
                  key={opt.id}
                  type="button"
                  className={`provider-tab${opt.id === currentOption?.id ? " active" : ""}`}
                  onClick={() => onSelectProvider(opt.id)}
                >
                  <ProviderLogo id={opt.id} size={14} />
                  <span>{opt.label}</span>
                </button>
              ))}
            </div>
          )}

          {/* 2. Speed / Thinking Effort Selector */}
          <div className="unified-effort-section">
            <div className="unified-section-label">
              <span>Reasoning & Effort</span>
              <span className="unified-active-effort">{activeEffort.name}</span>
            </div>
            <div className="unified-effort-pills">
              {EFFORT_LEVELS.map((lvl) => (
                <button
                  key={lvl.id}
                  type="button"
                  className={`effort-pill tier-${lvl.id}${lvl.id === effort ? " active" : ""}`}
                  onClick={() => onSelectEffort(lvl.id)}
                  title={lvl.blurb}
                >
                  <span className="pill-dot" />
                  <span className="pill-name">{lvl.name}</span>
                </button>
              ))}
            </div>

            {/* Design System Switch */}
            <button
              type="button"
              role="switch"
              aria-checked={designOn}
              className={`design-switch compact${designOn ? " on" : ""}`}
              onClick={onToggleDesign}
            >
              <span className="design-switch-copy">
                <span className="design-switch-title">
                  <IconSparkle size={11} />
                  Bhippi Design System
                </span>
              </span>
              <span className="design-switch-track" aria-hidden="true">
                <span className="design-switch-thumb" />
              </span>
            </button>
          </div>

          <div className="unified-popover-divider" />

          {/* 3. Model Search & Filter */}
          <div className="model-custom">
            <span className="model-custom-icon" aria-hidden="true">
              <IconSearch size={13} />
            </span>
            <input
              value={draft}
              placeholder={
                freeCount > 0
                  ? `Search models — type /free for ${freeCount} free…`
                  : `Search models or enter model id…`
              }
              autoFocus
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  if (orderedMatches.length === 1) choose(orderedMatches[0]);
                  else if (typedIsNew) submitTyped();
                }
              }}
            />
            {typedIsNew && (
              <button onClick={submitTyped} title={`Use "${draft.trim()}"`}>
                Use
              </button>
            )}
          </div>

          {/* 4. Model List */}
          <ul className="unified-model-list" role="listbox" aria-label="Models">
            {!rawQuery && !freeFilter && (
              <li>
                <button
                  role="option"
                  aria-selected={currentModel === null}
                  className={`dropup-item${currentModel === null ? " selected" : ""}`}
                  onClick={() => choose(null)}
                >
                  <span className="dropup-name">Default</span>
                  <span className="dropup-model">vendor default</span>
                  {currentModel === null ? <IconCheck size={12} /> : null}
                </button>
              </li>
            )}

            {orderedMatches.map((model) => {
              const modelVision = isVisionModel(model, providerId);
              const free = isFreeModel(model);
              const fav = isFav(model);
              const selected = model === currentModel;
              return (
                <li key={model} className={fav ? "fav-row" : undefined}>
                  <button
                    role="option"
                    aria-selected={selected}
                    className={`dropup-item${selected ? " selected" : ""}${fav ? " fav" : ""}`}
                    onClick={() => choose(model)}
                  >
                    <span className="dropup-name" title={model}>
                      {model}
                    </span>
                    <span className="model-badges">
                      {free && (
                        <span className="model-free-badge" title="Free — no cost">
                          <IconGift size={9} />
                          FREE
                        </span>
                      )}
                      {modelVision && (
                        <span className="model-vision-badge" title="Vision supported">
                          <IconVision size={10} />
                          <span>Vision</span>
                        </span>
                      )}
                    </span>
                    <span className="model-row-actions">
                      <span
                        role="button"
                        tabIndex={0}
                        className={`fav-btn${fav ? " active" : ""}`}
                        onClick={(e) => toggleFav(model, e)}
                        title={fav ? "Unfavourite" : "Favourite"}
                      >
                        {fav ? <IconStarFilled size={12} /> : <IconStar size={12} />}
                      </span>
                      {selected && <IconCheck size={12} />}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>

          {/* 5. Vision Status Footer */}
          <div className="unified-popover-footer">
            <span className={`vision-foot-pill ${hasVision ? "ready" : "none"}`}>
              <IconVision size={11} />
              <span>{hasVision ? "Vision Ready" : "Text Only (No Vision)"}</span>
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
