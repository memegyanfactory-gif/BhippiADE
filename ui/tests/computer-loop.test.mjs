/**
 * The Computer Use panel after ADR-0048 and ADR-0054.
 *
 * The loop's own behaviour is proved in Rust (`computer_loop::tests`); what can break on
 * this side is the wiring. Three things in particular, and each has cost a real bug in a
 * feature like this before: a step counter drawn against a number typed into the UI instead
 * of the one the backend enforces, a correction round counted as an action so the count
 * lies, and a permission card that cannot render the scope the loop actually raises.
 *
 * Since ADR-0054 this panel is also the *only* place a turn is watched, so what it fails to
 * show is not shown anywhere.
 */

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

// .gitattributes pins the sources to LF but not the stylesheets, so a Windows checkout
// hands these back with CRLF and every multi-line needle below would silently miss.
const read = (rel) =>
  readFileSync(new URL(rel, import.meta.url), "utf8").replaceAll("\r\n", "\n");

const panel = read("../src/components/BhippiComputerPanel.tsx");
const chat = read("../src/screens/Chat.tsx");
const chatCss = read("../src/styles/chat.css");
const ipcTs = read("../src/lib/ipc.ts");
const prompt = read("../../prompts/chat-computer-use.md");

test("CU-001: the action budget crosses IPC rather than being typed into the UI", () => {
  assert.ok(
    ipcTs.includes("max_actions_per_turn"),
    "the cap must reach the UI from Rust — re-run export-bindings",
  );
  assert.ok(
    chat.includes("status.max_actions_per_turn"),
    "Chat must read the cap from the status it already fetches",
  );
  assert.ok(
    panel.includes("maxActions"),
    "the panel must take the cap as a prop",
  );
  // The denominator must be the prop, not a literal. (A bare "no 24 anywhere" check would
  // be wrong: 24 is also an icon size and an animation delay in this file.)
  assert.ok(
    panel.includes("of {maxActions} steps"),
    "the counter's denominator must be the cap from Rust",
  );
  assert.ok(
    !/maxActions\s*=\s*[1-9]/.test(panel),
    "the prop must not default to an invented cap",
  );
  // The bar that used to divide these is gone (ADR-0054); the counter prints both, so the
  // numerator has to be the measured one rather than the raw step list.
  assert.ok(
    panel.includes("tools.filter(isExecutedAction).length"),
    "the count must exclude rounds that cost no budget",
  );
});

test("CU-002: a correction round is shown but not counted against the budget", () => {
  // A repair costs a provider round and does nothing to the machine, so counting it would
  // make the meter read as progress that never happened.
  assert.ok(panel.includes("isExecutedAction"), "executed actions are distinguished");
  assert.ok(
    panel.includes("tools.filter(isExecutedAction)"),
    "the meter counts executed actions only",
  );
  for (const nonAction of ["asked again", "asked for one", "Declined"]) {
    assert.ok(
      panel.includes(nonAction),
      `the filter must know about the "${nonAction}" row the loop emits`,
    );
  }
});

test("CU-003: the meter is hidden rather than invented when the cap is unknown", () => {
  assert.ok(
    panel.includes("maxActions = 0"),
    "an unknown cap defaults to zero",
  );
  assert.ok(
    panel.includes("maxActions > 0 ?"),
    "and zero draws no meter at all rather than a fake one",
  );
});

test("CU-004: the step counter is styled and does not jitter", () => {
  // ADR-0054 replaced the animated budget bar with a plain "N of M steps". The bar is gone
  // on purpose — it animated a number the text already said — but the reason the number
  // needed tabular figures has not changed: it moves every round.
  assert.ok(chatCss.includes(".computer-panel-steps"), "the counter is styled");
  assert.ok(
    /\.computer-panel-steps\s*\{[^}]*tabular-nums/.test(chatCss),
    "a counter that changes every round needs tabular numerals or it jitters",
  );
  // The one thing still allowed to move is the state dot, and only because it is the one
  // thing still changing.
  assert.ok(
    /@keyframes computer-pulse/.test(chatCss),
    "the working state has a signal that is not a word",
  );
  assert.ok(
    chatCss.includes("prefers-reduced-motion"),
    "and it stops when the viewer has asked for less motion",
  );
});

test("CU-005: the permission card can draw a gate the loop raises", () => {
  // The loop raises `scope: "computer"` with a high risk; the card is generic, and this
  // asserts it stays that way rather than switching on a fixed set of scopes.
  assert.ok(chat.includes("function PermissionCard"), "the card exists");
  assert.ok(chat.includes("{request.scope}"), "the scope is rendered, not matched on");
  assert.ok(chat.includes("{request.action}"), "the action is rendered");
  assert.ok(chat.includes("{request.detail}"), "the reason reaches the user");
  assert.ok(
    !/scope\s*===\s*["']engine["']/.test(chat),
    "the card must not be limited to the engine scope",
  );
});

test("CU-006: the model's reason is what the action row shows", () => {
  // ADR-0044 §2 promised a caption naming the action and its reason. The reason arrives as
  // the tool's `detail`, so the row must render that rather than a fixed string.
  assert.ok(panel.includes("tool?.detail?.trim()"), "the live line shows the reason");
  assert.ok(panel.includes("tool.detail?.trim() || tool.title"), "so does each earlier step");
});

test("CU-007: the prompt and the loop agree on how a turn finishes", () => {
  // The fault this whole change exists for: "no action block" used to mean both "done" and
  // "I mistyped a verb". The prompt has to teach the difference.
  assert.ok(prompt.startsWith("version: 6"), "the prompt version moved with the contract");
  assert.ok(prompt.includes("no JSON at all"), "finishing is stated unambiguously");
  assert.ok(prompt.includes("is **not** a failure"), "a repair is stated as recoverable");
  assert.ok(prompt.includes('"reason"'), "the reason field is documented");
});
