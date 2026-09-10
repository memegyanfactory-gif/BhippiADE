import type { ProjectSummary, WorkspaceSession } from "../lib/ipc";

/** Windows paths reach the UI in several spellings; one shape compares them. */
export function cleanProjectPath(value?: string | null): string {
  if (!value) return "";
  return value
    .replace(/^(\/\/\?\/|\/\/\?|[\\/]{2}\?)[\\/]?/, "")
    .replace(/\\/g, "/")
    .replace(/\/+$/, "")
    .toLowerCase()
    .trim();
}

export type ProjectColumn = {
  project: ProjectSummary;
  sessions: WorkspaceSession[];
};

/**
 * Groups every session under the project it belongs to, one column per project in the
 * order the projects were given, each column newest-first.
 *
 * A session whose project is no longer on the list is dropped — the rail drops it too,
 * and a column with no project has no name to draw.
 */
export function groupSessionsByProject(
  projects: readonly ProjectSummary[],
  sessions: readonly WorkspaceSession[],
): ProjectColumn[] {
  const columns = new Map<string, ProjectColumn>();
  for (const project of projects) {
    columns.set(cleanProjectPath(project.path), { project, sessions: [] });
  }
  for (const session of sessions) {
    const column = columns.get(cleanProjectPath(session.project_path));
    if (column) column.sessions.push(session);
  }
  for (const column of columns.values()) {
    column.sessions.sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  }
  return [...columns.values()];
}

/**
 * Every project's sessions as one flat list, newest activity first.
 *
 * The board draws windows, not columns, so the projects stop being containers and become
 * a label on each window. Ordering is global rather than per project — the window
 * manager remembers the order the user drags them into anyway, and a fresh board should
 * open with the chat they last touched on the left.
 */
export function flattenBoardSessions(columns: readonly ProjectColumn[]): WorkspaceSession[] {
  return columns
    .flatMap((column) => column.sessions)
    .slice()
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at));
}

/**
 * A stable colour for a project, so its windows read as a set at a glance.
 *
 * The hue is derived from the path, not from the project's position, so adding or
 * forgetting a folder never re-colours the others.
 */
export function projectHue(path: string): number {
  const key = cleanProjectPath(path);
  let hash = 0;
  for (let index = 0; index < key.length; index += 1) {
    hash = (hash * 31 + key.charCodeAt(index)) % 360000;
  }
  return hash % 360;
}

/** The project a session belongs to, or `null` when that folder is no longer known. */
export function projectForSession(
  projects: readonly ProjectSummary[],
  session: WorkspaceSession,
): ProjectSummary | null {
  const key = cleanProjectPath(session.project_path);
  return projects.find((project) => cleanProjectPath(project.path) === key) ?? null;
}
