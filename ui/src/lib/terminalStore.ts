/**
 * Live terminals, kept alive independently of React.
 *
 * A terminal pane unmounts every time the user switches tab or layout. If the emulator
 * and the PTY lived in component state, switching away from a running `opencode` would
 * kill it and switching back would show a fresh, empty shell. So the emulator instance,
 * its DOM node, and the PTY id live here, keyed by session id, and the component only
 * re-parents the existing node on mount. A terminal ends when the *session* is deleted,
 * which is the only moment `release` is called.
 */

import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import type { TerminalShell } from "./ipc";
import { api, events } from "./api";

export interface LiveTerminal {
  term: Terminal;
  fit: FitAddon;
  /** The detached host node, re-parented into whichever pane is showing this session. */
  element: HTMLDivElement;
  /** `null` until the PTY has been opened, or after the shell has exited. */
  ptyId: string | null;
  exit: { code: number | null } | null;
  /** Resolves once the PTY is open (or has failed), so a remount can await it. */
  ready: Promise<void>;
  error: string | null;
  dispose: () => void;
}

const live = new Map<string, LiveTerminal>();

/** Base64 -> bytes. xterm decodes UTF-8 itself, including sequences split across reads. */
function decode(chunk: string): Uint8Array {
  const binary = atob(chunk);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

/** Reads a CSS custom property off the document, for theming the emulator. */
function token(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

/**
 * The emulator's palette.
 *
 * The 16 ANSI colours are fixed: a program that prints "red" means red, and remapping
 * those to the app's accent would make `git diff` and every build's error output lie.
 * Only the surfaces the app owns — background, foreground, cursor, selection — follow
 * the active theme.
 */
function theme() {
  return {
    // Opaque, and the palette's own ground — see the renderer note in `acquire`.
    background: token("--base-bg", "#100f0d"),
    foreground: token("--text", "#eae7e1"),
    cursor: token("--accent", "#f0a02c"),
    cursorAccent: token("--bg", "#100f0d"),
    selectionBackground: token("--accent-dim", "rgba(240, 160, 44, 0.25)"),
    black: "#1c1b19",
    red: "#f85149",
    green: "#3fb950",
    yellow: "#e3b341",
    blue: "#58a6ff",
    magenta: "#bc8cff",
    cyan: "#39c5cf",
    white: "#b1aca4",
    brightBlack: "#6b655d",
    brightRed: "#ff7b72",
    brightGreen: "#56d364",
    brightYellow: "#f0c674",
    brightBlue: "#79c0ff",
    brightMagenta: "#d2a8ff",
    brightCyan: "#56d4dd",
    brightWhite: "#f0edE7",
  };
}

/** The terminal for this session, opening a PTY on first use. */
export function acquire(
  sessionId: string,
  projectPath: string,
  shell: TerminalShell,
): LiveTerminal {
  const existing = live.get(sessionId);
  if (existing) return existing;

  const element = document.createElement("div");
  element.className = "term-surface";

  const term = new Terminal({
    // The terminal gets its own font stack rather than the app's `--mono`. That stack
    // leads with JetBrains Mono and then `ui-monospace`, which on Windows resolves to
    // whatever the platform picks — and a font whose block glyph is narrower than the
    // cell leaves a seam between every column of a TUI's box art. Cascadia Mono is the
    // font Windows Terminal itself ships for this, and Consolas behind it is on every
    // Windows install, so this never falls through to a proportional default.
    fontFamily: '"Cascadia Mono", "Cascadia Code", Consolas, "Courier New", monospace',
    fontSize: 13,
    // Exactly 1. Box- and block-drawing characters are designed to tile edge to edge,
    // and any other line height leaves a seam between rows — which turns a TUI's banner
    // and every box border into broken stripes. The emulator draws those glyphs itself
    // (`customGlyphs`), but only tiles them correctly at a whole line.
    lineHeight: 1,
    letterSpacing: 0,
    cursorBlink: true,
    // The PTY is the source of truth for the visible screen; this is only how far back
    // the user can scroll through what has already gone by.
    scrollback: 10_000,
    allowProposedApi: true,
    // Deliberately NOT `allowTransparency`. It pins the emulator to the DOM renderer,
    // which draws box- and block-drawing characters from the font and leaves a hairline
    // seam between every cell — a TUI's banners and borders come out striped. The
    // terminal sits on an opaque pane anyway (`.cli-body`), so a transparent emulator
    // background bought nothing and cost correct glyph tiling.
    theme: theme(),
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(element);

  // The GPU renderer, not just for speed. It is the one that draws box- and
  // block-drawing characters itself as exact rectangles, so a TUI's banners and borders
  // tile without the hairline seams the DOM renderer leaves between cells. If the
  // context is refused (no GPU, a driver reset), xterm falls back to the DOM renderer on
  // its own and the terminal still works — so this is a best-effort upgrade, never a
  // requirement.
  try {
    const webgl = new WebglAddon();
    webgl.onContextLoss(() => webgl.dispose());
    term.loadAddon(webgl);
  } catch (error) {
    // Not fatal — xterm keeps the DOM renderer, which draws everything correctly apart
    // from leaving a hairline seam between adjacent block-drawing glyphs. Logged rather
    // than swallowed so the degraded path is visible instead of mysterious.
    console.warn("terminal: GPU renderer unavailable, using the DOM renderer", error);
  }

  let ptyId: string | null = null;
  let unlistenOutput: (() => void) | null = null;
  let unlistenExit: (() => void) | null = null;
  let disposed = false;

  const entry: LiveTerminal = {
    term,
    fit,
    element,
    ptyId: null,
    exit: null,
    error: null,
    ready: Promise.resolve(),
    dispose: () => {
      if (disposed) return;
      disposed = true;
      unlistenOutput?.();
      unlistenExit?.();
      if (ptyId) void api.terminalClose(ptyId).catch(() => undefined);
      term.dispose();
      live.delete(sessionId);
    },
  };

  // Input is wired up at construction, not after the PTY opens.
  //
  // A shell starts by asking the terminal where the cursor is (`CSI 6n`) and *blocks*
  // until it is answered. The emulator answers automatically — but only through this
  // handler, so attaching it after `terminalOpen` resolved meant the reply was thrown
  // away and PowerShell hung with a blank screen forever. Anything typed (or answered)
  // before the id is known is queued and flushed the moment it is.
  const outbox: string[] = [];
  const send = (data: string) => {
    if (!entry.ptyId) {
      outbox.push(data);
      return;
    }
    void api.terminalWrite(entry.ptyId, data).catch(() => undefined);
  };
  term.onData(send);
  term.onBinary((data) => send(data));

  // Clipboard.
  //
  // A terminal cannot use the app's Ctrl+C/Ctrl+V: Ctrl+C is the interrupt signal, and a
  // WebView does not give the emulator clipboard read access on its own — Ctrl+V arrived
  // at the shell as a literal ^V, so pasting a path or a command simply did not work.
  // These are the bindings every terminal emulator ships, handled here explicitly:
  //
  //   Ctrl+C with a selection  copy  (with none, it stays the interrupt)
  //   Ctrl+Shift+C             copy  (always, even mid-command)
  //   Ctrl+V / Ctrl+Shift+V    paste
  //   Shift+Insert             paste
  term.attachCustomKeyEventHandler((event) => {
    if (event.type !== "keydown") return true;
    const key = event.key.toLowerCase();

    const copy = () => {
      const selection = term.getSelection();
      if (!selection) return false;
      void navigator.clipboard.writeText(selection).catch(() => undefined);
      term.clearSelection();
      return true;
    };
    const paste = () => {
      void navigator.clipboard
        .readText()
        .then((text) => {
          if (text) term.paste(text);
        })
        .catch(() => undefined);
      return true;
    };

    if (event.ctrlKey && event.shiftKey && key === "c") return !copy();
    if (event.ctrlKey && !event.shiftKey && key === "c") {
      // Only steal Ctrl+C when there is something to copy. With no selection it must
      // still reach the shell, or nothing could ever be interrupted.
      return !copy();
    }
    if (event.ctrlKey && key === "v") return !paste();
    if (event.shiftKey && event.key === "Insert") return !paste();
    return true;
  });
  term.onResize(({ cols, rows }) => {
    if (!entry.ptyId) return;
    void api.terminalResize(entry.ptyId, cols, rows).catch(() => undefined);
  });

  entry.ready = (async () => {
    try {
      // Subscribe before opening: a shell can print its prompt before `terminalOpen`
      // has even resolved, and a late listener would miss it.
      const pending: string[] = [];
      unlistenOutput = await events.terminalOutput.listen((event) => {
        if (disposed) return;
        if (!ptyId) {
          pending.push(event.payload.chunk);
          return;
        }
        if (event.payload.id !== ptyId) return;
        term.write(decode(event.payload.chunk));
      });
      unlistenExit = await events.terminalExited.listen((event) => {
        if (disposed || event.payload.id !== ptyId) return;
        entry.exit = { code: event.payload.exit_code };
        entry.ptyId = null;
        ptyId = null;
        term.write(
          `\r\n\x1b[2m[process exited${
            event.payload.exit_code === null ? "" : ` with code ${event.payload.exit_code}`
          }]\x1b[0m\r\n`,
        );
      });

      // Fit before opening so the shell's first prompt is laid out at the real width.
      try {
        fit.fit();
      } catch {
        /* Not measurable yet; the ResizeObserver in the pane will fit again. */
      }
      const session = await api.terminalOpen(
        projectPath,
        shell,
        term.cols || 80,
        term.rows || 24,
      );
      if (disposed) {
        void api.terminalClose(session.id).catch(() => undefined);
        return;
      }
      ptyId = session.id;
      entry.ptyId = session.id;
      // Anything that arrived between the listener attaching and the id being known
      // belongs to this terminal: it is the only one that had not been assigned yet.
      for (const chunk of pending.splice(0)) term.write(decode(chunk));
      // Flush whatever the emulator answered (or the user typed) before the PTY existed.
      for (const data of outbox.splice(0)) {
        void api.terminalWrite(session.id, data).catch(() => undefined);
      }
    } catch (error) {
      const message =
        (error as { message?: string })?.message ?? String(error ?? "Unknown error");
      entry.error = message;
      term.write(`\x1b[31m${message}\x1b[0m\r\n`);
    }
  })();

  live.set(sessionId, entry);
  return entry;
}

/** Ends a session's terminal for good. Called when the session itself is deleted. */
export function release(sessionId: string): void {
  live.get(sessionId)?.dispose();
}

/** Ends every terminal. Used when a whole project's sessions are removed at once. */
export function releaseWhere(predicate: (sessionId: string) => boolean): void {
  for (const sessionId of [...live.keys()]) {
    if (predicate(sessionId)) release(sessionId);
  }
}

/** Re-themes every open terminal after an appearance change. */
export function retheme(): void {
  const next = theme();
  for (const entry of live.values()) {
    entry.term.options.theme = next;
  }
}
