/**
 * Editor text operations, as pure functions.
 *
 * Every editing command in the workbench is expressed here as
 * `(text, selection, …) -> { text, start, end }`. Nothing in this file touches the DOM,
 * so each command is testable on its own and the component stays a thin keymap over it.
 *
 * The pane itself is still a `<textarea>` with a coloured copy painted behind it, which
 * is what buys real caret, selection, undo, IME and screen-reader behaviour for free
 * (see CodeView). These helpers are the layer that turns that textarea into an editor.
 */

/** The result of any command: the new document and where the selection lands. */
export type Edit = { text: string; start: number; end: number };

/** How a file indents, read from the file itself rather than assumed. */
export type IndentStyle = { useTabs: boolean; size: number };

export type Selection = { start: number; end: number };

/** The line-comment token per language, keyed by the extension the backend reports. */
const LINE_COMMENTS: Record<string, string> = {
  ts: "//",
  tsx: "//",
  js: "//",
  jsx: "//",
  mjs: "//",
  cjs: "//",
  rs: "//",
  go: "//",
  java: "//",
  c: "//",
  h: "//",
  cpp: "//",
  hpp: "//",
  cs: "//",
  swift: "//",
  kt: "//",
  scala: "//",
  php: "//",
  css: null as unknown as string,
  py: "#",
  rb: "#",
  sh: "#",
  bash: "#",
  zsh: "#",
  yml: "#",
  yaml: "#",
  toml: "#",
  ini: "#",
  conf: "#",
  ps1: "#",
  r: "#",
  pl: "#",
  sql: "--",
  lua: "--",
  hs: "--",
  vim: '"',
  lisp: ";",
  clj: ";",
};

/** Pairs that close themselves as you type, and type over when you close them yourself. */
const PAIRS: Record<string, string> = { "(": ")", "[": "]", "{": "}", '"': '"', "'": "'", "`": "`" };

const OPENERS = "([{";
const CLOSERS = ")]}";

/** `null` when the language has no line comment, which disables the toggle. */
export function lineCommentToken(language: string): string | null {
  return LINE_COMMENTS[language.toLowerCase()] ?? null;
}

/** 1-based line and column for a caret offset — what the status bar reports. */
export function caretPosition(text: string, offset: number): { line: number; column: number } {
  const safe = Math.max(0, Math.min(offset, text.length));
  const before = text.slice(0, safe);
  const line = before.split("\n").length;
  const lastBreak = before.lastIndexOf("\n");
  return { line, column: safe - lastBreak };
}

/** Offset of the first character of a 1-based line, clamped to the document. */
export function offsetOfLine(text: string, line: number): number {
  const rows = text.split("\n");
  const target = Math.max(1, Math.min(line, rows.length));
  let offset = 0;
  for (let index = 0; index < target - 1; index += 1) offset += rows[index].length + 1;
  return offset;
}

/** The whole-line span covering a selection, so line commands act on every touched line. */
export function lineSpan(text: string, start: number, end: number): Selection {
  const from = text.lastIndexOf("\n", Math.max(0, start - 1)) + 1;
  const lineEnd = text.indexOf("\n", end);
  return { start: from, end: lineEnd === -1 ? text.length : lineEnd };
}

/**
 * Reads the file's own indentation.
 *
 * A file that indents with tabs keeps indenting with tabs when this editor touches it —
 * silently converting a repository to spaces on first save is the kind of diff that
 * makes an editor untrustworthy.
 */
export function detectIndent(text: string): IndentStyle {
  let tabs = 0;
  const widths: number[] = [];
  for (const row of text.split("\n").slice(0, 500)) {
    if (row.startsWith("\t")) {
      tabs += 1;
      continue;
    }
    const spaces = row.length - row.trimStart().length;
    if (spaces > 0 && row.trim().length > 0) widths.push(spaces);
  }
  if (tabs > widths.length) return { useTabs: true, size: 4 };
  // The smallest indent that actually appears is the unit; 2 when nothing indents yet.
  const smallest = widths.reduce((low, width) => (width < low ? width : low), Infinity);
  return { useTabs: false, size: Number.isFinite(smallest) ? Math.min(smallest, 8) : 2 };
}

export function indentUnit(style: IndentStyle): string {
  return style.useTabs ? "\t" : " ".repeat(style.size);
}

/** `CRLF` when the file already uses it, so saving does not rewrite every line ending. */
export function detectEol(text: string): "LF" | "CRLF" {
  return text.includes("\r\n") ? "CRLF" : "LF";
}

/** Tab / Shift+Tab across every line the selection touches. */
export function indentLines(
  text: string,
  start: number,
  end: number,
  style: IndentStyle,
  outdent: boolean,
): Edit {
  const span = lineSpan(text, start, end);
  const unit = indentUnit(style);
  const rows = text.slice(span.start, span.end).split("\n");
  let firstDelta = 0;
  let totalDelta = 0;

  const next = rows.map((row, index) => {
    if (outdent) {
      let removed = 0;
      if (row.startsWith("\t")) removed = 1;
      else {
        while (removed < style.size && row[removed] === " ") removed += 1;
      }
      if (index === 0) firstDelta = -removed;
      totalDelta -= removed;
      return row.slice(removed);
    }
    // Blank lines gain nothing: trailing whitespace on an empty line is noise in a diff.
    if (row.length === 0) return row;
    if (index === 0) firstDelta = unit.length;
    totalDelta += unit.length;
    return unit + row;
  });

  return {
    text: text.slice(0, span.start) + next.join("\n") + text.slice(span.end),
    start: Math.max(span.start, start + firstDelta),
    end: Math.max(span.start, end + totalDelta),
  };
}

/**
 * Ctrl+/ — comments the selected lines, or uncomments them when every one is already
 * commented. Mixed selections comment, which is what VS Code does and what people expect
 * when they select a block and hit the chord twice.
 */
export function toggleLineComment(
  text: string,
  start: number,
  end: number,
  token: string,
  style: IndentStyle,
): Edit {
  const span = lineSpan(text, start, end);
  const rows = text.slice(span.start, span.end).split("\n");
  const meaningful = rows.filter((row) => row.trim().length > 0);
  if (meaningful.length === 0) return { text, start, end };

  const allCommented = meaningful.every((row) => row.trimStart().startsWith(`${token}`));
  let firstDelta = 0;
  let totalDelta = 0;

  const next = rows.map((row, index) => {
    if (row.trim().length === 0) return row;
    if (allCommented) {
      const at = row.indexOf(token);
      const after = at + token.length;
      // Uncommenting also removes the single space the commenter added.
      const width = row[after] === " " ? token.length + 1 : token.length;
      if (index === 0) firstDelta = -width;
      totalDelta -= width;
      return row.slice(0, at) + row.slice(at + width);
    }
    const indent = row.length - row.trimStart().length;
    const insert = `${token} `;
    if (index === 0) firstDelta = insert.length;
    totalDelta += insert.length;
    return row.slice(0, indent) + insert + row.slice(indent);
  });

  void style;
  return {
    text: text.slice(0, span.start) + next.join("\n") + text.slice(span.end),
    start: Math.max(span.start, start + firstDelta),
    end: Math.max(span.start, end + totalDelta),
  };
}

/** Alt+Up / Alt+Down — moves the touched lines as a block, carrying the selection. */
export function moveLines(text: string, start: number, end: number, delta: number): Edit {
  const rows = text.split("\n");
  const from = caretPosition(text, start).line - 1;
  const to = caretPosition(text, end).line - 1;
  const target = from + delta;
  if (target < 0 || to + delta >= rows.length) return { text, start, end };

  const block = rows.splice(from, to - from + 1);
  rows.splice(target, 0, ...block);
  const next = rows.join("\n");
  const shift = offsetOfLine(next, target + 1) - offsetOfLine(text, from + 1);
  return { text: next, start: start + shift, end: end + shift };
}

/** Shift+Alt+Down — copies the touched lines below themselves. */
export function duplicateLines(text: string, start: number, end: number): Edit {
  const span = lineSpan(text, start, end);
  const block = text.slice(span.start, span.end);
  return {
    text: `${text.slice(0, span.end)}\n${block}${text.slice(span.end)}`,
    start: start + block.length + 1,
    end: end + block.length + 1,
  };
}

/** Ctrl+Shift+K — removes the touched lines entirely. */
export function deleteLines(text: string, start: number, end: number): Edit {
  const span = lineSpan(text, start, end);
  const cutEnd = span.end < text.length ? span.end + 1 : span.end;
  const cutStart = cutEnd === span.end && span.start > 0 ? span.start - 1 : span.start;
  const next = text.slice(0, cutStart) + text.slice(cutEnd);
  const caret = Math.min(cutStart, next.length);
  return { text: next, start: caret, end: caret };
}

/**
 * Enter — keeps the current indentation, and adds one level after a line that opens a
 * block. When the caret sits between a matching pair, the closer is pushed onto its own
 * line, which is the behaviour that makes typing `{` then Enter feel right.
 */
export function newlineWithIndent(text: string, caret: number, style: IndentStyle): Edit {
  const lineStart = text.lastIndexOf("\n", Math.max(0, caret - 1)) + 1;
  const current = text.slice(lineStart, caret);
  const indent = current.slice(0, current.length - current.trimStart().length);
  const before = current.trimEnd().slice(-1);
  const after = text.slice(caret, caret + 1);
  const unit = indentUnit(style);
  const opens = OPENERS.includes(before);
  const body = `\n${indent}${opens ? unit : ""}`;

  if (opens && CLOSERS.includes(after)) {
    const inserted = `${body}\n${indent}`;
    return {
      text: text.slice(0, caret) + inserted + text.slice(caret),
      start: caret + body.length,
      end: caret + body.length,
    };
  }
  return {
    text: text.slice(0, caret) + body + text.slice(caret),
    start: caret + body.length,
    end: caret + body.length,
  };
}

/**
 * Typing a bracket or quote.
 *
 * Returns `null` when the character should just be inserted normally, so the caller only
 * intercepts the keystroke when there is something clever to do.
 */
export function typePair(text: string, start: number, end: number, char: string): Edit | null {
  const closer = PAIRS[char];
  const nextChar = text.slice(end, end + 1);

  // Typing the closing half of a pair the editor added: step over it instead of doubling.
  if (start === end && CLOSERS.includes(char) && nextChar === char) {
    return { text, start: start + 1, end: start + 1 };
  }
  if (!closer) return null;

  // Wrap a selection rather than replacing it — the single most useful of these.
  if (start !== end) {
    const inner = text.slice(start, end);
    return {
      text: `${text.slice(0, start)}${char}${inner}${closer}${text.slice(end)}`,
      start: start + 1,
      end: end + 1,
    };
  }
  // A quote in the middle of a word is an apostrophe, not the start of a string.
  const isQuote = char === closer;
  if (isQuote && /[\w"'`]/.test(text.slice(start - 1, start))) return null;
  if (/[\w"'`]/.test(nextChar)) return null;

  return {
    text: `${text.slice(0, start)}${char}${closer}${text.slice(end)}`,
    start: start + 1,
    end: start + 1,
  };
}

/** Backspace between an empty pair removes both halves. */
export function backspacePair(text: string, caret: number): Edit | null {
  const before = text.slice(caret - 1, caret);
  const after = text.slice(caret, caret + 1);
  if (!before || PAIRS[before] !== after) return null;
  return { text: text.slice(0, caret - 1) + text.slice(caret + 1), start: caret - 1, end: caret - 1 };
}

/** Home — to the first non-blank character, then to column 1 on a second press. */
export function smartHome(text: string, caret: number): number {
  const lineStart = text.lastIndexOf("\n", Math.max(0, caret - 1)) + 1;
  const row = text.slice(lineStart, text.indexOf("\n", lineStart) === -1 ? text.length : text.indexOf("\n", lineStart));
  const firstWord = lineStart + (row.length - row.trimStart().length);
  return caret === firstWord ? lineStart : firstWord;
}

/**
 * The bracket matching the one beside the caret, or `null`.
 *
 * Scans with a depth counter and skips nothing else — good enough to be useful and cheap
 * enough to run on every caret move, which a parser-backed version would not be.
 */
export function matchingBracket(text: string, caret: number): { open: number; close: number } | null {
  const at = (index: number) => text.charAt(index);
  const scan = (from: number, char: string, forward: boolean): number | null => {
    const partner = forward ? CLOSERS[OPENERS.indexOf(char)] : OPENERS[CLOSERS.indexOf(char)];
    let depth = 0;
    for (let index = from; forward ? index < text.length : index >= 0; index += forward ? 1 : -1) {
      const here = at(index);
      if (here === char) depth += 1;
      else if (here === partner) {
        depth -= 1;
        if (depth === 0) return index;
      }
    }
    return null;
  };

  const before = at(caret - 1);
  const after = at(caret);
  if (OPENERS.includes(after)) {
    const close = scan(caret, after, true);
    return close === null ? null : { open: caret, close };
  }
  if (CLOSERS.includes(before)) {
    const open = scan(caret - 1, before, false);
    return open === null ? null : { open, close: caret - 1 };
  }
  return null;
}

export type FindOptions = { caseSensitive: boolean; wholeWord: boolean; regex: boolean };

/** Every match of the find query, in document order. Invalid regex yields no matches. */
export function findMatches(text: string, query: string, options: FindOptions): Selection[] {
  if (!query) return [];
  let source = options.regex ? query : query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (options.wholeWord) source = `\\b(?:${source})\\b`;
  let pattern: RegExp;
  try {
    pattern = new RegExp(source, options.caseSensitive ? "gm" : "gim");
  } catch {
    return [];
  }

  const found: Selection[] = [];
  for (const match of text.matchAll(pattern)) {
    if (match.index === undefined) continue;
    // A zero-width match would loop forever and means nothing to a reader.
    if (match[0].length === 0) continue;
    found.push({ start: match.index, end: match.index + match[0].length });
    if (found.length >= 5000) break;
  }
  return found;
}

/** The match the caret is inside or before — what "find next" starts from. */
export function nextMatchIndex(matches: Selection[], caret: number, forward: boolean): number {
  if (matches.length === 0) return -1;
  if (forward) {
    const at = matches.findIndex((match) => match.start >= caret);
    return at === -1 ? 0 : at;
  }
  for (let index = matches.length - 1; index >= 0; index -= 1) {
    if (matches[index].end <= caret) return index;
  }
  return matches.length - 1;
}

export function replaceRange(text: string, range: Selection, value: string): Edit {
  return {
    text: text.slice(0, range.start) + value + text.slice(range.end),
    start: range.start + value.length,
    end: range.start + value.length,
  };
}

/** Replace All, applied back to front so earlier offsets stay valid. */
export function replaceAll(text: string, matches: Selection[], value: string): string {
  let next = text;
  for (let index = matches.length - 1; index >= 0; index -= 1) {
    const match = matches[index];
    next = next.slice(0, match.start) + value + next.slice(match.end);
  }
  return next;
}
