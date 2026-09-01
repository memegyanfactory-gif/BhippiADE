import type { AgentPhase } from "../lib/ipc";

/**
 * The agent's current state, drawn.
 *
 * Twenty-eight states could be twenty-eight illustrations. They are not, deliberately:
 * each one is built from the same six primitives — dot, bar, ring, sweep, caret, track —
 * so the whole set reads as one instrument rather than as a sticker sheet, and a state
 * nobody has seen before is still legible because its parts are familiar.
 *
 * Every phase also carries a static tell (its glyph shape and its tone), because the
 * animation is off entirely under `prefers-reduced-motion` and a state that is only
 * legible while moving is not a state, it is a decoration.
 */

/** Which primitive a phase draws, and how it behaves. */
type Motion =
  | "dots" // three dots bouncing — deliberating
  | "meter" // bars rising and falling — measuring, accumulating
  | "sweep" // a line rotating — searching a space
  | "scan" // a bar crossing a track — passing over something linearly
  | "travel" // a dot crossing a track — moving between two places
  | "ring" // a ring drawing itself — a bounded operation
  | "caret" // a blinking block — writing
  | "halo" // an expanding halo — asking for attention
  | "still"; // no motion — a finished state

/** The tone a phase is drawn in. Colour is never the only signal; the label always says. */
type Tone = "accent" | "neutral" | "ok" | "warn" | "error";

type Spec = { motion: Motion; tone: Tone; label: string };

/**
 * Every phase the engine can emit. Exhaustive by construction — `Record<AgentPhase, …>`
 * makes adding a phase in Rust a TypeScript error here rather than a state that silently
 * renders as nothing.
 */
const PHASES: Record<AgentPhase, Spec> = {
  connecting: { motion: "travel", tone: "neutral", label: "Connecting" },
  queued: { motion: "still", tone: "neutral", label: "Queued" },
  thinking: { motion: "dots", tone: "accent", label: "Thinking" },
  reasoning: { motion: "meter", tone: "accent", label: "Reasoning" },
  planning: { motion: "meter", tone: "accent", label: "Planning" },
  searching: { motion: "sweep", tone: "accent", label: "Searching" },
  reading: { motion: "scan", tone: "neutral", label: "Reading" },
  writing: { motion: "caret", tone: "accent", label: "Writing" },
  editing: { motion: "scan", tone: "accent", label: "Editing" },
  refactoring: { motion: "meter", tone: "accent", label: "Refactoring" },
  running: { motion: "caret", tone: "neutral", label: "Running" },
  testing: { motion: "ring", tone: "neutral", label: "Testing" },
  building: { motion: "meter", tone: "neutral", label: "Building" },
  debugging: { motion: "sweep", tone: "warn", label: "Debugging" },
  installing: { motion: "travel", tone: "neutral", label: "Installing" },
  fetching: { motion: "travel", tone: "neutral", label: "Fetching" },
  browsing: { motion: "sweep", tone: "neutral", label: "Browsing" },
  analyzing: { motion: "meter", tone: "accent", label: "Analysing" },
  summarizing: { motion: "scan", tone: "accent", label: "Summarising" },
  reviewing: { motion: "ring", tone: "accent", label: "Reviewing" },
  awaiting_permission: { motion: "halo", tone: "warn", label: "Waiting for you" },
  compacting: { motion: "scan", tone: "warn", label: "Compacting" },
  retrying: { motion: "ring", tone: "warn", label: "Retrying" },
  streaming: { motion: "caret", tone: "accent", label: "Writing the answer" },
  finalizing: { motion: "ring", tone: "accent", label: "Finishing" },
  done: { motion: "still", tone: "ok", label: "Done" },
  stopped: { motion: "still", tone: "neutral", label: "Stopped" },
  failed: { motion: "still", tone: "error", label: "Failed" },
};

/** The spec for a phase, falling back to a generic working state for an unknown one. */
export function phaseSpec(phase: AgentPhase | null | undefined): Spec {
  if (!phase) return PHASES.thinking;
  return PHASES[phase] ?? PHASES.thinking;
}

/** The glyph alone, for a tight space like a status bar or a list row. */
export function PhaseGlyph({
  phase,
  size = 14,
}: {
  phase: AgentPhase | null | undefined;
  size?: number;
}) {
  const { motion, tone } = phaseSpec(phase);
  return (
    <span
      className={`phase-glyph is-${motion} tone-${tone}`}
      style={{ ["--glyph-size" as string]: `${size}px` }}
      aria-hidden="true"
    >
      {motion === "dots" ? (
        <>
          <i className="g-dot" />
          <i className="g-dot" />
          <i className="g-dot" />
        </>
      ) : null}

      {motion === "meter" ? (
        <>
          <i className="g-bar" />
          <i className="g-bar" />
          <i className="g-bar" />
          <i className="g-bar" />
        </>
      ) : null}

      {motion === "sweep" ? (
        <>
          <i className="g-orbit" />
          <i className="g-hand" />
        </>
      ) : null}

      {motion === "scan" ? (
        <>
          <i className="g-track" />
          <i className="g-scanline" />
        </>
      ) : null}

      {motion === "travel" ? (
        <>
          <i className="g-track" />
          <i className="g-traveller" />
        </>
      ) : null}

      {motion === "ring" ? (
        <svg viewBox="0 0 24 24" className="g-ring">
          <circle className="g-ring-track" cx="12" cy="12" r="9" />
          <circle className="g-ring-arc" cx="12" cy="12" r="9" />
        </svg>
      ) : null}

      {motion === "caret" ? (
        <>
          <i className="g-line" />
          <i className="g-caret" />
        </>
      ) : null}

      {motion === "halo" ? (
        <>
          <i className="g-halo" />
          <i className="g-core" />
        </>
      ) : null}

      {motion === "still" ? <i className="g-core" /> : null}
    </span>
  );
}

/**
 * The full indicator: glyph, what it is doing, and how long it has been doing it.
 *
 * The elapsed time is not decoration. It is the only thing that distinguishes "this is
 * working" from "this is stuck", and without it every long turn feels like a hang —
 * which was most of the complaint about the app being slow in the first place.
 */
export function PhaseIndicator({
  phase,
  label,
  since,
  compact = false,
}: {
  phase: AgentPhase | null | undefined;
  /** The engine's own words, which name the target as well as the verb. */
  label?: string | null;
  /** Epoch ms the phase started, for the elapsed counter. */
  since?: number | null;
  compact?: boolean;
}) {
  const spec = phaseSpec(phase);
  const text = label?.trim() || spec.label;
  const elapsed = since ? (Date.now() - since) / 1000 : null;

  return (
    <span
      className={`phase-indicator tone-${spec.tone}${compact ? " compact" : ""}`}
      role="status"
      aria-live="polite"
    >
      <PhaseGlyph phase={phase} size={compact ? 12 : 14} />
      <span className="phase-text">{text}</span>
      {elapsed !== null && elapsed >= 0.5 ? (
        <span className="phase-elapsed" aria-hidden="true">
          {formatElapsed(elapsed)}
        </span>
      ) : null}
    </span>
  );
}

/** Seconds below a minute, then minutes — precision nobody needs is just noise. */
function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds.toFixed(1)}s`;
  const minutes = Math.floor(seconds / 60);
  return `${minutes}m ${Math.floor(seconds % 60)}s`;
}
