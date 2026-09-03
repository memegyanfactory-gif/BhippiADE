/**
 * The studio's chat tab strip.
 *
 * The owner's ask, verbatim: "on the top it should look like tabs so user can click and add
 * more chat/tabs if needed and can talk to any provider, and there are many settings icons —
 * fix that". Two things are locked down here. The selection is behaviour and gets real unit
 * tests: conversations are per project, so a tab strip that leaks a sibling game's chats — or
 * a terminal session — is wrong, and a strip that reorders itself while you type in a tab is
 * not a tab strip. The rest is source and stylesheet fact, because "the gear row is gone" and
 * "App actually hands the strip its sessions" are exactly the things a later tidy-up puts back.
 */

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import { chatTabsFor, chatTabTitle } from "../src/studio/chatTabs.ts";

const read = (rel) => readFileSync(new URL(rel, import.meta.url), "utf8");

const DEMO = "C:/Users/aayus/BhippiGames/demo-game";
const OTHER = "C:/Users/aayus/BhippiGames/other-game";

/** A `WorkspaceSession`-shaped row, with only the fields the strip reads spelled out. */
const session = (id, over = {}) => ({
  id,
  project_path: DEMO,
  kind: "ai_chat",
  title: `chat ${id}`,
  provider: "claude",
  provider_label: "Claude",
  status: "idle",
  created_at: `2026-09-0${id}T10:00:00Z`,
  updated_at: `2026-09-0${id}T10:00:00Z`,
  turn_count: 1,
  ...over,
});

// ── the selection ──────────────────────────────────────────────────────────────

test("no sessions, or none for this project, is an empty strip and not a crash", () => {
  assert.deepEqual(chatTabsFor([], DEMO), []);
  assert.deepEqual(chatTabsFor(null, DEMO), []);
  assert.deepEqual(chatTabsFor(undefined, DEMO), []);
  assert.deepEqual(chatTabsFor([session(1, { project_path: OTHER })], DEMO), []);
});

test("only this project's chats, compared the way the app compares paths", () => {
  const rows = [
    session(1),
    // Same project: verbatim prefix, backslashes, case and a trailing slash all normalise.
    session(2, { project_path: "\\\\?\\C:\\Users\\aayus\\BhippiGames\\Demo-Game\\" }),
    session(3, { project_path: OTHER }),
  ];
  assert.deepEqual(
    chatTabsFor(rows, DEMO).map((s) => s.id),
    [1, 2],
  );
  // …and the same rule read from the other side.
  assert.deepEqual(
    chatTabsFor(rows, "c:\\users\\aayus\\bhippigames\\demo-game").map((s) => s.id),
    [1, 2],
  );
});

test("a project path that names nothing selects nothing", () => {
  assert.deepEqual(chatTabsFor([session(1)], ""), []);
  assert.deepEqual(chatTabsFor([session(1)], null), []);
});

test("shells are not chats: only ai_chat rows become tabs", () => {
  const rows = [session(1), session(2, { kind: "cli" }), session(3)];
  assert.deepEqual(
    chatTabsFor(rows, DEMO).map((s) => s.id),
    [1, 3],
  );
});

test("the order is stable: a chat does not jump when it is used", () => {
  const rows = [session(3), session(1), session(2)];
  const before = chatTabsFor(rows, DEMO).map((s) => s.id);
  assert.deepEqual(before, [1, 2, 3], "oldest first, so a new chat lands next to the +");

  // The middle chat gets a turn — `updated_at` moves, the strip does not.
  const used = rows.map((s) => (s.id === 2 ? { ...s, updated_at: "2026-09-09T23:00:00Z" } : s));
  assert.deepEqual(chatTabsFor(used, DEMO).map((s) => s.id), before);

  // Two rows created in the same instant still sort deterministically, either way round.
  const tie = [
    session(1, { id: "b", created_at: "2026-09-01T00:00:00Z" }),
    session(1, { id: "a", created_at: "2026-09-01T00:00:00Z" }),
  ];
  assert.deepEqual(chatTabsFor(tie, DEMO).map((s) => s.id), ["a", "b"]);
  assert.deepEqual(chatTabsFor([...tie].reverse(), DEMO).map((s) => s.id), ["a", "b"]);
});

test("the selection does not mutate what it was given", () => {
  const rows = [session(3), session(1)];
  chatTabsFor(rows, DEMO);
  assert.deepEqual(rows.map((s) => s.id), [3, 1], "sort() on the caller's array would reorder it");
});

test("an unnamed chat is labelled, not blank", () => {
  assert.equal(chatTabTitle("Boss fight"), "Boss fight");
  assert.equal(chatTabTitle("  Boss fight  "), "Boss fight");
  assert.equal(chatTabTitle(""), "New chat");
  assert.equal(chatTabTitle("   "), "New chat");
  assert.equal(chatTabTitle(null), "New chat");
  assert.equal(chatTabTitle(undefined), "New chat");
});

// ── the wiring ─────────────────────────────────────────────────────────────────

const screen = read("../src/screens/StudioScreen.tsx");
const app = read("../src/App.tsx");
const tabs = read("../src/studio/ChatTabs.tsx");
const studioCss = read("../src/styles/studio.css");

test("the studio column carries the strip above the chat, fed by the pure selection", () => {
  // Both specifiers keep their extension: on Windows `ChatTabs.tsx` and `chatTabs.ts` differ
  // only in case, and an extensionless import of either resolves to whichever tsc saw first.
  assert.match(screen, /import \{ ChatTabs \} from "\.\.\/studio\/ChatTabs\.tsx"/);
  assert.match(screen, /import \{ chatTabsFor \} from "\.\.\/studio\/chatTabs\.ts"/);
  assert.match(screen, /chatTabsFor\(sessions, projectPath\)/, "the screen selects, it does not filter");

  const column = screen.slice(
    screen.indexOf('<aside className="studio-left-column">'),
    screen.indexOf("</aside>"),
  );
  assert.ok(column.includes("<ChatTabs"), "the strip is inside the studio's left column");
  assert.ok(
    column.indexOf("<ChatTabs") < column.indexOf("<Chat\n"),
    "the strip is above the transcript, not below it",
  );
  for (const wire of [
    "tabs={chatTabs}",
    "activeId={activeConversationId}",
    "onOpen={onOpenConversation ?? (() => {})}",
    "onClose={onCloseTab ?? (() => {})}",
    "onNew={onNewConversation ?? (() => {})}",
  ]) {
    assert.ok(column.includes(wire), `the strip should be wired with ${wire}`);
  }
});

test("App hands the studio the sessions and a close that deletes the chat it names", () => {
  const call = app.slice(app.indexOf("<StudioScreen"), app.indexOf("</>", app.indexOf("<StudioScreen")));
  assert.ok(call.includes("sessions={allSessions ?? []}"), "the strip needs every session");
  assert.match(
    call,
    /onCloseTab=\{\(id\) => void deleteConversation\(id\)\}/,
    "the ✕ closes the tab it is on, not whichever chat happens to be active",
  );
  // The active-conversation close is still there; the strip did not replace it.
  assert.ok(call.includes("onCloseConversation={() => {"));
});

test("the strip replaces the chat's top bar in the studio, gear row and all", () => {
  assert.match(
    studioCss,
    /\.studio-main-layout \.studio-left-column \.chat-top-bar \{\s*display: none;\s*\}/,
    "the old bar is hidden in the studio column",
  );
  assert.ok(
    !tabs.includes("chat-top-plugins") && !tabs.includes("IconGear"),
    "the five identical gear buttons do not come back as part of the tab strip",
  );
  assert.ok(!screen.includes("chat-top-plugins"));
  // The column has to be a flex column or the strip pushes the chat off the bottom.
  const columnRule = studioCss.slice(
    studioCss.indexOf("\n.studio-main-layout .studio-left-column {"),
    studioCss.indexOf("}", studioCss.indexOf("\n.studio-main-layout .studio-left-column {")),
  );
  assert.match(columnRule, /display: flex/);
  assert.match(columnRule, /flex-direction: column/);
});

test("the strip is a keyboard-reachable row of buttons that scrolls instead of wrapping", () => {
  // Buttons, so Enter and Space activate them and focus order is the tab order. Not
  // `role="tab"`: that pattern promises arrow-key roving focus and a tabpanel, and a role
  // that lies about the keyboard is worse for a screen-reader user than no role at all.
  assert.match(tabs, /<button\s+type="button"\s+aria-current=\{active \? true : undefined\}/);
  assert.ok(!tabs.includes('role="tab"'), "no half-implemented ARIA tabs pattern");
  assert.ok(tabs.includes('aria-label={`Close ${label}`}'), "the ✕ says which chat it closes");
  assert.ok(tabs.includes('aria-label="Start a new chat"'), "the + is named for a screen reader");
  assert.match(tabs, /aria-label="Chats in this project"/, "the strip is named as a whole");

  const strip = studioCss.slice(studioCss.indexOf("\n.studio-chat-tabs {"));
  const scroller = strip.slice(
    strip.indexOf(".studio-chat-tabs-scroll {"),
    strip.indexOf("}", strip.indexOf(".studio-chat-tabs-scroll {")),
  );
  // SPA-403: tabs share the strip like Chrome's — they shrink, they never scroll away.
  assert.match(scroller, /overflow: hidden/, "many tabs shrink instead of scrolling off");
  assert.match(scroller, /flex-wrap: nowrap/, "…and never wrap onto a second row");
  const tab = strip.slice(
    strip.indexOf(".studio-chat-tab {"),
    strip.indexOf("}", strip.indexOf(".studio-chat-tab {")),
  );
  assert.match(tab, /flex: 1 1 168px/, "every tab gives way as more open");
  assert.match(tab, /min-width: 36px/, "down to the glyph alone");
  assert.match(strip, /@container \(max-width: 64px\)/, "the title goes before the tab does");

  const bar = strip.slice(0, strip.indexOf("}"));
  assert.match(bar, /height: 34px/, "one row, flush with the top of the column");
  assert.match(bar, /flex: none/, "the strip never takes height from the transcript");
});

test("the strip's colours are tokens, so it survives the light and contrast palettes", () => {
  const opensAt = studioCss.indexOf("\n.studio-chat-tabs {");
  const lastAt = studioCss.indexOf(".studio-chat-tabs :focus-visible {");
  assert.ok(opensAt > 0 && lastAt > opensAt, "the tab strip section should be one block");
  const strip = studioCss.slice(opensAt, studioCss.indexOf("}", lastAt) + 1);
  assert.doesNotMatch(strip, /#[0-9a-fA-F]{3,8}/, "a hard-coded colour dies on a light ground");
  assert.doesNotMatch(strip, /rgba?\(/);
});
