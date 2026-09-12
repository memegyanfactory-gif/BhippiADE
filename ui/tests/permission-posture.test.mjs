/**
 * The composer's permission chip (Ask approval · Auto · Full access).
 *
 * The chip looked finished and did almost nothing. Three switches with no relationship
 * between them: a mode in `localStorage` that auto-clicked permission cards in the page,
 * `engine.permission_mode` in config that nothing ever wrote, and a separate Computer Use
 * toggle. So "Auto" and "Full access" were two names for identical behaviour, ticking the
 * screen row granted input without moving the chip that claimed to govern it, and the
 * backend's own setting sat at its default forever. `set_permission_posture` existed in
 * Rust to fix exactly this — and was never registered, so it could not even compile.
 *
 * What follows is the shape of the repair, pinned: one posture, written once, read from
 * config, with the one bound that a posture may not buy its way past.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const root = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, root), "utf8");

const chat = read("src/screens/Chat.tsx");
const popovers = read("src/components/ComposerPopovers.tsx");
const api = read("src/lib/api.ts");
const ipc = read("src/lib/ipc.ts");
const css = read("src/styles/chat.css");

// ── the chip is actually connected to something ─────────────────────────────────

test("the posture command survives code generation, which means Rust registered it", () => {
  // `ipc.ts` is generated from the `collect_commands!` list (INV-032). The command was
  // written, documented and left out of that list, so the UI could only ever have got
  // "command not found" back. The generated file is the proof that it is reachable.
  assert.match(
    ipc,
    /setPermissionPosture: \(posture: string\)/,
    "set_permission_posture is missing from the generated bindings — re-register it in lib.rs",
  );
  assert.match(
    ipc,
    /permission: string,/,
    "the status the chip hydrates from must carry the posture",
  );
});

test("choosing a posture writes it through one call, not three", () => {
  assert.match(api, /setPermissionPosture: \(posture: string\)/);
  const writer = chat.slice(chat.indexOf("const applyPermissionMode"));
  const body = writer.slice(0, writer.indexOf("}, []);") + 7);
  assert.match(body, /api\.setPermissionPosture\(next\)/);

  // Rust applies the engine rule, the Computer Use gate and the input gate together, so
  // that a failure cannot leave two of the three agreeing and one not. Reaching around it
  // from the chip is what made "Full access" mean "Auto".
  assert.doesNotMatch(
    body,
    /setComputerUseEnabled|setComputerUseFullAccess/,
    "the posture must not be assembled from the individual switches",
  );
});

test("the posture is read back from config, never from localStorage", () => {
  assert.match(
    chat,
    /status\.permission === "full_access"[\s\S]{0,120}setPermissionMode\(status\.permission\)/,
    "the chip must show what the backend is actually set to on launch",
  );
  // A remembered copy is a second answer, and the two drift the moment config changes from
  // anywhere else — Settings, a hand-edited config.toml, another window.
  assert.doesNotMatch(
    chat,
    /bhippi_permission_mode|bhippi_permission_computer_browser/,
    "the posture has one home and it is config",
  );
});

test("opening the menu re-reads the gate rather than remembering it", () => {
  // Settings keeps its own Computer Use switches and config.toml can be edited by hand, so a
  // value read once at launch is a claim about the past. One IPC call on open is the whole
  // difference between a menu that reports the gate and one that recites it.
  assert.match(chat, /const refreshPermissionStatus = useCallback\(/);
  const opener = chat.slice(chat.indexOf("<PermissionPopover"));
  assert.match(
    opener.slice(0, opener.indexOf("/>")),
    /refreshPermissionStatus\(\);/,
    "the chip must refresh when it opens",
  );
});

test("the screen row is the posture, not a switch beside it", () => {
  assert.match(
    chat,
    /const computerBrowser = permissionMode === "full_access";/,
    "derived, so the tick and the chip cannot disagree",
  );
  // It used to grant `full_access` while leaving the chip reading "Ask" — a menu saying
  // nothing happens without a yes, over a config that could already drive the desktop.
  assert.match(
    chat,
    /applyPermissionMode\(next \? "full_access" : "auto"\)/,
    "turning it off must step the posture down, not just untick a box",
  );
});

test("Agent mode is the same posture under another name, not a fourth setting", () => {
  // It had its own remembered flag, and a launch effect that let that flag re-grant Auto over
  // whatever config said. So the switch could read "on" above a chip reading "Ask approval".
  assert.match(chat, /const agentMode = permissionMode !== "ask_approval";/);
  assert.doesNotMatch(chat, /bhippi_agent_mode/, "one fact, one home");
  // And it stops at Auto: a switch called "Agent mode" is not where somebody consents to
  // Bhippi driving their desktop.
  assert.match(chat, /applyPermissionMode\(agentMode \? "ask_approval" : "auto"\)/);
});

// ── the three options are three things ──────────────────────────────────────────

test("every posture says what it does, and no two say the same", () => {
  const details = [...popovers.matchAll(/detail: "([^"]+)"/g)].map((m) => m[1]);
  assert.equal(details.length, 3, "one line per posture");
  assert.equal(new Set(details).size, 3, "Auto and Full access must not read alike");
  assert.match(
    popovers,
    /detail: "Auto, and may drive the screen"/,
    "the line between Auto and Full access is the machine; the menu has to say so",
  );
});

test("the postures are one table, not three copied blocks", () => {
  // Three hand-written rows is how the copy drifted from the behaviour in the first place.
  assert.match(popovers, /export const PERMISSION_MODES/);
  const rows = [...popovers.matchAll(/^\s{4}id: "(ask_approval|auto|full_access)",$/gm)];
  assert.equal(rows.length, 3);
  // Counted as a JSX attribute on its own line, so neither the `radiogroup` wrapper nor the
  // focus selector that reads `[role="radio"]` is mistaken for a second hand-written row.
  assert.equal(
    popovers.match(/^\s+role="radio"$/gm).length,
    1,
    "one rendered row, mapped — a second literal means the table stopped being the source",
  );
});

test("NEXT ONLY is gone, because nothing ever expired it", () => {
  // The header promised a grant that lasted one turn. It was written straight to config and
  // stayed until somebody noticed, which is the worst kind of wrong label to put on a gate.
  // The rendered label, not the file: the comment above it explains why it went.
  assert.doesNotMatch(popovers, /<span>NEXT ONLY<\/span>/);
  assert.match(popovers, /<span>BEYOND THE PROJECT<\/span>/);
});

// ── the bound a posture cannot buy ──────────────────────────────────────────────

test("Full access never auto-answers the computer gate's own card", () => {
  // ADR-0048's ledger raises a card only when it has decided one specific action needs a
  // human — it left the window Bhippi launched, or it is high risk. The page answering yes
  // on the user's behalf would empty the gate of meaning: Full access is permission to drive
  // the game, not permission to skip the bound around it.
  assert.match(chat, /const gated = payload\.request\.scope === "computer";/);
  assert.match(chat, /if \(!gated && !PERMISSION_ASKS_FIRST\[permissionModeRef\.current\]\)/);
});

test("who asks first is a table, so a fourth posture cannot default to not asking", () => {
  assert.match(popovers, /export const PERMISSION_ASKS_FIRST: Record<PermissionMode, boolean>/);
  assert.match(popovers, /ask_approval: true/);
  assert.match(popovers, /auto: false/);
  assert.match(popovers, /full_access: false/);
});

// ── it can be used without a mouse ──────────────────────────────────────────────

test("the postures are a radio group and the screen row is a switch", () => {
  assert.match(popovers, /role="radiogroup"/);
  assert.match(popovers, /aria-checked=\{selected\}/);
  assert.match(popovers, /role="switch"/);
  assert.match(popovers, /aria-checked=\{computerBrowser\}/);
  // Roving tabindex: Tab reaches the current answer, arrows move within the group.
  assert.match(popovers, /tabIndex=\{selected \? 0 : -1\}/);
});

test("arrows move between postures and Escape gives focus back to the chip", () => {
  const menu = popovers.slice(popovers.indexOf("export function PermissionPopover"));
  for (const key of ["ArrowDown", "ArrowUp", "Home", "End"]) {
    assert.ok(menu.includes(key), `${key} has no handler`);
  }
  // Closing a menu and dropping focus on the body is how a keyboard user loses their place.
  assert.match(menu, /triggerRef\.current\?\.focus\(\)/);
  // And focus travels with the selection: without this the ring stays on the row the user
  // just left while the tick moves without it, and the next press starts from the wrong row.
  assert.match(
    menu,
    /querySelectorAll<HTMLButtonElement>\('\[role="radio"\]'\)\[next\]\?\.focus\(\)/,
  );
});

test("the focus ring is visible, or arrowing through the list shows nothing", () => {
  assert.match(css, /\.popover-row-btn:focus-visible/);
});

test("the screen row reads as on when it is on", () => {
  // It was a dim row with a tick somewhere off to the right — the same as every other row,
  // for the one setting in this menu that reaches off the project.
  assert.match(css, /\.popover-row-btn\.toggle-row\.active/);
});
