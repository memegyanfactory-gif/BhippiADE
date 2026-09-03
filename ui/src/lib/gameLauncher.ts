/**
 * The "Describe your game" launcher's logic (GAD-015), kept out of the component so it can
 * be tested directly: what the chips add to the prompt, and what folder name a prompt
 * turns into.
 *
 * Chips are *not* separate state. Picking "Platformer" appends the word to the prompt text,
 * so what the user sees in the box is exactly what the first Studio message carries — there
 * is no hidden second description that the plan is quietly built from.
 */

export const GENRES = [
  "Platformer",
  "Top-down action",
  "FPS arena",
  "Racing",
  "Exploration",
  "Physics puzzle",
  "Tower defense",
  "Survival",
  "Endless runner",
] as const;

export const PERSPECTIVES = [
  "Third-person",
  "First-person",
  "Top-down",
  "Side-scroller",
] as const;

export const ART_STYLES = [
  "Low-poly",
  "Pixel",
  "Cel-shaded",
  "Realistic",
  "Cozy",
  "Dark",
  "Neon",
] as const;

/** Appends a chip's words to the prompt, never twice and never gluing two words together. */
export function appendChip(prompt: string, chip: string): string {
  const current = prompt.trimEnd();
  if (current.toLowerCase().includes(chip.toLowerCase())) return prompt;
  if (current.length === 0) return chip;
  // A prompt that already ends a clause keeps its punctuation; otherwise chips read as a
  // comma-separated list, which is how someone would have typed them.
  const joiner = /[,.;:—-]$/.test(current) ? " " : ", ";
  return `${current}${joiner}${chip}`;
}

/** True when the prompt already mentions this chip, so the chip can render as chosen. */
export function chipChosen(prompt: string, chip: string): boolean {
  return prompt.toLowerCase().includes(chip.toLowerCase());
}

/**
 * A folder name from the first four meaningful words of the prompt.
 *
 * Lowercase, ASCII, hyphen-joined, never empty and never a Windows reserved name — the
 * folder is created on disk, so a name that cannot exist is a failure the user cannot fix
 * from the launcher.
 */
export function slugifyPrompt(prompt: string, words = 4): string {
  const cleaned = prompt
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, " ")
    .trim();
  const slug = cleaned
    .split(/[\s-]+/)
    .filter((word) => word.length > 0)
    .slice(0, words)
    .join("-")
    .replace(/^-+|-+$/g, "");
  if (slug.length === 0) return "new-game";
  // CON, PRN, AUX, NUL, COM1-9, LPT1-9 cannot be directory names on Windows.
  if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/.test(slug)) return `${slug}-game`;
  return slug.slice(0, 48).replace(/-+$/, "") || "new-game";
}

/** `taken` already exists, so the launcher counts up rather than failing the create. */
export function uniqueFolderName(base: string, taken: readonly string[]): string {
  const used = new Set(taken.map((name) => name.toLowerCase()));
  if (!used.has(base.toLowerCase())) return base;
  for (let suffix = 2; suffix < 1000; suffix += 1) {
    const candidate = `${base}-${suffix}`;
    if (!used.has(candidate.toLowerCase())) return candidate;
  }
  return `${base}-${Date.now()}`;
}

/**
 * The message the new game's first Studio turn carries.
 *
 * Reference images ride in the text because that is all the composer sends: one string per
 * turn. Listing their paths keeps the attachment visible to the user and to the agent
 * rather than silently dropping the files they picked.
 */
export function composeFirstMessage(prompt: string, references: readonly string[] = []): string {
  const text = prompt.trim();
  if (references.length === 0) return text;
  const list = references.map((path) => `- ${path}`).join("\n");
  return `${text}\n\nReference images:\n${list}`;
}

/**
 * Which Godot scaffold a described game starts from (GAD-014).
 *
 * Only the dimension is decided here, from the same words the chips insert: a top-down,
 * side-scrolling, pixel or "2D" description starts as the 2D template, everything else as
 * the third-person 3D one. The real archetype decision belongs to Rust's intent compiler
 * (`bhippi_engine::intent::draft`) once the chat bridge runs it on the first message; this
 * only chooses which starter project that message lands in.
 */
export function templateForPrompt(prompt: string): "third_person3_d" | "top_down2_d" {
  return /\b(top-?down|side-?scroll\w*|pixel(?:\s|-)?art|pixel|2d)\b/i.test(prompt)
    ? "top_down2_d"
    : "third_person3_d";
}
