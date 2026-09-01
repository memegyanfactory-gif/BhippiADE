import { useEffect, useRef } from "react";

type OverlayCursorProps = {
  x: number;
  y: number;
};

// Replacement pointer for a Computer Use turn. The OS arrow is blanked on the
// Rust side (ADR-0019) and the watcher streams its position here at up to
// ~12 ms. Lerps the glyph toward the target on rAF so the pointer reads as one
// continuous object instead of a chain of teleports.
export function OverlayCursor({ x, y }: OverlayCursorProps) {
  const ref = useRef<HTMLDivElement | null>(null);
  const pos = useRef({ x, y, ready: false });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (!pos.current.ready) {
      pos.current = { x, y, ready: true };
    }

    let frame = 0;
    const step = () => {
      pos.current.x += (x - pos.current.x) * 0.35;
      pos.current.y += (y - pos.current.y) * 0.35;
      el.style.transform = `translate3d(${pos.current.x.toFixed(2)}px, ${pos.current.y.toFixed(2)}px, 0)`;
      const dx = Math.abs(x - pos.current.x);
      const dy = Math.abs(y - pos.current.y);
      if (dx > 0.05 || dy > 0.05) {
        frame = requestAnimationFrame(step);
      }
    };

    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [x, y]);

  return (
    <div ref={ref} className="overlay-cursor" aria-hidden="true">
      <div className="cursor-aura" />
      <svg className="cursor-glyph" viewBox="0 0 26 26" aria-hidden="true">
        <path
          d="M3 3v21l6.2-5.8 3.4 5 2.6-1.2-3.4-4.6H19Z"
          fill="#000000"
          stroke="#ffb020"
          strokeWidth="1.6"
          strokeLinejoin="round"
        />
        <path
          d="M3 3v21l6.2-5.8 3.4 5 2.6-1.2-3.4-4.6H19Z"
          fill="none"
          stroke="#fff7e0"
          strokeWidth="0.5"
          opacity="0.6"
          strokeLinejoin="round"
        />
      </svg>
    </div>
  );
}