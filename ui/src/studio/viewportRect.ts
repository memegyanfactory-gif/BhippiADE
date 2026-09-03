/**
 * The viewport's box, as the page reports it to Rust (ADR-0045).
 *
 * The studio viewport is a hole in the page with the real Godot window sitting in it. The
 * page's only job is to say where the hole is, in CSS pixels, and whether anything covers
 * it; Rust converts to physical pixels with the window's scale factor and moves the child.
 * These helpers decide *when* that message is worth sending: layout reads sub-pixel values
 * that jitter, and a jitter must not become an IPC call every frame.
 */

export interface ViewportBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** `getBoundingClientRect` rounded to whole CSS pixels. */
export function roundBox(rect: {
  left: number;
  top: number;
  width: number;
  height: number;
}): ViewportBox {
  const x = Math.round(rect.left);
  const y = Math.round(rect.top);
  return {
    x,
    y,
    width: Math.max(0, Math.round(rect.left + rect.width) - x),
    height: Math.max(0, Math.round(rect.top + rect.height) - y),
  };
}

export function sameBox(a: ViewportBox | null | undefined, b: ViewportBox): boolean {
  return (
    a !== null &&
    a !== undefined &&
    a.x === b.x &&
    a.y === b.y &&
    a.width === b.width &&
    a.height === b.height
  );
}

/**
 * Whether the native window may be shown: the host has a real size and nothing in the page
 * (a modal, another screen) is standing where it would be. A native child cannot be painted
 * over, so the page hides it instead.
 */
export function hostVisible(box: ViewportBox, obstructed: boolean): boolean {
  return !obstructed && box.width > 0 && box.height > 0;
}
