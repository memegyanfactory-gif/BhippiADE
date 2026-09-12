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
  assert.ok(
    blender.includes("never write outside `assets/`"),
    "and lands under assets/ only",
  );
  const assets = prompt("chat-assets.md");
  assert.ok(assets.includes("<asset_import>"), "the library import protocol");
  assert.ok(assets.includes("You never write `.meta.json` yourself"), "sidecars are Bhippi's");
});

test("Blender is reached by writing Python, never by asking the user to open it", () => {
  // ADR-0062. The old path was Bhippi -> an MCP server -> an addon inside a *running*
  // Blender, so the owner's request for models came back as "open Blender and hit Connect".
  // Two of those three hops were a human, and the third is the only one that did anything.
  const blender = prompt("chat-blender.md");
  assert.ok(blender.includes("<blender_script>"), "the protocol is a script");
  assert.ok(blender.includes("no window"), "and it runs headless");
  // Collapsed, because the sentence is wrapped in the file and a line break is not a
  // difference in what the model reads.
  const flowed = blender.replace(/\s+/g, " ");
  assert.ok(
    flowed.includes("Do not ask the user to open Blender"),
    "the thing that made the old path useless is named so it cannot come back",
  );
  // The failure the owner hit: the agent wrote "the Blender bridge is offline and the sandbox
  // blocks outside executable lookups" and built the props in GDScript instead. It had reached
  // for its own shell. The tag is not a shell command — Bhippi runs it, outside whatever
  // sandbox the vendor CLI gives the model — and the doctrine now says so in those words.
  assert.ok(flowed.includes("You do not run Blender. Bhippi does."));
  assert.ok(flowed.includes("Do not look for a `blender` executable"));
  assert.ok(
    flowed.includes("the Blender bridge is offline"),
    "the exact sentence it wrote is quoted back as the wrong mechanism",
  );
  assert.ok(
    !blender.includes("MCP"),
    "no trace of the server path is left in the doctrine",
  );
  // The two things a generated script gets wrong on a first attempt, both pre-empted.
  assert.ok(blender.includes("default cube"), "a background Blender is not an empty scene");
  assert.ok(blender.includes('r"C:'), "Windows paths need raw strings or they silently move");
});

test("a Blender script is offered whenever there is a project, not behind a switch", () => {
  // It used to hang off `mcp.blender.enabled`, which defaults to off — so on a default
  // install the model had never been told modelling was something it could do.
  const chat = read("../crates/bhippi-app/src/chat.rs");
  const at = chat.indexOf("BLENDER_SYSTEM);");
  assert.ok(at > 0, "the doctrine is still attached somewhere");
  const guard = chat.slice(chat.lastIndexOf("if ", at), at);
  assert.ok(
    !guard.includes("blender_mcp"),
    "attaching it must not depend on the MCP server being switched on",
  );
});

test("a Computer Use turn is read in exactly one panel", () => {
  // ADR-0054 retired the full-screen overlay window. The visibility promise it carried is
  // kept — every action is shown with its reason — and the panel is where.
  //
  // ADR-0057 amended one half of that: the desktop is painted again, at its edge only, by a
  // page that carries no information at all (`ui/tests/computer-glow.test.mjs` holds it to
  // that). This test's subject is unchanged — the panel is still the only place the run can
  // be *read* — so what moved here is the wording, not the promise.
  const panel = read("src/components/BhippiComputerPanel.tsx");
  assert.ok(panel.includes("computer-panel-frame"), "the frame it is looking at");
  assert.ok(panel.includes("computer-panel-status"), "what it is doing now");
  assert.ok(panel.includes("of {maxActions} steps"), "how far through the budget it is");
  assert.ok(panel.includes("computer-panel-stop"), "and how to stop it");
  assert.ok(
    panel.includes("Esc twice to stop"),
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

test("the overlay ADR-0054 deleted has not crept back", () => {
  // The edge glow ADR-0057 added is a different object: no bundle, no Rollup input, no React
  // and no content. What must never return is the *demo* — the aura, the second cursor, the
  // ticker — so this list stays exactly as ADR-0054 left it.
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
