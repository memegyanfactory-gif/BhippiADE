/**
 * Quick / Balanced / Max: presets over the composer's own pickers (GAD-017).
 *
 * The rule this module exists to hold: **a tier is never a silent swap.** When a tier names
 * a backend that is not usable right now, the chip is disabled and says why — it does not
 * quietly answer with a different provider, and it does not disappear, because a chip that
 * vanishes teaches nothing about how to get it back.
 */

export type TierName = "quick" | "balanced" | "max";

export const TIER_NAMES: readonly TierName[] = ["quick", "balanced", "max"] as const;

export const TIER_LABELS: Record<TierName, string> = {
  quick: "Quick",
  balanced: "Balanced",
  max: "Max",
};

export type TierPreset = {
  provider: string;
  model: string | null;
  effort: string;
};

export type Tiers = Record<TierName, TierPreset>;

/** Only the two fields this module reads from a provider row. */
export type UsableProvider = { id: string; label: string };

export type TierUsability =
  | { usable: true; reason: null }
  | { usable: false; reason: string };

/**
 * Whether a tier can answer right now, and — when it cannot — the sentence the tooltip
 * shows. `chatOptions` is `AppStatus.chat_options`: enabled *and* reachable.
 */
export function tierUsability(
  preset: TierPreset | undefined,
  chatOptions: readonly UsableProvider[],
): TierUsability {
  if (!preset || preset.provider.trim().length === 0) {
    return { usable: false, reason: "This tier has no provider yet — set one in Settings › Providers." };
  }
  const match = chatOptions.find(
    (option) => option.id.toLowerCase() === preset.provider.toLowerCase(),
  );
  if (!match) {
    return {
      usable: false,
      reason: `${preset.provider} is not available right now — enable or install it in Settings › Providers.`,
    };
  }
  return { usable: true, reason: null };
}

/**
 * Which tier the composer's current pickers correspond to, or `null` for a combination
 * the user assembled by hand. A tier whose model is `null` matches any model, because the
 * preset deliberately left the provider on its own default.
 */
export function matchTier(
  tiers: Tiers | null,
  current: { provider: string | null; model: string | null; effort: string },
): TierName | null {
  if (!tiers || !current.provider) return null;
  for (const name of TIER_NAMES) {
    const preset = tiers[name];
    if (!preset) continue;
    if (preset.provider.toLowerCase() !== current.provider.toLowerCase()) continue;
    if (preset.effort !== current.effort) continue;
    if (preset.model !== null && preset.model !== current.model) continue;
    return name;
  }
  return null;
}
