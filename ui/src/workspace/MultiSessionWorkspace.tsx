import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { WorkspaceSession } from "../lib/ipc";
import {
  IconChat,
  IconChevronLeft,
  IconChevronRight,
  IconClose,
  IconGrid,
  IconGripVertical,
  IconMaximize2,
  IconMinimize2,
  IconTerminal,
} from "../components/icons";
import { ProviderLogo } from "../components/ProviderLogo";

import {
  CANVAS_GAP_PX,
  MIN_COL_PX,
  MIN_ROW_PX,
  RESIZE_STEP,
  cycleLayout,
  focusTracks,
  isFocusedTracks,
  planLayout,
  resizeTrack,
  slotForPointerGrid,
  trackTemplate,
  type LayoutPlan,
  type PlanArea,
  type WorkspaceLayout,
} from "./layoutPlan";
import { reconcileSessionOrder } from "./workspaceState";
export type { WorkspaceLayout };
/** How far the pointer travels before a press on the title bar becomes a pick-up. */
const LIFT_THRESHOLD_PX = 6;
/** How close to the canvas edge the pointer must be for the half-screen snap. */
const EDGE_SNAP_PX = 28;
/** How close to the top of the canvas the pointer must be for the snap-layout menu. */
const TOP_SNAP_PX = 36;

function readBoolean(key: string, fallback: boolean): boolean {
  try {
    const value = window.localStorage.getItem(key);
    return value === null ? fallback : value === "true";
  } catch {
    return fallback;
  }
}

function readLayout(key: string): WorkspaceLayout {
  try {
    const value = window.localStorage.getItem(key);
    return value === "adaptive" || value === "smart" ? value : "balanced";
  } catch {
    return "balanced";
  }
}

/** The hand-set track weights, or `null` when the layout has never been touched. */
function readTracks(key: string): number[] | null {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(key) ?? "null");
    if (!Array.isArray(value) || value.length === 0) return null;
    return value.every((entry) => typeof entry === "number" && Number.isFinite(entry) && entry > 0)
      ? (value as number[])
      : null;
  } catch {
    return null;
  }
}

function readOrder(key: string): string[] {
  try {
    const saved = window.localStorage.getItem(key);
    if (!saved) return [];
    const parsed = JSON.parse(saved);
    return Array.isArray(parsed) ? parsed.filter((v): v is string => typeof v === "string") : [];
  } catch {
    return [];
  }
}

function statusText(status: WorkspaceSession["status"]): string {
  return status.charAt(0).toUpperCase() + status.slice(1);
}

/** Moves `id` to `index` inside `order` (clamped), keeping everyone else's order. */
export function moveToIndex(order: readonly string[], id: string, index: number): string[] {
  const without = order.filter((entry) => entry !== id);
  const at = Math.max(0, Math.min(without.length, index));
  return [...without.slice(0, at), id, ...without.slice(at)];
}

/* ── snap layouts (Windows 11's, in Bhippi's three layouts) ───────────────────────────
   Dragging a window to the top of the canvas opens these. Each template is one of the
   organizer's layouts drawn as cells; releasing on a cell puts the window in that slot
   and applies the layout, so "snap left half" and "make this the big one" are one drop. */

export type SnapTemplate = {
  id: string;
  label: string;
  layout: WorkspaceLayout;
  /** Cells as CSS grid areas over a 2×3 board; the index is the slot the drop lands in. */
  cells: { area: string }[];
  /** How many windows the template wants; fewer still works (empty cells are hidden). */
  minCount: number;
};

export const SNAP_TEMPLATES: readonly SnapTemplate[] = [
  {
    id: "halves",
    label: "Side by side",
    layout: "balanced",
    cells: [{ area: "1 / 1 / 3 / 2" }, { area: "1 / 2 / 3 / 3" }],
    minCount: 2,
  },
  {
    id: "primary",
    label: "Primary + side",
    layout: "adaptive",
    cells: [{ area: "1 / 1 / 3 / 2" }, { area: "1 / 2 / 3 / 3" }],
    minCount: 2,
  },
  {
    id: "thirds",
    label: "Three columns",
    layout: "balanced",
    cells: [{ area: "1 / 1 / 3 / 2" }, { area: "1 / 2 / 3 / 3" }, { area: "1 / 3 / 3 / 4" }],
    minCount: 3,
  },
  {
    id: "focus",
    label: "Focus + stack",
    layout: "smart",
    cells: [{ area: "1 / 1 / 3 / 2" }, { area: "1 / 2 / 2 / 3" }, { area: "2 / 2 / 3 / 3" }],
    minCount: 3,
  },
];

type Lift = {
  id: string;
  /** The panel's size when it was picked up; the ghost keeps it. */
  width: number;
  height: number;
  /** Where inside the panel the pointer grabbed it. */
  grabX: number;
  grabY: number;
  /** The ghost's top-left, CSS pixels. */
  x: number;
  y: number;
};

type SnapTarget =
  | { kind: "edge"; side: "left" | "right" }
  | { kind: "cell"; template: string; cell: number };

/** What a window says it belongs to, when the canvas spans more than one project. */
export type PanelProject = {
  name: string;
  path: string;
  /** 0–359, from `projectHue`; the window wears it as an accent. */
  hue: number;
};

type MultiSessionWorkspaceProps = {
  /** Namespaces the saved layout. The all-projects board passes its own sentinel. */
  projectPath: string;
  sessions: WorkspaceSession[] | null;
  sessionsError: string | null;
  activeSessionId: string | null;
  renderSession: (session: WorkspaceSession) => ReactNode;
  onActivate: (sessionId: string) => void;
  onFocusSingle: (sessionId: string) => void;
  onCloseSession?: (sessionId: string) => void;
  onNewChat: () => void;
  onNewCli: () => void;
  onRetry: () => void;
  layout?: WorkspaceLayout;
  autoFit?: boolean;
  resetKey?: number;
  onAutoFitChange?: (fit: boolean) => void;
  /** A snap-layout drop changes the layout; the owner of `layout` hears about it here. */
  onApplyLayout?: (layout: WorkspaceLayout) => void;
  /**
   * The project each window belongs to. Left out on a single-project canvas, where every
   * window shares the project already named in the sidebar; the all-projects board fills
   * it in, and each window then wears its project's name and colour.
   */
  projectFor?: (session: WorkspaceSession) => PanelProject | null;
  /** Replaces the "no sessions in this project" copy when the canvas is not one project's. */
  emptyCopy?: { title: string; hint: string };
};

export function MultiSessionWorkspace({
  projectPath,
  sessions,
  sessionsError,
  activeSessionId,
  renderSession,
  onActivate,
  onFocusSingle,
  onCloseSession,
  onNewChat,
  onNewCli,
  onRetry,
  layout: propLayout,
  autoFit: propAutoFit,
  resetKey,
  onAutoFitChange,
  onApplyLayout,
  projectFor,
  emptyCopy,
}: MultiSessionWorkspaceProps) {
  const storagePrefix = `bhippi-multi-workspace:${projectPath}`;
  const [internalAutoFit, setInternalAutoFit] = useState(() => readBoolean(`${storagePrefix}:auto-fit`, true));
  const [internalLayout, setInternalLayout] = useState<WorkspaceLayout>(() => readLayout(`${storagePrefix}:layout`));
  const autoFit = propAutoFit ?? internalAutoFit;
  const layout = propLayout ?? internalLayout;

  const setAutoFit = (fit: boolean) => {
    setInternalAutoFit(fit);
    onAutoFitChange?.(fit);
  };
  const applyLayout = (next: WorkspaceLayout) => {
    setInternalLayout(next);
    setColumnTracks(null);
    setRowTracks(null);
    setInternalAutoFit(true);
    onApplyLayout?.(next);
  };
  // The hand-set grid. `null` means "as the layout planned it"; an array is what the
  // user dragged or keyed the tracks to, and it is kept only while it still describes
  // the same grid — a window opening or closing hands the layout back to the planner.
  const [columnTracks, setColumnTracks] = useState<number[] | null>(() =>
    readTracks(`${storagePrefix}:columns`),
  );
  const [rowTracks, setRowTracks] = useState<number[] | null>(() =>
    readTracks(`${storagePrefix}:rows`),
  );
  const [panelOrder, setPanelOrder] = useState<string[]>(() => readOrder(`${storagePrefix}:order`));
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [recentlySwapped, setRecentlySwapped] = useState<[string, string] | null>(null);

  // The pick-up (SPA-401). `lift` is the ghost that follows the pointer, `slot` the gap
  // the others open for it, `snap` the zone the pointer is over — an edge half or a cell
  // in the snap-layout menu that opens when the window is dragged to the top.
  const [lift, setLift] = useState<Lift | null>(null);
  const [slot, setSlot] = useState<number | null>(null);
  const [snap, setSnap] = useState<SnapTarget | null>(null);
  const [snapMenuOpen, setSnapMenuOpen] = useState(false);
  const pressRef = useRef<{ id: string; startX: number; startY: number; rect: DOMRect } | null>(null);
  const liftRef = useRef<Lift | null>(null);
  const slotRef = useRef<number | null>(null);
  const snapRef = useRef<SnapTarget | null>(null);
  const menuOpenRef = useRef(false);
  const frameRef = useRef<number | null>(null);
  const panelRefs = useRef<Map<string, HTMLElement>>(new Map());
  const snapMenuRef = useRef<HTMLDivElement | null>(null);

  const canvasRef = useRef<HTMLDivElement | null>(null);
  /** A resize in flight: which track boundary, from which starting weights. */
  const dragRef = useRef<{
    axis: "columns" | "rows";
    index: number;
    start: number;
    base: number[];
    extent: number;
  } | null>(null);

  useEffect(() => {
    if (resetKey !== undefined && resetKey > 0) {
      setColumnTracks(null);
      setRowTracks(null);
      setInternalAutoFit(true);
    }
  }, [resetKey]);

  const orderedSessions = useMemo(() => {
    if (!sessions) return [];
    const map = new Map(sessions.map((s) => [s.id, s]));
    const result: WorkspaceSession[] = [];

    // Honor saved user-dragged order first
    for (const id of panelOrder) {
      const s = map.get(id);
      if (s) {
        result.push(s);
        map.delete(id);
      }
    }

    // New sessions appear once at the end; later activity cannot move them.
    return [...result, ...map.values()];
  }, [sessions, panelOrder]);

  useEffect(() => {
    window.localStorage.setItem(`${storagePrefix}:auto-fit`, String(autoFit));
  }, [autoFit, storagePrefix]);

  useEffect(() => {
    window.localStorage.setItem(`${storagePrefix}:layout`, layout);
  }, [layout, storagePrefix]);

  useEffect(() => {
    const save = (key: string, tracks: number[] | null) => {
      if (tracks) window.localStorage.setItem(key, JSON.stringify(tracks));
      else window.localStorage.removeItem(key);
    };
    save(`${storagePrefix}:columns`, columnTracks);
    save(`${storagePrefix}:rows`, rowTracks);
  }, [columnTracks, rowTracks, storagePrefix]);

  useEffect(() => {
    if (panelOrder.length > 0) {
      window.localStorage.setItem(`${storagePrefix}:order`, JSON.stringify(panelOrder));
    } else {
      window.localStorage.removeItem(`${storagePrefix}:order`);
    }
  }, [panelOrder, storagePrefix]);

  // The backend returns newest activity first. Capture the initial visual order and
  // reconcile only real additions/removals so sending in one chat never moves panels.
  useEffect(() => {
    if (!sessions) return;
    setPanelOrder((current) =>
      reconcileSessionOrder(current, sessions.map((session) => session.id)),
    );
  }, [sessions]);

  // Auto-fit is "let the layout decide": it drops whatever was set by hand, and so does
  // a change of layout or a window opening or closing.
  useEffect(() => {
    if (!autoFit) return;
    setColumnTracks(null);
    setRowTracks(null);
  }, [autoFit, layout, orderedSessions.length]);

  // ── the grid, and every way the user can change it ───────────────────────────────

  // The canvas measures itself: how wide and tall it is decides how many columns and
  // rows the plan may use, so a narrow window tiles instead of shaving slivers.
  const [canvasSize, setCanvasSize] = useState({ width: 0, height: 0 });
  useEffect(() => {
    const node = canvasRef.current;
    if (!node || typeof ResizeObserver === "undefined") return undefined;
    const observer = new ResizeObserver(([entry]) => {
      const box = entry.contentRect;
      setCanvasSize((current) =>
        Math.abs(current.width - box.width) < 1 && Math.abs(current.height - box.height) < 1
          ? current
          : { width: box.width, height: box.height },
      );
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [sessions === null, sessionsError]);

  const plan: LayoutPlan = useMemo(
    () =>
      planLayout({
        layout,
        windows: orderedSessions.map((session) => ({
          id: session.id,
          kind: session.kind === "cli" ? "cli" : "chat",
        })),
        canvasWidth: canvasSize.width,
        canvasHeight: canvasSize.height,
        columnOverrides: autoFit ? null : columnTracks,
        rowOverrides: autoFit ? null : rowTracks,
      }),
    [layout, orderedSessions, canvasSize, autoFit, columnTracks, rowTracks],
  );

  const areaOf = useMemo(() => {
    const map = new Map<string, PlanArea>();
    for (const area of plan.areas) map.set(area.id, area);
    return map;
  }, [plan]);

  const setTracks = (axis: "columns" | "rows", next: number[]) => {
    setAutoFit(false);
    if (axis === "columns") setColumnTracks(next);
    else setRowTracks(next);
  };

  /** Hands the grid back to the planner: auto-fit on, nothing set by hand. */
  const equalize = () => {
    setAutoFit(true);
    setColumnTracks(null);
    setRowTracks(null);
  };

  /** One keyboard step across a boundary, in the same units a drag uses. */
  const stepTrack = (sessionId: string, axis: "columns" | "rows", direction: 1 | -1) => {
    const area = areaOf.get(sessionId);
    if (!area) return;
    const weights = axis === "columns" ? plan.columns : plan.rows;
    if (weights.length < 2) return;
    const index =
      axis === "columns" ? area.column + area.columnSpan - 2 : area.row + area.rowSpan - 2;
    const next = resizeTrack(weights, Math.max(0, index), direction * RESIZE_STEP);
    setTracks(axis, next);
  };

  /**
   * Gives one window most of the canvas, and gives it back on a second press.
   *
   * It grows tracks rather than hiding the others, because a window that leaves the
   * layout takes its terminal's scrollback and its chat's scroll position with it.
   */
  const growFocused = (sessionId: string) => {
    const area = areaOf.get(sessionId);
    if (!area) return;
    const column = area.column - 1;
    const row = area.row - 1;
    const grown =
      isFocusedTracks(plan.columns, column) || isFocusedTracks(plan.rows, row);
    if (grown) {
      equalize();
      return;
    }
    setAutoFit(false);
    if (plan.columns.length > 1) setColumnTracks(focusTracks(plan.columns, column));
    if (plan.rows.length > 1) setRowTracks(focusTracks(plan.rows, row));
  };

  /** Dragging a window's right or bottom edge moves that one boundary. */
  const beginTrackDrag = (
    event: React.PointerEvent<HTMLElement>,
    sessionId: string,
    axis: "columns" | "rows",
  ) => {
    const area = areaOf.get(sessionId);
    const canvas = canvasRef.current?.getBoundingClientRect();
    if (!area || !canvas) return;
    const weights = axis === "columns" ? plan.columns : plan.rows;
    const index =
      axis === "columns" ? area.column + area.columnSpan - 2 : area.row + area.rowSpan - 2;
    if (weights.length < 2 || index < 0) return;
    event.stopPropagation();
    dragRef.current = {
      axis,
      index,
      start: axis === "columns" ? event.clientX : event.clientY,
      base: weights,
      extent: axis === "columns" ? canvas.width : canvas.height,
    };
    setDraggingId(sessionId);
    setAutoFit(false);
  };

  useEffect(() => {
    const onPointerMove = (event: PointerEvent) => {
      const drag = dragRef.current;
      if (!drag || drag.extent <= 0) return;
      const travelled = (drag.axis === "columns" ? event.clientX : event.clientY) - drag.start;
      const next = resizeTrack(drag.base, drag.index, travelled / drag.extent);
      if (drag.axis === "columns") setColumnTracks(next);
      else setRowTracks(next);
    };
    const onPointerUp = () => {
      dragRef.current = null;
      setDraggingId(null);
    };
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    };
  }, []);

  const flashSwap = (a: string, b: string) => {
    setRecentlySwapped([a, b]);
    setTimeout(() => {
      setRecentlySwapped((curr) => (curr && curr[0] === a && curr[1] === b ? null : curr));
    }, 600);
  };

  const swapWithNeighbor = (sessionId: string, direction: "left" | "right") => {
    const currentOrder = orderedSessions.map((s) => s.id);
    const idx = currentOrder.indexOf(sessionId);
    if (idx === -1) return;
    const targetIdx = direction === "left" ? idx - 1 : idx + 1;
    if (targetIdx < 0 || targetIdx >= currentOrder.length) return;
    const targetId = currentOrder[targetIdx];
    const nextOrder = [...currentOrder];
    const temp = nextOrder[idx];
    nextOrder[idx] = nextOrder[targetIdx];
    nextOrder[targetIdx] = temp;
    setPanelOrder(nextOrder);
    flashSwap(sessionId, targetId);
  };

  // ── the keyboard ─────────────────────────────────────────────────────────────────

  /**
   * The whole layout from the keyboard, on the chords a terminal multiplexer uses.
   *
   * Ctrl+Alt stands in for herdr's prefix key — a modifier rather than a leader, because
   * a chat composer and a terminal both want every plain key for themselves. The step is
   * 2% of the canvas, the same as herdr's, and it moves the same boundary a drag moves.
   */
  const shortcutTarget = activeSessionId ?? orderedSessions[0]?.id ?? null;
  useEffect(() => {
    if (!shortcutTarget) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (!event.ctrlKey || !event.altKey || event.shiftKey) return;
      // Two canvases are never on screen together, but a hidden one must stay deaf.
      const canvas = canvasRef.current;
      if (!canvas || !canvas.isConnected || canvas.offsetParent === null) return;

      const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
      const act = (run: () => void) => {
        event.preventDefault();
        run();
      };

      switch (key) {
        case "ArrowLeft":
        case "h":
          return act(() => stepTrack(shortcutTarget, "columns", -1));
        case "ArrowRight":
        case "l":
          return act(() => stepTrack(shortcutTarget, "columns", 1));
        case "ArrowUp":
        case "k":
          return act(() => stepTrack(shortcutTarget, "rows", -1));
        case "ArrowDown":
        case "j":
          return act(() => stepTrack(shortcutTarget, "rows", 1));
        case "\\":
          return act(equalize);
        case " ":
          return act(() => {
            applyLayout(cycleLayout(layout));
            setAutoFit(true);
            setColumnTracks(null);
            setRowTracks(null);
          });
        case "z":
          return act(() => growFocused(shortcutTarget));
        default:
          return undefined;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // Every handler reads the current plan through the closure this effect is rebuilt with.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shortcutTarget, plan, layout, autoFit]);

  // ── the pick-up ──────────────────────────────────────────────────────────────────

  const templatesForCount = (count: number) =>
    SNAP_TEMPLATES.filter((template) => template.minCount <= Math.max(2, count));

  /** Where the pointer is relative to the canvas: a menu cell, an edge half, or nothing. */
  const detectSnap = (clientX: number, clientY: number): SnapTarget | null => {
    const canvas = canvasRef.current?.getBoundingClientRect();
    if (!canvas) return null;
    if (menuOpenRef.current && snapMenuRef.current) {
      const cells = snapMenuRef.current.querySelectorAll<HTMLElement>("[data-snap-cell]");
      for (const cell of cells) {
        const rect = cell.getBoundingClientRect();
        if (
          clientX >= rect.left &&
          clientX <= rect.right &&
          clientY >= rect.top &&
          clientY <= rect.bottom
        ) {
          return {
            kind: "cell",
            template: cell.dataset.snapTemplate ?? "",
            cell: Number(cell.dataset.snapCell ?? 0),
          };
        }
      }
    }
    if (clientX <= canvas.left + EDGE_SNAP_PX) return { kind: "edge", side: "left" };
    if (clientX >= canvas.right - EDGE_SNAP_PX) return { kind: "edge", side: "right" };
    return null;
  };

  const settleLift = (drop: boolean) => {
    const lifted = liftRef.current;
    const finalSlot = slotRef.current;
    const finalSnap = snapRef.current;
    pressRef.current = null;
    liftRef.current = null;
    slotRef.current = null;
    snapRef.current = null;
    menuOpenRef.current = false;
    if (frameRef.current !== null) {
      cancelAnimationFrame(frameRef.current);
      frameRef.current = null;
    }
    setLift(null);
    setSlot(null);
    setSnap(null);
    setSnapMenuOpen(false);
    if (lifted) {
      const node = panelRefs.current.get(lifted.id);
      if (node) {
        node.style.left = "";
        node.style.top = "";
      }
    }
    if (!lifted || !drop) return;

    const order = orderedSessions.map((session) => session.id);
    const from = order.indexOf(lifted.id);
    if (finalSnap?.kind === "cell") {
      const template = SNAP_TEMPLATES.find((entry) => entry.id === finalSnap.template);
      if (template) {
        setPanelOrder(moveToIndex(order, lifted.id, finalSnap.cell));
        applyLayout(template.layout);
        equalize();
        onActivate(lifted.id);
        return;
      }
    }
    if (finalSnap?.kind === "edge") {
      const target = finalSnap.side === "left" ? 0 : order.length - 1;
      setPanelOrder(moveToIndex(order, lifted.id, target));
      equalize();
      onActivate(lifted.id);
      if (from !== target) flashSwap(lifted.id, order[target] ?? lifted.id);
      return;
    }
    if (finalSlot !== null) {
      // Dropped where the user left it: the gap the others opened is the new place.
      setPanelOrder(moveToIndex(order, lifted.id, finalSlot));
      onActivate(lifted.id);
      if (finalSlot !== from) flashSwap(lifted.id, order[Math.min(finalSlot, order.length - 1)] ?? lifted.id);
    }
  };

  const beginPress = (event: React.PointerEvent<HTMLElement>, sessionId: string) => {
    if (event.button !== 0) return;
    if ((event.target as HTMLElement).closest("button")) return;
    const panel = panelRefs.current.get(sessionId);
    if (!panel) return;
    pressRef.current = {
      id: sessionId,
      startX: event.clientX,
      startY: event.clientY,
      rect: panel.getBoundingClientRect(),
    };
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // Capture is a nicety; the move and up handlers still see the pointer.
    }
  };

  const handlePointerMoveAction = (clientX: number, clientY: number) => {
    const press = pressRef.current;
    if (!press) return;
    let lifted = liftRef.current;
    if (!lifted) {
      const travelled = Math.hypot(clientX - press.startX, clientY - press.startY);
      if (travelled < LIFT_THRESHOLD_PX) return;
      const grabX = press.startX - press.rect.left;
      const grabY = press.startY - press.rect.top;
      lifted = {
        id: press.id,
        width: press.rect.width,
        height: press.rect.height,
        grabX,
        grabY,
        x: clientX - grabX,
        y: clientY - grabY,
      };
      liftRef.current = lifted;
      setLift(lifted);
      onActivate(press.id);
    }
    const nextX = clientX - lifted.grabX;
    const nextY = clientY - lifted.grabY;
    liftRef.current = {
      ...lifted,
      x: nextX,
      y: nextY,
    };

    const panelNode = panelRefs.current.get(lifted.id);
    if (panelNode) {
      panelNode.style.left = `${Math.round(nextX)}px`;
      panelNode.style.top = `${Math.round(nextY)}px`;
    }

    // The ghost, the gap and the snap zone are settled once per frame so a fast drag
    // stays smooth; the ghost itself is positioned from the latest pointer.
    if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
    frameRef.current = requestAnimationFrame(() => {
      frameRef.current = null;
      const current = liftRef.current;
      if (!current) return;
      setLift(current);
      const canvas = canvasRef.current?.getBoundingClientRect();
      const nearTop = Boolean(canvas && clientY <= canvas.top + TOP_SNAP_PX);
      const menuRect = snapMenuRef.current?.getBoundingClientRect();
      const insideMenu = Boolean(
        menuRect &&
          clientX >= menuRect.left - 8 &&
          clientX <= menuRect.right + 8 &&
          clientY >= menuRect.top - 8 &&
          clientY <= menuRect.bottom + 8,
      );
      const openMenu = orderedSessions.length >= 2 && (nearTop || (menuOpenRef.current && insideMenu));
      if (openMenu !== menuOpenRef.current) {
        menuOpenRef.current = openMenu;
        setSnapMenuOpen(openMenu);
      }

      // Where the window would land, read left to right and top to bottom. The canvas
      // tiles into rows now, so a pointer low on the screen is past every window above
      // it — comparing x centres alone would have put a second-row drop back in row one.
      const rects = orderedSessions
        .filter((session) => session.id !== current.id)
        .map((session) => {
          const rect = panelRefs.current.get(session.id)?.getBoundingClientRect();
          return rect
            ? { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom }
            : {
                left: Number.POSITIVE_INFINITY,
                right: Number.POSITIVE_INFINITY,
                top: Number.POSITIVE_INFINITY,
                bottom: Number.POSITIVE_INFINITY,
              };
        });
      const nextSlot = slotForPointerGrid(rects, clientX, clientY);

      if (nextSlot !== slotRef.current) {
        slotRef.current = nextSlot;
        setSlot(nextSlot);
      }
      const nextSnap = detectSnap(clientX, clientY);
      if (JSON.stringify(nextSnap) !== JSON.stringify(snapRef.current)) {
        snapRef.current = nextSnap;
        setSnap(nextSnap);
      }
    });
  };

  const movePress = (event: React.PointerEvent<HTMLElement>) => {
    handlePointerMoveAction(event.clientX, event.clientY);
  };

  const endPress = (event?: React.PointerEvent<HTMLElement>) => {
    if (event) {
      try {
        event.currentTarget.releasePointerCapture(event.pointerId);
      } catch {
        // Nothing to release when capture never took.
      }
    }
    if (!liftRef.current) {
      pressRef.current = null;
      return;
    }
    settleLift(true);
  };

  // Window-level move and up listeners ensure dragging never drops if pointer moves outside the header
  useEffect(() => {
    const onWindowPointerMove = (event: PointerEvent) => {
      if (!pressRef.current) return;
      handlePointerMoveAction(event.clientX, event.clientY);
    };
    const onWindowPointerUp = () => {
      if (!pressRef.current && !liftRef.current) return;
      if (!liftRef.current) {
        pressRef.current = null;
        return;
      }
      settleLift(true);
    };
    window.addEventListener("pointermove", onWindowPointerMove);
    window.addEventListener("pointerup", onWindowPointerUp);
    return () => {
      window.removeEventListener("pointermove", onWindowPointerMove);
      window.removeEventListener("pointerup", onWindowPointerUp);
    };
  }, [orderedSessions]);

  // Escape puts the window back where it was picked up.
  const liftActive = lift !== null;
  useEffect(() => {
    if (!liftActive) return undefined;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") settleLift(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // settleLift reads refs only; the listener needs to exist just while a window is lifted.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [liftActive]);

  /** A window's cell, as CSS grid lines. */
  const cellStyle = (area: PlanArea | undefined): CSSProperties =>
    area
      ? {
          gridColumn: `${area.column} / span ${area.columnSpan}`,
          gridRow: `${area.row} / span ${area.rowSpan}`,
        }
      : {};

  const renderPanel = (session: WorkspaceSession, index: number, area?: PlanArea) => {
    const isActive = session.id === activeSessionId;
    const isSmartPrimary = layout === "smart" && index === 0;
    const isLifted = lift?.id === session.id;
    const panelStyle: CSSProperties =
      isLifted && lift
        ? {
            position: "fixed",
            left: `${Math.round(lift.x)}px`,
            top: `${Math.round(lift.y)}px`,
            width: `${Math.round(lift.width)}px`,
            height: `${Math.round(lift.height)}px`,
            margin: 0,
          }
        : cellStyle(area);
    // Which boundaries this window owns: the one on its right, and the one under it.
    const lastColumn = area ? area.column + area.columnSpan - 1 : 1;
    const lastRow = area ? area.row + area.rowSpan - 1 : 1;
    const canResizeWidth = Boolean(area) && lastColumn < plan.columns.length;
    const canResizeHeight = Boolean(area) && lastRow < plan.rows.length;
    const isGrown =
      Boolean(area) &&
      (isFocusedTracks(plan.columns, (area as PlanArea).column - 1) ||
        isFocusedTracks(plan.rows, (area as PlanArea).row - 1));
    const isCli = session.kind === "cli";
    const owner = projectFor?.(session) ?? null;
    const isRecentlySwapped = Boolean(
      recentlySwapped && (recentlySwapped[0] === session.id || recentlySwapped[1] === session.id),
    );

    return (
      <article
        key={session.id}
        ref={(node) => {
          if (node) panelRefs.current.set(session.id, node);
          else panelRefs.current.delete(session.id);
        }}
        className={`session-panel${isActive ? " active" : ""}${
          draggingId === session.id ? " resizing" : ""
        }${isLifted ? " is-lifted" : ""}${isRecentlySwapped ? " panel-just-swapped" : ""}${
          isSmartPrimary ? " smart-primary" : ""
        }${owner ? " has-project" : ""}`}
        style={
          owner
            ? ({ ...panelStyle, "--panel-hue": String(owner.hue) } as CSSProperties)
            : panelStyle
        }
        onPointerDown={() => onActivate(session.id)}
        onPointerDownCapture={() => onActivate(session.id)}
        onFocusCapture={() => onActivate(session.id)}
        onKeyDown={(e) => {
          // Alt+Arrow moves the window; Ctrl+Alt+Arrow moves the split beside it. The
          // guard matters: without it one chord did both, and a resize walked the window
          // across the canvas.
          if (e.ctrlKey || e.metaKey) return;
          if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
            e.preventDefault();
            swapWithNeighbor(session.id, e.key === "ArrowLeft" ? "left" : "right");
          }
        }}
        tabIndex={0}
        aria-label={`${session.title}${owner ? ` in ${owner.name}` : ""} panel. Drag its title bar to move it; press Alt+Left/Right to reorder.`}
      >
        <header
          className="session-panel-head"
          title={`${session.title} — drag this bar to move the window; drag to an edge or the top to snap`}
          onPointerDown={(event) => beginPress(event, session.id)}
          onPointerMove={movePress}
          onPointerUp={endPress}
          onPointerCancel={() => settleLift(false)}
        >
          <div className="session-panel-reorder-group" onClick={(e) => e.stopPropagation()}>
            {index > 0 && (
              <button
                type="button"
                className="session-panel-move-btn"
                onClick={(e) => {
                  e.stopPropagation();
                  swapWithNeighbor(session.id, "left");
                }}
                title="Move window left"
                aria-label="Move window left"
              >
                <IconChevronLeft size={11} />
              </button>
            )}
            <span
              className="session-panel-drag-handle"
              title="Drag to move this window (or use ‹ › or Alt+Left/Right)"
              aria-hidden="true"
              onPointerDown={(event) => beginPress(event, session.id)}
            >
              <IconGripVertical size={13} />
            </span>
            {index < orderedSessions.length - 1 && (
              <button
                type="button"
                className="session-panel-move-btn"
                onClick={(e) => {
                  e.stopPropagation();
                  swapWithNeighbor(session.id, "right");
                }}
                title="Move window right"
                aria-label="Move window right"
              >
                <IconChevronRight size={11} />
              </button>
            )}
          </div>
          <span className="session-panel-provider" aria-hidden="true">
            {isCli ? (
              <IconTerminal size={14} />
            ) : session.provider ? (
              <ProviderLogo id={session.provider} size={16} />
            ) : (
              <IconChat size={14} />
            )}
          </span>
          <span className="session-panel-title" title={session.title}>
            <strong>{session.title.replace(/^CLI:\s*/, "")}</strong>
            <small>
              {owner ? (
                <span className="session-panel-project" title={owner.path}>
                  {owner.name}
                </span>
              ) : null}
              {session.provider_label ?? (isCli ? "Terminal" : "Agent chat")}
            </small>
          </span>
          <span className={`session-panel-status st-${session.status}`}>
            <i aria-hidden="true" />
            {statusText(session.status)}
          </span>
          <button
            type="button"
            className="session-panel-action"
            onClick={(event) => {
              event.stopPropagation();
              growFocused(session.id);
            }}
            title={
              isGrown
                ? "Even the windows out again (Ctrl+Alt+\)"
                : "Grow this window (Ctrl+Alt+Z)"
            }
            aria-label={isGrown ? "Even the windows out" : "Grow this window"}
          >
            {isGrown ? <IconMinimize2 size={13} /> : <IconMaximize2 size={13} />}
          </button>
          <button
            type="button"
            className="session-panel-action"
            onClick={(event) => {
              event.stopPropagation();
              onFocusSingle(session.id);
            }}
            title="Open this session in Single mode"
            aria-label="Open this session in Single mode"
          >
            <IconGrid size={13} />
          </button>
          {onCloseSession ? (
            <button
              type="button"
              className="session-panel-action session-panel-close"
              onClick={(event) => {
                event.stopPropagation();
                onCloseSession(session.id);
              }}
              title="Close session"
              aria-label={`Close ${session.title}`}
            >
              <IconClose size={12} />
            </button>
          ) : null}
        </header>

        <div className="session-panel-body">{renderSession(session)}</div>

        {canResizeWidth ? (
          <div
            className="session-panel-resizer"
            role="separator"
            aria-orientation="vertical"
            title="Drag to move this split (Ctrl+Alt+←/→) · double-click to even the windows out"
            onDoubleClick={equalize}
            onPointerDown={(event) => beginTrackDrag(event, session.id, "columns")}
          >
            <span className="session-resizer-line" />
          </div>
        ) : null}

        {canResizeHeight ? (
          <div
            className="session-panel-resizer horizontal"
            role="separator"
            aria-orientation="horizontal"
            title="Drag to move this split (Ctrl+Alt+↑/↓) · double-click to even the windows out"
            onDoubleClick={equalize}
            onPointerDown={(event) => beginTrackDrag(event, session.id, "rows")}
          >
            <span className="session-resizer-line" />
          </div>
        ) : null}
      </article>
    );
  };

  // While a window is lifted, the row is the others plus one gap at `slot`; the lifted
  // window itself floats as a ghost and is rendered last so it stays on top without
  // leaving the DOM (its chat or terminal keeps its state).
  // The cells belong to positions, not to windows: while one is lifted the gap takes its
  // cell and everyone after it moves up one, which is what makes the drop preview honest.
  const row: ReactNode[] = [];
  if (!lift) {
    orderedSessions.forEach((session, index) => row.push(renderPanel(session, index, plan.areas[index])));
  } else {
    const others = orderedSessions.filter((session) => session.id !== lift.id);
    const gapAt = Math.max(0, Math.min(others.length, slot ?? orderedSessions.findIndex((s) => s.id === lift.id)));
    const slots: (WorkspaceSession | null)[] = [...others];
    slots.splice(gapAt, 0, null);
    slots.forEach((session, position) => {
      if (!session) {
        row.push(
          <div
            key="__placeholder"
            className="session-panel session-panel-placeholder"
            style={cellStyle(plan.areas[position])}
            aria-hidden="true"
          />,
        );
        return;
      }
      row.push(renderPanel(session, position, plan.areas[position]));
    });
    const lifted = orderedSessions.find((session) => session.id === lift.id);
    if (lifted) row.push(renderPanel(lifted, orderedSessions.indexOf(lifted)));
  }

  return (
    <section className="multi-session-workspace" aria-label="Multi-session workspace">
      {sessionsError ? (
        <div className="multi-workspace-state error" role="alert">
          <strong>Sessions could not be loaded.</strong>
          <span>{sessionsError}</span>
          <button type="button" onClick={onRetry}>
            Retry
          </button>
        </div>
      ) : sessions === null ? (
        <div className="multi-workspace-state loading" role="status">
          <span className="multi-loading-line" />
          <strong>Loading project sessions…</strong>
        </div>
      ) : orderedSessions.length === 0 ? (
        <div className="multi-workspace-state empty">
          <IconGrid size={22} />
          <strong>{emptyCopy?.title ?? "No sessions in this project yet"}</strong>
          <span>{emptyCopy?.hint ?? "Start a chat or terminal and it will join this workspace."}</span>
          <div>
            <button type="button" onClick={onNewChat}>
              <IconChat size={13} /> New chat
            </button>
            <button type="button" onClick={onNewCli}>
              <IconTerminal size={13} /> New CLI
            </button>
          </div>
        </div>
      ) : (
        <div
          ref={canvasRef}
          className={`multi-workspace-canvas layout-${layout}${draggingId ? " resizing" : ""}${
            lift ? " lifting" : ""
          }`}
          data-count={orderedSessions.length}
          data-rows={plan.rows.length}
          style={{
            gridTemplateColumns: trackTemplate(plan.columns, MIN_COL_PX),
            gridTemplateRows: trackTemplate(plan.rows, MIN_ROW_PX),
            gap: `${CANVAS_GAP_PX}px`,
          }}
        >
          {/* The half-screen preview at either edge, as Windows draws it. */}
          {lift && snap?.kind === "edge" ? (
            <div className={`snap-edge-preview ${snap.side}`} aria-hidden="true" />
          ) : null}

          {/* The snap-layout menu, dropped from the top edge while a window is lifted. */}
          {lift && snapMenuOpen ? (
            <div className="snap-layouts" ref={snapMenuRef} role="presentation">
              <div className="snap-layouts-title">Snap layout</div>
              <div className="snap-layouts-grid">
                {templatesForCount(orderedSessions.length).map((template) => (
                  <div
                    key={template.id}
                    className={`snap-template${template.layout === layout ? " current" : ""}`}
                    title={template.label}
                  >
                    {template.cells.map((cell, index) => {
                      const hot =
                        snap?.kind === "cell" &&
                        snap.template === template.id &&
                        snap.cell === index;
                      return (
                        <span
                          key={index}
                          className={`snap-cell${hot ? " hot" : ""}`}
                          style={{ gridArea: cell.area }}
                          data-snap-cell={index}
                          data-snap-template={template.id}
                        />
                      );
                    })}
                  </div>
                ))}
              </div>
            </div>
          ) : null}

          {row}
        </div>
      )}
    </section>
  );
}
