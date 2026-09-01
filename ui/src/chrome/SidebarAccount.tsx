import { useEffect, useRef, useState } from "react";
import {
  IconCheck,
  IconCopy,
  IconCrown,
  IconEye,
  IconEyeOff,
  IconKey,
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
  const cardRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const unsub = onProfileChange(setProfile);
    return unsub;
  }, []);

  // Close popover when clicking outside or pressing Escape
  useEffect(() => {
    if (!popoverOpen) return;
    const onPointerDown = (e: PointerEvent) => {
      if (cardRef.current && !cardRef.current.contains(e.target as Node)) {
        setPopoverOpen(false);
      }
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") setPopoverOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [popoverOpen]);

  const copyKey = async () => {
    try {
      await navigator.clipboard.writeText(profile.licenseKey);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // fallback
    }
  };

  const initials = profile.name
    .split(" ")
    .map((w) => w[0])
    .filter(Boolean)
    .slice(0, 2)
    .join("")
    .toUpperCase() || "D";

  return (
    <div className={`sidebar-account-wrapper${collapsed ? " collapsed" : ""}`} ref={cardRef}>
      {/* Quick Profile Dropup Popover */}
      {popoverOpen && !collapsed ? (
        <div className="account-popover" role="dialog" aria-label="Account details">
          <div className="account-popover-hero">
            <div className="popover-avatar-wrap">
              {profile.avatarUrl ? (
                <img src={profile.avatarUrl} alt={profile.name} className="popover-avatar-img" />
              ) : (
                <div className="popover-avatar-fallback">{initials}</div>
              )}
              <span className="popover-crown-badge" title="Lifetime Activation Member">
                <IconCrown size={13} />
              </span>
            </div>
            <div className="popover-hero-meta">
              <div className="popover-name">{profile.name}</div>
              <div className="popover-email">{profile.email}</div>
              <div className="popover-plan-badge">
                <span className="status-dot" />
                <IconCrown size={11} />
                <span>{profile.plan}</span>
              </div>
            </div>
          </div>

          {/* Hidden/Revealable Product Key */}
          <div className="popover-license-card">
            <div className="license-card-header">
              <span className="license-card-label">
                <IconKey size={12} />
                <span>Product License Key</span>
              </span>
              <span className="license-status-tag">Verified</span>
            </div>
            <div className="license-key-box">
              <code className="license-key-text">
                {keyRevealed ? profile.licenseKey : maskLicenseKey(profile.licenseKey)}
              </code>
              <div className="license-key-actions">
                <button
                  type="button"
                  className="license-action-btn"
                  onClick={() => setKeyRevealed((r) => !r)}
                  title={keyRevealed ? "Hide Product Key" : "Show Product Key"}
                  aria-label={keyRevealed ? "Hide Product Key" : "Show Product Key"}
                >
                  {keyRevealed ? <IconEyeOff size={13} /> : <IconEye size={13} />}
                </button>
                <button
                  type="button"
                  className={`license-action-btn${copied ? " copied" : ""}`}
                  onClick={copyKey}
                  title="Copy License Key"
                  aria-label="Copy License Key"
                >
                  {copied ? <IconCheck size={13} /> : <IconCopy size={13} />}
                </button>
              </div>
            </div>
          </div>

          <div className="popover-actions">
            <button
              type="button"
              className="popover-btn primary"
              onClick={() => {
                setPopoverOpen(false);
                onOpenSettings("Profile");
              }}
            >
              <IconUser size={13} />
              <span>Edit Profile & Avatar</span>
            </button>
            <button
              type="button"
              className="popover-btn"
              onClick={() => {
                setPopoverOpen(false);
                onOpenSettings("Providers");
              }}
            >
              <IconSettings size={13} />
              <span>Settings & Preferences</span>
            </button>
          </div>

          <div className="popover-footer">
            <span>
              bhippi{version ? ` · v${version}` : ""}
              {demoMode ? <span className="badge-demo" style={{ marginLeft: 6 }}>demo</span> : null}
            </span>
            <span className="popover-tier">{profile.tier}</span>
          </div>
        </div>
      ) : null}

      {/* Sidebar Account Trigger Bar */}
      <div
        className="side-account-card"
        onClick={() => setPopoverOpen((prev) => !prev)}
        role="button"
        tabIndex={0}
        aria-haspopup="dialog"
        aria-expanded={popoverOpen}
        title={`${profile.name} · ${profile.plan} (Click to view account)`}
      >
        <div className="side-account-avatar-wrap">
          {profile.avatarUrl ? (
            <img src={profile.avatarUrl} alt={profile.name} className="side-avatar-img" />
          ) : (
            <div className="side-avatar-fallback">{initials}</div>
          )}
          <span className="side-crown-badge" title="Lifetime Activation">
            <IconCrown size={11} />
          </span>
        </div>

        {!collapsed ? (
          <>
            <div className="side-account-info">
              <span className="side-user-name">{profile.name}</span>
              <span className="side-lifetime-tag">
                <IconCrown size={10} />
                <span>{profile.plan}</span>
              </span>
            </div>

            <button
              type="button"
              className="side-settings-gear-btn"
              onClick={(e) => {
                e.stopPropagation();
                onOpenSettings("Profile");
              }}
              title="Open Settings & Preferences"
              aria-label="Open Settings"
            >
              <IconSettings size={15} />
            </button>
          </>
        ) : null}
      </div>
    </div>
  );
}
