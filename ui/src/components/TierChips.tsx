import { useEffect, useState } from "react";
import { api } from "../lib/api";
import {
  TIER_LABELS,
  TIER_NAMES,
  matchTier,
  tierUsability,
  type TierName,
  type TierPreset,
  type Tiers,
  type UsableProvider,
} from "../lib/tiers";

export type { TierName } from "../lib/tiers";

/**
 * The Quick / Balanced / Max chips (GAD-017).
 *
 * Presets over the pickers that are already there: choosing one writes provider, model and
 * effort into the composer's own state, so what answers the next turn is visible in the
 * pickers rather than hidden behind the chip. A tier whose backend is not usable renders
 * disabled with the reason — never swapped for something else.
 */
export function TierChips({
  chatOptions,
  match,
  active,
  onSelect,
  compact = false,
}: {
  /** `AppStatus.chat_options`: enabled *and* reachable. */
  chatOptions: readonly UsableProvider[];
  /** The composer's current pickers; the matching chip highlights itself. */
  match?: { provider: string | null; model: string | null; effort: string };
  /** Explicit highlight for surfaces with no pickers (the launcher). */
  active?: TierName | null;
  onSelect: (name: TierName, preset: TierPreset) => void;
  compact?: boolean;
}) {
  const [tiers, setTiers] = useState<Tiers | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let stale = false;
    api
      .tiers()
      .then((rows) => {
        if (!stale) {
          setTiers(rows);
          setError(null);
        }
      })
      .catch((loadError: unknown) => {
        if (!stale) {
          setTiers(null);
          setError(String((loadError as Error).message ?? loadError));
        }
      });
    return () => {
      stale = true;
    };
  }, []);

  const matched = match ? matchTier(tiers, match) : (active ?? null);

  return (
    <div
      className={`tier-chips${compact ? " compact" : ""}`}
      role="group"
      aria-label="Answer tier"
      aria-busy={tiers === null && error === null}
    >
      {TIER_NAMES.map((name) => {
        const preset = tiers?.[name];
        const state = tiers === null
          ? {
              usable: false,
              reason: error ? `Tiers could not load: ${error}` : "Loading tiers…",
            }
          : tierUsability(preset, chatOptions);
        return (
          <button
            key={name}
            type="button"
            className={`tier-chip${matched === name ? " active" : ""}`}
            disabled={!state.usable}
            aria-pressed={matched === name}
            title={
              state.usable
                ? `${TIER_LABELS[name]} · ${preset?.provider ?? ""}${
                    preset?.model ? ` · ${preset.model}` : ""
                  } · ${preset?.effort ?? ""}`
                : state.reason
            }
            onClick={() => {
              if (preset) onSelect(name, preset);
            }}
          >
            {TIER_LABELS[name]}
          </button>
        );
      })}
      {error ? (
        <span className="tier-chips-error" role="alert">
          Tiers unavailable
        </span>
      ) : null}
    </div>
  );
}
