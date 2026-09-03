import ReactDOM from "react-dom/client";
import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ComputerUseAura, type AuraAction } from "./components/ComputerUseAura";
import { OverlayCursor } from "./components/OverlayCursor";
import "./styles/tokens.css";
import "./styles/aura.css";
import "./styles/overlay.css";

type ShowPayload = {
  label: string;
  originX: number;
  originY: number;
  width: number;
  height: number;
};

type CursorPayload = {
  x?: number;
  y?: number;
  originX?: number;
  originY?: number;
};

function isOverlayHost(): boolean {
  if (typeof window === "undefined") return false;
  const host = window as Window & { isTauri?: boolean };
  return "__TAURI_INTERNALS__" in host || "__TAURI__" in host || Boolean(host.isTauri);
}

function numberField(payload: Record<string, unknown>, names: string[]): number | null {
  for (const name of names) {
    const value = payload[name];
    if (typeof value === "number" && Number.isFinite(value)) return value;
  }
  return null;
}

// The overlay is a separate Vite entry served only to the desktop overlay window
// (ADR-0019). In the browser dev server it still runs as a self-painting demo so
// the grid-scan and pointer can be tuned without launching a turn.
export default function Overlay() {
  const [active, setActive] = useState(false);
  const [label, setLabel] = useState<string | null>(null);
  const [cursor, setCursor] = useState({ x: -200, y: -200 });
  // What the agent did, newest last, for the ripples and the HUD ticker (SPA-304).
  const [actions, setActions] = useState<AuraAction[]>([]);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const originRef = useRef({ x: 0, y: 0 });

  useEffect(() => {
    if (!isOverlayHost()) {
      setActive(true);
      setLabel("Standalone overlay preview");
      setStartedAt(Date.now());
      const started = performance.now();
      let frame = 0;
      let demoIndex = 0;
      // A pretend action every few seconds, so the ripple and the caption can be tuned.
      const demoActions = window.setInterval(() => {
        demoIndex += 1;
        const w = window.innerWidth;
        const h = window.innerHeight;
        setActions((current) => [
          ...current.slice(-11),
          {
            title: demoIndex % 2 ? `left click ×1 at (${Math.round(w * 0.6)}, ${Math.round(h * 0.4)})` : "Type \"hello world\"",
            x: demoIndex % 2 ? w * 0.6 : null,
            y: demoIndex % 2 ? h * 0.4 : null,
            index: demoIndex,
            at: performance.now(),
          },
        ]);
      }, 2600);
      const walk = (now: number) => {
        const t = (now - started) / 1000;
        const w = window.innerWidth;
        const h = window.innerHeight;
        setCursor({
          x: w * (0.5 + 0.42 * Math.sin(t * 1.1)),
          y: h * (0.5 + 0.42 * Math.sin(t * 0.9 + 1.7)),
        });
        frame = requestAnimationFrame(walk);
      };
      frame = requestAnimationFrame(walk);
      return () => {
        cancelAnimationFrame(frame);
        window.clearInterval(demoActions);
      };
    }

    let unlisteners: UnlistenFn[] = [];
    const wire = async () => {
      const offShow = await listen<ShowPayload>("computer-overlay-show", (event) => {
        const payload = event.payload as ShowPayload & Record<string, unknown>;
        const originX = numberField(payload, ["originX", "origin_x"]) ?? 0;
        const originY = numberField(payload, ["originY", "origin_y"]) ?? 0;
        originRef.current = { x: originX, y: originY };
        setLabel(payload.label || "Scanning the desktop");
        setActions([]);
        setStartedAt(Date.now());
        setActive(true);
      });
      const offHide = await listen("computer-overlay-hide", () => setActive(false));
      const offStopping = await listen<string>("computer-overlay-stopping", (event) => {
        setLabel(event.payload || "Stopping Computer Use");
      });
      const offCursor = await listen<CursorPayload>("computer-overlay-cursor", (event) => {
        const dpr = window.devicePixelRatio || 1;
        const payload = (event.payload ?? {}) as Record<string, unknown>;
        const x = numberField(payload, ["x"]);
        const y = numberField(payload, ["y"]);
        if (x === null || y === null) return;
        // Reports are physical pixels from the virtual desktop origin; the webview
        // canvas is CSS pixels whose zero sits at the overlay window origin.
        setCursor({
          x: (x - originRef.current.x) / dpr,
          y: (y - originRef.current.y) / dpr,
        });
      });
      // Every executed action, with its caption (ADR-0044 §2). Coordinates arrive as
      // physical virtual-desktop pixels, like the cursor; keys and waits carry none.
      const offAction = await listen<Record<string, unknown>>("computer-overlay-action", (event) => {
        const dpr = window.devicePixelRatio || 1;
        const payload = (event.payload ?? {}) as Record<string, unknown>;
        const x = numberField(payload, ["x"]);
        const y = numberField(payload, ["y"]);
        const title = typeof payload.title === "string" ? payload.title : "Action";
        const index = numberField(payload, ["index"]) ?? 0;
        setActions((current) => [
          ...current.slice(-11),
          {
            title,
            x: x === null ? null : (x - originRef.current.x) / dpr,
            y: y === null ? null : (y - originRef.current.y) / dpr,
            index,
            at: performance.now(),
          },
        ]);
      });
      unlisteners = [offShow, offHide, offStopping, offCursor, offAction];
    };
    void wire();
    return () => {
      for (const unsubscribe of unlisteners) unsubscribe();
    };
  }, []);

  return (
    <div className="overlay-root">
      <ComputerUseAura
        active={active}
        label={label}
        cursor={active ? cursor : null}
        actions={actions}
        startedAt={startedAt}
      />
      {active ? <OverlayCursor x={cursor.x} y={cursor.y} /> : null}
    </div>
  );
}

const root = document.getElementById("overlay-root");
if (root) {
  ReactDOM.createRoot(root).render(<Overlay />);
}
