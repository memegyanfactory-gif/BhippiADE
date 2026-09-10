import type { WorkspaceSession } from "../lib/ipc";
import { samePath } from "../lib/gameCards.ts";

/** Worker chats of this project (spawned by a team lead). */
export function teamWorkers(
  sessions: readonly WorkspaceSession[],
  projectPath: string,
  leadId: string | null,
): WorkspaceSession[] {
  return sessions.filter((session) => {
    if (!samePath(session.project_path, projectPath)) return false;
    if (!session.parent_id) return false;
    if (leadId && session.parent_id !== leadId) return false;
    return true;
  });
}
