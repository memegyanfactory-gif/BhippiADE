/**
 * Multi-tab model for the VS Code–class editor.
 *
 * Every tab operation is a pure function: `(state, …) → state`. Nothing here touches
 * the DOM, so the component is a thin React layer over this model.
 *
 * VS Code preview tabs: clicking a file in the explorer opens it as a preview (italic
 * title, replaced by the next preview). Double-clicking or editing the file pins it.
 * Preview tabs are evicted LRU when the tab bar overflows a configurable cap.
 */

export type TabId = string;

export interface Tab {
  id: TabId;
  /** Display name (just the filename, not the full path). */
  name: string;
  /** Full absolute path — the stable identity. */
  path: string;
  /** The content on disk at open time; dirty = current !== saved. */
  savedText: string;
  /** Current editor buffer. */
  text: string;
  /** Language tag from the backend (extension key). */
  language: string;
  /** File size in bytes, from the backend. */
  bytes: number;
  /** Read-only / binary files cannot be edited. */
  editable: boolean;
  /** File is truncated (>1 MB). */
  truncated: boolean;
  /** base64 content for images. */
  content_base64?: string;
  /** true while the file is being saved. */
  saving: boolean;
  /** true when current buffer !== saved text. */
  dirty: boolean;
  /** true when opened via a single click (preview). Pinned on edit/double-click. */
  preview: boolean;
  /** 1-based line to scroll to on first focus. */
  focusLine?: number;
  /** The file's auto-detected indent style. */
  indentStyle: { useTabs: boolean; size: number };
  /** LF or CRLF. */
  eol: "LF" | "CRLF";
  /** Undo stack (array of snapshots). Capped at 200. */
  undoStack: string[];
  /** Redo stack. */
  redoStack: string[];
}

export interface EditorTabsState {
  tabs: Tab[];
  /** Index into `tabs` of the active tab. -1 when empty. */
  active: number;
  /** Max simultaneous open tabs before preview eviction kicks in. */
  maxTabs: number;
}

/** A result from any mutation that may need the caller to focus the editor. */
export interface TabsResult {
  state: EditorTabsState;
  /** If a tab was opened/switched, this is the path to focus. */
  focusPath?: string;
}

export const DEFAULT_MAX_TABS = 20;

export function createTabsState(maxTabs = DEFAULT_MAX_TABS): EditorTabsState {
  return { tabs: [], active: -1, maxTabs };
}

function makeId(path: string): TabId {
  return path;
}

/**
 * Open (or focus) a file in the tab bar.
 *
 * If the file is already open, it becomes active. If it is not open, a new tab is
 * created. When the cap is exceeded, the oldest preview tab is evicted.
 */
export function openTab(
  state: EditorTabsState,
  file: {
    path: string;
    name: string;
    text: string;
    language: string;
    bytes: number;
    editable: boolean;
    truncated: boolean;
    content_base64?: string;
    indentStyle: { useTabs: boolean; size: number };
    eol: "LF" | "CRLF";
  },
  options?: { focusLine?: number; preview?: boolean },
): TabsResult {
  const id = makeId(file.path);
  const existing = state.tabs.findIndex((t) => t.id === id);

  if (existing !== -1) {
    const tabs = [...state.tabs];
    const tab = { ...tabs[existing], preview: false, focusLine: options?.focusLine };
    tabs.splice(existing, 1);
    tabs.unshift(tab);
    return { state: { ...state, tabs, active: 0 }, focusPath: file.path };
  }

  const preview = options?.preview ?? true;
  const dirty = false;

  const newTab: Tab = {
    id,
    name: file.name,
    path: file.path,
    savedText: file.text,
    text: file.text,
    language: file.language,
    bytes: file.bytes,
    editable: file.editable,
    truncated: file.truncated,
    content_base64: file.content_base64,
    saving: false,
    dirty,
    preview,
    focusLine: options?.focusLine,
    indentStyle: file.indentStyle,
    eol: file.eol,
    undoStack: [],
    redoStack: [],
  };

  let tabs = [newTab, ...state.tabs];

  // Evict oldest preview tabs if over cap.
  if (tabs.length > state.maxTabs) {
    const previewTabs = tabs
      .map((t, i) => ({ t, i }))
      .filter(({ t }) => t.preview && !t.dirty);
    // Evict from the end (oldest previews).
    let overage = tabs.length - state.maxTabs;
    for (let j = previewTabs.length - 1; j >= 0 && overage > 0; j--) {
      tabs.splice(previewTabs[j].i, 1);
      overage--;
    }
  }

  return { state: { ...state, tabs, active: 0 }, focusPath: file.path };
}

/** Close a tab by id. Returns the next active index. */
export function closeTab(state: EditorTabsState, id: TabId): EditorTabsState {
  const index = state.tabs.findIndex((t) => t.id === id);
  if (index === -1) return state;

  const tabs = [...state.tabs];
  tabs.splice(index, 1);

  if (tabs.length === 0) return { ...state, tabs, active: -1 };

  let nextActive = state.active;
  if (index <= state.active) {
    nextActive = Math.max(0, state.active - 1);
  }
  if (nextActive >= tabs.length) nextActive = tabs.length - 1;

  return { ...state, tabs, active: nextActive };
}

/** Close all tabs. */
export function closeAllTabs(state: EditorTabsState): EditorTabsState {
  return { ...state, tabs: [], active: -1 };
}

/** Close all tabs except the given one. */
export function closeOtherTabs(state: EditorTabsState, keepId: TabId): EditorTabsState {
  const tab = state.tabs.find((t) => t.id === keepId);
  if (!tab) return closeAllTabs(state);
  return { ...state, tabs: [tab], active: 0 };
}

/** Close tabs to the right of the given tab. */
export function closeTabsToRight(state: EditorTabsState, id: TabId): EditorTabsState {
  const index = state.tabs.findIndex((t) => t.id === id);
  if (index === -1) return state;
  const tabs = state.tabs.slice(0, index + 1);
  return {
    ...state,
    tabs,
    active: Math.min(state.active, tabs.length - 1),
  };
}

/** Switch to a tab by index. */
export function activateTab(state: EditorTabsState, index: number): EditorTabsState {
  if (index < 0 || index >= state.tabs.length) return state;
  return { ...state, active: index };
}

/** Switch to the next tab. */
export function nextTab(state: EditorTabsState): EditorTabsState {
  if (state.tabs.length <= 1) return state;
  const next = (state.active + 1) % state.tabs.length;
  return activateTab(state, next);
}

/** Switch to the previous tab. */
export function previousTab(state: EditorTabsState): EditorTabsState {
  if (state.tabs.length <= 1) return state;
  const prev = (state.active - 1 + state.tabs.length) % state.tabs.length;
  return activateTab(state, prev);
}

/** Switch to the most recently used tab (Alt+Cycling equivalent). */
export function reopenClosedTab(state: EditorTabsState, closedOrder: Tab[]): EditorTabsState {
  if (closedOrder.length === 0) return state;
  // This is a placeholder — the caller manages the closedOrder stack.
  return state;
}

/** Pin a tab (converts from preview to pinned). */
export function pinTab(state: EditorTabsState, id: TabId): EditorTabsState {
  const index = state.tabs.findIndex((t) => t.id === id);
  if (index === -1) return state;
  const tabs = [...state.tabs];
  tabs[index] = { ...tabs[index], preview: false };
  return { ...state, tabs };
}

/** Move a tab from one position to another (drag reorder). */
export function moveTab(
  state: EditorTabsState,
  fromIndex: number,
  toIndex: number,
): EditorTabsState {
  if (
    fromIndex === toIndex ||
    fromIndex < 0 || fromIndex >= state.tabs.length ||
    toIndex < 0 || toIndex >= state.tabs.length
  ) {
    return state;
  }
  const tabs = [...state.tabs];
  const [moved] = tabs.splice(fromIndex, 1);
  tabs.splice(toIndex, 0, moved);
  // Adjust active index to follow the active tab.
  let active = state.active;
  if (state.active === fromIndex) {
    active = toIndex;
  } else if (fromIndex < state.active && toIndex >= state.active) {
    active = state.active - 1;
  } else if (fromIndex > state.active && toIndex <= state.active) {
    active = state.active + 1;
  }
  return { ...state, tabs, active };
}

/** Update the buffer text of a tab. Pushes onto the undo stack. */
export function updateTabText(
  state: EditorTabsState,
  id: TabId,
  text: string,
): EditorTabsState {
  const index = state.tabs.findIndex((t) => t.id === id);
  if (index === -1) return state;
  const tabs = [...state.tabs];
  const tab = tabs[index];
  if (!tab.editable) return state;

  // Push current text onto undo stack before changing.
  const undoStack = [...tab.undoStack, tab.text];
  if (undoStack.length > 200) undoStack.shift();

  tabs[index] = {
    ...tab,
    text,
    dirty: text !== tab.savedText,
    preview: false, // Editing pins a preview tab.
    undoStack,
    redoStack: [], // New edit clears redo.
  };
  return { ...state, tabs };
}

/** Undo the last edit in a tab. */
export function undoTab(state: EditorTabsState, id: TabId): { state: EditorTabsState; text: string | null } {
  const index = state.tabs.findIndex((t) => t.id === id);
  if (index === -1) return { state, text: null };
  const tab = state.tabs[index];
  if (tab.undoStack.length === 0) return { state, text: null };

  const tabs = [...state.tabs];
  const undoStack = [...tab.undoStack];
  const previous = undoStack.pop()!;
  const redoStack = [...tab.redoStack, tab.text];

  tabs[index] = {
    ...tab,
    text: previous,
    dirty: previous !== tab.savedText,
    undoStack,
    redoStack,
  };
  return { state: { ...state, tabs }, text: previous };
}

/** Redo the last undone edit in a tab. */
export function redoTab(state: EditorTabsState, id: TabId): { state: EditorTabsState; text: string | null } {
  const index = state.tabs.findIndex((t) => t.id === id);
  if (index === -1) return { state, text: null };
  const tab = state.tabs[index];
  if (tab.redoStack.length === 0) return { state, text: null };

  const tabs = [...state.tabs];
  const redoStack = [...tab.redoStack];
  const next = redoStack.pop()!;
  const undoStack = [...tab.undoStack, tab.text];

  tabs[index] = {
    ...tab,
    text: next,
    dirty: next !== tab.savedText,
    undoStack,
    redoStack,
  };
  return { state: { ...state, tabs }, text: next };
}

/** Mark a tab as saved (called after a successful write). */
export function markTabSaved(
  state: EditorTabsState,
  id: TabId,
  savedText: string,
): EditorTabsState {
  const index = state.tabs.findIndex((t) => t.id === id);
  if (index === -1) return state;
  const tabs = [...state.tabs];
  tabs[index] = {
    ...tabs[index],
    savedText,
    dirty: false,
    saving: false,
  };
  return { ...state, tabs };
}

/** Set the saving flag on a tab. */
export function markTabSaving(state: EditorTabsState, id: TabId, saving: boolean): EditorTabsState {
  const index = state.tabs.findIndex((t) => t.id === id);
  if (index === -1) return state;
  const tabs = [...state.tabs];
  tabs[index] = { ...tabs[index], saving };
  return { ...state, tabs };
}

/** Set a focusLine on a tab (e.g. from go-to-line or search result click). */
export function setTabFocusLine(
  state: EditorTabsState,
  id: TabId,
  line: number,
): EditorTabsState {
  const index = state.tabs.findIndex((t) => t.id === id);
  if (index === -1) return state;
  const tabs = [...state.tabs];
  tabs[index] = { ...tabs[index], focusLine: line };
  return { ...state, tabs };
}

/** The active tab object, or null. */
export function activeTab(state: EditorTabsState): Tab | null {
  return state.tabs[state.active] ?? null;
}

/** Get a tab by path. */
export function tabByPath(state: EditorTabsState, path: string): Tab | null {
  return state.tabs.find((t) => t.path === path) ?? null;
}
