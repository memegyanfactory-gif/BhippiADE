/**
 * The Computer Use edge glow (ADR-0057).
 *
 * The glow is a static page with no script and no state, so there is no behaviour to drive —
 * what there is to protect is a short list of promises, every one of which is a property of
 * the file itself:
 *
 *   · it carries no information, so it cannot become a second place to read the run;
 *   · it costs the app nothing — no bundle, no Rollup input, no stylesheet in the CSS graph;
 *   · it cannot take a click away from the agent it is watching;
 *   · it uses the product's own accent, which it cannot import and therefore can drift from;
 *   · it stops moving when the user has asked for less motion.
 *
 * Each of those went wrong in the overlay ADR-0054 deleted. Pinning them is how this one
 * stays the small thing it is.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const root = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, root), "utf8");

const glow = read("public/computer-glow.html");
const tokens = read("src/styles/tokens.css");
const motion = read("src/styles/motion.css");
const panel = read("src/components/BhippiComputerPanel.tsx");
const chatCss = read("src/styles/chat.css");

// ── it says nothing ─────────────────────────────────────────────────────────────

test("the glow page has no script at all", () => {
  assert.doesNotMatch(glow, /<script/i, "a page with no state cannot drift from the panel");
  // The app's CSP is `script-src 'self'`, so an inline script would not have run anyway.
  assert.doesNotMatch(glow, /onload=|onclick=|javascript:/i);
});

test("the glow says exactly one thing, and it is how to stop", () => {
  // ADR-0061 narrowed ADR-0057's "no content" to "no content *about the run*". This is the
  // one permitted exception, pinned to its wording: it fails if the line goes missing, and
  // it fails if a second one appears. A step count, an action label or a caption here would
  // make the glow a second place to read the run, which is what ADR-0054 was protecting.
  const body = glow.slice(glow.indexOf("<body"), glow.indexOf("</body>"));
  const text = body
    .replace(/<[^>]*>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  assert.equal(
    text,
    "Bhippi is in control — press Esc Esc to stop",
    "the glow body renders something other than the stop line",
  );
  assert.doesNotMatch(body, /<img|<svg|<canvas/i);
  assert.match(glow, /cursor:\s*none/, "it never draws a pointer of its own");
});

test("the stop line is a label, never a button", () => {
  // A stop that needs a successful mouse click is a stop that fails in exactly the situation
  // it exists for: something else has taken the pointer. The keyboard is the stop.
  const body = glow.slice(glow.indexOf("<body"), glow.indexOf("</body>"));
  assert.doesNotMatch(body, /<button|<a\s|role=/i);
  assert.match(glow, /pointer-events:\s*none/, "the whole surface stays click-through");
});

test("the stop line names the chord the guard actually watches for", () => {
  // `computer_guard.rs` arms Esc/Esc before anything else in a turn. If that chord ever
  // changes, a glow still promising it would be worse than one promising nothing.
  const guard = fs.readFileSync(
    new URL("../crates/bhippi-app/src/computer_guard.rs", root),
    "utf8",
  );
  assert.match(guard, /DOUBLE_ESCAPE_WINDOW/, "the stop is still a double Escape");
  assert.match(guard, /VK_ESCAPE/);
  assert.match(glow, /<kbd>Esc<\/kbd> <kbd>Esc<\/kbd>/);
});

// ── it costs nothing ────────────────────────────────────────────────────────────

test("the glow is a public asset, so it needs no Rollup input and no bundle", () => {
  // Vite copies `public/` verbatim. If this file ever moves into `src/`, it acquires a build
  // step and a place in the app's CSS graph — the two costs ADR-0054 named.
  assert.ok(fs.existsSync(new URL("public/computer-glow.html", root)));
  const viteConfig = read("vite.config.ts");
  assert.doesNotMatch(viteConfig, /computer-glow/, "no Rollup input is needed for it");
  const stylesheets = fs.readdirSync(new URL("src/styles/", root));
  assert.ok(
    !stylesheets.some((file) => file.includes("glow")),
    "the glow brings no stylesheet into the app",
  );
});

test("it is transparent to the desktop underneath it", () => {
  assert.match(glow, /background:\s*transparent/);
  assert.match(glow, /pointer-events:\s*none/, "it must never eat the agent's own clicks");
  // Every painted layer is an *inset* shadow, so nothing is drawn over the middle of the
  // screen and the user's own work is never tinted.
  const shadows = glow.match(/box-shadow:[\s\S]*?;/g) ?? [];
  assert.ok(shadows.length > 0, "the glow paints something");
  for (const shadow of shadows) {
    for (const layer of splitLayers(shadow.replace(/box-shadow:|;/g, ""))) {
      assert.match(layer, /^inset\b/, `a non-inset layer would paint inward: ${layer}`);
    }
  }
});

/** Split a `box-shadow` on its top-level commas — `rgba(r, g, b, a)` has commas of its own. */
function splitLayers(value) {
  const layers = [];
  let depth = 0;
  let current = "";
  for (const character of value) {
    if (character === "(") depth += 1;
    if (character === ")") depth -= 1;
    if (character === "," && depth === 0) {
      layers.push(current.trim());
      current = "";
      continue;
    }
    current += character;
  }
  if (current.trim()) layers.push(current.trim());
  return layers;
}

// ── it matches the product it belongs to ────────────────────────────────────────

/** `--accent: #f0a02c;` → `240, 160, 44` */
function accentChannels(css) {
  const hex = css.match(/--accent:\s*#([0-9a-f]{6})/i);
  assert.ok(hex, "tokens.css defines --accent");
  const value = hex[1];
  return [0, 2, 4].map((at) => parseInt(value.slice(at, at + 2), 16)).join(", ");
}

test("the glow's colour is the product's accent, which it cannot import", () => {
  const expected = accentChannels(tokens);
  const declared = glow.match(/--glow:\s*([^;]+);/);
  assert.ok(declared, "the glow declares its colour");
  assert.equal(
    declared[1].trim(),
    expected,
    "the glow has drifted from --accent in tokens.css",
  );
});

test("the glow breathes at the motion system's slowest loop", () => {
  const expected = motion.match(/--t-breathe:\s*(\d+)ms/);
  assert.ok(expected, "motion.css defines --t-breathe");
  const declared = glow.match(/--breathe:\s*(\d+)ms/);
  assert.ok(declared, "the glow declares its period");
  assert.equal(declared[1], expected[1], "the glow has drifted from --t-breathe");
});

test("it stops moving when the user asks for less motion, and stays visible", () => {
  const block = glow.match(/@media \(prefers-reduced-motion: reduce\)\s*\{[\s\S]*?\n {6}\}/);
  assert.ok(block, "the glow honours prefers-reduced-motion (INV-034)");
  assert.match(block[0], /animation:\s*none/);
  assert.match(
    block[0],
    /box-shadow:[\s\S]*?rgba\(var\(--glow\)/,
    "with the animation off it must still be on screen — its meaning is its presence",
  );
});

test("the beat moves the edge itself, not just its opacity", () => {
  // ADR-0061: an opacity wobble between 0.72 and 1 on a hairline is indistinguishable from a
  // static border at two feet, and this window exists to be read from across a room.
  const beat = glow.match(/@keyframes breathe\s*\{[\s\S]*?\n {6}\}/);
  assert.ok(beat, "the glow still has a breathe keyframe");
  assert.match(beat[0], /box-shadow:/, "the edge has to swell, not just dim");
  assert.match(glow, /--reach-peak:\s*\d+px/, "and it needs somewhere wider to swell to");

  const rest = Number(glow.match(/--reach:\s*(\d+)px/)[1]);
  const peak = Number(glow.match(/--reach-peak:\s*(\d+)px/)[1]);
  assert.ok(peak > rest, `the peak (${peak}px) must reach further than rest (${rest}px)`);
});

// ── the panel and the glow are one object ───────────────────────────────────────

test("the panel explains the border the user can see around their screen", () => {
  assert.match(panel, /Bhippi is in control of your screen/);
  assert.match(panel, /glowing edge around your screen/);
});

test("the emergency stop is beside the Stop button, not buried in a footnote", () => {
  assert.match(panel, /Esc twice to stop/);
  const header = panel.slice(
    panel.indexOf("computer-panel-head"),
    panel.indexOf("computer-panel-screen"),
  );
  assert.match(header, /computer-panel-escape/, "Esc/Esc sits with Stop");
});

test("the working panel wears the same accent as the screen edge, and does not breathe", () => {
  const working = chatCss.match(/\.computer-panel\.working \{[\s\S]*?\}/);
  assert.ok(working, "the working panel has its own edge");
  assert.match(working[0], /--accent/);
  assert.doesNotMatch(
    working[0],
    /animation/,
    "two loops out of sync is the busyness ADR-0054 removed",
  );
});
