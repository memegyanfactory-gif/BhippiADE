import { useEffect, useRef, useSyncExternalStore } from "react";
import {
  isViewportObstructed,
  newObstructionToken,
  obstructViewport,
  releaseViewport,
  subscribeObstruction,
} from "./viewportObstruction";

/**
 * Joins the viewport-obstruction registry while `open` is true (SPA-001).
 *
 * Any floating surface that can land over the Studio viewport — a title-bar dropdown, a
 * portal popover, a menu — calls this with its open state. Unmounting releases the token,
 * so a surface that disappears without closing cleanly cannot keep the engine hidden.
 */
export function useObstructsViewport(open: boolean): void {
  const token = useRef<string | null>(null);
  if (token.current === null) token.current = newObstructionToken();
  useEffect(() => {
    const id = token.current;
    if (!id) return undefined;
    if (open) obstructViewport(id);
    else releaseViewport(id);
    return () => releaseViewport(id);
  }, [open]);
}

function subscribe(onChange: () => void): () => void {
  return subscribeObstruction(() => onChange());
}

/** True while any floating surface is over the viewport. */
export function useViewportObstructed(): boolean {
  return useSyncExternalStore(subscribe, isViewportObstructed, () => false);
}
