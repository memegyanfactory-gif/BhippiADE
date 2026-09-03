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

import type { WorkspaceLayout } from "./WorkspaceOrganizer";
import { reconcileSessionOrder } from "./workspaceState";
export type { WorkspaceLayout };

const MIN_PANEL_WIDTH = 300;
const MAX_PANEL_FRACTION = 0.85;
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

function readSizes(key: string): Record<string, number> {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(key) ?? "{}");
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    return Object.fromEntries(
      Object.entries(value).filter(
        (entry): entry is [string, number] =>
          typeof entry[1] === "number" && Number.isFinite(entry[1]),
      ),
    );
  } catch {
    return {};
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

function defaultBasis(layout: WorkspaceLayout, index: number, count: number): number {
  if (count <= 1) return 900;
  if (layout === "smart" && index === 0) return 640;
  if (layout === "adaptive" && index === 0) return 560;
  return count === 2 ? 540 : 380;
}

function computePanelFlex(
  layout: WorkspaceLayout,
  index: number,
  count: number,
  autoFit: boolean,
  customSize?: number,
  isPrimary = false,
): CSSProperties {
  if (customSize && !autoFit) {
    return {
      flex: `0 0 ${customSize}px`,
      width: `${customSize}px`,
      minWidth: `${MIN_PANEL_WIDTH}px`,
    };
  }

  if (autoFit) {
    if (count <= 1) {
      return { flex: "1 1 100%", width: "100%" };
    }
    if (layout === "smart") {
      const weight = isPrimary ? 1.4 : 0.9;
      return {
        flex: `${weight} 1 0px`,
        minWidth: isPrimary ? "340px" : "260px",
      };
    }
    // Balanced and Adaptive: completely stable equal distribution so clicking a panel never shifts or resizes windows
    return {
      flex: "1 1 0px",
      minWidth: "280px",
    };
  }

  const basis = defaultBasis(layout, index, count);
  return {
    flex: `1 1 ${basis}px`,
    width: `${basis}px`,
    minWidth: `${MIN_PANEL_WIDTH}px`,
  };
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

/**
 * Which slot the pointer is over, given the horizontal centres of the other panels, in
 * order. Left of the first centre is slot 0; right of the last is the end.
 */
export function slotForPointer(centres: readonly number[], pointerX: number): number {
  let slot = 0;
  for (const centre of centres) {
    if (pointerX > centre) slot += 1;
  }
  return slot;
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

type MultiSessionWorkspaceProps = {
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
    onApplyLayout?.(next);
  };
  const [sizes, setSizes] = useState<Record<string, number>>(() =>
    readSizes(`${storagePrefix}:sizes`),
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
  const dragRef = useRef<{ id: string; startX: number; startBasis: number } | null>(null);

  useEffect(() => {
    if (resetKey !== undefined) {
      setSizes({});
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
    window.localStorage.setItem(`${storagePrefix}:sizes`, JSON.stringify(sizes));
  }, [sizes, storagePrefix]);

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

  useEffect(() => {
    if (!autoFit) return;
    setSizes({});
  }, [autoFit, layout, orderedSessions.length]);

  useEffect(() => {
    const onPointerMove = (event: PointerEvent) => {
      const drag = dragRef.current;
      const canvasWidth = canvasRef.current?.getBoundingClientRect().width ?? 0;
      if (!drag || canvasWidth <= 0) return;
      const otherPanelsMin = Math.max(0, (orderedSessions.length - 1) * MIN_PANEL_WIDTH);
      const max = Math.max(
        MIN_PANEL_WIDTH,
        Math.min(Math.round(canvasWidth * MAX_PANEL_FRACTION), canvasWidth - otherPanelsMin - 24),
      );
      const next = Math.min(max, Math.max(MIN_PANEL_WIDTH, drag.startBasis + event.clientX - drag.startX));
      setSizes((current) => ({ ...current, [drag.id]: next }));
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
  }, [orderedSessions.length]);

  const resizePanel = (sessionId: string, delta: number) => {
    const canvasWidth = canvasRef.current?.getBoundingClientRect().width ?? 0;
    const index = orderedSessions.findIndex((session) => session.id === sessionId);
    const basis = sizes[sessionId] ?? defaultBasis(layout, index, orderedSessions.length);
    const otherPanelsMin = Math.max(0, (orderedSessions.length - 1) * MIN_PANEL_WIDTH);
    const max = Math.max(
      MIN_PANEL_WIDTH,
      Math.min(Math.round(canvasWidth * MAX_PANEL_FRACTION), canvasWidth - otherPanelsMin - 24),
    );
    setAutoFit(false);
    setSizes((current) => ({
      ...current,
      [sessionId]: Math.min(max, Math.max(MIN_PANEL_WIDTH, basis + delta)),
    }));
  };

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
    if (!lifted || !drop) return;

    const order = orderedSessions.map((session) => session.id);
    const from = order.indexOf(lifted.id);
    if (finalSnap?.kind === "cell") {
      const template = SNAP_TEMPLATES.find((entry) => entry.id === finalSnap.template);
      if (template) {
        setPanelOrder(moveToIndex(order, lifted.id, finalSnap.cell));
        applyLayout(template.layout);
        setAutoFit(true);
        setSizes({});
        onActivate(lifted.id);
        return;
      }
    }
    if (finalSnap?.kind === "edge") {
      const target = finalSnap.side === "left" ? 0 : order.length - 1;
      setPanelOrder(moveToIndex(order, lifted.id, target));
      setAutoFit(true);
      setSizes({});
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
      lifted = {
        id: press.id,
        width: press.rect.width,
        height: press.rect.height,
        grabX: press.startX - press.rect.left,
        grabY: press.startY - press.rect.top,
        x: press.rect.left,
        y: press.rect.top,
      };
      liftRef.current = lifted;
      setLift(lifted);
      onActivate(press.id);
    }
    liftRef.current = {
      ...lifted,
      x: clientX - lifted.grabX,
      y: clientY - lifted.grabY,
    };

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

      const centres = orderedSessions
        .filter((session) => session.id !== current.id)
        .map((session) => {
          const rect = panelRefs.current.get(session.id)?.getBoundingClientRect();
          return rect ? rect.left + rect.width / 2 : Number.POSITIVE_INFINITY;
        });

      // Calculate slot based on both cursor and window center across canvas slots:
      let nextSlot = slotForPointer(centres, clientX);
      if (canvas && orderedSessions.length > 1) {
        const count = orderedSessions.length;
        const slotWidth = canvas.width / count;
        const dragCenterX = current.x + current.width / 2;
        const relCenter = Math.max(0, Math.min(canvas.width, dragCenterX - canvas.left));
        const relCursor = Math.max(0, Math.min(canvas.width, clientX - canvas.left));
        const slotFromCenter = Math.max(0, Math.min(count - 1, Math.floor(relCenter / slotWidth)));
        const slotFromCursor = Math.max(0, Math.min(count - 1, Math.floor(relCursor / slotWidth)));
        const currentSlot = slotRef.current ?? orderedSessions.findIndex((s) => s.id === current.id);
        if (slotFromCenter > currentSlot || slotFromCursor > currentSlot) {
          nextSlot = Math.max(slotFromCenter, slotFromCursor);
        } else if (slotFromCenter < currentSlot || slotFromCursor < currentSlot) {
          nextSlot = Math.min(slotFromCenter, slotFromCursor);
        } else {
          nextSlot = slotFromCenter;
        }
      }

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

  const placeholderStyle: CSSProperties | null = lift
    ? { flex: `0 0 ${Math.round(lift.width)}px`, width: `${Math.round(lift.width)}px` }
    : null;

  const renderPanel = (session: WorkspaceSession, index: number) => {
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
            flex: "none",
            margin: 0,
          }
        : computePanelFlex(
            layout,
            index,
            orderedSessions.length,
            autoFit,
            sizes[session.id],
            isSmartPrimary,
          );
    const isCli = session.kind === "cli";
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
        }`}
        style={panelStyle}
        onPointerDown={() => onActivate(session.id)}
        onKeyDown={(e) => {
          if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
            e.preventDefault();
            swapWithNeighbor(session.id, e.key === "ArrowLeft" ? "left" : "right");
          }
        }}
        tabIndex={0}
        aria-label={`${session.title} panel. Drag its title bar to move it; press Alt+Left/Right to reorder.`}
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
            <small>{session.provider_label ?? (isCli ? "Terminal" : "Agent chat")}</small>
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
              if (sizes[session.id]) {
                setAutoFit(true);
                setSizes((current) => {
                  const next = { ...current };
                  delete next[session.id];
                  return next;
                });
              } else {
                resizePanel(session.id, 200);
              }
            }}
            title={
              sizes[session.id]
                ? "Return to auto-fit width"
                : "Custom expand this window"
            }
            aria-label={
              sizes[session.id] ? "Return to auto-fit width" : "Custom expand window"
            }
          >
            {sizes[session.id] ? <IconMinimize2 size={13} /> : <IconMaximize2 size={13} />}
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

        <div
          className="session-panel-resizer"
          role="separator"
          aria-orientation="vertical"
          title="Drag to resize panel, double-click to auto-fit"
          onDoubleClick={() => {
            setAutoFit(true);
            setSizes({});
          }}
          onPointerDown={(event) => {
            event.stopPropagation();
            const el = event.currentTarget.parentElement;
            const basis = el?.getBoundingClientRect().width ?? defaultBasis(layout, index, orderedSessions.length);
            dragRef.current = { id: session.id, startX: event.clientX, startBasis: basis };
            setDraggingId(session.id);
            setAutoFit(false);
          }}
        >
          <span className="session-resizer-line" />
        </div>
      </article>
    );
  };

  // While a window is lifted, the row is the others plus one gap at `slot`; the lifted
  // window itself floats as a ghost and is rendered last so it stays on top without
  // leaving the DOM (its chat or terminal keeps its state).
  const row: ReactNode[] = [];
  if (!lift) {
    orderedSessions.forEach((session, index) => row.push(renderPanel(session, index)));
  } else {
    const others = orderedSessions.filter((session) => session.id !== lift.id);
    const gapAt = Math.max(0, Math.min(others.length, slot ?? orderedSessions.findIndex((s) => s.id === lift.id)));
    const placeholder = (
      <div
        key="__placeholder"
        className="session-panel session-panel-placeholder"
        style={placeholderStyle ?? undefined}
        aria-hidden="true"
      />
    );
    others.forEach((session, index) => {
      if (index === gapAt) row.push(placeholder);
      row.push(renderPanel(session, index < gapAt ? index : index + 1));
    });
    if (gapAt >= others.length) row.push(placeholder);
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
          <strong>No sessions in this project yet</strong>
          <span>Start a chat or terminal and it will join this workspace.</span>
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
