import { useCallback, useEffect, useMemo, useState } from "react";
import type { EngineConsoleRow, EngineHistoryEntry } from "../lib/ipc";
import { api } from "../lib/api";
import { IconClose, IconRefresh, IconSearch } from "../components/icons";
import { requestOpenWorkspaceFile } from "../workbench/openFileRequest";

/**
 * The Output Log (ENG-149).
 *
 * Two sources, one list: the **transaction journal** (INV-071 — every applied change with
 * its actor, which is the honest answer to "what did the agent change?") and the pane's own
 * notices. The journal half survives a restart because it is read from the database rather
 * than kept in memory, which is the point of having journaled it.
 */

export type LogLevel = "info" | "warn" | "error";

export interface LogLine {
  id: string;
  at: string;
  level: LogLevel;
  channel: string;
  text: string;
  /// Set on agent journal rows: the transaction this line can undo in one operation.
  undoTxn?: string;
  undoLabel?: string;
  source?: { path: string; line: number };
}

interface Props {
  /** Lines the pane pushed (notices, failures). Newest last. */
  local: LogLine[];
  onClear: () => void;
  /** Called after a journalled change is reverted, so the pane can re-read the scene. */
  onReverted?: () => void;
}

const LEVELS: { id: LogLevel | "all"; label: string }[] = [
  { id: "all", label: "All" },
  { id: "info", label: "Info" },
  { id: "warn", label: "Warnings" },
  { id: "error", label: "Errors" },
];

export function EngineOutputLog({ local, onClear, onReverted }: Props) {
  const [journal, setJournal] = useState<EngineHistoryEntry[]>([]);
  const [consoleRows, setConsoleRows] = useState<EngineConsoleRow[]>([]);
  const [level, setLevel] = useState<LogLevel | "all">("all");
  const [filter, setFilter] = useState("");
  /// ENG-189: only agent rows offer Undo, and only one revert runs at a time.
  const [reverting, setReverting] = useState<string | null>(null);
  const [revertError, setRevertError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [history, console] = await Promise.all([
        api.engineHistory(null, 120),
        api.engineConsoleRows(null, null, null, 0, 40),
      ]);
      setJournal(history);
      setConsoleRows(console);
    } catch {
      // No journal (no game, or the database is unavailable) is a quiet state, not an error
      // to shout about in the log that would be reporting it.
      setJournal([]);
      setConsoleRows([]);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh, local.length]);

  const lines = useMemo(() => {
    const fromJournal: LogLine[] = journal.map((entry) => ({
      id: `r${entry.revision}`,
      at: entry.issued_at,
      level: "info",
      channel: entry.actor === "agent" ? "agent" : "editor",
      text: `r${entry.revision} ${entry.label} · ${entry.scene_path} (${entry.op_count} ops)`,
      // ENG-189: the whole batch is one transaction, so one button takes all of it back.
      undoTxn: entry.actor === "agent" ? entry.txn_id : undefined,
      undoLabel: entry.label,
    }));
    const fromConsole: LogLine[] = consoleRows.map((row) => ({
      id: `c${row.id}`,
      at: row.at,
      level: row.level === "warn" || row.level === "error" ? row.level : "info",
      channel: row.channel,
      text: row.text,
      source: row.file && row.line ? { path: row.file, line: row.line } : undefined,
    }));
    // Local rows make a newly emitted line visible before its async telemetry write returns.
    // Once the typed row arrives, the source-of-truth copy wins and the duplicate is hidden.
    const remoteKeys = new Set(fromConsole.map((line) => `${line.channel}\0${line.text}`));
    const immediate = local.filter((line) => !remoteKeys.has(`${line.channel}\0${line.text}`));
    const all = [...fromJournal, ...fromConsole, ...immediate].sort((a, b) => (a.at < b.at ? 1 : -1));
    const query = filter.trim().toLowerCase();
    return all.filter(
      (line) =>
        (level === "all" || line.level === level) &&
        (!query || line.text.toLowerCase().includes(query) || line.channel.includes(query)),
    );
  }, [consoleRows, filter, journal, level, local]);

  return (
    <section className="engine-log" aria-label="Output log">
      <div className="engine-log-bar">
        {LEVELS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            className={`outliner-chip${level === entry.id ? " active" : ""}`}
            onClick={() => setLevel(entry.id)}
            aria-pressed={level === entry.id}
          >
            {entry.label}
          </button>
        ))}
        <span className="engine-log-search">
          <IconSearch size={11} />
          <input
            value={filter}
            placeholder="Filter…"
            aria-label="Filter log"
            onChange={(event) => setFilter(event.target.value)}
          />
        </span>
        <button type="button" className="outliner-toggle" onClick={() => void refresh()} title="Refresh">
          <IconRefresh size={12} />
        </button>
        <button type="button" className="outliner-toggle" onClick={onClear} title="Clear local notices">
          <IconClose size={11} />
        </button>
      </div>
      {revertError ? (
        <div className="engine-log-notice" role="alert">
          {revertError}
        </div>
      ) : null}
      <div className="engine-log-list">
        {lines.map((line) => (
          <div key={line.id} className={`engine-log-line ${line.level}`}>
            <span className="engine-log-channel">{line.channel}</span>
            {line.source ? (
              <button
                type="button"
                className="engine-log-source"
                title={`Open ${line.source.path} at line ${line.source.line}`}
                onClick={() => requestOpenWorkspaceFile(line.source!.path, line.source!.line)}
              >
                {line.source.path}:{line.source.line} · {line.text}
              </button>
            ) : (
              <span className="engine-log-text">{line.text}</span>
            )}
            {line.undoTxn ? (
              <button
                type="button"
                className="engine-log-undo"
                disabled={reverting !== null}
                title={`Undo the whole change “${line.undoLabel ?? ""}” in one step`}
                onClick={() => {
                  const txn = line.undoTxn;
                  if (!txn) return;
                  setReverting(txn);
                  setRevertError(null);
                  void api
                    .engineUndoJournalled(txn)
                    .then(() => {
                      onReverted?.();
                      return refresh();
                    })
                    .catch((error: any) =>
                      setRevertError(
                        `${error?.message ?? "Could not undo that change."}${error?.hint ? ` — ${error.hint}` : ""}`,
                      ),
                    )
                    .finally(() => setReverting(null));
                }}
              >
                {reverting === line.undoTxn ? "Undoing…" : "Undo AI change"}
              </button>
            ) : null}
          </div>
        ))}
        {lines.length === 0 ? <div className="engine-empty-hint">Nothing logged yet.</div> : null}
      </div>
    </section>
  );
}
