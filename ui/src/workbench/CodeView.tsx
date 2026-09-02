import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { tokenizeLines } from "./highlight";
import {
  caretPosition,
  findMatches,
  type FindOptions,
  type Selection,
} from "./editorModel";
import type { Tab } from "./editorTabs";
import { IconClose } from "../components/icons";

/**
 * VS Code–class editor pane with tab bar, find/replace, minimap, and status bar.
 *
 * The textarea-overlay approach is preserved: real caret, selection, undo, IME,
 * accessibility for free. This component adds the surrounding chrome that makes
 * the editor feel like a real IDE.
 */

/* ── Find / Replace Widget ──────────────────────────────────────────────── */

function FindWidget({
  text,
  onJump,
  onClose,
  onReplace,
}: {
  text: string;
  onJump: (selection: Selection) => void;
  onClose: () => void;
  onReplace: (from: Selection, to: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [replaceValue, setReplaceValue] = useState("");
  const [showReplace, setShowReplace] = useState(false);
  const [options, setOptions] = useState<FindOptions>({
    caseSensitive: false,
    wholeWord: false,
    regex: false,
  });
  const [current, setCurrent] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const matches = useMemo(() => findMatches(text, query, options), [text, query, options]);
  const count = matches.length;

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    setCurrent(0);
  }, [query, options]);

  const jump = useCallback(
    (index: number) => {
      if (index >= 0 && index < matches.length) {
        setCurrent(index);
        onJump(matches[index]);
      }
    },
    [matches, onJump],
  );

  const next = useCallback(() => jump((current + 1) % Math.max(count, 1)), [current, count, jump]);
  const prev = useCallback(
    () => jump((current - 1 + count) % Math.max(count, 1)),
    [current, count, jump],
  );

  const replace = useCallback(() => {
    if (current >= 0 && current < matches.length) {
      onReplace(matches[current], replaceValue);
    }
  }, [current, matches, replaceValue, onReplace]);

  const replaceAll = useCallback(() => {
    // Replace all occurrences back-to-front so offsets stay valid.
    for (let i = matches.length - 1; i >= 0; i--) {
      onReplace(matches[i], replaceValue);
    }
  }, [matches, replaceValue, onReplace]);

  const toggle = (key: keyof FindOptions) => setOptions((o) => ({ ...o, [key]: !o[key] }));

  const toggleReplace = () => setShowReplace((v) => !v);

  return (
    <div className="find-widget">
      <div className="find-main-row">
        <button className="find-chevron" onClick={toggleReplace} aria-label="Toggle replace" title="Toggle replace">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" style={{ transform: showReplace ? "rotate(90deg)" : "none", transition: "transform 120ms ease" }}>
            <path d="M6 4l4 4-4 4" />
          </svg>
        </button>
        <div className="find-input-wrap">
          <input
            ref={inputRef}
            className="find-input"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); next(); }
              if (e.key === "Enter" && e.shiftKey) { e.preventDefault(); prev(); }
              if (e.key === "Escape") { e.preventDefault(); onClose(); }
            }}
            placeholder="Find"
            spellCheck={false}
          />
          <span className="find-count">{count > 0 ? `${current + 1}/${count}` : count === 0 && query ? "No results" : ""}</span>
        </div>
        <button className={`find-opt${options.caseSensitive ? " on" : ""}`} onClick={() => toggle("caseSensitive")} title="Match Case (Alt+C)">
          <b>Aa</b>
        </button>
        <button className={`find-opt${options.wholeWord ? " on" : ""}`} onClick={() => toggle("wholeWord")} title="Match Whole Word (Alt+W)">
          <b>Ab</b>
        </button>
        <button className={`find-opt${options.regex ? " on" : ""}`} onClick={() => toggle("regex")} title="Use Regular Expression (Alt+R)">
          <b>.*</b>
        </button>
        <div className="find-nav">
          <button onClick={prev} title="Previous Match (Shift+Enter)" aria-label="Previous match">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M4 10l4-4 4 4" /></svg>
          </button>
          <button onClick={next} title="Next Match (Enter)" aria-label="Next match">
            <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M4 6l4 4 4-4" /></svg>
          </button>
        </div>
        <button className="find-close" onClick={onClose} aria-label="Close find">
          <IconClose size={10} />
        </button>
      </div>
      {showReplace ? (
        <div className="find-replace-row">
          <span style={{ width: 28 }} />
          <input
            className="find-input"
            value={replaceValue}
            onChange={(e) => setReplaceValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") { e.preventDefault(); replace(); }
              if (e.key === "Escape") { e.preventDefault(); onClose(); }
            }}
            placeholder="Replace"
            spellCheck={false}
          />
          <button onClick={replace} title="Replace (Enter)" className="find-replace-btn">Replace</button>
          <button onClick={replaceAll} title="Replace All" className="find-replace-btn">All</button>
        </div>
      ) : null}
    </div>
  );
}

/* ── Minimap ────────────────────────────────────────────────────────────── */

function Minimap({
  lines,
  scrollTop,
  viewportHeight,
  lineCount,
}: {
  lines: string[];
  scrollTop: number;
  viewportHeight: number;
  lineCount: number;
}) {
  const scale = 3;
  const lineHeight = 2;
  const totalHeight = lineCount * lineHeight;
  const thumbHeight = Math.max(20, (viewportHeight / (lineCount * 20)) * totalHeight);
  const thumbTop = lineCount > 0 ? (scrollTop / (lineCount * 20)) * totalHeight : 0;

  // Render up to 300 lines max for perf.
  const visibleCount = Math.min(lineCount, 300);

  return (
    <div className="minimap" aria-hidden="true">
      <canvas
        className="minimap-canvas"
        width={80}
        height={visibleCount * lineHeight}
        style={{ width: 80 / scale, height: (visibleCount * lineHeight) / scale }}
      >
        {/* We use a simple canvas-free approach: just thin bars. */}
      </canvas>
      <div className="minimap-lines">
        {lines.slice(0, visibleCount).map((line, i) => {
          const hasContent = line.trim().length > 0;
          const indent = line.length - line.trimStart().length;
          const contentWidth = hasContent ? Math.max(4, line.trim().length * 0.55) : 0;
          return (
            <div
              key={i}
              className="minimap-line"
              style={{ height: lineHeight, paddingLeft: indent * 0.55, width: contentWidth }}
            />
          );
        })}
      </div>
      <div
        className="minimap-thumb"
        style={{ top: thumbTop, height: thumbHeight }}
      />
    </div>
  );
}

/* ── Status Bar ─────────────────────────────────────────────────────────── */

function StatusBar({
  line,
  column,
  language,
  indent,
  eol,
  encoding,
}: {
  line: number;
  column: number;
  language: string;
  indent: string;
  eol: "LF" | "CRLF";
  encoding: string;
}) {
  return (
    <div className="status-bar" aria-label="Editor status">
      <div className="status-bar-left">
        <span className="status-item branch" title="Current branch">
          <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor"><path d="M14 11a2 2 0 1 1-3.999.001A2 2 0 0 1 14 11zm-2 0a.5.5 0 0 0-.5-.5h-1V9a.5.5 0 0 0-1 0v1.5h-1V8.5a.5.5 0 0 0-1 0V10H7a.5.5 0 0 0 0 1h2.5v1.5a.5.5 0 0 0 1 0V11h1a.5.5 0 0 0 .5-.5zM2 4a2 2 0 1 1 3.999.001A2 2 0 0 1 2 4zm2 0a.5.5 0 0 0-.5-.5H1a.5.5 0 0 0 0 1h.5V6H1a.5.5 0 0 0 0 1h1.5v1.5a.5.5 0 0 0 1 0V7H5a.5.5 0 0 0 0-1H3.5V4.5a.5.5 0 0 0-.5-.5z" /></svg>
          main
        </span>
      </div>
      <div className="status-bar-right">
        <span className="status-item" title="Line ending">{eol}</span>
        <span className="status-item" title="Encoding">{encoding}</span>
        <span className="status-item" title="Indent">{indent}</span>
        <span className="status-item" title="Language">{language || "Plain Text"}</span>
        <span className="status-item" title="Position">{line}:{column}</span>
      </div>
    </div>
  );
}

/* ── Main CodeView ──────────────────────────────────────────────────────── */

export function CodeView({
  tabs,
  activeTab,
  focusLine,
  onChange,
  onSave,
  onCloseTab,
  onSwitchTab,
  onPinTab,
  onReorderTab,
  onUndo,
  onRedo,
}: {
  tabs: Tab[];
  activeTab: Tab | null;
  focusLine?: number | null;
  onChange: (text: string) => void;
  onSave: () => void;
  onCloseTab: (id: string) => void;
  onSwitchTab: (index: number) => void;
  onPinTab: (id: string) => void;
  onReorderTab: (from: number, to: number) => void;
  onUndo: () => void;
  onRedo: () => void;
}) {
  const [draft, setDraft] = useState(activeTab?.text ?? "");
  const [showFind, setShowFind] = useState(false);
  const [showMinimap, setShowMinimap] = useState(true);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const areaRef = useRef<HTMLTextAreaElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);

  const file = activeTab;

  useEffect(() => {
    setDraft(file?.text ?? "");
  }, [file?.id, file?.text]);

  // Focus line on first open.
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

  // Track scroll for minimap.
  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (el) setScrollTop(el.scrollTop);
  }, []);

  const caretPos = useMemo(() => {
    if (!file) return { line: 1, column: 1 };
    const area = areaRef.current;
    return caretPosition(draft, area?.selectionStart ?? 0);
  }, [draft, file?.id]);

  // Full VS Code keymap.
  const onKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      const mod = event.ctrlKey || event.metaKey;

      // Ctrl+S — save.
      if (mod && event.key.toLowerCase() === "s") {
        event.preventDefault();
        if (file?.dirty && file?.editable) onSave();
        return;
      }

      // Ctrl+Z / Ctrl+Y — undo/redo.
      if (mod && !event.shiftKey && event.key.toLowerCase() === "z") {
        event.preventDefault();
        onUndo();
        return;
      }
      if (mod && (event.key.toLowerCase() === "y" || (event.shiftKey && event.key.toLowerCase() === "z"))) {
        event.preventDefault();
        onRedo();
        return;
      }

      // Ctrl+F — find.
      if (mod && event.key.toLowerCase() === "f") {
        event.preventDefault();
        setShowFind((v) => !v);
        return;
      }

      // Ctrl+H — find & replace.
      if (mod && event.key.toLowerCase() === "h") {
        event.preventDefault();
        setShowFind(true);
        return;
      }

      // Ctrl+W — close tab.
      if (mod && event.key.toLowerCase() === "w") {
        event.preventDefault();
        if (file) onCloseTab(file.id);
        return;
      }

      // Ctrl+Tab / Ctrl+Shift+Tab — next/prev tab.
      if (mod && event.key === "Tab") {
        event.preventDefault();
        if (event.shiftKey) {
          const idx = tabs.findIndex((t) => t.id === file?.id);
          const prev = (idx - 1 + tabs.length) % Math.max(tabs.length, 1);
          onSwitchTab(prev);
        } else {
          const idx = tabs.findIndex((t) => t.id === file?.id);
          const next = (idx + 1) % Math.max(tabs.length, 1);
          onSwitchTab(next);
        }
        return;
      }

      // Ctrl+PageUp / PageDown — prev/next tab.
      if (mod && event.key === "PageUp") {
        event.preventDefault();
        const idx = tabs.findIndex((t) => t.id === file?.id);
        onSwitchTab(Math.max(0, idx - 1));
        return;
      }
      if (mod && event.key === "PageDown") {
        event.preventDefault();
        const idx = tabs.findIndex((t) => t.id === file?.id);
        onSwitchTab(Math.min(tabs.length - 1, idx + 1));
        return;
      }

      // Escape — close find widget.
      if (event.key === "Escape" && showFind) {
        event.preventDefault();
        setShowFind(false);
        return;
      }

      // Tab — insert two spaces (using indent style).
      if (event.key === "Tab" && !mod) {
        event.preventDefault();
        const area = areaRef.current;
        if (!area) return;
        const { selectionStart, selectionEnd } = area;
        const unit = file?.indentStyle?.useTabs ? "\t" : " ".repeat(file?.indentStyle?.size ?? 2);
        const next = `${draft.slice(0, selectionStart)}${unit}${draft.slice(selectionEnd)}`;
        setDraft(next);
        onChange(next);
        requestAnimationFrame(() => {
          area.selectionStart = selectionStart + unit.length;
          area.selectionEnd = selectionStart + unit.length;
        });
      }

      // F11 — toggle minimap.
      if (event.key === "F11") {
        event.preventDefault();
        setShowMinimap((v) => !v);
      }
    },
    [draft, file, onSave, onUndo, onRedo, onCloseTab, onSwitchTab, tabs, showFind, onChange],
  );

  if (!file) {
    return (
      <div className="code-view">
        <div className="code-tabs">
          <span className="code-tabs-empty" />
        </div>
        <div className="code-empty">
          <div className="code-empty-mark" aria-hidden="true">
            <span /><span /><span />
          </div>
          <strong>No file open</strong>
          <p>Pick a file from the explorer to read or edit it here.</p>
        </div>
      </div>
    );
  }

  const crumbs = file.path.split("/");
  const indentDisplay = file.indentStyle?.useTabs ? "Tab Size: 4" : `Spaces: ${file.indentStyle?.size ?? 2}`;

  return (
    <div className="code-view">
      {/* ── Tab Bar ── */}
      <div className="code-tabs">
        {tabs.map((tab, i) => (
          <div
            key={tab.id}
            className={`code-tab${tab.id === file.id ? " active" : ""}${tab.preview ? " preview" : ""}${tab.dirty ? " dirty" : ""}`}
            onClick={() => onSwitchTab(i)}
            onDoubleClick={() => onPinTab(tab.id)}
            draggable
            onDragStart={(e) => e.dataTransfer.setData("text/tab-index", String(i))}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              const from = Number(e.dataTransfer.getData("text/tab-index"));
              if (!isNaN(from)) onReorderTab(from, i);
            }}
          >
            <span className="code-tab-name">{tab.name}</span>
            {tab.dirty ? <i className="code-tab-dot" aria-label="Unsaved changes" /> : null}
            <button
              className="code-tab-close"
              onClick={(e) => { e.stopPropagation(); onCloseTab(tab.id); }}
              aria-label={`Close ${tab.name}`}
            >
              <IconClose size={10} />
            </button>
          </div>
        ))}
        <span className="grow" />
        {file.editable ? (
          <button
            className={`code-save${file.dirty ? " armed" : ""}`}
            onClick={onSave}
            disabled={!file.dirty || file.saving}
            title="Save (Ctrl+S)"
          >
            {file.saving ? "Saving…" : file.dirty ? "Save" : "Saved"}
          </button>
        ) : null}
      </div>

      {/* ── Breadcrumbs ── */}
      <div className="code-crumbs" aria-label="File path">
        {crumbs.map((crumb, index) => (
          <span key={`${crumb}-${index}`}>
            {crumb}
            {index < crumbs.length - 1 ? <i aria-hidden="true">/</i> : null}
          </span>
        ))}
        <span className="grow" />
        <span className="code-meta">
          {file.bytes.toLocaleString()} bytes · {lines.length} lines
        </span>
      </div>

      {/* ── Find Widget ── */}
      {showFind ? (
        <FindWidget
          text={draft}
          onJump={(sel) => {
            const area = areaRef.current;
            if (!area) return;
            area.focus();
            area.setSelectionRange(sel.start, sel.end);
            // Scroll the match into view.
            const rows = draft.slice(0, sel.start).split("\n");
            const lineNum = rows.length;
            scrollRef.current?.scrollTo({ top: Math.max(0, (lineNum - 3) * 20) });
          }}
          onClose={() => setShowFind(false)}
          onReplace={(sel, val) => {
            const next = draft.slice(0, sel.start) + val + draft.slice(sel.end);
            setDraft(next);
            onChange(next);
          }}
        />
      ) : null}

      {/* ── Editor Surface ── */}
      <div className="code-surface" ref={scrollRef} onScroll={onScroll}>
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
            onScroll={onScroll}
            onChange={(event) => {
              setDraft(event.target.value);
              onChange(event.target.value);
            }}
          />
        </div>
        {showMinimap && lines.length > 0 ? (
          <Minimap
            lines={lines}
            scrollTop={scrollTop}
            viewportHeight={scrollRef.current?.clientHeight ?? 400}
            lineCount={lines.length}
          />
        ) : null}
      </div>

      {/* ── Status Bar ── */}
      <StatusBar
        line={caretPos.line}
        column={caretPos.column}
        language={file.language}
        indent={indentDisplay}
        eol={file.eol ?? "LF"}
        encoding="UTF-8"
      />
    </div>
  );
}
