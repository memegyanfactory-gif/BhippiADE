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
  // The owner's call of 2026-09-09 emptied everything around it: the empty chat is the mark
  // alone — small and nearly transparent — with no title and no starter pills beside it.
  const welcome = read("components/ChatWelcome.tsx");
  assert.match(welcome, /import logo from "\.\.\/assets\/logo\.png";/);
  assert.match(welcome, /<img src=\{logo\} className="chat-welcome-logo" alt="" draggable=\{false\} \/>/);
  assert.ok(!welcome.includes("chat-welcome-title"), "no title beside the mark");
  assert.ok(!welcome.includes("chat-welcome-minimal-btn"), "no starter pills beside the mark");

  const viewport = read("studio/GodotViewport.tsx");
  assert.match(viewport, /import logo from "\.\.\/assets\/logo\.png";/);
  assert.match(viewport, /className="godot-viewport-logo"/);
  // The logo lives inside the empty state only: a live Godot child cannot be painted over.
  const emptyAt = viewport.indexOf('className="godot-viewport-empty"');
  const logoAt = viewport.indexOf('className="godot-viewport-logo"');
  assert.ok(emptyAt > 0 && logoAt > emptyAt, "the logo is inside .godot-viewport-empty");

  const chat = read("styles/chat.css");
  assert.match(chat, /\.chat-welcome-logo \{[^}]*opacity: 0\.07;/);
  assert.ok(!chat.includes(".chat-welcome-title"), "the dead title rule went with its markup");
  const studio = read("styles/studio.css");
  assert.match(studio, /\.godot-viewport-logo \{/);
  assert.ok(fs.existsSync(path.join(here, "..", "src", "assets", "logo.png")), "the asset exists");
});

test("SPA-502: the working state is drawn, not typed as three dots", () => {
  const chat = read("screens/Chat.tsx");
  assert.match(chat, /import \{ PhaseIndicator \} from "\.\.\/components\/AgentPhase";/);
  assert.ok(!chat.includes(">Working...<"), "no literal 'Working...'");
  assert.ok(!chat.includes(">Thinking...<"), "no literal 'Thinking...'");
  // ADR-0049 moved the running state into the activity stream. The rule it inherited is
  // the same one SPA-502 set: the working state is *drawn*, and the word beside it is the
  // engine's own sentence — "Working" survives only as the fallback for a turn that has
  // not said anything yet. A hardcoded "Working" here would be the bug this ticket removed.
  assert.match(chat, /<LivePhaseRow\s/);
  const stream = read("agent/AgentActivityStream.tsx");
  assert.match(stream, /export function LivePhaseRow\(/);
  assert.match(
    stream,
    /\{label\?\.trim\(\) \|\| "Working"\}/,
    "the row prints the engine's label, and only falls back to a literal",
  );
  // The moving thing is one mark, on the indicator alone — never the whole row (§5).
  // It is the app's own B now, and its motion and tone come from the pure module
  // rather than being picked here, so a row cannot animate in a way nothing tested.
  assert.match(stream, /motion=\{markMotionOf\(activity\)\}/);
  assert.match(stream, /tone=\{markToneOf\(activity\)\}/);
  // Pressable: a step row opens onto the real command output or the real file list.
  assert.match(stream, /aria-expanded=\{open\}/);

  const markCss = read("styles/bhippi-mark.css");
  assert.match(markCss, /\.bhippi-mark\.is-working \.bm-ring \{[\s\S]*?animation: bm-turn/);
  assert.match(markCss, /\.bhippi-mark\.is-working \.bm-letter \{[\s\S]*?animation: bm-breathe/);
  assert.match(markCss, /\.bhippi-mark\.is-seeking \.bm-sheen \{[\s\S]*?animation: bm-sweep/);
  assert.match(
    markCss,
    /prefers-reduced-motion: reduce\)[\s\S]*?animation: none;/,
    "motion is optional (§27)",
  );
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

test("SPA-504: the transcript reads like Codex — right-aligned prompt, stacked tool rows", () => {
  const css = read("styles/chat.css");
  const tail = css.slice(css.indexOf("Codex-shaped transcript"));
  assert.ok(tail.length > 0, "the Codex transcript block is appended");
  assert.match(tail, /\.turn\.user \{[\s\S]*?align-items: flex-end;/);
  assert.match(tail, /\.user-bubble-card \{[\s\S]*?border-radius: 16px 16px 4px 16px;/);
  assert.match(tail, /\.turn-work-tree \{[\s\S]*?border: none;/);
  assert.match(tail, /\.md-code-bar \{/);

  const chat = read("screens/Chat.tsx");
  assert.ok(!chat.includes("turn-work-tree-header"), "no Claude-style wrapping Worked card");
  const markdown = read("components/Markdown.tsx");
  assert.match(markdown, /class="md-code"/);
});

test("closing the main window exits the app even though the overlay window is alive", () => {
  const lib = readCrate("lib.rs");
  assert.match(
    lib,
    /tauri::RunEvent::WindowEvent \{\s*label,\s*event: tauri::WindowEvent::Destroyed,\s*\.\.\s*\} = &event/,
  );
  assert.match(lib, /if label == "main" \{\s*app_handle\.exit\(0\);/);
});
