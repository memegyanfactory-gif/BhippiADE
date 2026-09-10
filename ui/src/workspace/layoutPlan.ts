/**
 * Where every window sits on the canvas, as a CSS grid.
 *
 * The canvas is a grid of column and row *tracks*; a window occupies one cell, and the
 * primary window may span several. That is the whole model, and it buys three things a
 * row of flex items could not give:
 *
 *  - windows tile into rows instead of being squeezed into ever-thinner columns;
 *  - resizing is resizing a *track*, so the neighbour gives way exactly as it does in a
 *    terminal multiplexer (herdr, tmux) — no window is ever left with nothing;
 *  - the same numbers describe a drag, a keyboard step and a saved layout, so all three
 *    can never disagree.
 *
 * Everything here is pure: no React, no DOM, no storage. The workspace hands it the
 * windows and the canvas size and gets back tracks and areas.
 */

export type WorkspaceLayout = "balanced" | "adaptive" | "smart";

/** Narrower than this and a chat's composer starts wrapping badly. */
export const MIN_COL_PX = 300;
/** Shorter than this and a pane is a title bar with a sliver under it. */
export const MIN_ROW_PX = 220;
/** The gap between windows, matching `--space-2` in the stylesheet. */
export const CANVAS_GAP_PX = 8;
/** One keyboard resize step, as a fraction of the canvas. Herdr's is 2%. */
export const RESIZE_STEP = 0.02;

export type PanelKind = "chat" | "cli";

export type PlanWindow = {
  id: string;
  kind: PanelKind;
};

/** A window's cell, 1-based like CSS grid lines. */
export type PlanArea = {
  id: string;
  column: number;
  columnSpan: number;
  row: number;
  rowSpan: number;
};

export type LayoutPlan = {
  /** Column track weights, in `fr`. */
  columns: number[];
  /** Row track weights, in `fr`. */
  rows: number[];
  areas: PlanArea[];
};

export type PlanInput = {
  layout: WorkspaceLayout;
  windows: readonly PlanWindow[];
  canvasWidth: number;
  canvasHeight: number;
  /** Track weights the user set by hand. Used only when they still fit the plan. */
  columnOverrides?: readonly number[] | null;
  rowOverrides?: readonly number[] | null;
};

function clamp(value: number, low: number, high: number): number {
  return Math.max(low, Math.min(high, value));
}

/** How many columns of usable width the canvas can actually hold. */
export function columnsThatFit(canvasWidth: number, cap = 4): number {
  if (!Number.isFinite(canvasWidth) || canvasWidth <= 0) return cap;
  const fits = Math.floor((canvasWidth + CANVAS_GAP_PX) / (MIN_COL_PX + CANVAS_GAP_PX));
  return clamp(fits, 1, cap);
}

/** How many rows of usable height the canvas can actually hold. */
export function rowsThatFit(canvasHeight: number, cap = 3): number {
  if (!Number.isFinite(canvasHeight) || canvasHeight <= 0) return cap;
  const fits = Math.floor((canvasHeight + CANVAS_GAP_PX) / (MIN_ROW_PX + CANVAS_GAP_PX));
  return clamp(fits, 1, cap);
}

/**
 * What a window is worth in width.
 *
 * A terminal reads fine narrow — it is 80 columns of text with no composer — while a
 * chat wants room for its message column and its input. Smart fit is the only layout
 * that spends this; the others treat every window alike on purpose.
 */
function widthWeight(window: PlanWindow): number {
  return window.kind === "cli" ? 0.82 : 1.15;
}

/** Row-major placement into `columns` columns, with the last row spread to fill. */
function tile(
  windows: readonly PlanWindow[],
  columns: number,
  firstRow = 1,
): { areas: PlanArea[]; rows: number } {
  const areas: PlanArea[] = [];
  const rows = Math.max(1, Math.ceil(windows.length / columns));
  for (let index = 0; index < windows.length; index += 1) {
    const row = Math.floor(index / columns);
    const column = index % columns;
    const inThisRow = Math.min(columns, windows.length - row * columns);
    // A short last row spreads over the free columns rather than leaving a hole.
    const span = column === inThisRow - 1 ? columns - column : 1;
    areas.push({
      id: windows[index].id,
      column: column + 1,
      columnSpan: span,
      row: firstRow + row,
      rowSpan: 1,
    });
  }
  return { areas, rows };
}

/** Equal columns, tiling into rows once the canvas can no longer hold them side by side. */
function planBalanced(input: PlanInput): LayoutPlan {
  const count = input.windows.length;
  const columns = Math.max(1, Math.min(count, columnsThatFit(input.canvasWidth)));
  const { areas, rows } = tile(input.windows, columns);
  return {
    columns: Array.from({ length: columns }, () => 1),
    rows: Array.from({ length: rows }, () => 1),
    areas,
  };
}

/** The first window wide on the left, everything else tiled beside it. */
function planMainLeft(input: PlanInput, primaryWeight: number, sideCap: number): LayoutPlan {
  const [primary, ...rest] = input.windows;
  const fits = columnsThatFit(input.canvasWidth);
  const roomForRows = rowsThatFit(input.canvasHeight);

  // A canvas too narrow for two columns stacks instead, primary first and tallest.
  if (fits <= 1) {
    return {
      columns: [1],
      rows: input.windows.map((_, index) => (index === 0 ? 1.4 : 1)),
      areas: input.windows.map((window, index) => ({
        id: window.id,
        column: 1,
        columnSpan: 1,
        row: index + 1,
        rowSpan: 1,
      })),
    };
  }

  let sideColumns = Math.max(1, Math.min(rest.length, Math.min(fits - 1, sideCap)));
  // Wrapping is only worth it while the rows stay tall enough to work in; past that,
  // widen the side instead of stacking rows nobody can read.
  if (Math.ceil(rest.length / sideColumns) > roomForRows) {
    sideColumns = Math.min(rest.length, fits - 1, Math.ceil(rest.length / roomForRows));
  }
  const side = tile(rest, sideColumns);
  const rows = side.rows;

  // Each side column is worth what the windows sitting in it are worth.
  const sideWeights = Array.from({ length: sideColumns }, (_, column) => {
    const inColumn = rest.filter((_, index) => index % sideColumns === column);
    if (inColumn.length === 0) return 1;
    return inColumn.reduce((sum, window) => sum + widthWeight(window), 0) / inColumn.length;
  });

  return {
    columns: [primaryWeight, ...sideWeights],
    rows: Array.from({ length: rows }, () => 1),
    areas: [
      { id: primary.id, column: 1, columnSpan: 1, row: 1, rowSpan: rows },
      ...side.areas.map((area) => ({ ...area, column: area.column + 1 })),
    ],
  };
}

/**
 * Smart fit: the layout that reads the windows it has been given.
 *
 * It looks at how many there are, what kind each one is and how much canvas there
 * actually is, then builds the tidiest arrangement that keeps every window usable — the
 * primary dominant, terminals narrower than chats, the rest tiled rather than shaved
 * into slivers, and never more rows than the canvas is tall enough for.
 */
function planSmart(input: PlanInput): LayoutPlan {
  const count = input.windows.length;
  if (count <= 1) return { columns: [1], rows: [1], areas: singleArea(input.windows) };

  if (count === 2) {
    const fits = columnsThatFit(input.canvasWidth);
    const [first, second] = input.windows;
    if (fits <= 1) {
      return {
        columns: [1],
        rows: [1.35, 1],
        areas: [
          { id: first.id, column: 1, columnSpan: 1, row: 1, rowSpan: 1 },
          { id: second.id, column: 1, columnSpan: 1, row: 2, rowSpan: 1 },
        ],
      };
    }
    return {
      columns: [widthWeight(first) * 1.35, widthWeight(second)],
      rows: [1],
      areas: [
        { id: first.id, column: 1, columnSpan: 1, row: 1, rowSpan: 1 },
        { id: second.id, column: 2, columnSpan: 1, row: 1, rowSpan: 1 },
      ],
    };
  }

  // Three or more: one dominant window with the rest tiled next to it, at most two side
  // columns unless the canvas is too short to stack them.
  return planMainLeft(input, 1.45, 2);
}

function singleArea(windows: readonly PlanWindow[]): PlanArea[] {
  return windows.map((window) => ({
    id: window.id,
    column: 1,
    columnSpan: 1,
    row: 1,
    rowSpan: 1,
  }));
}

/** Overrides only survive while they still describe the same grid. */
function applyOverrides(track: number[], overrides?: readonly number[] | null): number[] {
  if (!overrides || overrides.length !== track.length) return track;
  if (overrides.some((value) => !Number.isFinite(value) || value <= 0)) return track;
  return [...overrides];
}

/** The grid for these windows: column and row tracks, and the cell each window sits in. */
export function planLayout(input: PlanInput): LayoutPlan {
  if (input.windows.length === 0) return { columns: [1], rows: [1], areas: [] };
  if (input.windows.length === 1) {
    return { columns: [1], rows: [1], areas: singleArea(input.windows) };
  }

  const plan =
    input.layout === "smart"
      ? planSmart(input)
      : input.layout === "adaptive"
        ? planMainLeft(input, 1.6, 3)
        : planBalanced(input);

  return {
    columns: applyOverrides(plan.columns, input.columnOverrides),
    rows: applyOverrides(plan.rows, input.rowOverrides),
    areas: plan.areas,
  };
}

/** `grid-template-columns` / `-rows` for a set of track weights. */
export function trackTemplate(weights: readonly number[], minPx: number): string {
  return weights.map((weight) => `minmax(${minPx}px, ${weight}fr)`).join(" ");
}

/**
 * Moves weight across one boundary, the way dragging a split does: what one track gains
 * the next one gives up, and neither may fall under `minShare` of the canvas.
 *
 * `delta` is a fraction of the whole canvas, so a 2% keyboard step and a 40px drag are
 * the same kind of number. Returns the original array when the move is not possible, so
 * holding a key against the edge is a no-op rather than a slow drift.
 */
export function resizeTrack(
  weights: readonly number[],
  index: number,
  delta: number,
  minShare = 0.12,
): number[] {
  if (index < 0 || index >= weights.length || weights.length < 2 || delta === 0) {
    return [...weights];
  }
  // Grow against the neighbour on the right; the last track pushes into its left one.
  const partner = index === weights.length - 1 ? index - 1 : index + 1;
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  if (total <= 0) return [...weights];

  const shares = weights.map((weight) => weight / total);
  const nextSelf = shares[index] + delta;
  const nextPartner = shares[partner] - delta;
  if (nextSelf < minShare || nextPartner < minShare) return [...weights];

  const next = [...shares];
  next[index] = nextSelf;
  next[partner] = nextPartner;
  return next.map((share) => share * total);
}

/**
 * Track weights that give one window most of the canvas, for the "grow this one" key.
 *
 * The others keep an equal share of what is left, so nothing disappears — a zoom that
 * unmounts panes would throw away a terminal's scrollback and a chat's scroll position.
 */
export function focusTracks(weights: readonly number[], index: number, share = 0.7): number[] {
  if (weights.length < 2 || index < 0 || index >= weights.length) return [...weights];
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  const rest = (total * (1 - share)) / (weights.length - 1);
  return weights.map((_, position) => (position === index ? total * share : rest));
}

/** True when these tracks already read as "one window grown", within a hair. */
export function isFocusedTracks(weights: readonly number[], index: number, share = 0.7): boolean {
  if (weights.length < 2 || index < 0 || index >= weights.length) return false;
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  if (total <= 0) return false;
  return Math.abs(weights[index] / total - share) < 0.02;
}

/** The layout after this one, for the cycle key. */
export function cycleLayout(current: WorkspaceLayout): WorkspaceLayout {
  return current === "balanced" ? "adaptive" : current === "adaptive" ? "smart" : "balanced";
}

/**
 * Which slot a dragged window would land in, reading the canvas left to right, top to
 * bottom. A one-row canvas is the special case where every rect shares a band, which is
 * why this replaced the old "compare the x centres" version once windows could tile.
 */
export function slotForPointerGrid(
  rects: readonly { left: number; right: number; top: number; bottom: number }[],
  pointerX: number,
  pointerY: number,
): number {
  let slot = 0;
  for (const rect of rects) {
    const belowIt = pointerY > rect.bottom;
    const pastItInRow = pointerY >= rect.top && pointerX > (rect.left + rect.right) / 2;
    if (belowIt || pastItInRow) slot += 1;
  }
  return slot;
}
