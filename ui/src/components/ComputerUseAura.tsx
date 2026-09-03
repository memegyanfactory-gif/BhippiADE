import { useEffect, useRef, useState } from "react";
import { IconMonitor, IconSparkle } from "./icons";

/** One thing the agent just did, as Rust announced it (`computer-overlay-action`). */
export type AuraAction = {
  title: string;
  /** CSS pixels in the overlay's own frame; absent for keys, scrolls and waits. */
  x: number | null;
  y: number | null;
  index: number;
  at: number;
};

type ComputerUseAuraProps = {
  active: boolean;
  label?: string | null;
  /** The pointer, CSS pixels, so the reticle and the packets know where the work is. */
  cursor?: { x: number; y: number } | null;
  actions?: AuraAction[];
  /** When the turn began, for the elapsed clock in the HUD. */
  startedAt?: number | null;
};

/* ── the picture (SPA-304) ────────────────────────────────────────────────────
   Four layers on one canvas, cheap enough for 60 fps on a 4K desktop:
   1. A perspective floor receding to a horizon, drifting sideways so it reads as
      a space rather than a texture; a scan front sweeps it near → far.
   2. A vertical beam crossing left → right on a slower clock, so the two fronts
      cross and the floor never looks like one looping GIF.
   3. Packets: bright points that run the floor's rays into the vanishing point —
      the "something deep is happening" the owner asked for — and, when the
      pointer jumps, a burst that runs from the old spot to the new one.
   4. A reticle around the pointer (rings, ticks, a radar arc) and a ripple at
      every action's point, captioned with what the action was.
   Everything is drawn from `time` and a few refs; React re-renders nothing per
   frame. Reduced motion freezes the sweeps and keeps the reticle. */

const HORIZON = 0.42;
const SCAN_SECONDS = 4.2;
const BEAM_SECONDS = 7.5;
const DRIFT_SECONDS = 26;
const PACKET_COUNT = 34;
const RIPPLE_SECONDS = 1.6;

type Packet = { lane: number; phase: number; speed: number; size: number };

function projectY(depth: number, h: number): number {
  return h * HORIZON + (h - h * HORIZON) * Math.pow(depth, 1.8);
}

/** Where a point at `depth` along the ray from `x0` (bottom) to the vanishing point sits. */
function alongRay(x0: number, w: number, h: number, depth: number): { x: number; y: number } {
  const vpX = w / 2;
  const horizonY = h * HORIZON;
  const t = 1 - depth; // depth 1 = near (bottom), 0 = horizon
  return { x: x0 + (vpX - x0) * t, y: h + (horizonY - h) * t };
}

export function ComputerUseAura({
  active,
  label,
  cursor = null,
  actions = [],
  startedAt = null,
}: ComputerUseAuraProps) {
  const [visible, setVisible] = useState(active);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const cursorRef = useRef<{ x: number; y: number } | null>(cursor);
  const previousCursor = useRef<{ x: number; y: number } | null>(null);
  const actionsRef = useRef<AuraAction[]>(actions);
  const burst = useRef<{
    from: { x: number; y: number };
    to: { x: number; y: number };
    at: number;
  } | null>(null);
  const [clock, setClock] = useState("");

  cursorRef.current = cursor;
  actionsRef.current = actions;

  // A pointer jump becomes a burst of packets between the two spots.
  useEffect(() => {
    if (!cursor) return;
    const previous = previousCursor.current;
    if (previous && Math.hypot(cursor.x - previous.x, cursor.y - previous.y) > 40) {
      burst.current = { from: previous, to: cursor, at: performance.now() };
    }
    previousCursor.current = cursor;
  }, [cursor]);

  useEffect(() => {
    if (active) {
      setVisible(true);
    } else {
      const timer = setTimeout(() => setVisible(false), 350);
      return () => clearTimeout(timer);
    }
  }, [active]);

  // The HUD's elapsed clock ticks once a second, off the canvas loop.
  useEffect(() => {
    if (!visible || !startedAt) {
      setClock("");
      return undefined;
    }
    const tick = () => {
      const seconds = Math.max(0, Math.floor((Date.now() - startedAt) / 1000));
      const m = Math.floor(seconds / 60);
      const s = seconds % 60;
      setClock(`${m}:${s.toString().padStart(2, "0")}`);
    };
    tick();
    const timer = window.setInterval(tick, 1000);
    return () => window.clearInterval(timer);
  }, [visible, startedAt]);

  useEffect(() => {
    if (!visible) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let animId = 0;
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      canvas.width = window.innerWidth * dpr;
      canvas.height = window.innerHeight * dpr;
      // setTransform, not scale: resize fires repeatedly and scale compounds.
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    resize();
    window.addEventListener("resize", resize);

    const startTime = performance.now();
    const lanes = [-0.06, 0.08, 0.2, 0.31, 0.43, 0.575, 0.69, 0.8, 0.92, 1.06];
    const packets: Packet[] = Array.from({ length: PACKET_COUNT }, (_, i) => ({
      lane: i % lanes.length,
      phase: (i * 0.618) % 1,
      speed: 0.11 + ((i * 7) % 5) * 0.035,
      size: 1.2 + ((i * 3) % 3) * 0.6,
    }));

    const laneX = (frac: number, w: number, drift: number) =>
      ((frac * w + drift + w * 1.5) % (w * 1.5)) - w * 0.25;

    const drawFloor = (w: number, h: number, t: number) => {
      const horizonY = h * HORIZON;
      const vpX = w / 2;
      const drift = reduceMotion ? 0 : ((t / DRIFT_SECONDS) % 1) * w * 0.12;

      // Depth bands, near = brighter, crowding toward the horizon.
      const BANDS = 26;
      ctx.lineWidth = 1;
      for (let i = 1; i <= BANDS; i++) {
        const d = i / BANDS;
        const y = projectY(d, h);
        const alpha = 0.3 * d * d;
        ctx.strokeStyle = `rgba(240, 160, 44, ${alpha.toFixed(3)})`;
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
        ctx.stroke();
      }
      // Rays: anchored at the bottom, drifting sideways as one, meeting at the
      // vanishing point — the drift is what makes it a room and not wallpaper.
      for (const frac of lanes) {
        const x0 = laneX(frac, w, drift);
        const gradient = ctx.createLinearGradient(x0, h, vpX, horizonY);
        gradient.addColorStop(0, "rgba(240, 160, 44, 0.32)");
        gradient.addColorStop(0.6, "rgba(240, 160, 44, 0.1)");
        gradient.addColorStop(1, "rgba(240, 160, 44, 0)");
        ctx.strokeStyle = gradient;
        ctx.beginPath();
        ctx.moveTo(x0, h);
        ctx.lineTo(vpX, horizonY);
        ctx.stroke();
      }
      // A faint lattice above the horizon: the ceiling of the room, breathing.
      const breathe = 0.5 + 0.5 * Math.sin(t * 1.5);
      ctx.strokeStyle = `rgba(240, 160, 44, ${(0.05 + 0.04 * breathe).toFixed(3)})`;
      const step = 64;
      ctx.beginPath();
      for (let x = (drift * 0.5) % step; x < w; x += step) {
        ctx.moveTo(x, 0);
        ctx.lineTo(x, horizonY);
      }
      for (let y = 0; y < horizonY; y += step) {
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
      }
      ctx.stroke();
    };

    const drawScanFront = (w: number, h: number, t: number) => {
      const horizonY = h * HORIZON;
      const sweep = reduceMotion ? 0.55 : (t / SCAN_SECONDS) % 1;
      const depth = 1 - sweep;
      const yS = projectY(depth, h);
      const trailHeight = 0.11 * (h - horizonY);
      const trail = ctx.createLinearGradient(0, yS - trailHeight, 0, yS);
      trail.addColorStop(0, "rgba(240, 160, 44, 0)");
      trail.addColorStop(1, "rgba(240, 160, 44, 0.22)");
      ctx.fillStyle = trail;
      ctx.fillRect(0, yS - trailHeight, w, trailHeight);
      const glareHeight = 0.05 * (h - horizonY);
      const glare = ctx.createLinearGradient(0, yS, 0, yS + glareHeight);
      glare.addColorStop(0, "rgba(255, 190, 80, 0.2)");
      glare.addColorStop(1, "rgba(240, 160, 44, 0)");
      ctx.fillStyle = glare;
      ctx.fillRect(0, yS, w, glareHeight);
      const frontEnergy = Math.min(1, depth / 0.4);
      ctx.strokeStyle = `rgba(255, 206, 120, ${(0.9 * frontEnergy).toFixed(3)})`;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(0, yS);
      ctx.lineTo(w, yS);
      ctx.stroke();
      ctx.lineWidth = 1;
      ctx.strokeStyle = `rgba(255, 90, 90, ${0.16 * frontEnergy})`;
      ctx.beginPath();
      ctx.moveTo(0, yS - 2);
      ctx.lineTo(w, yS - 2);
      ctx.stroke();
      ctx.strokeStyle = `rgba(90, 200, 255, ${0.1 * frontEnergy})`;
      ctx.beginPath();
      ctx.moveTo(0, yS + 2);
      ctx.lineTo(w, yS + 2);
      ctx.stroke();
    };

    const drawBeam = (w: number, h: number, t: number) => {
      if (reduceMotion) return;
      const x = ((t / BEAM_SECONDS) % 1) * (w + 240) - 120;
      const beam = ctx.createLinearGradient(x - 90, 0, x + 90, 0);
      beam.addColorStop(0, "rgba(90, 200, 255, 0)");
      beam.addColorStop(0.5, "rgba(120, 210, 255, 0.11)");
      beam.addColorStop(1, "rgba(90, 200, 255, 0)");
      ctx.fillStyle = beam;
      ctx.fillRect(x - 90, 0, 180, h);
      ctx.strokeStyle = "rgba(160, 225, 255, 0.35)";
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, h);
      ctx.stroke();
    };

    const drawPackets = (w: number, h: number, t: number) => {
      const drift = reduceMotion ? 0 : ((t / DRIFT_SECONDS) % 1) * w * 0.12;
      for (const packet of packets) {
        const frac = lanes[packet.lane] ?? 0.5;
        const x0 = laneX(frac, w, drift);
        const progress = reduceMotion ? packet.phase : (packet.phase + t * packet.speed) % 1;
        // Near → far: born at the bottom, gone at the horizon.
        const depth = 1 - progress;
        const p = alongRay(x0, w, h, depth);
        const alpha = 0.25 + 0.75 * depth;
        const radius = packet.size * (0.4 + depth);
        ctx.fillStyle = `rgba(255, 214, 140, ${alpha.toFixed(3)})`;
        ctx.beginPath();
        ctx.arc(p.x, p.y, radius, 0, Math.PI * 2);
        ctx.fill();
        // A short tail toward the bottom, so the direction reads.
        const tail = alongRay(x0, w, h, Math.min(1, depth + 0.05));
        ctx.strokeStyle = `rgba(255, 190, 80, ${(alpha * 0.5).toFixed(3)})`;
        ctx.beginPath();
        ctx.moveTo(p.x, p.y);
        ctx.lineTo(tail.x, tail.y);
        ctx.stroke();
      }
      // The pointer burst: packets that run from where the pointer was to where it is.
      const live = burst.current;
      if (live) {
        const age = (performance.now() - live.at) / 1000;
        if (age > 1) {
          burst.current = null;
        } else {
          for (let i = 0; i < 9; i++) {
            const k = Math.min(1, age * 1.6 + i * 0.07);
            const x = live.from.x + (live.to.x - live.from.x) * k;
            const y = live.from.y + (live.to.y - live.from.y) * k;
            const fade = 1 - age;
            ctx.fillStyle = `rgba(255, 224, 164, ${(0.9 * fade * (1 - i * 0.08)).toFixed(3)})`;
            ctx.beginPath();
            ctx.arc(x, y, Math.max(0.4, 2.2 - i * 0.12), 0, Math.PI * 2);
            ctx.fill();
          }
        }
      }
    };

    const drawReticle = (t: number) => {
      const point = cursorRef.current;
      if (!point || point.x < -100) return;
      const { x, y } = point;
      const spin = reduceMotion ? 0 : t * 0.9;
      const breathe = reduceMotion ? 0 : Math.sin(t * 2.4) * 3;
      // Outer ring with four gaps, turning.
      ctx.lineWidth = 1.2;
      ctx.strokeStyle = "rgba(255, 196, 92, 0.75)";
      for (let i = 0; i < 4; i++) {
        const start = spin + (i * Math.PI) / 2 + 0.22;
        ctx.beginPath();
        ctx.arc(x, y, 34 + breathe, start, start + Math.PI / 2 - 0.44);
        ctx.stroke();
      }
      // Inner ring, counter-turning, dashed.
      ctx.setLineDash([3, 5]);
      ctx.strokeStyle = "rgba(255, 232, 180, 0.55)";
      ctx.beginPath();
      ctx.arc(x, y, 20, -spin * 1.6, -spin * 1.6 + Math.PI * 2);
      ctx.stroke();
      ctx.setLineDash([]);
      // Crosshair ticks, kept clear of the glyph itself.
      ctx.strokeStyle = "rgba(255, 210, 120, 0.7)";
      ctx.beginPath();
      for (const [dx, dy] of [
        [1, 0],
        [-1, 0],
        [0, 1],
        [0, -1],
      ]) {
        ctx.moveTo(x + dx * 42, y + dy * 42);
        ctx.lineTo(x + dx * 54, y + dy * 54);
      }
      ctx.stroke();
      // A radar arc sweeping around the pointer, fading behind itself.
      if (!reduceMotion && typeof ctx.createConicGradient === "function") {
        const angle = (t * 1.3) % (Math.PI * 2);
        const radar = ctx.createConicGradient(angle, x, y);
        radar.addColorStop(0, "rgba(255, 196, 92, 0.32)");
        radar.addColorStop(0.18, "rgba(255, 196, 92, 0)");
        radar.addColorStop(1, "rgba(255, 196, 92, 0)");
        ctx.fillStyle = radar;
        ctx.beginPath();
        ctx.moveTo(x, y);
        ctx.arc(x, y, 72, 0, Math.PI * 2);
        ctx.fill();
      }
    };

    const drawRipples = () => {
      const now = performance.now();
      ctx.font = "600 11px ui-monospace, SFMono-Regular, Menlo, Consolas, monospace";
      ctx.textBaseline = "middle";
      for (const action of actionsRef.current) {
        const age = (now - action.at) / 1000;
        if (age > RIPPLE_SECONDS) continue;
        const k = age / RIPPLE_SECONDS;
        const point =
          action.x !== null && action.y !== null ? { x: action.x, y: action.y } : cursorRef.current;
        if (!point) continue;
        // Two expanding rings, the second a beat behind.
        for (const delay of [0, 0.18]) {
          const kk = Math.max(0, k - delay);
          if (kk <= 0) continue;
          ctx.lineWidth = 2 - kk;
          ctx.strokeStyle = `rgba(255, 206, 120, ${(0.8 * (1 - kk)).toFixed(3)})`;
          ctx.beginPath();
          ctx.arc(point.x, point.y, 8 + kk * 70, 0, Math.PI * 2);
          ctx.stroke();
        }
        // The caption: what happened, beside the point, fading with the rings.
        const alpha = k < 0.7 ? 1 : 1 - (k - 0.7) / 0.3;
        const text = `${action.index.toString().padStart(2, "0")} · ${action.title}`;
        const width = ctx.measureText(text).width + 14;
        const cx = Math.min(window.innerWidth - width - 12, point.x + 26);
        const cy = Math.max(14, point.y - 26);
        ctx.fillStyle = `rgba(11, 15, 25, ${(0.88 * alpha).toFixed(3)})`;
        ctx.strokeStyle = `rgba(240, 160, 44, ${(0.55 * alpha).toFixed(3)})`;
        ctx.beginPath();
        ctx.roundRect(cx, cy - 10, width, 20, 5);
        ctx.fill();
        ctx.stroke();
        ctx.fillStyle = `rgba(255, 224, 164, ${alpha.toFixed(3)})`;
        ctx.fillText(text, cx + 7, cy);
      }
    };

    const render = (time: number) => {
      const w = window.innerWidth;
      const h = window.innerHeight;
      const t = (time - startTime) / 1000;
      ctx.clearRect(0, 0, w, h);

      // Vignette: the floor sits in a lit corner of the room, the app stays legible.
      const cx = w / 2;
      const cy = h / 2;
      const maxR = Math.hypot(w, h) / 2;
      const vignette = ctx.createRadialGradient(cx, cy, maxR * 0.45, cx, cy, maxR);
      vignette.addColorStop(0, "rgba(0, 0, 0, 0)");
      vignette.addColorStop(0.75, "rgba(0, 0, 0, 0)");
      vignette.addColorStop(1, "rgba(0, 0, 0, 0.5)");
      ctx.fillStyle = vignette;
      ctx.fillRect(0, 0, w, h);

      drawFloor(w, h, t);
      drawBeam(w, h, t);
      drawScanFront(w, h, t);
      drawPackets(w, h, t);
      drawRipples();
      drawReticle(t);

      animId = requestAnimationFrame(render);
    };

    animId = requestAnimationFrame(render);
    return () => {
      cancelAnimationFrame(animId);
      window.removeEventListener("resize", resize);
    };
  }, [visible]);

  if (!visible) return null;

  const latest = actions[actions.length - 1];
  const count = latest ? latest.index : 0;

  return (
    <div
      className={`computer-use-aura grid-scan-aura${active ? " active" : " exit"}`}
      aria-hidden="true"
    >
      <canvas ref={canvasRef} className="grid-scan-canvas" />

      <div className="grid-edge-bloom" />
      <div className="grid-perimeter-loop" aria-hidden="true">
        <span className="perimeter-streak top" />
        <span className="perimeter-streak right" />
        <span className="perimeter-streak bottom" />
        <span className="perimeter-streak left" />
      </div>

      <div className="grid-corner-bracket top-left">
        <span className="bracket-angle" />
        <span className="bracket-tag">BHIPPI // COMPUTER.01</span>
      </div>
      <div className="grid-corner-bracket top-right">
        <span className="bracket-angle" />
        <span className="bracket-tag">
          {cursor ? `PTR ${Math.round(cursor.x)},${Math.round(cursor.y)}` : "FRAME // LIVE"}
        </span>
      </div>
      <div className="grid-corner-bracket bottom-left">
        <span className="bracket-angle" />
        <span className="bracket-tag">
          {latest ? `LAST // ${latest.title.toUpperCase().slice(0, 40)}` : "CURSOR // READY"}
        </span>
      </div>
      <div className="grid-corner-bracket bottom-right">
        <span className="bracket-angle" />
        <span className="bracket-tag">{`ACTIONS // ${count.toString().padStart(2, "0")}`}</span>
      </div>

      <div className="aura-hud-container">
        <div className="aura-hud-pill grid-hud-pill">
          <div className="aura-hud-orb">
            <div className="aura-orb-core" />
            <div className="aura-orb-ring" />
          </div>
          <div className="aura-hud-content">
            <div className="aura-hud-title">
              <IconMonitor size={13} />
              <span>BHIPPI COMPUTER // LIVE CONTROL</span>
              <IconSparkle size={11} className="sparkle-spin" />
            </div>
            <div className="aura-hud-sub">
              {label ? `${label} · ` : ""}
              {clock ? `${clock} · ` : ""}
              {count > 0 ? `${count} ${count === 1 ? "action" : "actions"} · ` : ""}
              Press Esc twice to stop
            </div>
          </div>
          {latest ? (
            <div className="aura-hud-ticker" key={latest.index}>
              <span className="aura-hud-ticker-index">{latest.index}</span>
              <span className="aura-hud-ticker-text">{latest.title}</span>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
