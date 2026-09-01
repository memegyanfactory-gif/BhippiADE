import { useCallback, useEffect, useRef, useState } from "react";
import type { WorkspaceEntry, WorkspaceFile } from "../lib/ipc";
import { api } from "../lib/api";
import { FileTree } from "./FileTree";
import { CodeView } from "./CodeView";
import { BrowserView } from "./BrowserView";
import { EngineView } from "../engine/EngineView";
import { ModeSwitch, type WorkbenchMode } from "./ModeSwitch";
import { IconClose } from "../components/icons";
import { OPEN_WORKSPACE_FILE_EVENT, type OpenWorkspaceFileDetail } from "./openFileRequest";

/**
 * The right-hand workbench: the file editor, the local-preview browser, and the game
 * engine, one at a time, behind a switch.
 *
 * Panes stay mounted once they have been opened. Unmounting the browser would throw away
 * the frame and reload the dev server on every toggle, unmounting the editor would drop
 * an unsaved buffer, and the engine pane would re-read the manifest — so the inactive
 * one is hidden, not destroyed.
 */
export function Workbench({
  projectPath,
  mode,
  onMode,
  onClose,
}: {
  /** Changing project invalidates the tree and any open file. */
  projectPath: string;
  /** Owned by the shell, so the toolbar toggle and this switch never disagree. */
  mode: WorkbenchMode;
  onMode: (mode: WorkbenchMode) => void;
  onClose: () => void;
}) {
  const [file, setFile] = useState<WorkspaceFile | null>(null);
  const [buffer, setBuffer] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshToken, setRefreshToken] = useState(0);
  const [focusLine, setFocusLine] = useState<number | null>(null);
  /** The browser and engine panes only mount once they have been asked for — no idle
      port probing, no engine state before the user asks. */
  const browserSeen = useRef(false);
  const engineSeen = useRef(false);

  if (mode === "browser") browserSeen.current = true;
  if (mode === "engine") engineSeen.current = true;

  useEffect(() => {
    setFile(null);
    setBuffer("");
    setError(null);
    setRefreshToken((value) => value + 1);
  }, [projectPath]);

  const open = useCallback(async (entry: WorkspaceEntry) => {
    setError(null);
    setFocusLine(null);
    try {
      const loaded = await api.readFile(entry.path);
      setFile(loaded);
      setBuffer(loaded.text);
    } catch (openError) {
      setError(String((openError as { message?: string }).message ?? openError));
    }
  }, []);

  useEffect(() => {
    const onOpenFile = (raw: Event) => {
      const event = raw as CustomEvent<OpenWorkspaceFileDetail>;
      const { path, line } = event.detail;
      onMode("editor");
      setFocusLine(line);
      setError(null);
      void api.readFile(path).then((loaded) => {
        setFile(loaded);
        setBuffer(loaded.text);
      }).catch((openError) => {
        setFile(null);
        setBuffer("");
        setError(`Could not open ${path}:${line} — ${String((openError as { message?: string }).message ?? openError)}`);
      });
    };
    window.addEventListener(OPEN_WORKSPACE_FILE_EVENT, onOpenFile);
    return () => window.removeEventListener(OPEN_WORKSPACE_FILE_EVENT, onOpenFile);
  }, [onMode]);

  const save = useCallback(async () => {
    if (!file) return;
    setSaving(true);
    setError(null);
    try {
      const saved = await api.writeFile(file.path, buffer);
      setFile(saved);
      setBuffer(saved.text);
    } catch (saveError) {
      setError(String((saveError as { message?: string }).message ?? saveError));
    } finally {
      setSaving(false);
    }
  }, [buffer, file]);

  const dirty = file !== null && file.editable && buffer !== file.text;

  return (
    <section className="workbench" aria-label="Workbench">
      <div className="workbench-top">
        <ModeSwitch mode={mode} onMode={onMode} />
        <span className="grow" />
        {dirty ? <span className="workbench-dirty">unsaved</span> : null}
        <button className="workbench-close" onClick={onClose} aria-label="Close workbench" title="Close workbench">
          <IconClose size={12} />
        </button>
      </div>

      <div className="workbench-panes">
        <div className="workbench-pane" hidden={mode !== "editor"}>
          <div className="editor-split">
            <FileTree activePath={file?.path ?? null} onOpen={(entry) => void open(entry)} refreshToken={refreshToken} />
            <CodeView
              file={file}
              dirty={dirty}
              saving={saving}
              error={error}
              focusLine={focusLine}
              onChange={setBuffer}
              onSave={() => void save()}
              onClose={() => {
                setFile(null);
                setBuffer("");
              }}
            />
          </div>
        </div>

        {browserSeen.current ? (
          <div className="workbench-pane" hidden={mode !== "browser"}>
            <BrowserView active={mode === "browser"} />
          </div>
        ) : null}

        {engineSeen.current ? (
          <div className="workbench-pane" hidden={mode !== "engine"}>
            <EngineView projectPath={projectPath} refreshToken={refreshToken} active={mode === "engine"} />
          </div>
        ) : null}
      </div>
    </section>
  );
}
