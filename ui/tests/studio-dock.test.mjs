/**
 * The Studio bottom dock (StudioBottomDock.tsx) shows the open project and nothing else.
 *
 * The owner's screenshot of a freshly scaffolded game showed eight "assets" — `.gitignore`,
 * `project.godot`, `main.gd` — every one of them stamped CC0 or MIT by a UI that had
 * guessed, next to a Versions tab that read `v0.3.1` in a project with no versions. These
 * tests read the shipping source so those defaults cannot come back: a mock list is easy to
 * reintroduce and impossible to notice in a screenshot until it is wrong in front of a user.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const dockPath = path.join(here, "..", "src", "studio", "StudioBottomDock.tsx");
const dock = fs.readFileSync(dockPath, "utf8");
const apiSource = fs.readFileSync(path.join(here, "..", "src", "lib", "api.ts"), "utf8");
const ipcSource = fs.readFileSync(path.join(here, "..", "src", "lib", "ipc.ts"), "utf8");

test("the dock carries no seeded asset or version lists", () => {
  assert.ok(!dock.includes("DEFAULT_ASSETS"), "DEFAULT_ASSETS is gone");
  assert.ok(!dock.includes("DEFAULT_VERSIONS"), "DEFAULT_VERSIONS is gone");
  // The invented library that shipped with them.
  for (const invented of [
    "player_jelly",
    "grass_moss",
    "coin_pickup",
    "waterfall_loop",
    "cliff_rock",
    "moving_platform",
    "Bhippi AI",
    "System Scaffold",
  ]) {
    assert.ok(!dock.includes(invented), `${invented} is not a real project asset`);
  }
});

test("no licence is ever assumed: CC0 and MIT are not literals in the dock", () => {
  assert.ok(!dock.includes('"CC0"'), "a licence is read from a sidecar, never defaulted");
  assert.ok(!dock.includes('"MIT"'), "a script's licence is not inferred from its extension");
  assert.ok(!/licences\s*=\s*\[/.test(dock), "the licence cycler is gone");
  // An asset with no sidecar reads `unknown`, in the warning style.
  assert.ok(dock.includes('"unknown"'), "unknown is what a missing licence says");
});

test("the Versions tab label carries no hard-coded version number", () => {
  assert.ok(!dock.includes("v0.3.1"), "no invented current version");
  assert.ok(!/v0\.\d+\.\d+/.test(dock), "no invented version numbers at all");
  assert.ok(!dock.includes("commitHash"), "no fabricated commit hashes");
});

test("Assets are read through the Rust command, rooted at assets/", () => {
  assert.ok(dock.includes("api.listProjectAssets(projectPath)"), "the dock calls Rust");
  assert.ok(
    !dock.includes("api.workspaceDir"),
    "the dock no longer walks the whole project itself",
  );
  // Rust roots the walk at `assets/` and classifies there.
  assert.ok(
    apiSource.includes("listProjectAssets: (project: string) => ok(commands.listProjectAssets(project))"),
    "api.ts forwards to the generated binding",
  );
  assert.ok(
    ipcSource.includes('__TAURI_INVOKE("list_project_assets"'),
    "the binding is generated, not hand written",
  );
  // Uploading an asset actually copies it into the project.
  assert.ok(dock.includes("api.importWorkspaceFile("), "Upload imports a real file");
});

test("the empty state names what will fill it", () => {
  assert.ok(
    dock.includes("No assets yet — Bhippi adds them as it builds, or upload one."),
    "the Assets empty state is the agreed sentence",
  );
  assert.ok(dock.includes("No versions yet"), "the Versions empty state");
  assert.ok(
    dock.includes("No engine output yet"),
    "the Console empty state, rather than invented log lines",
  );
  assert.ok(dock.includes("No GDScript files yet"), "the Code empty state");
});

test("Versions come from godot_list_versions and the create/revert commands", () => {
  assert.ok(dock.includes("api.godotListVersions(projectPath)"), "the list is live");
  assert.ok(dock.includes("api.godotCreateVersion("), "Create version calls Rust");
  assert.ok(dock.includes("api.godotRevertTo("), "Revert calls Rust");
  assert.ok(!dock.includes("alert("), "Revert is not a fake alert any more");
  for (const binding of ["godotListVersions", "godotCreateVersion", "godotRevertTo"]) {
    assert.ok(apiSource.includes(`${binding}:`), `api.ts exposes ${binding}`);
  }
});

test("Library is the engine capability registry, not a hard-coded list", () => {
  assert.ok(dock.includes("api.listCapabilities()"), "the Library tab calls Rust");
  assert.ok(
    ipcSource.includes('__TAURI_INVOKE("list_capabilities"'),
    "list_capabilities is a generated binding",
  );
  for (const invented of [
    "3D Platformer Hero",
    "Kinematic Moving Platform",
    "Collectible Coin",
    "Directional Sun Rig",
    "Floating Island Chunk",
  ]) {
    assert.ok(!dock.includes(invented), `${invented} was an invented capability`);
  }
});

test("Console is project-scoped: seeded from godot_output and following the event", () => {
  assert.ok(dock.includes("api.godotOutput(projectPath)"), "seeded from the buffer");
  assert.ok(dock.includes("events.godotOutput"), "and follows the live event");
  assert.ok(
    dock.includes("samePath(payload.project, projectPath)"),
    "lines from another project are ignored",
  );
  assert.ok(!dock.includes("Godot 4.7.1 initialized"), "no scripted console transcript");
});

test("Code lists the project's own scripts", () => {
  assert.ok(dock.includes("api.listProjectScripts(projectPath)"), "the file list is live");
  assert.ok(!dock.includes("extends CharacterBody3D"), "no built-in sample script");
  assert.ok(dock.includes(".readFile(openScript)"), "the body is the file on disk");
});

test("every panel owes loading, empty and error, and re-fetches on project change", () => {
  for (const state of ['state: "loading"', 'state: "error"', 'state: "ready"']) {
    assert.ok(dock.includes(state), `the dock models ${state}`);
  }
  assert.ok(dock.includes('role="alert"'), "an error is announced, not just coloured");
  assert.ok(dock.includes('aria-busy="true"'), "a loading panel is announced");
  // The reset effect keys on projectPath, so nothing from the old game survives.
  const resetEffect = dock.slice(
    dock.indexOf("setAssets(IDLE);\n    setScripts(IDLE);"),
    dock.indexOf("// A tab loads the first time it is opened"),
  );
  assert.ok(resetEffect.includes("}, [projectPath]);"), "the reset keys on projectPath");
  for (const loader of ["loadAssets", "loadScripts", "loadVersions"]) {
    assert.ok(
      new RegExp(`${loader} = useCallback\\([\\s\\S]*?\\}, \\[projectPath\\]\\);`).test(dock),
      `${loader} re-binds when the project changes`,
    );
  }
});

test("the drawer stays in flow, never painted over the viewport (ADR-0045)", () => {
  const css = fs.readFileSync(path.join(here, "..", "src", "styles", "studio.css"), "utf8");
  const drawer = css.slice(css.indexOf(".studio-drawer {"), css.indexOf("@keyframes drawer-slide-up"));
  assert.ok(drawer.includes("position: relative;"), "the drawer is in flow");
  assert.ok(!drawer.includes("position: absolute"), "and never absolutely positioned");
  // The new dock styles use tokens, not raw colours.
  const dockStyles = css.slice(css.indexOf(".studio-dock-empty {"), css.indexOf(".studio-dock-count {"));
  assert.ok(dockStyles.length > 0, "the dock panel styles exist");
  assert.ok(!/#[0-9a-fA-F]{3,8}\b/.test(dockStyles), "no raw hex colours in the new styles");
});
