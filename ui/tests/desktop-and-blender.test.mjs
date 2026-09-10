/**
 * SPA-2xx / SPA-3xx: Blender over MCP and the self-directed desktop.
 *
 * The owner's asks: the AI may build props in Blender and land them in `assets/`; it uses
 * the desktop when the task genuinely needs it; while it runs, one panel shows what it is
 * doing; and it can reach the whole machine. These tests read the prompts, that panel and
 * the Settings card so none of it quietly regresses.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const read = (rel) => fs.readFileSync(path.join(here, "..", rel), "utf8");
const prompt = (name) => fs.readFileSync(path.join(here, "..", "..", "prompts", name), "utf8");

test("the desktop protocol names the reach actions and the self-request", () => {
  const computer = prompt("chat-computer-use.md");
  for (const action of ["open_app", "open_url", "focus_window", "list_windows", "wait"]) {
    assert.ok(computer.includes(`"action":"${action}"`), `${action} is documented`);
  }
  const desktop = prompt("chat-desktop.md");
  assert.ok(desktop.includes("<computer_request>"), "the model can ask for the desktop");
  assert.ok(desktop.includes("Stay in text when text is enough"), "and is told when not to");
});

test("Blender is a prompt with a landing rule, not a free-for-all", () => {
  const blender = prompt("chat-blender.md");
  assert.ok(blender.includes("<asset_register>"), "what Blender exports gets registered");
  assert.ok(blender.includes("write only under `assets/`"), "and lands under assets/ only");
  const assets = prompt("chat-assets.md");
  assert.ok(assets.includes("<asset_import>"), "the library import protocol");
  assert.ok(assets.includes("You never write `.meta.json` yourself"), "sidecars are Bhippi's");
});

test("a Computer Use turn is watched in one panel, with nothing painted on the desktop", () => {
  // ADR-0054 retired the full-screen overlay window. The visibility promise it carried is
  // kept — every action is shown with its reason — and the panel is where.
  const panel = read("src/components/BhippiComputerPanel.tsx");
  assert.ok(panel.includes("computer-panel-frame"), "the frame it is looking at");
  assert.ok(panel.includes("computer-panel-status"), "what it is doing now");
  assert.ok(panel.includes("of {maxActions} steps"), "how far through the budget it is");
  assert.ok(panel.includes("computer-panel-stop"), "and how to stop it");
  assert.ok(
    panel.includes("press Esc twice to stop"),
    "the emergency stop is printed wherever the run is watched",
  );

  // The decoration that described actions the caption already names is gone, not moved.
  for (const removed of [
    "bhippi-virtual-cursor",
    "bhippi-cursor-spark",
    "bhippi-screen-scan",
    "bhippi-screen-vignette",
    "TrailSpark",
  ]) {
    assert.ok(!panel.includes(removed), `${removed} should have been deleted`);
  }
});

test("the desktop overlay window is gone from the build", () => {
  const vite = read("vite.config.ts");
  assert.ok(!vite.includes("overlay.html"), "the overlay is no longer a Vite entry");
  for (const gone of [
    "src/overlay.tsx",
    "src/components/ComputerUseAura.tsx",
    "src/components/OverlayCursor.tsx",
    "overlay.html",
  ]) {
    assert.ok(!fs.existsSync(path.join(here, "..", gone)), `${gone} should have been deleted`);
  }
});

test("Settings › Integrations carries the Blender card through typed commands", () => {
  const settings = read("src/screens/SettingsModal.tsx");
  assert.ok(settings.includes("function BlenderMcpCard"));
  assert.ok(settings.includes("api.setBlenderMcp(enabled, command, args.split("));
  const api = read("src/lib/api.ts");
  assert.ok(api.includes("blenderMcpStatus:") && api.includes("setBlenderMcp:"));
  const ipc = read("src/lib/ipc.ts");
  assert.ok(ipc.includes("export type BlenderMcpStatus"), "the status is Rust's shape");
});
