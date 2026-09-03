/**
 * The Studio shell: routes, launcher and tiers (GAD-008, GAD-015, GAD-017).
 *
 * These are the decisions that are genuinely logic rather than layout — where an old
 * screen key now points, what the launcher's chips do to the prompt, what folder that
 * prompt becomes, and whether a tier can answer. Getting any of them wrong shows up as a
 * blank canvas, a folder that cannot exist, or a chip that quietly answers with the wrong
 * backend, none of which a screenshot would catch.
 */

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import {
  DEFAULT_SCREEN,
  SCREENS,
  isScreen,
  migrateScreenKey,
  readScreen,
} from "../src/lib/screens.ts";
import {
  ART_STYLES,
  GENRES,
  PERSPECTIVES,
  appendChip,
  chipChosen,
  composeFirstMessage,
  slugifyPrompt,
  uniqueFolderName,
} from "../src/lib/gameLauncher.ts";
import { TIER_NAMES, matchTier, tierUsability } from "../src/lib/tiers.ts";

// ── Screen keys ────────────────────────────────────────────────────────────────

test("the nav has the game destinations", () => {
  assert.deepEqual([...SCREENS], ["studio", "projects", "games", "assets", "addons"]);
  assert.equal(DEFAULT_SCREEN, "studio");
});

test("a screen key written by the pre-pivot build lands on its renamed screen", () => {
  assert.equal(migrateScreenKey("chat"), "studio");
  assert.equal(migrateScreenKey("plugins"), "addons");
  // Case and stray whitespace come from hand-edited config, not from us.
  assert.equal(migrateScreenKey("  Chat "), "studio");
});

test("a key naming a deleted screen falls back rather than routing to nothing", () => {
  // Research, Automation and Library are gone. Landing on them is a blank canvas.
  for (const gone of ["research", "automation", "library", "", "nonsense"]) {
    assert.equal(migrateScreenKey(gone), null, gone);
    assert.equal(readScreen(gone), "studio", gone);
  }
  assert.equal(readScreen(null), "studio");
  assert.equal(readScreen(undefined), "studio");
});

test("a current key survives the migration untouched", () => {
  for (const screen of SCREENS) {
    assert.equal(migrateScreenKey(screen), screen);
    assert.ok(isScreen(screen));
  }
  assert.equal(isScreen("chat"), false, "the old key is not a route on its own");
});

// ── Launcher ───────────────────────────────────────────────────────────────────

test("the chip rows are the ones the plan names", () => {
  assert.equal(GENRES.length, 9);
  assert.ok(GENRES.includes("Physics puzzle"));
  assert.deepEqual([...PERSPECTIVES], [
    "Third-person",
    "First-person",
    "Top-down",
    "Side-scroller",
  ]);
  assert.ok(ART_STYLES.includes("Cel-shaded"));
});

test("a chip appends its words to the prompt instead of living in its own state", () => {
  let prompt = "";
  prompt = appendChip(prompt, "Platformer");
  assert.equal(prompt, "Platformer");
  prompt = appendChip(prompt, "Third-person");
  prompt = appendChip(prompt, "Low-poly");
  assert.equal(prompt, "Platformer, Third-person, Low-poly");
  // What the user sees in the box is exactly what the first message carries.
  assert.equal(composeFirstMessage(prompt), "Platformer, Third-person, Low-poly");
});

test("a chip is added once and reads as chosen afterwards", () => {
  const once = appendChip("A cozy Low-poly island game", "Low-poly");
  assert.equal(once, "A cozy Low-poly island game");
  assert.ok(chipChosen(once, "Low-poly"));
  assert.ok(chipChosen(once, "low-poly"), "matching is case-insensitive");
  assert.equal(chipChosen(once, "Pixel"), false);
});

test("a chip after a typed sentence keeps the punctuation the user wrote", () => {
  assert.equal(appendChip("collect 10 feathers.", "Exploration"), "collect 10 feathers. Exploration");
  assert.equal(appendChip("a racer", "Racing"), "a racer, Racing");
});

test("reference images ride in the first message rather than being dropped", () => {
  const message = composeFirstMessage("a neon racer", ["C:/art/ref-1.png", "C:/art/ref-2.png"]);
  assert.ok(message.startsWith("a neon racer"));
  assert.ok(message.includes("Reference images:"));
  assert.ok(message.includes("- C:/art/ref-1.png"));
  assert.ok(message.includes("- C:/art/ref-2.png"));
});

test("the folder name is the first four words of the prompt, slugified", () => {
  assert.equal(
    slugifyPrompt("A cozy third-person exploration game with jump-and-glide"),
    "a-cozy-third-person",
  );
  assert.equal(slugifyPrompt("Top-down action, Pixel, Neon"), "top-down-action-pixel");
});

test("a folder name is always something the filesystem can hold", () => {
  // Punctuation, emoji and accents cannot reach the folder name.
  assert.equal(slugifyPrompt("Café ☆ blaster!! 2000"), "cafe-blaster-2000");
  // A prompt with no usable words still creates a game rather than failing.
  assert.equal(slugifyPrompt("!!! ??? ***"), "new-game");
  assert.equal(slugifyPrompt(""), "new-game");
  // Windows device names are not directories.
  assert.equal(slugifyPrompt("con"), "con-game");
  assert.equal(slugifyPrompt("nul"), "nul-game");
  assert.ok(slugifyPrompt("word ".repeat(40)).length <= 48);
});

test("a folder name that is taken counts up instead of colliding", () => {
  assert.equal(uniqueFolderName("a-cozy-island", []), "a-cozy-island");
  assert.equal(uniqueFolderName("a-cozy-island", ["a-cozy-island"]), "a-cozy-island-2");
  assert.equal(
    uniqueFolderName("a-cozy-island", ["A-Cozy-Island", "a-cozy-island-2"]),
    "a-cozy-island-3",
    "an existing folder differing only in case is still taken",
  );
});

// ── Tiers ──────────────────────────────────────────────────────────────────────

const preset = (provider, effort = "balanced", model = null) => ({ provider, model, effort });

test("a tier whose provider is not usable is disabled and says why", () => {
  const options = [{ id: "demo", label: "Demo (offline)" }];
  const off = tierUsability(preset("claude"), options);
  assert.equal(off.usable, false);
  assert.match(off.reason, /claude/);
  assert.match(off.reason, /Settings/, "the reason names where to fix it");

  const on = tierUsability(preset("demo", "fast"), options);
  assert.equal(on.usable, true);
  assert.equal(on.reason, null);
});

test("an unusable tier is never swapped for a provider the user did not choose", () => {
  const options = [{ id: "demo", label: "Demo (offline)" }, { id: "codex", label: "Codex" }];
  const row = preset("claude", "quality");
  const state = tierUsability(row, options);
  assert.equal(state.usable, false);
  // The preset is left exactly as stored — nothing here rewrites it to a usable id.
  assert.deepEqual(row, { provider: "claude", model: null, effort: "quality" });
});

test("a tier with no provider set is disabled rather than silently working", () => {
  assert.equal(tierUsability(preset("  "), [{ id: "demo", label: "Demo" }]).usable, false);
  assert.equal(tierUsability(undefined, [{ id: "demo", label: "Demo" }]).usable, false);
});

test("the chip highlights only when the pickers really match its row", () => {
  const tiers = {
    quick: preset("demo", "fast"),
    balanced: preset("claude", "balanced"),
    max: preset("claude", "quality", "opus"),
  };
  assert.deepEqual([...TIER_NAMES], ["quick", "balanced", "max"]);
  assert.equal(matchTier(tiers, { provider: "demo", model: null, effort: "fast" }), "quick");
  // A preset with no model matches whatever model the provider is on.
  assert.equal(
    matchTier(tiers, { provider: "claude", model: "sonnet", effort: "balanced" }),
    "balanced",
  );
  // A preset that names a model does not match a different one.
  assert.equal(
    matchTier(tiers, { provider: "claude", model: "sonnet", effort: "quality" }),
    null,
  );
  assert.equal(matchTier(tiers, { provider: "claude", model: "opus", effort: "quality" }), "max");
  // A hand-assembled combination belongs to no tier, so no chip claims it.
  assert.equal(matchTier(tiers, { provider: "codex", model: null, effort: "ultra" }), null);
  assert.equal(matchTier(null, { provider: "demo", model: null, effort: "fast" }), null);
});

test("Studio uses the shared title bar and keeps the engine controls below the viewport", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const studio = readFileSync(new URL("../src/screens/StudioScreen.tsx", import.meta.url), "utf8");
  assert.match(app, /screen === "studio" \? \(\s*<>\s*<TitleBar/);
  assert.doesNotMatch(studio, /<StudioHeader/);
  assert.match(studio, /className="studio-engine-toolbar"/);
  assert.match(studio, /Play/);
  assert.match(studio, /Preview/);
  assert.match(studio, /Export/);
  assert.match(studio, /Undo/);
  // Redo went with the mock viewport (ADR-0045): there is no redo command to wire it to, and a
  // button that does nothing is worse than none. Workspace took its place.
  assert.match(studio, /Workspace/);
  assert.doesNotMatch(studio, /Redo/);
});

test("Studio chat is a left-side, resizable conversation dock", () => {
  const styles = readFileSync(new URL("../src/styles/studio.css", import.meta.url), "utf8");
  assert.match(styles, /\.studio-main-layout \.studio-left-column\s*\{[\s\S]*position: relative/);
  assert.match(styles, /resize: horizontal/);
  assert.match(styles, /\.studio-main-layout \.studio-left-column \.thread-wrap/);
  assert.match(styles, /display: none/);
  assert.match(styles, /\.studio-main-layout \.studio-left-column \.composer-zone/);
});
