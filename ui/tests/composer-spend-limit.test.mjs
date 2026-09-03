/**
 * SPA-002 / SPA-003: the strip reads model · effort · ring, and a reached ceiling is a
 * card above the box that blocks the turn.
 *
 * Every word on the card is Rust's (`SpendLimitView`); the page only decides whether the
 * button exists. These tests read the shipping source so a "helpful" local dollar figure
 * or a re-grown token pill cannot come back unnoticed.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const read = (rel) => fs.readFileSync(path.join(here, "..", "src", rel), "utf8");
const chat = read("screens/Chat.tsx");
const meter = read("components/ChatUsageMeter.tsx");
const ipc = read("lib/ipc.ts");
const api = read("lib/api.ts");
const usagePanel = read("screens/UsagePanel.tsx");
const popovers = read("components/ComposerPopovers.tsx");

test("the ceiling is typed by Rust and reaches the page through the summary", () => {
  assert.ok(ipc.includes("export type SpendLimitView"), "the view is generated, not hand-written");
  assert.ok(ipc.includes("spend_limit: SpendLimitView | null"), "the summary carries it");
  assert.ok(api.includes("setMonthlySpendCap"), "the cap is set through the typed command");
});

test("a reached ceiling blocks the turn and shows the card", () => {
  // The ceiling is the composer's provider's, never the default provider's: Claude's spent
  // week must not block OpenCode.
  assert.ok(chat.includes("row.id.toLowerCase() === composerProviderId)?.spend_limit"));
  assert.ok(
    chat.includes("const spendBlocked = Boolean(spendLimit?.reached && spendLimit.can_raise);"),
    "only Bhippi's own caps block; a vendor limit offers Switch provider",
  );
  assert.ok(chat.includes("Switch provider"));
  assert.ok(
    chat.includes("if (spendBlocked && spendLimit) {"),
    "sendText refuses while the limit stands",
  );
  assert.ok(chat.includes("disabled={sending || spendBlocked}"), "the send circle is disabled");
  assert.ok(chat.includes("className={`spend-limit-card kind-${spendLimit.kind}`}"));
  assert.ok(chat.includes("{spendLimit.headline}"), "the headline is Rust's");
  assert.ok(chat.includes("{spendLimit.detail} · {spendLimit.resets_label}"), "so is the rest");
  assert.ok(chat.includes("Increase spend limit"), "the one action");
  assert.ok(
    chat.includes("{spendLimit.can_raise ? ("),
    "a vendor ceiling has no button — Bhippi cannot raise it",
  );
});

test("the strip ends in a ring, not a token pill, and the perception dot is gone", () => {
  assert.ok(meter.includes("<UsageRing fraction={ring.fraction} capped={ring.capped}"));
  assert.ok(!meter.includes("ledger-trigger-pill"), "the dot + text pill is gone");
  assert.ok(!meter.includes("ledger-dot-meter"), "no second meter beside the ring");
  assert.ok(
    !chat.includes("className={`composer-bar-btn dot-trigger"),
    "the perception monitor left the right cluster",
  );
  assert.ok(
    popovers.includes("Computer + Browser included"),
    "the desktop toggle still lives in the permission popover",
  );
});

test("the ring's source order is written down: weekly, then session, then the local cap", () => {
  // Node cannot import TSX, so the order is pinned in the source: the three fallbacks
  // appear in exactly this sequence inside `ringReading`.
  const start = meter.indexOf("export function ringReading");
  const body = meter.slice(start, meter.indexOf("\n}\n", start));
  const weekly = body.indexOf('source: "weekly"');
  const session = body.indexOf('source: "session"');
  const local = body.indexOf('source: "local"');
  const none = body.indexOf('source: "none"');
  assert.ok(weekly > 0 && session > weekly && local > session && none > local, body);
  assert.ok(body.includes("capped: false"), "nothing known leaves the ring an empty track");
});

test("Settings › Usage edits the monthly ceiling through the command", () => {
  assert.ok(usagePanel.includes("function MonthlyCapField"));
  assert.ok(usagePanel.includes(".setMonthlySpendCap(next)"));
  assert.ok(usagePanel.includes("summary.monthly_usd_cap"), "the field shows the stored figure");
});
