/**
 * The four Studio destinations and the migration that gets a returning user to one of them
 * (GAD-008, docs/16 §4.2).
 *
 * The nav used to read Agent · Research · Automation · Library · Plugins. Two of those
 * screens no longer exist and two were renamed, so a persisted key from an older build —
 * in `localStorage`, or in a plugin card's `screen:` target that Rust still ships — has to
 * be translated on read. Landing on a route that renders nothing is a blank canvas, which
 * reads as a broken app rather than as a renamed screen.
 *
 * This module is deliberately free of React so the migration can be tested directly.
 */

export type Screen = "studio" | "projects" | "games" | "assets" | "addons";

/** Nav order. The sidebar renders these; the router switches on them. */
export const SCREENS: readonly Screen[] = ["studio", "projects", "games", "assets", "addons"] as const;

/** The route the app opens on when nothing was remembered. */
export const DEFAULT_SCREEN: Screen = "studio";

/** Where a screen name saved by an older build now lives. */
const RENAMED: Record<string, Screen> = {
  chat: "studio",
  engine: "studio",
  project: "projects",
  plugins: "addons",
};

export function isScreen(value: unknown): value is Screen {
  return typeof value === "string" && (SCREENS as readonly string[]).includes(value);
}

/**
 * A persisted or Rust-supplied screen key as this build understands it.
 *
 * Returns `null` for a key that named a screen this build removed (Research, Automation,
 * Library) so the caller can fall back rather than route to nothing.
 */
export function migrateScreenKey(raw: string | null | undefined): Screen | null {
  if (typeof raw !== "string") return null;
  const key = raw.trim().toLowerCase();
  if (isScreen(key)) return key;
  return RENAMED[key] ?? null;
}

/** The same migration with the launcher-safe fallback applied. */
export function readScreen(raw: string | null | undefined): Screen {
  return migrateScreenKey(raw) ?? DEFAULT_SCREEN;
}
