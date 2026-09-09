/**
 * The B — Bhippi's mark, drawn small enough to stand in a row of text.
 *
 * Everywhere the app used to draw a generic spinner while it worked, it now draws itself:
 * the logo's split ring with the B inside it. Two rules keep that from becoming decoration.
 *
 * **It stays the size of the thing it replaced.** The default is 11px, which is exactly the
 * footprint of the ring that was there before, so no row moves and no line reflows. A logo
 * that grew to announce itself would be a worse indicator than the spinner it replaced.
 *
 * **The letter never spins.** The ring turns and a sheen travels around it; the B sits
 * upright at the centre and only breathes. A rotating letterform reads as a broken asset,
 * and a mark you cannot read is not a mark.
 *
 * Colour comes from `currentColor`, so a caller tints it by setting a colour on the mark —
 * which is how the stream gives a different hue to exploring, changing and checking without
 * this file knowing anything about activities.
 */

import "../styles/bhippi-mark.css";

/** What the mark is doing, which is the only thing that decides how it moves. */
export type MarkMotion =
  /** Working: the ring turns, the B breathes. */
  | "working"
  /** Looking for something: the ring holds still and a sheen sweeps around it. */
  | "seeking"
  /** Present but idle — a header, a finished row. Nothing moves. */
  | "still";

export function BhippiMark({
  size = 11,
  motion = "working",
  tone,
  title,
}: {
  size?: number;
  motion?: MarkMotion;
  /** A tone class the stylesheet colours; omit to inherit the surrounding text colour. */
  tone?: string | null;
  /** Set only when the mark is the sole thing saying what is happening. */
  title?: string;
}) {
  return (
    <span
      className={`bhippi-mark is-${motion}${tone ? ` tone-${tone}` : ""}`}
      style={{ ["--mark-size" as string]: `${size}px` }}
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
    >
      <svg viewBox="0 0 24 24" width={size} height={size} focusable="false">
        {/* The split ring: two arcs with the logo's gaps at twelve and six o'clock. Each
            spans 156°, so the gaps stay narrow enough to read as one broken circle — widen
            them and the mark turns into a pair of parentheses. */}
        <g className="bm-ring" fill="none" stroke="currentColor" strokeLinecap="round">
          <path d="M13.98 2.71A9.5 9.5 0 0 1 13.98 21.29" />
          <path d="M10.02 21.29A9.5 9.5 0 0 1 10.02 2.71" />
        </g>
        {/* The sheen: one short bright arc travelling the whole circle, gaps included. */}
        <circle
          className="bm-sheen"
          cx="12"
          cy="12"
          r="9.5"
          pathLength={100}
          fill="none"
          stroke="currentColor"
          strokeLinecap="round"
        />
        {/* The B: a stem and two bowls. Upright, always. */}
        <g
          className="bm-letter"
          fill="none"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M10.2 7.3V16.7" />
          <path d="M10.2 7.3h2.3a2.35 2.35 0 0 1 0 4.7H10.2" />
          <path d="M10.2 12h2.65a2.35 2.35 0 0 1 0 4.7H10.2" />
        </g>
      </svg>
    </span>
  );
}
