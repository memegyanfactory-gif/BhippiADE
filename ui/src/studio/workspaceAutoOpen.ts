/**
 * The workspace opens itself (ADR-0045).
 *
 * The engine is not a button. When the Studio comes up on a Godot project — and again
 * every time the active project changes — the Godot editor belongs in the viewport
 * without anyone asking for it. What the page owns is only *when* to ask Rust; the
 * switching, the closing of the outgoing engine and every refusal live in `godot_embed`.
 *
 * Two things this must not do, which is why the decision is a function and not an effect
 * body: ask twice for the same project while nothing has changed (the state event fires on
 * every layout change), and undo a deliberate "Close workspace" by reopening it a frame
 * later. Both fall out of one rule — **a project is settled while it is the active
 * project**, whether that ended in a call, a refusal, or a workspace that was already open.
 *
 * The rule used to be "settled for the rest of the session", which is why A → B → A left A
 * with an empty viewport for good: A was in the settled set, so coming back to it decided
 * nothing. The state is therefore one key, not a set. Changing the active project clears
 * the suppression, so the viewport always follows the project; staying on a project keeps
 * it, so a close the user asked for is never undone.
 */

// The real extension: this module is imported by the Node test harness as well as by Vite.
import { projectKey } from "../lib/gameCards.ts";

export interface AutoOpenState {
  workspace: { project: string } | null;
}

export interface AutoOpenInput {
  /** The active project's display path, or `""` when there is none. */
  projectPath: string;
  /** The embed state, or `null` while `godot_embed_state()` has not answered yet. */
  embed: AutoOpenState | null;
  /** The project settled so far, as a [[projectKey]] key, or `null` at mount. */
  settled: string | null;
}

export interface AutoOpenDecision {
  /** The path to hand `godot_embed_open_workspace`, or `null` to do nothing. */
  open: string | null;
  /** The key to record as settled, or `null` when there is nothing to decide yet. */
  remember: string | null;
}

const NOTHING: AutoOpenDecision = { open: null, remember: null };

/** Whether the viewport already holds a live workspace for this project. */
export function workspaceHolds(embed: AutoOpenState | null, projectPath: string): boolean {
  const key = projectKey(projectPath);
  const held = projectKey(embed?.workspace?.project);
  return key.length > 0 && key === held;
}

/**
 * What the studio should do about the workspace right now.
 *
 * Nothing at all until the embed state is known: opening before Rust has said what is
 * already in the hole is how a project ends up with two editors racing over its import
 * cache.
 */
export function decideAutoOpen({ projectPath, embed, settled }: AutoOpenInput): AutoOpenDecision {
  if (embed === null) return NOTHING;
  const key = projectKey(projectPath);
  if (key.length === 0) return NOTHING;
  // Already decided for the project on screen. A different project is a new decision.
  if (settled === key) return NOTHING;
  // Already open — settle it so a later "Close workspace" is not undone by this effect.
  if (workspaceHolds(embed, projectPath)) return { open: null, remember: key };
  return { open: projectPath, remember: key };
}
