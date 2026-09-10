/**
 * Where a broken page goes to be recorded.
 *
 * A JavaScript exception used to leave nothing behind. React unmounted the tree, the window
 * went dark, and the only account anyone could give afterwards was "it went blank" — which
 * is not enough to fix anything, and is exactly what happened to a fifteen-minute turn.
 *
 * Three things reach this module: a render error caught by a boundary, an uncaught
 * exception, and a promise nobody handled. All three end up as an `ERROR` line in
 * `~/.bhippi/logs` with a stack, alongside every other failure the app records.
 *
 * Nothing here may throw. It runs in a page that has already failed once, and a reporter
 * that can fail is a reporter that needs its own reporter.
 */

/** How the page broke, in the words the log uses. */
export type CrashKind = "render" | "uncaught" | "rejection";

/** One crash, kept in memory so the fallback screen can show what it just reported. */
export interface CrashRecord {
  kind: CrashKind;
  message: string;
  stack: string | null;
  componentStack: string | null;
  surface: string | null;
  at: number;
}

/** The last few, newest first. Bounded: a render loop can throw thousands of times. */
const recent: CrashRecord[] = [];
const KEEP = 20;

/**
 * Identical crashes, collapsed.
 *
 * A component that throws on every render throws again on every retry, and a loop that
 * posts the same stack a thousand times buries the one line someone needs to read.
 */
const seen = new Map<string, number>();
const REPEAT_WINDOW_MS = 10_000;

export function recentCrashes(): readonly CrashRecord[] {
  return recent;
}

/** The message of a thrown value, which is not always an `Error`. */
function messageOf(thrown: unknown): string {
  if (thrown instanceof Error) return thrown.message || thrown.name || "Error";
  if (typeof thrown === "string") return thrown;
  try {
    return JSON.stringify(thrown) ?? String(thrown);
  } catch {
    return String(thrown);
  }
}

function stackOf(thrown: unknown): string | null {
  return thrown instanceof Error && typeof thrown.stack === "string" ? thrown.stack : null;
}

/**
 * Record one crash: keep it, and send it to the log.
 *
 * Returns the record so a fallback screen can show exactly what was reported rather than
 * re-deriving it and risking a different answer.
 */
export function reportCrash(
  kind: CrashKind,
  thrown: unknown,
  extra?: { componentStack?: string | null; surface?: string | null },
): CrashRecord {
  const record: CrashRecord = {
    kind,
    message: messageOf(thrown),
    stack: stackOf(thrown),
    componentStack: extra?.componentStack ?? null,
    surface: extra?.surface ?? null,
    at: Date.now(),
  };

  recent.unshift(record);
  if (recent.length > KEEP) recent.length = KEEP;

  const signature = `${kind}:${record.surface ?? ""}:${record.message}`;
  const last = seen.get(signature) ?? 0;
  if (record.at - last > REPEAT_WINDOW_MS) {
    seen.set(signature, record.at);
    // Console first: it costs nothing and it is where a developer with devtools open looks.
    console.error(`[bhippi] ${kind} in ${record.surface ?? "the app"}:`, thrown);
    void send(record);
  }
  return record;
}

/**
 * Hand it to Rust. Failing to report must never become the next thing that fails.
 *
 * The IPC layer is imported *here*, at the moment of use, rather than at the top of the
 * file. A crash reporter that pulls the whole `@tauri-apps` graph into its import chain
 * cannot be loaded anywhere that graph does not resolve — including the tests that check
 * it behaves, which is a poor trade for a module whose job is to work when nothing else is.
 */
async function send(record: CrashRecord): Promise<void> {
  try {
    const { api } = await import("./api");
    await api.reportUiError({
      kind: record.kind,
      message: record.message,
      stack: record.stack,
      component_stack: record.componentStack,
      surface: record.surface,
    });
  } catch {
    // Outside Tauri (a plain browser, a test page) there is nothing to report to, and a
    // crash reporter that complains about being unable to report is just noise.
  }
}

/**
 * Catch what never reaches a boundary.
 *
 * React error boundaries see render errors and nothing else: an exception in an event
 * handler, a `setTimeout`, or a rejected promise goes straight past them. Those are the
 * ones that used to leave no trace at all, because they do not even blank the screen —
 * they just make the app stop responding to one thing, silently.
 *
 * Installed once, from the entry point, before the app renders.
 */
export function installCrashReporting(): void {
  if (installed) return;
  installed = true;

  window.addEventListener("error", (event) => {
    // A failed <img> or <script> also fires `error` on the window, and it is not a crash.
    if (event.target && event.target !== window) return;
    reportCrash("uncaught", event.error ?? event.message, { surface: sourceOf(event) });
  });

  window.addEventListener("unhandledrejection", (event) => {
    reportCrash("rejection", event.reason);
  });
}

let installed = false;

/** "main.js:1240:9" — enough to find the frame when the thrown value carried no stack. */
function sourceOf(event: ErrorEvent): string | null {
  if (!event.filename) return null;
  const file = event.filename.split("/").pop() ?? event.filename;
  return `${file}:${event.lineno}:${event.colno}`;
}
