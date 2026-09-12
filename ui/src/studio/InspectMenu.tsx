// The `◉ Inspect` control in the studio toolbar (ADR-0056 §31).
//
// One compact button and a six-item menu. It does not clutter the toolbar and it does not
// open a screen: every item is an inspection request, handed to the drawer, which is where
// findings live. The menu opens upward, which puts it over a native Godot window that cannot
// be painted on — so it joins the viewport-obstruction registry and the studio hides the
// child for exactly as long as the menu is open (SPA-001, INV-090).

import { useCallback, useEffect, useRef, useState } from "react";
import type { InspectRequest } from "../lib/ipc";
import { useObstructsViewport } from "../lib/useViewportObstruction";

export interface InspectMenuProps {
  disabled?: boolean;
  /** The scene open in the viewport, project-relative. Enables *Inspect current level*. */
  currentLevel?: string | null;
  /** Files the studio knows changed. Enables *Inspect changes*. */
  changedFiles?: string[];
  /** What the user has selected. Enables *Inspect current selection*. */
  selection?: { scene?: string; node?: string; file?: string; asset?: string } | null;
  onInspect: (request: InspectRequest) => void;
}

interface MenuItem {
  id: string;
  label: string;
  /** Why it is unavailable, when it is. `null` means available. */
  unavailable: string | null;
  request: InspectRequest;
}

export function InspectMenu({
  disabled = false,
  currentLevel = null,
  changedFiles = [],
  selection = null,
  onInspect,
}: InspectMenuProps) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement | null>(null);

  // The menu opens upward out of the engine toolbar, which sits directly under the viewport
  // card — so it can land over a native Godot window, which cannot be painted over. Joining
  // the registry hides the child for exactly as long as the menu is open (SPA-001, INV-090).
  useObstructsViewport(open);

  // A click anywhere else, or Escape, closes it. The viewport must not stay hidden because
  // a menu was left open behind a modal.
  useEffect(() => {
    if (!open) return;
    const onDown = (event: MouseEvent) => {
      if (!wrap.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const items: MenuItem[] = [
    {
      id: "selection",
      label: "Inspect current selection",
      unavailable: selection ? null : "Nothing is selected",
      request: {
        scope: "selection",
        scene: selection?.scene ?? null,
        node: selection?.node ?? null,
        file: selection?.file ?? null,
        asset: selection?.asset ?? null,
      },
    },
    {
      id: "level",
      label: "Inspect current level",
      unavailable: currentLevel ? null : "No scene is open in the viewport",
      request: { scope: "level", scene: currentLevel },
    },
    {
      id: "project",
      label: "Inspect entire project",
      unavailable: null,
      request: { scope: "project" },
    },
    {
      id: "gameplay",
      label: "Gameplay scan",
      unavailable: null,
      request: { scope: "project", inspectors: ["gameplay"] },
    },
    {
      id: "performance",
      label: "Performance scan",
      unavailable: null,
      request: { scope: "project", inspectors: ["performance"] },
    },
    {
      id: "changes",
      label: "Inspect changes",
      unavailable: changedFiles.length > 0 ? null : "Nothing has changed since the last version",
      request: { scope: "changes", files: changedFiles },
    },
  ];

  const choose = useCallback(
    (item: MenuItem) => {
      if (item.unavailable) return;
      setOpen(false);
      onInspect(item.request);
    },
    [onInspect],
  );

  return (
    <div className="inspect-menu-wrap" ref={wrap}>
      <button
        type="button"
        className={`studio-engine-control${open ? " active" : ""}`}
        disabled={disabled}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        title="Inspect this game without changing it"
      >
        <span aria-hidden="true">◉</span> Inspect
      </button>
      {open ? (
        <div className="inspect-menu" role="menu" aria-label="Inspect">
          {items.map((item) => (
            <button
              key={item.id}
              type="button"
              role="menuitem"
              className="inspect-menu-item"
              disabled={item.unavailable !== null}
              title={item.unavailable ?? item.label}
              onClick={() => choose(item)}
            >
              <span>{item.label}</span>
              {item.unavailable ? (
                <span className="inspect-menu-why">{item.unavailable}</span>
              ) : null}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
