/**
 * SPA-2xx / SPA-3xx: Blender over MCP and the self-directed desktop.
 *
 * The owner's asks: the AI may build props in Blender and land them in `assets/`; it uses
 * the desktop when *it* decides the task needs it; while it runs, the overlay should feel
 * like something deep is happening; and it can reach the whole machine. These tests read
 * the prompts, the overlay wiring and the Settings card so none of that quietly regresses.
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

test("the overlay draws every action with its caption and follows the pointer", () => {
  const overlay = read("src/overlay.tsx");
  assert.ok(overlay.includes('"computer-overlay-action"'), "the page listens for actions");
  assert.ok(overlay.includes("cursor={active ? cursor : null}"), "the reticle gets the pointer");
  const aura = read("src/components/ComputerUseAura.tsx");
  for (const layer of ["drawFloor", "drawBeam", "drawScanFront", "drawPackets", "drawRipples", "drawReticle"]) {
    assert.ok(aura.includes(`const ${layer} =`), `${layer} is drawn`);
  }
  assert.ok(aura.includes("prefers-reduced-motion"), "reduced motion is respected");
  assert.ok(aura.includes("Press Esc twice to stop"), "the emergency stop is always printed");
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
