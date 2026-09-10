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

/* ── The drop-up's two scopes ──────────────────────────────────────────────────
   `ProviderUsage.total_tokens` is deliberately Bhippi's own ledger, so machine-wide CLI
   spend can never fill a local token cap. `models` also carries rows read out of the vendor
   CLI's session files, which means a model row can be *larger* than the provider total it
   sits under. Mixing the two in one list is what drew a 148M model inside a 2M day. */

test("the breakdown never lists CLI-history models beside the ledger's own", () => {
  const start = meter.indexOf("const allModels =");
  assert.ok(start > 0, "the model list must be split by scope, not sorted as one list");
  const body = meter.slice(start, meter.indexOf("/* ── render", start));
  assert.match(body, /ledgerModels[\s\S]*!model\.from_cli_history/, "the ledger group excludes history rows");
  assert.match(body, /historyModels[\s\S]*filter\(\(model\) => model\.from_cli_history\)/, "the history group is its own list");
  assert.ok(
    !meter.includes("const topModels"),
    "the single mixed list is gone, not merely filtered somewhere else",
  );
});

test("the CLI-history group says whose sessions it is counting", () => {
  // Without a heading these numbers read as part of the day above them, which is the whole
  // bug: the figure is real, it just belongs to a different question.
  assert.ok(meter.includes("All {providerLabel} sessions"), "the group names the provider");
  assert.ok(meter.includes("on this machine"), "and the scope");
  assert.ok(
    ipc.includes("from_cli_history: boolean"),
    "the scope is Rust's answer, not a comparison the screen makes up",
  );
});

test("a limit row's value can never wrap onto the progress bar", () => {
  const css = fs.readFileSync(path.join(here, "..", "src", "styles", "chat.css"), "utf8");
  const pct = css.slice(css.indexOf(".usage-limit-pct {"));
  const pctBody = pct.slice(0, pct.indexOf("}"));
  // `right` is a sentence on the token-cap row (`2.0M of 2.0M tokens`), not a percentage.
  assert.match(pctBody, /white-space:\s*nowrap/, "the value is one line");
  assert.match(pctBody, /flex:\s*0 0 auto/, "and it is never the item that shrinks");

  const reset = css.slice(css.indexOf(".usage-limit-reset {"));
  const resetBody = reset.slice(0, reset.indexOf("}"));
  // A flex item will not shrink below its content width without this, which is why the
  // value was the thing that broke instead.
  assert.match(resetBody, /min-width:\s*0/, "the reset text is what gives way");
  assert.match(resetBody, /text-overflow:\s*ellipsis/);
  assert.ok(meter.includes('className="usage-limit-reset" title={reset}'), "and keeps its full text on hover");
});

test("all three limit rows word their reset the same way", () => {
  // Rust's own `Cap resets at midnight` said "cap" twice under a row already labelled
  // "Token cap", and was long enough to push the value onto a second line.
  const rows = meter.slice(meter.indexOf('label="5-hour limit"'), meter.indexOf("This window"));
  assert.equal(
    (rows.match(/Resets \$\{fmtResetEpoch\(/g) ?? []).length,
    3,
    "session, weekly and the local cap all format their reset the same way",
  );
});
