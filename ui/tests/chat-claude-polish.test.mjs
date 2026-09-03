/**
 * SPA-501…504: the owner's fourth round of 2026-09-03.
 *
 * "Add the logo in the middle of the empty space when in a project; improve the animation
 * in the chat while it is working; make the chat look and feel like Claude; let me drag
 * and drop images into the chat or paste them with Ctrl+V." Source pins for each, plus the
 * exit fix the overlay window made necessary.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const read = (rel) => fs.readFileSync(path.join(here, "..", "src", rel), "utf8");
const readCrate = (rel) =>
  fs.readFileSync(path.join(here, "..", "..", "crates", "bhippi-app", "src", rel), "utf8");

test("SPA-501: the mark sits in the middle of both empty spaces", () => {
  const welcome = read("components/ChatWelcome.tsx");
  assert.match(welcome, /import logo from "\.\.\/assets\/logo\.png";/);
  assert.match(welcome, /<img src=\{logo\} className="chat-welcome-logo" alt="" draggable=\{false\} \/>/);

  const viewport = read("studio/GodotViewport.tsx");
  assert.match(viewport, /import logo from "\.\.\/assets\/logo\.png";/);
  assert.match(viewport, /className="godot-viewport-logo"/);
  // The logo lives inside the empty state only: a live Godot child cannot be painted over.
  const emptyAt = viewport.indexOf('className="godot-viewport-empty"');
  const logoAt = viewport.indexOf('className="godot-viewport-logo"');
  assert.ok(emptyAt > 0 && logoAt > emptyAt, "the logo is inside .godot-viewport-empty");

  const chat = read("styles/chat.css");
  assert.match(chat, /\.chat-welcome-logo \{/);
  const studio = read("styles/studio.css");
  assert.match(studio, /\.godot-viewport-logo \{/);
  assert.ok(fs.existsSync(path.join(here, "..", "src", "assets", "logo.png")), "the asset exists");
});

test("SPA-502: the working state is drawn, not typed as three dots", () => {
  const chat = read("screens/Chat.tsx");
  assert.match(chat, /import \{ PhaseGlyph, PhaseIndicator \} from "\.\.\/components\/AgentPhase";/);
  assert.ok(!chat.includes(">Working...<"), "no literal 'Working...'");
  assert.ok(!chat.includes(">Thinking...<"), "no literal 'Thinking...'");
  assert.match(chat, /<PhaseGlyph phase="thinking" size=\{12\} \/>\s*<span className="turn-work-working-label work-shimmer">Working<\/span>/);
  assert.match(chat, /<span className="thinking-label work-shimmer">Thinking<\/span>/);

  const css = read("styles/chat.css");
  assert.match(css, /\.work-shimmer \{[\s\S]*?animation: m-shimmer/);
  assert.match(css, /\.turn\.assistant:has\(\.turn-work-item\.working\)::before/);

  const activity = read("styles/activity.css");
  assert.match(activity, /\.activity-live-trigger\.is-streaming::after \{[\s\S]*?animation: live-sweep/);
  // Motion is never the only signal, and it is off under reduced motion.
  assert.match(css, /prefers-reduced-motion: reduce\)\s*\{\s*\.work-shimmer \{\s*animation: none;/);
});

test("SPA-503: images arrive by drop and by Ctrl+V, through Rust", () => {
  const chat = read("screens/Chat.tsx");
  assert.match(chat, /import \{ getCurrentWebview \} from "@tauri-apps\/api\/webview";/);
  assert.match(chat, /getCurrentWebview\(\)\s*\.onDragDropEvent\(/);
  // A drop lands only in the chat the pointer is over, so side-by-side windows stay apart.
  assert.match(chat, /containsPhysicalPoint\(chatRootRef\.current, payload\.position\)/);
  assert.match(chat, /if \(inside && payload\.paths\.length > 0\) void attachPaths\(payload\.paths\);/);
  assert.match(chat, /onPaste=\{onComposerPaste\}/);
  assert.match(chat, /api\.savePastedImage\(await fileToBase64\(file\), file\.type\)/);
  // Text pastes stay with the textarea: only image items are intercepted.
  assert.match(chat, /item\.kind === "file" && item\.type\.toLowerCase\(\)\.startsWith\("image\/"\)/);
  assert.match(chat, /\$\{dropActive \? " drop-active" : ""\}/);
  assert.match(chat, /className="composer-drop-hint"/);

  const api = read("lib/api.ts");
  assert.match(api, /savePastedImage: \(dataBase64: string, mediaType: string\) =>/);
  const css = read("styles/chat.css");
  assert.match(css, /\.chat\.drop-active \.composer-shell \{/);
  assert.match(css, /\.composer-drop-hint \{/);

  const commands = readCrate("commands.rs");
  assert.match(commands, /pub async fn save_pasted_image\(/);
  assert.match(commands, /pub fn save_pasted_image_to\(/);
  // Only images, and only up to a ceiling; a paste is not a way to smuggle a file in.
  assert.match(commands, /fn pasted_extension\(media_type: &str\) -> Option<&'static str>/);
  assert.match(commands, /pub const PASTED_IMAGE_MAX_BYTES: usize/);
  const lib = readCrate("lib.rs");
  assert.match(lib, /save_pasted_image,/);
});

test("SPA-504: the transcript reads like Claude — a soft user card, plain agent prose", () => {
  const css = read("styles/chat.css");
  const tail = css.slice(css.indexOf("SPA-501…504"));
  assert.ok(tail.length > 0, "the polish block is appended");
  assert.match(tail, /\.user-bubble-card \{[\s\S]*?border-radius: 18px;[\s\S]*?box-shadow: none;/);
  assert.match(tail, /\.assistant-turn-body \{[\s\S]*?font-size: 15px;[\s\S]*?line-height: 1\.7;/);
  // Tool work is one bordered block, the way Claude folds its tool calls.
  assert.match(tail, /\.turn-work-tree \{[\s\S]*?border: 1px solid var\(--line\);[\s\S]*?border-radius: 12px;/);
});

test("closing the main window exits the app even though the overlay window is alive", () => {
  const lib = readCrate("lib.rs");
  assert.match(
    lib,
    /tauri::RunEvent::WindowEvent \{\s*label,\s*event: tauri::WindowEvent::Destroyed,\s*\.\.\s*\} = &event/,
  );
  assert.match(lib, /if label == "main" \{\s*app_handle\.exit\(0\);/);
});
