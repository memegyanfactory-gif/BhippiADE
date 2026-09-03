import { useCallback, useEffect, useRef, useState } from "react";
import logo from "../assets/logo.png";
import { api, events } from "../lib/api";
import type { GodotEmbedState } from "../lib/ipc";
import { hostVisible, roundBox, sameBox, type ViewportBox } from "./viewportRect";

/**
 * The studio viewport (ADR-0045).
 *
 * There is nothing to draw here. The real Godot window — the editor as the workspace, the
 * running game on top of it — is a native child of Bhippi's window, placed over this
 * element by Rust. This component's whole job is to keep Rust told where the element is and
 * whether it is covered, and to show the empty state when nothing is embedded.
 *
 * Nothing may be rendered on top of this element while a surface is embedded: the page
 * cannot paint over a native child. Anything that must cover it (a modal) sets `obstructed`
 * and the child is hidden for the duration.
 */

interface Props {
  projectPath: string;
  /** A modal or another screen is standing where the viewport is. */
  obstructed: boolean;
  onState?: (state: GodotEmbedState) => void;
  resizing?: boolean;
}

interface Sent {
  box: ViewportBox;
  visible: boolean;
}

/** While the splitter drags, how often the native window is given a new size (SPA-402).
 *  The editor re-lays itself out on every size it receives, and sixty of those a second
 *  stutter; twenty-five still track the cursor, and the release sends the exact box. */
const RESIZE_PUSH_INTERVAL_MS = 40;

export function GodotViewport({ projectPath, obstructed, onState, resizing = false }: Props) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const lastSent = useRef<Sent | null>(null);
  const lastPushAt = useRef(0);
  const obstructedRef = useRef(obstructed);
  obstructedRef.current = obstructed;
  const resizingRef = useRef(resizing);
  resizingRef.current = resizing;
  const [state, setState] = useState<GodotEmbedState | null>(null);

  // SPA-402: the native window follows the splitter *while* it moves. A push per frame
  // is one SetWindowPos on the Rust side, which is cheaper than the visible lag of a
  // window that only catches up on release — so a drag no longer suppresses pushes;
  // it only skips the ones whose box did not change.
  const push = useCallback((force = false) => {
    const host = hostRef.current;
    if (!host) return;
    const box = roundBox(host.getBoundingClientRect());
    const visible = hostVisible(box, obstructedRef.current);
    const last = lastSent.current;
    if (!force && last && sameBox(last.box, box) && last.visible === visible) return;
    if (
      resizingRef.current &&
      !force &&
      performance.now() - lastPushAt.current < RESIZE_PUSH_INTERVAL_MS
    ) {
      return;
    }
    lastPushAt.current = performance.now();
    lastSent.current = { box, visible };
    void api.godotEmbedLayout(box, visible).catch(() => {
      /* Rust reports the failure through the pane's state; the hole is still a hole. */
    });
  }, []);

  // The end of a drag still forces one push, so the final box is exact even when the
  // last pointer move and the last frame disagreed by a pixel.
  useEffect(() => {
    if (!resizing) {
      push(true);
    }
  }, [resizing, push]);

  // Follow the host. Size changes arrive from the ResizeObserver; position-only changes
  // (a pane beside us resizing) are caught by a per-frame comparison that sends nothing
  // unless the rounded box actually moved.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    push(true);
    const observer = new ResizeObserver(() => push());
    observer.observe(host);
    const onResize = () => push();
    window.addEventListener("resize", onResize);
    let frame = 0;
    const tick = () => {
      push();
      frame = window.requestAnimationFrame(tick);
    };
    frame = window.requestAnimationFrame(tick);
    return () => {
      observer.disconnect();
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", onResize);
      // Leaving the studio: the native window must not float over whatever screen is next.
      const last = lastSent.current;
      if (last) {
        lastSent.current = null;
        void api.godotEmbedLayout(last.box, false).catch(() => {});
      }
    };
  }, [push]);

  useEffect(() => {
    push(true);
  }, [obstructed, push]);

  useEffect(() => {
    let cancelled = false;
    const apply = (next: GodotEmbedState) => {
      if (cancelled) return;
      setState(next);
      onState?.(next);
    };
    void api.godotEmbedState().then(apply).catch(() => {});
    const unlisten = events.godotEmbedState.listen((event) => apply(event.payload));
    return () => {
      cancelled = true;
      void unlisten.then((stop) => stop());
    };
  }, [onState]);

  const front = state?.front ?? null;
  const starting =
    (state?.game !== null && state?.game !== undefined && !state.game.attached) ||
    (state?.workspace !== null && state?.workspace !== undefined && !state.workspace.attached);

  return (
    <div
      ref={hostRef}
      className="godot-viewport"
      data-front={front ?? "none"}
      role="region"
      aria-label="Godot viewport"
    >
      {front === null ? (
        <div className="godot-viewport-empty">
          {/* SPA-501: nothing native is embedded while the engine is idle, so the page may
              paint here — the mark in the middle, the hint under it. */}
          <img src={logo} className="godot-viewport-logo" alt="" draggable={false} />
          {starting ? (
            <span className="godot-viewport-hint" aria-live="polite">
              Starting Godot…
            </span>
          ) : projectPath ? (
            <span className="godot-viewport-hint">
              Nothing running. Open the workspace or press Play.
            </span>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
