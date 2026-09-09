import DOMPurify from "dompurify";
import { marked } from "marked";
import { useMemo } from "react";
import { requestOpenWorkspaceFile } from "../workbench/openFileRequest";
import { workspaceMarkdownTarget } from "./workspaceMarkdownLink";

/** Renders model markdown as sanitized HTML. Content never contains scripts. */
marked.setOptions({ gfm: true, breaks: true });

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

marked.use({
  renderer: {
    code({ text, lang }: { text: string; lang?: string }) {
      const language = (lang ?? "").trim().split(/\s+/)[0] ?? "";
      const label = language || "code";
      return `<div class="md-code"><div class="md-code-bar"><span class="md-code-lang">${escapeHtml(label)}</span><button type="button" class="md-code-copy" aria-label="Copy code">Copy</button></div><pre><code>${escapeHtml(text)}</code></pre></div>`;
    },
  },
});

function copyCodeBlock(button: HTMLElement) {
  const block = button.closest(".md-code");
  const text = block?.querySelector("pre")?.textContent ?? "";
  if (!text) return;
  void navigator.clipboard?.writeText(text).then(
    () => {
      button.textContent = "Copied";
      window.setTimeout(() => {
        if (button.textContent === "Copied") button.textContent = "Copy";
      }, 1200);
    },
    () => undefined,
  );
}

export function Markdown({ text, workspaceRoot }: { text: string; workspaceRoot?: string }) {
  const html = useMemo(() => {
    const raw = marked.parse(text, { async: false }) as string;
    return DOMPurify.sanitize(raw, {
      FORBID_TAGS: ["style", "iframe", "form", "input"],
      FORBID_ATTR: ["style"],
      ADD_TAGS: ["button"],
      ADD_ATTR: ["aria-label", "type"],
    });
  }, [text]);

  return (
    <div
      className="assistant-content"
      onClick={(event) => {
        const copyButton = (event.target as HTMLElement).closest(".md-code-copy");
        if (copyButton instanceof HTMLElement) {
          event.preventDefault();
          copyCodeBlock(copyButton);
          return;
        }
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
