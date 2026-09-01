import { useEffect, useRef, useState } from "react";
import { IconMonitor, IconSparkle } from "./icons";

type ComputerUseAuraProps = {
  active: boolean;
  label?: string | null;
};

export function ComputerUseAura({ active, label }: ComputerUseAuraProps) {
  const [visible, setVisible] = useState(active);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    if (active) {
      setVisible(true);
    } else {
      const timer = setTimeout(() => setVisible(false), 350);
      return () => clearTimeout(timer);
    }
  }, [active]);

  // Grid scan aura (ADR-0019, Phase 4 of doc 12).
  //
  // A perspective floor receding to a horizon line reads as "scanning the whole
  // desktop" rather than "animating near one element". Depth is faked cheaply:
  // horizontal bands are spaced with d^2 so they crowd toward the horizon, and a
  // vertical ray walks from the vanishing point to each bottom anchor. A scan
  // band sweeps from the near floor toward the horizon and resets invisibly at
  // the top where its alpha has already faded.
  useEffect(() => {
    if (!visible) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let animId: number;
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
    const SCAN_SECONDS = 3.6;
    const HORIZON = 0.42; // fraction of height -- far enough to imply the room
    const projectY = (depth: number, h: number) =>
      h * HORIZON + (h - h * HORIZON) * Math.pow(depth, 1.8);

    const drawGrid = (w: number, h: number) => {
      const horizonY = h * HORIZON;
      const vpX = w / 2;

      // Depth bands, near = brighter. Each band drawn as a single horizontal line.
      const BANDS = 24;
      ctx.lineWidth = 1;
      for (let i = 1; i <= BANDS; i++) {
        const d = i / BANDS;
        const y = projectY(d, h);
        const alpha = 0.36 * d * d; // near lines pop, far lines fog out
        ctx.strokeStyle = `rgba(240, 160, 44, ${alpha.toFixed(3)})`;
        ctx.beginPath();
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
        ctx.stroke();
      }

      // Vertical rays: a straight segment from each bottom anchor back into the
      // vanishing point. Spread anchors past the viewport so the outside tapers.
      const anchors = [-0.06, 0.08, 0.20, 0.31, 0.43, 0.575, 0.69, 0.80, 0.92, 1.06].map(
        (frac) => frac * w,
      );
      for (const x0 of anchors) {
        const gradient = ctx.createLinearGradient(x0, h, vpX, horizonY);
        gradient.addColorStop(0, "rgba(240, 160, 44, 0.34)");
        gradient.addColorStop(0.6, "rgba(240, 160, 44, 0.10)");
        gradient.addColorStop(1, "rgba(240, 160, 44, 0)");
        ctx.strokeStyle = gradient;
        ctx.beginPath();
        ctx.moveTo(x0, h);
        ctx.lineTo(vpX, horizonY);
        ctx.stroke();
      }
    };

    const render = (time: number) => {
      const w = window.innerWidth;
      const h = window.innerHeight;
      ctx.clearRect(0, 0, w, h);

      // Soft vignette so the floor sits in a lit corner of the room and the
      // underlying app stays legible.
      const cx = w / 2;
      const cy = h / 2;
      const maxR = Math.hypot(w, h) / 2;
      const vignette = ctx.createRadialGradient(cx, cy, maxR * 0.45, cx, cy, maxR);
      vignette.addColorStop(0, "rgba(0, 0, 0, 0)");
      vignette.addColorStop(0.75, "rgba(0, 0, 0, 0)");
      vignette.addColorStop(1, "rgba(0, 0, 0, 0.5)");
      ctx.fillStyle = vignette;
      ctx.fillRect(0, 0, w, h);

      drawGrid(w, h);

      // Chaser along the horizon: the grid's far edge comes alive without once
      // sweeping over the whole screen.
      const t = (time - startTime) / 1000;
      if (reduceMotion) {
        drawScanBand(projectY(0.45, h), h);
      } else {
        // Sweep near -> far: the front leaves the bottom of the screen and winds
        // down to nothing at the horizon, so its reset is already invisible.
        const sweep = (t / SCAN_SECONDS) % 1;
        const depth = 1 - sweep;
        const yS = projectY(depth, h);
        drawScanBand(yS, h);
        // The leading edge at full strength, frontier-colored.
        ctx.strokeStyle = "rgba(255, 206, 120, 0.9)";
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        ctx.moveTo(0, yS);
        ctx.lineTo(w, yS);
        ctx.stroke();
        ctx.lineWidth = 1;
        // Chromatic split on the front edge sells the "scan" and hides the plain
        // line. Fades as the front recedes so the reset at the horizon is mute.
        const frontEnergy = Math.min(1, depth / 0.4);
        ctx.strokeStyle = `rgba(255, 90, 90, ${0.16 * frontEnergy})`;
        ctx.beginPath();
        ctx.moveTo(0, yS - 2);
        ctx.lineTo(w, yS - 2);
        ctx.stroke();
        ctx.strokeStyle = `rgba(90, 200, 255, ${0.10 * frontEnergy})`;
        ctx.beginPath();
        ctx.moveTo(0, yS + 2);
        ctx.lineTo(w, yS + 2);
        ctx.stroke();
      }

      animId = requestAnimationFrame(render);
    };

    const drawScanBand = (yS: number, h: number) => {
      const horizonY = h * HORIZON;
      // A trail above the leading edge (already-scanned banner fading out) plus a
      // short soft glare below (the front heating the floor).
      const trailHeight = 0.11 * (h - horizonY);
      const trail = ctx.createLinearGradient(0, yS - trailHeight, 0, yS);
      trail.addColorStop(0, "rgba(240, 160, 44, 0)");
      trail.addColorStop(1, "rgba(240, 160, 44, 0.24)");
      ctx.fillStyle = trail;
      ctx.fillRect(0, yS - trailHeight, window.innerWidth, trailHeight);

      const glareHeight = 0.05 * (h - horizonY);
      const glare = ctx.createLinearGradient(0, yS, 0, yS + glareHeight);
      glare.addColorStop(0, "rgba(255, 190, 80, 0.20)");
      glare.addColorStop(1, "rgba(240, 160, 44, 0)");
      ctx.fillStyle = glare;
      ctx.fillRect(0, yS, window.innerWidth, glareHeight);
    };

    animId = requestAnimationFrame(render);

    return () => {
      cancelAnimationFrame(animId);
      window.removeEventListener("resize", resize);
    };
  }, [visible]);

  if (!visible) return null;

  return (
    <div
      className={`computer-use-aura grid-scan-aura${active ? " active" : " exit"}`}
      aria-hidden="true"
    >
      {/* High-Performance Canvas Grid Scan Background */}
      <canvas ref={canvasRef} className="grid-scan-canvas" />

      {/* An even edge bloom breathes with the mesh while four staggered streaks
          form one continuous clockwise loop around the full viewport. */}
      <div className="grid-edge-bloom" />
      <div className="grid-perimeter-loop" aria-hidden="true">
        <span className="perimeter-streak top" />
        <span className="perimeter-streak right" />
        <span className="perimeter-streak bottom" />
        <span className="perimeter-streak left" />
      </div>

      {/* Cyber Corner HUD Brackets with Coordinate Telemetry */}
      <div className="grid-corner-bracket top-left">
        <span className="bracket-angle" />
        <span className="bracket-tag">BHIPPI // COMPUTER.01</span>
      </div>
      <div className="grid-corner-bracket top-right">
        <span className="bracket-angle" />
        <span className="bracket-tag">FRAME // LIVE.ACTIVE</span>
      </div>
      <div className="grid-corner-bracket bottom-left">
        <span className="bracket-angle" />
        <span className="bracket-tag">CURSOR // BHIPPI.READY</span>
      </div>
      <div className="grid-corner-bracket bottom-right">
        <span className="bracket-angle" />
        <span className="bracket-tag">VIEWPORT // ADAPTIVE</span>
      </div>

      {/* Top Floating Cyber HUD Automation Indicator */}
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
              {label ? `${label} · ` : ""}Press Esc twice to stop
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
