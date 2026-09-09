/**
 * The Splash tab of the Studio dock (GAD-161), and the side-by-side diff it shipped beside.
 *
 * These are source and wiring facts, and that is the point: the panel's job is to draw what
 * Rust decided and send two strings back, so what can actually break here are the seams. A
 * command missing from `api.ts`, a tab in the union but not in the drawer, a control that
 * invents a bound Rust does not enforce — none of those are visible to a test of the panel's
 * own logic, and every one of them ships a dead button.
 */

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

// .gitattributes pins the sources to LF but not the stylesheets, so a Windows checkout hands
// these back with CRLF and every multi-line needle below would silently miss.
const read = (rel) =>
  readFileSync(new URL(rel, import.meta.url), "utf8").replaceAll("\r\n", "\n");

const panel = read("../src/components/SplashPanel.tsx");
const dock = read("../src/studio/StudioBottomDock.tsx");
const api = read("../src/lib/api.ts");
const main = read("../src/main.tsx");
const modal = read("../src/screens/ReviewChangesModal.tsx");

test("SPL-001: every splash command the panel needs is on the api", () => {
  for (const command of [
    "splashLibrary",
    "splashProjectState",
    "splashGenerate",
    "splashApply",
    "splashImportLogo",
    "splashFavourite",
    "splashFavourites",
    "splashForgetFavourite",
    "splashExport",
  ]) {
    assert.ok(api.includes(`${command}:`), `api.ts is missing ${command}`);
    assert.ok(panel.includes(`api.${command}(`), `the panel never calls ${command}`);
  }
});

test("SPL-002: the dock carries a Splash tab, wired like every other one", () => {
  assert.ok(dock.includes('"splash"'), "the tab must be in the union");
  assert.ok(dock.includes('splash: "Splash Screen"'), "the drawer needs a title for it");
  assert.ok(dock.includes('activeTab === "splash" && ('), "the drawer body renders the panel");
  assert.ok(dock.includes("<SplashPanel"), "the panel is mounted");
  assert.ok(dock.includes("import { SplashPanel }"), "the panel is imported");
  assert.ok(
    dock.includes('onSelectTab(activeTab === "splash" ? null : "splash")'),
    "the tab button toggles the drawer like every other tab",
  );
});

test("SPL-003: the panel's stylesheet is loaded", () => {
  assert.ok(main.includes('import "./styles/splash.css"'), "splash.css must be imported");
});

test("SPL-004: the panel has all four load states", () => {
  // INV-075: every panel says which of idle, loading, ready and error it is in.
  for (const state of ["idle", "loading", "ready", "error"]) {
    assert.ok(panel.includes(`"${state}"`), `the panel has no ${state} state`);
  }
  assert.ok(panel.includes("studio-dock-empty"), "the empty state uses the dock's own style");
  assert.ok(panel.includes("studio-dock-error"), "and so does the error state");
});

test("SPL-005: every hook runs before the panel's early returns", () => {
  // The fault that blanked the HUD tab (React #310). Pinned here so the newest panel does
  // not reintroduce it.
  const body = panel.slice(panel.indexOf("export function SplashPanel"));
  const firstReturn = body.indexOf("\n  if (library.state === ");
  assert.ok(firstReturn > 0, "the panel still guards on its load state");
  const strayHook = body
    .slice(firstReturn)
    .match(/\n\s+(?:const [\w{}, ]+ = )?use(?:State|Effect|Memo|Callback|Ref|Reducer)\(/);
  assert.equal(strayHook, null, `a hook after an early return crashes React: ${strayHook?.[0]}`);
});

test("SPL-006: the hold slider takes its bounds from Rust, never its own", () => {
  // INV-051. If the panel hard-coded 3000–5000 and Rust changed, the slider would offer a
  // hold the gate then refuses, and the user would meet the refusal instead of the limit.
  assert.ok(panel.includes("min_duration_ms"), "the minimum comes from the library view");
  assert.ok(panel.includes("max_duration_ms"), "and so does the maximum");
  assert.ok(
    panel.includes("min={durationBounds.min}") && panel.includes("max={durationBounds.max}"),
    "the slider must be bound to them",
  );
  assert.ok(
    panel.includes("maxLength={view.max_brief_chars}"),
    "the brief's limit is Rust's too",
  );
});

test("SPL-007: a logo is uploaded with a licence, never without", () => {
  // INV-074: a game's boot screen is exactly the asset that reaches a store listing.
  assert.ok(panel.includes("SUGGESTED_LICENCE"), "a licence is offered");
  assert.ok(
    panel.includes("api.splashImportLogo(projectPath, picked, SUGGESTED_LICENCE)"),
    "the import must carry a licence",
  );
});

test("SPL-008: the panel offers favourite and export once a splash exists", () => {
  assert.ok(panel.includes("Favourite"), "a splash can be kept");
  assert.ok(panel.includes("Export"), "and taken out of the studio");
  assert.ok(panel.includes("api.splashExport("), "export is wired");
  assert.ok(panel.includes("splash-favourite-list"), "saved splashes are listed to reload");
});

test("SPL-009: the side-by-side diff is a second layout, not a second class name", () => {
  // The complaint: the split toggle did nothing. It swapped a class that no rule matched,
  // so both modes drew the same unified table.
  assert.ok(modal.includes("function SplitHunk("), "split needs its own rows");
  assert.ok(modal.includes("function UnifiedHunk("), "and unified keeps its own");
  assert.ok(
    modal.includes('viewMode === "split" ? (') && modal.includes("<SplitHunk"),
    "the toggle must choose between them",
  );
  assert.ok(modal.includes("function splitRows("), "the two sides have to be paired");

  const css = read("../src/styles/screens.css");
  assert.ok(css.includes(".diff-table-split"), "the split table needs rules of its own");
  assert.ok(
    css.includes("table-layout: fixed"),
    "without a fixed layout one long line pushes the other column off screen",
  );
});

test("SPL-010: generating puts the splash into the game in the same press", () => {
  // The owner asked for this outright: "when it generates use that ingame when clicked".
  // Generate used to only preview, so nothing reached the game until a second button.
  assert.ok(
    panel.includes("api.splashApply(projectPath, next,"),
    "generate must apply the spec it just made",
  );
  assert.ok(
    panel.includes("Generate and use in game"),
    "and the button must say so",
  );
  // The spec is set before the apply is attempted, so a refused build still shows the card.
  const generate = panel.slice(panel.indexOf("const generate = useCallback("));
  assert.ok(
    generate.indexOf("setSpec(next)") < generate.indexOf("api.splashApply("),
    "a gate that stops the build must not also swallow the preview",
  );
});

test("SPL-011: a splash can be the logo alone, with no lettering", () => {
  // Rust allows it; the preview must not draw an empty label where there is no text, since
  // an empty line of type pushes the logo off centre for no visible reason.
  assert.ok(panel.includes("spec.title.trim() ? ("), "the title is drawn only when there is one");
  assert.ok(panel.includes("const hasText ="), "the preview knows whether there is any text");
  assert.ok(
    !panel.includes('spec.title || "Untitled"'),
    "no placeholder title may be invented for a logo-only splash",
  );
});

test("SPL-012: the panel shows what it is doing while it generates", () => {
  const css = read("../src/styles/splash.css");

  // Two named steps, not one "Working…" — they take very different amounts of time, and a
  // single label makes the slow half (Godot compiling the script) look like a hang.
  assert.ok(panel.includes("type SplashStep ="), "the steps must be a type, not a boolean");
  assert.ok(panel.includes("STEP_CAPTION"), "each step needs words of its own");
  assert.ok(panel.includes('setStep("generating")'), "the first step is announced");
  assert.ok(panel.includes('setStep("building")'), "and so is the second");
  assert.ok(panel.includes("setStep(null)"), "and it is cleared when the work ends");

  // The card replaces the preview while it runs, so the panel keeps its height.
  assert.ok(panel.includes("function SplashGenerating("), "there must be a generating card");
  assert.ok(panel.includes("<SplashGenerating"), "and it must be rendered");
  assert.ok(panel.includes('role="status"'), "a live region, so it is announced");

  assert.ok(css.includes(".splash-generating-sheen"), "the card needs the moving sheen");
  assert.ok(css.includes("@keyframes splash-sheen"), "and the keyframes behind it");
  assert.ok(css.includes(".splash-spinner"), "the button needs a spinner");
  assert.ok(
    css.includes("prefers-reduced-motion"),
    "the caption must still say the step when motion is off",
  );
});
