/** Antigravity (`agy`) encodes speed in the model slug (`gemini-3.8-flash-high`).
 *  The picker shows one family name; the composer's speed control picks the slug. */

const SPEED_SUFFIX = /-(low|medium|high)$/i;

export type AntigravitySpeed = "low" | "medium" | "high";

export type AntigravityFamily = {
  id: string;
  label: string;
};

export function isAntigravityProvider(providerId: string | null | undefined): boolean {
  const id = (providerId ?? "").trim().toLowerCase();
  return id === "agy" || id.includes("antigravity");
}

/** `gemini-3.8-flash-high` → `gemini-3.8-flash`. Normalizes display names or slugs. */
export function antigravityFamilyId(slug: string): string {
  let s = slug.trim().toLowerCase();
  // Strip trailing notes: e.g. " (high)", " (thinking)", " (medium)", " (low)"
  s = s.replace(/\s*\((low|medium|high|thinking)\)$/i, "");
  // Strip speed suffixes: "-high", "-medium", "-low", "-thinking"
  s = s.replace(SPEED_SUFFIX, "");
  s = s.replace(/-thinking$/i, "");
  // Normalize whitespace or underscores to hyphens
  s = s.replace(/[\s_]+/g, "-");
  // For Claude versions: e.g. "claude-sonnet-4.6" -> "claude-sonnet-4-6"
  if (s.includes("claude")) {
    s = s.replace(/(\d+)\.(\d+)/g, "$1-$2");
  }
  return s || slug.trim();
}

export function antigravitySpeedOf(slug: string): AntigravitySpeed | null {
  const match = slug.trim().match(SPEED_SUFFIX);
  if (!match) return null;
  return match[1].toLowerCase() as AntigravitySpeed;
}

/** Composer speed → the three bands Antigravity actually ships. */
export function antigravitySpeedForEffort(effort: string | null | undefined): AntigravitySpeed {
  const key = (effort ?? "").trim().toLowerCase();
  if (key === "fast" || key === "low" || key === "minimal") return "low";
  if (key === "medium") return "medium";
  return "high";
}

/**
 * `gemini-3.8-flash-high` → `Gemini 3.8 Flash`
 * `claude-sonnet-4-6` → `Claude Sonnet 4.6`
 */
export function antigravityDisplayName(slug: string): string {
  let family = antigravityFamilyId(slug);
  family = family.replace(/-thinking$/i, "");
  family = family.replace(/(\d+)-(\d+)(?=-|$)/g, "$1.$2");
  const words = family
    .split(/[-_]/)
    .filter(Boolean)
    .map(prettyWord)
    .join(" ")
    .replace(/\bGPT OSS\b/i, "GPT-OSS");
  return words || slug.trim();
}

function prettyWord(part: string): string {
  const lower = part.toLowerCase();
  const named: Record<string, string> = {
    gemini: "Gemini",
    claude: "Claude",
    gpt: "GPT",
    oss: "OSS",
    flash: "Flash",
    pro: "Pro",
    sonnet: "Sonnet",
    opus: "Opus",
    haiku: "Haiku",
  };
  if (named[lower]) return named[lower];
  const numbered = part.match(/^(\d+(?:\.\d+)?)([a-z]+)$/i);
  if (numbered) return numbered[1] + numbered[2].toUpperCase();
  if (/^\d/.test(part)) return part;
  return part.charAt(0).toUpperCase() + part.slice(1);
}

/** Unique families in catalogue order, High/Medium/Low collapsed. */
export function collapseAntigravityModels(slugs: readonly string[]): AntigravityFamily[] {
  const seen = new Set<string>();
  const out: AntigravityFamily[] = [];
  for (const slug of slugs) {
    const id = antigravityFamilyId(slug);
    if (!id || seen.has(id)) continue;
    seen.add(id);
    out.push({ id, label: antigravityDisplayName(id) });
  }
  return out;
}

/**
 * Returns supported reasoning speed bands for an Antigravity model:
 * e.g. Flash -> ["low", "medium", "high"], Pro -> ["low", "high"], Claude -> [].
 */
export function getSupportedSpeedsForAntigravityModel(
  model: string | null | undefined,
  catalog?: readonly string[] | null | undefined,
): AntigravitySpeed[] {
  if (!model) return ["low", "medium", "high"];
  const family = antigravityFamilyId(model);
  const slugs = (catalog ?? []).map((s) => s.trim()).filter(Boolean);
  const pool = slugs.filter((s) => antigravityFamilyId(s) === family);

  if (pool.length > 0) {
    const speeds = pool
      .map(antigravitySpeedOf)
      .filter((s): s is AntigravitySpeed => s !== null);
    if (speeds.length > 0) {
      const order: AntigravitySpeed[] = ["low", "medium", "high"];
      return order.filter((s) => speeds.includes(s));
    }
  }

  if (family.includes("pro")) {
    return ["low", "high"];
  }
  if (family.startsWith("gemini-")) {
    return ["low", "medium", "high"];
  }
  return [];
}

/**
 * Pick the vendor slug for this family at this speed. Missing bands fall over
 * to the nearest neighbour so a Pro row that only ships High/Low still works.
 */
export function resolveAntigravitySlug(
  selected: string | null | undefined,
  effort: string | null | undefined,
  catalog?: readonly string[] | null | undefined,
): string | null {
  if (!selected?.trim()) return null;
  const family = antigravityFamilyId(selected);
  const want = antigravitySpeedForEffort(effort);
  const slugs = (catalog ?? []).map((slug) => slug.trim()).filter(Boolean);
  const pool = slugs.filter((slug) => antigravityFamilyId(slug) === family);

  const pick = (band: AntigravitySpeed): string | undefined =>
    pool.find((slug) => antigravitySpeedOf(slug) === band);

  const fallbackOrder: AntigravitySpeed[] =
    want === "low"
      ? ["low", "medium", "high"]
      : want === "medium"
        ? ["medium", "high", "low"]
        : ["high", "medium", "low"];

  for (const band of fallbackOrder) {
    const hit = pick(band);
    if (hit) return hit;
  }

  const unsuffixed = pool.find((slug) => antigravitySpeedOf(slug) == null);
  if (unsuffixed) return unsuffixed;
  if (pool[0]) return pool[0];

  if (family.startsWith("gemini-")) {
    if (family.includes("pro") && want === "medium") {
      return `${family}-high`;
    }
    return `${family}-${want}`;
  }
  if (family.includes("sonnet")) return "claude-sonnet-4-6";
  if (family.includes("opus")) return "claude-opus-4-6-thinking";
  if (family.includes("gpt-oss") || family.includes("120b")) return "gpt-oss-120b-medium";

  return `${family}-${want}`;
}
