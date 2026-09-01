import { useEffect, useRef } from "react";
import {
  IconBolt,
  IconCheck,
  IconChevronDown,
  IconHand,
  IconMonitor,
  IconShieldAlert,
} from "./icons";

export type PermissionMode = "ask_approval" | "auto" | "full_access";

type PermissionPickerProps = {
  mode: PermissionMode;
  computerBrowser: boolean;
  open: boolean;
  disabled?: boolean;
  onOpenChange: (open: boolean) => void;
  onSelectMode: (mode: PermissionMode) => void;
  onToggleComputerBrowser: () => void;
};

export const PERMISSION_MODES: {
  id: PermissionMode;
  label: string;
  desc: string;
  tone: "green" | "blue" | "indigo";
}[] = [
  {
    id: "ask_approval",
    label: "Ask approval",
    desc: "Ask permission before executing tools & changes",
    tone: "green",
  },
  {
    id: "auto",
    label: "Auto",
    desc: "Automatically approve tool permissions",
    tone: "blue",
  },
  {
    id: "full_access",
    label: "Full access",
    desc: "Autonomous execution without confirmation",
    tone: "indigo",
  },
];

export function PermissionPicker({
  mode,
  computerBrowser,
  open,
  disabled = false,
  onOpenChange,
  onSelectMode,
  onToggleComputerBrowser,
}: PermissionPickerProps) {
  const wrapRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return undefined;
    const onPointer = (event: MouseEvent) => {
      if (!wrapRef.current?.contains(event.target as Node)) {
        onOpenChange(false);
      }
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onOpenChange(false);
      }
    };
    window.addEventListener("mousedown", onPointer);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onPointer);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, onOpenChange]);

  const activeOption =
    PERMISSION_MODES.find((item) => item.id === mode) ?? PERMISSION_MODES[0];

  return (
    <div className="permission-picker-wrap" ref={wrapRef}>
      <button
        type="button"
        className={`permission-trigger-btn ${activeOption.tone}${open ? " open" : ""}`}
        onClick={() => onOpenChange(!open)}
        disabled={disabled}
        aria-haspopup="dialog"
        aria-expanded={open}
        title={`Permission Mode: ${activeOption.label}`}
      >
        <span className={`permission-trigger-icon ${activeOption.tone}`}>
          {activeOption.id === "ask_approval" ? (
            <IconHand size={14} />
          ) : activeOption.id === "auto" ? (
            <IconBolt size={14} />
          ) : (
            <IconShieldAlert size={14} />
          )}
        </span>
        <span className="permission-trigger-label">{activeOption.label}</span>
        <span className="permission-trigger-chev" aria-hidden="true">
          <IconChevronDown size={11} />
        </span>
      </button>

      {open ? (
        <div
          className="permission-dropup-menu"
          role="dialog"
          aria-label="Permission settings"
        >
          <div className="permission-dropup-head">
            <span>PERMISSION</span>
          </div>

          <div className="permission-options-list" role="radiogroup">
            {PERMISSION_MODES.map((item) => {
              const isSelected = item.id === mode;
              return (
                <button
                  key={item.id}
                  type="button"
                  role="radio"
                  aria-checked={isSelected}
                  className={`permission-menu-item ${item.tone}${isSelected ? " active" : ""}`}
                  onClick={() => {
                    onSelectMode(item.id);
                    onOpenChange(false);
                  }}
                >
                  <span className={`permission-item-icon ${item.tone}`}>
                    {item.id === "ask_approval" ? (
                      <IconHand size={15} />
                    ) : item.id === "auto" ? (
                      <IconBolt size={15} />
                    ) : (
                      <IconShieldAlert size={15} />
                    )}
                  </span>
                  <div className="permission-item-copy">
                    <span className="permission-item-label">{item.label}</span>
                  </div>
                  {isSelected ? (
                    <span className="permission-check-badge">
                      <IconCheck size={12} />
                    </span>
                  ) : null}
                </button>
              );
            })}
          </div>

          <div className="permission-dropup-divider" />

          <div className="permission-dropup-subhead">
            <span>NEXT ONLY</span>
            <div className="permission-subhead-badges" aria-hidden="true">
              <span className="mini-glyph green"><IconHand size={11} /></span>
              <span className="mini-glyph blue"><IconBolt size={11} /></span>
            </div>
          </div>

          <button
            type="button"
            className={`permission-menu-item toggle-item${computerBrowser ? " active" : ""}`}
            onClick={() => onToggleComputerBrowser()}
            aria-pressed={computerBrowser}
          >
            <span className="permission-item-icon blue">
              <IconMonitor size={15} />
            </span>
            <div className="permission-item-copy">
              <span className="permission-item-label">
                Computer + Browser included
              </span>
            </div>
            {computerBrowser ? (
              <span className="permission-check-badge">
                <IconCheck size={12} />
              </span>
            ) : null}
          </button>
        </div>
      ) : null}
    </div>
  );
}
