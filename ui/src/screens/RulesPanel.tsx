import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { IconClose, IconRules } from "../components/icons";

const PLACEHOLDER = `# Project rules

- Match the conventions already in this codebase.
- Prefer small, reviewable changes over sweeping rewrites.
- Ask before changing the database schema.
`;

/**
 * Standing instructions for the agent in this project.
 *
 * The text is stored as `.bhippi/rules.md` **inside the project folder**, not in
 * Bhippi's own config, so it travels with the repository, can be committed, reviewed,
 * and edited outside the app — and so switching projects genuinely switches rules
 * rather than carrying one project's conventions into another.
 *
 * Rust reads the same file when it assembles a turn's system prompt, so what this panel
 * shows is exactly what the agent is told (`prompts/chat-rules.md`).
 */
export function RulesPanel({ onClose }: { onClose: () => void }) {
  const [text, setText] = useState("");
  const [saved, setSaved] = useState("");
  const [path, setPath] = useState(".bhippi/rules.md");
  const [status, setStatus] = useState<"loading" | "ready" | "saving">("loading");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .projectRules()
      .then((rules) => {
        setText(rules.text);
        setSaved(rules.text);
        setPath(rules.path);
        setStatus("ready");
      })
      .catch((loadError) => {
        setError(String((loadError as { message?: string }).message ?? loadError));
        setStatus("ready");
      });
  }, []);

  useEffect(() => {
    const escape = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [onClose]);

  const save = async () => {
    setStatus("saving");
    setError(null);
    try {
      const rules = await api.saveProjectRules(text);
      setSaved(rules.text);
      setPath(rules.path);
    } catch (saveError) {
      setError(String((saveError as { message?: string }).message ?? saveError));
    } finally {
      setStatus("ready");
    }
  };

  const dirty = text !== saved;

  return (
    <div className="dialog-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <div className="rules-dialog" role="dialog" aria-label="Project rules" aria-modal="true">
        <header>
          <span className="rules-mark">
            <IconRules size={15} />
          </span>
          <span>
            <strong>Project rules</strong>
            <small>{path}</small>
          </span>
          <span className="grow" />
          <button onClick={onClose} aria-label="Close">
            <IconClose size={13} />
          </button>
        </header>

        <p className="rules-blurb">
          Standing instructions for the agent in this project. They are sent with every
          turn here, and never widen what the agent may read, run, or reach.
        </p>

        {error ? (
          <div className="project-error" role="alert">
            {error}
          </div>
        ) : null}

        <textarea
          className="rules-input"
          value={text}
          spellCheck={false}
          disabled={status === "loading"}
          placeholder={PLACEHOLDER}
          aria-label="Project rules, Markdown"
          onChange={(event) => setText(event.target.value)}
        />

        <footer>
          <span className="rules-count">{text.trim() ? `${text.trim().length} characters` : "No rules yet"}</span>
          <span className="grow" />
          <button className="btn-ghost" onClick={onClose}>
            Close
          </button>
          <button className="project-primary" onClick={() => void save()} disabled={!dirty || status !== "ready"}>
            {status === "saving" ? "Saving…" : dirty ? "Save rules" : "Saved"}
          </button>
        </footer>
      </div>
    </div>
  );
}
