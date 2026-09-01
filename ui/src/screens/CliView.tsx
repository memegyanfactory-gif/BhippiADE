import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";

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
  history: CliHistoryItem[];
  createdAt: string;
  /** The project this shell was opened in, so the rail can group it. */
  projectPath: string;
};

const SHELLS: { id: string; label: string; prompt: string }[] = [
  { id: "cmd", label: "Command Prompt", prompt: "CMD" },
  { id: "powershell", label: "PowerShell", prompt: "PS" },
];

/* Build a clean, authentic prompt for the given working directory. */
function promptOf(shell: string, cwd: string): string {
  let path = cwd.replace(/\//g, "\\").replace(/\\+$/, "");
  if (path.startsWith("\\\\?\\")) {
    path = path.slice(4);
  }
  if (shell === "powershell") {
    return `PS ${path}>`;
  }
  return `${path}>`;
}

/* Authentic startup banners matching real Windows Terminal */
const BANNER: Record<string, string[]> = {
  cmd: [
    "Microsoft Windows [Version 10.0.26100.3194]",
    "(c) Microsoft Corporation. All rights reserved.",
  ],
  powershell: [
    "Windows PowerShell",
    "Copyright (C) Microsoft Corporation. All rights reserved.",
    "",
    "Install the latest PowerShell for new features and improvements! https://aka.ms/PSWindows",
  ],
};

export function CliView({
  session,
  projectPath,
  onUpdateSession,
}: {
  session: CliSession;
  projectPath: string;
  onUpdateSession: (updated: CliSession) => void;
}) {
  const [input, setInput] = useState("");
  const [running, setRunning] = useState(false);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const [shellMenuOpen, setShellMenuOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const [externalMsg, setExternalMsg] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const scrollEndRef = useRef<HTMLDivElement | null>(null);

  const activeShell = SHELLS.find((s) => s.id === session.shell) ?? SHELLS[0];
  const prompt = promptOf(activeShell.id, projectPath);
  const banner = BANNER[activeShell.id] ?? BANNER.cmd;

  useEffect(() => {
    scrollEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [session.history, running]);

  useEffect(() => {
    if (!running) inputRef.current?.focus();
  }, [session.id, running]);

  const handleRun = useCallback(
    async (cmdToRun?: string) => {
      const commandText = (cmdToRun ?? input).trim();
      if (!commandText || running) return;

      if (commandText.toLowerCase() === "clear" || commandText.toLowerCase() === "cls") {
        onUpdateSession({ ...session, history: [] });
        setInput("");
        setHistoryIndex(null);
        return;
      }

      setRunning(true);
      setInput("");
      setHistoryIndex(null);
      const start = Date.now();

      try {
        const result = await api.runCliCommand(projectPath, session.shell, commandText);
        const durationMs = Date.now() - start;
        const newItem: CliHistoryItem = {
          id: `cli-item-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
          command: commandText,
          shell: session.shell,
          stdout: result.stdout,
          stderr: result.stderr,
          exitCode: result.exit_code,
          success: result.success,
          timestamp: Date.now(),
          durationMs,
        };
        onUpdateSession({
          ...session,
          history: [...session.history, newItem],
        });
      } catch (err) {
        const durationMs = Date.now() - start;
        const errMsg = String((err as Error).message ?? err);
        const newItem: CliHistoryItem = {
          id: `cli-item-${Date.now()}`,
          command: commandText,
          shell: session.shell,
          stdout: "",
          stderr: errMsg,
          exitCode: 1,
          success: false,
          timestamp: Date.now(),
          durationMs,
        };
        onUpdateSession({
          ...session,
          history: [...session.history, newItem],
        });
      } finally {
        setRunning(false);
      }
    },
    [input, running, session, projectPath, onUpdateSession],
  );

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void handleRun();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (session.history.length === 0) return;
      const nextIdx =
        historyIndex === null
          ? session.history.length - 1
          : Math.max(0, historyIndex - 1);
      setHistoryIndex(nextIdx);
      setInput(session.history[nextIdx]?.command ?? "");
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (historyIndex === null) return;
      const nextIdx = historyIndex + 1;
      if (nextIdx >= session.history.length) {
        setHistoryIndex(null);
        setInput("");
      } else {
        setHistoryIndex(nextIdx);
        setInput(session.history[nextIdx]?.command ?? "");
      }
    } else if (e.key === "Escape") {
      setInput("");
      setHistoryIndex(null);
    }
  };

  const handleCopyAll = () => {
    const text = session.history
      .map(
        (i) =>
          `${promptOf(i.shell, projectPath)} ${i.command}\n${i.stdout.trimEnd()}${
            i.stderr ? "\n" + i.stderr.trimEnd() : ""
          }`,
      )
      .join("\n\n");
    if (!text) return;
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  const handleOpenExternal = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await api.openExternalTerminal(projectPath, session.shell);
      setExternalMsg("External terminal launched");
      setTimeout(() => setExternalMsg(null), 2500);
    } catch (err) {
      setExternalMsg(`Launch failed: ${(err as Error).message ?? err}`);
      setTimeout(() => setExternalMsg(null), 3000);
    }
  };

  const handleShellChange = (newShell: string) => {
    const s = SHELLS.find((x) => x.id === newShell);
    onUpdateSession({
      ...session,
      shell: newShell,
      title: `CLI: ${s?.label ?? newShell}`,
    });
    setShellMenuOpen(false);
  };

  const clearBuffer = () => {
    onUpdateSession({ ...session, history: [] });
    setHistoryIndex(null);
  };

  return (
    <div className={`cli-container shell-${activeShell.id}`} aria-label="CLI Terminal">
      {/* Terminal tab strip — authentic Windows Terminal chrome */}
      <div className="cli-tabbar">
        <div className="cli-tab-title" title={projectPath}>
          {activeShell.label} — {projectPath}
        </div>
        <div className="cli-tab-actions">
          <div className="cli-shell-picker">
            <button
              className="cli-tab-act"
              onClick={() => setShellMenuOpen((open) => !open)}
              aria-expanded={shellMenuOpen}
              aria-haspopup="menu"
              title="Change shell"
            >
              {activeShell.label} ▾
            </button>
            {shellMenuOpen && (
              <>
                <button
                  className="menu-scrim"
                  onClick={() => setShellMenuOpen(false)}
                  aria-label="Close shell picker"
                />
                <div className="cli-shell-menu" role="menu">
                  <div className="cli-menu-header">Select shell</div>
                  {SHELLS.map((s) => (
                    <button
                      key={s.id}
                      role="menuitem"
                      className={`cli-menu-item${s.id === session.shell ? " active" : ""}`}
                      onClick={() => handleShellChange(s.id)}
                    >
                      <span className="cli-shell-badge">{s.prompt}</span>
                      <span>{s.label}</span>
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>
          <button className="cli-tab-act" onClick={handleOpenExternal} title="Open in an external terminal window">
            external
          </button>
          <button
            className="cli-tab-act"
            onClick={handleCopyAll}
            disabled={session.history.length === 0}
            title="Copy the whole session"
          >
            {copied ? "copied" : "copy"}
          </button>
          <button
            className="cli-tab-act"
            onClick={clearBuffer}
            disabled={session.history.length === 0}
            title="Clear the buffer"
          >
            clear
          </button>
        </div>
      </div>

      {externalMsg && (
        <div className="cli-toast" role="status">
          {externalMsg}
        </div>
      )}

      {/* One continuous terminal surface: scrollback + prompt line */}
      <div
        className="cli-body"
        onClick={() => {
          if (!running) inputRef.current?.focus();
        }}
      >
        <div className="cli-scroll" role="log" aria-live="polite">
          {session.history.length === 0 && !running && (
            <div className="cli-welcome">
              {banner.map((line, i) =>
                line === "" ? (
                  <div key={i} className="cli-wl-spacer" />
                ) : (
                  <div key={i} className="cli-wl">
                    {line}
                  </div>
                ),
              )}
            </div>
          )}

          {session.history.map((item) => (
            <div key={item.id} className="cli-line">
              <span className="cli-ps" aria-hidden="true">
                {promptOf(item.shell, projectPath)}
              </span>{" "}
              <span className="cli-cmd">{item.command}</span>
              {item.stdout !== "" && <pre className="cli-out">{item.stdout.trimEnd()}</pre>}
              {item.stderr !== "" && <pre className="cli-err">{item.stderr.trimEnd()}</pre>}
            </div>
          ))}

          {/* Blinking block cursor while a command is executing */}
          {running && (
            <div className="cli-line cli-running">
              <span className="cli-ps" aria-hidden="true">
                {prompt}
              </span>{" "}
              <span className="cli-cursor" aria-hidden="true" />
            </div>
          )}

          <div ref={scrollEndRef} />
        </div>

        {/* Prompt line — fused into the surface so the terminal reads as one screen */}
        <div
          className={`cli-promptline${running ? " running" : ""}`}
          onClick={(e) => {
            e.stopPropagation();
            if (!running) inputRef.current?.focus();
          }}
        >
          <span className="cli-ps" aria-hidden="true">
            {prompt}
          </span>{" "}
          <input
            ref={inputRef}
            type="text"
            className="cli-input-field"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={running ? undefined : `Run ${activeShell.label} command…`}
            readOnly={running}
            spellCheck={false}
            autoComplete="off"
            aria-label="Terminal command input"
          />
          {running && <span className="cli-cursor" aria-hidden="true" />}
        </div>
      </div>
    </div>
  );
}