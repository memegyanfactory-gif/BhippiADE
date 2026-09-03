/**
 * What a Games card says (GAD-018), separated from how it looks.
 *
 * The card joins the project list with `list_workspace_sessions`: name, path, when the game
 * was last worked on, and how many sessions it has. The poster itself is not here — Rust's
 * `game_card_info` reads it out of the project and hands the page a `data:` URL. What is
 * here is the fallback: a deterministic wash derived from the game's name, deterministic so
 * a game keeps the same tile between launches instead of flickering to a new colour every
 * render.
 */

export type GameCardProject = {
  name: string;
  path: string;
  last_opened_at: number;
};

export type GameCardSession = {
  project_path: string;
  updated_at: string;
};

export type GameCard = {
  name: string;
  path: string;
  sessionCount: number;
  /** ISO timestamp of the most recent session, or `null` when the game has none. */
  lastActivity: string | null;
  /** Unix seconds — the sort key, newest first. */
  lastActivityAt: number;
};

/**
 * A project path as a comparison key: verbatim prefix stripped, `/`-separated, no trailing
 * slash, lowercase — the same normalisation Rust's `paths_match` applies. Empty for a path
 * that names nothing, which is never equal to another.
 */
export function projectKey(value?: string | null): string {
  return (value ?? "")
    .replace(/^(\/\/\?\/|\/\/\?|[\\/]{2}\?)[\\/]?/, "")
    .replace(/\\/g, "/")
    .replace(/\/+$/, "")
    .toLowerCase()
    .trim();
}

/** Whether two paths name the same project. */
export function samePath(a?: string | null, b?: string | null): boolean {
  const left = projectKey(a);
  return left === projectKey(b) && left.length > 0;
}

export function buildGameCards(
  projects: readonly GameCardProject[],
  sessions: readonly GameCardSession[],
): GameCard[] {
  const cards = projects.map((project) => {
    const own = sessions.filter((session) => samePath(session.project_path, project.path));
    let latest = 0;
    let lastActivity: string | null = null;
    for (const session of own) {
      const at = Date.parse(session.updated_at);
      if (Number.isFinite(at) && at > latest) {
        latest = at;
        lastActivity = session.updated_at;
      }
    }
    // With no sessions yet, "last activity" is when the game was last opened — which is
    // still true, and better than an empty cell that reads like missing data.
    const lastActivityAt = latest > 0 ? Math.floor(latest / 1000) : project.last_opened_at;
    return {
      name: project.name,
      path: project.path,
      sessionCount: own.length,
      lastActivity,
      lastActivityAt,
    };
  });
  return cards.sort((a, b) => b.lastActivityAt - a.lastActivityAt);
}

/**
 * A stable two-stop wash for a game with no poster. Hue comes from the name, so the same
 * game is the same colour on every machine and every launch.
 *
 * Both stops are translucent, which is the whole point: the tile paints this over
 * `--surface-2`, so the placeholder is a tint of whatever the current theme's card is
 * rather than a dark rectangle that only reads correctly in the dark palette. It is a
 * quiet background for the game's initial, not a picture pretending to be one.
 */
export function posterGradient(name: string): string {
  let hash = 0;
  for (let index = 0; index < name.length; index += 1) {
    hash = (hash * 31 + name.charCodeAt(index)) % 360_000;
  }
  const hue = hash % 360;
  const second = (hue + 42) % 360;
  return `linear-gradient(135deg, hsl(${hue} 46% 52% / 0.22), hsl(${second} 40% 46% / 0.07))`;
}

/**
 * The letter a poster-less tile shows. Uppercase, one character, and never the empty
 * string — a nameless game gets a dot rather than a hole where the art should be.
 */
export function posterInitial(name: string): string {
  const first = name.trim().charAt(0);
  return first.length > 0 ? first.toUpperCase() : "·";
}
