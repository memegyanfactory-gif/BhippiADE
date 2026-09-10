/**
 * Which chats belong in the studio's tab strip.
 *
 * Conversations are per project, so the strip shows exactly the chats of the project the
 * studio is looking at — nothing from a sibling game, and nothing that is not a chat. The
 * selection is a pure function rather than an inline `filter` for two reasons: the path
 * comparison is the app's own rule (a Windows path can arrive verbatim-prefixed, in either
 * slash and in either case, and still name the same project), and the order has to be
 * *stable*. A tab that jumps to the front the moment you type in it is not a tab.
 *
 * The order is therefore oldest-created first, so a new chat appears at the right-hand end
 * next to the `+` and stays where the user left it, with the id as a tie-break so two
 * sessions created in the same millisecond still sort deterministically.
 */

// The real extension: this module is imported by the Node test harness as well as by Vite.
import { samePath } from "../lib/gameCards.ts";

/** The shape of a session this module needs — structurally satisfied by `WorkspaceSession`. */
export interface ChatTabSession {
  id: string;
  project_path: string;
  /** `SessionKind`: only `"ai_chat"` rows are chats; `"cli"` rows are shells. */
  kind: string;
  title: string;
  created_at: string;
}

/** The `SessionKind` that means "a conversation with a model". */
export const CHAT_KIND = "ai_chat";

/**
 * The chat sessions of one project, in the order the strip should draw them.
 *
 * Returns a new array; the input is never mutated. An empty `projectPath` selects
 * nothing, because a path that names no project is not equal to any other.
 */
export function chatTabsFor<T extends ChatTabSession>(
  sessions: readonly T[] | null | undefined,
  projectPath: string | null | undefined,
): T[] {
  if (!sessions || sessions.length === 0) return [];
  return sessions
    .filter((session) => session.kind === CHAT_KIND && samePath(session.project_path, projectPath))
    .sort((a, b) => {
      if (a.created_at !== b.created_at) return a.created_at < b.created_at ? -1 : 1;
      return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
    });
}

/** What a tab is labelled: the session's own title, or the placeholder for an unnamed one. */
export function chatTabTitle(title: string | null | undefined): string {
  const trimmed = (title ?? "").trim();
  return trimmed.length > 0 ? trimmed : "New chat";
}
