/**
 * How a recorded step is read: which file it touched, which verb describes it, and which
 * activity row it belongs in (CHT-110).
 *
 * Extracted from `ActivityDock.tsx` because it is logic, not layout — the Activity Dock and
 * the chat transcript must classify a step identically, and a pure module is the only way
 * either of them can be tested without a DOM.
 */

export function getFileBadge(pathOrDetail: string) {
  // Same glyph engine as the file tree, so the badge always matches the workspace.
  const match = pathOrDetail.match(/([a-zA-Z0-9_\-./\\]+\.[a-zA-Z0-9]+)/);
  return { name: match ? match[1].split(/[/\\]/).pop() ?? match[1] : "workspace", label: "File" };
}

export function parseToolTarget(title: string, detail: string) {
  const lineMatch =
    title.match(/[:#]L?(\d+(?:-\d+)?)/i) || detail.match(/[:#]L?(\d+(?:-\d+)?)/i);
  const lineRange = lineMatch ? `#L${lineMatch[1]}` : null;

  const pathMatch =
    title.match(/([a-zA-Z0-9_\-./\\]+\.[a-zA-Z0-9]+)/) ||
    detail.match(/([a-zA-Z0-9_\-./\\]+\.[a-zA-Z0-9]+)/);
  let fileName = pathMatch ? pathMatch[1].split(/[/\\]/).pop() : null;
  if (!fileName && title.length > 0) {
    fileName = title;
  }

  // Order matters: the more specific patterns are tested first, because a vendor tool
  // named "MultiEdit" also contains "edit" and one named "WebSearch" also contains
  // "search". Reading is last precisely because it is the vaguest.
  let verb = "Analyzed";
  if (/^test|pytest|jest|cargo test/i.test(title)) verb = "Tested";
  else if (/websearch|^grep|^glob|^search|find/i.test(title)) verb = "Searched";
  else if (/webfetch|^fetch|curl|https?:/i.test(title)) verb = "Fetched";
  else if (/^todo|^plan|^task/i.test(title)) verb = "Planned";
  else if (/^bash|^run|shell|powershell|cargo|npm|exec|cmd/i.test(title)) verb = "Ran";
  else if (/multiedit|^edit|patch|replace|modify|apply/i.test(title)) verb = "Edited";
  else if (/^write|^create|notebook/i.test(title)) verb = "Wrote";
  else if (/^read|explore|inspect|view|open/i.test(title)) verb = "Read";

  return { verb, fileName: fileName ?? "workspace", lineRange };
}
