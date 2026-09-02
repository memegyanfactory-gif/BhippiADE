import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import type { TerminalShell } from "../lib/ipc";
import { acquire, release, type LiveTerminal } from "../lib/terminalStore";
import { IconCopy, IconExternalLink, IconTrash } from "../components/icons";

/**
 * Kept only so sessions saved by an earlier build still parse. Nothing reads it: a PTY
 * owns its own scrollback, and the emulator renders it.
 */
export type CliHistoryItem = {
  id: string;
  command: string;
  shell: string;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  success: boolean;
  timestamp: number;
  durationMs: number;
};

export type CliSession = {
  id: string;
  title: string;
  shell: string;
  createdAt: string;
  /** The project this shell was opened in, so the rail can group it. */
  projectPath: string;
  /** Legacy field from the batch-runner build. Ignored. */
  history?: CliHistoryItem[];
};

const SHELLS: { id: TerminalShell; label: string }[] = [
  { id: "powershell", label: "PowerShell" },
  { id: "cmd", label: "Command Prompt" },
  { id: "git_bash", label: "Git Bash" },
  { id: "wsl", label: "WSL" },
];

/** Sessions saved before Git Bash and WSL existed still name a valid shell. */
function asShell(value: string): TerminalShell {
  return SHELLS.some((entry) => entry.id === value) ? (value as TerminalShell) : "powershell";
}

export function CliView({
  session,
  projectPath,
  onUpdateSession,
}: {
  session: CliSession;
  projectPath: string;
  onUpdateSession: (updated: CliSession) => void;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const liveRef = useRef<LiveTerminal | null>(null);
  const [shellMenuOpen, setShellMenuOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const [exit, setExit] = useState<{ code: number | null } | null>(null);

  const shell = asShell(session.shell);
  const shellLabel = SHELLS.find((entry) => entry.id === shell)?.label ?? shell;

  // The emulator lives in terminalStore, not in this component: switching tab unmounts
  // the pane, and a terminal that died on unmount would kill whatever was running in it.
  // Mounting re-parents the surviving node; it is disposed only when the session is.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const live = acquire(session.id, projectPath, shell);
    liveRef.current = live;
    host.appendChild(live.element);

    let cancelled = false;
    const syncExit = () => {
      if (!cancelled) setExit(live.exit);
    };
    void live.ready.then(syncExit);
    const poll = window.setInterval(syncExit, 1000);

    // Refit on every size change, including the workspace's manual pane resizing.
    const observer = new ResizeObserver(() => {
      try {
        live.fit.fit();
      } catch {
        /* Zero-sized while animating; the next observation will land. */
      }
    });
    observer.observe(host);
    try {
      live.fit.fit();
    } catch {
      /* Not measurable on this frame. */
    }
    live.term.focus();

    return () => {
      cancelled = true;
      window.clearInterval(poll);
      observer.disconnect();
      // Detach, never dispose — the terminal keeps running for the next mount.
      if (live.element.parentElement === host) host.removeChild(live.element);
    };
  }, [session.id, projectPath, shell]);

  const say = useCallback((message: string) => {
    setToast(message);
    window.setTimeout(() => setToast(null), 2500);
  }, []);

  const copyBuffer = () => {
    const live = liveRef.current;
    if (!live) return;
    live.term.selectAll();
    const text = live.term.getSelection();
    live.term.clearSelection();
    if (!text.trim()) return;
    void navigator.clipboard
      .writeText(text)
      .then(() => say("Buffer copied"))
      .catch(() => say("Clipboard unavailable"));
  };

  const clearScreen = () => {
    liveRef.current?.term.clear();
    liveRef.current?.term.focus();
  };

  const openExternal = async (event: React.MouseEvent) => {
    event.stopPropagation();
    try {
      await api.openExternalTerminal(projectPath, shell);
      say("External terminal launched");
    } catch (error) {
      say(`Launch failed: ${(error as Error).message ?? error}`);
    }
  };

  // A shell is a process, not a setting: switching one means ending the old PTY and
  // opening a new one. Releasing here is what makes the mount effect below build a fresh
  // terminal instead of handing back the cached one for this session id.
  const changeShell = (next: TerminalShell) => {
    setShellMenuOpen(false);
    if (next === shell) return;
    release(session.id);
    const label = SHELLS.find((entry) => entry.id === next)?.label ?? next;
    onUpdateSession({ ...session, shell: next, title: `CLI: ${label}` });
  };

  return (
    <div className="cli-container" aria-label={`${shellLabel} terminal`}>
      <div className="cli-tabbar">
        <div className="cli-tab-title" title={projectPath}>
          <span className="cli-tab-shell">{shellLabel}</span>
          <span className="cli-tab-path">{projectPath}</span>
          {exit ? (
            <span className="cli-tab-exit">
              exited{exit.code === null ? "" : ` · ${exit.code}`}
            </span>
          ) : null}
        </div>
        <div className="cli-tab-actions">
          <div className="cli-shell-picker">
            <button
              type="button"
              className="cli-tab-act"
              onClick={() => setShellMenuOpen((open) => !open)}
              aria-expanded={shellMenuOpen}
              aria-haspopup="menu"
              title="Change shell"
            >
              {shellLabel} ▾
            </button>
            {shellMenuOpen ? (
              <>
                <button
                  type="button"
                  className="menu-scrim"
                  onClick={() => setShellMenuOpen(false)}
                  aria-label="Close shell picker"
                />
                <div className="cli-shell-menu" role="menu">
                  {SHELLS.map((entry) => (
                    <button
                      key={entry.id}
                      type="button"
                      role="menuitem"
                      className={`cli-menu-item${entry.id === shell ? " active" : ""}`}
                      onClick={() => changeShell(entry.id)}
                    >
                      {entry.label}
                    </button>
                  ))}
                </div>
              </>
            ) : null}
          </div>
          <button
            type="button"
            className="cli-tab-act icon"
            onClick={openExternal}
            title="Open this folder in an external terminal"
            aria-label="Open in external terminal"
          >
            <IconExternalLink size={13} />
          </button>
          <button
            type="button"
            className="cli-tab-act icon"
            onClick={copyBuffer}
            title="Copy the whole buffer"
            aria-label="Copy buffer"
          >
            <IconCopy size={13} />
          </button>
          <button
            type="button"
            className="cli-tab-act icon"
            onClick={clearScreen}
            title="Clear the screen"
            aria-label="Clear screen"
          >
            <IconTrash size={13} />
          </button>
        </div>
      </div>

      {toast ? (
        <div className="cli-toast" role="status">
          {toast}
        </div>
      ) : null}

      <div
        className="cli-body"
        ref={hostRef}
        onClick={() => liveRef.current?.term.focus()}
      />
    </div>
  );
}
