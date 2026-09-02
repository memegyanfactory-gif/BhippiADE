import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  IconCheck,
  IconCopy,
  IconCrown,
  IconEye,
  IconEyeOff,
  IconSettings,
  IconUser,
} from "../components/icons";
import { getProfile, maskLicenseKey, onProfileChange, type UserProfile } from "../lib/profile";
import type { SettingsTab } from "../screens/SettingsModal";

interface SidebarAccountProps {
  version: string | null;
  demoMode: boolean;
  collapsed: boolean;
  onOpenSettings: (tab?: SettingsTab) => void;
}

/** Width of the floating card. Kept here so the flip maths and the CSS agree. */
const POPOVER_WIDTH = 268;
/** Breathing room between the card and the trigger, and between the card and the viewport. */
const GAP = 8;
const MARGIN = 8;

interface Anchor {
  left: number;
  top: number;
}

/**
 * Places the card above its trigger in viewport coordinates.
 *
 * The card is rendered into `document.body` rather than into the sidebar, because
 * `.sidebar` is `overflow: hidden` with its own stacking context (`z-index: 5`): an
 * absolutely-positioned child was clipped at the 240 px rail and painted *underneath*
 * the workspace pane, which is the bug this replaces. Fixed coordinates measured from
 * the trigger keep it visually attached without inheriting that clip.
 */
function place(trigger: DOMRect, card: DOMRect | null): Anchor {
  const width = card?.width || POPOVER_WIDTH;
  const height = card?.height || 0;
  const left = Math.min(
    Math.max(trigger.left, MARGIN),
    Math.max(MARGIN, window.innerWidth - width - MARGIN),
  );
  // Prefer above the trigger (it sits at the bottom of the rail). Flip below only when
  // the card genuinely does not fit, then clamp so it never runs off the viewport.
  const above = trigger.top - GAP - height;
  const top = above >= MARGIN ? above : Math.min(trigger.bottom + GAP, window.innerHeight - height - MARGIN);
  return { left, top: Math.max(MARGIN, top) };
}

export function SidebarAccount({
  version,
  demoMode,
  collapsed,
  onOpenSettings,
}: SidebarAccountProps) {
  const [profile, setProfile] = useState<UserProfile>(getProfile());
  const [popoverOpen, setPopoverOpen] = useState(false);
  const [keyRevealed, setKeyRevealed] = useState(false);
  const [copied, setCopied] = useState(false);
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  const triggerRef = useRef<HTMLDivElement | null>(null);
  const cardRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => onProfileChange(setProfile), []);

  const reposition = useCallback(() => {
    const trigger = triggerRef.current?.getBoundingClientRect();
    if (!trigger) return;
    setAnchor(place(trigger, cardRef.current?.getBoundingClientRect() ?? null));
  }, []);

  // Measure once the card is in the DOM so the flip decision knows its real height.
  useLayoutEffect(() => {
    if (!popoverOpen) {
      setAnchor(null);
      return;
    }
    reposition();
  }, [popoverOpen, reposition]);

  // Close on outside click or Escape; follow the trigger on scroll and resize.
  useEffect(() => {
    if (!popoverOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (cardRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
      setPopoverOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPopoverOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", reposition);
    window.addEventListener("scroll", reposition, true);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("resize", reposition);
      window.removeEventListener("scroll", reposition, true);
    };
  }, [popoverOpen, reposition]);

  // A collapsed rail has no room for the card; closing it avoids a card with no anchor.
  useEffect(() => {
    if (collapsed) setPopoverOpen(false);
  }, [collapsed]);

  const copyKey = async () => {
    try {
      await navigator.clipboard.writeText(profile.licenseKey);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      /* Clipboard denied — the key is on screen and selectable, so this is not fatal. */
    }
  };

  const initials =
    profile.name
      .split(" ")
      .map((word) => word[0])
      .filter(Boolean)
      .slice(0, 2)
      .join("")
      .toUpperCase() || "D";

  const avatar = (size: "sm" | "lg") =>
    profile.avatarUrl ? (
      <img src={profile.avatarUrl} alt="" className={`acct-avatar acct-avatar-${size}`} />
    ) : (
      <span className={`acct-avatar acct-avatar-${size} acct-avatar-text`}>{initials}</span>
    );

  const card =
    popoverOpen && !collapsed
      ? createPortal(
          <div
            className="acct-card"
            role="dialog"
            aria-label="Account"
            ref={cardRef}
            style={{
              left: anchor?.left ?? 0,
              top: anchor?.top ?? 0,
              width: POPOVER_WIDTH,
              // Until the first measurement lands the card would flash at 0,0.
              visibility: anchor ? "visible" : "hidden",
            }}
          >
            <div className="acct-card-head">
              {avatar("lg")}
              <div className="acct-card-id">
                <span className="acct-card-name">{profile.name}</span>
                <span className="acct-card-email">{profile.email}</span>
              </div>
            </div>

            <div className="acct-plan">
              <IconCrown size={11} />
              <span>{profile.plan}</span>
            </div>

            <div className="acct-key">
              <code className="acct-key-value">
                {keyRevealed ? profile.licenseKey : maskLicenseKey(profile.licenseKey)}
              </code>
              <button
                type="button"
                className="acct-key-btn"
                onClick={() => setKeyRevealed((revealed) => !revealed)}
                title={keyRevealed ? "Hide license key" : "Show license key"}
                aria-label={keyRevealed ? "Hide license key" : "Show license key"}
              >
                {keyRevealed ? <IconEyeOff size={13} /> : <IconEye size={13} />}
              </button>
              <button
                type="button"
                className={`acct-key-btn${copied ? " copied" : ""}`}
                onClick={copyKey}
                title="Copy license key"
                aria-label="Copy license key"
              >
                {copied ? <IconCheck size={13} /> : <IconCopy size={13} />}
              </button>
            </div>

            <div className="acct-menu">
              <button
                type="button"
                className="acct-menu-item"
                onClick={() => {
                  setPopoverOpen(false);
                  onOpenSettings("Profile");
                }}
              >
                <IconUser size={14} />
                <span>Profile &amp; avatar</span>
              </button>
              <button
                type="button"
                className="acct-menu-item"
                onClick={() => {
                  setPopoverOpen(false);
                  onOpenSettings("Appearance");
                }}
              >
                <IconSettings size={14} />
                <span>Settings</span>
              </button>
            </div>

            <div className="acct-card-foot">
              <span>bhippi{version ? ` ${version}` : ""}</span>
              {demoMode ? <span className="badge-demo">demo</span> : null}
            </div>
          </div>,
          document.body,
        )
      : null;

  return (
    <div className={`sidebar-account-wrapper${collapsed ? " collapsed" : ""}`}>
      {card}
      <div
        className={`side-account-card${popoverOpen ? " open" : ""}`}
        ref={triggerRef}
        onClick={() => setPopoverOpen((open) => !open)}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            setPopoverOpen((open) => !open);
          }
        }}
        role="button"
        tabIndex={0}
        aria-haspopup="dialog"
        aria-expanded={popoverOpen}
        title={collapsed ? `${profile.name} · ${profile.plan}` : undefined}
      >
        {avatar("sm")}

        {!collapsed ? (
          <>
            <div className="side-account-info">
              <span className="side-user-name">{profile.name}</span>
              <span className="side-user-plan">{profile.plan}</span>
            </div>

            <button
              type="button"
              className="side-settings-gear-btn"
              onClick={(event) => {
                event.stopPropagation();
                setPopoverOpen(false);
                onOpenSettings("Appearance");
              }}
              title="Settings"
              aria-label="Settings"
            >
              <IconSettings size={15} />
            </button>
          </>
        ) : null}
      </div>
    </div>
  );
}
