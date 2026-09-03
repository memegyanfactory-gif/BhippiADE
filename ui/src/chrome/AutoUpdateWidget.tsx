import { useCallback, useEffect, useRef, useState } from "react";
import type { GitUpdateResult, GitUpdateStatus } from "../lib/ipc";
import { api } from "../lib/api";
import { IconDownload, IconRefresh } from "../components/icons";
import { useObstructsViewport } from "../lib/useViewportObstruction";

export function AutoUpdateWidget() {
  const [updateStatus, setUpdateStatus] = useState<GitUpdateStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [installResult, setInstallResult] = useState<GitUpdateResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const widgetRef = useRef<HTMLDivElement | null>(null);
  // The dropdown lands over the Studio viewport, where a native Godot window would
  // otherwise cover it (SPA-001).
  useObstructsViewport(dropdownOpen);

  const checkForUpdates = useCallback(async (manual = false) => {
    setChecking(true);
    setError(null);
    try {
      const status = await api.checkAppUpdate();
      setUpdateStatus(status);
      if (manual && !status.update_available) {
        // Just let user see the up-to-date state in dropdown
      }
    } catch (err) {
      if (manual) {
        setError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      setChecking(false);
    }
  }, []);

  // Initial check on startup after 2.5s, then background poll every 5 minutes
  useEffect(() => {
    const initialTimer = window.setTimeout(() => {
      void checkForUpdates(false);
    }, 2500);

    const interval = window.setInterval(() => {
      void checkForUpdates(false);
    }, 300000);

    return () => {
      window.clearTimeout(initialTimer);
      window.clearInterval(interval);
    };
  }, [checkForUpdates]);

  // Click outside to dismiss dropdown
  useEffect(() => {
    if (!dropdownOpen) return;
    const onMouseDown = (e: MouseEvent) => {
      if (widgetRef.current && !widgetRef.current.contains(e.target as Node)) {
        setDropdownOpen(false);
      }
    };
    window.addEventListener("mousedown", onMouseDown);
    return () => window.removeEventListener("mousedown", onMouseDown);
  }, [dropdownOpen]);

  const handleInstall = async () => {
    setInstalling(true);
    setError(null);
    try {
      const result = await api.installAppUpdate();
      setInstallResult(result);
      if (result.success) {
        setUpdateStatus((prev) =>
          prev
            ? {
                ...prev,
                update_available: false,
                current_commit: result.current_commit,
                commits_behind: 0,
              }
            : null,
        );
      } else {
        setError(result.message);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setInstalling(false);
    }
  };

  const handleReload = () => {
    window.location.reload();
  };

  const hasUpdate = Boolean(updateStatus?.update_available);
  const currentVer = updateStatus?.current_version || "0.1.20125";
  const remoteVer = updateStatus?.remote_version || currentVer;

  return (
    <div className="titlebar-update-wrapper" ref={widgetRef}>
      <button
        type="button"
        className={`titlebar-update-btn${hasUpdate ? " has-update" : ""}${
          checking ? " is-checking" : ""
        }${dropdownOpen ? " active" : ""}`}
        onClick={() => {
          setDropdownOpen((open) => !open);
          if (!updateStatus && !checking) {
            void checkForUpdates(false);
          }
        }}
        title={
          checking
            ? "Checking for Git updates..."
            : hasUpdate
              ? `New version available (v${remoteVer}) · Click to view`
              : `Bhippi v${currentVer} (Up to date) · Click to check`
        }
        aria-expanded={dropdownOpen}
        aria-haspopup="dialog"
      >
        {checking ? (
          <IconRefresh size={13} className="spin-icon" />
        ) : (
          <IconDownload size={13} />
        )}
        {hasUpdate && <span className="update-pulse-badge" aria-hidden="true" />}
      </button>

      {dropdownOpen && (
        <div className="titlebar-update-dropdown" role="dialog" aria-label="Bhippi Updates">
          <div className="update-dropdown-header">
            <span className="update-dropdown-title">Bhippi Updates</span>
            <button
              type="button"
              className="update-dropdown-close"
              onClick={() => setDropdownOpen(false)}
              aria-label="Close updates menu"
            >
              ✕
            </button>
          </div>

          <div className="update-dropdown-body">
            {/* Version comparison row */}
            <div className="update-version-row">
              <div className="update-version-col">
                <span className="update-version-lbl">Current</span>
                <span className="update-version-tag current">v{currentVer}</span>
              </div>
              <div className="update-version-arrow">→</div>
              <div className="update-version-col">
                <span className="update-version-lbl">Git Latest</span>
                <span className={`update-version-tag${hasUpdate ? " new-version" : " current"}`}>
                  v{remoteVer}
                </span>
              </div>
            </div>

            {installResult?.success ? (
              <div className="update-success-box">
                <span className="update-status-badge latest">Update Installed!</span>
                <p className="update-subtext">
                  Successfully updated to commit <code>{installResult.current_commit}</code>. Reload to activate new changes.
                </p>
                <button
                  type="button"
                  className="btn-update-action primary"
                  onClick={handleReload}
                >
                  ↻ Reload Bhippi
                </button>
              </div>
            ) : hasUpdate ? (
              <div className="update-available-box">
                <div className="update-status-row">
                  <span className="update-status-badge available">New Version</span>
                  <span className="update-commits-behind">
                    {updateStatus?.commits_behind ?? 1} commit(s) ahead
                  </span>
                </div>

                {updateStatus?.commit_message && (
                  <p className="update-commit-msg">"{updateStatus.commit_message}"</p>
                )}

                <div className="update-commit-hash">
                  Commit: <code>{updateStatus?.remote_commit}</code> on <code>{updateStatus?.branch}</code>
                </div>

                {error && <div className="update-error-text">{error}</div>}

                <div className="update-btn-row">
                  <button
                    type="button"
                    className="btn-update-action primary"
                    onClick={handleInstall}
                    disabled={installing}
                  >
                    {installing ? "Installing..." : "↓ Download & Install"}
                  </button>
                </div>
              </div>
            ) : (
              <div className="update-current-box">
                <div className="update-status-row">
                  <span className="update-status-badge latest">Up to Date</span>
                </div>
                <p className="update-subtext">
                  Your local version matches or is newer than the remote git repository.
                </p>

                {error && <div className="update-error-text">{error}</div>}

                <div className="update-btn-row">
                  <button
                    type="button"
                    className="btn-update-action secondary"
                    onClick={() => void checkForUpdates(true)}
                    disabled={checking}
                  >
                    {checking ? "Checking Git..." : "↻ Check for Updates"}
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
