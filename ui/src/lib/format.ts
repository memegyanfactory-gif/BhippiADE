// Presentation helpers only. Nothing here derives a figure — every number these
// functions touch already arrived from Rust (INV: no business logic in TypeScript).

/** 912 → "912" · 84_700 → "85k" · 1_240_000 → "1.2M" */
export function tokens(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 10_000) return `${Math.round(count / 1_000)}k`;
  if (count >= 1_000) return `${(count / 1_000).toFixed(1)}k`;
  return `${count}`;
}

/**
 * A dollar figure that never lies about being zero.
 *
 * Real per-turn API spend is routinely a fraction of a cent — a 900-token Haiku turn is
 * $0.0024 — and both the old rules here ("<$0.01") and the chat meter's ("$0.00") threw
 * that away, so a panel showing a day of real usage could read $0.00 beside a five-figure
 * token count. Sub-cent amounts keep enough significant digits to be checkable; a cent
 * and above is the familiar two decimals. Exactly zero is still "$0.00".
 */
export function usd(amount: number): string {
  if (!Number.isFinite(amount) || amount === 0) return "$0.00";
  const sign = amount < 0 ? "-" : "";
  const value = Math.abs(amount);
  if (value < 0.0001) return `${sign}$${value.toFixed(6)}`;
  if (value < 0.01) return `${sign}$${value.toFixed(4)}`;
  return `${sign}$${value.toFixed(2)}`;
}

export function percent(fraction: number): string {
  return `${Math.round(Math.min(Math.max(fraction, 0), 1) * 100)}%`;
}

/** 15_960 → "4h 26m" · 900 → "15m" · 40 → "under a minute" */
export function countdown(seconds: number): string {
  if (seconds < 60) return "under a minute";
  const hours = Math.floor(seconds / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  if (hours === 0) return `${minutes}m`;
  return `${hours}h ${minutes}m`;
}

/** "2026-08-26" → "26 Aug", for chart tooltips and axis marks. */
export function shortDate(iso: string): string {
  const parsed = new Date(`${iso}T00:00:00`);
  if (Number.isNaN(parsed.getTime())) return iso;
  return parsed.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/**
 * The house rule for a project name in chrome: never longer than this many characters.
 *
 * A folder called `a-cozy-third-person-island-game` would otherwise set the width of
 * the sidebar badge, the header, and the composer chip all at once. CSS ellipsis alone
 * does not fix that — the element still *asks* for the full width first — so the string
 * is cut here and the full name stays in the `title` attribute.
 */
export const MAX_NAME_CHARS = 20;

/** "a-cozy-third-person-island-game" → "a-cozy-third-person…" */
export function clipName(name: string, limit = MAX_NAME_CHARS): string {
  const trimmed = name.trim();
  if (trimmed.length <= limit) return trimmed;
  return `${trimmed.slice(0, limit - 1).trimEnd()}…`;
}

/** "C:/Work/VSCode/Bhippi content" → "…/VSCode/Bhippi content", keeping the tail. */
export function clipPath(path: string, limit = 42): string {
  const normalised = path.replace(/\\/g, "/");
  if (normalised.length <= limit) return normalised;
  const segments = normalised.split("/").filter(Boolean);
  let tail = "";
  for (let index = segments.length - 1; index >= 0; index -= 1) {
    const next = `/${segments[index]}${tail}`;
    if (next.length + 1 > limit) break;
    tail = next;
  }
  return tail ? `…${tail}` : `…${normalised.slice(-limit + 1)}`;
}

/** 812 → "812 B" · 24_000 → "23 KB" · 3_100_000 → "3.0 MB" */
export function bytes(count: number): string {
  if (count < 1024) return `${count} B`;
  if (count < 1024 * 1024) return `${Math.round(count / 1024)} KB`;
  return `${(count / (1024 * 1024)).toFixed(1)} MB`;
}

/** Age of an ISO timestamp, short — "just now", "12m", "3h", "2d". Presentation only. */
export function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const seconds = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}
