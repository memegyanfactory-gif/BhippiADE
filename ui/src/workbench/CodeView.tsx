import { useEffect, useMemo, useRef, useState } from "react";
import type { WorkspaceFile } from "../lib/ipc";
import { bytes as formatBytes } from "../lib/format";
import { tokenizeLines } from "./highlight";
import { ImageViewer, isImageFile } from "./ImageViewer";
import { IconClose, IconSave } from "../components/icons";

/**
 * The editor pane: a highlighted read view with a transparent textarea over it.
 *
 * The overlay approach is what keeps this small. A `<textarea>` gives real caret
 * behavior, selection, undo, IME, and accessibility for free; the coloured copy sits
 * directly behind it in the same font metrics, so the two scroll and wrap as one. The
 * alternative — a contenteditable with hand-rolled key handling — is where in-house
 * editors go to die.
 */
export function CodeView({
  file,
  dirty,
  saving,
  error,
  focusLine,
  onChange,
  onSave,
  onClose,
}: {
  file: WorkspaceFile | null;
  dirty: boolean;
  saving: boolean;
  error: string | null;
  focusLine?: number | null;
  onChange: (text: string) => void;
  onSave: () => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState(file?.text ?? "");
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const areaRef = useRef<HTMLTextAreaElement | null>(null);

  useEffect(() => {
    setDraft(file?.text ?? "");
  }, [file?.path, file?.text]);

  useEffect(() => {
    if (!file || !focusLine || !file.editable) return;
    const area = areaRef.current;
    if (!area) return;
    const rows = (file.text ?? "").split("\n");
    const line = Math.max(1, Math.min(focusLine, rows.length));
    const offset = rows.slice(0, line - 1).reduce((total, row) => total + row.length + 1, 0);
    area.focus();
    area.setSelectionRange(offset, offset + (rows[line - 1]?.length ?? 0));
    area.scrollTop = Math.max(0, (line - 3) * 20);
    scrollRef.current?.scrollTo({ top: Math.max(0, (line - 3) * 20) });
  }, [file, focusLine]);

  const lines = useMemo(() => draft.split("\n"), [draft]);
  const tokens = useMemo(
    () => tokenizeLines(lines, file?.language ?? ""),
    [lines, file?.language],
  );

  // Ctrl/Cmd+S saves, the same chord it is everywhere else. It is scoped to the pane
  // rather than the window so it cannot steal the shortcut while the chat has focus.
  const onKeyDown = (event: React.KeyboardEvent) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
      event.preventDefault();
      if (dirty && file?.editable) onSave();
      return;
    }
    if (event.key === "Tab") {
      event.preventDefault();
      const area = areaRef.current;
      if (!area) return;
      const { selectionStart, selectionEnd } = area;
      const next = `${draft.slice(0, selectionStart)}  ${draft.slice(selectionEnd)}`;
      setDraft(next);
      onChange(next);
      requestAnimationFrame(() => {
        area.selectionStart = selectionStart + 2;
        area.selectionEnd = selectionStart + 2;
      });
    }
  };

  if (!file) {
    return (
      <div className="code-empty">
        <div className="code-empty-mark" aria-hidden="true">
          <span />
          <span />
          <span />
        </div>
        <strong>No file open</strong>
        <p>Pick a file from the explorer to read or edit it here.</p>
      </div>
    );
  }

  const crumbs = file.path.split("/");

  return (
    <div className="code-view">
      <div className="code-tabs">
        <div className={`code-tab active${dirty ? " dirty" : ""}`}>
          <span className="code-tab-name">{file.name}</span>
          {dirty ? <i className="code-tab-dot" aria-label="Unsaved changes" /> : null}
          <button onClick={onClose} aria-label={`Close ${file.name}`}>
            <IconClose size={11} />
          </button>
        </div>
        <span className="grow" />
        {file.editable ? (
          <button
            className={`code-save${dirty ? " armed" : ""}`}
            onClick={onSave}
            disabled={!dirty || saving}
            title="Save (Ctrl+S)"
          >
            <IconSave size={13} /> {saving ? "Saving…" : dirty ? "Save" : "Saved"}
          </button>
        ) : null}
      </div>

      <div className="code-crumbs" aria-label="File path">
        {crumbs.map((crumb, index) => (
          <span key={`${crumb}-${index}`}>
            {crumb}
            {index < crumbs.length - 1 ? <i aria-hidden="true">/</i> : null}
          </span>
        ))}
        <span className="grow" />
        <span className="code-meta">
          {formatBytes(file.bytes)} · {lines.length} lines
        </span>
      </div>

      {error ? (
        <div className="code-error" role="alert">
          {error}
        </div>
      ) : null}

      {file.editable ? (
        <div className="code-surface" ref={scrollRef}>
          <div className="code-gutter" aria-hidden="true">
            {lines.map((_, index) => (
              <span key={index}>{index + 1}</span>
            ))}
          </div>
          <div className="code-body">
            <pre className="code-paint" aria-hidden="true">
              {tokens.map((row, index) => (
                <div className="code-line" key={index}>
                  {row.length === 0 ? (
                    "\n"
                  ) : (
                    <>
                      {row.map((token, position) => (
                        <span key={position} className={`tok-${token.kind}`}>
                          {token.text}
                        </span>
                      ))}
                      {"\n"}
                    </>
                  )}
                </div>
              ))}
            </pre>
            <textarea
              ref={areaRef}
              className="code-input"
              value={draft}
              spellCheck={false}
              wrap="off"
              aria-label={`Contents of ${file.name}`}
              onKeyDown={onKeyDown}
              onChange={(event) => {
                setDraft(event.target.value);
                onChange(event.target.value);
              }}
            />
          </div>
        </div>
      ) : file.content_base64 && isImageFile(file.language) ? (
        <ImageViewer
          contentBase64={file.content_base64}
          name={file.name}
          bytes={file.bytes}
          language={file.language}
        />
      ) : (
        <div className="code-empty">
          <strong>{file.truncated ? "Too large to open here" : "Not a text file"}</strong>
          <p>
            {file.truncated
              ? `${formatBytes(file.bytes)} — Bhippi opens files up to 1 MB. Use an external editor for this one.`
              : "This file is binary, so there is nothing to show as text."}
          </p>
        </div>
      )}
    </div>
  );
}
