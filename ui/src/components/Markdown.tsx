import DOMPurify from "dompurify";
import { useMemo } from "react";
import { marked } from "marked";
import { requestOpenWorkspaceFile } from "../workbench/openFileRequest";
import { workspaceMarkdownTarget } from "./workspaceMarkdownLink";

/** Renders model markdown as sanitized HTML. Content never contains scripts. */
marked.setOptions({ gfm: true, breaks: true });

export function Markdown({ text, workspaceRoot }: { text: string; workspaceRoot?: string }) {
  const html = useMemo(() => {
    const raw = marked.parse(text, { async: false }) as string;
    return DOMPurify.sanitize(raw, {
      FORBID_TAGS: ["style", "iframe", "form", "input"],
      FORBID_ATTR: ["style"],
    });
  }, [text]);

  return (
    <div
      className="assistant-content"
      onClick={(event) => {
        if (!workspaceRoot) return;
        const target = event.target as HTMLElement;
        const anchor = target.closest("a");
        const href = anchor?.getAttribute("href");
        if (!href) return;
        const workspaceTarget = workspaceMarkdownTarget(href, workspaceRoot);
        if (!workspaceTarget) return;
        event.preventDefault();
        requestOpenWorkspaceFile(workspaceTarget.path, workspaceTarget.line);
      }}
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
