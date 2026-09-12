/**
 * The preview is the game and a Play button, and nothing else while it runs.
 *
 * The owner, over a screenshot of three stacked control bars under the viewport: *remove all
 * these, just add a play button on top ... remove everything else to make the preview looks
 * clean and good.* Nothing is deleted — Export, Console, Inspector and the rest come back the
 * moment the run stops — they are simply not on screen while there is a game to watch.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const root = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, root), "utf8");

const studio = read("src/screens/StudioScreen.tsx");
const css = read("src/styles/studio.css");

test("what chrome remains sits above the viewport, never over it", () => {
  // ADR-0045: the Godot surface is a native child window re-parented into Bhippi's. A native
  // window paints on top of everything the page draws, so an overlaid button would simply be
  // invisible the moment the viewport had anything in it.
  const topbar = studio.indexOf("studio-viewport-topbar");
  const card = studio.indexOf("studio-viewport-card");
  assert.ok(topbar > 0 && card > 0, "both exist");
  assert.ok(topbar < card, "the bar is rendered before the viewport, so it sits above it");
  assert.doesNotMatch(
    css.slice(css.indexOf(".studio-viewport-topbar"), css.indexOf(".studio-viewport-play")),
    /position:\s*absolute|z-index/,
    "nothing about this bar tries to float over the native surface",
  );
});

test("Play lives in the Godot toolbar, not in a strip above it", () => {
  // ADR-0064, the owner's words: "in mid add a play game button, currently the play button is
  // on top remove that and fix it". It is a button inside the editor because the viewport is
  // a native child window — nothing Bhippi draws over it is visible.
  const plugin = fs.readFileSync(
    new URL("../crates/bhippi-engine/src/godot/templates/studio_plugin.gd", root),
    "utf8",
  );
  assert.match(plugin, /add_control_to_container\(CONTAINER_TOOLBAR, _toolbar\)/);
  // An icon, not a word: `MainPlay` is the glyph Godot's own run bar uses, so it is the right
  // weight for this bar at every DPI. A theme without it gets the word back rather than a
  // nameless button.
  assert.match(plugin, /_editor_icon\("MainPlay"\)/);
  assert.match(plugin, /play\.text = "Play"/, "the fallback keeps the button legible");
  assert.doesNotMatch(plugin, /play\.text = "Play game"/, "no text on the icon button");
});

test("the toolbar wears the editor's own clothes and sits in the centred row", () => {
  const plugin = fs.readFileSync(
    new URL("../crates/bhippi-engine/src/godot/templates/studio_plugin.gd", root),
    "utf8",
  );
  // `MainScreenButton` is the variation 2D/3D/Script/Game/Asset Store actually use — read off
  // them in a running editor. Not an imitation of their look: their look.
  assert.match(plugin, /button\.theme_type_variation = "MainScreenButton"/);
  assert.match(plugin, /_style_like_main_screen\(play\)/);
  assert.match(plugin, /_style_like_main_screen\(_preview_button\)/);
  // Play is an action, so it does not latch; Preview is a state, so it does.
  assert.match(plugin, /_preview_button\.toggle_mode = true/);
  assert.doesNotMatch(plugin, /play\.toggle_mode = true/);
  // One word, like every other button on the row.
  assert.match(plugin, /_preview_button\.text = "Preview"/);
  assert.doesNotMatch(plugin, /"Preview: on"/, "the pressed state says which, not the label");
  // Seated next to the switcher: CONTAINER_TOOLBAR appends to the end of the title bar, which
  // is the far side of the run bar from the row these belong to.
  assert.match(plugin, /bar\.move_child\(_toolbar, switcher\.get_index\(\) \+ 1\)/);
});

test("the springs that centre the row are kept, not hidden", () => {
  // The first cut hid every title-bar child but the switcher — including the two expanding
  // boxes either side of it, which are the only reason it sits in the middle. An empty spring
  // still pushes, so they are kept and their contents hidden instead.
  const plugin = fs.readFileSync(
    new URL("../crates/bhippi-engine/src/godot/templates/studio_plugin.gd", root),
    "utf8",
  );
  assert.match(plugin, /size_flags_horizontal & Control\.SIZE_EXPAND/);
  assert.match(
    plugin,
    /SIZE_EXPAND\) != 0:[\s\S]{0,220}for inner in control\.get_children\(\)/,
    "an expanding child is emptied, never hidden",
  );
  // And Bhippi's own strip no longer offers one.
  assert.doesNotMatch(studio, /gameRunning \? "Stop" : "Play"/, "no Play in the page");
});

test("Stop stays in the page, because a running game covers the button that started it", () => {
  // The one thing that could not move with Play: the embedded game sits *over* the editor, so
  // the toolbar holding Play is underneath it the moment it starts.
  assert.match(studio, /\{gameRunning \|\| notice \? \(/, "the strip is absent when idle");
  assert.match(studio, /className="studio-viewport-play running"/);
  assert.match(studio, /title="Stop the game"/);
});

test("the editor's Play runs the same launch the studio always used", () => {
  // Not `EditorInterface.play_main_scene()`: that opens a window Bhippi never launched, and
  // the viewport can only re-parent a window it did launch — so the game would float over the
  // app. The addon asks; Bhippi runs it.
  const plugin = fs.readFileSync(
    new URL("../crates/bhippi-engine/src/godot/templates/studio_plugin.gd", root),
    "utf8",
  );
  // An actual call, not the comment that explains why there isn't one.
  assert.doesNotMatch(
    plugin,
    /EditorInterface\.play_main_scene/,
    "the addon never runs the game itself",
  );
  assert.match(plugin, /PLAY_REQUEST_REL/);
  assert.match(plugin, /_save_scenes\(\)[\s\S]{0,200}PLAY_REQUEST_REL/, "and flushes first");
  assert.match(studio, /events\.godotPlayRequested\.listen/, "the page hears the request");
  assert.match(studio, /api\.godotEmbedPlay\(projectPath\)/, "and runs the ordinary launch");
});

test("preview mode keeps the viewport and the screen switcher, and nothing else", () => {
  const plugin = fs.readFileSync(
    new URL("../crates/bhippi-engine/src/godot/templates/studio_plugin.gd", root),
    "utf8",
  );
  // Matched by editor CLASS, not by button text. The first cut matched text — "Scene",
  // "Output" — and hid nothing at all, because Godot 4 builds none of those as buttons: the
  // menus are one `MenuBar` node carrying five titles. Every name below was read out of a
  // real 4.7.1 editor's own control tree, not guessed.
  assert.match(plugin, /const HIDE_CLASSES := \["EditorSceneTabs", "EditorBottomPanel"\]/);
  assert.match(plugin, /_find_by_class/);
  assert.doesNotMatch(
    plugin,
    /child is Button and str\(child\.text\) in/,
    "matching a button's label is the thing that did not work",
  );
  // The title bar is kept by subtraction: the switcher survives, everything else there goes,
  // so a widget a future Godot adds to that bar is hidden by default.
  assert.match(plugin, /const TITLE_BAR_KEEP := "EditorMainScreenButtons"/);
  assert.match(plugin, /child == _toolbar or child\.is_ancestor_of\(_toolbar\)/,
    "and the plugin never hides its own buttons");
  // Bhippi's other addon puts a search strip under the viewport; in preview it is one more row.
  assert.match(plugin, /const SKETCHFAB_NODE := "BhippiSketchfab"/);
  assert.match(plugin, /set_distraction_free_mode\(_preview_on\)/);
  // A miss is reported rather than silently doing nothing — the failure mode of the first cut.
  assert.match(plugin, /could not find %s; leaving it alone/);
});

test("switching preview off puts the editor back, and so does unloading the plugin", () => {
  // A plugin that is turned off and leaves the menus hidden has taken the project hostage.
  const plugin = fs.readFileSync(
    new URL("../crates/bhippi-engine/src/godot/templates/studio_plugin.gd", root),
    "utf8",
  );
  assert.match(plugin, /control\.visible = not _preview_on/);
  const exit = plugin.slice(plugin.indexOf("func _exit_tree"), plugin.indexOf("func _build_toolbar"));
  assert.match(exit, /_preview_on = false/, "the restore runs on unload");
  assert.match(exit, /remove_control_from_container\(CONTAINER_TOOLBAR, _toolbar\)/);
});

test("Play is the only chrome there is", () => {
  // Not hidden while the game runs — gone. "REMOVE THESE PANELS AND BUTTON REMOVE THEM AND
  // MAKE THE PREVIEW CLEAN", over a screenshot of three stacked bars under the viewport.
  assert.doesNotMatch(studio, /className="studio-engine-toolbar"/);
  assert.doesNotMatch(studio, /<StudioBottomDock/);
  assert.doesNotMatch(studio, /studio-dock-wrap/);
  for (const gone of ["Playtest", "Watch play", "Close workspace", "Preview", "Export", "Undo"]) {
    assert.ok(!studio.includes(`> ${gone}`), `${gone} is still rendered under the viewport`);
  }
  // And the one control that is left is never itself hidden. Matched as a JSX binding so the
  // `aria-hidden` on the play glyph is not mistaken for one.
  const bar = studio.slice(studio.indexOf("studio-viewport-topbar"), studio.indexOf("studio-viewport-card"));
  assert.doesNotMatch(bar, /\shidden=\{/, "the transport is never hidden");
});

test("a Play that fails still says so", () => {
  // The status line went with the bar — "Ready", "Workspace open" said nothing a person
  // could not see by looking. The error half stays: a Play that silently does nothing is the
  // one thing worse than a busy bar.
  assert.match(studio, /\{notice \? \(/);
  assert.match(studio, /className="studio-viewport-notice"/);
  assert.match(studio, /setNotice\(`Could not \$\{label\}/, "act still reports failures");
  assert.match(css, /\.studio-viewport-notice/);
  // And it says nothing at all when nothing has gone wrong.
  assert.doesNotMatch(studio, /\{notice \?\? status\}/, "no idle status line");
});

test("the Stop button is reachable by keyboard", () => {
  assert.match(css, /\.studio-viewport-play:focus-visible/);
  assert.match(css, /\.studio-viewport-play\.running/, "and reads as the stop it is");
});
