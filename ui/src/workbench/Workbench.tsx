import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import type { WorkspaceEntry, WorkspaceFile } from "../lib/ipc";
import { api } from "../lib/api";
import { detectIndent, detectEol } from "./editorModel";
import {
  createTabsState,
  openTab,
  closeTab,
  closeAllTabs,
  closeOtherTabs,
  closeTabsToRight,
  activateTab,
  nextTab,
  previousTab,
  updateTabText,
  undoTab,
  redoTab,
  markTabSaved,
  markTabSaving,
  activeTab,
  type EditorTabsState,
} from "./editorTabs";
import { FileTree } from "./FileTree";
import { CodeView } from "./CodeView";
import { BrowserView } from "./BrowserView";
import { EngineView } from "../engine/EngineView";
import { ModeSwitch, type WorkbenchMode } from "./ModeSwitch";
import { IconClose } from "../components/icons";
import { OPEN_WORKSPACE_FILE_EVENT, type OpenWorkspaceFileDetail } from "./openFileRequest";

/**
 * The right-hand workbench: the file editor (with VS Code–class tabs), the local-
 * preview browser, and the game engine, one at a time, behind a mode switch.
 *
 * Tabs stay mounted once opened. Unmounting the browser would throw away the frame,
 * unmounting the editor would drop an unsaved buffer, and the engine pane would re-
 * read the manifest — so inactive panes are hidden, not destroyed.
 */

type TabsAction =
  | { type: "OPEN"; file: WorkspaceFile; focusLine?: number; preview?: boolean }
  | { type: "CLOSE"; id: string }
  | { type: "CLOSE_ALL" }
  | { type: "CLOSE_OTHERS"; id: string }
  | { type: "CLOSE_RIGHT"; id: string }
  | { type: "ACTIVATE"; index: number }
  | { type: "NEXT_TAB" }
  | { type: "PREV_TAB" }
  | { type: "UPDATE_TEXT"; id: string; text: string }
  | { type: "UNDO"; id: string }
  | { type: "REDO"; id: string }
  | { type: "MARK_SAVED"; id: string; savedText: string }
  | { type: "MARK_SAVING"; id: string; saving: boolean }
  | { type: "PIN"; id: string }
  | { type: "REORDER"; from: number; to: number };

function tabsReducer(state: EditorTabsState, action: TabsAction): EditorTabsState {
  switch (action.type) {
    case "OPEN": {
      const indent = detectIndent(action.file.text ?? "");
      const eol = detectEol(action.file.text ?? "");
      const result = openTab(state, {
        path: action.file.path,
        name: action.file.name,
        text: action.file.text ?? "",
        language: action.file.language,
        bytes: action.file.bytes,
        editable: action.file.editable,
        truncated: action.file.truncated,
        content_base64: action.file.content_base64 ?? undefined,
        indentStyle: indent,
        eol,
      }, { focusLine: action.focusLine, preview: action.preview });
      return result.state;
    }
    case "CLOSE":
      return closeTab(state, action.id);
    case "CLOSE_ALL":
      return closeAllTabs(state);
    case "CLOSE_OTHERS":
      return closeOtherTabs(state, action.id);
    case "CLOSE_RIGHT":
      return closeTabsToRight(state, action.id);
    case "ACTIVATE":
      return activateTab(state, action.index);
    case "NEXT_TAB":
      return nextTab(state);
    case "PREV_TAB":
      return previousTab(state);
    case "UPDATE_TEXT":
      return updateTabText(state, action.id, action.text);
    case "UNDO": {
      const result = undoTab(state, action.id);
      return result.state;
    }
    case "REDO": {
      const result = redoTab(state, action.id);
      return result.state;
    }
    case "MARK_SAVED":
      return markTabSaved(state, action.id, action.savedText);
    case "MARK_SAVING":
      return markTabSaving(state, action.id, action.saving);
    case "PIN": {
      // Pin a tab by re-opening it with preview=false.
      const tab = state.tabs.find((t) => t.id === action.id);
      if (!tab) return state;
      return openTab(state, {
        path: tab.path,
        name: tab.name,
        text: tab.text,
        language: tab.language,
        bytes: tab.bytes,
        editable: tab.editable,
        truncated: tab.truncated,
        content_base64: tab.content_base64,
        indentStyle: tab.indentStyle,
        eol: tab.eol,
      }, { preview: false }).state;
    }
    case "REORDER": {
      // Move tab from one index to another by closing + re-opening.
      const tabs = [...state.tabs];
      const [moved] = tabs.splice(action.from, 1);
      tabs.splice(action.to, 0, moved);
      let active = state.active;
      if (state.active === action.from) active = action.to;
      else if (action.from < state.active && action.to >= state.active) active = state.active - 1;
      else if (action.from > state.active && action.to <= state.active) active = state.active + 1;
      return { ...state, tabs, active };
    }
    default:
      return state;
  }
}

export function Workbench({
  projectPath,
  mode,
  onMode,
  onClose,
  modalOpen = false,
}: {
  projectPath: string;
  mode: WorkbenchMode;
  onMode: (mode: WorkbenchMode) => void;
  onClose: () => void;
  modalOpen?: boolean;
}) {
  const [tabs, dispatch] = useReducer(tabsReducer, createTabsState());
  const [refreshToken] = useReducer((v: number) => v + 1, 0);
  const [focusLine, setFocusLine] = useState<number | null>(null);

  const browserSeen = useRef(false);
  const engineSeen = useRef(false);

  if (mode === "browser") browserSeen.current = true;
  if (mode === "engine") engineSeen.current = true;

  // Reset tabs on project switch.
  useEffect(() => {
    dispatch({ type: "CLOSE_ALL" });
  }, [projectPath]);

  const current = activeTab(tabs);

  // Open file from explorer or external event.
  const openFile = useCallback(async (entry: WorkspaceEntry) => {
    setFocusLine(null);
    try {
      const loaded = await api.readFile(entry.path);
      dispatch({ type: "OPEN", file: loaded, preview: true });
    } catch {
      // Error is not displayed; tab just won't open.
    }
  }, []);

  useEffect(() => {
    const onOpenFile = (raw: Event) => {
      const event = raw as CustomEvent<OpenWorkspaceFileDetail>;
      const { path, line } = event.detail;
      onMode("editor");
      setFocusLine(line);
      void api.readFile(path).then((loaded) => {
        dispatch({ type: "OPEN", file: loaded, focusLine: line, preview: false });
      }).catch((openError) => {
        console.error(`Could not open ${path}:${line} — ${String((openError as { message?: string }).message ?? openError)}`);
      });
    };
    window.addEventListener(OPEN_WORKSPACE_FILE_EVENT, onOpenFile);
    return () => window.removeEventListener(OPEN_WORKSPACE_FILE_EVENT, onOpenFile);
  }, [onMode]);

  const save = useCallback(async () => {
    if (!current) return;
    dispatch({ type: "MARK_SAVING", id: current.id, saving: true });
    try {
      const saved = await api.writeFile(current.path, current.text);
      dispatch({ type: "MARK_SAVED", id: current.id, savedText: saved.text });
    } catch {
      // Error is not displayed; saving indicator will remain until next save.
    } finally {
      dispatch({ type: "MARK_SAVING", id: current.id, saving: false });
    }
  }, [current]);

  const handleUndo = useCallback(() => {
    if (!current) return;
    const result = undoTab(tabs, current.id);
    if (result.text !== null) {
      dispatch({ type: "UNDO", id: current.id });
    }
  }, [current, tabs]);

  const handleRedo = useCallback(() => {
    if (!current) return;
    const result = redoTab(tabs, current.id);
    if (result.text !== null) {
      dispatch({ type: "REDO", id: current.id });
    }
  }, [current, tabs]);

  return (
    <section className="workbench" aria-label="Workbench">
      <div className="workbench-top">
        <ModeSwitch mode={mode} onMode={onMode} />
        <span className="grow" />
        {current?.dirty ? <span className="workbench-dirty">unsaved</span> : null}
        <button className="workbench-close" onClick={onClose} aria-label="Close workbench" title="Close workbench">
          <IconClose size={12} />
        </button>
      </div>

      <div className="workbench-panes">
        <div className="workbench-pane" hidden={mode !== "editor"}>
          <div className="editor-split">
            <FileTree activePath={current?.path ?? null} onOpen={(entry) => void openFile(entry)} refreshToken={refreshToken} />
            <CodeView
              tabs={tabs.tabs}
              activeTab={current}
              focusLine={focusLine}
              onChange={(text) => {
                if (current) dispatch({ type: "UPDATE_TEXT", id: current.id, text });
              }}
              onSave={() => void save()}
              onCloseTab={(id) => dispatch({ type: "CLOSE", id })}
              onSwitchTab={(i) => dispatch({ type: "ACTIVATE", index: i })}
              onPinTab={(id) => dispatch({ type: "PIN", id })}
              onReorderTab={(from, to) => dispatch({ type: "REORDER", from, to })}
              onUndo={handleUndo}
              onRedo={handleRedo}
            />
          </div>
        </div>

        {browserSeen.current ? (
          <div className="workbench-pane" hidden={mode !== "browser"}>
            <BrowserView active={mode === "browser"} occluded={modalOpen} />
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
