import { IconBrowser, IconEditor } from "../components/icons";

export type WorkbenchMode = "editor" | "browser";

/**
 * The Editor ⇄ Browser switch.
 *
 * One track, two labels, and a pill that slides between them — the movement is what
 * tells you the two panes are the same surface in two states rather than two separate
 * screens. The pill is a single translated element rather than two fading backgrounds,
 * so it carries momentum: it overshoots very slightly on the way in and settles, which
 * is the difference between a control that feels sprung and one that feels like a
 * checkbox.
 *
 * There is no Engine mode here. Since ADR-0045 the real Godot editor is embedded in the
 * Studio viewport, so a second engine surface in the workbench could only ever be a
 * stale picture of the same project.
 *
 * Everything the movement conveys is also carried by `aria-checked`, the label text,
 * and the icon, so a reduced-motion user loses the flourish and none of the meaning.
 */

/** The authoritative ordering, also used by App (Ctrl+' cycling). */
export const WORKBENCH_ORDER: WorkbenchMode[] = ["editor", "browser"];

export function ModeSwitch({
  mode,
  onMode,
}: {
  mode: WorkbenchMode;
  onMode: (mode: WorkbenchMode) => void;
}) {
  const step = (delta: number) => {
    const index = WORKBENCH_ORDER.indexOf(mode);
    const next = Math.max(0, Math.min(WORKBENCH_ORDER.length - 1, index + delta));
    onMode(WORKBENCH_ORDER[next]);
  };

  return (
    <div
      className={`mode-switch ${mode}`}
      role="radiogroup"
      aria-label="Workbench mode"
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft") step(-1);
        if (event.key === "ArrowRight") step(1);
      }}
    >
      <span className="mode-pill" aria-hidden="true" />
      {WORKBENCH_ORDER.map((candidate) => {
        const Icon = candidate === "editor" ? IconEditor : IconBrowser;
        return (
          <button
            key={candidate}
            role="radio"
            aria-checked={mode === candidate}
            className={mode === candidate ? "active" : ""}
            onClick={() => onMode(candidate)}
            tabIndex={mode === candidate ? 0 : -1}
          >
            <Icon size={14} />
            {candidate === "editor" ? "Editor" : "Browser"}
          </button>
        );
      })}
    </div>
  );
}
