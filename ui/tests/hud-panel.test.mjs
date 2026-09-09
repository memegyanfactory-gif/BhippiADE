/**
 * The HUD tab of the Studio dock (GAD-160).
 *
 * These are source and wiring facts, and that is the point: the HUD panel's job is to draw
 * what Rust decided and send back two strings, so the things that can actually break here
 * are the seams. A command missing from `api.ts`, a slot the preview has no cell for, a tab
 * that is in the union but not in the drawer — none of those are visible to a test of the
 * panel's own logic, and every one of them ships a blank panel.
 */

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

// .gitattributes pins the sources to LF but not the stylesheets, so a Windows checkout
// hands these back with CRLF and every multi-line needle below would silently miss.
const read = (rel) =>
  readFileSync(new URL(rel, import.meta.url), "utf8").replaceAll("\r\n", "\n");

const panel = read("../src/components/HudPanel.tsx");
const hudCss = read("../src/styles/hud.css");
const dock = read("../src/studio/StudioBottomDock.tsx");
const apiTs = read("../src/lib/api.ts");
const ipcTs = read("../src/lib/ipc.ts");
const main = read("../src/main.tsx");

/** The nine anchor slots Rust names, in `HudSlot`'s serde form. */
const SLOTS = [
  "top_left",
  "top_centre",
  "top_right",
  "mid_left",
  "centre",
  "mid_right",
  "bottom_left",
  "bottom_centre",
  "bottom_right",
];

/** The twelve widget kinds Rust names, in `HudWidgetKind`'s serde form. */
const KINDS = [
  "bar",
  "segments",
  "counter",
  "timer",
  "text",
  "reticle",
  "ring",
  "compass",
  "minimap",
  "toast",
  "icon_row",
  "action",
];

test("HUD-001: every HUD command reaches the panel through api.ts", () => {
  for (const command of [
    "hudLibrary",
    "hudProjectState",
    "hudApply",
    "fabVaultScan",
    "fabImportIcons",
  ]) {
    assert.ok(
      ipcTs.includes(`${command}:`),
      `${command} is missing from the generated bindings — re-run export-bindings`,
    );
    assert.ok(apiTs.includes(`${command}:`), `${command} is missing from api.ts`);
    assert.ok(panel.includes(`api.${command}(`), `the panel never calls ${command}`);
  }
});

test("HUD-002: the preview has a cell for every anchor slot Rust can return", () => {
  // A slot with no cell renders nothing, and the widget in it silently disappears from
  // the card — which reads as a preset with fewer elements than it has.
  for (const slot of SLOTS) {
    assert.ok(panel.includes(`${slot}:`), `the preview has no cell for ${slot}`);
  }
});

test("HUD-003: every widget kind has a label", () => {
  for (const kind of KINDS) {
    assert.ok(panel.includes(`${kind}:`), `the preview has no label for the ${kind} kind`);
  }
});

test("HUD-004: the budget row is drawn against the cap Rust sends, not a constant", () => {
  // The doctrine's cap lives in Rust (`MAX_PERSISTENT_WIDGETS`). A hard-coded 5 here would
  // keep drawing five cells after that number changed.
  assert.ok(
    panel.includes("view.max_persistent"),
    "the budget meter must be sized from the library's own cap",
  );
  assert.ok(
    /length:\s*view\.max_persistent/.test(panel),
    "the cells are generated from the cap",
  );
});

test("HUD-005: persistent and on-change elements are drawn differently", () => {
  // The whole reason a HUD card is worth looking at is that you can see the budget: solid
  // for always-on, outlined for the ones that appear and fade.
  assert.ok(panel.includes('widget.visibility === "persistent"'), "visibility decides the chip");
  assert.ok(
    hudCss.includes(".hud-chip:not(.persistent)"),
    "on-change chips need a style of their own",
  );
  assert.ok(
    /\.hud-chip:not\(\.persistent\)\s*{[^}]*border-style:\s*dashed/.test(hudCss),
    "an on-change chip is outlined rather than filled",
  );
});

test("HUD-006: the preview draws the safe area the build actually applies", () => {
  // 4 % of the shorter edge, the same inset `hud.rs` bakes into the scene. A preview that
  // showed widgets flush to the edge would be lying about where they land.
  assert.ok(hudCss.includes(".hud-preview-safe"), "the safe area is drawn");
  assert.ok(/\.hud-preview-safe\s*{[^}]*inset:\s*4%/.test(hudCss), "the inset is 4 %");
});

test("HUD-007: the import prompt asks for a licence and cannot be sent empty", () => {
  // Bhippi cannot read a licence out of a Fab pack, and INV-074 blocks a release on an
  // asset whose sidecar cannot name one. The prompt is the only place that can answer.
  assert.ok(panel.includes("SUGGESTED_LICENCE"), "a licence is suggested, not assumed");
  assert.ok(
    panel.includes("licence.trim().length === 0"),
    "an empty licence must not be importable",
  );
});

test("HUD-008: packs Godot cannot open are listed with their reason", () => {
  // Hiding them reads as a bug in Bhippi. The useful answer is that the pack was never
  // downloaded in a format Godot has.
  assert.ok(panel.includes('pack.usability === "unusable"'), "unusable packs are surfaced");
  assert.ok(panel.includes("pack.note"), "and each one carries its reason");
  assert.ok(hudCss.includes(".hud-unusable"), "the disclosure has a style");
});

test("HUD-009: the dock carries the tab and the drawer draws it", () => {
  assert.ok(dock.includes('| "hud"'), "hud is in the StudioDockTab union");
  assert.ok(dock.includes('hud: "HUD Presets"'), "the drawer has a title for it");
  assert.ok(dock.includes('activeTab === "hud" && ('), "the drawer body renders the panel");
  assert.ok(dock.includes("<HudPanel"), "the panel is mounted");
  assert.ok(
    dock.includes('onSelectTab(activeTab === "hud" ? null : "hud")'),
    "the tab button toggles the drawer like every other tab",
  );
});

test("HUD-010: the panel's stylesheet is loaded", () => {
  assert.ok(main.includes('import "./styles/hud.css"'), "hud.css must be imported somewhere");
});

test("HUD-011: the panel has all four load states", () => {
  // INV-075: every panel says which of idle, loading, ready and error it is in.
  for (const state of ["loading", "error", "ready", "idle"]) {
    assert.ok(panel.includes(`"${state}"`), `the panel has no ${state} state`);
  }
  assert.ok(panel.includes("studio-dock-empty"), "the empty state uses the dock's own style");
  assert.ok(panel.includes("studio-dock-error"), "and so does the error state");
});

test("HUD-012: a project that already has a HUD opens on the one it has", () => {
  // Otherwise the panel opens on the first card and the primary button offers to replace a
  // HUD with a different one, which is the opposite of what the user came here to do.
  assert.ok(panel.includes("if (data.preset) setPreset(data.preset)"), "the preset is restored");
  assert.ok(panel.includes("if (data.skin) setSkin(data.skin)"), "and so is the skin");
  assert.ok(panel.includes("installed?.installed"), "the button says rebuild, not build");
});
