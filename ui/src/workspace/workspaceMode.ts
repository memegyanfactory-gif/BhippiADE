/**
 * Which board the project area draws.
 *
 * `single` is one session at a time behind a tab strip, `multi` tiles the active
 * project's sessions, and `multiproject` puts every project's sessions on one screen.
 *
 * Conversations are project-scoped in the engine (a cross-project read returns nothing
 * and a cross-project send is refused), so on the multi-project board only the active
 * project's column runs a live pane; the others are read-only until they are activated.
 */
export type WorkspaceMode = "single" | "multi" | "multiproject";

/** Reads a persisted mode, falling back to `single` for anything an older build wrote. */
export function readWorkspaceMode(value: string | null | undefined): WorkspaceMode {
  return value === "multi" || value === "multiproject" ? value : "single";
}
