import DOMPurify from "dompurify";
import { useMemo } from "react";
import { marked } from "marked";

/** Renders model markdown as sanitized HTML. Content never contains scripts. */
marked.setOptions({ gfm: true, breaks: true });

export function Markdown({ text }: { text: string }) {
  const html = useMemo(() => {
    const raw = marked.parse(text, { async: false }) as string;
    return DOMPurify.sanitize(raw, {
      FORBID_TAGS: ["style", "iframe", "form", "input"],
      FORBID_ATTR: ["style"],
    });
  }, [text]);

  return <div className="assistant-content" dangerouslySetInnerHTML={{ __html: html }} />;
}
