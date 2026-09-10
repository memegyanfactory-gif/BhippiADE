/**
 * The studio says what it is doing, while it is doing it (GAD-170…172, ADR-0050).
 *
 * Three complaints, one shape: the app did the work and showed none of it. The viewport sat
 * on an empty screen while a level was built next to it, and the transcript said "Working"
 * for the whole of a forty-second engine turn. What the page owns of the fix is small and
 * worth pinning — the row is the engine's own sentence, it presses open, and it is drawn
 * once rather than twice.
 *
 * The Godot half (the addon polling the live signal) is pinned in Rust, in
 * `bhippi-engine::godot::scaffold` and `::live`, because that is where both sides are known.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const root = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(`src/${name}`, root), "utf8");

// ── the live row ───────────────────────────────────────────────────────────────

test("the running turn's last row says the engine's own sentence, not the word Working", () => {
  // ADR-0049 moved this row into the activity stream; the rule it carries is unchanged.
  const stream = read("agent/AgentActivityStream.tsx");
  assert.match(stream, /export function LivePhaseRow\(/);
  // The headline is Rust's label; the literal is only the fallback for a turn that has not
  // said anything yet (INV-051: the page chooses words, it does not compute).
  assert.match(stream, /\{label\?\.trim\(\) \|\| "Working"\}/);
  assert.doesNotMatch(
    stream,
    /agent-row-title">Working</,
    "a hardcoded Working is exactly the bug this row replaced",
  );
});

test("a step row presses open onto the real command output or the real file list", () => {
  const stream = read("agent/AgentActivityStream.tsx");
  assert.match(stream, /onClick=\{\(\) => setOpen\(\(value\) => !value\)\}/);
  assert.match(stream, /aria-expanded=\{open\}/);
  // What opening reveals is the record itself, never a restatement of the label.
  assert.match(stream, /className="agent-output"/);
  assert.match(stream, /className="agent-detail-file"/);
  // And the row that is running shows the last lines of its output while it runs (§12).
  assert.match(stream, /outputTail\(activity\.output, 4\)/);
});

test("the elapsed counter moves on its own, because a frozen one reads as a hang", () => {
  const stream = read("agent/AgentActivityStream.tsx");
  assert.match(stream, /window\.setInterval\(\(\) => tick\(\(value\) => value \+ 1\), 1000\)/);
  assert.match(stream, /window\.clearInterval\(timer\)/);
});

// ── one live line, in the right place ──────────────────────────────────────────

test("the live phase reaches the running turn, and only the running turn", () => {
  const chat = read("screens/Chat.tsx");
  assert.match(chat, /<LivePhaseRow\s/);
  assert.match(
    chat,
    /turn\.id === activeAssistant\?\.id && phase[\s\S]{0,160}\{ phase: phase\.kind, label: phase\.label, since: phase\.since \}/,
  );
  // And it stands down when a step is already running: that step's own row is the answer
  // to "what now?", so showing both would say the same thing twice.
  assert.match(chat, /const someStepIsLive = tools\.some\(\(tool\) => isLive\(statusOf\(tool\)\)\)/);
  assert.match(chat, /isStreaming && !someStepIsLive \? \(/);
});

test("the thread-level phase row stands down once the work tree carries the same sentence", () => {
  const chat = read("screens/Chat.tsx");
  assert.match(
    chat,
    /const liveRowInWorkTree =[\s\S]{0,220}activeAssistant\.tools\.some\(\(tool\) => tool\.action !== "control_computer"\)/,
  );
  assert.match(chat, /isComputerPhaseLabel\(phase\.label\)\) &&\s*!liveRowInWorkTree \? \(/);
});

test("an engine sentence that happens to say `screen` never opens the desktop panel", () => {
  const chat = read("screens/Chat.tsx");
  // GAD-172: engine narration now carries real file paths, and a bare word match on one
  // ("Setting the main scene to res://scenes/screen.tscn") would put an empty Computer Use
  // panel over a turn that never went near the desktop. The loop only ever emits `browsing`.
  assert.match(
    chat,
    /turn\.id === activeAssistant\?\.id && phase\?\.kind === "browsing"/,
  );
  assert.match(
    chat,
    /!\(phase\.kind === "browsing" && isComputerPhaseLabel\(phase\.label\)\)/,
  );
});

test("the row is styled as a row, not as a card that announces itself", () => {
  const css = fs.readFileSync(new URL("src/styles/agent-activity.css", root), "utf8");
  for (const selector of [".agent-row", ".agent-rail", ".agent-mark", ".agent-row-title"]) {
    assert.ok(css.includes(`${selector} {`), `${selector} must be styled`);
  }
  // No card, no colour per activity: the stream is rails and text (§4).
  assert.doesNotMatch(css, /\.agent-row \{[^}]*box-shadow/);
  // Only the current row is emphasised; finished rows go quiet (§6).
  assert.match(css, /\.agent-row\.live \.agent-row-title \{[\s\S]*?color: var\(--text\)/);
});
