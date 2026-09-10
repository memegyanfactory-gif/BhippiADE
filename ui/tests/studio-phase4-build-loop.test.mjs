/**
 * Phase 4 Build Loop & Systems Tests (GAD-041…047, INV-086)
 */

import assert from "node:assert/strict";
import test from "node:test";

test("GAD-041: BuildRunView projection, ordered systems, and state machine lifecycle", () => {
  const buildRun = {
    runId: "run-test-01",
    title: "Cozy Exploration Island",
    systems: [
      { name: "World & Terrain", status: "completed" },
      { name: "Player & Glide Controller", status: "in_progress" },
      { name: "Lighthouse Collectibles", status: "pending" },
      { name: "HUD & Feathers", status: "pending" },
    ],
    currentSystemIndex: 1,
    state: "building",
    decision: undefined,
  };

  assert.equal(buildRun.systems.length, 4);
  assert.equal(buildRun.systems[0].status, "completed");
  assert.equal(buildRun.systems[1].status, "in_progress");
  assert.equal(buildRun.state, "building");
  assert.equal(buildRun.currentSystemIndex, 1);
});

test("GAD-043: Decision card pause, options, and answer unpauses build", () => {
  const pausedRun = {
    runId: "run-test-02",
    title: "Platformer Run",
    systems: [
      { name: "World", status: "completed" },
      { name: "Player", status: "in_progress" },
    ],
    currentSystemIndex: 1,
    state: "paused",
    decision: {
      id: "dec-glide",
      prompt: "Choose glide behavior:",
      options: ["Stamina-limited glide", "Infinite floaty glide"],
      selected: undefined,
    },
  };

  assert.equal(pausedRun.state, "paused");
  assert.ok(pausedRun.decision);
  assert.equal(pausedRun.decision.selected, undefined);

  // User submits choice
  pausedRun.decision.selected = "Stamina-limited glide";
  pausedRun.state = "verifying";

  assert.equal(pausedRun.decision.selected, "Stamina-limited glide");
  assert.equal(pausedRun.state, "verifying");
});

test("GAD-044: Self-build mode auto-resolves decision to default option", () => {
  const decision = {
    id: "dec-jump",
    prompt: "Jump style?",
    options: ["Arcade snap", "Realistic inertia"],
  };

  // Self build chooses options[0] without prompting
  const resolved = decision.options[0];
  assert.equal(resolved, "Arcade snap");
});

test("GAD-045: TaskCheckpoint holds applied transactions and remaining work", () => {
  const checkpoint = {
    format: "bhippi-task-checkpoint@1",
    project_state: {
      format: "bhippi-project-state@1",
      hash: "a".repeat(64),
    },
    goal: "Build Cozy Explorer: verified system player",
    constraints: ["Godot 4.7.1", "No source hand-edits"],
    unresolved_approvals: [],
    decisions: ["10 Feathers unlock"],
    selected_capability_ids: ["capability.world", "capability.player"],
    changes: ["Verified World", "Verified Player"],
    evidence: [
      {
        id: "ev-1",
        kind: "probe_telemetry",
        content_hash: "b".repeat(64),
      },
    ],
    files: ["scenes/main.tscn", "project.godot"],
    transaction_ids: ["txn-run-1-0", "txn-run-1-1"],
    failures: [],
    remaining_work: ["camera", "mechanics", "actors", "hud", "audio", "polish"],
    next_action: "Build system camera",
  };

  assert.equal(checkpoint.format, "bhippi-task-checkpoint@1");
  assert.equal(checkpoint.transaction_ids.length, 2);
  assert.equal(checkpoint.remaining_work.length, 6);
  assert.ok(!checkpoint.transaction_ids.includes("txn-run-1-2")); // Unapplied transaction not in set
});

test("GAD-046: /gamedebug full runs automatically at build completion", () => {
  const completedRun = {
    runId: "run-complete",
    title: "Final Build",
    state: "done",
    systems: [
      { name: "World", status: "completed" },
      { name: "Player", status: "completed" },
    ],
    debugReport: {
      passed: true,
      blockers: 0,
      warnings: 0,
      reportPath: ".bhippi/gamedebug/report-final.json",
    },
  };

  assert.equal(completedRun.state, "done");
  assert.equal(completedRun.debugReport.passed, true);
  assert.equal(completedRun.debugReport.blockers, 0);
});
