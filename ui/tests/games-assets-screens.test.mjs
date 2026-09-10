/**
 * Games and Assets (GAD-018 and docs/16 §4.2): the joins each card and row is built from,
 * the four states every new surface owes the user (INV-034/075), and — since the Godot pane
 * was retired — that no engine surface survives outside the Studio viewport.
 *
 * The state tests read the shipping source rather than rendering it: what matters is that
 * loading, empty, error and populated all exist and are announced, and a screen that quietly
 * drops one of them is the regression this catches.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  buildGameCards,
  posterGradient,
  posterInitial,
  samePath,
} from "../src/lib/gameCards.ts";
import {
  ASSET_KIND_LABEL,
  assetFolder,
  assetKind,
  formatBytes,
  licenceFromMeta,
  metaPathFor,
} from "../src/lib/assetKinds.ts";

const project = (name, path, lastOpened = 0) => ({ name, path, last_opened_at: lastOpened });
const session = (path, updated) => ({ project_path: path, updated_at: updated });

// ── Games ──────────────────────────────────────────────────────────────────────

test("a game card joins its project with its sessions", () => {
  const cards = buildGameCards(
    [project("Feathers", "C:/Games/feathers", 100)],
    [
      session("C:/Games/feathers", "2026-09-01T10:00:00Z"),
      session("C:/Games/feathers", "2026-09-02T12:00:00Z"),
      session("C:/Games/other", "2026-09-03T12:00:00Z"),
    ],
  );
  assert.equal(cards.length, 1);
  assert.equal(cards[0].sessionCount, 2, "another game's sessions are not counted here");
  assert.equal(cards[0].lastActivity, "2026-09-02T12:00:00Z", "the newest session wins");
});

test("paths are matched the way the rest of the shell matches them", () => {
  assert.ok(samePath("C:\\Games\\Feathers", "c:/games/feathers/"));
  assert.ok(samePath("//?/C:/Games/x", "C:/Games/x"));
  assert.equal(samePath("C:/Games/a", "C:/Games/b"), false);
  assert.equal(samePath(null, null), false, "two missing paths are not the same game");
});

test("a game with no sessions still reports when it was last opened", () => {
  const [card] = buildGameCards([project("Fresh", "C:/Games/fresh", 1_700_000_000)], []);
  assert.equal(card.sessionCount, 0);
  assert.equal(card.lastActivity, null, "the card says 'no sessions yet' rather than inventing one");
  assert.equal(card.lastActivityAt, 1_700_000_000);
});

test("cards are ordered by real recency, newest first", () => {
  const cards = buildGameCards(
    [
      project("Old", "C:/Games/old", 10),
      project("New", "C:/Games/new", 20),
      project("Busy", "C:/Games/busy", 5),
    ],
    [session("C:/Games/busy", "2026-09-02T12:00:00Z")],
  );
  assert.deepEqual(cards.map((card) => card.name), ["Busy", "New", "Old"]);
});

test("the poster fallback is stable for a game and different between games", () => {
  assert.equal(posterGradient("Feathers"), posterGradient("Feathers"));
  assert.notEqual(posterGradient("Feathers"), posterGradient("Dungeon"));
  assert.match(posterGradient("Feathers"), /^linear-gradient\(135deg, hsl\(\d+ /);
  // Translucent on purpose: the tile paints it over a token surface, so the placeholder
  // is a tint of the current theme rather than a dark rectangle in a light palette.
  assert.match(posterGradient("Feathers"), /\/ 0\.\d+\)/);
});

test("a poster-less tile always has a letter to show, even for a nameless game", () => {
  assert.equal(posterInitial("Feathers"), "F");
  assert.equal(posterInitial("  dungeon"), "D");
  assert.equal(posterInitial(""), "·", "a hole where the art should be is worse than a dot");
  assert.equal(posterInitial("   "), "·");
});

// ── Assets ─────────────────────────────────────────────────────────────────────

test("kind comes from the extension, not from the folder it sits in", () => {
  assert.equal(assetKind("assets/models/hero.glb"), "model");
  assert.equal(assetKind("assets/models/hero_albedo.png"), "texture");
  assert.equal(assetKind("assets/audio/jump.wav"), "audio");
  assert.equal(assetKind("assets/ui/Inter.ttf"), "ui");
  assert.equal(assetKind("assets/notes.txt"), "other");
  assert.equal(assetKind("assets/LICENSE"), "other", "an extensionless file is not a kind");
  assert.equal(assetKind("assets/models/HERO.GLB"), "model", "extensions are case-insensitive");
  for (const kind of ["model", "texture", "audio", "ui", "other"]) {
    assert.equal(typeof ASSET_KIND_LABEL[kind], "string");
  }
});

test("rows group by their own folder", () => {
  assert.equal(assetFolder("assets/models/hero.glb"), "assets/models");
  assert.equal(assetFolder("assets/readme.md"), "assets");
});

test("a licence is read from the sibling meta file, and its absence is loud", () => {
  assert.equal(metaPathFor("assets/models/hero.glb"), "assets/models/hero.glb.meta.json");
  assert.equal(licenceFromMeta('{"license":"CC0-1.0"}'), "CC0-1.0");
  assert.equal(licenceFromMeta('{"license":"  MIT  "}'), "MIT");
  // Everything that is not a licence reads as unknown, never as blank.
  assert.equal(licenceFromMeta('{"licence":"CC0"}'), null, "the field is `license`");
  assert.equal(licenceFromMeta('{"license":""}'), null);
  assert.equal(licenceFromMeta("{ not json"), null);
  assert.equal(licenceFromMeta(null), null);
  assert.equal(licenceFromMeta(undefined), null);
});

test("sizes are readable and a missing size is not reported as zero bytes", () => {
  assert.equal(formatBytes(0), "0 B");
  assert.equal(formatBytes(2048), "2 KB");
  assert.equal(formatBytes(5 * 1024 * 1024), "5.0 MB");
  assert.equal(formatBytes(-1), "—");
  assert.equal(formatBytes(Number.NaN), "—");
});

// ── Four states ────────────────────────────────────────────────────────────────

const source = (file) =>
  readFile(new URL(`../src/screens/${file}`, import.meta.url), "utf8");

test("Games renders all four states and announces the failing one", async () => {
  const text = await source("Games.tsx");
  assert.match(text, /aria-busy="true"/, "loading");
  assert.match(text, /game-card skeleton/, "loading skeleton");
  assert.match(text, /No games yet/, "empty");
  assert.match(text, /No games match this search/, "empty under a filter");
  assert.match(text, /role="alert"/, "error is announced, not just coloured");
  assert.match(text, /onClick=\{onRetry\}/, "an error offers a way out");
  assert.match(text, /games-grid/, "populated");
});

test("Assets renders all four states and announces the failing one", async () => {
  const text = await source("Assets.tsx");
  assert.match(text, /aria-busy="true"/, "loading");
  assert.match(text, /No assets yet/, "empty");
  assert.match(text, /No assets match this filter/, "empty under a filter");
  assert.match(text, /role="alert"/, "error is announced");
  assert.match(text, /Retry/, "an error offers a way out");
  assert.match(text, /<table className="table">/, "populated");
});

test("every new surface is keyboard reachable and named", async () => {
  for (const file of ["Games.tsx", "Assets.tsx"]) {
    const text = await source(file);
    // Buttons and inputs, never a click handler on a bare div.
    assert.equal(
      /<div[^>]*\sonClick=/.test(text),
      false,
      `${file} routes interaction through real controls`,
    );
    assert.match(text, /aria-label=/, `${file} names its controls`);
    assert.match(text, /className="screen-title"/, `${file} reuses the shell's heading`);
  }
});

// ── the card's poster and its actions ──────────────────────────────────────────

test("the poster on a card is the frame Rust read, never one the page composed", async () => {
  const text = await source("Games.tsx");
  // Rust returns a ready `data:` URL — the asset protocol is off, and a webview that
  // guessed the media type from a file name would be deciding what the bytes are.
  assert.match(text, /poster_data_url/, "the card renders Rust's data URL");
  assert.doesNotMatch(text, /data:image\//, "the page never builds a data URL of its own");
  assert.doesNotMatch(text, /content_base64/, "the poster no longer comes through readFile");
  assert.doesNotMatch(text, /\.bhippi\//, "the page composes no path inside a project");
  assert.match(text, /api\.gameCardInfo\(/, "the card's detail is one Rust reply");
});

test("Snapshot takes a real frame and then re-reads the card", async () => {
  const text = await source("Games.tsx");
  assert.match(text, /api\.godotCapturePoster\(project\.path\)/);
  // Capture then re-read: a tile still showing the old frame after a snapshot is a lie
  // about what the button just did.
  const order = text.indexOf("api.godotCapturePoster");
  const reread = text.indexOf("api.gameCardInfo(project.path)");
  assert.ok(order > 0 && reread > order, "the card is re-read after the capture");
});

test("Play and Snapshot are blocked with Rust's reason, never with the page's guess", async () => {
  const text = await source("Games.tsx");
  assert.match(text, /api\.godotEmbedPlay\(project\.path\)/);
  assert.match(text, /blocked_reason/, "the tooltip is the reason Rust gave");
  assert.match(text, /title=\{blocked \?\? /, "a disabled action still says why");
  // `project.godot` presence is Rust's answer too — the page has no filesystem.
  assert.match(text, /is_godot_project/);
  assert.doesNotMatch(text, /project\.godot/, "the page never sniffs for the project file");
});

test("the grid is a token-only surface with a real 16:9 poster slot", async () => {
  const css = await readFile(new URL("../src/styles/screens.css", import.meta.url), "utf8");
  const start = css.indexOf("== The Games grid ==");
  assert.ok(start > 0, "the Games grid has its own section");
  const block = css.slice(start);
  assert.match(block, /\.games-grid\s*\{/);
  assert.match(block, /grid-template-columns:\s*repeat\(auto-fill, minmax\(240px, 1fr\)\)/);
  assert.match(block, /aspect-ratio:\s*16 \/ 9/, "a game frame's own shape");
  assert.match(block, /border-radius:\s*10px/);
  assert.match(block, /box-shadow:\s*var\(--lift-1\)/, "hover lifts no harder than the system");
  assert.doesNotMatch(block, /#[0-9a-fA-F]{3,8}\b/, "colours are tokens, in both palettes");
});

// ── the retired engine surface ─────────────────────────────────────────────────

const uiSrc = fileURLToPath(new URL("../src/", import.meta.url));

function walk(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(full));
    else if (/\.(ts|tsx)$/.test(entry.name)) out.push(full);
  }
  return out;
}

test("the old Godot pane is gone from the tree, not merely unrouted", () => {
  assert.equal(fs.existsSync(path.join(uiSrc, "godot")), false, "src/godot is deleted");
  const offenders = walk(uiSrc).filter((file) =>
    /GodotPane|GodotStage|GodotOutliner|GodotDrawer|GodotErrorBoundary|godotTree|godotValue/.test(
      fs.readFileSync(file, "utf8"),
    ),
  );
  assert.deepEqual(offenders, [], "nothing imports or names the retired pane");
});

test('"engine" is no longer a workbench mode anywhere in the page', () => {
  const patterns = [
    /workbenchMode\s*===\s*"engine"/,
    /setWorkbenchMode\("engine"\)/,
    /mode\s*===\s*"engine"/,
    /id:\s*"engine"/,
    /"editor"\s*\|\s*"browser"\s*\|\s*"engine"/,
  ];
  const offenders = walk(uiSrc).filter((file) => {
    const text = fs.readFileSync(file, "utf8");
    return patterns.some((pattern) => pattern.test(text));
  });
  assert.deepEqual(offenders, []);
});

test("the title bar offers two panels, and neither of them is the Engine", async () => {
  const text = await readFile(
    new URL("../src/chrome/TitleBarCenterControls.tsx", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(text, /IconEngine/, "the Engine toggle's glyph is gone with it");
  assert.doesNotMatch(text, /Game Engine/);
  assert.match(text, /Code Editor/);
  assert.match(text, /Web Browser/);
});

test("WORKBENCH_ORDER is exactly the editor and the browser", async () => {
  // Read rather than imported: the module is JSX, which the runner cannot load.
  const text = await readFile(new URL("../src/workbench/ModeSwitch.tsx", import.meta.url), "utf8");
  assert.match(
    text,
    /export const WORKBENCH_ORDER: WorkbenchMode\[\] = \["editor", "browser"\];/,
  );
  assert.match(text, /export type WorkbenchMode = "editor" \| "browser";/);
  assert.doesNotMatch(text, /IconEngine/);
});

test("a persisted engine mode falls back to the editor rather than to nothing", async () => {
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /saved === "browser" \? saved : "editor"/);
  assert.doesNotMatch(app, /saved === "engine"/);
  assert.doesNotMatch(app, /event\.key === "3"/, "the Engine shortcut is gone with the pane");
});

test("Playtest and Watch play survived the pane, in the Studio toolbar", async () => {
  const studio = await source("StudioScreen.tsx");
  assert.match(studio, /api\.godotPlaytest\(projectPath, null, null\)/);
  assert.match(studio, /api\.godotVisualPlaytest\(projectPath, null\)/);
  assert.match(studio, />\s*Playtest\s*</);
  assert.match(studio, />\s*Watch play\s*</);
  // The toolbar shows one line of Rust's report and computes none of it.
  assert.match(studio, /result\.report\.frames/);
  assert.match(studio, /result\.stopped_reason/);
});
