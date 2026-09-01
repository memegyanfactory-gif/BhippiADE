import React, { useEffect, useRef, useState } from "react";
import type { ProviderInfo } from "../lib/ipc";
import {
  IconAttach,
  IconBolt,
  IconCheck,
  IconChevronDown,
  IconChevronRight,
  IconHand,
  IconPalette,
  IconSearch,
  IconShield,
  IconSliders,
  IconStar,
  IconStarFilled,
} from "./icons";
import { ProviderLogo } from "./ProviderLogo";

export type Effort = "fast" | "balanced" | "quality" | "ultra";
export type PermissionMode = "ask_approval" | "auto" | "full_access";

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
  const active = providers.find((p) => p.id === currentId) ?? providers[0] ?? null;

  return (
    <div className="composer-popover-anchor" ref={containerRef}>
      <button
        type="button"
        className={`composer-bar-btn provider-trigger${open ? " active" : ""}`}
        onClick={() => onOpenChange(!open)}
        aria-label={`Provider: ${active?.label ?? "Select provider"}`}
        aria-expanded={open}
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
              const isSelected = active?.id === item.id || (active?.label.toLowerCase() === item.label.toLowerCase());
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

// Preset Claude models with intelligence dots matching Screenshot 2
const CLAUDE_PRESETS = [
  { id: "Fable 5 (1M)", dots: [true, true, true, true, true], half: false },
  { id: "Opus 5 (1M)", dots: [true, true, true, true], half: true },
  { id: "Sonnet 5", dots: [true, true, true, false, false], half: false },
  { id: "Sonnet 5 (1M)", dots: [true, true, true], half: true },
  { id: "Haiku 4.5", dots: [true, true, false, false, false], half: false },
];

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
  let baseList: { id: string; isFree?: boolean; backend?: string; dots?: boolean[]; half?: boolean }[] = [];

  if (isClaude) {
    baseList = [...CLAUDE_PRESETS];
    // merge dynamically discovered models if any
    for (const m of provider.models) {
      if (!baseList.some((b) => b.id.toLowerCase() === m.toLowerCase())) {
        baseList.push({ id: m, dots: [true, true, true, false, false], half: false });
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
    ? baseList.filter((m) => m.id.toLowerCase().includes(searchQuery.toLowerCase()))
    : baseList;

  return (
    <div className="composer-popover-anchor" ref={containerRef}>
      <button
        type="button"
        className={`composer-bar-btn model-trigger${open ? " active" : ""}`}
        onClick={() => onOpenChange(!open)}
        aria-label={`Model: ${activeLabel}`}
        aria-expanded={open}
      >
        <span className="model-trigger-text">{activeLabel}</span>
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

          {/* Model Item List */}
          <div className="popover-item-list">
            {filteredList.map((item) => {
              const isSelected = activeLabel.toLowerCase() === item.id.toLowerCase();
              const fav = isFav(item.id);

              return (
                <button
                  key={item.id}
                  type="button"
                  className={`popover-row-btn model-row${isSelected ? " selected" : ""}`}
                  onClick={() => {
                    onSelect(item.id);
                    onOpenChange(false);
                  }}
                >
                  <span className="popover-row-left">
                    {/* Prefix: chevron arrow > or favorite star */}
                    {isOpenCode ? (
                      <span
                        className={`model-fav-star${fav ? " active" : ""}`}
                        onClick={(e) => toggleFav(item.id, e)}
                        title={fav ? "Remove favorite" : "Favorite"}
                      >
                        {fav ? <IconStarFilled size={13} /> : <IconStar size={13} />}
                      </span>
                    ) : (
                      <span className="model-row-prefix">&gt;</span>
                    )}

                    {/* Claude Intelligence Dot Meter (Screenshot 2) */}
                    {isClaude && item.dots ? (
                      <span className="model-dot-meter" aria-hidden="true">
                        {item.dots.map((activeDot, dotIdx) => (
                          <span
                            key={dotIdx}
                            className={`meter-dot${activeDot ? " filled" : ""}`}
                          />
                        ))}
                        {item.half ? <span className="meter-dot half" /> : null}
                      </span>
                    ) : null}

                    <span className="popover-row-name model-id-text" title={item.id}>
                      {item.id}
                    </span>
                  </span>

                  <span className="popover-row-right">
                    {/* Free / Paid Pill & Backend Badge (Screenshot 4) */}
                    {isOpenCode ? (
                      <>
                        {item.isFree ? (
                          <span className="model-badge-free">Free</span>
                        ) : (
                          <span className="model-badge-paid">Paid</span>
                        )}
                        {item.backend ? (
                          <span className="model-source-text">{item.backend}</span>
                        ) : null}
                      </>
                    ) : null}

                    {/* Active Checkmark */}
                    {isSelected ? <IconCheck size={14} /> : null}
                  </span>
                </button>
              );
            })}
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

const EFFORT_STEPS: { id: Effort; label: string; name: string }[] = [
  { id: "fast", label: "Fast", name: "Fast" },
  { id: "balanced", label: "Balanced", name: "Balanced" },
  { id: "quality", label: "Quality", name: "Quality" },
  { id: "ultra", label: "Ultra", name: "Ultra" },
];

export function ThinkingPopover({
  effort,
  open,
  onOpenChange,
  onSelect,
}: {
  effort: Effort;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (effort: Effort) => void;
}) {
  const containerRef = useClickOutside<HTMLDivElement>(open, () => onOpenChange(false));
  const trackRef = useRef<HTMLDivElement | null>(null);
  const stepIndex = Math.max(0, EFFORT_STEPS.findIndex((s) => s.id === effort));
  const currentStep = EFFORT_STEPS[stepIndex] ?? EFFORT_STEPS[1];
  const fillPct = (stepIndex / Math.max(1, EFFORT_STEPS.length - 1)) * 100;

  const pickFromClientX = (clientX: number) => {
    const rect = trackRef.current?.getBoundingClientRect();
    if (!rect || rect.width <= 0) return;
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    const targetIdx = Math.round(ratio * (EFFORT_STEPS.length - 1));
    const next = EFFORT_STEPS[targetIdx];
    if (next && next.id !== effort) onSelect(next.id);
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
        className={`composer-bar-btn thinking-trigger${open ? " active" : ""}`}
        onClick={() => onOpenChange(!open)}
        aria-label={`Effort: ${currentStep.name}`}
        aria-expanded={open}
      >
        <span>{currentStep.name}</span>
        <IconChevronDown size={10} />
      </button>

      {open ? (
        <div
          className={`bhippi-popover thinking-popover tier-${effort}`}
          role="dialog"
          aria-label="Effort slider"
        >
          <div className="thinking-head-row">
            <div className="thinking-title-area">
              <span className="thinking-label">Effort</span>
              <strong className="thinking-val">{currentStep.name}</strong>
            </div>
            <span className="thinking-help" title="Faster uses fewer reasoning tokens. Smarter thinks longer.">
              ?
            </span>
          </div>

          <div className="thinking-ends-row">
            <span>Faster</span>
            <span>Smarter</span>
          </div>

          <div
            ref={trackRef}
            className="thinking-track-wrap"
            onPointerDown={onPointerDown}
            onPointerMove={(e) => {
              if (e.currentTarget.hasPointerCapture(e.pointerId)) pickFromClientX(e.clientX);
            }}
            role="slider"
            aria-valuemin={0}
            aria-valuemax={EFFORT_STEPS.length - 1}
            aria-valuenow={stepIndex}
            aria-valuetext={currentStep.name}
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === "ArrowRight" || e.key === "ArrowUp") {
                const next = EFFORT_STEPS[Math.min(EFFORT_STEPS.length - 1, stepIndex + 1)];
                if (next) onSelect(next.id);
              }
              if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
                const next = EFFORT_STEPS[Math.max(0, stepIndex - 1)];
                if (next) onSelect(next.id);
              }
            }}
          >
            <div className="thinking-rail-bg">
              {EFFORT_STEPS.map((step, idx) => (
                <span
                  key={step.id}
                  className={`rail-step-dot${idx <= stepIndex ? " lit" : ""}`}
                  style={{ left: `${(idx / (EFFORT_STEPS.length - 1)) * 100}%` }}
                />
              ))}
              <div className="thinking-rail-filled" style={{ width: `${fillPct}%` }} />
              <div className="thinking-rail-particles" style={{ width: `${fillPct}%` }} />
            </div>
            <div className="thinking-pill-knob" style={{ left: `${fillPct}%` }} />
          </div>
        </div>
      ) : null}
    </div>
  );
}

/* ────────────────────────────────────────────────────────────────────────── */
/* 4. PERMISSION POPOVER (Screenshot 5)                                      */
/* ────────────────────────────────────────────────────────────────────────── */

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

  const modeBadge =
    mode === "ask_approval"
      ? { label: "Ask", color: "#22c55e", icon: <IconHand size={13} /> }
      : mode === "auto"
        ? { label: "Auto", color: "#3b82f6", icon: <IconBolt size={13} /> }
        : { label: "Full", color: "#38bdf8", icon: <IconShield size={13} /> };

  return (
    <div className="composer-popover-anchor" ref={containerRef}>
      <button
        type="button"
        className={`composer-bar-btn permission-trigger${open ? " active" : ""}`}
        style={{ color: modeBadge.color }}
        onClick={() => onOpenChange(!open)}
        aria-label={`Permission: ${modeBadge.label}`}
        aria-expanded={open}
      >
        {modeBadge.icon}
        <span style={{ color: modeBadge.color }}>{modeBadge.label}</span>
        <IconChevronDown size={10} />
      </button>

      {open ? (
        <div className="bhippi-popover permission-popover" role="dialog" aria-label="Permissions">
          <div className="popover-head-simple">PERMISSION</div>

          <div className="popover-item-list">
            {/* Ask approval */}
            <button
              type="button"
              className={`popover-row-btn${mode === "ask_approval" ? " selected" : ""}`}
              onClick={() => {
                onSelectMode("ask_approval");
                onOpenChange(false);
              }}
            >
              <span className="popover-row-left">
                <span className="permission-row-icon" style={{ color: "#22c55e" }}>
                  <IconHand size={16} />
                </span>
                <span className="popover-row-name">Ask approval</span>
              </span>
              {mode === "ask_approval" ? <IconCheck size={14} /> : null}
            </button>

            {/* Auto */}
            <button
              type="button"
              className={`popover-row-btn${mode === "auto" ? " selected" : ""}`}
              onClick={() => {
                onSelectMode("auto");
                onOpenChange(false);
              }}
            >
              <span className="popover-row-left">
                <span className="permission-row-icon" style={{ color: "#3b82f6" }}>
                  <IconBolt size={16} />
                </span>
                <span className="popover-row-name">Auto</span>
              </span>
              {mode === "auto" ? <IconCheck size={14} /> : null}
            </button>

            {/* Full access */}
            <button
              type="button"
              className={`popover-row-btn${mode === "full_access" ? " selected" : ""}`}
              onClick={() => {
                onSelectMode("full_access");
                onOpenChange(false);
              }}
            >
              <span className="popover-row-left">
                <span className="permission-row-icon" style={{ color: "#38bdf8" }}>
                  <IconShield size={16} />
                </span>
                <span className="popover-row-name">Full access</span>
              </span>
              {mode === "full_access" ? <IconCheck size={14} /> : null}
            </button>
          </div>

          <div className="popover-divider" />

          {/* NEXT ONLY Header with mini icons */}
          <div className="popover-subhead-row">
            <span>NEXT ONLY</span>
            <span className="mini-icons">
              <IconHand size={12} />
              <IconBolt size={12} />
            </span>
          </div>

          {/* Computer + Browser included Toggle */}
          <button
            type="button"
            className={`popover-row-btn toggle-row${computerBrowser ? " active" : ""}`}
            onClick={onToggleComputerBrowser}
          >
            <span className="popover-row-left">
              <span className="permission-row-icon" style={{ color: "#38bdf8" }}>
                <IconShield size={14} />
              </span>
              <span className="popover-row-name muted-text">
                Computer + Browser included
              </span>
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
        className={`tool-btn${open ? " active" : ""}`}
        title="Tools & options"
        aria-label="Tools and options"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => onOpenChange(!open)}
      >
        <IconSliders size={14} />
      </button>

      {open ? (
        <div className="bhippi-popover options-popover" role="dialog" aria-label="Composer Options">
          {/* Section: ESSENTIALS */}
          <div className="popover-head-simple">ESSENTIALS</div>

          <div className="popover-item-list">
            {/* Attach */}
            <button
              type="button"
              className="popover-row-btn"
              onClick={() => {
                onAttach();
                onOpenChange(false);
              }}
            >
              <span className="popover-row-left">
                <span className="popover-row-name bold-label">Attach</span>
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
            >
              <span className="popover-row-left">
                <IconPalette size={14} />
                <span className="popover-row-name bold-label">Bhippi Design</span>
              </span>
              <span className={`popover-row-right toggle-text${designOn ? " on" : " off"}`}>
                {designOn ? "On" : "Off"}
              </span>
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
            >
              <span className="popover-row-left">
                <span className="popover-row-name bold-label">Focus</span>
              </span>
              <span className={`popover-row-right toggle-text${focusMode ? " on" : " off"}`}>
                {focusMode ? "On" : "Off"}
              </span>
            </button>

            {/* Agent mode */}
            <button
              type="button"
              className="popover-row-btn"
              onClick={onToggleAgentMode}
            >
              <span className="popover-row-left">
                <span className="popover-row-name bold-label">Agent mode</span>
              </span>
              <span className={`popover-row-right toggle-text${agentMode ? " on" : " off"}`}>
                {agentMode ? "On" : "Off"}
              </span>
            </button>

            {/* Predictive text */}
            <button
              type="button"
              className="popover-row-btn"
              onClick={onTogglePredictiveText}
            >
              <span className="popover-row-left">
                <span className="popover-row-name bold-label">Predictive text</span>
              </span>
              <span className={`popover-row-right toggle-text${predictiveText ? " on" : " off"}`}>
                {predictiveText ? "On" : "Off"}
              </span>
            </button>

            {/* Caveman */}
            {onToggleCaveman ? (
              <button
                type="button"
                className="popover-row-btn"
                onClick={onToggleCaveman}
                title="Caveman mode: telegraphic, high-density responses. Slashes token usage & cost by up to 70%."
              >
                <span className="popover-row-left">
                  <span className="popover-row-name bold-label">Caveman</span>
                </span>
                <span className={`popover-row-right toggle-text${caveman ? " on" : " off"}`}>
                  {caveman ? "On" : "Off"}
                </span>
              </button>
            ) : null}

            {/* IndexMap */}
            {onToggleIndexMap ? (
              <button
                type="button"
                className="popover-row-btn"
                onClick={onToggleIndexMap}
              >
                <span className="popover-row-left">
                  <span className="popover-row-name bold-label">IndexMap</span>
                </span>
                <span className={`popover-row-right toggle-text${indexMapOn ? " on" : " off"}`}>
                  {indexMapOn ? "On" : "Off"}
                </span>
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
