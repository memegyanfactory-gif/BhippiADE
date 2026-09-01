import type { RuntimeBudgets } from "../lib/ipc";
import type {
  RuntimeCapability,
  RuntimeDocument,
  RuntimeEvent,
  RuntimeFrame,
  ScriptedPlaytestReport,
  ScriptedPlaytestStep,
  Vec3,
} from "./playRuntime.ts";
import type { ScriptProgram } from "./scriptVm.ts";
import {
  RUNTIME_PROTOCOL_FORMAT,
  deserialiseRuntimeFrame,
  type RuntimeWorkerSandboxEvidence,
  type RuntimeWorkerBudgets,
  type RuntimeWorkerEnvelope,
  type RuntimeWorkerRequest,
  type RuntimeWorkerResponse,
} from "./runtimeWorkerSession.ts";
import type { InputDocument } from "./playRuntime.ts";
import type {
  GameTestScenario,
  GameTestScenarioReport,
  RuntimeHudDocument,
} from "./gameTestPlan.ts";
import { sha256SessionIdentity } from "./gameTestIdentity.ts";

type Pending = {
  resolve: (response: RuntimeWorkerResponse) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};

export type RuntimeWorkerClientOptions = {
  document: RuntimeDocument;
  gravity: Vec3;
  input: InputDocument;
  hud?: RuntimeHudDocument | null;
  levels?: string[];
  programs: Array<{ entity: string; path: string; program: ScriptProgram }>;
  capabilities: RuntimeCapability[];
  budgets: RuntimeBudgets;
  seed?: number;
  pauseOnError: boolean;
  onFault: (message: string) => void;
};

/** A fault already surfaced through the client's onFault channel. */
export class RuntimeWorkerReportedError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RuntimeWorkerReportedError";
  }
}

/** One client owns exactly one module worker and one monotonic protocol session. */
export class RuntimeWorkerClient {
  private sequence = 0;
  private readonly pending = new Map<number, Pending>();
  private readonly sourcePathByEntity: Map<string, string>;
  private readonly deadlineAtMillis: number;
  private closed = false;

  private constructor(
    private readonly worker: Worker,
    private readonly nonce: string,
    sourcePathByEntity: Map<string, string>,
    wallClockMillis: number,
    private readonly onFault: (message: string) => void,
  ) {
    this.sourcePathByEntity = sourcePathByEntity;
    this.deadlineAtMillis = performance.now() + wallClockMillis;
    worker.onmessage = (event: MessageEvent<RuntimeWorkerEnvelope<RuntimeWorkerResponse>>) => {
      const envelope = event.data;
      if (
        !isWorkerResponseEnvelope(envelope) ||
        envelope.format !== RUNTIME_PROTOCOL_FORMAT ||
        envelope.sessionNonce !== this.nonce
      ) {
        this.failAll("The gameplay worker returned an invalid session envelope.");
        return;
      }
      const pending = this.pending.get(envelope.sequence);
      if (!pending) {
        this.failAll(`The gameplay worker returned unexpected sequence ${envelope.sequence}.`);
        return;
      }
      clearTimeout(pending.timer);
      this.pending.delete(envelope.sequence);
      if (envelope.payload.kind === "fault") {
        const message = `${envelope.payload.code}: ${envelope.payload.message}`;
        pending.reject(new RuntimeWorkerReportedError(message));
        this.failAll(message);
      } else {
        pending.resolve(envelope.payload);
      }
    };
    worker.onerror = (event) => this.failAll(`Gameplay worker exited: ${event.message}`);
    worker.onmessageerror = () => this.failAll("Gameplay worker returned an unreadable message.");
  }

  static async start(options: RuntimeWorkerClientOptions): Promise<RuntimeWorkerClient> {
    const worker = new Worker(new URL("./playRuntime.worker.ts", import.meta.url), {
      type: "module",
      name: "bhippi-gameplay-runtime",
    });
    const nonce = crypto.randomUUID();
    const sourcePaths = new Map(options.programs.map((item) => [item.entity, item.path]));
    const client = new RuntimeWorkerClient(
      worker,
      nonce,
      sourcePaths,
      options.budgets.wall_clock_millis,
      options.onFault,
    );
    const response = await client.send({
      kind: "start",
      document: options.document,
      gravity: options.gravity,
      input: options.input,
      hud: options.hud ?? null,
      levels: options.levels ?? [],
      programs: options.programs.map((item, index) => ({
        entity: item.entity,
        program: { ...item.program, file: `script-${index}` },
      })),
      capabilities: options.capabilities,
      seed: options.seed ?? 0x5eed,
      pauseOnError: options.pauseOnError,
      budgets: workerBudgets(options.budgets),
    });
    if (response.kind !== "started") throw new Error("Gameplay worker did not acknowledge start.");
    return client;
  }

  tick(deltaSeconds: number, timeScale: number, force: boolean): Promise<RuntimeFrame> {
    return this.send({ kind: "tick", deltaSeconds, timeScale, force }).then((response) => {
      if (response.kind !== "frame") throw new Error("Gameplay worker returned no frame.");
      const frame = deserialiseRuntimeFrame(response.frame);
      frame.events = frame.events.map((event) => this.restoreSourcePath(event));
      return frame;
    });
  }

  runScriptedPlaytest(
    steps: ScriptedPlaytestStep[],
    fixedDeltaSeconds: number,
    watchdogMillis: number,
  ): Promise<ScriptedPlaytestReport & { sandbox: RuntimeWorkerSandboxEvidence }> {
    if (!Number.isSafeInteger(watchdogMillis) || watchdogMillis <= 0) {
      return Promise.reject(new Error("Gameplay worker playtest watchdog is invalid."));
    }
    return this.send(
      { kind: "scripted_playtest", steps, fixedDeltaSeconds },
      watchdogMillis,
    )
      .then((response) => {
        if (response.kind !== "playtest_report") {
          throw new Error("Gameplay worker returned no playtest report.");
        }
        return {
          ...response.report,
          faults: response.report.faults.map((event) => this.restoreSourcePath(event)),
          samples: response.report.samples.map((sample) => ({
            ...sample,
            events: sample.events.map((event) => this.restoreSourcePath(event)),
          })),
        };
      })
      .finally(() => this.terminate());
  }

  runGameTestScenario(
    scenario: GameTestScenario,
    fixedDeltaSeconds: number,
    watchdogMillis: number,
  ): Promise<
    GameTestScenarioReport & {
      sandbox: RuntimeWorkerSandboxEvidence;
      workerSessionHash: string;
    }
  > {
    if (!Number.isSafeInteger(watchdogMillis) || watchdogMillis <= 0) {
      return Promise.reject(new Error("Gameplay worker game-test watchdog is invalid."));
    }
    return this.send(
      { kind: "game_test_scenario", scenario, fixedDeltaSeconds },
      watchdogMillis,
    )
      .then(async (response) => {
        if (response.kind !== "game_test_report") {
          throw new Error("Gameplay worker returned no game-test scenario report.");
        }
        return {
          ...response.report,
          workerSessionHash: await sha256SessionIdentity(this.nonce),
          faults: response.report.faults.map((event) => this.restoreSourcePath(event)),
          samples: response.report.samples.map((sample) => ({
            ...sample,
            events: sample.events.map((event) => this.restoreSourcePath(event)),
          })),
        };
      })
      .finally(() => this.terminate());
  }

  input(code: string, pressed: boolean): void {
    void this.send({ kind: "input", code, pressed }).catch(() => undefined);
  }

  pause(paused: boolean): void {
    void this.send({ kind: "pause", paused }).catch(() => undefined);
  }

  reset(): void {
    void this.send({ kind: "reset" }).catch(() => undefined);
  }

  setVariable(path: string, value: string | number | boolean): void {
    void this.send({ kind: "set_variable", path, value }).catch(() => undefined);
  }

  stop(): void {
    if (this.closed) return;
    void this.send({ kind: "stop" })
      .catch(() => undefined)
      .finally(() => this.terminate());
  }

  terminate(): void {
    if (this.closed) return;
    this.closed = true;
    this.worker.terminate();
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error("Gameplay worker session ended."));
    }
    this.pending.clear();
  }

  private send(
    payload: RuntimeWorkerRequest,
    watchdogMillis = 2_000,
  ): Promise<RuntimeWorkerResponse> {
    if (this.closed) return Promise.reject(new Error("Gameplay worker session is closed."));
    const remainingMillis = Math.floor(this.deadlineAtMillis - performance.now());
    if (remainingMillis <= 0) {
      this.failAll("Gameplay worker wall-clock budget exhausted.");
      return Promise.reject(
        new RuntimeWorkerReportedError("Gameplay worker wall-clock budget exhausted."),
      );
    }
    const sequence = this.sequence;
    this.sequence += 1;
    const envelope: RuntimeWorkerEnvelope<RuntimeWorkerRequest> = {
      format: RUNTIME_PROTOCOL_FORMAT,
      sessionNonce: this.nonce,
      sequence,
      payload,
    };
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(sequence);
        reject(new RuntimeWorkerReportedError("Gameplay worker watchdog timed out."));
        this.failAll("Gameplay worker watchdog timed out.");
      }, Math.min(watchdogMillis, remainingMillis));
      this.pending.set(sequence, { resolve, reject, timer });
      this.worker.postMessage(envelope);
    });
  }

  private failAll(message: string): void {
    if (this.closed) return;
    this.onFault(message);
    this.closed = true;
    this.worker.terminate();
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new RuntimeWorkerReportedError(message));
    }
    this.pending.clear();
  }

  private restoreSourcePath(event: RuntimeEvent): RuntimeEvent {
    return event.kind === "script_fault"
      ? { ...event, file: this.sourcePathByEntity.get(event.entity) ?? event.file }
      : event;
  }
}

function isWorkerResponseEnvelope(
  value: unknown,
): value is RuntimeWorkerEnvelope<RuntimeWorkerResponse> {
  if (typeof value !== "object" || value === null) return false;
  const candidate = value as Partial<RuntimeWorkerEnvelope<RuntimeWorkerResponse>>;
  return (
    typeof candidate.format === "string" &&
    typeof candidate.sessionNonce === "string" &&
    Number.isSafeInteger(candidate.sequence) &&
    typeof candidate.payload === "object" &&
    candidate.payload !== null &&
    typeof candidate.payload.kind === "string"
  );
}

function workerBudgets(value: RuntimeBudgets): RuntimeWorkerBudgets {
  return {
    instructionsPerTick: value.instructions_per_tick,
    instructionsTotal: value.instructions_total,
    callDepth: value.call_depth,
    timers: value.timers,
    heapEstimateBytes: value.heap_estimate_bytes,
    wallClockMillis: value.wall_clock_millis,
    messageBytes: value.message_bytes,
    messagesPerTick: value.messages_per_tick,
    spawnedEntities: value.spawned_entities,
    emittedEvents: value.emitted_events,
    logBytes: value.log_bytes,
  };
}
