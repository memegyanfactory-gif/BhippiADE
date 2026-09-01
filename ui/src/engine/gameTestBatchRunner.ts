import type { RuntimeBudgets } from "../lib/ipc";
import type {
  InputDocument,
  RuntimeCapability,
  RuntimeDocument,
  Vec3,
} from "./playRuntime.ts";
import type { ScriptProgram } from "./scriptVm.ts";
import {
  GAME_TEST_BATCH_FORMAT,
  type GameTestAssertionEvidence,
  type GameTestPlan,
  type GameTestScenario,
  type GameTestScenarioReport,
  type RuntimeHudDocument,
} from "./gameTestPlan.ts";
import type { RuntimeWorkerSandboxEvidence } from "./runtimeWorkerSession.ts";

export type GameTestWorld = {
  document: RuntimeDocument;
  gravity: Vec3;
  input: InputDocument;
  hud: RuntimeHudDocument | null;
  levels: string[];
  programs: Array<{ entity: string; path: string; program: ScriptProgram }>;
  capabilities: RuntimeCapability[];
  budgets: RuntimeBudgets;
};

export type GameTestScenarioWorker = {
  runGameTestScenario(
    scenario: GameTestScenario,
    fixedDeltaSeconds: number,
    watchdogMillis: number,
  ): Promise<GameTestScenarioReport & {
    sandbox: RuntimeWorkerSandboxEvidence;
    workerSessionHash: string;
  }>;
};

export type GameDebugRuntimeEvidence = {
  protocol: string;
  execution: string;
  capabilities: RuntimeCapability[];
  budgets: {
    instructions_per_tick: number;
    instructions_total: number;
    call_depth: number;
    timers: number;
    heap_estimate_bytes: number;
    wall_clock_millis: number;
    message_bytes: number;
    messages_per_tick: number;
    spawned_entities: number;
    emitted_events: number;
    log_bytes: number;
  };
  termination_reason: "completed" | "runtime_fault";
  authored_hash_before: string;
  authored_hash_after: string;
  frames: number;
  checkpoint_hashes: string[];
  fault_count: number;
  trace: {
    entries: Array<{
      kind: string;
      capability?: RuntimeCapability;
      decision?: "granted" | "denied";
      subject?: string;
      line?: number;
      instruction?: number;
      message?: string;
    }>;
    truncated: boolean;
    redactions: number;
    usage: {
      instructions: number;
      messages: number;
      spawned_entities: number;
      emitted_events: number;
      log_bytes: number;
      timers: number;
      heap_estimate_bytes: number;
      wall_clock_millis: number;
    };
  };
};

export type GameTestScenarioEvidence = {
  name: string;
  initial_level: string;
  seed: number;
  worker_session_hash: string;
  runtime: GameDebugRuntimeEvidence;
  assertions: GameTestAssertionEvidence[];
  completed: boolean;
};

export type GameTestBatchEvidence = {
  format: typeof GAME_TEST_BATCH_FORMAT;
  plan_format: GameTestPlan["format"];
  authored_tree_before: string;
  authored_tree_after: string;
  scenarios: GameTestScenarioEvidence[];
};

export type GameTestBatchOptions = {
  authoredTreeHash: string;
  fixedDeltaSeconds: number;
  watchdogMillis: number;
  loadWorld: (initialLevel: string) => Promise<GameTestWorld>;
  startWorker: (world: GameTestWorld, seed: number) => Promise<GameTestScenarioWorker>;
};

/** One world composition and one new worker are mandatory for every scenario. */
export async function executeGameTestBatch(
  plan: GameTestPlan,
  options: GameTestBatchOptions,
): Promise<GameTestBatchEvidence> {
  const scenarios: GameTestScenarioEvidence[] = [];
  const deadline = performance.now() + options.watchdogMillis;
  for (const scenario of plan.scenarios) {
    const world = await options.loadWorld(scenario.initial_level);
    const worker = await options.startWorker(world, scenario.seed);
    const remainingMillis = Math.floor(deadline - performance.now());
    if (remainingMillis <= 0) {
      throw new Error("Game-test batch exhausted its Rust-owned watchdog before the next scenario.");
    }
    const report = await worker.runGameTestScenario(
      scenario,
      options.fixedDeltaSeconds,
      remainingMillis,
    );
    const runtime = runtimeEvidence(report, report.sandbox);
    const completed =
      runtime.termination_reason === "completed" &&
      runtime.fault_count === 0 &&
      runtime.checkpoint_hashes.length === scenario.checkpoints.length &&
      report.assertions.every((assertion) => assertion.passed);
    scenarios.push({
      name: scenario.name,
      initial_level: scenario.initial_level,
      seed: scenario.seed,
      worker_session_hash: report.workerSessionHash,
      runtime,
      assertions: report.assertions,
      completed,
    });
  }
  return {
    format: GAME_TEST_BATCH_FORMAT,
    plan_format: plan.format,
    authored_tree_before: options.authoredTreeHash,
    authored_tree_after: options.authoredTreeHash,
    scenarios,
  };
}

function runtimeEvidence(
  report: GameTestScenarioReport,
  sandbox: RuntimeWorkerSandboxEvidence,
): GameDebugRuntimeEvidence {
  const budgets = sandbox.budgets;
  const usage = sandbox.trace.usage;
  return {
    protocol: sandbox.protocol,
    execution: sandbox.execution,
    // Rust sends canonical enum-declaration order; lexical sorting would invalidate it.
    capabilities: [...sandbox.capabilities],
    budgets: {
      instructions_per_tick: budgets.instructionsPerTick,
      instructions_total: budgets.instructionsTotal,
      call_depth: budgets.callDepth,
      timers: budgets.timers,
      heap_estimate_bytes: budgets.heapEstimateBytes,
      wall_clock_millis: budgets.wallClockMillis,
      message_bytes: budgets.messageBytes,
      messages_per_tick: budgets.messagesPerTick,
      spawned_entities: budgets.spawnedEntities,
      emitted_events: budgets.emittedEvents,
      log_bytes: budgets.logBytes,
    },
    termination_reason: sandbox.terminationReason,
    authored_hash_before: report.authoredHashBefore,
    authored_hash_after: report.authoredHashAfter,
    frames: report.frames,
    checkpoint_hashes: report.samples.map((sample) => sample.checkpointHash),
    fault_count: report.faults.length,
    trace: {
      entries: sandbox.trace.entries.map((entry) => ({ ...entry })),
      truncated: sandbox.trace.truncated,
      redactions: sandbox.trace.redactions,
      usage: {
        instructions: usage.instructions,
        messages: usage.messages,
        spawned_entities: usage.spawnedEntities,
        emitted_events: usage.emittedEvents,
        log_bytes: usage.logBytes,
        timers: usage.timers,
        heap_estimate_bytes: usage.heapEstimateBytes,
        wall_clock_millis: usage.wallClockMillis,
      },
    },
  };
}
