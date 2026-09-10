// The budget gauge. The arc grows clockwise as the window's tokens are spent, and its
// colour walks green → amber → orange → red on the way. Empty ring = nothing spent;
// full red ring = the cap is gone. Colour is never the only signal — every caller
// prints the percentage beside it (INV-034).

const RAMP = [
  { under: 0.5, token: "var(--gauge-0)" },
  { under: 0.75, token: "var(--gauge-1)" },
  { under: 0.9, token: "var(--gauge-2)" },
  { under: Infinity, token: "var(--gauge-3)" },
] as const;

/** The ramp step a fraction lands on. Exported so bars elsewhere read the same. */
export function gaugeColor(fraction: number): string {
  const step = RAMP.find((entry) => fraction < entry.under);
  return step ? step.token : "var(--gauge-3)";
}

type UsageRingProps = {
  /** 0..1 of the budget already spent. Values outside the range are clamped. */
  fraction: number;
  /** False when the provider has no cap — the ring stays an empty track. */
  capped: boolean;
  size?: number;
  thickness?: number;
};

export function UsageRing({ fraction, capped, size = 14, thickness = 2 }: UsageRingProps) {
  const spent = capped ? Math.min(Math.max(fraction, 0), 1) : 0;
  const radius = (size - thickness) / 2;
  const circumference = 2 * Math.PI * radius;
  const center = size / 2;

  return (
    <svg
      className="usage-ring"
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      aria-hidden="true"
      focusable="false"
    >
      <circle
        cx={center}
        cy={center}
        r={radius}
        fill="none"
        stroke="var(--gauge-track, rgba(255, 255, 255, 0.24))"
        strokeWidth={thickness}
      />
      {spent > 0 ? (
        <circle
          className="usage-ring-arc"
          cx={center}
          cy={center}
          r={radius}
          fill="none"
          stroke={gaugeColor(spent)}
          strokeWidth={thickness}
          strokeLinecap="butt"
          strokeDasharray={`${circumference * spent} ${circumference}`}
          transform={`rotate(-90 ${center} ${center})`}
        />
      ) : null}
    </svg>
  );
}
