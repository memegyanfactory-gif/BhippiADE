/**
 * Inspector Agents in the webview (ADR-0056).
 *
 * Two kinds of test. The first exercises the only logic the page owns — which rows a filter
 * leaves visible, and how a dimension with no answer is drawn. The second reads the source,
 * because the rules that matter here are rules about what the page must *not* do: compute a
 * score, apply a fix without a token, or put anything over the Godot viewport.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

import {
  NO_FILTER,
  SEVERITY_LABEL,
  SEVERITY_ORDER,
  notMeasuredHint,
  offers,
  railCount,
  railScore,
  visibleFindings,
} from "../src/studio/inspectorView.ts";

const root = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, root), "utf8");

function finding(overrides = {}) {
  return {
    id: "f_1",
    inspector: "gameplay",
    code: "BHP-INS-302",
    severity: "high",
    confidence: 97,
    title: "`Trigger` never delivers body_entered",
    location: { scene: "scenes/main.tscn", node: "Door/Trigger" },
    where_label: "scenes/main.tscn · Door/Trigger",
    cause: "no connection",
    impact: "the door will not open",
    recommendation: "connect it",
    evidence: [],
    fix: null,
    status: "open",
    actions: ["open_scene", "locate", "ask", "send_to_agent", "ignore"],
    ...overrides,
  };
}

// ── the filter ──────────────────────────────────────────────────────────────────

test("with no filter every finding Rust returned is shown, in the order it returned them", () => {
  const rows = [finding(), finding({ id: "f_2", inspector: "code", severity: "low" })];
  const shown = visibleFindings(rows, NO_FILTER);
  assert.deepEqual(
    shown.map((row) => row.id),
    ["f_1", "f_2"],
  );
});

test("the rail filters by inspector and the chips by severity, and they compose", () => {
  const rows = [
    finding(),
    finding({ id: "f_2", inspector: "code", severity: "low" }),
    finding({ id: "f_3", inspector: "code", severity: "high" }),
  ];
  assert.deepEqual(
    visibleFindings(rows, { ...NO_FILTER, inspector: "code" }).map((row) => row.id),
    ["f_2", "f_3"],
  );
  assert.deepEqual(
    visibleFindings(rows, { ...NO_FILTER, severity: "high" }).map((row) => row.id),
    ["f_1", "f_3"],
  );
  assert.deepEqual(
    visibleFindings(rows, { inspector: "code", severity: "high", query: "" }).map(
      (row) => row.id,
    ),
    ["f_3"],
  );
});

test("search matches the title, the location line and the check code", () => {
  const rows = [
    finding(),
    finding({
      id: "f_2",
      title: "Debug print",
      code: "BHP-INS-204",
      where_label: "scripts/door.gd:12",
    }),
  ];
  assert.equal(visibleFindings(rows, { ...NO_FILTER, query: "door/trig" }).length, 1);
  assert.equal(visibleFindings(rows, { ...NO_FILTER, query: "bhp-ins-204" })[0].id, "f_2");
  assert.equal(visibleFindings(rows, { ...NO_FILTER, query: "nothing here" }).length, 0);
});

// ── the rail reads Rust's numbers ───────────────────────────────────────────────

const health = {
  score: 82,
  complete: false,
  incomplete: ["performance"],
  incomplete_line: "Project health incomplete. 1 inspector has not yet scanned.",
  dimensions: [
    {
      inspector: "gameplay",
      label: "Gameplay",
      coverage: { state: "scanned", items: 4 },
      findings: 3,
      score: 76,
    },
    {
      inspector: "performance",
      label: "Performance",
      coverage: { state: "not_measured", how: "Run Performance Scan" },
      findings: 0,
      score: null,
    },
  ],
  counts: { critical: 1, high: 2, medium: 0, low: 0, suggestion: 0, info: 0, total: 3 },
};

test("a rail row shows the score Rust computed, and an em dash where there is none", () => {
  assert.equal(railScore(health, "gameplay"), "76");
  assert.equal(railScore(health, "performance"), "—");
  assert.equal(railScore(health, "physics"), "—", "a dimension with no row has no score");
  assert.equal(railScore(null, "gameplay"), "—");
});

test("a rail count is read, never counted, and a missing row is not zero", () => {
  assert.equal(railCount(health, "gameplay"), 3);
  assert.equal(railCount(health, "performance"), 0);
  assert.equal(railCount(health, "physics"), null);
  assert.equal(railCount(null, "gameplay"), null);
});

test("an unmeasured inspector offers the scan that would measure it, and a scanned one does not", () => {
  assert.equal(notMeasuredHint(health, "performance"), "Run Performance Scan");
  assert.equal(notMeasuredHint(health, "gameplay"), null);
});

test("a finding only offers the actions Rust said it could", () => {
  const row = finding();
  assert.ok(offers(row, "locate"));
  assert.ok(!offers(row, "fix"), "no fix in the action list means no Fix button");
  assert.ok(offers(finding({ actions: ["fix"] }), "fix"));
});

test("every severity has a label, and the order is worst first", () => {
  assert.equal(SEVERITY_ORDER[0], "critical");
  assert.equal(SEVERITY_ORDER[SEVERITY_ORDER.length - 1], "info");
  for (const severity of SEVERITY_ORDER) {
    assert.ok(SEVERITY_LABEL[severity], `${severity} has no label`);
  }
});

// ── what the page must not do ───────────────────────────────────────────────────

const panel = read("src/studio/InspectorPanel.tsx");
const view = read("src/studio/inspectorView.ts");
const menu = read("src/studio/InspectMenu.tsx");
const dock = read("src/studio/StudioBottomDock.tsx");
const screen = read("src/screens/StudioScreen.tsx");

test("the panel prints the score and the incompleteness line Rust computed, and computes neither", () => {
  assert.match(panel, /report\.health\.score/);
  assert.match(panel, /report\.health\.incomplete_line/);
  assert.match(panel, /report\.health\.counts\.total/);
  // No arithmetic over findings anywhere in the panel or its view helpers.
  for (const [name, source] of [
    ["InspectorPanel.tsx", panel],
    ["inspectorView.ts", view],
  ]) {
    assert.doesNotMatch(source, /findings\.(reduce|filter)\([^)]*length\s*\*/, name);
    assert.doesNotMatch(source, /100\s*-\s*/, `${name} must not recompute a health score`);
  }
});

test("the fix flow is preview then apply, and apply carries the preview's own token", () => {
  assert.match(panel, /api\.inspectorPreviewFix\(/);
  assert.match(panel, /api\.inspectorApplyFix\(projectPath, card\.finding_id, card\.token\)/);
  // The page never sends a batch of its own; there is no other write path from here.
  assert.doesNotMatch(panel, /godotApplyBatch/);
  // Apply only exists inside the preview block.
  const previewBlock = panel.slice(panel.indexOf("Proposed fix"));
  assert.match(previewBlock, /Apply fix/);
});

test("a scan is never started without a project, and the panel says so instead", () => {
  assert.match(panel, /if \(!projectPath\)/);
  assert.match(panel, /Open a game to inspect it\./);
});

test("the panel keeps all four states, including the honest empty one", () => {
  for (const state of ['state: "idle"', 'state: "loading"', 'state: "ready"', 'state: "error"']) {
    assert.ok(panel.includes(state), `the panel has no ${state}`);
  }
  assert.match(panel, /Nothing found\. That is a real answer/);
  assert.match(panel, /Not measured yet/);
});

test("the Inspect menu offers the six scopes and refuses the ones the studio cannot satisfy", () => {
  for (const label of [
    "Inspect current selection",
    "Inspect current level",
    "Inspect entire project",
    "Gameplay scan",
    "Performance scan",
    "Inspect changes",
  ]) {
    assert.ok(menu.includes(label), `the menu is missing "${label}"`);
  }
  // An unavailable item is disabled and says why, rather than silently widening the scope.
  assert.match(menu, /unavailable: selection \? null : "Nothing is selected"/);
  assert.match(menu, /disabled=\{item\.unavailable !== null\}/);
  assert.match(menu, /if \(item\.unavailable\) return;/);
});

test("the drawer sits in the dock's flow, so nothing is ever painted over the Godot window", () => {
  // The panel is rendered inside the drawer body like every other dock panel, and neither
  // it nor its stylesheet positions itself over the viewport (INV-090).
  assert.match(dock, /activeTab === "inspector" && \(\s*<InspectorPanel/);
  const css = read("src/styles/inspector.css");
  assert.doesNotMatch(css, /position:\s*fixed/);
  // The one absolutely-positioned thing is the toolbar menu, which sits over the toolbar.
  const absolute = css.match(/position:\s*absolute/g) ?? [];
  assert.equal(absolute.length, 1, "only the ◉ Inspect popover is positioned");
});

test("the Inspect menu is no longer mounted in the studio", () => {
  // The engine toolbar that carried it was removed so the preview would be the game and
  // nothing else. `InspectorPanel` and the scan behind it are untouched and still tested
  // below — what went is the way into them from under the viewport.
  assert.doesNotMatch(screen, /<InspectMenu/);
  assert.doesNotMatch(screen, /setDockTab\("inspector"\)/);
});

test("severity is never carried by colour alone", () => {
  // Every mark in the list and the chips sits beside the severity's word.
  assert.match(panel, /<SeverityMark severity=\{finding\.severity\} \/>/);
  assert.match(panel, /SEVERITY_LABEL\[finding\.severity\]/);
  assert.match(panel, /aria-hidden="true"/);
});
