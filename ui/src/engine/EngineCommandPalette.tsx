import { useEffect, useMemo, useRef, useState } from "react";

/**
 * The command palette (ENG-147).
 *
 * `Ctrl+Shift+P` over every engine command the pane exposes. It is deliberately fed the
 * *same* command list the toolbar buttons call, so a command reachable one way is reachable
 * both ways and neither can quietly drift from the other.
 */

export interface PaletteCommand {
  id: string;
  label: string;
  /** Shown on the right — the keyboard shortcut, when it has one. */
  hint?: string;
  group: string;
  disabled?: boolean;
  run: () => void;
}

interface Props {
  open: boolean;
  commands: PaletteCommand[];
  onClose: () => void;
  label?: string;
  placeholder?: string;
}

export function EngineCommandPalette({
  open,
  commands,
  onClose,
  label = "Engine command palette",
  placeholder = "Type a command…",
}: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const returnFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    returnFocusRef.current = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;
    setQuery("");
    setActive(0);
    // The palette is useless if you have to click into it.
    const timer = window.setTimeout(() => inputRef.current?.focus(), 0);
    return () => {
      window.clearTimeout(timer);
      returnFocusRef.current?.focus();
    };
  }, [open]);

  const matches = useMemo(() => {
    const text = query.trim().toLowerCase();
    const usable = commands.filter((command) => !command.disabled);
    if (!text) return usable;
    // Subsequence matching, so "sw" finds "Set Weather" the way every palette does.
    return usable.filter((command) => {
      const haystack = `${command.group} ${command.label} ${command.hint ?? ""}`.toLowerCase();
      let at = 0;
      for (const char of text) {
        at = haystack.indexOf(char, at);
        if (at === -1) return false;
        at += 1;
      }
      return true;
    });
  }, [commands, query]);

  useEffect(() => {
    setActive((current) => Math.min(current, Math.max(matches.length - 1, 0)));
  }, [matches.length]);

  if (!open) return null;

  return (
    <div
      className="engine-palette-backdrop"
      role="presentation"
      onClick={onClose}
    >
      <div
        className="engine-palette"
        role="dialog"
        aria-label={label}
        onClick={(event) => event.stopPropagation()}
      >
        <input
          ref={inputRef}
          value={query}
          placeholder={placeholder}
          aria-label="Command"
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              onClose();
            } else if (event.key === "ArrowDown") {
              event.preventDefault();
              setActive((current) => Math.min(current + 1, matches.length - 1));
            } else if (event.key === "ArrowUp") {
              event.preventDefault();
              setActive((current) => Math.max(current - 1, 0));
            } else if (event.key === "Enter") {
              event.preventDefault();
              const command = matches[active];
              if (command) {
                onClose();
                command.run();
              }
            }
          }}
        />
        <div className="engine-palette-list">
          {matches.map((command, index) => (
            <button
              key={command.id}
              type="button"
              className={`engine-palette-row${index === active ? " active" : ""}`}
              onMouseEnter={() => setActive(index)}
              onClick={() => {
                onClose();
                command.run();
              }}
            >
              <span className="engine-palette-group">{command.group}</span>
              <span className="engine-palette-label">{command.label}</span>
              {command.hint ? <kbd>{command.hint}</kbd> : null}
            </button>
          ))}
          {matches.length === 0 ? (
            <div className="engine-empty-hint">No command matches.</div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
