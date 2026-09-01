const WORKSPACE_FRAGMENT = "#bhippi-file=";

export type WorkspaceMarkdownTarget = { path: string; line: number };

/** Resolve only the confined fragment links emitted by the engine's report renderer. */
export function workspaceMarkdownTarget(
  href: string,
  workspaceRoot: string,
): WorkspaceMarkdownTarget | null {
  if (!href.startsWith(WORKSPACE_FRAGMENT)) return null;
  const query = new URLSearchParams(href.slice(1));
  const relative = query.get("bhippi-file")?.replaceAll("\\", "/") ?? "";
  const segments = relative.split("/");
  if (
    !relative ||
    relative.startsWith("/") ||
    /^[A-Za-z]:/.test(relative) ||
    segments.some((segment) => !segment || segment === "." || segment === "..")
  ) {
    return null;
  }
  const rawLine = Number(query.get("line") ?? "1");
  if (!Number.isSafeInteger(rawLine) || rawLine < 1 || rawLine > 10_000_000) return null;
  const root = workspaceRoot.replace(/[\\/]+$/, "");
  if (!root) return null;
  return { path: `${root}/${relative}`, line: rawLine };
}
