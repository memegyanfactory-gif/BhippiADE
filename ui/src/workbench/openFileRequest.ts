export const OPEN_WORKSPACE_FILE_EVENT = "bhippi:open-workspace-file";

export type OpenWorkspaceFileDetail = { path: string; line: number };

export function requestOpenWorkspaceFile(path: string, line: number) {
  window.dispatchEvent(
    new CustomEvent<OpenWorkspaceFileDetail>(OPEN_WORKSPACE_FILE_EVENT, {
      detail: { path, line },
    }),
  );
}
